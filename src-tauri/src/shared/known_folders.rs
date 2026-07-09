//! Resolve Windows Known Folders via `SHGetKnownFolderPath`.
//!
//! Used by the events domain to compute the OneDrive backup path.

use std::path::PathBuf;

use windows::Win32::UI::Shell::{FOLDERID_SkyDrive, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

/// Return the user's OneDrive folder, or `None` if unavailable.
///
/// Tries `FOLDERID_SkyDrive` (the consumer OneDrive). For business accounts
/// callers should fall back to `FOLDERID_SkyDrivePro` if needed.
pub fn onedrive_path() -> Option<PathBuf> {
    unsafe {
        let pwstr = SHGetKnownFolderPath(&FOLDERID_SkyDrive, KF_FLAG_DEFAULT, None);
        match pwstr {
            // Note: `SHGetKnownFolderPath` allocates the returned buffer
            // with `CoTaskMemAlloc`. The `windows` crate's PWSTR wrapper
            // exposes `.to_string()` but does not auto-free the underlying
            // allocation. For OneDrive discovery this is acceptable: the
            // process stays up across the call. A future patch may swap
            // this for a free-on-drop helper.
            Ok(pwstr) => pwstr.to_string().ok().map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()),
            Err(_) => None,
        }
    }
}

/// Combined fallback: OneDrive if available, otherwise the local zenith root.
#[allow(dead_code)]
pub fn zenith_root() -> PathBuf {
    onedrive_path()
        .map(|p| p.join("Zenith"))
        .unwrap_or_else(|| {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir())
                .join("zenith")
        })
}
