use crate::model::{BigFile, DirNode, Header, Snapshot, FORMAT_VERSION};
use crate::{platform, WgError};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub exclude: Vec<String>,
    pub big_file_threshold: u64,
    pub prune_threshold: u64,
    pub preserve_depth: usize,
}

impl ScanOptions {
    pub fn new(root: PathBuf) -> Self {
        Self { root, exclude: Vec::new(), big_file_threshold: 50 * 1024 * 1024,
            prune_threshold: 5 * 1024 * 1024, preserve_depth: 3 }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanProgress {
    pub files: u64,
    pub bytes: u64,
    pub directories: u64,
    pub current_dir: String,
}

#[derive(Debug)]
struct Tree {
    name: String,
    path: PathBuf,
    alloc: u64,
    logical: u64,
    files: u64,
    children: Vec<Tree>,
}

struct Candidate {
    dir: PathBuf,
    name: String,
    alloc: u64,
    modified: i64,
}

struct State {
    started: Instant,
    errors: AtomicU32,
    error_messages: Mutex<Vec<String>>,
    files: AtomicU64,
    bytes: AtomicU64,
    directories: AtomicU64,
    last_update_ms: AtomicU64,
    hardlinks: Mutex<HashSet<(u64, u64)>>,
    big_files: Mutex<Vec<Candidate>>,
}

impl State {
    fn new() -> Self {
        Self { started: Instant::now(), errors: AtomicU32::new(0),
            error_messages: Mutex::new(Vec::new()), files: AtomicU64::new(0),
            bytes: AtomicU64::new(0), directories: AtomicU64::new(0),
            last_update_ms: AtomicU64::new(0), hardlinks: Mutex::new(HashSet::new()),
            big_files: Mutex::new(Vec::new()) }
    }

    fn error(&self, path: &Path, error: &std::io::Error) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        let mut messages = self.error_messages.lock().unwrap_or_else(|p| p.into_inner());
        if messages.len() < 50 { messages.push(format!("{}: {error}", path.display())); }
    }

    fn emit(&self, path: &Path, callback: Option<&(dyn Fn(ScanProgress) + Send + Sync)>) {
        let Some(callback) = callback else { return; };
        let ms = self.started.elapsed().as_millis() as u64;
        let prior = self.last_update_ms.load(Ordering::Relaxed);
        if ms.saturating_sub(prior) >= 100 && self.last_update_ms
            .compare_exchange(prior, ms, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            callback(ScanProgress {
                files: self.files.load(Ordering::Relaxed),
                bytes: self.bytes.load(Ordering::Relaxed),
                directories: self.directories.load(Ordering::Relaxed),
                current_dir: path.to_string_lossy().to_string(),
            });
        }
    }
}

pub fn scan(options: &ScanOptions, cancelled: &AtomicBool,
    progress: Option<&(dyn Fn(ScanProgress) + Send + Sync)>) -> Result<Snapshot, WgError> {
    let meta = fs::symlink_metadata(&options.root)?;
    if !meta.is_dir() || platform::is_link_or_placeholder(&meta) {
        return Err(WgError::Format("scan root must be a real directory (not a link)".into()));
    }
    let canonical = options.root.canonicalize()?;
    let mut opts = options.clone();
    opts.root = canonical.clone();
    let mut builder = GlobSetBuilder::new();
    for pattern in &opts.exclude {
        let glob = Glob::new(pattern).map_err(|e| WgError::Format(format!("invalid exclusion pattern {pattern}: {e}")))?;
        builder.add(glob);
    }
    let exclusions = builder.build().map_err(|e| WgError::Format(e.to_string()))?;
    let state = State::new();
    let root_device = platform::device(&meta);
    let tree = walk(&canonical, String::new(), &opts, &exclusions, cancelled, &state, root_device, progress)?;
    if cancelled.load(Ordering::Relaxed) { return Err(WgError::Cancelled); }
    let mut dirs = Vec::new();
    let mut dir_ids = HashMap::new();
    flatten(&tree, None, 0, options, &mut dirs, &mut dir_ids);
    let mut candidates = state.big_files.into_inner().unwrap_or_else(|p| p.into_inner());
    candidates.sort_by(|a, b| b.alloc.cmp(&a.alloc));
    candidates.truncate(1000);
    let big_files = candidates.into_iter().filter_map(|file| {
        let id = dir_ids.get(&file.dir).copied()?;
        Some(BigFile { dir_id: id, name: file.name, alloc_bytes: file.alloc,
            modified_utc: file.modified })
    }).collect();
    let (total, free) = platform::volume_space(&canonical);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_secs() as i64);
    Ok(Snapshot { header: Header {
        format_version: FORMAT_VERSION,
        taken_at_utc: now, scan_root: canonical.to_string_lossy().to_string(),
        os: std::env::consts::OS.into(),
        hostname: std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default(),
        volume_total: total, volume_free: free,
        scan_ms: state.started.elapsed().as_millis() as u64,
        unreadable_dirs: state.errors.load(Ordering::Relaxed),
        truncated_errors: state.error_messages.into_inner().unwrap_or_else(|p| p.into_inner()),
        cancelled: false,
    }, dirs, big_files })
}

fn walk(path: &Path, name: String, options: &ScanOptions, exclusions: &GlobSet, cancelled: &AtomicBool,
    state: &State, root_device: Option<u64>,
    progress: Option<&(dyn Fn(ScanProgress) + Send + Sync)>) -> Result<Tree, WgError> {
    if cancelled.load(Ordering::Relaxed) { return Err(WgError::Cancelled); }
    state.directories.fetch_add(1, Ordering::Relaxed);
    state.emit(path, progress);
    let iter = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            state.error(path, &err);
            return Ok(Tree { name, path: path.to_path_buf(), alloc: 0, logical: 0, files: 0, children: vec![] });
        }
    };
    let mut entries = Vec::new();
    for item in iter {
        match item { Ok(e) => entries.push(e.path()), Err(err) => state.error(path, &err) }
    }
    entries.sort();
    let scanned = entries.par_iter().map(|child| -> Result<Option<(Option<Tree>, u64, u64)>, WgError> {
        if cancelled.load(Ordering::Relaxed) { return Err(WgError::Cancelled); }
        let rel = child.strip_prefix(&options.root).unwrap_or(child);
        let normalized = rel.to_string_lossy().replace('\\', "/");
        if exclusions.is_match(Path::new(&normalized)) { return Ok(None); }
        let meta = match fs::symlink_metadata(child) {
            Ok(meta) => meta,
            Err(err) => { state.error(child, &err); return Ok(None); }
        };
        if platform::is_link_or_placeholder(&meta) { return Ok(None); }
        if root_device.is_some() && platform::device(&meta) != root_device { return Ok(None); }
        if meta.is_dir() {
            let child_name = child.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let sub = walk(child, child_name, options, exclusions, cancelled, state, root_device, progress)?;
            return Ok(Some((Some(sub), 0, 0)));
        }
        if !meta.is_file() { return Ok(None); }
        if let Some(identity) = platform::identity(&meta) {
            let mut seen = state.hardlinks.lock().unwrap_or_else(|p| p.into_inner());
            if !seen.insert(identity) { return Ok(None); }
        }
        let alloc = platform::allocated(child, &meta);
        state.files.fetch_add(1, Ordering::Relaxed);
        state.bytes.fetch_add(alloc, Ordering::Relaxed);
        state.emit(path, progress);
        if alloc >= options.big_file_threshold {
            let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            let mut files = state.big_files.lock().unwrap_or_else(|p| p.into_inner());
            files.push(Candidate { dir: path.to_path_buf(),
                name: child.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                alloc, modified });
        }
        Ok(Some((None, alloc, meta.len())))
    }).collect::<Result<Vec<_>, _>>()?;
    let mut node = Tree { name, path: path.to_path_buf(), alloc: 0, logical: 0, files: 0, children: Vec::new() };
    for result in scanned.into_iter().flatten() {
        if let Some(dir) = result.0 {
            node.alloc = node.alloc.saturating_add(dir.alloc);
            node.logical = node.logical.saturating_add(dir.logical);
            node.files = node.files.saturating_add(dir.files);
            node.children.push(dir);
        } else {
            node.alloc = node.alloc.saturating_add(result.1);
            node.logical = node.logical.saturating_add(result.2);
            node.files += 1;
        }
    }
    Ok(node)
}

fn flatten(tree: &Tree, parent: Option<u32>, depth: usize, options: &ScanOptions,
    nodes: &mut Vec<DirNode>, id_map: &mut HashMap<PathBuf, u32>) {
    let id = nodes.len() as u32;
    nodes.push(DirNode { id, parent, name: tree.name.clone(), alloc_bytes: tree.alloc,
        logical_bytes: tree.logical, file_count: tree.files });
    id_map.insert(tree.path.clone(), id);
    for child in &tree.children {
        if depth < options.preserve_depth || child.alloc >= options.prune_threshold {
            flatten(child, Some(id), depth + 1, options, nodes, id_map);
        }
    }
}
