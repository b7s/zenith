//! Explorer-restart detection for the AppBar.
//!
//! When `explorer.exe` crashes and restarts, it broadcasts the registered
//! window message `"TaskbarCreated"`. Any app that registered an AppBar
//! (`ABM_NEW`) must re-register it, because the new explorer instance has no
//! knowledge of bar windows that existed before the crash. After the restart
//! the work-area reservation is gone and maximized windows cover the bar.
//!
//! We listen for this message on a dedicated thread that runs a message-only
//! window. When `TaskbarCreated` arrives we emit `zenith:appbar-restore`
//! (consumed in `lib.rs`, which calls `register_appbar` again).

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, HMODULE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterWindowMessageW,
    RegisterClassExW, TranslateMessage, HWND_MESSAGE, MSG, WNDCLASSEXW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_QUIT,
};

use tauri::{AppHandle, Emitter};

const EVENT_APPBAR_RESTORE: &str = "zenith:appbar-restore";

/// Stop event used only at process exit (best-effort).
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        // The TaskbarCreated message id is queried per-thread (registered msgs
        // are per-session, the value is the same process-wide, but we cache it
        // lazily in a thread-local to avoid re-registering each call).
        thread_local! {
            static TASKBAR_MSG: std::cell::Cell<u32> = std::cell::Cell::new(0);
        }
        let tb = TASKBAR_MSG.with(|c| {
            let v = c.get();
            if v != 0 { return v; }
            let mut name: [u16; 14] = [0; 14];
            let bytes = "TaskbarCreated\0".encode_utf16().collect::<Vec<u16>>();
            for (i, b) in bytes.iter().take(name.len()).enumerate() { name[i] = *b; }
            let id = RegisterWindowMessageW(PCWSTR(name.as_ptr()));
            c.set(id);
            id
        });

        if msg == tb && tb != 0 {
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit(EVENT_APPBAR_RESTORE, ());
                eprintln!("[zenith:appbar] TaskbarCreated received → emitting restore");
            }
            return LRESULT(0);
        }

        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

use windows::core::PCWSTR;

/// Spawn the explorer-restart watcher thread. The thread creates a
/// message-only window (hwnd = HWND_MESSAGE) and pumps messages until
/// WM_QUIT. When explorer restarts, it broadcasts TaskbarCreated and our
/// wnd_proc fires `zenith:appbar-restore`.
pub fn install(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
    std::thread::spawn(|| unsafe {
        let hinst: HMODULE = GetModuleHandleW(None).unwrap_or_default();
        let mut class_name: [u16; 20] = [0; 20];
        let bytes = "ZenithAppBarWatch\0".encode_utf16().collect::<Vec<u16>>();
        for (i, b) in bytes.iter().take(class_name.len()).enumerate() { class_name[i] = *b; }

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            eprintln!("[zenith:appbar] RegisterClassExW failed: {}", std::io::Error::last_os_error());
            return;
        }

        // HWND_MESSAGE = a message-only window (no visual surface).
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR::null(),
            WINDOW_STYLE::default(),
            0, 0, 0, 0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst.into()),
            None,
        );
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(_) => {
                eprintln!("[zenith:appbar] CreateWindowExW failed: {}", std::io::Error::last_os_error());
                return;
            }
        };
        eprintln!("[zenith:appbar] watcher window installed (hwnd={:p})", hwnd.0);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
            if msg.message == WM_QUIT { break; }
        }
    });
}