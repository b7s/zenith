use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::UI::Shell::{APPBARDATA, SHAppBarMessage, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS, ABM_REMOVE};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SetWindowPos, SM_CXSCREEN, SWP_NOACTIVATE, SWP_NOZORDER,
};

pub fn register_appbar(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let cfg = crate::config::load();

    let bar_height = cfg.appearance.bar_height.max(20).min(200) as i32;
    let margin_top = cfg.appearance.margin_top.max(0);
    let margin_right = cfg.appearance.margin_right.max(0);
    let margin_bottom = cfg.appearance.margin_bottom.max(0);
    let margin_left = cfg.appearance.margin_left.max(0);

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let reserved_height = margin_top + bar_height + margin_bottom;

    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: 0,
            uEdge: 1, // ABE_TOP
            rc: RECT {
                left: 0,
                top: 0,
                right: screen_w,
                bottom: reserved_height,
            },
            lParam: LPARAM(0),
        };

        SHAppBarMessage(ABM_NEW, &mut abd);
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        SHAppBarMessage(ABM_SETPOS, &mut abd);

        let r = abd.rc;
        let win_x = r.left + margin_left;
        let win_y = r.top + margin_top;
        let win_w = (r.right - r.left) - margin_left - margin_right;
        let win_h = bar_height;

        let _ = SetWindowPos(
            hwnd,
            None,
            win_x,
            win_y,
            win_w,
            win_h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    Ok(())
}

#[allow(dead_code)]
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
