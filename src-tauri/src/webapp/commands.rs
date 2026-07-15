use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::{Manager, WindowEvent};
use tauri::webview::NewWindowResponse;

use crate::webapp::model;
use crate::webapp::webview as wv;

use crate::window;

const LINK_PREFIX: &str = "link-";
pub const LK_OPEN: &str = "lk-open-";
pub const LK_CLOSE: &str = "lk-close-";
pub const LK_RELOAD: &str = "lk-reload-";

const DEBOUNCE_MS: u128 = 600;

fn window_label(id: &str) -> String {
    format!("{}{}", LINK_PREFIX, id)
}

/// Per the Tauri 2 docs, `WebviewWindowBuilder::build()` must run on an async thread,
/// NOT on the main thread via a sync command (which can deadlock on Windows).
/// The pattern below — async command calling `build()` directly — is the documented
/// approach: <https://docs.rs/tauri/2.9.3/tauri/webview/struct.WebviewWindowBuilder.html#method.new>
#[tauri::command]
pub async fn open_link(app: tauri::AppHandle, id: String, x: f64, y: f64) -> Result<(), String> {
    let r = open_link_inner(&app, &id, x, y);
    if let Err(ref e) = r {
        eprintln!("[webapp] open_link failed: {e}");
    }
    r
}

pub fn open_link_inner(app: &tauri::AppHandle, id: &str, x: f64, y: f64) -> Result<(), String> {
    eprintln!("[webapp] open_link_inner id={id} x={x} y={y}");
    let link = model::find_link(id).ok_or_else(|| format!("link '{}' not found", id))?;
    if link.url.trim().is_empty() {
        return Err("link has no url".into());
    }
    eprintln!("[webapp] found link url={}", link.url);

    let lbl = window_label(id);

    if let Some(win) = app.get_webview_window(&lbl) {
        eprintln!("[webapp] reusing existing window {lbl}");
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let w = link.width.max(320) as f64;
    let h = link.height.max(240) as f64;
    let url = link.url.trim().to_string();
    let persistent = link.persistent;
    let label = if link.label.trim().is_empty() {
        url.clone()
    } else {
        link.label.trim().to_string()
    };

    let pos_x = link.pos_x.unwrap_or_else(|| x.round() as i32);
    let pos_y = link.pos_y.unwrap_or_else(|| y.round() as i32);

    create_link_window(app, id, pos_x as f64, pos_y as f64, w, h, &url, &label, persistent)
}

fn create_link_window(
    app: &tauri::AppHandle,
    id: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    url: &str,
    label: &str,
    persistent: bool,
) -> Result<(), String> {
    eprintln!("[webapp] create_link_window id={id} url={url} w={w} h={h} x={x} y={y} persistent={persistent}");
    let lbl = window_label(id);

    let parsed_url = url::Url::parse(url).map_err(|e| format!("invalid url: {e}"))?;
    let lo_scheme = parsed_url.scheme().to_string();
    let lo_host = parsed_url.host_str().unwrap_or("").to_string();

    let (cx, cy, _, _) = window::monitor::clamp_to_monitor(
        x.round() as i32,
        y.round() as i32,
        w as i32,
        h as i32,
    );

    let lbl = window_label(id);

    let win = tauri::WebviewWindowBuilder::new(
        app,
        &lbl,
        tauri::WebviewUrl::External(parsed_url),
    )
    .title(label)
    .inner_size(w, h)
    .position(cx as f64, cy as f64)
    .resizable(true)
    .decorations(false)
    .transparent(false)
    .skip_taskbar(false)
    .visible(true)
    .focused(true)
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
    .on_navigation(move |nav_url| {
        let nav_scheme = nav_url.scheme();
        let nav_host = nav_url.host_str().unwrap_or("");
        let allow = (nav_scheme == lo_scheme && nav_host == lo_host)
            || nav_scheme == "tauri"
            || (nav_scheme == "http" && nav_host == "localhost");
        eprintln!("[webapp] on_navigation allow={allow} url={}", nav_url.as_str());
        if allow {
            return true;
        }
        crate::shared::shell::open_url(nav_url.as_str());
        false
    })
    .on_new_window(move |nav_url, _| {
        eprintln!("[webapp] on_new_window url={}", nav_url.as_str());
        crate::shared::shell::open_url(nav_url.as_str());
        NewWindowResponse::Deny
    })
    .build()
    .map_err(|e| e.to_string())?;
    eprintln!("[webapp] build() succeeded");

    let ev_last_save = Arc::new(Mutex::new(Instant::now()));
    let ev_app = app.clone();
    let ev_id = id.to_string();
    let ev_persist = persistent;
    let ev_ls = ev_last_save.clone();
    let ev_label = lbl.clone();

    win.on_window_event(move |ev| {
        match ev {
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                eprintln!("[webapp] {} Resized {}x{}", ev_label, size.width, size.height);
                let now = Instant::now();
                if let Ok(mut last) = ev_ls.lock() {
                    if now.duration_since(*last).as_millis() >= DEBOUNCE_MS {
                        *last = now;
                        drop(last);
                        save_link_window_state(&ev_app, &ev_id);
                    }
                }
            }
            WindowEvent::Moved(pos) => {
                eprintln!("[webapp] {} Moved {}x{}", ev_label, pos.x, pos.y);
                let now = Instant::now();
                if let Ok(mut last) = ev_ls.lock() {
                    if now.duration_since(*last).as_millis() >= DEBOUNCE_MS {
                        *last = now;
                        drop(last);
                        save_link_window_state(&ev_app, &ev_id);
                    }
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                eprintln!("[webapp] {} CloseRequested persistent={}", ev_label, ev_persist);
                // TEMP DEBUG: always prevent close to see if the window survives
                api.prevent_close();
                save_link_window_state(&ev_app, &ev_id);
                if ev_persist {
                    if let Some(c) = ev_app.get_webview_window(&ev_label) {
                        let _ = c.hide();
                    }
                }
            }
            _ => {}
        }
    });
    eprintln!("[webapp] on_window_event registered");

    let _ = window::set_rounded_corners(&win);
    eprintln!("[webapp] set_rounded_corners done");

    let _ = win.set_focus();
    eprintln!("[webapp] create_link_window done");
    Ok(())
}

fn save_link_window_state(app: &tauri::AppHandle, id: &str) {
    eprintln!("[webapp] save_link_window_state id={id}");
    let Some(win) = app.get_webview_window(&window_label(id)) else { eprintln!("[webapp] save: window not found"); return };
    let Ok(pos) = win.outer_position() else { eprintln!("[webapp] save: outer_position failed"); return };
    let Ok(size) = win.outer_size() else { eprintln!("[webapp] save: outer_size failed"); return };
    eprintln!("[webapp] save: pos={}x{} size={}x{}", pos.x, pos.y, size.width, size.height);

    let mut cfg = crate::config::load();
    if let Some(links_map) = cfg.widgets.config.get_mut("links") {
        if let Some(arr) = links_map
            .get_mut("links")
            .and_then(|v| v.as_array_mut())
        {
            for item in arr.iter_mut() {
                if let Some(obj) = item.as_object_mut() {
                    if obj.get("id").and_then(|v| v.as_str()) == Some(id) {
                        obj.insert("width".into(), serde_json::json!(size.width));
                        obj.insert("height".into(), serde_json::json!(size.height));
                        obj.insert("pos_x".into(), serde_json::json!(pos.x));
                        obj.insert("pos_y".into(), serde_json::json!(pos.y));
                        break;
                    }
                }
            }
        }
    }
    let _ = crate::config::repository::save(&cfg);
}

#[tauri::command]
pub fn close_link(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&window_label(&id)) {
        let _ = win.destroy();
    }
    Ok(())
}

#[tauri::command]
pub fn reload_link(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&window_label(&id)) {
        let _ = win.eval("location.reload();");
    }
    Ok(())
}

#[tauri::command]
pub fn show_link_menu(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let bar = app.get_webview_window("bar").ok_or("bar window not found")?;
    let menu = build_link_menu(&app, &id)?;
    bar.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

fn build_link_menu(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<tauri::menu::Menu<tauri::Wry>, String> {
    let open_id = format!("{}{}", LK_OPEN, id);
    let reload_id = format!("{}{}", LK_RELOAD, id);
    let close_id = format!("{}{}", LK_CLOSE, id);
    let open = MenuItemBuilder::with_id(open_id, "Open").build(app).map_err(|e| e.to_string())?;
    let reload = MenuItemBuilder::with_id(reload_id, "Reload").build(app).map_err(|e| e.to_string())?;
    let close = MenuItemBuilder::with_id(close_id, "Close").build(app).map_err(|e| e.to_string())?;
    MenuBuilder::new(app)
        .item(&open)
        .item(&reload)
        .item(&close)
        .build()
        .map_err(|e| e.to_string())
}

pub fn handle_link_menu_event(app: &tauri::AppHandle, id: &str) {
    if let Some(wid) = id.strip_prefix(LK_OPEN) {
        let _ = open_link_inner(app, wid, 0.0, 0.0);
    } else if let Some(wid) = id.strip_prefix(LK_RELOAD) {
        let _ = reload_link(app.clone(), wid.to_string());
    } else if let Some(wid) = id.strip_prefix(LK_CLOSE) {
        let _ = close_link(app.clone(), wid.to_string());
    }
}
