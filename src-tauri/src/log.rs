use std::io::Write;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static START: OnceLock<Instant> = OnceLock::new();

/// Date-stamped directory: `%TEMP%/zenith/2026-07-04/`
fn log_dir() -> std::path::PathBuf {
    let total_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = total_secs / 86400;

    // Howard Hinnant days-to-date algorithm (zero-dependency).
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let dir = std::env::temp_dir()
        .join("zenith")
        .join(format!("{y:04}-{m:02}-{d:02}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn elapsed() -> String {
    let start = START.get_or_init(Instant::now);
    let e = start.elapsed();
    format!("{: >8}.{:03}", e.as_secs(), e.subsec_millis())
}

#[tauri::command]
pub fn log_write(window: String, level: String, message: String) {
    let path = log_dir().join(format!("{window}.log"));
    let line = format!("[{}] [{:>5}] {}\n", elapsed(), level.to_uppercase(), message);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

#[tauri::command]
pub fn log_clear(window: String) {
    let path = log_dir().join(format!("{window}.log"));
    let _ = std::fs::write(&path, b"");
}
