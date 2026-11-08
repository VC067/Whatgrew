use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub header: Header,
    pub dirs: Vec<DirNode>,
    pub big_files: Vec<BigFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub format_version: u32,
    pub taken_at_utc: i64,
    pub scan_root: String,
    pub os: String,
    pub hostname: String,
    pub volume_total: u64,
    pub volume_free: u64,
    pub scan_ms: u64,
    pub unreadable_dirs: u32,
    pub truncated_errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirNode {
    pub id: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub alloc_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BigFile {
    pub dir_id: u32,
    pub name: String,
    pub alloc_bytes: u64,
    pub modified_utc: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub file: String,
    pub taken_at_utc: i64,
    pub scan_root: String,
    pub alloc_bytes: u64,
    pub volume_free: u64,
    pub unreadable_dirs: u32,
}

impl Snapshot {
    pub fn root(&self) -> Option<&DirNode> {
        self.dirs.iter().find(|dir| dir.parent.is_none())
    }

    pub fn meta(&self, file: String) -> SnapshotMeta {
        SnapshotMeta {
            file,
            taken_at_utc: self.header.taken_at_utc,
            scan_root: self.header.scan_root.clone(),
            alloc_bytes: self.root().map_or(0, |d| d.alloc_bytes),
            volume_free: self.header.volume_free,
            unreadable_dirs: self.header.unreadable_dirs,
        }
    }

    pub fn node_paths(&self) -> Vec<String> {
        let mut paths = vec![String::new(); self.dirs.len()];
        for dir in &self.dirs {
            if dir.id as usize >= paths.len() { continue; }
            let name = if let Some(parent) = dir.parent {
                let prefix = paths.get(parent as usize).map(String::as_str).unwrap_or("");
                if prefix.is_empty() { dir.name.clone() } else { format!("{prefix}/{}", dir.name) }
            } else {
                String::new()
            };
            if let Some(slot) = paths.get_mut(dir.id as usize) { *slot = name; }
        }
        paths
    }
}
