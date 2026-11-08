# whatgrew engineering rules

- Rust 2021 stable. Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- `wg-core` is free of UI and console output. Never use `unwrap` or `expect` in non-test Rust code.
- Read-only filesystem scanner: no deletion, movement or modification of scanned files. Only the app's own data/config and OS schedule registration may be written.
- No application networking, telemetry, accounts, updater, or remote assets. Frontend dependencies are bundled at build time.
- Skip symlinks, Windows reparse points and cloud placeholders. Never try to hydrate offline cloud files.
- Do not cross filesystem boundaries on Unix. Hardlinks are deduplicated per scan.
- Partial scans must be visibly labeled (unreadable-directory count). Never misrepresent aggregate allocation as exact physical disk usage on APFS clones.
- Every bug fix or new logic requires tests. Use temporary filesystems; don't delete user data.
- Keep UI simple, high-contrast and keyboard accessible. No fake snapshots outside explicitly labeled demo fixtures.
- When touching releases, check macOS / Windows build results and clearly state unsigned-app limitations.
