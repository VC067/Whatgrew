#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| arg == "--background-scan") {
        let root = std::env::args().skip_while(|a| a != "--background-scan").nth(1);
        let path = root.map(std::path::PathBuf::from).unwrap_or_else(wg_core::platform::default_root);
        let options = wg_core::scan::ScanOptions::new(path);
        if let Ok(snapshot) = wg_core::scan::scan(&options, &std::sync::atomic::AtomicBool::new(false), None) {
            let _ = wg_core::snapshot::save(&snapshot);
        }
        return;
    }
    whatgrew_desktop_lib::run();
}
