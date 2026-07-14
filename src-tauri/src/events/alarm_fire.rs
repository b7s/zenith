//! Alarm-firing background thread.
//!
//! Runs every 30 seconds, scans enabled events for any whose next
//! occurrence falls within the current 30-second window. On a hit:
//!   * plays a Windows system sound (configurable via `widgets.config.alarms.sound_enabled`)
//!   * shows a small notification popup window
//!   * for one-shot items (`Recurrence::None`), disables the row so it won't
//!     fire again unless the user re-enables it
//!
//! Two kinds of rows fire this popup:
//!   - `kind = Alarm` — the user's stand-alone timed reminders.
//!   - `kind = Event` with `notify_on_start = true` — synced events from
//!     Google Calendar / Outlook (or a local event the user has flagged
//!     for notification). For these we record `last_notified_at` so the
//!     next 30 seconds don't refire the same row. Local all-day events
//!     skip the popup (they have no `time`); the alarms widget still
//!     surfaces them on the bar.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use super::model::{CalendarEvent, EventKind, Recurrence};
use super::repository;

const TICK: Duration = Duration::from_secs(30);
/// Don't re-fire the same alarm within this window even if it stays due
/// across tick boundaries (e.g. alarm popups stay open longer than the
/// re-check interval).
const DEDUP_WINDOW_SECS: i64 = 60;
/// Event-start notifications are pushed onto a one-shot queue with
/// `last_notified_at`. We treat 5+ minutes as "stale enough to re-notify"
/// so a missed tick (e.g. the bar was off when an event fired) still
/// alarms when the bar comes back up.
const EVENT_NOTIFY_REFIRE_SECS: i64 = 5 * 60;

/// In-memory dedup set — uses alarm IDs echoed with their most-recent
/// fire-time (epoch secs). Spills to file when the process exits isn't
/// needed because a fired one-shot alarm is disabled in the data.
static LAST_FIRED_AT: std::sync::Mutex<Option<HashSet<(String, i64)>>> =
    std::sync::Mutex::new(None);
static LAST_TICK_AT: AtomicI64 = AtomicI64::new(0);

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn alarms_app_enabled(app: &AppHandle) -> bool {
    crate::config::repository::get_or(
        "/widgets/config/alarms/sound_enabled",
        true,
    ) && app.get_webview_window("bar").is_some()
}

/// Spawn the alarm-firing thread. Safe to call from `lib.rs::setup`.
pub fn spawn(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(TICK);
        let _ = run_tick(&app);
    });
}

fn run_tick(app: &AppHandle) -> Result<(), String> {
    let now = now_secs();
    LAST_TICK_AT.store(now, Ordering::Relaxed);
    let events = repository::load();
    let mut to_disable: Vec<String> = Vec::new();
    let mut event_notified: Vec<(String, i64)> = Vec::new();

    for ev in &events {
        if !ev.enabled {
            continue;
        }

        // ---- One-shot Alarm rows (unchanged from the legacy path) ----
        if ev.kind == EventKind::Alarm && ev.time.is_some() {
            let Some(fire_at) = next_fire_secs(ev, now) else {
                continue;
            };
            let delta = fire_at - now;
            if !(0..=TICK.as_secs() as i64).contains(&delta) {
                continue;
            }
            if already_fired(&ev.id, fire_at) {
                continue;
            }
            record_fired(&ev.id, fire_at);
            fire_alarm(app, ev, fire_at);
            if matches!(ev.recurrence, Recurrence::None) {
                to_disable.push(ev.id.clone());
            }
            continue;
        }

        // ---- Event-start notifications (kind = Event) — synced or local
        // events with `notify_on_start: true` and a concrete start `time`.
        // All-day events (no `time`) never fire a popup (we have no
        // "0:00 sharp" trigger for them); the bar/alarms widget still
        // shows them on the day. Recurring events fire once per occurrence.
        if ev.kind != EventKind::Event || !ev.notify_on_start {
            continue;
        }
        if ev.time.is_none() {
            continue;
        }
        let Some(fire_at) = next_fire_secs(ev, now) else {
            continue;
        };
        // Skip if this row already fired within EVENT_NOTIFY_REFIRE_SECS
        // of `fire_at`. This guards against double-notifying across
        // multi-tick windows (a 1h meeting straddles two ticks).
        if !should_fire_event_notify(ev, fire_at, now) {
            continue;
        }

        fire_alarm(app, ev, fire_at);
        if matches!(ev.recurrence, Recurrence::None) {
            // One-shot event notifications: delete the row entirely (the
            // event has already happened, so it isn't useful on the
            // calendar). Recurring events keep firing each cycle.
            to_disable.push(ev.id.clone());
        } else {
            event_notified.push((ev.id.clone(), now));
        }
    }

    // One-shot rows (alarm OR event) that have fired: delete so the user
    // isn't repeatedly reminded of a past event.
    if !to_disable.is_empty() {
        for id in &to_disable {
            let _ = repository::delete_by_id(id);
        }
    }

    // Recurring event rows that just fired: stamp `last_notified_at` so
    // the next tick skips them within `EVENT_NOTIFY_REFIRE_SECS`.
    let notified_count = event_notified.len();
    if notified_count > 0 {
        let mut by_id: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for (id, t) in event_notified {
            let prev = by_id.get(&id).copied().unwrap_or(0);
            if t > prev {
                by_id.insert(id, t);
            }
        }
        for (id, t) in by_id {
            let _ = repository::mark_event_notified(&id, t);
        }
    }

    if !to_disable.is_empty() || notified_count > 0 {
        let _ = app.emit(crate::shared::EVENT_EVENTS_UPDATED, ());
    }
    Ok(())
}

/// Decide whether to notify this event row now.
///
/// Rules:
///   * If `last_notified_at == 0`, never notified — fire.
///   * If `last_notified_at` was within `EVENT_NOTIFY_REFIRE_SECS` of the
///     current `now`, skip (the popup is still considered pending or was
///     already shown for this occurrence).
///   * Otherwise the row is stale (the user just powered the bar back on
///     after a long sleep, or the tick missed) — fire once and stamp.
///   * For recurring events: only fire when the NEXT occurrence lives
///     inside the current 30s lookahead window.
fn should_fire_event_notify(ev: &CalendarEvent, fire_at: i64, now: i64) -> bool {
    let delta = fire_at - now;
    if !(0..=TICK.as_secs() as i64).contains(&delta) {
        return false;
    }
    if ev.last_notified_at == 0 {
        return true;
    }
    let elapsed = now - ev.last_notified_at;
    // If the event fired at `fire_at` and we recorded `last_notified_at`
    // against `now`, then `fire_at - last_notified_at` is at most
    // (TICK + jitter) and a value pretty close to 0 means we're still in
    // the same occurrence window. We consider the same occurrence to be
    // "elapsed < EVENT_NOTIFY_REFIRE_SECS".
    elapsed >= EVENT_NOTIFY_REFIRE_SECS
}

fn already_fired(id: &str, fire_at: i64) -> bool {
    if let Ok(g) = LAST_FIRED_AT.lock() {
        let set = g.as_ref();
        if let Some(set) = set {
            return set.contains(&(id.to_string(), fire_at));
        }
    }
    false
}

fn record_fired(id: &str, fire_at: i64) {
    if let Ok(mut g) = LAST_FIRED_AT.lock() {
        let set = g.get_or_insert_with(HashSet::new);
        set.insert((id.to_string(), fire_at));
        // Trim old records
        let cutoff = fire_at - DEDUP_WINDOW_SECS;
        set.retain(|(_, t)| *t >= cutoff);
    }
}

/// Compute the next epoch-second when this alarm should fire relative to
/// `now`. Returns `None` if no future occurrence is computable.
fn next_fire_secs(ev: &CalendarEvent, now: i64) -> Option<i64> {
    let (h, m) = parse_hhmm(ev.time.as_deref()?)?;
    match ev.recurrence {
        Recurrence::None => parse_date_secs(&ev.date, h, m),
        Recurrence::Daily => {
            let day = now / 86400;
            let secs = day * 86400 + h * 3600 + m * 60;
            Some(if secs > now { secs } else { secs + 86400 })
        }
        Recurrence::Weekly => next_weekly(&ev.date, ev.weekdays, h, m, now),
        Recurrence::Monthly => next_monthly(&ev.date, h, m, now),
    }
}

fn next_weekly(_date: &str, weekdays: u32, h: i64, m: i64, now: i64) -> Option<i64> {
    let today = now / 86400;
    for offset in 0..14 {
        let d = today + offset;
        let wd = weekday_of_epoch_day(d);
        if (weekdays >> wd) & 1 == 1 {
            let candidate = d * 86400 + h * 3600 + m * 60;
            if candidate > now {
                return Some(candidate);
            }
        }
    }
    None
}

fn next_monthly(date: &str, h: i64, m: i64, now: i64) -> Option<i64> {
    let by: i64 = date.get(0..4)?.parse().ok()?;
    let bm: i64 = date.get(5..7)?.parse().ok()?;
    let bd: i64 = date.get(8..10)?.parse().ok()?;
    let today = now / 86400;
    for offset in 0..366 {
        let d = today + offset;
        let (y, mo, dd) = civil_from_days(d);
        if y == by && mo == bm && dd == bd {
            let candidate = d * 86400 + h * 3600 + m * 60;
            if candidate > now {
                return Some(candidate);
            }
        }
    }
    None
}

fn parse_hhmm(s: &str) -> Option<(i64, i64)> {
    let mut parts = s.splitn(2, ':');
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    Some((h, m))
}

fn parse_date_secs(s: &str, h: i64, m: i64) -> Option<i64> {
    let mut d = [0i64; 3];
    for (i, p) in s.splitn(3, '-').enumerate() {
        d[i] = p.parse().ok()?;
    }
    let days = civil_to_days(d[0], d[1], d[2]);
    Some(days * 86400 + h * 3600 + m * 60)
}

fn civil_to_days(y: i64, m: i64, d: i64) -> i64 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719469
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn weekday_of_epoch_day(d: i64) -> u32 {
    // 1970-01-01 was Thursday (weekday index 4 in 0=Sun scale).
    let r = (d + 4).rem_euclid(7);
    r as u32
}

/// Fire the alarm: play sound + open notification window.
fn fire_alarm(app: &AppHandle, ev: &CalendarEvent, fire_at: i64) {
    if alarms_app_enabled(app) {
        play_windows_alarm_sound();
    }
    open_alarm_popup(app, ev, fire_at);
}

fn play_windows_alarm_sound() {
    // Use winmm!MessageBeep for the system "alarm" beep. Simplest path that
    // doesn't require feature additions or file shipping.
    extern "system" {
        fn MessageBeep(uType: u32) -> i32;
    }
    unsafe {
        let _ = MessageBeep(0x00000040); // MB_ICONWARNING (system alarm tone)
    }
}

fn open_alarm_popup(app: &AppHandle, ev: &CalendarEvent, fire_at: i64) {
    let label = format!("alarm-popup-{}", ev.id);
    let title = ev.title.clone();
    let title_js = escape_js_string(&title);
    let time = format_fire_clock(fire_at);
    let time_js = escape_js_string(&time);
    let end_js = escape_js_string(ev.end_time.as_deref().unwrap_or(""));
    let h = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = tauri::WebviewWindowBuilder::new(
            &h,
            label,
            tauri::WebviewUrl::App("alarms/alarm-popup.html".into()),
        )
        .title(title.clone())
        .inner_size(360.0, 200.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .additional_browser_args("--default-background-color=00000000")
          .initialization_script(format!(
            "window.__ZENITH_ALARM_TITLE = '{}';\nwindow.__ZENITH_ALARM_TIME = '{}';\nwindow.__ZENITH_ALARM_END = '{}';",
            title_js, time_js, end_js
        ))
        .build()
        {
            eprintln!("[zenith:events] alarm popup build failed: {e:?}");
        }
    });
}

fn escape_js_string(s: &str) -> String {
    s.replace('\\', r"\\").replace('\'', r"\'").replace('\n', r"\n")
}

fn format_fire_clock(secs: i64) -> String {
    let day = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let (y, mo, d) = civil_from_days(day);
    format!("{:02}:{:02} · {:04}-{:02}-{:02}", h, m, y, mo, d)
}
