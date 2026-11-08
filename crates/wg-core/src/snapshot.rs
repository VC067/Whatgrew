use crate::model::{Snapshot, SnapshotMeta, FORMAT_VERSION};
use crate::WgError;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 4] = b"WGRW";

pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| ".".into())
        .join("whatgrew")
}

pub fn snapshot_dir() -> PathBuf {
    data_dir().join("snapshots")
}

pub fn save(snapshot: &Snapshot) -> Result<PathBuf, WgError> {
    save_to(&snapshot_dir(), snapshot)
}

pub fn save_to(directory: &Path, snapshot: &Snapshot) -> Result<PathBuf, WgError> {
    fs::create_dir_all(directory)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.subsec_nanos());
    let file = format!(
        "{}-{}-{}.wg",
        snapshot.header.taken_at_utc,
        std::process::id(),
        nanos
    );
    let dest = directory.join(file);
    let tmp = dest.with_extension("wg.tmp");
    let serialized = postcard::to_allocvec(snapshot).map_err(|e| WgError::Codec(e.to_string()))?;
    let compressed = zstd::stream::encode_all(&serialized[..], 3)?;
    let result = (|| -> Result<(), WgError> {
        let mut output = fs::File::create(&tmp)?;
        output.write_all(MAGIC)?;
        output.write_all(&FORMAT_VERSION.to_le_bytes())?;
        output.write_all(&compressed)?;
        output.sync_all()?;
        fs::rename(&tmp, &dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(dest)
}

pub fn load(file: &Path) -> Result<Snapshot, WgError> {
    let mut data = fs::File::open(file)?;
    let mut prefix = [0; 8];
    data.read_exact(&mut prefix)?;
    if &prefix[..4] != MAGIC {
        return Err(WgError::Format("wrong magic bytes".into()));
    }
    let version = u32::from_le_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]);
    if version > FORMAT_VERSION {
        return Err(WgError::Format(format!(
            "snapshot version {version} is newer than supported version {FORMAT_VERSION}"
        )));
    }
    if version != FORMAT_VERSION {
        return Err(WgError::Format(format!(
            "unsupported snapshot version {version}"
        )));
    }
    let decoded = zstd::stream::decode_all(data)?;
    let snapshot: Snapshot =
        postcard::from_bytes(&decoded).map_err(|e| WgError::Codec(e.to_string()))?;
    if snapshot.header.format_version != version {
        return Err(WgError::Format("inconsistent format version".into()));
    }
    Ok(snapshot)
}

pub fn list() -> Result<Vec<SnapshotMeta>, WgError> {
    list_in(&snapshot_dir())
}

pub fn list_in(directory: &Path) -> Result<Vec<SnapshotMeta>, WgError> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|e| e == "wg") {
            if let Ok(snapshot) = load(&entry.path()) {
                result.push(snapshot.meta(entry.file_name().to_string_lossy().to_string()));
            }
        }
    }
    result.sort_by(|a, b| {
        b.taken_at_utc
            .cmp(&a.taken_at_utc)
            .then_with(|| b.file.cmp(&a.file))
    });
    Ok(result)
}

pub fn load_named(name: &str) -> Result<Snapshot, WgError> {
    if Path::new(name).components().count() != 1 || !name.ends_with(".wg") {
        return Err(WgError::Format("invalid snapshot filename".into()));
    }
    load(&snapshot_dir().join(name))
}

pub fn retain_latest(keep: usize) -> Result<usize, WgError> {
    let snapshots = list()?;
    let mut count = 0;
    for old in snapshots.iter().skip(keep.max(1)) {
        fs::remove_file(snapshot_dir().join(&old.file))?;
        count += 1;
    }
    Ok(count)
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |v| v.as_secs() as i64)
}
