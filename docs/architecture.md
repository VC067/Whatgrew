# Architecture and safety notes

## Responsibilities

- `wg-core`: library only; can be used in both CLI and GUI. Scans recursively, aggregates allocated and logical bytes, records big files, and saves compressed versioned snapshots. No printing or UI dependencies.
- `wg-cli`: terminal output, JSON, OS scheduling, choice of snapshot pairs.
- `whatgrew-desktop`: Tauri IPC endpoints, process-spawned scans, native scheduler integration, UI state, and Windows/macOS system file-manager reveal.

## Scanner

The scanner traverses real directories using `symlink_metadata`; skips symlinks and Windows reparse/offline entries; and verifies Unix `st_dev` remains on the same filesystem. `rayon` processes entries concurrently. Unix hardlinked files (nlink > 1) are deduplicated by `(st_dev, st_ino)` using a shared set. Errors are collected (up to 50 messages, count unlimited) instead of aborting the scan. Cancellation aborts the scan and **does not save** partial data.

Per-file allocated space is `st_blocks * 512` on Unix, and `GetCompressedFileSizeW` with a logical-size fallback when the call fails on Windows. For performance this early implementation does not yet use batched NTFS enumeration, Windows file-ID hardlink deduplication, or block sharing detection on APFS.

Each directory node contains recursive totals and a parent ID; individual small files are discarded. Directories below 5 MiB deeper than depth three are pruned from *saved* snapshots; their bytes remain in ancestor totals. Up to 1,000 files with allocated size >= 50 MiB are separately stored.

## Growth attribution

Snapshots must share an exact canonical scan root. Directory paths are reconstructed from single path components; Windows comparisons are case-insensitive. Growth = new allocated - previous allocated bytes. A directory with a change above the configurable hotspot minimum descends toward significant children when at least one explains >= 60% of the parent's delta. Otherwise that directory itself is presented as the hotspot. Diffs use signed deltas, so shrinkage is shown separately. Available disk-space changes are read from snapshot headers, not inferred from scanned folders.

## Tauri IPC

`get_settings`, `set_settings`, `list_snapshots`, `start_scan`, `cancel_scan`, `get_diff`, `get_children`, `get_big_files`, `reveal`, `set_schedule`. A scan runs on a worker thread and sends `scan-progress` and `scan-done` events. Explore lazily requests one directory level at a time so a large snapshot is not sent whole to the WebView.

A scan reads file metadata only. The UI can *reveal* files in the OS file manager; it has no delete endpoint and makes no network requests. Filesystem records are sensitive local metadata and should not be uploaded automatically.

## Things requiring real-machine verification

1. Mac APFS permissions, firmlinks and Full Disk Access; whole disk scan entry point may need `/System/Volumes/Data` instead of `/`.
2. Windows allocation size, inaccessible `System Volume Information`, compressed files, junctions, and OneDrive placeholders.
3. macOS LaunchAgent and Windows Task Scheduler registration with installation paths containing spaces.
4. Cancel responsiveness and scan speed on millions of files. Don't promise WizTree-level MFT speed without measurements.
5. Windows installer SmartScreen and macOS Gatekeeper for unsigned/unnotarized builds.
