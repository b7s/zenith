//! Weather widget — OpenWeatherMap One Call 4.0 + Air Pollution backend.
//!
//! Owns the single source of truth for weather data: a process-wide
//! `WEATHER_STATE` mutex. The bar widget calls `weather_refresh` on the
//! configured interval; on success the combined payload is cached here and
//! returned, on failure the previous good cache is preserved and an error
//! snapshot is returned instead (the bar + popup render a Lucide error icon +
//! message). The popup window only ever calls `weather_get_cache` — it never
//! refetches, so opening it costs zero API calls and never flickers.
//!
//! The OpenWeatherMap API key is stored DPAPI-protected (per-Windows-user)
//! under `widgets.config.weather.api_key` as a base64 blob; it is decrypted
//! here in Rust and only ever lives in process memory between the decrypt and
//! the HTTPS request. Plaintext never touches the frontend.
//!
//! Endpoints (all with `?appid={key}`):
//!   - geocode:  https://api.openweathermap.org/geo/1.0/direct?q={city}&limit=1
//!   - current:  https://api.openweathermap.org/data/4.0/onecall/current?lat&lon&units
//!   - daily:    https://api.openweathermap.org/data/4.0/onecall/timeline/1day?cnt=N&lat&lon&units
//!   - air qual: https://api.openweathermap.org/data/2.5/air_pollution?lat&lon
//!
//! `ureq` is the project's existing blocking HTTP client (Cargo feature
//! `json`,`tls`). Sync Tauri commands run on a worker thread, so the blocking
//! calls (~<=2s total) never stall the main thread / IPC channel.

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

use crate::config;
use crate::git::secrets;
use crate::window;

const WEATHER_LABEL: &str = "weather";

/// Cached snapshot returned to both the bar widget and the popup window.
/// `payload` is the combined, pre-shaped data the frontend renders from.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WeatherSnapshot {
    pub ok: bool,
    /// Human label resolved from geocoding (e.g. "Lisbon, PT"), or the raw
    /// configured city when geocoding has not yet run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// "metric" | "imperial" — drives the °C/°F + m/s/mph suffixes in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// 7-day (or fewer) daily forecast records (One Call `1day` timeline).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub daily: Vec<Value>,
    /// Current conditions record (One Call `current` `data[0]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<Value>,
    /// Air pollution record (`list[0]`), includes the `main.aqi` 1..5 index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air: Option<Value>,
    /// Last fetch error message (empty when healthy). Surfaced to the UI
    /// alongside a Lucide `triangle-alert` icon so the user sees the reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Unix seconds of the last **successful** fetch. 0 when never fetched.
    pub updated_at: i64,
}

static WEATHER_STATE: OnceLock<Mutex<WeatherSnapshot>> = OnceLock::new();

fn state() -> &'static Mutex<WeatherSnapshot> {
    WEATHER_STATE.get_or_init(|| Mutex::new(WeatherSnapshot::default()))
}

/// Read the cached snapshot without fetching. The popup window calls this on
/// open so it costs zero API calls and never flickers.
#[tauri::command]
pub fn weather_get_cache() -> WeatherSnapshot {
    state().lock().map(|g| g.clone()).unwrap_or_default()
}

struct WeatherCfg {
    city: String,
    api_key: String,
    forecast_days: u32,
    units: String,
}

/// Read `widgets.config.weather` from the safe config getter and decrypt the
/// DPAPI-protected API key. Returns `Err(message)` when the widget is
/// unconfigured (no city / no key) so the caller can surface a friendly hint.
fn read_weather_cfg() -> Result<WeatherCfg, String> {
    let cfg = config::load();
    let wc = cfg
        .widgets
        .config
        .get("weather")
        .ok_or_else(|| "Weather widget is not configured yet — open its settings.".to_string())?;

    let city = wc
        .get("city")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if city.is_empty() {
        return Err("No city configured — open the weather widget settings.".into());
    }

    let key_blob = wc
        .get("api_key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if key_blob.is_empty() {
        return Err("No API key configured — open the weather widget settings.".into());
    }
    let api_key = secrets::unprotect(&key_blob)
        .ok_or_else(|| "Could not decrypt the saved API key (DPAPI). Re-enter it in settings.".to_string())?;

    let forecast_days = wc
        .get("forecast_days")
        .and_then(Value::as_i64)
        .map(|n| n.clamp(1, 7) as u32)
        .unwrap_or(7);
    let units = wc
        .get("units")
        .and_then(Value::as_str)
        .unwrap_or("metric")
        .to_string();

    Ok(WeatherCfg {
        city,
        api_key,
        forecast_days,
        units,
    })
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn http_get(url: &str) -> Result<Value, String> {
    ureq::get(url)
        .timeout(std::time::Duration::from_secs(12))
        .call()
        .map_err(|e| format!("network: {e}"))
        .and_then(|r| {
            r.into_json::<Value>()
                .map_err(|e| format!("parse: {e}"))
        })
}

/// Geocode a free-text city to (lat, lon, label). Returns the first result.
fn geocode(city: &str, key: &str) -> Result<(f64, f64, String), String> {
    let q = urlencoding_compat(city);
    let url = format!(
        "https://api.openweathermap.org/geo/1.0/direct?q={q}&limit=1&appid={key}"
    );
    let v = http_get(&url)?;
    let arr = v.as_array().ok_or_else(|| "geocode: unexpected response".to_string())?;
    let first = arr
        .first()
        .ok_or_else(|| format!("city '{city}' not found"))?;
    let lat = first
        .get("lat")
        .and_then(Value::as_f64)
        .ok_or_else(|| "geocode: missing lat".to_string())?;
    let lon = first
        .get("lon")
        .and_then(Value::as_f64)
        .ok_or_else(|| "geocode: missing lon".to_string())?;
    let name = first.get("name").and_then(Value::as_str).unwrap_or(city);
    let country = first.get("country").and_then(Value::as_str).unwrap_or("");
    let label = if country.is_empty() {
        name.to_string()
    } else {
        format!("{name}, {country}")
    };
    Ok((lat, lon, label))
}

/// Minimal URL-encoder for the city query (spaces, commas are common in
/// "City, Country" / "São Paulo" inputs). Encodes only the few characters
/// that break a query value — enough for geocoding without pulling in a
/// `url`-crate percent-encoder (the `url` crate is already a dependency but
/// this keeps the weather domain self-contained).
fn urlencoding_compat(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Fetch + cache an updated weather snapshot. Runs on a Tauri worker thread
/// (sync command). On **any** failure the previous good cache is preserved and
/// an error snapshot is returned (ok:false + error), so the bar keeps showing
/// the last known temperature instead of blanking out.
#[tauri::command]
pub fn weather_refresh() -> WeatherSnapshot {
    let cfg = match read_weather_cfg() {
        Ok(c) => c,
        Err(e) => {
            // Not configured / decrypt failed — keep cache, surface error.
            let mut snap = current_snapshot();
            if snap.current.is_none() {
                snap.ok = false;
                snap.error = Some(e);
            } else {
                // Keep showing last good data but annotate the fresh error.
                snap.error = Some(e);
            }
            put_snapshot(snap.clone());
            return snap;
        }
    };

    let result = fetch_all(&cfg);
    let snap = match result {
        Ok((label, current, daily, air)) => WeatherSnapshot {
            ok: true,
            city: Some(label),
            units: Some(cfg.units.clone()),
            daily,
            current: Some(current),
            air: Some(air),
            error: None,
            updated_at: now_secs(),
        },
        Err(e) => {
            let mut prev = current_snapshot();
            prev.ok = false;
            prev.error = Some(e);
            prev
        }
    };
    put_snapshot(snap.clone());
    snap
}

fn fetch_all(cfg: &WeatherCfg) -> Result<(String, Value, Vec<Value>, Value), String> {
    let key = &cfg.api_key;
    let (lat, lon, label) = geocode(&cfg.city, key)?;

    let units = &cfg.units;

    // Try One Call 4.0 first (requires "One Call by Call" subscription).
    // If it fails with 401/403, fall back to free endpoints.
    let (current, daily, air) = match try_one_call_40(lat, lon, units, key) {
        Ok(tuple) => tuple,
        Err(e) if e.contains("401") || e.contains("403") => {
            // Fallback: Current Weather + 5-day/3-hour forecast (free tier)
            let current = http_get(&format!(
                "https://api.openweathermap.org/data/2.5/weather?lat={lat}&lon={lon}&units={units}&lang=en&appid={key}"
            ))?;
            // Normalize 2.5/weather response to match One Call 4.0 structure
            let current = normalize_current_weather(&current)?;
            let forecast = http_get(&format!(
                "https://api.openweathermap.org/data/2.5/forecast?lat={lat}&lon={lon}&units={units}&lang=en&appid={key}"
            ))?;
            // Convert 3-hour forecast to daily min/max for 7 days
            let daily = convert_forecast_to_daily(&forecast, cfg.forecast_days)?;
            let air = http_get(&format!(
                "https://api.openweathermap.org/data/2.5/air_pollution?lat={lat}&lon={lon}&appid={key}"
            ))
                .ok()
                .and_then(|v| v.get("list").and_then(Value::as_array).and_then(|a| a.first().cloned()))
                .unwrap_or(Value::Null);
            (current, daily, air)
        }
        Err(e) => return Err(e),
    };

    Ok((label, current, daily, air))
}

fn try_one_call_40(
    lat: f64,
    lon: f64,
    units: &str,
    key: &str,
) -> Result<(Value, Vec<Value>, Value), String> {
    let current = http_get(&format!(
        "https://api.openweathermap.org/data/4.0/onecall/current?lat={lat}&lon={lon}&units={units}&lang=en&appid={key}"
    ))?;
    let current = current
        .get("data")
        .and_then(Value::as_array)
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| "current: empty data array".to_string())?;

    let daily = http_get(&format!(
        "https://api.openweathermap.org/data/4.0/onecall/timeline/1day?cnt={n}&lat={lat}&lon={lon}&units={units}&lang=en&appid={key}",
        n = 7
    ))?;
    let daily = daily
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let air = http_get(&format!(
        "https://api.openweathermap.org/data/2.5/air_pollution?lat={lat}&lon={lon}&appid={key}"
    ))
        .ok()
        .and_then(|v| v.get("list").and_then(Value::as_array).and_then(|a| a.first().cloned()))
        .unwrap_or(Value::Null);

    Ok((current, daily, air))
}

/// Convert 5-day/3-hour forecast list to daily min/max for up to N days.
fn convert_forecast_to_daily(forecast: &Value, max_days: u32) -> Result<Vec<Value>, String> {
    let list = forecast
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| "forecast: missing list".to_string())?;

    use std::collections::HashMap;
    let mut by_day: HashMap<String, Vec<&Value>> = HashMap::new();

    for entry in list {
        let dt = entry.get("dt").and_then(Value::as_i64).unwrap_or(0);
        let date = chrono::DateTime::from_timestamp(dt, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        if !date.is_empty() {
            by_day.entry(date).or_default().push(entry);
        }
    }

    let mut out = Vec::new();
    let mut sorted_days: Vec<_> = by_day.keys().cloned().collect();
    sorted_days.sort();
    for day in sorted_days.into_iter().take(max_days as usize) {
        if let Some(entries) = by_day.get(&day) {
            let mut min_temp = f64::MAX;
            let mut max_temp = f64::MIN;
            let mut weather_id = 800;
            let mut weather_icon = "01d";
            let mut weather_desc = "clear sky";
            let mut pop_max = 0.0f64;

            for e in entries {
                if let Some(main) = e.get("main") {
                    if let Some(tmin) = main.get("temp_min").and_then(Value::as_f64) {
                        min_temp = min_temp.min(tmin);
                    }
                    if let Some(tmax) = main.get("temp_max").and_then(Value::as_f64) {
                        max_temp = max_temp.max(tmax);
                    }
                }
                if let Some(w) = e.get("weather").and_then(Value::as_array).and_then(|a| a.first()) {
                    weather_id = w.get("id").and_then(Value::as_i64).unwrap_or(800) as i32;
                    weather_icon = w.get("icon").and_then(Value::as_str).unwrap_or("01d");
                    weather_desc = w.get("description").and_then(Value::as_str).unwrap_or("clear sky");
                }
                if let Some(pop) = e.get("pop").and_then(Value::as_f64) {
                    pop_max = pop_max.max(pop);
                }
            }

            // dt for the day (noon)
            let dt = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(12, 0, 0))
                .map(|dt| Utc.from_utc_datetime(&dt).timestamp())
                .unwrap_or(0);

            out.push(serde_json::json!({
                "dt": dt,
                "temp": { "min": min_temp, "max": max_temp },
                "weather": [{ "id": weather_id, "icon": weather_icon, "description": weather_desc }],
                "pop": pop_max,
            }));
        }
    }
    Ok(out)
}

fn normalize_current_weather(v: &Value) -> Result<Value, String> {
    // OpenWeatherMap 2.5/weather returns:
    // { main: { temp, feels_like, humidity, pressure, temp_min, temp_max }, weather: [...], wind: { speed, deg }, visibility, dt, sys: { sunrise, sunset }, ... }
    // One Call 4.0 current expects:
    // { temp, feels_like, humidity, pressure, wind_speed, wind_deg, visibility, dt, weather: [{id, icon, description}], sunrise, sunset, dew_point, uvi, clouds, ... }
    let main = v.get("main").ok_or("missing main")?;
    let weather = v.get("weather").and_then(Value::as_array).and_then(|a| a.first().cloned()).unwrap_or(Value::Null);
    let wind = v.get("wind").unwrap_or(&Value::Null);
    let sys = v.get("sys").unwrap_or(&Value::Null);

    Ok(serde_json::json!({
        "temp": main.get("temp"),
        "feels_like": main.get("feels_like"),
        "humidity": main.get("humidity"),
        "pressure": main.get("pressure"),
        "wind_speed": wind.get("speed"),
        "wind_deg": wind.get("deg"),
        "visibility": v.get("visibility"),
        "dt": v.get("dt"),
        "weather": [weather],
        "sunrise": sys.get("sunrise"),
        "sunset": sys.get("sunset"),
        "dew_point": main.get("temp").and_then(|t| t.as_f64()).map(|t| t - ((100.0 - main.get("humidity").and_then(|h| h.as_f64()).unwrap_or(50.0)) / 5.0)), // rough dew point
        "uvi": Value::Null,
        "clouds": v.get("clouds").and_then(|c| c.get("all")),
    }))
}

fn current_snapshot() -> WeatherSnapshot {
    state().lock().map(|g| g.clone()).unwrap_or_default()
}

fn put_snapshot(snap: WeatherSnapshot) {
    if let Ok(mut g) = state().lock() {
        *g = snap;
    }
}

/// Open the weather forecast popup window, centered under the triggering
/// widget. Mirrors the `calendar`/`volume-popup` transparent-popup recipe
/// (clamp → acrylic → SWP show with NOACTIVATE dropped, §13.10a/b).
#[tauri::command]
pub async fn open_weather(app: tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(WEATHER_LABEL) {
        // Re-show + refocus an already-open window at the new anchor.
        let (cx, cy, cw, ch) =
            window::clamp_to_monitor(x.round() as i32, y.round() as i32, WEATHER_W as i32, WEATHER_H as i32);
        let _ = win.set_size(tauri::PhysicalSize::new(cw as f64, ch as f64));
        let _ = win.set_position(tauri::PhysicalPosition::new(cx as f64, cy as f64));
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || create_weather_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

const WEATHER_W: f64 = 380.0;
const WEATHER_H: f64 = 560.0;

fn create_weather_window(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let (cx, cy, cw, ch) =
        window::clamp_to_monitor(x.round() as i32, y.round() as i32, WEATHER_W as i32, WEATHER_H as i32);

    let win = tauri::WebviewWindowBuilder::new(
        app,
        WEATHER_LABEL,
        tauri::WebviewUrl::App("widgets/weather/window/weather.html".into()),
    )
    .title("Weather")
    .inner_size(cw as f64, ch as f64)
    .min_inner_size(320.0, 440.0)
    .max_inner_size(440.0, 660.0)
    .position(cx as f64, cy as f64)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .always_on_top(true)
    .additional_browser_args("--default-background-color=00000000")
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, WEATHER_LABEL);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

    // Show after the material is applied (no white flash). Drop NOACTIVATE so
    // the popup actually takes foreground (§13.10b). Keep the canonical
    // SWP_NOSIZE|SWP_NOMOVE so the 0,0,0,0 geometry args are ignored.
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = win.set_focus();

    Ok(())
}

/// Geocoding suggestions for the city autocomplete in widget-config.
/// Uses the stored DPAPI-protected API key to query OpenWeatherMap
/// `geo/1.0/direct?q={query}&limit=8`. Returns display names like
/// "London, GB" or "Paris, FR" for the frontend <datalist>.
#[tauri::command]
pub fn weather_geocode_suggestions(query: String) -> Result<Vec<String>, String> {
    if query.trim().len() < 2 {
        return Ok(Vec::new());
    }
    let cfg = config::load();
    let wc = cfg
        .widgets
        .config
        .get("weather")
        .ok_or_else(|| "Weather widget not configured".to_string())?;

    let key_blob = wc
        .get("api_key")
        .and_then(Value::as_str)
        .ok_or_else(|| "API key not set".to_string())?;

    let api_key = secrets::unprotect(key_blob)
        .ok_or_else(|| "Failed to decrypt API key".to_string())?;

    let q = urlencoding_compat(&query);
    let url = format!(
        "https://api.openweathermap.org/geo/1.0/direct?q={q}&limit=8&appid={api_key}"
    );
    let v = http_get(&url)?;
    let arr = v.as_array().ok_or_else(|| "unexpected geocode response".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("");
        let country = item.get("country").and_then(Value::as_str).unwrap_or("");
        let state = item.get("state").and_then(Value::as_str).unwrap_or("");
        let label = if state.is_empty() {
            if country.is_empty() {
                name.to_string()
            } else {
                format!("{name}, {country}")
            }
        } else {
            format!("{name}, {state}, {country}")
        };
        out.push(label);
    }
    Ok(out)
}
