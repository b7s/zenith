//! Tiny shell helpers (single home — do not duplicate).
//!
//! Re-exposed by the git commands so existing callers don't change.
//! Other domains that need to talk to the shell (e.g. calendar_sync
//! opening the OAuth browser) call into this module directly.

/// Open an external URL in the user's default browser via `ShellExecuteW`'s
/// `"open"` verb. Returns `true` on success.
///
/// Used by the widget-config window to deep-link into the OAuth consent
/// screen, and by the git widget to jump straight to a failed run or
/// open PR on the provider's site.
pub fn open_url(url: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = HSTRING::from("open");
    let file = HSTRING::from(url);
    let r = unsafe { ShellExecuteW(None, &verb, &file, None, None, SW_SHOWNORMAL) };
    // HINSTANCE > 32 (casted as a usize) means success per Win32 convention.
    r.0 as usize > 32
}
