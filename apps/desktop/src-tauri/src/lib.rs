use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use wg_core::{diff, model, platform, rules, scan, snapshot};

struct ScanState(Mutex<Option<Arc<AtomicBool>>>);
struct SnapshotCache(Mutex<Vec<(String, Arc<model::Snapshot>)>>);

fn cached_snapshot(name: &str, cache: &SnapshotCache) -> Result<Arc<model::Snapshot>, String> {
    {
        let known = cache.0.lock().map_err(|e| e.to_string())?;
        if let Some((_, snapshot)) = known.iter().find(|(file, _)| file == name) {
            return Ok(Arc::clone(snapshot));
        }
    }
    let snapshot = Arc::new(snapshot::load_named(name).map_err(|e| e.to_string())?);
    let mut known = cache.0.lock().map_err(|e| e.to_string())?;
    known.push((name.into(), Arc::clone(&snapshot)));
    if known.len() > 4 {
        known.remove(0);
    }
    Ok(snapshot)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    scan_root: String,
    theme: String,
    retention: usize,
    exclusions: Vec<String>,
    schedule: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_root: platform::default_root().to_string_lossy().to_string(),
            theme: "system".into(),
            retention: 30,
            exclusions: vec![],
            schedule: false,
        }
    }
}

fn settings_path() -> PathBuf {
    snapshot::data_dir().join("settings.json")
}

#[tauri::command]
fn get_settings() -> Settings {
    fs::read(settings_path())
        .ok()
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn set_settings(settings: Settings) -> Result<(), String> {
    if !["system", "dark", "light"].contains(&settings.theme.as_str()) {
        return Err("invalid theme".into());
    }
    if settings.retention == 0 || settings.retention > 500 {
        return Err("retention must be between 1 and 500".into());
    }
    let path = settings_path();
    fs::create_dir_all(path.parent().ok_or("missing settings directory")?)
        .map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_snapshots() -> Result<Vec<model::SnapshotMeta>, String> {
    snapshot::list().map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanDone {
    file: Option<String>,
    error: Option<String>,
    unreadable_dirs: u32,
}

#[tauri::command]
fn start_scan(
    app: tauri::AppHandle,
    state: tauri::State<'_, ScanState>,
    root: String,
) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Err("A scan is already running".into());
    }
    let path = PathBuf::from(root);
    if !path.is_dir() {
        return Err("Choose an existing directory to scan".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    *guard = Some(Arc::clone(&flag));
    drop(guard);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = get_settings();
        let mut opts = scan::ScanOptions::new(path);
        opts.exclude = settings.exclusions;
        let app_for_progress = app.clone();
        let cb = move |progress| {
            let _ = app_for_progress.emit("scan-progress", progress);
        };
        let result = scan::scan(&opts, &flag, Some(&cb)).and_then(|snap| {
            let errors = snap.header.unreadable_dirs;
            snapshot::save(&snap).map(|p| (p, errors))
        });
        let scan_state = app.state::<ScanState>();
        if let Ok(mut running) = scan_state.0.lock() {
            *running = None;
        }
        let event = match result {
            Ok((path, unreadable_dirs)) => {
                let _ = snapshot::retain_latest(settings.retention);
                ScanDone {
                    file: path.file_name().map(|s| s.to_string_lossy().to_string()),
                    error: None,
                    unreadable_dirs,
                }
            }
            Err(error) => ScanDone {
                file: None,
                error: Some(error.to_string()),
                unreadable_dirs: 0,
            },
        };
        let _ = app.emit("scan-done", event);
    });
    Ok(())
}

#[tauri::command]
fn cancel_scan(state: tauri::State<'_, ScanState>) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(cancel) = guard.as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn get_diff(
    cache: tauri::State<'_, SnapshotCache>,
    a: String,
    b: String,
    min_mb: Option<u64>,
) -> Result<diff::DiffResult, String> {
    let old = cached_snapshot(&a, &cache)?;
    let new = cached_snapshot(&b, &cache)?;
    let params = diff::DiffParams {
        min_hotspot: min_mb.unwrap_or(10).saturating_mul(1024 * 1024),
        ..Default::default()
    };
    diff::compare(&old, &new, &params, &rules::load_rules()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_children(
    cache: tauri::State<'_, SnapshotCache>,
    new_file: String,
    path: String,
    old_file: Option<String>,
) -> Result<Vec<diff::ExploreChild>, String> {
    let new = cached_snapshot(&new_file, &cache)?;
    let old = old_file
        .as_deref()
        .map(|file| cached_snapshot(file, &cache))
        .transpose()?;
    Ok(diff::children(&new, &path, old.as_deref()))
}

#[tauri::command]
fn get_big_files(
    cache: tauri::State<'_, SnapshotCache>,
    file: String,
) -> Result<Vec<BigFileInfo>, String> {
    let snap = cached_snapshot(&file, &cache)?;
    let paths = snap.node_paths();
    Ok(snap
        .big_files
        .iter()
        .take(20)
        .filter_map(|item| {
            let rel = paths.get(item.dir_id as usize)?;
            let full = std::path::Path::new(&snap.header.scan_root)
                .join(rel)
                .join(&item.name);
            Some(BigFileInfo {
                path: full.to_string_lossy().to_string(),
                name: item.name.clone(),
                bytes: item.alloc_bytes,
            })
        })
        .collect())
}

#[derive(Serialize)]
struct BigFileInfo {
    path: String,
    name: String,
    bytes: u64,
}

#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    let file = PathBuf::from(path);
    if !file.exists() {
        return Err("That file or folder is no longer present".into());
    }
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(&file)
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", file.display()))
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = std::process::Command::new("xdg-open")
        .arg(if file.is_dir() {
            file
        } else {
            file.parent()
                .unwrap_or(std::path::Path::new("/"))
                .to_path_buf()
        })
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("file manager exited with {s}")),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn set_schedule(enabled: bool, root: String) -> Result<(), String> {
    configure_schedule(enabled, &root).map_err(|e| e.to_string())?;
    let mut settings = get_settings();
    settings.schedule = enabled;
    set_settings(settings)
}

#[cfg(target_os = "windows")]
fn configure_schedule(enabled: bool, root: &str) -> std::io::Result<()> {
    let result = if enabled {
        let exe = std::env::current_exe()?;
        let command = format!("\"{}\" --background-scan \"{}\"", exe.display(), root);
        std::process::Command::new("schtasks")
            .args([
                "/Create",
                "/F",
                "/SC",
                "WEEKLY",
                "/D",
                "SUN",
                "/ST",
                "10:00",
                "/TN",
                "whatgrew-weekly",
                "/TR",
                &command,
            ])
            .status()?
    } else {
        std::process::Command::new("schtasks")
            .args(["/Delete", "/F", "/TN", "whatgrew-weekly"])
            .status()?
    };
    if result.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "Task Scheduler exited with {result}"
        )))
    }
}

#[cfg(target_os = "macos")]
fn configure_schedule(enabled: bool, root: &str) -> std::io::Result<()> {
    let dir = dirs::home_dir()
        .ok_or_else(|| std::io::Error::other("missing home directory"))?
        .join("Library/LaunchAgents");
    let path = dir.join("dev.whatgrew.desktop.weekly.plist");
    if enabled {
        fs::create_dir_all(&dir)?;
        let exe = std::env::current_exe()?;
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let xml = format!("<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>dev.whatgrew.desktop.weekly</string><key>ProgramArguments</key><array><string>{}</string><string>--background-scan</string><string>{}</string></array><key>StartCalendarInterval</key><dict><key>Weekday</key><integer>1</integer><key>Hour</key><integer>10</integer></dict></dict></plist>", esc(&exe.to_string_lossy()), esc(root));
        fs::write(&path, xml)?;
        let status = std::process::Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(path)
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other("launchctl failed"));
        }
    } else if path.exists() {
        let _ = std::process::Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&path)
            .status();
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn configure_schedule(_enabled: bool, _root: &str) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "scheduling is only supported on macOS and Windows",
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ScanState(Mutex::new(None)))
        .manage(SnapshotCache(Mutex::new(Vec::new())))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            list_snapshots,
            start_scan,
            cancel_scan,
            get_diff,
            get_children,
            get_big_files,
            reveal,
            set_schedule
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| eprintln!("cannot launch whatgrew: {error}"));
}
