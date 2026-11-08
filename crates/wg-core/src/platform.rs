use std::fs::Metadata;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};

pub fn allocated(path: &Path, meta: &Metadata) -> u64 {
    #[cfg(unix)]
    { let _ = path; meta.blocks().saturating_mul(512) }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut high = 0_u32;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path, and `high` is valid writable memory.
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
        if low == u32::MAX && std::io::Error::last_os_error().raw_os_error().is_some_and(|e| e != 0) {
            meta.file_size()
        } else { ((high as u64) << 32) | u64::from(low) }
    }
    #[cfg(not(any(unix, windows)))]
    { let _ = path; meta.len() }
}

pub fn is_link_or_placeholder(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() { return true; }
    #[cfg(windows)]
    {
        const REPARSE: u32 = 0x400;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        const OFFLINE: u32 = 0x1000;
        let flags = meta.file_attributes();
        return flags & (REPARSE | RECALL_ON_DATA_ACCESS | OFFLINE) != 0;
    }
    #[cfg(not(windows))]
    { false }
}

pub fn identity(meta: &Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    { (meta.nlink() > 1).then_some((meta.dev(), meta.ino())) }
    #[cfg(not(unix))]
    { let _ = meta; None }
}

pub fn device(meta: &Metadata) -> Option<u64> {
    #[cfg(unix)] { Some(meta.dev()) }
    #[cfg(not(unix))] { let _ = meta; None }
}

pub fn volume_space(path: &Path) -> (u64, u64) {
    (fs2::total_space(path).unwrap_or(0), fs2::available_space(path).unwrap_or(0))
}

pub fn default_root() -> std::path::PathBuf {
    #[cfg(windows)]
    { std::env::var_os("SystemDrive").map(|s| std::path::PathBuf::from(format!("{}\\", s.to_string_lossy()))).unwrap_or_else(|| "C:\\".into()) }
    #[cfg(not(windows))]
    { dirs::home_dir().unwrap_or_else(|| ".".into()) }
}
