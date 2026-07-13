//! Background polling loop for calendar sync.
//!
//! Spawns a dedicated thread that runs `sync_all()` periodically (every 5
//! minutes) plus once shortly after startup. A global flag lets the loop be
//! cancelled on shutdown. The loop is intentionally light — it only touches
//! config + the events store, never the UI thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::calendar_sync::sync;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Interval between full syncs when no account asks for something sooner.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Start the background sync thread (idempotent — calling twice is a no-op).
pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    thread::spawn(move || {
        // Initial sync shortly after launch (give config a moment to settle).
        thread::sleep(Duration::from_secs(10));
        let _ = sync::sync_all();
        while RUNNING.load(Ordering::SeqCst) {
            thread::sleep(DEFAULT_INTERVAL);
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            let _ = sync::sync_all();
        }
    });
}

/// Stop the background sync thread (best-effort; the loop checks the flag).
#[allow(dead_code)]
pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
}

/// Spawn a one-off sync (used right after a new account connects so the
/// user sees events without waiting for the next interval).
pub fn trigger_now() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    thread::spawn(move || {
        let _ = sync::sync_all();
        r.store(false, Ordering::SeqCst);
    });
}
