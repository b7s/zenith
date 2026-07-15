use tauri::Manager;
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY, SE_PRIVILEGE_ENABLED,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::window;

const SHUTDOWN_LABEL: &str = "shutdown-popup";

/// Enable `SE_SHUTDOWN_NAME` in the current process token. Windows requires
/// this privilege for `ExitWindowsEx` and `SetSuspendState` even when the user
/// is an administrator — the privilege is present in the token but disabled by
/// default. Without it the calls fail and the action silently does nothing.
fn enable_shutdown_privilege() -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .map_err(|e| format!("OpenProcessToken: {e}"))?;

        let mut luid = LUID::default();
        LookupPrivilegeValueW(None, w!("SeShutdownPrivilege"), &mut luid)
            .map_err(|e| format!("LookupPrivilegeValueW: {e}"))?;

        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None)
            .map_err(|e| format!("AdjustTokenPrivileges: {e}"))?;
        let _ = CloseHandle(token);
        Ok(())
    }
}

#[tauri::command]
pub fn system_shutdown() -> Result<(), String> {
    enable_shutdown_privilege()?;
    unsafe {
        windows::Win32::System::Shutdown::ExitWindowsEx(
            windows::Win32::System::Shutdown::EWX_POWEROFF
                | windows::Win32::System::Shutdown::EWX_FORCE,
            windows::Win32::System::Shutdown::SHUTDOWN_REASON(0),
        )
        .map_err(|e| format!("ExitWindowsEx: {e}"))
    }
}

#[tauri::command]
pub fn system_restart() -> Result<(), String> {
    enable_shutdown_privilege()?;
    unsafe {
        windows::Win32::System::Shutdown::ExitWindowsEx(
            windows::Win32::System::Shutdown::EWX_REBOOT
                | windows::Win32::System::Shutdown::EWX_FORCE,
            windows::Win32::System::Shutdown::SHUTDOWN_REASON(0),
        )
        .map_err(|e| format!("ExitWindowsEx: {e}"))
    }
}

#[tauri::command]
pub fn system_sleep() -> Result<(), String> {
    enable_shutdown_privilege()?;
    let ok = unsafe {
        windows::Win32::System::Power::SetSuspendState(false, true, false)
    };
    if ok { Ok(()) } else { Err("SetSuspendState (sleep) returned FALSE".into()) }
}

#[tauri::command]
pub fn system_hibernate() -> Result<(), String> {
    enable_shutdown_privilege()?;
    let ok = unsafe {
        windows::Win32::System::Power::SetSuspendState(true, true, false)
    };
    if ok { Ok(()) } else { Err("SetSuspendState (hibernate) returned FALSE".into()) }
}

#[tauri::command]
pub fn system_lock() -> Result<(), String> {
    unsafe {
        windows::Win32::System::Shutdown::LockWorkStation()
            .map_err(|e| format!("LockWorkStation: {e}"))
    }
}

#[tauri::command]
pub fn system_logout() -> Result<(), String> {
    enable_shutdown_privilege()?;
    unsafe {
        windows::Win32::System::Shutdown::ExitWindowsEx(
            windows::Win32::System::Shutdown::EWX_LOGOFF
                | windows::Win32::System::Shutdown::EWX_FORCE,
            windows::Win32::System::Shutdown::SHUTDOWN_REASON(0),
        )
        .map_err(|e| format!("ExitWindowsEx: {e}"))
    }
}

#[tauri::command]
pub async fn open_shutdown_popup(app: tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    // Toggle: if the popup is already open, close it (clicking the bar button
    // again dismisses it). Otherwise create it.
    if let Some(win) = app.get_webview_window(SHUTDOWN_LABEL) {
        let _ = win.close();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_shutdown_popup_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

fn create_shutdown_popup_window(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let win_w = 360_i32;
    let win_h = 260_i32;
    let (cx, cy, cw, ch) = window::clamp_to_monitor(x.round() as i32, y.round() as i32, win_w, win_h);

    let win = tauri::WebviewWindowBuilder::new(
        app,
        SHUTDOWN_LABEL,
        tauri::WebviewUrl::App("shutdown-popup.html".into()),
    )
    .title("Shutdown")
    .inner_size(cw as f64, ch as f64)
    .min_inner_size(320.0, 220.0)
    .max_inner_size(460.0, 360.0)
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

    let _ = window::apply_fixed_acrylic(app, SHUTDOWN_LABEL);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

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
