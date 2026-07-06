//! Foreground-window helpers for the workspace domain.
//!
//! `SetWinEventHook` for `EVENT_SYSTEM_FOREGROUND` is **disabled** in this
//! build: on Windows 11 24H2 from a Tauri/Rust binary it returned
//! `ERROR_HOOK_NEEDS_HMOD (1428)` and never installed, so all cache writes
//! came solely from the seed at Zenith startup. That stale cache caused move
//! window to act on the wrong window (the one foreground when Zenith launched)
//! every time.
//!
//! Until the WinEvent hook is rewritten — or we adopt a different mechanism
//! for capturing a top-level application HWND — move/pin actions are gated
//! off in `commands.rs::build_workspace_menu`. The workspace widget's
//! remaining functionality (rename, delete, create, switch) is unaffected.
//!
//! See `lib.rs::setup` for the temporary installation marker and
//! `commands.rs::build_workspace_menu` for the gating code.

use std::ffi::c_void;

/// Returns null. See module docs.
///
/// Pending reinstatement of foreground-capture: callers should walk the
/// returned HWND up to its top-level owner via `GetAncestor(hwnd, GA_ROOTOWNER)`
/// and filter out windows whose `GetWindowThreadProcessId` equals the host
/// process PID.
#[allow(dead_code)]
pub fn last_real_foreground_ptr() -> *mut c_void {
    std::ptr::null_mut()
}

/// Returns null. See module docs.
#[allow(dead_code)]
pub fn live_foreign_foreground_ptr() -> *mut c_void {
    std::ptr::null_mut()
}

/// Returns null. See module docs.
#[allow(dead_code)]
pub fn best_effort_foreground_ptr() -> *mut c_void {
    std::ptr::null_mut()
}

/// No-op. Re-enable when a foreground-capture mechanism is in place.
pub fn install() {
    // Intentionally empty.
}
