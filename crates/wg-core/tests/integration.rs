use std::fs;
use std::sync::atomic::AtomicBool;
use wg_core::{diff, rules, scan, snapshot};

#[test]
fn scans_allocated_bytes_and_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("nested"))?;
    fs::write(root.path().join("nested/alpha.txt"), vec![42; 4100])?;
    fs::write(root.path().join("beta.txt"), vec![7; 3100])?;
    let opts = scan::ScanOptions::new(root.path().to_path_buf());
    let value = scan::scan(&opts, &AtomicBool::new(false), None)?;
    let expected = wg_core::platform::allocated(&root.path().join("nested/alpha.txt"), &fs::metadata(root.path().join("nested/alpha.txt"))?)
        + wg_core::platform::allocated(&root.path().join("beta.txt"), &fs::metadata(root.path().join("beta.txt"))?);
    assert_eq!(value.root().ok_or("missing root")?.alloc_bytes, expected);
    assert_eq!(value.root().ok_or("missing root")?.file_count, 2);
    let store = tempfile::tempdir()?;
    let saved = snapshot::save_to(store.path(), &value)?;
    let loaded = snapshot::load(&saved)?;
    assert_eq!(loaded.root().ok_or("missing root")?.alloc_bytes, expected);
    assert_eq!(snapshot::list_in(store.path())?.len(), 1);
    Ok(())
}

#[test]
fn attributes_growth_to_child_and_respects_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("node_modules"))?;
    let mut opts = scan::ScanOptions::new(root.path().to_path_buf());
    opts.prune_threshold = 0;
    let before = scan::scan(&opts, &AtomicBool::new(false), None)?;
    fs::write(root.path().join("node_modules/big.bin"), vec![1; 8193])?;
    let after = scan::scan(&opts, &AtomicBool::new(false), None)?;
    let delta = diff::compare(&before, &after, &diff::DiffParams { min_hotspot: 1, ..Default::default() }, &rules::load_rules())?;
    assert!(delta.growth_bytes > 0);
    assert!(delta.hotspots.iter().any(|h| h.name == "node_modules"));
    let cancelled = AtomicBool::new(true);
    assert!(scan::scan(&opts, &cancelled, None).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn ignores_symlinks_and_dedupes_hardlinks() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("file"), vec![3; 8000])?;
    fs::hard_link(root.path().join("file"), root.path().join("hard"))?;
    symlink(root.path(), root.path().join("loop"))?;
    let result = scan::scan(&scan::ScanOptions::new(root.path().to_path_buf()), &AtomicBool::new(false), None)?;
    assert_eq!(result.root().ok_or("missing root")?.file_count, 1);
    Ok(())
}

#[test]
fn rejects_invalid_exclusions_and_newer_snapshot_versions() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let mut opts = scan::ScanOptions::new(root.path().to_path_buf());
    opts.exclude = vec!["[invalid".into()];
    assert!(scan::scan(&opts, &AtomicBool::new(false), None).is_err());
    opts.exclude.clear();
    let snapshot = scan::scan(&opts, &AtomicBool::new(false), None)?;
    let store = tempfile::tempdir()?;
    let saved = snapshot::save_to(store.path(), &snapshot)?;
    let mut encoded = fs::read(&saved)?;
    encoded[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&saved, encoded)?;
    let error = snapshot::load(&saved).err().ok_or("expected invalid version")?;
    assert!(error.to_string().contains("newer"));
    Ok(())
}
