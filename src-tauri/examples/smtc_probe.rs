//! Standalone SMTC probe — no Tauri, no app state.
//!
//! Run with (from src-tauri/):
//!     cargo run --example smtc_probe
//!
//! It tries three independent approaches to enumerate Windows'
//! SystemMediaTransportControls (SMTC) sessions for audio that is
//! currently playing in any app (browsers, Spotify, etc.) and prints:
//!   - GetCurrentSession() source AUMID / status / title / artist
//!   - GetSessions() full list
//!
//! This isolates whether Windows sees the audio. If every variant returns
//! "nothing playing" while audio IS playing, the issue is system-level
//! (no SMTC integration in that player). If at least one variant shows
//! it, we know the data is there and the difference is in how the bar
//! consumes it.

use std::time::{Duration, Instant};

use windows::{
    core::HSTRING,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as SessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Win32::Foundation::{HWND, LPARAM, WPARAM},
    Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED},
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED},
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
        MSG, WNDCLASSEXW,
    },
};

fn status_str(s: PlaybackStatus) -> &'static str {
    if s == PlaybackStatus::Playing {
        "Playing"
    } else if s == PlaybackStatus::Paused {
        "Paused"
    } else if s == PlaybackStatus::Stopped {
        "Stopped"
    } else if s == PlaybackStatus::Closed {
        "Closed"
    } else if s == PlaybackStatus::Opened {
        "Opened"
    } else if s == PlaybackStatus::Changing {
        "Changing"
    } else {
        "Unknown"
    }
}

// Hidden message-only window so the thread has a proper HWND for COM
// routing. We never read back from it; we just keep it alive.
fn create_pump_window() -> HWND {
    use windows::{
        core::w,
        Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE,
            },
        },
    };
    unsafe {
        let class = w!("SmtcProbePumpClass");
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(pump_proc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("SmtcProbePump"),
            WINDOW_STYLE(0),
            0, 0, 0, 0,
            None, None, None, None,
        ).unwrap_or_default()
    }
}

unsafe extern "system" fn pump_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, w, l)
}

// A pure GetMessageW message loop. Sleeps OOM uses this to receive any
// dispatched Windows messages while waiting for the async completion.
#[allow(dead_code)]
fn pump_messages_for(deadline: Instant) {
    unsafe {
        while Instant::now() < deadline {
            let mut msg: MSG = std::mem::zeroed();
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == -1 || ret.0 == 0 {
                break;
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn dump_session(prefix: &str, s: &Session) {
    let src = s
        .SourceAppUserModelId()
        .map(|h: HSTRING| h.to_string())
        .unwrap_or_default();
    let status = s
        .GetPlaybackInfo()
        .ok()
        .and_then(|pb| pb.PlaybackStatus().ok())
        .map(status_str)
        .unwrap_or("?");
    // Try media props (10s timeout — some browser sources are very slow).
    let props = match s.TryGetMediaPropertiesAsync() {
        Ok(op) => match wait_async_message(op, Duration::from_secs(10)) {
            Ok(p) => p,
            Err(_e) => return println!("{}  source={:?} status={:>8} (props timed out)", prefix, src, status),
        },
        Err(_e) => return println!("{}  source={:?} status={:>8} (TryGet err)", prefix, src, status),
    };
    let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
    let artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
    println!("{}  source={:?} status={:>8} title={:?} artist={:?}", prefix, src, status, title, artist);
}

fn wait_async_message<T: windows::core::RuntimeType + 'static>(
    op: windows_future::IAsyncOperation<T>,
    timeout: Duration,
) -> Result<T, String> {
    // Install Completed first, then pump GetMessageW.
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_future::{AsyncOperationCompletedHandler, AsyncStatus};

    static DONE: AtomicBool = AtomicBool::new(false);
    DONE.store(false, Ordering::SeqCst);

    let handler = AsyncOperationCompletedHandler::new(move |_op, _st: AsyncStatus| {
        DONE.store(true, Ordering::SeqCst);
        Ok(())
    });
    if let Err(e) = op.SetCompleted(&handler) {
        return Err(format!("SetCompleted: {e}"));
    }

    let deadline = Instant::now() + timeout;
    loop {
        if DONE.load(Ordering::SeqCst) {
            return op
                .GetResults()
                .map_err(|e| format!("results: {e}"));
        }
        if Instant::now() >= deadline {
            return Err("timeout".into());
        }
        // Pump one message via MsgWaitForMultipleObjects+PeekMessageW
        // (non-blocking 50ms slice) so we don't freeze the thread on
        // GetMessageW when nothing is arriving.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::MsgWaitForMultipleObjects(
                None,
                false,
                50,
                windows::Win32::UI::WindowsAndMessaging::QS_ALLINPUT,
            );
        }
        // Check for completed by status too (in case handler wasn't invoked).
        if let Ok(s) = op.Status() {
            if s == AsyncStatus::Completed {
                return op.GetResults().map_err(|e| format!("results: {e}"));
            }
            if s == AsyncStatus::Error {
                return Err(format!(
                    "async errored (hr 0x{:08x})",
                    op.ErrorCode().map(|c| c.0).unwrap_or(0)
                ));
            }
        }
    }
}

fn variant_sta_pump() {
    println!("\n=== variant 1: STA + RoInit(Single) + hidden window + MsgWait pump ===");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = RoInitialize(RO_INIT_SINGLETHREADED);
    }
    let _pump = create_pump_window();
    let op = match SessionManager::RequestAsync() {
        Ok(o) => o,
        Err(e) => {
            println!("RequestAsync construct err: {e}");
            return;
        }
    };
    let mgr = match wait_async_message(op, Duration::from_secs(20)) {
        Ok(m) => m,
        Err(e) => {
            println!("variant 1 FAILED: {e} (this is the same code path as the bar)");
            return;
        }
    };
    println!("variant 1 OK");
    match mgr.GetCurrentSession() {
        Ok(s) => dump_session("  current", &s),
        Err(e) => println!("  GetCurrentSession err: {e}"),
    }
    let n = mgr.GetSessions().ok().and_then(|v| v.Size().ok()).unwrap_or(0);
    println!("  GetSessions count = {}", n);
    for i in 0..n {
        if let Ok(s) = mgr.GetSessions().unwrap().GetAt(i) {
            dump_session(&format!("  [{}]", i), &s);
        }
    }
}

fn variant_mta_get() {
    println!("\n=== variant 2: MTA + RoInit(Multi) + op.get() ===");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let op = match SessionManager::RequestAsync() {
        Ok(o) => o,
        Err(e) => {
            println!("RequestAsync construct err: {e}");
            return;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(s) = op.Status() {
            if s == windows_future::AsyncStatus::Completed {
                break;
            }
            if s == windows_future::AsyncStatus::Error {
                println!("  async error: hr 0x{:08x}", op.ErrorCode().map(|c| c.0).unwrap_or(0));
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mgr = match op.GetResults() {
        Ok(m) => m,
        Err(e) => {
            println!("variant 2 FAILED: {e}");
            return;
        }
    };
    println!("variant 2 OK");
    match mgr.GetCurrentSession() {
        Ok(s) => dump_session("  current", &s),
        Err(e) => println!("  GetCurrentSession err: {e}"),
    }
    let n = mgr.GetSessions().ok().and_then(|v| v.Size().ok()).unwrap_or(0);
    println!("  GetSessions count = {}", n);
    for i in 0..n {
        if let Ok(s) = mgr.GetSessions().unwrap().GetAt(i) {
            dump_session(&format!("  [{}]", i), &s);
        }
    }
}

fn main() {
    println!("=== SMTC probe ===");
    println!("(Try playing media in a browser or Spotify first, then re-run.)");
    variant_sta_pump();
    variant_mta_get();
    println!("\n=== done ===");
}
