# Release acceptance checklist (real devices required)

- [ ] `cargo fmt`, `clippy -D warnings`, unit/integration tests pass on Windows + macOS.
- [ ] GUI launches on Windows 10/11 with WebView2 and on macOS Intel/ARM.
- [ ] First scan stores a snapshot, shows baseline and largest folders, no fake comparison.
- [ ] After adding/removing test files, second scan shows signed delta at the correct parent.
- [ ] A symlink loop, Windows junction and offline OneDrive file are not followed/downloaded.
- [ ] Permission-denied paths do not crash scan; warning appears with the correct count.
- [ ] Cancel a long scan; no partial `.wg` snapshot remains.
- [ ] Drill down and up in the treemap; verify all rectangle sizes reflect underlying bytes.
- [ ] Reveal and copy file paths; test paths containing spaces, Unicode, and punctuation.
- [ ] Weekly scheduling runs as the *same local user*; disabling it removes the scheduled task.
- [ ] macOS Full Disk Access instructions verified for CLI and GUI.
- [ ] Windows and macOS memory usage and peak CPU measured on real large volumes.
- [ ] Sign installers/notarize or publish prominent warnings with checksum instructions.
- [ ] Verify fresh-machine installs and complete file checksums before public release.

## First release gate

This source bundle was assembled in an environment with no Rust toolchain and no npm-registry access. The Rust unit tests, Windows/macOS installers and GUI interactions were **not** executed here; treat the workflows as required acceptance checks rather than proof that the app is already production-ready. Do not publish binaries as "tested" until the checklist is completed.
