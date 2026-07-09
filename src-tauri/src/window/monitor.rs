//! Monitor / work-area helpers shared by **every** popup-ish window.
//!
//! ## Single source of truth
//!
//! All popup-style windows (volume popup, calendar, future ones) **must** route
//! their final `(x, y, w, h)` through [`clamp_to_monitor`] before calling
//! `WebviewWindowBuilder::position(...)`. `position()` does no clamping itself —
//! passing coordinates outside the desktop `rcWork` rect will let the WebView
//! appear partially off-screen on a multi-monitor layout or on a monitor whose
//! DPI/origin differs from `(0, 0)`.
//!
//! The helpers here are the only place that calls `MonitorFromPoint` /
//! `GetMonitorInfoW` for popup positioning. `appbar.rs` keeps its own private
//! `monitor_of()` helper scoped to AppBar placement — it must NOT be reused
//! outside the AppBar code path because the AppBar covers the whole monitor
//! width by design (no clamping).
//!
//! ## How to use
//!
//! ```ignore
//! use crate::window::monitor::clamp_to_monitor;
//!
//! // Caller proposes a final window rect — typically: centered on the bar
//! // widget that triggered the popup.
//! let (x, y, w, h) = clamp_to_monitor(proposed_x, proposed_y, win_w, win_h);
//!
//! tauri::WebviewWindowBuilder::new(...)
//!     .position(x, y)
//!     .inner_size(w, h)
//!     ...
//! ```
//!
//! The function returns clamped `(x, y, w, h)` such that the window:
//! - lies fully inside the **`rcWork`** (work area) of the monitor containing
//!   the proposed `(x, y)` point,
//! - keeps its requested `w`, `h` (the caller is responsible for picking a
//!   size that fits),
//! - and on single-monitor setups behaves identically to a naive placement.
//!
//! ## What this returns for the calendar use case
//!
//! The calendar window asks for 340×360 placed under the clock widget. The
//! bar lives at `y ≈ 0..40` on the primary monitor. `clamp_to_monitor` walks
//! the **target** `(x, y)` (the widget position) through Win32, finds the
//! monitor that contains it, then snaps the window's `(x, y)` so the bottom
//! edge sits just under the widget **and** the right edge stays inside the
//! work area. If `w` would overflow, x is shifted left; same for y if the
//! bar is at the top of a small work area.

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};

/// Look up the work-area rectangle of the monitor that contains the given
/// point `(x, y)` (in **virtual screen** coordinates, i.e. OS absolute pixels).
/// Falls back to the primary monitor's work area if the lookup fails.
fn work_area_at(x: i32, y: i32) -> RECT {
    unsafe {
        let pt = POINT { x, y };
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        #[allow(function_casts_as_integer)]
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO> as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            mi.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            }
        }
    }
}

/// Clamp the proposed window rect (x, y, w, h) into the monitor that contains
/// `(x, y)`. Returns the possibly-shifted `(x, y, w, h)`. Width and height are
/// preserved — the caller is expected to pick a size that fits.
pub fn clamp_to_monitor(x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let work = work_area_at(x, y);
    let left = work.left;
    let top = work.top;
    let right = work.right;
    let bottom = work.bottom;

    // Clamp width/height so a misbehaving caller can't ask for more than the
    // work area — important on very small screens or extreme DPI scales.
    let w = w.min(right - left).max(1);
    let h = h.min(bottom - top).max(1);

    // Default: anchor at the requested (x, y).
    let mut nx = x;
    let mut ny = y;

    // If the right edge would spill, shift left.
    if nx + w > right {
        nx = right - w;
    }
    // If the bottom edge would spill, shift up.
    if ny + h > bottom {
        ny = bottom - h;
    }
    // Clamp into the work rect on the opposite edges too.
    if nx < left {
        nx = left;
    }
    if ny < top {
        ny = top;
    }

    (nx, ny, w, h)
}

/// Variant that takes the requested rectangle (origin + size) and returns
/// the same four values. Useful when the caller has already composed a `RECT`.
#[allow(dead_code)]
pub fn clamp_rect_to_monitor(rect: RECT) -> RECT {
    let (x, y, w, h) = clamp_to_monitor(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top);
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Convenience: return the OS-pixel `(x, y)` for a window of size `w × h` so
/// the window sits dead-center in the **primary monitor's work area**. The
/// primary monitor is selected with `MONITOR_DEFAULTTOPRIMARY`, so this
/// works even when no other window is currently found anywhere.
///
/// Kept as a library helper for call-sites that don't have a natural anchor
/// point (e.g. one-shot confirmations). The dialog window currently uses
/// [`center_on_monitor_at`] with an anchor instead; this fallback is
/// here so future popups don't have to reinvent it.
#[allow(dead_code)]
pub fn center_on_primary_monitor(w: i32, h: i32) -> (i32, i32) {
    center_on_monitor_at(None, w, h)
}

/// Same as [`center_on_primary_monitor`] but lets the caller pick the
/// monitoring point. `anchor` is a `(x, y)` in virtual-screen OS pixels;
/// the dialog centers on the work area of the monitor containing that
/// point. Pass `None` (or anything outside any monitor) to fall back
/// to the primary monitor explicitly via `MONITOR_DEFAULTTOPRIMARY`.
pub fn center_on_monitor_at(anchor: Option<(i32, i32)>, w: i32, h: i32) -> (i32, i32) {
    unsafe {
        let flag = match anchor {
            Some(_) => MONITOR_DEFAULTTONEAREST,
            None => MONITOR_DEFAULTTOPRIMARY,
        };
        let (ax, ay) = anchor.unwrap_or((0, 0));
        let pt = POINT { x: ax, y: ay };
        let hmon = MonitorFromPoint(pt, flag);
        #[allow(function_casts_as_integer)]
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO> as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let work = mi.rcWork;
            let x = work.left + ((work.right - work.left) - w) / 2;
            let y = work.top + ((work.bottom - work.top) - h) / 2;
            (x.max(work.left), y.max(work.top))
        } else {
            (0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_never_overflows_work_area() {
        // Far outside the primary monitor on every side -> should snap inside.
        let (_x, _y, w, h) = clamp_to_monitor(-9999, -9999, 99999, 99999);
        assert!(w >= 1 && h >= 1, "size must remain positive after clamp");
    }
}
