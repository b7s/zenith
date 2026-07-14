//! Minimal std-only ISO-8601 (RFC3339) <-> Unix-seconds helpers.
//!
//! Used by the calendar providers to format `timeMin`/`timeMax` query
//! params and to parse event start/end timestamps. Only UTC and simple
//! `YYYY-MM-DD` date forms are supported (good enough for calendar
//! sync). No external date crate — keeps the dependency surface small.

#[allow(dead_code)]
const DAYS_PER_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

#[allow(dead_code)]
fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days since the Unix epoch (1970-01-01) for a Gregorian date (UTC).
fn ymd_to_days(y: i64, m: u32, d: u32) -> i64 {
    let mut yy = y;
    let mm = m as i64;
    // Shift so March is month 0, Feb last -> leap-day math simplifies.
    if mm <= 2 {
        yy -= 1;
    }
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400; // [0, 399]
    let m_from_mar = (mm + 9) % 12; // 0..11
    let doy = (m_from_mar * 306 + 5) / 10 + (d as i64) - 1; // day of year [0,365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day of era
    era * 146097 + doe - 719468 // days since 1970-01-01
}

/// Format a Unix timestamp as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let mut rem = secs.rem_euclid(86400);
    let h = rem / 3600;
    rem %= 3600;
    let m = rem / 60;
    let s = rem % 60;

    // Recover year/month/day from `days` since epoch.
    let d0 = days + 719468;
    let era = if d0 >= 0 { d0 } else { d0 - 146096 } / 146097;
    let doe = d0 - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0,399]
    let y = yoe + era * 400;
    let d_y = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0,365]
    let mp = (5 * d_y + 2) / 153; // month since March [0,11]
    let m_on = if mp < 10 { mp + 3 } else { mp - 9 }; // 1..12
    let y_on = if m_on <= 2 { y + 1 } else { y };
    let d_on = d_y - (153 * mp + 2) / 5 + 1; // 1..31

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y_on, m_on, d_on, h, m, s
    )
}

/// Parse an RFC3339 timestamp (`YYYY-MM-DDTHH:MM:SSZ`, with optional
/// offset, or a date-only `YYYY-MM-DD`) into Unix seconds. Returns 0 when
/// unparseable. All-day dates resolve to midnight UTC.
pub fn parse_rfc3339(value: &str) -> i64 {
    let v = value.trim();
    if v.len() >= 10 && v.as_bytes()[4] == b'-' && v.as_bytes()[7] == b'-' {
        let y: i64 = match v.get(0..4).and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => return 0,
        };
        let mo: u32 = match v.get(5..7).and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => return 0,
        };
        let d: u32 = match v.get(8..10).and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => return 0,
        };
        let day_secs = ymd_to_days(y, mo, d) * 86400;
        // Date-only form (length 10) → midnight UTC.
        if v.len() == 10 {
            return day_secs;
        }
        // Expect `T` at index 10.
        if v.as_bytes().get(10) != Some(&b'T') {
            return day_secs;
        }
        let rest = &v[11..];
        // Split time and optional offset.
        let (time_part, offset) = match rest.find(|c| ['Z', '+', '-'].contains(&c)) {
            Some(i) => {
                let t = &rest[..i];
                let off = &rest[i..];
                (t, Some(off))
            }
            None => (rest, None),
        };
        let mut it = time_part.split(':');
        let hh: i64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let mm: i64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let ss: i64 = it
            .next()
            .map(|x| x.trim_matches(|c: char| !c.is_ascii_digit()))
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
        let mut total = day_secs + hh * 3600 + mm * 60 + ss;
        if let Some(off) = offset {
            if off != "Z" {
                // Parse `+HH:MM` / `-HH:MM`.
                let sign = if off.starts_with('-') { -1 } else { 1 };
                let body = &off[1..];
                let mut oit = body.split(':');
                let oh: i64 = oit.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let om: i64 = oit.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                total -= sign * (oh * 3600 + om * 60);
            }
        }
        return total;
    }
    0
}

#[allow(dead_code)]
fn _assert_days() {
    // Sanity: 1970-01-01 -> 0, 2000-01-01 -> 10957, 2024-02-29 -> 19781.
    debug_assert_eq!(ymd_to_days(1970, 1, 1), 0);
    debug_assert_eq!(ymd_to_days(2000, 1, 1), 10957);
    debug_assert_eq!(ymd_to_days(2024, 2, 29), 19781);
}
