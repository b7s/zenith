//! Alarm-firing background thread.
//!
//! Runs every 30 seconds, scans enabled events for any whose configured
//! notify instant falls within the current window. The notify instant is
//! `scheduled - notify_advance_secs` (advance=0 means "at the scheduled
//! time"). On a hit:
//!   * raises a native Windows alarm toast (looping alarm sound, stays on
//!     screen until dismissed) — see `toast::fire_alarm_toast`
//!   * for one-shot items (`Recurrence::None`), disables the row so it won't
//!     fire again unless the user re-enables it
//!
//! Two kinds of rows fire this toast:
//!   - `kind = Alarm` — the user's stand-alone timed reminders.
//!   - `kind = Event` with `notify_on_start = true` — synced events from
//!     Google Calendar / Outlook (or a local event the user has flagged
//!     for notification). For these we stamp `last_notified_at` so the
//!     next 30 seconds don't refire the same row. Local all-day events
//!     skip the toast (they have no `time`); the alarms widget still
//!     surfaces them on the bar.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use super::model::{CalendarEvent, EventKind, Recurrence};
use super::repository;

const TICK: Duration = Duration::from_secs(30);
/// A pending toast fires when its `notify_at` instant is within this many
/// seconds of `now`. Lower bounds the live window; upper bounds the
/// "catch-up after a missed tick" window so a slept-through alarm still
/// fires once when the bar is back on, but is not repeatedly alarmed.
const DEDUP_WINDOW_SECS: i64 = 60;

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

fn current_bar_present(app: &AppHandle) -> bool {
    // The alarm-firing thread runs while Zenith is alive. The bar-window
    // probe is a cheap liveness check — when the bar is closed the app is
    // shutting down and toasts are unnecessary; the toast API itself would
    // also fail on a torn-down process.
    app.get_webview_window("bar").is_some()
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

        // ---- Shared gate: both Alarm rows and Event-start notifications
        // require `notify_on_start: true` and a concrete start `time`
        // (all-day events never fire a toast). The `notify_advance_secs`
        // field shifts the toast earlier — the toast fires at
        // `scheduled - advance_secs`, NOT at the scheduled instant.
        if !ev.notify_on_start || ev.time.is_none() {
            continue;
        }

        let Some(fire_at) = next_fire_secs(ev, now) else {
            continue;
        };

        // `notify_at` is the epoch-second when the toast should actually
        // fire. If `notify_advance_secs` is `0`, this is `fire_at` itself
        // (no advance) — the legacy behavior. We deduplicate by
        // `(id, notify_at)` so an event with an advance fires exactly once
        // at the advanced instant, not again at the scheduled instant.
        let notify_at = fire_at - ev.notify_advance_secs.max(0);
        if notify_at > now {
            // Notify instant is still in the future. Skip — no toast yet.
            continue;
        }
        let delta = now - notify_at;
        // A pending toast is one whose notify_at is within the last tick
        // window. We keep DEDUP_WINDOW_SECS as the upper bound so a missed
        // tick (sleep, bar off) still fires the toast up to 60s later,
        // after which we treat it as "missed" and move on.
        if delta > DEDUP_WINDOW_SECS {
            continue;
        }

        let is_alarm = ev.kind == EventKind::Alarm;

        // Dedup with the *notify_at* instant (not fire_at) — the user
        // configured the toast to appear at notify_at, and that's what
        // we want to record as "fired this occurrence".
        if already_fired(&ev.id, notify_at) {
            continue;
        }
        record_fired(&ev.id, notify_at);
        fire_alarm(app, ev, notify_at);

        if matches!(ev.recurrence, Recurrence::None) {
            to_disable.push(ev.id.clone());
        } else if !is_alarm {
            // Recurring Event-kind rows stamp `last_notified_at` so the
            // next tick won't refire the same occurrence. Alarm rows
            // dedupe purely via the in-memory set above.
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
    // the next tick skips them until a new occurrence rolls around.
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

/// Fire the alarm: raise a native Windows alarm toast.
///
/// The toast itself owns the looping alarm sound (its XML declares
/// `<scenario value="alarm">` + `Notification.Looping.Alarm`). On any toast
/// failure `toast::fire_alarm_toast` falls back to a single `MessageBeep`
/// so the user always hears at least one beep. We no longer gate on
/// `alarms_app_enabled` here: that helper used to skip both the sound and
/// the popup when the user muted alarms via config; now we always raise
/// the toast so the user sees the reminder, and let the OS apply Focus
/// Assist / system mute to the looping audio.
fn fire_alarm(app: &AppHandle, ev: &CalendarEvent, fire_at: i64) {
    if !current_bar_present(app) {
        return;
    }
    super::toast::fire_alarm_toast(ev, fire_at);
}
