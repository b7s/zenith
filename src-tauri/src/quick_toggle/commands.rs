use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::process::Command;
use windows::Win32::Foundation::WPARAM;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_VALUE_TYPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_SETTINGCHANGE,
};

#[derive(Debug, Clone, Serialize)]
pub struct ToggleResult {
    pub on: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleStatus {
    pub states: std::collections::HashMap<String, bool>,
    pub available: std::collections::HashMap<String, bool>,
}

const QT_TOGGLE_UPDATED: &str = "zenith:quick-toggle-updated";

struct ToggleCache {
    states: std::collections::HashMap<String, bool>,
    available: std::collections::HashMap<String, bool>,
    initialized: bool,
}

static CACHE: OnceLock<Mutex<ToggleCache>> = OnceLock::new();

fn cache() -> &'static Mutex<ToggleCache> {
    CACHE.get_or_init(|| {
        Mutex::new(ToggleCache {
            states: std::collections::HashMap::new(),
            available: std::collections::HashMap::new(),
            initialized: false,
        })
    })
}

fn reg_read_dword(path: &[u16], name: &[u16]) -> Option<u32> {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        ).is_err() {
            return None;
        }
        let mut val: u32 = 0;
        let mut kind = REG_VALUE_TYPE(0);
        let mut size = std::mem::size_of::<u32>() as u32;
        let r = RegQueryValueExW(
            hkey,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(&mut val as *mut _ as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if r.is_ok() { Some(val) } else { None }
    }
}

fn reg_write_dword(path: &[u16], name: &[u16], val: u32) -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut hkey,
        ).is_err() {
            return false;
        }
        let bytes = val.to_le_bytes();
        let r = RegSetValueExW(
            hkey,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            REG_DWORD,
            Some(&bytes),
        );
        let _ = RegCloseKey(hkey);
        r.is_ok()
    }
}

fn broadcast_setting_change() {
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
            SMTO_BLOCK | SMTO_ABORTIFHUNG,
            100,
            None,
        );
    }
}

fn ps_bool(script: &str) -> Option<bool> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    match s.trim().to_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => Some(true),
        "false" | "0" | "off" | "no" | "" => Some(false),
        _ => None,
    }
}

fn ps_run(script: &str) -> bool {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn u16z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn read_wifi() -> bool {
    let s = "try { (Get-NetAdapter -Name 'Wi-Fi' -ErrorAction Stop).Status -eq 'Up' } catch { (Get-NetAdapter -Physical | Where-Object {$_.MediaType -eq '802.11'} | Select-Object -First 1).Status -eq 'Up' }";
    ps_bool(s).unwrap_or(false)
}

fn read_wifi_available() -> bool {
    ps_bool("(Get-NetAdapter -Physical | Where-Object {$_.MediaType -eq '802.11'} | Measure-Object).Count -gt 0")
        .unwrap_or(false)
}

fn read_bluetooth() -> bool {
    ps_bool("(Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Where-Object {$_.Status -eq 'OK'} | Measure-Object).Count -gt 0")
        .unwrap_or(false)
}

fn read_bluetooth_available() -> bool {
    ps_bool("(Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0")
        .unwrap_or(false)
}

fn read_dark_mode() -> bool {
    let path = u16z("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = u16z("AppsUseLightTheme");
    reg_read_dword(&path, &name).map(|v| v == 0).unwrap_or(false)
}

fn read_focus_assist() -> bool {
    // DND state on Win11 24H2 lives in a CloudStore blob whose `Data` field
    // embeds the active profile name as a UTF-16LE string, e.g.
    //   "Microsoft.QuietHoursProfile.Unrestricted"  → OFF
    //   "Microsoft.QuietHoursProfile.PriorityOnly" → ON
    //   "Microsoft.QuietHoursProfile.AlarmsOnly"   → ON
    // Anything but Unrestricted means DND is engaged.
    const PROFILE_PREFIX: &str = "Microsoft.QuietHoursProfile.";
    let needle: Vec<u16> = PROFILE_PREFIX.encode_utf16().collect();
    let path = u16z("Software\\Microsoft\\Windows\\CurrentVersion\\CloudStore\\Store\\DefaultAccount\\Current\\{af15e3cc-9b2e-4769-8aee-f66a5de5bd97}$windows.data.donotdisturb.quiethourssettings\\windows.data.donotdisturb.quiethourssettings");
    let blob = reg_read_binary(&path, &u16z("Data"));
    if blob.len() < needle.len() * 2 {
        return false;
    }
    // Scan all byte offsets for a UTF-16LE match of the prefix.
    for start in 0..=(blob.len() - needle.len() * 2) {
        let mut matched = true;
        for (i, &wc) in needle.iter().enumerate() {
            let lo = blob[start + i * 2] as u16;
            let hi = blob[start + i * 2 + 1] as u16;
            if (lo | (hi << 8)) != wc {
                matched = false;
                break;
            }
        }
        if !matched {
            continue;
        }
        // Read the suffix as UTF-16LE until we hit a 0x0000 terminator or the
        // known/likely trailer bytes. Cap at 32 wide chars to bound the scan.
        let mut chars: Vec<u16> = Vec::with_capacity(32);
        let mut p = start + needle.len() * 2;
        while p + 1 < blob.len() && chars.len() < 32 {
            let lo = blob[p] as u16;
            let hi = blob[p + 1] as u16;
            let wc = lo | (hi << 8);
            if wc == 0 {
                break;
            }
            let c = char::from_u32(wc as u32);
            match c {
                Some(ch) if ch.is_ascii_alphanumeric() || ch == '.' => {
                    chars.push(wc);
                    p += 2;
                }
                _ => break,
            }
        }
        let suffix = String::from_utf16_lossy(&chars);
        return suffix != "Unrestricted";
    }
    false
}

/// Read a raw binary registry value under HKCU. Returns the byte blob (empty on failure).
fn reg_read_binary(path: &[u16], name: &[u16]) -> Vec<u8> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
    };
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        ).is_err() {
            return Vec::new();
        }
        // Two-pass: first determine size, then read.
        let mut kind = REG_VALUE_TYPE(0);
        let mut size: u32 = 0;
        let r1 = RegQueryValueExW(
            hkey,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        );
        if r1.is_err() || size == 0 {
            let _ = RegCloseKey(hkey);
            return Vec::new();
        }
        let mut buf = vec![0u8; size as usize];
        let r2 = RegQueryValueExW(
            hkey,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if r2.is_err() {
            return Vec::new();
        }
        buf.truncate(size as usize);
        buf
    }
}

fn read_airplane_mode() -> bool {
    let path = u16z("Software\\Microsoft\\Windows\\CurrentVersion\\NetworkState\\AirplaneMode");
    let name = u16z("On");
    reg_read_dword(&path, &name).map(|v| v != 0).unwrap_or(false)
}

fn night_light_reg_path() -> Vec<u16> {
    u16z("Software\\Microsoft\\Windows\\CurrentVersion\\CloudStore\\Store\\DefaultAccount\\Current\\default$windows.data.bluelightreduction.bluelightreductionstate\\windows.data.bluelightreduction.bluelightreductionstate")
}

/// Read the raw `Data` binary blob from the CloudStore night-light key.
fn read_night_light_blob() -> Option<Vec<u8>> {
    use windows::Win32::System::Registry::{RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE};
    let path = night_light_reg_path();
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        ).is_err() {
            return None;
        }
        // Blob can be up to 64 bytes; read into a local buffer.
        let mut buf = [0u8; 64];
        let mut kind = REG_VALUE_TYPE(0);
        let mut size: u32 = buf.len() as u32;
        let r = RegQueryValueExW(
            hkey,
            windows::core::PCWSTR(u16z("Data").as_ptr()),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if r.is_err() || size == 0 || size > buf.len() as u32 {
            return None;
        }
        Some(buf[..size as usize].to_vec())
    }
}

fn read_night_light() -> bool {
    // Byte 18 is the on/off flag: 0x15 = ON, 0x13 = OFF.
    match read_night_light_blob() {
        Some(b) if b.len() > 18 => b[18] == 0x15,
        _ => false,
    }
}

/// Increment the version counter at bytes 10..14 so Windows detects the change.
fn bump_version(b: &mut [u8]) {
    for i in 10..15 {
        if i < b.len() && b[i] != 0xff {
            b[i] += 1;
            break;
        }
    }
}

fn write_night_light(on: bool) -> bool {
    use windows::Win32::System::Registry::{RegSetValueExW, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_BINARY};
    let path = night_light_reg_path();
    let cur = match read_night_light_blob() {
        Some(b) => b,
        None => return false,
    };
    let is_enabled = cur.len() > 18 && cur[18] == 0x15;

    // If the requested state already matches, do nothing.
    if is_enabled == on {
        return true;
    }

    let mut new_data: Vec<u8>;
    if on {
        // Enable: rebuild as 43 bytes, set flag + padding, bump version.
        new_data = vec![0u8; 43];
        new_data[..22].copy_from_slice(&cur[..22]);
        new_data[25..43].copy_from_slice(&cur[23..41]);
        new_data[18] = 0x15;
        new_data[23] = 0x10;
        new_data[24] = 0x00;
    } else {
        // Disable: rebuild as 41 bytes, clear flag, bump version.
        new_data = vec![0u8; 41];
        new_data[..22].copy_from_slice(&cur[..22]);
        new_data[23..41].copy_from_slice(&cur[25..43.min(cur.len())]);
        new_data[18] = 0x13;
    }
    bump_version(&mut new_data);

    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_READ | KEY_SET_VALUE,
            &mut hkey,
        ).is_err() {
            return false;
        }
        let wr = RegSetValueExW(
            hkey,
            windows::core::PCWSTR(u16z("Data").as_ptr()),
            None,
            REG_BINARY,
            Some(&new_data),
        );
        let _ = RegCloseKey(hkey);
        if wr.is_err() {
            return false;
        }
    }
    broadcast_setting_change();
    true
}

fn initialize_cache() {
    let mut g = match cache().lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if g.initialized { return; }

    g.states.insert("wifi".into(), read_wifi());
    g.states.insert("bluetooth".into(), read_bluetooth());
    g.states.insert("dark_mode".into(), read_dark_mode());
    g.states.insert("focus_assist".into(), read_focus_assist());
    g.states.insert("airplane".into(), read_airplane_mode());
    g.states.insert("night_light".into(), read_night_light());

    g.available.insert("wifi".into(), read_wifi_available());
    g.available.insert("bluetooth".into(), read_bluetooth_available());
    g.available.insert("dark_mode".into(), true);
    g.available.insert("focus_assist".into(), true);
    g.available.insert("airplane".into(), true);
    g.available.insert("night_light".into(), true);

    g.initialized = true;
}

fn set_cached(key: &str, val: bool) {
    if let Ok(mut g) = cache().lock() {
        g.states.insert(key.into(), val);
    }
}

fn do_toggle_wifi() -> bool {
    let cur = read_wifi();
    let action = if cur { "Disable" } else { "Enable" };
    let s = format!(
        "try {{ {}-NetAdapter -Name 'Wi-Fi' -Confirm:$false }} catch {{ {}-NetAdapter -Physical -MediaType 802.11 -Confirm:$false }}",
        action, action
    );
    let ok = ps_run(&s);
    let new_state = if ok { !cur } else { cur };
    set_cached("wifi", new_state);
    new_state
}

fn do_toggle_bluetooth() -> bool {
    let cur = read_bluetooth();
    let action = if cur { "Disable" } else { "Enable" };
    let status_filter = if cur { "OK" } else { "Error" };
    let s = format!(
        "Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Where-Object {{$_.Status -eq '{}'}} | ForEach-Object {{ {}-PnpDevice $_.InstanceId -Confirm:$false }}",
        status_filter, action
    );
    let _ = ps_run(&s);
    let new_state = !cur;
    set_cached("bluetooth", new_state);
    new_state
}

fn do_toggle_dark_mode() -> bool {
    let cur = read_dark_mode();
    let new_val = if cur { 1u32 } else { 0u32 };
    let path = u16z("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let _ = reg_write_dword(&path, &u16z("AppsUseLightTheme"), new_val);
    let _ = reg_write_dword(&path, &u16z("SystemUsesLightTheme"), new_val);
    broadcast_setting_change();
    let new_state = !cur;
    set_cached("dark_mode", new_state);
    new_state
}

fn do_toggle_focus_assist() -> bool {
    open_focus_app();
    let cur = read_focus_assist();
    set_cached("focus_assist", cur);
    cur
}

/// Launch the Focus Sessions page in the Alarms & Clock app.
/// Uses `ms-clock:focus` deep-link URI (version-independent, no package path needed).
/// Falls back to `ms-clock:` and then dynamic/hardcoded AUMID.
fn open_focus_app() {
    // 1. Try ms-clock:focus deep-link (opens Focus Sessions tab directly)
    if ps_run("Start-Process 'ms-clock:focus'") {
        return;
    }
    // 2. Fallback to generic ms-clock: protocol
    if ps_run("Start-Process 'ms-clock:'") {
        return;
    }
    // 3. Detect AUMID dynamically from the installed package
    if ps_run(
        "$p = Get-AppxPackage -Name Microsoft.WindowsAlarms -ErrorAction SilentlyContinue; \
         if ($p) { Start-Process \"shell:AppsFolder\\$($p.PackageFamilyName)!App\" }"
    ) {
        return;
    }
    // 4. Last resort: hardcoded AUMID
    let _ = Command::new("explorer")
        .arg("shell:AppsFolder\\Microsoft.WindowsAlarms_8wekyb3d8bbwe!App")
        .spawn();
}

fn do_toggle_night_light() -> bool {
    let cur = read_night_light();
    let new_state = !cur;
    if write_night_light(new_state) {
        set_cached("night_light", new_state);
        new_state
    } else {
        cur
    }
}

fn do_toggle_airplane_mode() -> bool {
    // Airplane mode can't be reliably toggled via registry on Win11 24H2
    // (no stable key; the Radio WinRT API isn't accessible from desktop).
    // Open the Settings page so the user can toggle it there. Return the
    // actual current state — don't fake a flip.
    let _ = ps_run("Start-Process 'ms-settings:network-airplanemode'");
    let cur = read_airplane_mode();
    set_cached("airplane", cur);
    cur
}

#[tauri::command]
pub async fn toggle_wifi(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let on = tauri::async_runtime::spawn_blocking(|| do_toggle_wifi())
        .await
        .map_err(|e| e.to_string())?;
    emit_toggle_updated(&app);
    Ok(ToggleResult { on })
}

#[tauri::command]
pub async fn toggle_bluetooth(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let on = tauri::async_runtime::spawn_blocking(|| do_toggle_bluetooth())
        .await
        .map_err(|e| e.to_string())?;
    emit_toggle_updated(&app);
    Ok(ToggleResult { on })
}

#[tauri::command]
pub fn toggle_dark_mode(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let r = ToggleResult { on: do_toggle_dark_mode() };
    emit_toggle_updated(&app);
    Ok(r)
}

#[tauri::command]
pub fn toggle_focus_assist(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let r = ToggleResult { on: do_toggle_focus_assist() };
    emit_toggle_updated(&app);
    Ok(r)
}

#[tauri::command]
pub async fn toggle_airplane(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let on = tauri::async_runtime::spawn_blocking(|| do_toggle_airplane_mode())
        .await
        .map_err(|e| e.to_string())?;
    emit_toggle_updated(&app);
    Ok(ToggleResult { on })
}

#[tauri::command]
pub async fn toggle_night_light(app: tauri::AppHandle) -> Result<ToggleResult, String> {
    let on = tauri::async_runtime::spawn_blocking(|| do_toggle_night_light())
        .await
        .map_err(|e| e.to_string())?;
    emit_toggle_updated(&app);
    Ok(ToggleResult { on })
}

#[tauri::command]
pub fn get_quick_toggle_status() -> Result<ToggleStatus, String> {
    initialize_cache();
    let g = match cache().lock() {
        Ok(g) => g,
        Err(_) => return Ok(ToggleStatus {
            states: std::collections::HashMap::new(),
            available: std::collections::HashMap::new(),
        }),
    };
    let cheap = ["dark_mode", "focus_assist", "airplane", "night_light"];
    let mut states = g.states.clone();
    for key in cheap {
        let v = match key {
            "dark_mode" => read_dark_mode(),
            "focus_assist" => read_focus_assist(),
            "airplane" => read_airplane_mode(),
            "night_light" => read_night_light(),
            _ => false,
        };
        states.insert(key.into(), v);
    }
    Ok(ToggleStatus { states, available: g.available.clone() })
}

pub fn emit_toggle_updated(app: &tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(QT_TOGGLE_UPDATED, ());
}
