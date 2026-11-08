use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use wg_core::{diff, platform, rules, scan, snapshot};

#[derive(Parser)]
#[command(
    name = "wg",
    version,
    about = "See where your disk space went, not just where it is."
)]
struct Args {
    #[arg(long, global = true, help = "Machine-readable JSON output")]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a directory and save a read-only snapshot.
    Scan {
        path: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
    /// Explain growth between two snapshots (defaults to newest two).
    Diff {
        a: Option<String>,
        b: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value_t = 100)]
        min_mb: u64,
        #[arg(short = 'n', default_value_t = 20)]
        top: usize,
    },
    /// Show biggest folders in the latest saved snapshot.
    Top {
        #[arg(short = 'n', default_value_t = 30)]
        n: usize,
    },
    /// List saved snapshots and free-space history.
    History,
    /// Register or disable an offline weekly scan using the OS scheduler.
    Schedule { mode: ScheduleMode },
}

#[derive(Copy, Clone, ValueEnum)]
enum ScheduleMode {
    Weekly,
    Off,
}

fn main() {
    let args = Args::parse();
    match run(args) {
        Ok(2) => std::process::exit(2),
        Ok(_) => (),
        Err(err) => {
            eprintln!("whatgrew: {err:#}");
            std::process::exit(1);
        }
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn run(args: Args) -> Result<i32> {
    match args.command {
        Command::Scan { path, quiet } => {
            let opts = scan::ScanOptions::new(path.unwrap_or_else(platform::default_root));
            let snapshot = scan::scan(&opts, &AtomicBool::new(false), None)?;
            let file = snapshot::save(&snapshot)?;
            if args.json {
                print_json(&snapshot.meta(file.to_string_lossy().to_string()))?;
            } else if !quiet {
                let root = snapshot.root();
                println!("Snapshot: {}", file.display());
                println!("Scanned: {}", snapshot.header.scan_root);
                println!(
                    "Allocated: {}  |  Files: {}  |  Time: {} ms",
                    format_size(root.map_or(0, |r| r.alloc_bytes)),
                    root.map_or(0, |r| r.file_count),
                    snapshot.header.scan_ms
                );
                if snapshot.header.unreadable_dirs > 0 {
                    eprintln!(
                        "Warning: {} unreadable paths; this scan is partial.",
                        snapshot.header.unreadable_dirs
                    );
                    #[cfg(target_os = "macos")]
                    eprintln!("For protected folders, grant Full Disk Access to the terminal or app running whatgrew.");
                }
            }
            Ok(if snapshot.header.unreadable_dirs > 0 {
                2
            } else {
                0
            })
        }
        Command::Top { n } => {
            let newest = snapshot::list()?
                .into_iter()
                .next()
                .context("no snapshots; run `wg scan` first")?;
            let s = snapshot::load_named(&newest.file)?;
            let paths = s.node_paths();
            let mut items: Vec<_> = s.dirs.iter().filter(|d| d.parent.is_some()).collect();
            items.sort_by_key(|d| std::cmp::Reverse(d.alloc_bytes));
            let rows: Vec<_> = items
                .into_iter()
                .take(n)
                .filter_map(|d| {
                    Some(TopRow {
                        path: Path::new(&s.header.scan_root)
                            .join(paths.get(d.id as usize)?)
                            .to_string_lossy()
                            .to_string(),
                        alloc_bytes: d.alloc_bytes,
                        file_count: d.file_count,
                    })
                })
                .collect();
            if args.json {
                print_json(&rows)?;
            } else {
                for row in rows {
                    println!("{:>10}  {}", format_size(row.alloc_bytes), row.path);
                }
            }
            Ok(0)
        }
        Command::History => {
            let items = snapshot::list()?;
            if args.json {
                print_json(&items)?;
            } else if items.is_empty() {
                println!("No snapshots yet. Run `wg scan` to start tracking.");
            } else {
                for s in items {
                    let date = chrono::DateTime::from_timestamp(s.taken_at_utc, 0).map_or_else(
                        || "unknown".to_string(),
                        |d| d.format("%Y-%m-%d %H:%M UTC").to_string(),
                    );
                    println!(
                        "{date}  scanned {:>9}  free {:>9}  {}{}",
                        format_size(s.alloc_bytes),
                        format_size(s.volume_free),
                        s.scan_root,
                        if s.unreadable_dirs != 0 {
                            " [partial]"
                        } else {
                            ""
                        }
                    );
                }
            }
            Ok(0)
        }
        Command::Diff {
            a,
            b,
            since,
            min_mb,
            top,
        } => {
            let all = snapshot::list()?;
            let newest = all
                .first()
                .context("need two snapshots; run `wg scan` twice")?;
            let b = b.unwrap_or_else(|| newest.file.clone());
            let new = snapshot::load_named(&b)?;
            let a = if let Some(a) = a {
                a
            } else if let Some(since) = since {
                let secs = parse_duration(&since)?;
                let target = new.header.taken_at_utc.saturating_sub(secs);
                all.iter()
                    .filter(|s| s.scan_root == new.header.scan_root && s.file != b)
                    .min_by_key(|s| (i128::from(s.taken_at_utc) - i128::from(target)).abs())
                    .context("no older snapshot for this root")?
                    .file
                    .clone()
            } else {
                all.iter()
                    .find(|s| s.scan_root == new.header.scan_root && s.file != b)
                    .context("need another snapshot of the same directory")?
                    .file
                    .clone()
            };
            let old = snapshot::load_named(&a)?;
            let params = diff::DiffParams {
                min_hotspot: min_mb.saturating_mul(1024 * 1024),
                top_n: top,
                ..Default::default()
            };
            let result = diff::compare(&old, &new, &params, &rules::load_rules())?;
            if args.json {
                print_json(&result)?;
            } else {
                println!(
                    "Growth: {}  |  Free space change: {}",
                    signed_size(result.growth_bytes),
                    signed_size(result.free_delta_bytes)
                );
                for item in result.hotspots {
                    let label = item.label.unwrap_or_else(|| item.name.clone());
                    println!(
                        "{:>10}  {:<24}  {}",
                        signed_size(item.delta_bytes),
                        label,
                        item.path
                    );
                    if let Some(hint) = item.hint {
                        println!("            {hint}");
                    }
                }
            }
            Ok(0)
        }
        Command::Schedule { mode } => {
            let enabled = matches!(mode, ScheduleMode::Weekly);
            let root = snapshot::list()?
                .first()
                .map(|s| PathBuf::from(&s.scan_root))
                .unwrap_or_else(platform::default_root);
            schedule(enabled, &root)?;
            if args.json {
                print_json(&serde_json::json!({"weekly":enabled,"root":root}))?;
            } else {
                println!(
                    "Weekly scanning {}",
                    if enabled { "enabled" } else { "disabled" }
                );
            }
            Ok(0)
        }
    }
}

#[derive(Serialize)]
struct TopRow {
    path: String,
    alloc_bytes: u64,
    file_count: u64,
}

fn parse_duration(input: &str) -> Result<i64> {
    let (value, multiplier) = if let Some(s) = input.strip_suffix('d') {
        (s, 86400)
    } else if let Some(s) = input.strip_suffix('h') {
        (s, 3600)
    } else if let Some(s) = input.strip_suffix('w') {
        (s, 604800)
    } else {
        bail!("use a duration like 7d, 30d or 2w");
    };
    let n: i64 = value
        .parse()
        .context("duration must be a positive number")?;
    if n <= 0 {
        bail!("duration must be positive");
    }
    n.checked_mul(multiplier).context("duration is too large")
}

fn format_size(bytes: u64) -> String {
    let mut size = bytes as f64;
    let mut unit = "B";
    for label in ["KiB", "MiB", "GiB", "TiB", "PiB"] {
        if size < 1024.0 {
            break;
        }
        size /= 1024.0;
        unit = label;
    }
    format!("{size:.1} {unit}")
}
fn signed_size(bytes: i64) -> String {
    format!(
        "{}{}",
        if bytes >= 0 { "+" } else { "-" },
        format_size(bytes.unsigned_abs())
    )
}

#[cfg(target_os = "windows")]
fn schedule(enabled: bool, root: &Path) -> Result<()> {
    use std::process::Command;
    let status = if enabled {
        let exe = std::env::current_exe()?;
        let task = format!("\"{}\" scan \"{}\" --quiet", exe.display(), root.display());
        Command::new("schtasks")
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
                &task,
            ])
            .status()?
    } else {
        Command::new("schtasks")
            .args(["/Delete", "/F", "/TN", "whatgrew-weekly"])
            .status()?
    };
    if !status.success() {
        bail!("Task Scheduler failed with {status}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn schedule(enabled: bool, root: &Path) -> Result<()> {
    use std::process::Command;
    let directory = dirs::home_dir()
        .context("cannot find home")?
        .join("Library/LaunchAgents");
    let file = directory.join("dev.whatgrew.weekly.plist");
    if enabled {
        std::fs::create_dir_all(&directory)?;
        let exe = std::env::current_exe()?;
        let escape = |v: &Path| {
            v.to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>dev.whatgrew.weekly</string><key>ProgramArguments</key><array><string>{}</string><string>scan</string><string>{}</string><string>--quiet</string></array><key>StartCalendarInterval</key><dict><key>Weekday</key><integer>1</integer><key>Hour</key><integer>10</integer></dict></dict></plist>", escape(&exe), escape(root));
        std::fs::write(&file, xml)?;
        let result = Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(&file)
            .status()?;
        if !result.success() {
            bail!("launchctl load failed: {result}");
        }
    } else if file.exists() {
        let _ = Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&file)
            .status();
        std::fs::remove_file(file)?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn schedule(_enabled: bool, _root: &Path) -> Result<()> {
    bail!("weekly scheduling is currently supported on macOS and Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_formats() {
        assert_eq!(parse_duration("7d").ok(), Some(604800));
        assert!(parse_duration("0d").is_err());
        assert!(parse_duration("3x").is_err());
    }
}
