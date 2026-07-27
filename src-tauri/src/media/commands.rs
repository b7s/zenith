use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use windows::{
    core::HSTRING,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::DataReader,
    Win32::Foundation::{HWND, LPARAM, WPARAM},
    Win32::System::Com::{
        CoInitializeEx, COINIT_APARTMENTTHREADED,
    },
    Win32::System::Threading::{
        CreateEventW, SetEvent,
    },
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, MsgWaitForMultipleObjects,
        PeekMessageW, RegisterClassExW, TranslateMessage,
        MSG, QS_ALLINPUT, WNDCLASSEXW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_QUIT,
    },
};
use windows_future::{AsyncStatus, IAsyncOperation};

use super::MediaInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSnapshot {
    pub available: bool,
    pub info: Option<MediaInfo>,
}

/// Diagnostic log to stderr. Always on, prefixed `[media]` so the user can
/// filter the terminal. Kept off `log.rs` (file logger) on purpose — the
/// poll thread is too chatty and would dwarf the per-window logs.
macro_rules! mlog {
    ($($arg:tt)*) => {{
        eprintln!("[media] {}", format_args!($($arg)*));
    }};
}

// ---- cached snapshot --------------------------------------------------------
//
// The poll thread refreshes this on every actual change so `get_media` can
// return the last-known state WITHOUT touching SMTC. SMTC calls are slow
// (`RequestAsync` + `TryGetMediaPropertiesAsync` round-trips into the app
// process) and must never run on the Tauri main thread (AGENTS.md §13.1 —
// blocking the IPC channel freezes the bar, the bug that prompted this
// rewrite). Transport commands still go async + spawn_blocking.
pub(crate) static CACHED: OnceLock<Mutex<Option<MediaSnapshot>>> = OnceLock::new();

fn cache_init() -> &'static Mutex<Option<MediaSnapshot>> {
    CACHED.get_or_init(|| Mutex::new(None))
}

/// Replace the cached snapshot. Called only from the poll thread (writer)
/// and from the async `capture_and_cache` path on a fresh fallback.
pub(crate) fn cache_set(snap: Option<MediaSnapshot>) {
    if let Ok(mut g) = cache_init().lock() {
        *g = snap;
    }
}

pub(crate) fn cache_get() -> Option<MediaSnapshot> {
    cache_init().lock().ok().and_then(|g| g.clone())
}

// ---- COM --------------------------------------------------------------------

fn ensure_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// Per-thread hidden window used to pump WinRT-completion callbacks. Built
/// once per worker thread (the `OnceLock` is keyed to the thread via
/// thread-local storage). The window's only job is to own a message
/// queue — we pump it with `PeekMessageW` while waiting for an async
/// completion event to fire.
fn ensure_pump_window() -> HWND {
    thread_local! {
        static THREAD_HWND: std::cell::RefCell<Option<HWND>> = const { std::cell::RefCell::new(None) };
    }
    THREAD_HWND.with(|c| {
        let mut slot = c.borrow_mut();
        if let Some(h) = *slot {
            return h;
        }
        let h = unsafe {
            use windows::core::w;
            let class_name = w!("MediaStaPumpClass");
            let hinst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: Default::default(),
                lpfnWndProc: Some(media_pump_proc),
                hInstance: hinst.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            let atom = RegisterClassExW(&wc);
            // Re-registration across shared apartment is fine; the atom
            // comes back as 0 with last error `ERROR_CLASS_ALREADY_EXISTS`
            // (1410) which we treat as success.
            if atom == 0 {
                let _last_err = windows::Win32::Foundation::GetLastError();
            }
            match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("MediaStaPump"),
                WINDOW_STYLE(0),
                0, 0, 0, 0,
                None, None, None, None,
            ) {
                Ok(h) => h,
                Err(_e) => HWND(std::ptr::null_mut()),
            }
        };
        *slot = Some(h);
        h
    })
}

unsafe extern "system" fn media_pump_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Rank a session by how "active" it is for now-playing purposes.
///
/// Used by `resolve_current` (live SMTC scan) and mirrors the pure
/// `pick_best_session` tie-break contract. Higher = better.
///
/// | status      | rank |
/// |-------------|------|
/// | Playing     | 4    |
/// | Paused      | 3    | (a loaded track is still "now playing")
/// | Stopped     | 2    | (only counts — see note below)
/// | Opened      | 1    | (browser just loaded media; not invisible)
/// | Changing    | 1    |
/// | Closed      | 0    |
/// | Unknown     | 0    |
///
/// `Stopped` previously returned 0, which made the scanner skip browsers
/// that had just loaded a page (reporting `Opened`/`Changing`) AND any
/// session that had *finished* a track but still held the SMTC slot. The
/// fallback at the end of `resolve_current` then returned that very
/// stopped session, but `capture_session` reads an empty title and the
/// widget renders "No media" — the "sometimes not recognize" symptom.
///
/// Note: we do NOT have the title here (rank is status-only; reading
/// `MediaProperties` is a slow async and would defeat the cache). The
/// tie-break in `pick_best_session` covers the "stopped session with
/// nothing loaded" case by preferring a live current session of equal
/// rank.
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

/// Pure selection: given one (rank, is_current) per candidate session,
/// return the index of the best one. Tie-breaks:
///   1. higher rank wins;
///   2. on equal rank, the OS "current" session wins (it is the one the
///      user most recently interacted with);
///   3. otherwise the lowest index (stable, deterministic).
///
/// This is the single decision procedure for "which session is now
/// playing" — extracted from `resolve_current` specifically so it can be
/// unit tested without a live SMTC. See `media::commands::tests`.
fn pick_best_session(candidates: &[(u8, bool)]) -> Option<usize> {
    let mut best: Option<(usize, u8, bool)> = None;
    for (i, &(rank, is_current)) in candidates.iter().enumerate() {
        if rank == 0 {
            continue;
        }
        match best {
            None => best = Some((i, rank, is_current)),
            Some((_, br, bc)) => {
                if rank > br || (rank == br && is_current && !bc) {
                    best = Some((i, rank, is_current));
                }
            }
        }
    }
    best.map(|(i, _, _)| i)
}

fn status_string(status: PlaybackStatus) -> &'static str {
    if status == PlaybackStatus::Playing { "playing" }
    else if status == PlaybackStatus::Paused { "paused" }
    else if status == PlaybackStatus::Stopped { "stopped" }
    else if status == PlaybackStatus::Closed { "closed" }
    else if status == PlaybackStatus::Opened { "opened" }
    else if status == PlaybackStatus::Changing { "changing" }
    else { "unknown" }
}

/// Block-wait on a WinRT `IAsyncOperation<T>` by hooking the Completed event
/// + pumping COM messages on a hidden window so the async state machine can
///   deliver its callback to this apartment.
///
/// Default timeout: 15 s. Sources like Edge Web Media Player can take
/// several seconds for their first `OpenReadAsync`.
pub(crate) fn wait_async<T: windows::core::RuntimeType + 'static>(
    op: IAsyncOperation<T>,
) -> Result<T, String> {
    wait_async_inner(op, std::time::Duration::from_secs(15))
}

/// Same as `wait_async` but with a custom timeout. Used for thumbnail reads
/// that must not block the whole capture cycle.
fn wait_async_inner<T: windows::core::RuntimeType + 'static>(
    op: IAsyncOperation<T>,
    timeout: std::time::Duration,
) -> Result<T, String> {
    unsafe fn drain() {
        loop {
            let mut msg: MSG = std::mem::zeroed();
            let got = PeekMessageW(
                &mut msg,
                None,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
            );
            if !got.as_bool() {
                break;
            }
            if msg.message == WM_QUIT {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    use std::time::Instant;

    unsafe {
        ensure_com();
        // Pump window required even with MsgWaitForMultipleObjects — some
        // COM callbacks need a window handle for message dispatch routing.
        let _pump_hwnd = ensure_pump_window();
        let event = match CreateEventW(None, false, false, None) {
            Ok(e) => e,
            Err(e) => return Err(format!("CreateEventW: {e}")),
        };
        struct EventGuard(windows::Win32::Foundation::HANDLE);
        impl Drop for EventGuard {
            fn drop(&mut self) {
                unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.0); }
            }
        }
        let _evt_guard = EventGuard(event);

        let evt_addr: usize = event.0 as usize;
        let handler = windows_future::AsyncOperationCompletedHandler::new(
            move |_sender: windows::core::Ref<'_, IAsyncOperation<T>>, _status: AsyncStatus| {
                let raw = windows::Win32::Foundation::HANDLE(evt_addr as *mut core::ffi::c_void);
                let _ = SetEvent(raw);
                windows::core::Result::Ok(())
            },
        );
        if let Err(e) = op.SetCompleted(&handler) {
            return Err(format!("SetCompleted err: {e}"));
        }
        let _keep_handler = handler;

        let deadline = Instant::now() + timeout;
        loop {
            drain();
            let wait_ms = u32::min(
                100,
                deadline.saturating_duration_since(Instant::now()).as_millis() as u32,
            );
            // MsgWaitForMultipleObjects waits for BOTH the event AND incoming
            // Windows messages. COM async callbacks are delivered via messages
            // in an STA — a plain event-wait can miss them because the thread
            // must be in a message-pumping state for the callback to fire.
            let handles = [event];
            let res = MsgWaitForMultipleObjects(
                Some(&handles),
                false,
                wait_ms,
                QS_ALLINPUT,
            );
            if res == windows::Win32::Foundation::WAIT_EVENT(0) {
                // WAIT_OBJECT_0 — event signaled
                break;
            }
            if res == windows::Win32::Foundation::WAIT_EVENT(1) {
                // WAIT_OBJECT_0 + 1 — message available; next drain() will process
                continue;
            }
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
        drain();
    }

    match op.Status() {
        Ok(AsyncStatus::Completed) => op.GetResults().map_err(|e| format!("async results: {e}")),
        Ok(AsyncStatus::Error) => {
            let code = op.ErrorCode().map(|c| c.0).unwrap_or(0);
            Err(format!("async errored (hr 0x{code:08X})"))
        }
        Ok(AsyncStatus::Canceled) => Err("async cancelled".into()),
        Ok(other) => Err(format!("async in state {:?}", other)),
        Err(e) => Err(format!("async status: {e}")),
    }
}

/// Synchronously capture a session snapshot. Runs on a worker thread,
/// NEVER the Tauri main thread (see §13.1).
pub fn capture_session(session: &Session) -> Option<MediaInfo> {
    let source = session
        .SourceAppUserModelId()
        .map(|h: HSTRING| h.to_string())
        .unwrap_or_default();

    let timeline = session.GetTimelineProperties().ok();
    let (position_ms, duration_ms) = match &timeline {
        Some(t) => (
            t.Position().map(|ts| ts.Duration / 10_000).unwrap_or(0),
            t.EndTime().map(|ts| ts.Duration / 10_000).unwrap_or(0),
        ),
        None => (0, 0),
    };

    let (status_str, rate) = session
        .GetPlaybackInfo()
        .ok()
        .map(|pb| {
            let status = pb
                .PlaybackStatus()
                .map(status_string)
                .unwrap_or("unknown")
                .to_string();
            let rate = pb
                .PlaybackRate()
                .ok()
                .and_then(|r| r.Value().ok())
                .unwrap_or(1.0);
            (status, rate)
        })
        .unwrap_or_else(|| ("unknown".to_string(), 1.0));

    let media_props = session
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|op| wait_async(op).ok());
    let (title, artist, album, thumbnail) = match media_props {
        Some(m) => {
            let title = m.Title().map(|h| h.to_string()).unwrap_or_default();
            let artist = m.Artist().map(|h| h.to_string()).unwrap_or_default();
            let album = m.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
            let thumb = read_thumbnail(&m);
            (title, artist, album, thumb)
        }
        None => {
            (String::new(), String::new(), String::new(), None)
        }
    };

    Some(MediaInfo {
        title,
        artist,
        album,
        thumbnail,
        status: status_str,
        position_ms,
        duration_ms,
        rate,
        source,
    })
}

/// Open the thumbnail stream → DataReader → ReadBytes → base64 data URL.
/// Mime falls back to `image/jpeg`. Returns None on empty/oversized/unreadable.
///
/// Reading is iterative: `ReadBytes(&mut [u8])` is documented to fill the
/// buffer up to `value.len()` bytes from whatever is currently
/// unconsumed. After `LoadAsync(size)` the unconsumed length *should* equal
/// what we requested, but to remain safe across sources (Spotify, browsers,
/// browser players called from Edge etc.) we loop while there's still data
/// left in the DataReader. Pre-allocating `size` upfront means a single
/// reallocation in the common case.
fn read_thumbnail(props: &MediaProperties) -> Option<String> {
    let stream_ref = match props.Thumbnail() {
        Ok(r) => r,
        Err(e) => { mlog!("thumbnail: Thumbnail() err: {e}"); return None; }
    };
    let stream = match stream_ref.OpenReadAsync() {
        Ok(op) => match wait_async_inner(op, std::time::Duration::from_secs(2)) {
            Ok(s) => s,
            Err(e) => { mlog!("thumbnail: OpenReadAsync wait failed: {e}"); return None; }
        },
        Err(e) => { mlog!("thumbnail: OpenReadAsync err: {e}"); return None; }
    };
    let size = match stream.Size() {
        Ok(s) => s as u32,
        Err(e) => { mlog!("thumbnail: Size() err: {e}"); return None; }
    };
    if size == 0 {
        return None;
    }
    if size > 4 * 1024 * 1024 {
        return None;
    }
    let reader = match DataReader::CreateDataReader(&stream) {
        Ok(r) => r,
        Err(e) => { mlog!("thumbnail: CreateDataReader err: {e}"); return None; }
    };
    if let Err(_e) = wait_async_inner(match reader.LoadAsync(size) {
        Ok(op) => op,
        Err(e) => { mlog!("thumbnail: LoadAsync call err: {e}"); return None; }
    }, std::time::Duration::from_secs(2)) {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let mut total: usize = 0;
    let first = reader.UnconsumedBufferLength().ok().unwrap_or(size);
    if first > 0 {
        let n = (first as usize).min(buf.len());
        if reader.ReadBytes(&mut buf[..n]).is_err() {
            return None;
        }
        total = n;
    }
    let mut iters = 0u32;
    while total < buf.len() && iters < 64 {
        iters += 1;
        let remain_buf = reader.UnconsumedBufferLength().ok().unwrap_or(0);
        if remain_buf == 0 { break; }
        let n = (remain_buf as usize).min(buf.len() - total);
        let off = total;
        let end = off + n;
        if reader.ReadBytes(&mut buf[off..end]).is_err() {
            break;
        }
        total = end;
    }
    if total == 0 {
        return None;
    }
    buf.truncate(total);
    let mime = match stream.ContentType() {
        Ok(h) if !h.is_empty() => h.to_string(),
        _ => "image/jpeg".to_string(),
    };
    Some(format!("data:{};base64,{}", mime, base64_encode(&buf)))
}

/// Minimal inline base64 encoder — avoids adding a crate dep just for one
/// thumbnail. Mirrors the standard alphabet.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for c in chunks.by_ref() {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(CHARS[((n >> 6) & 63) as usize] as char);
        out.push(CHARS[(n & 63) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push(CHARS[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Rank a session by how "active" it is for now-playing purposes.
/// 4 = playing, 3 = paused, 2 = stopped (still has a slot), 1 = opened/
/// changing (browser just loaded media), 0 = closed/unknown. See
/// `rank_from` for the rationale — `Stopped` is no longer 0, which is
/// what fixes the "sometimes not recognize" bug for browsers.
fn session_active_rank(s: &Session) -> u8 {
    s.GetPlaybackInfo()
        .ok()
        .and_then(|pb| pb.PlaybackStatus().ok())
        .map(rank_from)
        .unwrap_or(0)
}

fn session_is_active(s: &Session) -> bool {
    session_active_rank(s) > 0
}

/// Activate the SMTC session manager and return the best session to display.
///
/// Browsers (Chrome/Edge) register a `GlobalSystemMediaTransportControls`
/// session when playing media, but Windows does **not** always report their
/// session as the OS "current" one — `GetCurrentSession()` can return
/// `None` or a stale/closed session while a track is clearly playing. So we
/// first trust `GetCurrentSession()` *only* if it is actually active, then
/// fall back to scanning **all** sessions and pick the most active one via
/// the pure, tested `pick_best_session` (which prefers the OS "current"
/// session on ties so the user's last-used player wins).
///
/// Returns `None` when nothing has registered with SMTC. **Slow** — must
/// run on a worker thread, never the Tauri main thread.
pub fn resolve_current() -> Option<Session> {
    ensure_com();
    // `RequestAsync` normally completes in well under 500 ms. When the
    // SMTC broker (RuntimeBroker.exe / UIServer) is wedged, it NEVER
    // completes — confirmed live on the user's machine where the bar
    // wiget only works after a reboot. Capping this single call at 2 s
    // means a wedged poll wastes 2 s (not 15 s) per cycle, and the
    // diagnostic log tells the user the real cause instead of just
    // looking "No media" forever.
    let mgr: SessionManager = match wait_async_inner(
        SessionManager::RequestAsync().ok()?,
        std::time::Duration::from_secs(2),
    ) {
        Ok(m) => m,
        Err(e) => {
            mlog!("RequestAsync FAILED (2s cap): {e} — SMTC broker is wedged; reboot Windows to restore");
            return None;
        }
    };

    let os_current = mgr.GetCurrentSession().ok();

    // Fast path: the OS current session is genuinely active — use it.
    if let Some(ref s) = os_current {
        if session_is_active(s) {
            return Some(s.clone());
        }
    }

    // Scan every registered session and keep the best via the tested
    // `pick_best_session`. The OS "current" session is marked so it wins
    // ties (deterministic across boots even when several players are
    // paused/stopped with the same rank).
    if let Ok(sessions) = mgr.GetSessions() {
        let count = sessions.Size().unwrap_or(0);
        if count == 0 {
            // Single-session fast path: GetSessions() returned nothing but
            // GetCurrentSession might still hold one.
            return os_current;
        }
        let mut candidates: Vec<(u8, bool)> = Vec::with_capacity(count as usize);
        for i in 0..count {
            let s = match sessions.GetAt(i) {
                Ok(s) => s,
                Err(_) => {
                    candidates.push((0, false));
                    continue;
                }
            };
            let rank = session_active_rank(&s);
            let is_current = match &os_current {
                Some(c) => c == &s,
                None => false,
            };
            candidates.push((rank, is_current));
        }
        if let Some(idx) = pick_best_session(&candidates) {
            if let Ok(s) = sessions.GetAt(idx as u32) {
                return Some(s);
            }
        }
    }

    // Last resort: whatever the OS reports as current (even if not active).
    os_current
}

/// Run a transport command (`TryPlayAsync` etc.) on a worker thread. `f`
/// calls the SMTC method (returns the not-yet-awaited `IAsyncOperation`),
/// `run_bool` then waits on it via `wait_async` (worker-thread polling).
async fn run_transport<F>(label: &str, f: F) -> Result<bool, String>
where
    F: FnOnce(Session) -> windows::core::Result<IAsyncOperation<bool>> + Send + 'static,
{
    let _label = label.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _started = std::time::Instant::now();
        let Some(s) = resolve_current() else {
            return Err("no media session".into());
        };
        match run_bool(f(s)) {
            Ok(b) => {
                Ok(b)
            }
            Err(e) => {
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Wait for the bool result of a Try*Async; treat any async error as
/// "command rejected". Worker-thread safe.
fn run_bool(op: windows::core::Result<IAsyncOperation<bool>>) -> Result<bool, String> {
    let async_op = op.map_err(|e| e.to_string())?;
    wait_async(async_op).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_media() -> MediaSnapshot {
    // Fast path ONLY. The poll thread (`media::listen`) is the single writer
    // of the cache and keeps it fresh every ~2 s. We deliberately do NOT
    // fall back to `capture_fresh()` here: that path runs SMTC's
    // `RequestAsync` (15 s worst-case) and would block the IPC channel —
    // freezing the bar's first paint and every other window's `get_config`
    // round-trip (AGENTS.md §13.1). On cold start the cache is empty for at
    // most one poll cycle (~2 s); returning an "unavailable" snapshot for
    // that brief window is far better than a frozen bar.
    if let Some(snap) = cache_get() {
        return snap;
    }
    MediaSnapshot { available: false, info: None }
}

#[tauri::command]
pub async fn media_play() -> Result<bool, String> {
    run_transport("play", |s| s.TryPlayAsync()).await
}

#[tauri::command]
pub async fn media_pause() -> Result<bool, String> {
    run_transport("pause", |s| s.TryPauseAsync()).await
}

#[tauri::command]
pub async fn media_toggle_play_pause() -> Result<bool, String> {
    run_transport("toggle", |s| s.TryTogglePlayPauseAsync()).await
}

#[tauri::command]
pub async fn media_next() -> Result<bool, String> {
    run_transport("next", |s| s.TrySkipNextAsync()).await
}

#[tauri::command]
pub async fn media_previous() -> Result<bool, String> {
    run_transport("previous", |s| s.TrySkipPreviousAsync()).await
}

#[tauri::command]
pub async fn media_seek(position_ms: i64) -> Result<bool, String> {
    let ticks = position_ms.saturating_mul(10_000);
    run_transport("seek", move |s| s.TryChangePlaybackPositionAsync(ticks)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    };

    /// Reproduces the "sometimes not recognize what is playing" bug.
    ///
    /// Symptom: a browser (Chrome/Edge) that finished a track still holds
    /// an SMTC session reporting `Stopped`. The *previous* ranker gave
    /// `Stopped` a rank of 0, so the scanner skipped it; the scanner then
    /// also skipped every other rank-0 session, fell through to the
    /// `GetCurrentSession().ok()` fallback, picked that same stopped
    /// session, `capture_session` read an empty title, and the widget
    /// rendered "No media" — even though the player was right there.
    ///
    /// After the fix, `Stopped` carries rank 2, so it is selectable and
    /// `capture_session` can read its `MediaProperties` to surface any
    /// title that's still loaded.
    #[test]
    fn stopped_session_is_selectable_after_fix() {
        assert_eq!(rank_from(PlaybackStatus::Stopped), 2);
        // Pure selection picks the stopped session, not None.
        assert_eq!(pick_best_session(&[(2, true)]), Some(0));
    }

    #[test]
    fn playing_beats_paused_beats_stopped_beats_opened() {
        let cands = vec![
            (rank_from(PlaybackStatus::Opened), false),   // idx 0
            (rank_from(PlaybackStatus::Stopped), false), // idx 1
            (rank_from(PlaybackStatus::Paused), false),  // idx 2
            (rank_from(PlaybackStatus::Playing), false),  // idx 3
        ];
        assert_eq!(pick_best_session(&cands), Some(3));
    }

    /// Tie-break: on equal rank, the OS "current" session wins.
    /// This is what makes the result deterministic across boots when
    /// multiple players are paused at the same time.
    #[test]
    fn tie_break_prefers_current_session() {
        // Two paused sessions; idx 1 is the OS current.
        let cands = vec![
            (rank_from(PlaybackStatus::Paused), false), // idx 0
            (rank_from(PlaybackStatus::Paused), true),  // idx 1 (current)
        ];
        assert_eq!(pick_best_session(&cands), Some(1));

        // Reverse the order — current still wins regardless of position.
        let cands = vec![
            (rank_from(PlaybackStatus::Paused), true),  // idx 0 (current)
            (rank_from(PlaybackStatus::Paused), false), // idx 1
        ];
        assert_eq!(pick_best_session(&cands), Some(0));
    }

    /// A higher rank always wins, even over a "current" lower-rank session.
    /// (User paused Browser A and started Player B → B is now playing.)
    #[test]
    fn higher_rank_beats_current() {
        let cands = vec![
            (rank_from(PlaybackStatus::Paused), true),  // idx 0 — current, paused
            (rank_from(PlaybackStatus::Playing), false), // idx 1 — not current, playing
        ];
        assert_eq!(pick_best_session(&cands), Some(1));
    }

    /// All rank-0 (e.g. every session `Closed`) → None (no media).
    #[test]
    fn all_zero_returns_none() {
        let cands = vec![
            (rank_from(PlaybackStatus::Closed), false),
            (rank_from(PlaybackStatus::Closed), true),
            (0, false),
        ];
        assert_eq!(pick_best_session(&cands), None);
    }

    /// `Opened`/`Changing` (rank 1) is selectable — the browser just
    /// loaded a page; we should not show "No media" while it spins up.
    #[test]
    fn opened_and_changing_are_selectable() {
        assert_eq!(rank_from(PlaybackStatus::Opened), 1);
        assert_eq!(rank_from(PlaybackStatus::Changing), 1);
        assert_eq!(
            pick_best_session(&[(rank_from(PlaybackStatus::Opened), false)]),
            Some(0)
        );
    }

    /// Empty candidate list → None (defensive).
    #[test]
    fn empty_candidates_returns_none() {
        assert_eq!(pick_best_session(&[]), None);
    }

    /// Stability: lowest-index wins when rank and is_current both tie.
    #[test]
    fn lowest_index_on_full_tie() {
        let cands = vec![
            (rank_from(PlaybackStatus::Playing), false),
            (rank_from(PlaybackStatus::Playing), false),
            (rank_from(PlaybackStatus::Playing), false),
        ];
        assert_eq!(pick_best_session(&cands), Some(0));
    }
}
