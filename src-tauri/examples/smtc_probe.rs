//! Standalone SMTC probe — a live diagnostic for the media widget's
//! "sometimes not recognize what is playing" bug.
//!
//! Run with (from `src-tauri/`):
//!     cargo run --example smtc_probe
//!
//! ## What it tells you
//!
//! The probe runs the **same** code path that the bar's poll thread
//! (`media::listen`) runs every 2 s — `zenith_lib::test_util::resolve_current`
//! → `zenith_lib::test_util::capture_session` — and prints every step,
//! so you can see exactly where the widget loses the track:
//!
//! 1. **COM init** — whether `CoInitializeEx(APARTMENTTHREADED)` succeeds
//!    on this thread. (If it returns `RPC_E_CHANGED_MODE`, SMTC will fail
//!    silently and emit nothing — see `ensure_com` in `media::commands.rs`.)
//! 2. **Session discovery** — what `SessionManager::RequestAsync` returns;
//!    the full `GetSessions()` list with the rank the bar gives each one
//!    (`Playing=4, Paused=3, Stopped=2, Opened/Changing=1, Closed=0`);
//!    and what `GetCurrentSession()` (OS "current") reports.
//! 3. **Selection** — which session the bar's real `resolve_current`
//!    function picks, including the rank vectors fed into the pure
//!    `pick_best_session` decision.
//! 4. **Capture** — running the real `capture_session` on the chosen
//!    session and printing the resulting `MediaInfo` (title / artist /
//!    position / status / source / whether it has a thumbnail).
//!
//! ## How to read it
//!
//! - If the probe prints `0 sessions` while audio IS playing → the player
//!   has not registered with Windows SMTC. The bar can't see it. This is
//!   system-level — out of Zenith's control. (Common with raw WASAPI
//!   players; browsers and Spotify should always register.)
//! - If a session is listed with `rank=0` and status `Closed`/`Unknown`
//!   while audio IS playing → SMTC saw it once and now it's stale. The
//!   bar will skip it. Try restarting the player.
//! - If the chosen session shows `rank=2/3/4` BUT `capture_session` returns
//!   `title=""` → `TryGetMediaPropertiesAsync` failed/timed out. The bug
//!   is the async wait, not discovery. (The probe prints the elapsed ms.)
//! - If everything prints correctly but the **widget** still shows "No
//!   media" → the bug is between the cache and the frontend, not in
//!   `resolve_current` at all. Check the bar's IPC path / cache.
//!
//! Play media in Chrome/Edge/Spotify/VLC first, then run this binary
//! repeatedly — the bug is intermittent, so several runs are needed.

use std::time::{Duration, Instant};

use windows::{
    core::HSTRING,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Win32::Foundation::{HWND, LPARAM, WPARAM},
    Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED},
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, MsgWaitForMultipleObjects,
        PeekMessageW, RegisterClassExW, TranslateMessage, MSG, QS_ALLINPUT, WNDCLASSEXW,
        WINDOW_EX_STYLE, WINDOW_STYLE,
    },
};

// The REAL bar code path. Anything below reuses what the bar runs in
// production — no parallel implementation, so this probe cannot drift.
use zenith_lib::test_util::{capture_session, resolve_current};

fn main() {
    println!("=== SMTC probe (live) ===");
    println!();
    println!("INSTRUCTIONS:");
    println!("  1. Start media playing in your usual app (browser/Spotify/VLC).");
    println!("  2. Run this probe. Repeat 4-5x — bug is intermittent.");
    println!("  3. Read the verdict line at the end of each run.");
    println!();
    println!("== step 1: COM init ==");
    // Match `media::commands::ensure_com` exactly.
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // CoInitializeEx returns S_OK / S_FALSE on success; RPC_E_CHANGED_MODE
        // means the thread is already MTA and SMTC won't work here.
        let code = hr.0;
        if hr.is_ok() {
            println!("  CoInitializeEx(APARTMENTTHREADED) OK (hr=0x{code:08X})");
        } else {
            println!("  CoInitializeEx FAILED hr=0x{code:08X} — SMTC will be unable to init on this thread.");
            println!("  This is fatal: media::listen calls CoInitializeEx the same way.");
            return;
        }
    }

    // Hidden pump window — required by `wait_async` for STA async routing.
    let _pump = create_pump_window();

    println!();
    println!("== step 2: SessionManager + discovery ==");

    // Mirror `resolve_current`'s own `RequestAsync` path — but call it
    // directly here so we can PRINT the raw session list before the bar's
    // ranking decision is applied.
    let mgr: SessionManager = match SessionManager::RequestAsync() {
        Ok(op) => match wait_async_msg(op, Duration::from_secs(2)) {
            Ok(m) => {
                println!("  SessionManager::RequestAsync OK");
                m
            }
            Err(e) => {
                println!("  SessionManager::RequestAsync FAILED: {e}");
                println!();
                println!("VERDICT: SMTC broker is wedged (RequestAsync would not complete in 2s).");
                println!("         On this machine empirically only a full OS reboot clears it;");
                println!("         restarting RuntimeBroker.exe does NOT help. So the bar can");
                println!("         never see sessions between reboots — out of Zenith's reach.");
                println!("         What Zenith CAN do (and now does): cap RequestAsync at 2s,");
                println!("         log \"SMTC broker is wedged; reboot Windows\" per poll, and");
                println!("         stop burning 15s per cycle while waiting for a session.");
                return;
            }
        },
        Err(e) => {
            println!("  SessionManager::RequestAsync construct err: {e}");
            return;
        }
    };

    let os_current: Option<Session> = match mgr.GetCurrentSession() {
        Ok(s) => {
            println!("  GetCurrentSession OK");
            Some(s)
        }
        Err(e) => {
            println!("  GetCurrentSession err: {e} (treated as None by the bar)");
            None
        }
    };

    let sessions_vec = match mgr.GetSessions() {
        Ok(v) => v,
        Err(e) => {
            println!("  GetSessions err: {e}");
            return;
        }
    };
    let count = sessions_vec.Size().unwrap_or(0);
    println!("  GetSessions count = {count}");

    if count == 0 && os_current.is_none() {
        println!();
        println!("VERDICT: 0 sessions AND no OS current. Windows SMTC has nothing.");
        println!("         If audio IS playing, the player doesn't register with SMTC");
        println!("         — out of Zenith's reach. Try a different player.");
        return;
    }

    println!();
    println!("== step 3: per-session snapshot ==");
    let mut candidates: Vec<(u8, bool, usize)> = Vec::new();
    for i in 0..count {
        let s = match sessions_vec.GetAt(i) {
            Ok(s) => s,
            Err(e) => {
                println!("  [{}] GetAt err: {e}", i);
                continue;
            }
        };
        let src = s
            .SourceAppUserModelId()
            .map(|h: HSTRING| h.to_string())
            .unwrap_or_default();
        let (status_str, status_enum) = playback(&s);
        let rank = rank_from(status_enum);
        let is_current = match &os_current {
            Some(c) => eq_sessions(c, &s),
            None => false,
        };
        candidates.push((rank, is_current, i as usize));
        println!(
            "  [{i}] rank={rank} status={status_str:>10} current={} source={src}",
            if is_current { "Y" } else { "n" }
        );
    }
    // Treat the OS "current" session as an extra candidate even if it's
    // not in GetSessions() (Windows does this sometimes — GetCurrentSession
    // succeeds but GetSessions is empty).
    if let Some(ref sc) = os_current {
        if count == 0
            || (0..count)
                .map(|i| sessions_vec.GetAt(i).ok())
                .all(|maybe_s| maybe_s.is_none_or(|s| !eq_sessions(sc, &s)))
        {
            let src = sc
                .SourceAppUserModelId()
                .map(|h: HSTRING| h.to_string())
                .unwrap_or_default();
            let (status_str, status_enum) = playback(sc);
            let rank = rank_from(status_enum);
            println!(
                "  [os] rank={rank} status={status_str:>10} current=Y source={src}   (not in GetSessions())"
            );
        }
    }

    println!();
    println!("== step 4: bar's resolve_current picks ==");
    let t0 = Instant::now();
    let picked = resolve_current();
    let elapsed = t0.elapsed();
    match &picked {
        Some(s) => {
            let src = s
                .SourceAppUserModelId()
                .map(|h: HSTRING| h.to_string())
                .unwrap_or_default();
            let (status_str, _) = playback(s);
            println!(
                "  resolve_current OK in {:.1} ms — picked source={src} status={status_str}",
                elapsed.as_secs_f64() * 1000.0
            );
        }
        None => {
            println!("  resolve_current returned None in {:.1} ms", elapsed.as_secs_f64() * 1000.0);
            println!();
            println!("VERDICT: resolve_current returned None — bar will show 'No media'.");
            if count == 0 {
                println!("         Cause: GetSessions() is empty AND GetCurrentSession is None.");
                println!("         — Windows SMTC has no sessions for this process.");
            } else {
                println!("         Cause: every session had rank 0 (Closed/Unknown). Either");
                println!("           nothing is registered, or all sessions are stale.");
            }
            return;
        }
    };

    println!();
    println!("== step 5: bar's capture_session reads the track ==");
    let picked_session = picked.as_ref().unwrap();
    let t1 = Instant::now();
    let captured = capture_session(picked_session);
    let cap_elapsed = t1.elapsed();
    match &captured {
        Some(info) => {
            println!(
                "  capture_session OK in {:.1} ms",
                cap_elapsed.as_secs_f64() * 1000.0
            );
            println!("    title={:?} artist={:?} album={:?}", info.title, info.artist, info.album);
            println!(
                "    status={} position_ms={} duration_ms={} rate={} source={}",
                info.status, info.position_ms, info.duration_ms, info.rate, info.source
            );
            println!("    thumbnail present = {}", info.thumbnail.is_some());
            println!();
            if info.title.is_empty() {
                println!("VERDICT: session selected, but title is EMPTY.");
                println!("         `TryGetMediaPropertiesAsync` returned an empty title prop.");
                println!("         — `MediaInfo` IS produced, but the widget shows 'No media'");
                println!("           because widget.js treats empty title as 'No media' (line");
                println!("           widget.js:122). See widget.js · render() fallback.");
            } else {
                println!("VERDICT: BAR PATH WORKS end-to-end on this run.");
                println!("         resolve_current → capture_session returned a real title.");
                println!("         If the widget still shows 'No media', the bug is downstream");
                println!("         — cache/event/IPC layer, NOT discovery/capture. Check:");
                println!("           • media::listen::spawn is running (look for mlog lines)");
                println!("           • zenith:media-changed event reaches the bar");
                println!("           • cache_get() returns Some, not None (commands.rs:66)");
            }
        }
        None => {
            println!(
                "  capture_session returned None in {:.1} ms",
                cap_elapsed.as_secs_f64() * 1000.0
            );
            println!();
            println!("VERDICT: session selected but capture_session returned None.");
            println!("         This should be impossible — capture_session only returns");
            println!("         None on a panic path. Inspect its body in commands.rs:342.");
        }
    }
    println!();
    println!("=== done ===");
}

fn playback(s: &Session) -> (&'static str, PlaybackStatus) {
    match s.GetPlaybackInfo().ok().and_then(|pb| pb.PlaybackStatus().ok()) {
        Some(st) => (status_str(st), st),
        None => ("?", PlaybackStatus::Closed),
    }
}

fn status_str(s: PlaybackStatus) -> &'static str {
    if s == PlaybackStatus::Playing { "Playing" }
    else if s == PlaybackStatus::Paused { "Paused" }
    else if s == PlaybackStatus::Stopped { "Stopped" }
    else if s == PlaybackStatus::Closed { "Closed" }
    else if s == PlaybackStatus::Opened { "Opened" }
    else if s == PlaybackStatus::Changing { "Changing" }
    else { "Unknown" }
}

// Mirror of `media::commands::rank_from` — exact same table so the
// probe reports the same ranks the bar uses.
fn rank_from(status: PlaybackStatus) -> u8 {
    match status {
        PlaybackStatus::Playing => 4,
        PlaybackStatus::Paused => 3,
        PlaybackStatus::Stopped => 2,
        PlaybackStatus::Opened => 1,
        PlaybackStatus::Changing => 1,
        PlaybackStatus::Closed => 0,
        _ => 0,
    }
}

// Equality via SourceAppUserModelId — runtime `==` on the COM Session
// interface is identity-based and works across references inside the
// same manager.
fn eq_sessions(a: &Session, b: &Session) -> bool {
    a == b
}

// ---- hidden pump window + async wait -----------------------------------
//
// Same approach as `media::commands::ensure_pump_window` + `wait_async`:
// STA + MsgWaitForMultipleObjects + PeekMessageW drain. Re-implemented
// here so the probe is self-contained and doesn't depend on private
// helpers of `media::commands` (only the public `resolve_current` /
// `capture_session` and `MediaInfo` are pulled from the lib).

fn create_pump_window() -> HWND {
    use windows::{core::w, Win32::System::LibraryLoader::GetModuleHandleW};
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
        )
        .unwrap_or_default()
    }
}

unsafe extern "system" fn pump_proc(
    hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, w, l)
}

fn wait_async_msg<T: windows::core::RuntimeType + 'static>(
    op: windows_future::IAsyncOperation<T>,
    timeout: Duration,
) -> Result<T, String> {
    use windows_future::{AsyncOperationCompletedHandler, AsyncStatus};
    use windows::Win32::System::Threading::{CreateEventW, SetEvent};
    use windows::Win32::UI::WindowsAndMessaging::PM_REMOVE;

    unsafe {
        let event = CreateEventW(None, false, false, None).map_err(|e| format!("CreateEventW: {e}"))?;
        struct Guard(windows::Win32::Foundation::HANDLE);
        impl Drop for Guard {
            fn drop(&mut self) { unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.0); } }
        }
        let _g = Guard(event);
        let evt_addr: usize = event.0 as usize;

        let handler = AsyncOperationCompletedHandler::new(
            move |_s, _st: AsyncStatus| {
                let raw = windows::Win32::Foundation::HANDLE(evt_addr as *mut core::ffi::c_void);
                let _ = SetEvent(raw);
                Ok(())
            },
        );
        op.SetCompleted(&handler).map_err(|e| format!("SetCompleted: {e}"))?;
        let _keep = handler;

        let deadline = Instant::now() + timeout;
        loop {
            // drain messages — required so the STA can receive the completion
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let wait_ms = u32::min(100, deadline.saturating_duration_since(Instant::now()).as_millis() as u32);
            let res = MsgWaitForMultipleObjects(Some(&[event]), false, wait_ms, QS_ALLINPUT);
            if res == windows::Win32::Foundation::WAIT_EVENT(0) { break; }
            if Instant::now() >= deadline {
                return Err("async timeout".into());
            }
            if let Ok(s) = op.Status() {
                match s {
                    AsyncStatus::Completed => break,
                    AsyncStatus::Error => {
                        let code = op.ErrorCode().map(|c| c.0).unwrap_or(0);
                        return Err(format!("async errored (hr 0x{code:08X})"));
                    }
                    AsyncStatus::Canceled => return Err("async cancelled".into()),
                    _ => {}
                }
            }
        }
        op.GetResults().map_err(|e| format!("results: {e}"))
    }
}
