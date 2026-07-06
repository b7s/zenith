use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, GetForegroundWindow, EVENT_SYSTEM_FOREGROUND,
    GetMessageW, MSG, DispatchMessageW,
};

const WINEVENT_OUTOFCONTEXT: u32 = 0x0004;

static LAST_REAL_FG: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// The last foreground window that did NOT belong to our own process.
pub fn last_real_foreground_ptr() -> *mut c_void {
    LAST_REAL_FG.load(Ordering::Relaxed)
}

/// Store the foreground HWND if it does not belong to our own process.
fn record_if_foreign(hwnd: HWND) {
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        if pid == GetCurrentProcessId() {
            return;
        }
        LAST_REAL_FG.store(hwnd.0, Ordering::Relaxed);
    }
}

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _idobject: i32,
    _idchild: i32,
    _thread: u32,
    _time: u32,
) {
    record_if_foreign(hwnd);
}

/// Install the foreground tracking hook on a dedicated thread that pumps
/// messages (required so `WINEVENT_OUTOFCONTEXT` callbacks get delivered).
/// Seeds the cache with the current foreground window so move/pin work
/// immediately, before any focus change occurs.
pub fn install() {
    // Seed: capture the current foreground window now (filtered by PID).
    unsafe { record_if_foreign(GetForegroundWindow()); }

    std::thread::spawn(|| unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if hook.0.is_null() {
            eprintln!("[zenith:ws] SetWinEventHook failed");
            return;
        }
        eprintln!("[zenith:ws] foreground hook installed");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    });
}
