//! Calendar popup window for the date/time widget.
//!
//! Mirrors the volume popup shape (transparent, frameless, fixed acrylic via
//! `window::apply_fixed_acrylic`) and uses `window::clamp_to_monitor` to keep
//! the final position inside the monitor that owns the triggering widget.

use tauri::Manager;

use crate::window;

const CALENDAR_LABEL: &str = "calendar";
// Single-month popup. Mirror of `CALENDAR_POPUP_CSS_*` in
// `src/shared/widget-popup.ts` — keep the two languages in sync.
const CALENDAR_W: i32 = 340;
const CALENDAR_H: i32 = 370;
// Two-month popup: same height, ~2× width. Activated by the widget's
// `show_next_month` config option. Same vertical alignment so the bar
// widget stays visually centred below both panels.
const CALENDAR_W_WIDE: i32 = 680;

#[tauri::command]
pub async fn open_calendar(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    wide: Option<bool>,
) -> Result<(), String> {
    let wide = wide.unwrap_or(false);
    let (win_w, win_h) = if wide {
        (CALENDAR_W_WIDE, CALENDAR_H)
    } else {
        (CALENDAR_W, CALENDAR_H)
    };

    if let Some(win) = app.get_webview_window(CALENDAR_LABEL) {
        // Re-using an already-open window: clamp to the requested position
        // (mode-aware), resize, show, refocus.
        let (cx, cy, cw, ch) =
            window::clamp_to_monitor(x.round() as i32, y.round() as i32, win_w, win_h);
        let _ = win.set_size(tauri::PhysicalSize::new(cw as f64, ch as f64));
        let _ = win.set_position(tauri::PhysicalPosition::new(cx as f64, cy as f64));
        let _ = win.show();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = win.set_focus();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_calendar_window(&app, x, y, wide))
        .await
        .map_err(|e| e.to_string())?
}

fn create_calendar_window(
    app: &tauri::AppHandle,
    x: f64,
    y: f64,
    wide: bool,
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
    .min_inner_size(if wide { 560.0 } else { 280.0 }, 300.0)
    .max_inner_size(if wide { 820.0 } else { 420.0 }, 480.0)
    .position(cx as f64, cy as f64)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .always_on_top(true)
    .additional_browser_args("--default-background-color=00000000")
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
