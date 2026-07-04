use tauri::Manager;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE};
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
struct WINDOWCOMPOSITIONATTRIBDATA {
    attribute: u32,
    data: *mut ACCENT_POLICY,
    size_of_data: u32,
}

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

type SetWindowCompositionAttribute = unsafe extern "system" fn(HWND, *mut WINDOWCOMPOSITIONATTRIBDATA) -> i32;

fn get_swca() -> Option<SetWindowCompositionAttribute> {
    unsafe {
        let module = LoadLibraryA(s!("user32.dll")).ok()?;
        let addr = GetProcAddress(module, s!("SetWindowCompositionAttribute"))?;
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

pub fn apply_material(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
    let Some(window) = app.get_webview_window(label) else {
        eprintln!("[zenith] apply_material: window '{label}' not found");
        return Ok(());
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let cfg = config::load();

    let is_light = !is_dark_mode();
    let (accent_state, base_color) = match cfg.appearance.material.as_str() {
        "acrylic" if is_light => (ACCENT_ENABLE_ACRYLICBLURBEHIND, 0x99F3F3F3),
        "acrylic" => (ACCENT_ENABLE_ACRYLICBLURBEHIND, 0x991A1A1A),
        "mica" if is_light => (ACCENT_ENABLE_ACRYLICBLURBEHIND, 0x66F3F3F3),
        "mica" => (ACCENT_ENABLE_ACRYLICBLURBEHIND, 0x661A1A1A),
        _ => (ACCENT_DISABLED, 0),
    };

    if accent_state == ACCENT_DISABLED {
        set_window_accent(hwnd, ACCENT_DISABLED, 0);
    } else {
        let alpha = (cfg.appearance.tint_alpha as u32).min(255).max(0);
        let color = (base_color & 0x00FFFFFF) | (alpha << 24);
        set_window_accent(hwnd, accent_state, color);
    }

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
