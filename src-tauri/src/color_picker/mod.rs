//! Color-picker domain.
//!
//! Powers the bar widget's eyedropper mode and the right-click
//! color-picker window.
//!
//! - `start_eyedropper` captures every monitor into a static cache (one
//!   BGRA buffer per monitor) and returns the bounds so the frontend can
//!   stitch them onto a fullscreen overlay canvas.
//! - `eyedropper_pixel` returns the cached pixel at virtual-screen coords
//!   (used for real-time preview during eyedropper hover).
//! - `read_live_pixel` does a fresh 1×1 BitBlt for exact pick accuracy.
//! - `end_eyedropper` frees the cache (called on overlay close).
//! - `open_color_picker` creates the picker window (mirrors the calendar
//!   popup shape: transparent, frameless, fixed acrylic, clamped to the
//!   owning monitor).

pub mod commands;
