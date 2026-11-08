use crate::model::Snapshot;
use crate::rules::{label_for, Rule};
use crate::WgError;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Serialize)]
pub struct Hotspot {
    pub path: String,
    pub name: String,
    pub delta_bytes: i64,
    pub old_bytes: u64,
    pub new_bytes: u64,
    pub label: Option<String>,
    pub hint: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffResult {
    pub from_utc: i64,
    pub to_utc: i64,
    pub growth_bytes: i64,
    pub free_delta_bytes: i64,
    pub hotspots: Vec<Hotspot>,
}

#[derive(Clone, Debug)]
pub struct DiffParams {
    pub min_hotspot: u64,
    pub dominance: f64,
    pub top_n: usize,
}
impl Default for DiffParams {
    fn default() -> Self { Self { min_hotspot: 100 * 1024 * 1024, dominance: 0.6, top_n: 20 } }
}

fn delta(a: u64, b: u64) -> i64 {
    (i128::from(b) - i128::from(a)).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

pub fn compare(old: &Snapshot, new: &Snapshot, params: &DiffParams, rules: &[Rule]) -> Result<DiffResult, WgError> {
    if old.header.scan_root != new.header.scan_root { return Err(WgError::DifferentRoots); }
    let old_paths = old.node_paths();
    let new_paths = new.node_paths();
    let norm = |p: &str| if old.header.os == "windows" { p.to_lowercase() } else { p.to_owned() };
    let mut sizes: BTreeMap<String, (String, u64, u64)> = BTreeMap::new();
    for dir in &old.dirs {
        if let Some(path) = old_paths.get(dir.id as usize) {
            let item = sizes.entry(norm(path)).or_insert((path.clone(), 0, 0));
            item.1 = dir.alloc_bytes;
        }
    }
    for dir in &new.dirs {
        if let Some(path) = new_paths.get(dir.id as usize) {
            let item = sizes.entry(norm(path)).or_insert((path.clone(), 0, 0));
            item.0.clone_from(path);
            item.2 = dir.alloc_bytes;
        }
    }
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for path in sizes.keys() {
        if path.is_empty() { continue; }
        let parent = path.rsplit_once('/').map_or("", |(p, _)| p);
        children.entry(parent.into()).or_default().push(path.clone());
    }
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    collect("", &sizes, &children, params, &mut found, &mut visited);
    found.sort_by_key(|(path, d)| (std::cmp::Reverse(d.unsigned_abs()), path.clone()));
    let hotspots = found.into_iter().take(params.top_n).filter_map(|(key, d)| {
        let (name, old_bytes, new_bytes) = sizes.get(&key)?;
        let full = if name.is_empty() { new.header.scan_root.clone() }
                   else { format!("{}/{}", new.header.scan_root.trim_end_matches(&['/', '\\'][..]), name.replace('/', &std::path::MAIN_SEPARATOR.to_string())) };
        let label = label_for(&full, rules);
        Some(Hotspot { path: full, name: name.rsplit('/').next().unwrap_or(name).to_string(),
            delta_bytes: d, old_bytes: *old_bytes, new_bytes: *new_bytes,
            label: label.as_ref().map(|x| x.0.clone()), hint: label.map(|x| x.1) })
    }).collect();
    let old_root = old.root().map_or(0, |x| x.alloc_bytes);
    let new_root = new.root().map_or(0, |x| x.alloc_bytes);
    Ok(DiffResult { from_utc: old.header.taken_at_utc, to_utc: new.header.taken_at_utc,
        growth_bytes: delta(old_root, new_root),
        free_delta_bytes: delta(old.header.volume_free, new.header.volume_free), hotspots })
}

fn collect(key: &str, sizes: &BTreeMap<String, (String, u64, u64)>,
    children: &HashMap<String, Vec<String>>, params: &DiffParams,
    found: &mut Vec<(String, i64)>, visited: &mut HashSet<String>) {
    if !visited.insert(key.to_string()) { return; }
    let Some((_, a, b)) = sizes.get(key) else { return; };
    let change = delta(*a, *b);
    if change.unsigned_abs() < params.min_hotspot { return; }
    let significant: Vec<String> = children.get(key).into_iter().flatten().filter(|child| {
        sizes.get(*child).is_some_and(|(_, a, b)| delta(*a, *b).unsigned_abs() >= params.min_hotspot)
    }).cloned().collect();
    let dominant = significant.iter().any(|child| {
        sizes.get(child).is_some_and(|(_, a, b)| {
            let child_delta = delta(*a, *b);
            child_delta.signum() == change.signum()
                && (child_delta.unsigned_abs() as f64) >= (change.unsigned_abs() as f64 * params.dominance)
        })
    });
    if !significant.is_empty() && dominant {
        for child in significant { collect(&child, sizes, children, params, found, visited); }
    } else {
        found.push((key.to_string(), change));
    }
}

#[derive(Debug, Serialize)]
pub struct ExploreChild {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub delta: Option<i64>,
}

pub fn children(new: &Snapshot, relative_path: &str, old: Option<&Snapshot>) -> Vec<ExploreChild> {
    let paths = new.node_paths();
    let matching = paths.iter().position(|p| p == relative_path);
    let Some(node_id) = matching else { return Vec::new(); };
    let old_map: HashMap<String, u64> = old.map(|s| s.node_paths().into_iter().zip(s.dirs.iter().map(|d| d.alloc_bytes)).collect())
        .unwrap_or_default();
    let mut result: Vec<ExploreChild> = new.dirs.iter().filter(|dir| dir.parent == Some(node_id as u32)).filter_map(|dir| {
        let path = paths.get(dir.id as usize)?.clone();
        Some(ExploreChild { path: path.clone(), name: dir.name.clone(), bytes: dir.alloc_bytes,
            files: dir.file_count, delta: old.map(|_| delta(old_map.get(&path).copied().unwrap_or(0), dir.alloc_bytes)) })
    }).collect();
    let own = new.dirs.get(node_id).map_or(0, |dir| dir.alloc_bytes);
    let used: u64 = result.iter().map(|d| d.bytes).sum();
    if own > used {
        result.push(ExploreChild { path: "".into(), name: "Smaller items & files".into(),
            bytes: own - used, files: 0, delta: None });
    }
    result.sort_by_key(|d| std::cmp::Reverse(d.bytes));
    result
}
