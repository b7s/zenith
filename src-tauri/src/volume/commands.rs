use serde::{Deserialize, Serialize};
use tauri::Manager;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::{CLSCTX_ALL, CoInitializeEx, COINIT_APARTMENTTHREADED};

use crate::window;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub level: f64,
    pub muted: bool,
}

fn get_endpoint_volume() -> Result<IAudioEndpointVolume, String> {
    unsafe {
        // CoInitializeEx is safe to call multiple times: returns S_OK on first init,
        // S_FALSE if already initialized on this thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let enumerator: IMMDeviceEnumerator = windows::Win32::System::Com::CoCreateInstance(
            &MMDeviceEnumerator as *const _,
            None,
            CLSCTX_ALL,
        )
        .map_err(|e| format!("CoCreateInstance MMDeviceEnumerator: {e}"))?;

        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| format!("GetDefaultAudioEndpoint: {e}"))?;

        let ep: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Activate IAudioEndpointVolume: {e}"))?;

        Ok(ep)
    }
}

#[tauri::command]
pub fn get_volume() -> Result<VolumeInfo, String> {
    let ep = get_endpoint_volume()?;
    unsafe {
        let level = ep
            .GetMasterVolumeLevelScalar()
            .map_err(|e| format!("GetMasterVolumeLevelScalar: {e}"))?;
        let muted = ep
            .GetMute()
            .map_err(|e| format!("GetMute: {e}"))?;
        Ok(VolumeInfo {
            level: level as f64,
            muted: muted.as_bool(),
        })
    }
}

#[tauri::command]
pub fn set_volume(level: f64) -> Result<(), String> {
    let ep = get_endpoint_volume()?;
    let clamped = level.clamp(0.0, 1.0);
    unsafe {
        ep.SetMasterVolumeLevelScalar(clamped as f32, core::ptr::null())
            .map_err(|e| format!("SetMasterVolumeLevelScalar: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_muted(muted: bool) -> Result<(), String> {
    let ep = get_endpoint_volume()?;
    unsafe {
        ep.SetMute(muted, core::ptr::null())
            .map_err(|e| format!("SetMute: {e}"))?;
    }
    Ok(())
}

const VOLUME_POPUP_LABEL: &str = "volume-popup";

#[tauri::command]
pub async fn open_volume_popup(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(VOLUME_POPUP_LABEL) {
        let _ = win.set_focus();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_volume_popup_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

fn create_volume_popup_window(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let win = tauri::WebviewWindowBuilder::new(
        app,
        VOLUME_POPUP_LABEL,
        tauri::WebviewUrl::App("volume-popup.html".into()),
    )
    .title("Volume")
    .inner_size(260.0, 60.0)
    .min_inner_size(200.0, 48.0)
    .max_inner_size(400.0, 80.0)
    .position(x, y)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .additional_browser_args("--default-background-color=00000000")
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, VOLUME_POPUP_LABEL);
    let _ = window::set_rounded_corners(&win);

    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
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
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    let _ = win.set_focus();

    Ok(())
}
