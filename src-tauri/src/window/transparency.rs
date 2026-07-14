use tauri::Manager;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE};
use windows::core::{s, w};

use crate::config;

#[repr(C)]
struct ACCENT_POLICY {
    accent_state: u32,
    accent_flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
#[allow(clippy::upper_case_acronyms)]
struct WINDOWCOMPOSITIONATTRIBDATA {
    attribute: u32,
    data: *mut ACCENT_POLICY,
    size_of_data: u32,
}

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
const ACCENT_ENABLE_HOSTBACKDROP: u32 = 5; // Mica (Win11+)

type SetWindowCompositionAttribute = unsafe extern "system" fn(HWND, *mut WINDOWCOMPOSITIONATTRIBDATA) -> i32;

fn get_swca() -> Option<SetWindowCompositionAttribute> {
    unsafe {
        let module = LoadLibraryA(s!("user32.dll")).ok()?;
        let addr = GetProcAddress(module, s!("SetWindowCompositionAttribute"))?;
        #[allow(clippy::missing_transmute_annotations)]
        Some(std::mem::transmute::<_, SetWindowCompositionAttribute>(addr))
    }
}

fn set_window_accent(hwnd: HWND, accent_state: u32, gradient_color: u32) {
    let Some(func) = get_swca() else { return };

    unsafe {
        let mut accent = ACCENT_POLICY {
            accent_state,
            accent_flags: 0,
            gradient_color,
            animation_id: 0,
        };
        let mut data = WINDOWCOMPOSITIONATTRIBDATA {
            attribute: WCA_ACCENT_POLICY,
            data: &mut accent,
            size_of_data: std::mem::size_of::<ACCENT_POLICY>() as u32,
        };
        func(hwnd, &mut data);
    }
}

/// Apply the bar's config-driven background material.
/// Only called for the bar window — settings/widgets use `apply_fixed_acrylic`.
pub fn apply_material(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let cfg = config::load();
    let is_light = !is_dark_mode();

    let mode = cfg.appearance.background.mode.as_str();
    let mode = match mode {
        "transparent" => "acrylic",
        other => other,
    };

    let alpha = (cfg.appearance.tint_alpha as u32).min(255);

    match mode {
        "acrylic" => {
            let base = if is_light { 0xF3F3F3u32 } else { 0x1A1A1Au32 };
            let color = (alpha << 24) | base;
            set_window_accent(hwnd, ACCENT_ENABLE_ACRYLICBLURBEHIND, color);
        }
        "mica" => {
            let base = if is_light { 0xF3F3F3u32 } else { 0x1A1A1Au32 };
            let mica_alpha = (alpha / 2).max(40);
            let color = (mica_alpha << 24) | base;
            set_window_accent(hwnd, ACCENT_ENABLE_HOSTBACKDROP, color);
        }
        _ => {
            set_window_accent(hwnd, ACCENT_DISABLED, 0);
        }
    }

    Ok(())
}

/// Apply a fixed neutral acrylic to settings/widgets/dialog windows.
/// Alpha at 25% so the blur effect is clearly visible with a subtle tint.
pub fn apply_fixed_acrylic(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let is_light = !is_dark_mode();
    let base = if is_light { 0xF3F3F3u32 } else { 0x1A1A1Au32 };
    let color = (0x59 << 24) | base; // 35% alpha = 89/255
    set_window_accent(hwnd, ACCENT_ENABLE_ACRYLICBLURBEHIND, color);
    Ok(())
}

pub fn set_rounded_corners(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let pref = DWM_WINDOW_CORNER_PREFERENCE(2);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
    Ok(())
}

/// Disable the OS open/close animation (fade+zoom) on a window.
///
/// Tauri builds windows with `visible(false)` and reveals them via
/// `SetWindowPos(SWP_SHOWWINDOW)`. DWM still animates that visibility
/// transition, which on a transparent acrylic window looks like a
/// "style change" — the bar briefly shows with no blur, then acrylic
/// settles in. Setting `DWMWA_TRANSITIONS_FORCEDISABLED` on the HWND
/// turns off that animation so the window appears in its final state.
pub fn set_disable_transitions(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let val: i32 = 1;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &val as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
    Ok(())
}

pub fn is_dark_mode() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let path = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        if RegOpenKeyExW(HKEY_CURRENT_USER, path, None, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let name = w!("AppsUseLightTheme");
        let mut buf: [u8; 4] = [0; 4];
        let mut len: u32 = 4;
        let mut ty = REG_VALUE_TYPE(0);
        let result =
            RegQueryValueExW(hkey, name, None, Some(&mut ty), Some(buf.as_mut_ptr()), Some(&mut len));
        let _ = RegCloseKey(hkey);
        result.is_ok() && buf[0] == 0
    }
}
