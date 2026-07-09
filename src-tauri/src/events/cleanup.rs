//! Hourly background cleanup of old one-shot events.
//!
//! Runs on a dedicated thread spawned from `lib.rs::setup`. At every tick
//! (every ~60 seconds) it checks whether the hourly interval has elapsed
//! since the last run; when due, it removes `Recurrence::None` events
//! whose date is more than `delete_old_events_days` (default 7) old and
//! pushes the cleaned file to OneDrive when sync is enabled.

use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use super::repository;

const CHECK_INTERVAL: Duration = Duration::from_secs(60);
const RUN_INTERVAL_SECS: i64 = 60 * 60;
const DEFAULT_DELETE_DAYS: u32 = 7;

static LAST_RUN_AT: AtomicI64 = AtomicI64::new(0);

fn delete_days() -> u32 {
    crate::config::repository::get_or(
        "/widgets/config/datetime/delete_old_events_days",
        DEFAULT_DELETE_DAYS,
    )
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn spawn(_app: AppHandle) {
    thread::spawn(move || {
        let app = _app;
        loop {
            thread::sleep(CHECK_INTERVAL);
            let now = now_secs();
            let last = LAST_RUN_AT.load(Ordering::Relaxed);
            // First run: fire ASAP so the user's data is cleaned on startup.
            // Subsequent runs: strictly every `RUN_INTERVAL_SECS`.
            if last != 0 && now - last < RUN_INTERVAL_SECS {
                continue;
            }
            if run_cycle(&app).is_ok() {
                LAST_RUN_AT.store(now, Ordering::Relaxed);
            }
        }
    });
}

fn run_cycle(app: &AppHandle) -> Result<(), String> {
    let days = delete_days();
    match repository::cleanup_old_events(days) {
        Ok(0) => Ok(()),
        Ok(n) => {
            eprintln!("[zenith:events] cleanup removed {n} events older than {days} days");
            let _ = repository::force_sync(app);
            Ok(())
        }
        Err(e) => {
            eprintln!("[zenith:events] cleanup error: {e}");
            Ok(())
        }
    }
}
