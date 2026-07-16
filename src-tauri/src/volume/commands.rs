use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::Manager;
use windows::core::Interface;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioSessionControl2, IAudioSessionEnumerator,
    IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::{CLSCTX_ALL, CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_WIN32,
    QueryFullProcessImageNameW,
};

use crate::window;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub level: f64,
    pub muted: bool,
}

/// One row in the per-app mixer accordion. `id` is the display name
/// (the prettified app name from `pretty_app_name`) — used as the
/// deduplication key and passed back to `set_app_volume` / `set_app_muted`
/// which apply the change to ALL audio sessions with that name.
///
/// `pid` is the owning process id (0 for the system-sounds session that
/// has no single owner). `name` duplicates `id` for convenience (so the
/// frontend doesn't need to special-case it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSessionInfo {
    pub id: String,
    pub pid: u32,
    pub name: String,
    pub level: f64,
    pub muted: bool,
}

fn get_endpoint_volume() -> Result<IAudioEndpointVolume, String> {
    unsafe {
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

fn get_session_manager() -> Result<IAudioSessionManager2, String> {
    unsafe {
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

        let mgr: IAudioSessionManager2 = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Activate IAudioSessionManager2: {e}"))?;

        Ok(mgr)
    }
}

fn process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()?;

        let _ = CloseHandle(handle);

        let full = String::from_utf16_lossy(&buf[..len as usize]);
        let file = full.rsplit(['\\', '/']).next()?;
        Some(file.trim_end_matches(".exe").to_string())
    }
}

fn pretty_app_name(raw: Option<String>) -> String {
    let Some(raw) = raw else {
        return "System sounds".into();
    };
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "" | "unknown" => "Unknown".into(),
        "audiodg" => "Windows Audio".into(),
        "system sounds" => "System sounds".into(),
        "chrome" | "msedge" | "firefox" | "brave" | "vivaldi" | "opera" => "Browser".into(),
        "explorer" => "Explorer".into(),
        "spotify" => "Spotify".into(),
        "discord" => "Discord".into(),
        "telegram" => "Telegram".into(),
        "zoom" => "Zoom".into(),
        _ => {
            if let Some(first) = raw.chars().next() {
                let mut s = String::new();
                s.extend(first.to_uppercase());
                s.push_str(&raw[first.len_utf8()..]);
                s
            } else {
                raw
            }
        }
    }
}

/// Determine the session's display name from its `IAudioSessionControl2`.
fn session_display_name(session2: &IAudioSessionControl2) -> String {
    unsafe {
        let pid = session2.GetProcessId().unwrap_or(0);
        let is_system = session2.IsSystemSoundsSession().0 == 0;
        pretty_app_name(if is_system { None } else { process_name(pid) })
    }
}

/// Enumerate every active audio session on the default render endpoint,
/// group them by display name (Sndvol-style), and return one row per
/// unique app with that app's current volume/mute state.
///
/// The volume / mute for each row is read from the session's *own*
/// `ISimpleAudioVolume` interface, obtained directly via
/// `QueryInterface` on the session's `IAudioSessionControl` COM object.
/// This avoids the `ISimpleAudioVolume(GUID_NULL, cross-process)` quirk:
/// when an application never calls `SetGroupingParam`, Windows reports
/// `GUID_NULL` and `GetSimpleAudioVolume` with that NULL pointer targets
/// the **default session**, not the specific tab/context session — which
/// is why per-app sliders previously failed to mute browsers (Chromium,
/// Firefox, Electron-based: each tab/audio context has its own session
/// with grouping `GUID_NULL`).
///
/// Some runtimes (WebView2, Chromium, Firefox, AMD noise reduction,
/// Electron) create one audio session per context — each with the SAME
/// process name but a distinct COM object. Grouping by name collapses
/// them into one row and mirrors how Sndvol/Windows Volume Mixer
/// presents per-app controls. Writes (`set_app_volume`, `set_app_muted`)
/// walk every session whose `process_name(pid)` matches.
#[tauri::command]
pub fn get_app_sessions() -> Result<Vec<AppSessionInfo>, String> {
    let mgr = get_session_manager()?;
    unsafe {
        let enumerator: IAudioSessionEnumerator = mgr
            .GetSessionEnumerator()
            .map_err(|e| format!("GetSessionEnumerator: {e}"))?;

        let count = enumerator
            .GetCount()
            .map_err(|e| format!("IAudioSessionEnumerator::GetCount: {e}"))?;

        let mut out: Vec<AppSessionInfo> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for i in 0..count {
            let session = match enumerator.GetSession(i) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let state = match session.GetState() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if state.0 == 2 {
                continue;
            }
            let session2: IAudioSessionControl2 = match session.cast() {
                Ok(s2) => s2,
                Err(_) => continue,
            };

            let name = session_display_name(&session2);
            if !seen.insert(name.clone()) {
                continue;
            }

            let pid = session2.GetProcessId().unwrap_or(0);
            let simple: ISimpleAudioVolume = match session.cast::<ISimpleAudioVolume>() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let level = simple
                .GetMasterVolume()
                .map(|v| v as f64)
                .unwrap_or(0.5);
            let muted = simple.GetMute().map(|v| v.as_bool()).unwrap_or(false);

            out.push(AppSessionInfo {
                id: name.clone(),
                pid,
                name,
                level,
                muted,
            });
        }

        Ok(out)
    }
}

/// Set the per-session volume for ALL audio sessions whose display name
/// matches `id`. This mirrors Sndvol: one row controls every instance
/// of that app.
#[tauri::command]
pub fn set_app_volume(id: String, level: f64) -> Result<(), String> {
    let mgr = get_session_manager()?;
    let clamped = level.clamp(0.0, 1.0) as f32;
    unsafe {
        let enumerator: IAudioSessionEnumerator = mgr
            .GetSessionEnumerator()
            .map_err(|e| format!("GetSessionEnumerator: {e}"))?;
        let count = enumerator
            .GetCount()
            .map_err(|e| format!("IAudioSessionEnumerator::GetCount: {e}"))?;

        let mut matched = 0u32;
        for i in 0..count {
            let session = match enumerator.GetSession(i) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let session2: IAudioSessionControl2 = match session.cast() {
                Ok(s2) => s2,
                Err(_) => continue,
            };
            if session_display_name(&session2) != id {
                continue;
            }
            matched += 1;
            let simple: ISimpleAudioVolume = session
                .cast::<ISimpleAudioVolume>()
                .map_err(|e| format!("cast ISimpleAudioVolume for {id}: {e}"))?;
            simple
                .SetMasterVolume(clamped, core::ptr::null())
                .map_err(|e| format!("SetMasterVolume for {id}: {e}"))?;
        }
        if matched == 0 {
            return Err(format!("no session found with name '{id}'"));
        }
    }
    Ok(())
}

/// Toggle mute for ALL audio sessions whose display name matches `id`.
#[tauri::command]
pub fn set_app_muted(id: String, muted: bool) -> Result<(), String> {
    let mgr = get_session_manager()?;
    unsafe {
        let enumerator: IAudioSessionEnumerator = mgr
            .GetSessionEnumerator()
            .map_err(|e| format!("GetSessionEnumerator: {e}"))?;
        let count = enumerator
            .GetCount()
            .map_err(|e| format!("IAudioSessionEnumerator::GetCount: {e}"))?;

        let mut matched = 0u32;
        for i in 0..count {
            let session = match enumerator.GetSession(i) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let session2: IAudioSessionControl2 = match session.cast() {
                Ok(s2) => s2,
                Err(_) => continue,
            };
            if session_display_name(&session2) != id {
                continue;
            }
            matched += 1;
            let simple: ISimpleAudioVolume = session
                .cast::<ISimpleAudioVolume>()
                .map_err(|e| format!("cast ISimpleAudioVolume for {id}: {e}"))?;
            simple
                .SetMute(muted, core::ptr::null())
                .map_err(|e| format!("SetMute for {id}: {e}"))?;
        }
        if matched == 0 {
            return Err(format!("no session found with name '{id}'"));
        }
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
        // Toggle: clicking the bar button again dismisses the popup.
        let _ = win.close();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_volume_popup_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

fn create_volume_popup_window(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let win_w = 300.0_f64;
    let win_h = 72.0_f64;
    let (cx, cy, cw, ch) = window::monitor::clamp_to_monitor(
        x.round() as i32, y.round() as i32, win_w as i32, win_h as i32,
    );
    let win = tauri::WebviewWindowBuilder::new(
        app,
        VOLUME_POPUP_LABEL,
        tauri::WebviewUrl::App("widgets/volume/window/volume-popup.html".into()),
    )
    .title("Volume")
    .inner_size(cw as f64, ch as f64)
    .min_inner_size(260.0, 56.0)
    .max_inner_size(400.0, 400.0)
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

    let _ = window::apply_fixed_acrylic(app, VOLUME_POPUP_LABEL);
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
