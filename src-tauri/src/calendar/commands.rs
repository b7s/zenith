//! Calendar popup window for the date/time widget.
//!
//! Mirrors the volume popup shape (transparent, frameless, fixed acrylic via
//! `window::apply_fixed_acrylic`) and uses `window::clamp_to_monitor` to keep
//! the final position inside the monitor that owns the triggering widget.

use std::sync::{Mutex, OnceLock};

use tauri::{Emitter, Manager};

use crate::shared::EVENT_CALENDAR_VIEW;
use crate::window;

const CALENDAR_LABEL: &str = "calendar";
// Single-month popup. Mirror of `CALENDAR_POPUP_CSS_*` in
// `src/shared/widget-popup.ts` — keep the two languages in sync.
const CALENDAR_W: i32 = 360;
const CALENDAR_H: i32 = 410;
// Two-month popup: same height, ~2× width. Activated by the widget's
// `show_next_month` config option. Same vertical alignment so the bar
// widget stays visually centred below both panels.
const CALENDAR_W_WIDE: i32 = 700;

/// The currently-requested calendar view mode. Stored in a Mutex so the
/// frontend can query it via `get_calendar_view()` on load — this is the
/// **primary** source of truth (the init script is just an instant seed,
/// and the event is a push-notification for reuse). Mirrors the proven
/// `DIALOG_STATE` + `get_dialog_data` pattern used by the unified dialog.
static CALENDAR_VIEW_STATE: OnceLock<Mutex<String>> = OnceLock::new();

fn calendar_view_state() -> &'static Mutex<String> {
    CALENDAR_VIEW_STATE.get_or_init(|| Mutex::new("calendar".into()))
}

#[tauri::command]
pub async fn open_calendar(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    wide: Option<bool>,
    mode: Option<String>,
    single: Option<bool>,
) -> Result<(), String> {
    // "calendar" (default) | "events" — when events, only the events list is
    // rendered (used by the alarms widget). Hides the month grid + nav.
    // In events mode we ALWAYS collapse to a single month width even if
    // the user configured `show_next_month`: the dates list is a single
    // narrow column, so a two-panel (680px) layout would leave
    // whitespace / break the flex-fit. See §13.x "calendar events".
    let view = match mode.as_deref() {
        Some("events") => "events",
        _ => "calendar",
    };
    // Persist the view so the frontend can query it via `get_calendar_view()`
    // on load. This is the primary source of truth — the init script is just
    // an instant seed and the event is a push for reuse.
    if let Ok(mut g) = calendar_view_state().lock() {
        *g = view.into();
    }
    let single = single.unwrap_or(false) || view == "events";
    if let Ok(mut g) = calendar_single_state().lock() {
        *g = single;
    }
    let wide = if single { false } else { wide.unwrap_or(false) };
    let (win_w, win_h) = if wide {
        (CALENDAR_W_WIDE, CALENDAR_H)
    } else {
        (CALENDAR_W, CALENDAR_H)
    };

    if let Some(win) = app.get_webview_window(CALENDAR_LABEL) {
        // Re-using an already-open window: clamp to the requested position
        // (mode-aware), resize, show, refocus. Critically, we also emit the
        // view-switch event so the frontend swaps modes — otherwise a window
        // previously opened by the datetime widget (2-month calendar grid)
        // would keep rendering that grid even when the alarms widget asked
        // for the events list (the init script does NOT re-run on reuse).
        let (cx, cy, cw, ch) =
            window::clamp_to_monitor(x.round() as i32, y.round() as i32, win_w, win_h);
        let _ = win.set_size(tauri::PhysicalSize::new(cw as f64, ch as f64));
        let _ = win.set_position(tauri::PhysicalPosition::new(cx as f64, cy as f64));
        let _ = win.show();
        let _ = win.emit(EVENT_CALENDAR_VIEW, view);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = win.set_focus();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_calendar_window(&app, x, y, wide, view))
        .await
        .map_err(|e| e.to_string())?
}

/// Return the currently-requested calendar view mode
/// (`"calendar"` | `"events"`). The frontend calls this on load as the
/// primary source of truth (mirrors the dialog's `get_dialog_data`).
#[tauri::command]
pub fn get_calendar_view() -> String {
    calendar_view_state()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "calendar".into())
}

/// Whether the current view forces a single month (no `show_next_month`
/// two-panel layout). True when the view is "events" or the caller passed
/// `single: true`. The frontend uses this to suppress `showNextMonth`
/// even in calendar mode as a safety net.
#[tauri::command]
pub fn get_calendar_single() -> bool {
    calendar_single_state()
        .lock()
        .map(|g| *g)
        .unwrap_or(false)
}

static CALENDAR_SINGLE_STATE: OnceLock<Mutex<bool>> = OnceLock::new();
fn calendar_single_state() -> &'static Mutex<bool> {
    CALENDAR_SINGLE_STATE.get_or_init(|| Mutex::new(false))
}

fn create_calendar_window(
    app: &tauri::AppHandle,
    x: f64,
    y: f64,
    wide: bool,
    view: &str,
) -> Result<(), String> {
    let (disp_w, disp_h) = if wide {
        (CALENDAR_W_WIDE, CALENDAR_H)
    } else {
        (CALENDAR_W, CALENDAR_H)
    };
    let (cx, cy, cw, ch) =
        window::clamp_to_monitor(x.round() as i32, y.round() as i32, disp_w, disp_h);

    let win = tauri::WebviewWindowBuilder::new(
        app,
        CALENDAR_LABEL,
        tauri::WebviewUrl::App("calendar.html".into()),
    )
    .title("Calendar")
    .inner_size(cw as f64, ch as f64)
    .min_inner_size(280.0, 300.0)
    .max_inner_size(820.0, 600.0)
    .position(cx as f64, cy as f64)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .always_on_top(true)
    .additional_browser_args("--default-background-color=00000000")
    .initialization_script(format!(
        "window.__ZENITH_CALENDAR_VIEW = '{}';",
        view
    ))
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, CALENDAR_LABEL);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

    // Show after the material is applied (no white flash) and DROP NOACTIVATE
    // so the popup actually takes foreground on open (see §13.10b).
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = win.set_focus();

    Ok(())
}
