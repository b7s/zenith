//! Native Windows toast notifications for alarm firing.
//!
//! Replaces the previous custom Tauri popup window with the OS-native
//! "alarm scenario" toast — the same UX the built-in Windows Clock / Alarms
//! app uses. The toast stays on screen and loops the default alarm sound
//! until the user dismisses it; no custom HTML/CSS/JS window is required.
//!
//! Two prerequisites must hold for `ToastNotificationManager` to surface a
//! toast:
//!
//! 1. **AppUserModelID (AUMID)** — the calling process must be associated
//!    with an AUMID that Windows knows about. We use the app's Tauri
//!    identifier (`com.zenith.bar`, declared in `tauri.conf.json`). The AUMID
//!    only needs registration in the registry under
//!    `HKCU\Software\Classes\AppUserModelId\<AUMID>` with `DisplayName` and
//!    `IconUri` so Windows can title the toast and stamp the notification
//!    center entry. `register_aumid()` does this once at startup and is
//!    idempotent.
//!
//! 2. **COM apartment** — the toast APIs are WinRT and require COM to be
//!    initialized on the calling thread. `fire_alarm_toast` runs on the
//!    alarm-firing thread which already initializes COM via `winvd`; we still
//!    guard with `CoInitializeEx`-on-failure because the toast path must work
//!    even on a fresh thread spawned by `tauri::async_runtime`.
//!
//! After the AUMID is registered, `fire_alarm_toast` builds the toast XML
//! (alarm scenario + looping alarm audio + dismiss button) and submits it
//! through `ToastNotificationManager::CreateToastNotifier(aumid)`. Failures
//! fall back to `MessageBeep(MB_ICONWARNING)` so the user still hears
//! *something* if the toast path fails (e.g. Focus Assist suppressing
//! notifications, missing registry entry, etc.).

use windows::core::{w, HSTRING, PCWSTR};
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPEN_CREATE_OPTIONS, REG_SZ,
};

use super::model::CalendarEvent;

/// The AUMID — identical to `tauri.conf.json::identifier`. Single source of
/// truth for the toast pipeline. Keep these two literals in sync.
pub const AUMID: &str = "com.zenith.bar";

/// Register `AUMID` in `HKCU\Software\Classes\AppUserModelId\<AUMID>` so the
/// Action Center can resolve the toast's title + icon. Idempotent: re-running
/// overwrites the previous `DisplayName` / `IconUri` / `ShowInSettings`
/// values, which is safe and cheap. Call once at `lib.rs::setup`.
///
/// We don't read Tauri's bundled icon path dynamically — the install dir is
/// unknown at this point and the registry value is optional. Windows falls
/// back to a generic notification icon when `IconUri` is absent, and the toast
/// still fires. A bug in the icon path therefore never blocks alarms.
pub fn register_aumid() {
    let subkey = format!("Software\\Classes\\AppUserModelId\\{}", AUMID);
    let subkey_h = HSTRING::from(subkey);
    let subkey_pcwstr = PCWSTR(subkey_h.as_ptr());

    let mut hkey = HKEY::default();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey_pcwstr,
            None,
            None,
            REG_OPEN_CREATE_OPTIONS(0),
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        )
    };
    if result.is_err() {
        return;
    }

    set_reg_sz(hkey, w!("DisplayName"), w!("Zenith"));
    set_reg_sz(hkey, w!("IconUri"), w!(""));
    // ShowInSettings = "0" keeps the AUMID out of the Windows Settings →
    // Apps list (we are not pretending to be a first-class installed app for
    // the purposes of the Settings page).
    set_reg_sz(hkey, w!("ShowInSettings"), w!("0"));

    unsafe { let _ = RegCloseKey(hkey); }
}

fn set_reg_sz(hkey: HKEY, name: PCWSTR, value: PCWSTR) {
    // `PCWSTR` is a thin wrapper over `*const u16`; `RegSetValueExW` needs
    // the value bytes as a byte slice (counted in bytes, NUL-terminated
    // for REG_SZ). We re-interpret the UTF-16 buffer as a byte slice.
    let wide = if value.0.is_null() { &[] as &[u16] } else {
        unsafe { std::slice::from_raw_parts(value.0, len_until_null(value.0)) }
    };
    let raw: &[u8] = unsafe {
        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2)
    };
    unsafe { let _ = RegSetValueExW(hkey, name, None, REG_SZ, Some(raw)); }
}

/// Length (in `u16` units, excluding the trailing NUL) of a NUL-terminated
/// wide string. Safe to call only on a pointer that came from `HSTRING`
/// or a Rust string literal widened by `w!` — both are NUL-terminated.
fn len_until_null(p: *const u16) -> usize {
    let mut n = 0usize;
    unsafe {
        while *p.add(n) != 0 { n += 1; }
    }
    n
}

/// Build the toast XML for an alarm. The `<scenario value="alarm"/>` element
/// is what makes this a real OS alarm: the toast stays on screen and the
/// `<audio>` element loops until the user dismisses. The Dismiss button is
/// MANDATORY for the alarm scenario — Windows rejects the toast without
/// at least one `arguments="dismiss"` system-activated button.
fn build_alarm_xml(ev: &CalendarEvent, fire_at: i64) -> String {
    let title = xml_escape(ev.title.as_str());
    let display_title = if title.is_empty() { "Alarm".to_string() } else { title };
    let time_line = format_fire_clock(fire_at);
    let body_line = match ev.end_time.as_deref() {
        Some(end) if !end.is_empty() => format!("{} → {}", time_line, end),
        _ => time_line,
    };
    let body_line_esc = xml_escape(&body_line);

    format!(
        "<toast scenario=\"alarm\">\
            <visual>\
                <binding template=\"ToastGeneric\">\
                    <text>{display_title}</text>\
                    <text>{body_line_esc}</text>\
                </binding>\
            </visual>\
            <audio src=\"ms-winsoundevent:Notification.Looping.Alarm\" loop=\"true\"/>\
            <actions>\
                <action\
                    content=\"Dismiss\"\
                    arguments=\"dismiss\"\
                    activationType=\"system\"/>\
            </actions>\
        </toast>"
    )
}

/// Submit an alarm toast to the system. Best-effort: on any failure (COM not
/// ready, AUMID not registered, Focus Assist blocking) we fall back to a
/// single `MessageBeep(MB_ICONWARNING)` so the user still gets an audible
/// cue. The alarm-fire thread is the only caller.
pub fn fire_alarm_toast(ev: &CalendarEvent, fire_at: i64) {
    let xml_str = build_alarm_xml(ev, fire_at);
    let xml_h: HSTRING = HSTRING::from(&xml_str);

    let doc = match XmlDocument::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[zenith:events] toast XmlDocument new failed: {e}");
            fallback_beep();
            return;
        }
    };
    if let Err(e) = doc.LoadXml(&xml_h) {
        eprintln!("[zenith:events] toast LoadXml failed: {e}");
        fallback_beep();
        return;
    }

    let toast = match ToastNotification::CreateToastNotification(&doc) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[zenith:events] toast CreateToastNotification failed: {e}");
            fallback_beep();
            return;
        }
    };

    let aumid_h: HSTRING = HSTRING::from(AUMID);
    let notifier = match ToastNotificationManager::CreateToastNotifierWithId(&aumid_h) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[zenith:events] toast CreateToastNotifier failed: {e}");
            fallback_beep();
            return;
        }
    };

    if let Err(e) = notifier.Show(&toast) {
        eprintln!("[zenith:events] toast Show failed: {e}");
        fallback_beep();
    }
}

fn fallback_beep() {
    extern "system" {
        fn MessageBeep(uType: u32) -> i32;
    }
    unsafe { let _ = MessageBeep(0x00000040); }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => { out.push('&'); out.push_str("amp;"); }
            '<' => { out.push('&'); out.push_str("lt;"); }
            '>' => { out.push('&'); out.push_str("gt;"); }
            '"' => { out.push('&'); out.push_str("quot;"); }
            '\'' => { out.push('&'); out.push_str("#39;"); }
            c => out.push(c),
        }
    }
    out
}

/// Format the fire time as `HH:MM · YYYY-MM-DD` (local time) for the toast
/// body. Mirrors the clock the alarm-fire thread computed for the popup.
fn format_fire_clock(secs: i64) -> String {
    let day = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let (y, mo, d) = civil_from_days(day);
    format!("{:02}:{:02} · {:04}-{:02}-{:02}", h, m, y, mo, d)
}

/// Civil-from-days — shared with alarm_fire.rs. The winvd-free epoch-day →
/// (y, m, d) algorithm (Howard Hinnant's `civil_from_days`). Kept private to
/// this module so the alarm toast owns its date formatting independent of
/// the alarm-fire tick implementation.
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
