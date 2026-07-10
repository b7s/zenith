use windows::Win32::Foundation::{LPARAM, RECT, HWND};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITORINFO, GetMonitorInfoW};
use windows::Win32::UI::Shell::{APPBARDATA, SHAppBarMessage, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS, ABM_REMOVE};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, SetWindowPos,
    GWL_EXSTYLE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
};

fn monitor_of(hwnd: windows::Win32::Foundation::HWND) -> RECT {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(hmon, &mut mi);
        mi.rcMonitor
    }
}

/// Position a top-edge AppBar: compute rect from config + window's monitor,
/// call ABM_QUERYPOS + ABM_SETPOS, then SetWindowPos.
unsafe fn position_appbar(hwnd: HWND) {
    let cfg = crate::config::load();
    let bar_h = cfg.appearance.bar_height.max(20).min(200);
    let total_h = bar_h + cfg.appearance.margin_top + cfg.appearance.margin_bottom + cfg.appearance.padding_top + cfg.appearance.padding_bottom;
    let bar_height = total_h as i32;

    let mon = monitor_of(hwnd);
    let screen_w = mon.right - mon.left;

    let mut abd = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: 0,
        uEdge: 1,
        rc: RECT { left: 0, top: 0, right: screen_w, bottom: bar_height },
        lParam: LPARAM(0),
    };

    SHAppBarMessage(ABM_QUERYPOS, &mut abd);

    abd.rc.top = 0;
    abd.rc.left = 0;
    abd.rc.right = screen_w;
    abd.rc.bottom = bar_height;

    SHAppBarMessage(ABM_SETPOS, &mut abd);

    let r = abd.rc;
    let win_w = r.right - r.left;
    let win_h = r.bottom - r.top;

    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW;
    let _ = SetWindowPos(hwnd, None, r.left, r.top, win_w, win_h, flags);
}

pub fn register_appbar(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;

    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: 0,
            uEdge: 1,
            rc: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            lParam: LPARAM(0),
        };
        SHAppBarMessage(ABM_NEW, &mut abd);

        // AppBar registration can add WS_EX_APPWINDOW (0x00040000), which
        // makes the bar appear on the taskbar and respond to Win+M minimize-all.
        // Clear it and ensure WS_EX_TOOLWINDOW (0x00000080) is set instead.
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_ex = (ex & !0x00040000) | 0x00000080;
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex);

        position_appbar(hwnd);
    }
    Ok(())
}

/// Re-position an already-registered AppBar (e.g. after config changes).
pub fn update_appbar(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe { position_appbar(hwnd); }
    Ok(())
}

pub fn unregister_appbar(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                uCallbackMessage: 0,
                uEdge: 0,
                rc: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                lParam: LPARAM(0),
            };
            SHAppBarMessage(ABM_REMOVE, &mut abd);
        }
    }
}
