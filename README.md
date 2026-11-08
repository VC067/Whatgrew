<div align="center">

# whatgrew.

**Your disk didn't just fill up. Something grew. Find out what.**

A private, read-only disk-growth tracker for **macOS + Windows**. It tells you which folders grew *between* scans, rather than merely showing what is large today.

**Rust engine · CLI · Tauri 2 desktop GUI · No account · Offline**

</div>

> **Status — 0.1.0 source preview:** This is a new implementation provided from the project specification. It has not been built or dogfooded on macOS/Windows yet. Run the included CI, then test on real machines before distributing it. No installers are published yet.

## Why this exists

Disk analyzers like WizTree tell you where your space **is**. whatgrew records small snapshots and compares them, so you can see **where your space went**: Docker images, local AI models, project dependencies, build caches, and anything else that ballooned unnoticed.

## Features

- **Snapshot scanner:** recursive file metadata scan; disk *allocated bytes* and logical bytes; does not read file contents.
- **Safety:** skips Unix symlinks, Windows reparse points/cloud placeholders; avoids crossing Unix filesystem boundaries. Does not delete, modify, hydrate, move or upload any scanned file.
- **Local history:** versioned postcard+zstd `.wg` snapshots with atomic writes and rolling retention.
- **Growth attribution:** compare snapshots, identify hotspots, distinguish expansion from shrinkage and display available-volume-space changes.
- **Helpful labeling:** local rules identify common caches and app data and offer informational hints. Never presents a delete action.
- **Desktop:** overview, free-space trend, top growers, growth history, drill-down treemap, large-file list, scan overlay with cancellation, accessibility-minded light/dark design.
- **CLI:** `wg scan`, `wg top`, `wg diff`, `wg history`, `wg schedule`, optional `--json`.
- **Automation:** native Windows Task Scheduler or macOS LaunchAgent weekly scan; no background daemon when scheduling is off.
- **Open by design:** the UI and core contain no telemetry or network-request code. All file processing happens locally.

## Desktop — build from source

**Prerequisites:** Rust stable, Node.js 20+, Tauri 2 OS build prerequisites. On Windows, install the MSVC C++ toolchain and WebView2 runtime; on macOS, install Xcode Command Line Tools. See the [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
cd apps/desktop
npm install
npm run tauri dev
```

To build an installer:

```bash
cd apps/desktop
npm run tauri build
```

> Tauri uses the platform WebView. There is **no account or cloud API**. Dependency installation and build-time package downloads are separate from application runtime behavior.

## CLI — build from source

```bash
cargo build --release -p wg-cli
# binary: target/release/wg (wg.exe on Windows)
```

```bash
wg scan                         # home folder on macOS; system drive on Windows
wg scan "/your/projects"         # custom directory
wg history                      # list local snapshots and free space
wg top -n 30                    # biggest tracked folders, newest snapshot
wg diff                         # newest vs previous scan of the same root
wg diff --since 30d             # compare against snapshot closest to 30 days ago
wg diff --min-mb 10 -n 40       # lower hotspot threshold
wg --json diff                  # machine-readable output
wg schedule weekly             # Sunday 10am, OS scheduler
wg schedule off                # disable schedule
```

`wg scan` exit status **0** = complete, **2** = complete with inaccessible paths, **1** = error or cancellation. On first run you get a baseline and a useful `top` view. Run a second scan for growth attribution.

The `wg diff` arguments `A B` are snapshot *filenames* shown under the app data directory, not arbitrary paths. The CLI's `--since` chooses the available snapshot **closest** to the requested age; it cannot compare dates that were never scanned.

## Data location

Snapshots and settings are stored in the OS's per-user local application data directory under `whatgrew/`. On macOS this is normally `~/Library/Application Support/whatgrew`; on Windows it is normally `%LOCALAPPDATA%\whatgrew`. The GUI saves settings as JSON and snapshots as `.wg` files. Each snapshot contains folder names, approximate filesystem allocation, file counts and the names of large files; treat it as sensitive local metadata.

### Custom labels

Create `rules.toml` in the OS's whatgrew config directory. Rules have the same format as [`default.toml`](crates/wg-core/rules/default.toml):

```toml
[[rule]]
match = "**/my-build-cache/**"
label = "My build cache"
hint = "Look here if this cache grows unexpectedly."
```

## Engineering and privacy constraints

Please read [AGENTS.md](AGENTS.md) before modifying the scanner. It intentionally favors safety over speed: it never removes files or follows symlinks and its default scan does not require Administrator rights. Permission denials and skipped areas can make totals **incomplete**; the UI reports a warning when a metadata operation fails.

Allocated disk space is **not always identical** to physical disk blocks uniquely held by your files. APFS clones can share blocks, Windows hard links are not deduplicated in this first release, and sparse/compressed files complicate space accounting. On macOS protected folders may require Full Disk Access. This app never claims that summing allocated file sizes must equal volume usage. Free-space totals are obtained independently from the filesystem.

**Known limitations:** the recursive scanner currently uses per-entry metadata calls instead of an MFT fast path on NTFS; some Windows directory/placeholder metadata can still be inaccessible. Windows hardlink deduplication, APFS clone-aware accounting, richer exclusion presets, robust first-run permissions guidance, installer signing/notarization, and large-volume performance profiling remain to be validated or improved. When folders are moved, diffs may appear as growth in one location and shrinkage in another.

## Verify the implementation

```bash
cargo fmt --all --check
cargo clippy -p wg-core -p wg-cli --all-targets -- -D warnings
cargo test -p wg-core -p wg-cli
cd apps/desktop && npm install && npm run build
```

GitHub Actions [`ci.yml`](.github/workflows/ci.yml) runs Rust checks on Windows, macOS and Linux plus frontend type/build checks. Add real-machine acceptance tests before a public release; no sample metrics or benchmark claims are fabricated.

## Project structure

```text
crates/wg-core/             filesystem scanner, snapshots, diff, rules
crates/wg-cli/              wg command-line frontend
apps/desktop/src-tauri/     native Rust commands, scheduler and Tauri shell
apps/desktop/src/           React + TypeScript UI and treemap
.github/workflows/          CI and optional release builds
bench/                      local comparison benchmark helper
docs/                       architecture and testing notes
```

## Inspiration and acknowledgments

The desktop UI takes visual inspiration from [HatchZip](https://hatchzip.dev/): a purposeful, compact utility-app style, transparent safety messaging, and minimal visual noise. This is a separate product with original implementation and branding, not a fork of HatchZip.

## License

MIT. See [LICENSE](LICENSE).
