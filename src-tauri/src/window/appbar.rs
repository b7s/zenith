use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{APPBARDATA, SHAppBarMessage, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS, ABM_REMOVE};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};

pub fn register_appbar(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    register_hwnd(hwnd)
}

fn register_hwnd(hwnd: HWND) -> Result<(), String> {
    let cfg = crate::config::load();
    let bar_height = cfg.appearance.bar_height.max(20).min(200);

    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: 0,
            uEdge: 3, // ABE_TOP
            rc: RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: bar_height as i32,
            },
            lParam: LPARAM(0),
        };

        SHAppBarMessage(ABM_NEW, &mut abd);
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        SHAppBarMessage(ABM_SETPOS, &mut abd);
    }
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
