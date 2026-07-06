use std::sync::atomic::{AtomicU32, AtomicPtr, AtomicBool, Ordering};
use std::ffi::c_void;
use std::sync::mpsc;
use serde::Serialize;
use tauri::Emitter;

use windows_058::Win32::Foundation::HWND as WinvdHwnd;
use windows::Win32::Foundation::HWND as Win61Hwnd;

/// Last "real" foreground window (not belonging to our process), updated by the
/// `EVENT_SYSTEM_FOREGROUND` hook. This is the window that move/pin act on.
static WS_FOREGROUND_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static FALLBACK_COUNT: AtomicU32 = AtomicU32::new(0);
static FALLBACK_ACTIVE: AtomicU32 = AtomicU32::new(0);

/// Guards all workspace menu actions so `on_menu_event` firing twice (Tauri 2
/// quirk) cannot double-execute create / move / pin.
static WS_ACTION_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Best-effort foreground HWND: prefer the event-tracked "real" window, fall
/// back to the explicit capture taken at menu-open time.
pub fn get_cached_foreground_hwnd_ptr() -> *mut c_void {
    let tracked = super::foreground::last_real_foreground_ptr();
    if !tracked.is_null() { tracked } else { WS_FOREGROUND_HWND.load(Ordering::Relaxed) }
}

pub fn set_foreground_hwnd(hwnd: *mut c_void) {
    if !hwnd.is_null() {
        WS_FOREGROUND_HWND.store(hwnd, Ordering::Relaxed);
    }
}

fn get_winvd_hwnd() -> WinvdHwnd { WinvdHwnd(get_cached_foreground_hwnd_ptr()) }
fn skip_hwnd(hwnd: WinvdHwnd) -> bool { hwnd.is_invalid() }

/// Try to claim the single-action guard. Returns true if this caller should
/// proceed, false if a duplicate event already claimed it. The guard is held
/// for a short window (RELEASE_AFTER_MS) so the duplicate `on_menu_event`
/// that Tauri 2 fires ~immediately after the first is dropped.
pub fn try_claim_action() -> bool { !WS_ACTION_IN_FLIGHT.swap(true, Ordering::SeqCst) }

/// Schedule the release of the action guard after `RELEASE_AFTER_MS`.
/// Spawning a timer (instead of releasing inline) ensures the duplicate
/// menu event — which arrives within microseconds of the first — still sees
/// the guard as claimed and is dropped.
const RELEASE_AFTER_MS: u64 = 400;
pub fn release_action() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(RELEASE_AFTER_MS));
        WS_ACTION_IN_FLIGHT.store(false, Ordering::SeqCst);
    });
}

#[derive(Debug, Clone, Serialize)]
pub struct DesktopInfo {
    pub id: u32,
    pub label: String,
}

fn info_from_desktop(d: winvd::Desktop, idx: u32) -> DesktopInfo {
    let label = d.get_name().unwrap_or_else(|_| format!("{}", idx + 1));
    DesktopInfo { id: idx, label }
}

#[tauri::command]
pub fn get_workspaces() -> Vec<DesktopInfo> {
    match winvd::get_desktops() {
        Ok(desktops) => {
            let infos: Vec<DesktopInfo> = desktops.iter().enumerate()
                .map(|(i, d)| info_from_desktop(*d, i as u32)).collect();
            FALLBACK_COUNT.store(infos.len() as u32, Ordering::Relaxed);
            infos
        }
        Err(e) => {
            eprintln!("[zenith:ws] get_desktops failed: {e:?}");
            let n = FALLBACK_COUNT.load(Ordering::Relaxed).max(3);
            (0..n).map(|i| DesktopInfo { id: i, label: format!("{}", i + 1) }).collect()
        }
    }
}

#[tauri::command]
pub fn get_active_workspace() -> u32 {
    let idx = match winvd::get_current_desktop() {
        Ok(d) => d.get_index().unwrap_or_else(|e| {
            eprintln!("[zenith:ws] get_index: {e:?}");
            FALLBACK_ACTIVE.load(Ordering::Relaxed)
        }),
        Err(e) => {
            eprintln!("[zenith:ws] get_current_desktop failed: {e:?}");
            FALLBACK_ACTIVE.load(Ordering::Relaxed)
        }
    };
    FALLBACK_ACTIVE.store(idx, Ordering::Relaxed);
    idx
}

#[tauri::command]
pub fn switch_workspace(id: u32) -> Result<(), String> {
    eprintln!("[zenith:ws] switch_workspace id={}", id);
    winvd::switch_desktop(id).map_err(|e| format!("switch_desktop failed: {e:?}"))?;
    FALLBACK_ACTIVE.store(id, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn create_desktop() -> Result<(), String> {
    winvd::create_desktop().map_err(|e| format!("create_desktop failed: {e:?}"))?;
    Ok(())
}

#[tauri::command]
pub fn delete_desktop(id: u32) -> Result<(), String> {
    let fallback_id: u32 = if id == 0 { 1 } else { 0 };
    winvd::remove_desktop(id, fallback_id).map_err(|e| format!("remove_desktop failed: {e:?}"))?;
    Ok(())
}

#[tauri::command]
pub fn rename_desktop(id: u32, name: String) -> Result<(), String> {
    let d = winvd::get_desktop(id);
    d.set_name(&name).map_err(|e| format!("set_name failed: {e:?}"))?;
    Ok(())
}

#[tauri::command]
pub fn move_window_to_desktop(id: u32) -> Result<(), String> {
    let hwnd = get_winvd_hwnd();
    if skip_hwnd(hwnd) { return Err("no foreground window".into()); }
    eprintln!("[zenith:ws] move_window_to_desktop id={} hwnd={:p}", id, hwnd.0);
    let mut pid: u32 = 0;
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(Win61Hwnd(hwnd.0), Some(&mut pid as *mut u32)); }
    let me = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
    eprintln!("[zenith:ws] move_window_to_desktop: target_pid={} me={}", pid, me);
    winvd::move_window_to_desktop(id, &hwnd).map_err(|e| format!("move_window failed: {e:?}"))?;
    Ok(())
}

#[tauri::command]
pub fn toggle_pin_window() -> Result<bool, String> {
    let hwnd = get_winvd_hwnd();
    if skip_hwnd(hwnd) { return Err("no foreground window".into()); }
    let is_pinned = winvd::is_pinned_window(hwnd).map_err(|e| format!("is_pinned_window failed: {e:?}"))?;
    if is_pinned {
        winvd::unpin_window(hwnd).map_err(|e| format!("unpin_window failed: {e:?}"))?;
        Ok(false)
    } else {
        winvd::pin_window(hwnd).map_err(|e| format!("pin_window failed: {e:?}"))?;
        Ok(true)
    }
}

pub fn pin_state() -> bool {
    let hwnd = get_winvd_hwnd();
    if skip_hwnd(hwnd) { return false; }
    winvd::is_pinned_window(hwnd).unwrap_or(false)
}

pub fn setup_events(app_handle: tauri::AppHandle) -> Result<winvd::DesktopEventThread, winvd::Error> {
    let (tx, rx) = mpsc::channel::<winvd::DesktopEvent>();
    let _handle = winvd::listen_desktop_events(tx)?;
    eprintln!("[zenith:ws] event listener registered");

    std::thread::spawn(move || {
        // Debounce: coalesce events that arrive within this window into a
        // single emit. `winvd` can deliver overlapping notifications for one
        // user action (e.g. create fires DesktopCreated + DesktopChanged),
        // and the COM sink may also repeat events. We deduplicate by tracking
        // the last emitted (event-kind, desktop-index) pair plus the time.
        let mut last_emit: Option<(u8, u32)> = None;
        let mut last_emit_time = std::time::Instant::now();
        const DEDUP_WINDOW: std::time::Duration = std::time::Duration::from_millis(150);

        while let Ok(event) = rx.recv() {
            let (kind, idx) = match event {
                winvd::DesktopEvent::DesktopChanged { new, .. } => {
                    (0u8, new.get_index().unwrap_or(0))
                }
                winvd::DesktopEvent::DesktopNameChanged(d, _) => {
                    (1u8, d.get_index().unwrap_or(0))
                }
                winvd::DesktopEvent::DesktopCreated(_) => {
                    (2u8, match winvd::get_current_desktop() {
                        Ok(d) => d.get_index().unwrap_or(0),
                        Err(_) => 0,
                    })
                }
                winvd::DesktopEvent::DesktopDestroyed { .. } => {
                    (3u8, match winvd::get_current_desktop() {
                        Ok(d) => d.get_index().unwrap_or(0),
                        Err(_) => 0,
                    })
                }
                _ => continue,
            };

            let now = std::time::Instant::now();
            let is_dup = last_emit == Some((kind, idx)) && now.duration_since(last_emit_time) < DEDUP_WINDOW;
            if is_dup {
                continue;
            }
            last_emit = Some((kind, idx));
            last_emit_time = now;

            let _ = app_handle.emit(crate::shared::EVENT_WORKSPACE_CHANGED, idx);
        }
    });
    Ok(_handle)
}