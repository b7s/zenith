use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::{Manager, WindowEvent};

use crate::webapp::model;

use crate::window;

const LINK_PREFIX: &str = "link-";
pub const LK_OPEN: &str = "lk-open-";
pub const LK_CLOSE: &str = "lk-close-";
pub const LK_RELOAD: &str = "lk-reload-";

const DEBOUNCE_MS: u128 = 600;

fn window_label(id: &str) -> String {
    format!("{}{}", LINK_PREFIX, id)
}

/// Build the WebView2 window on a dedicated thread (same proven pattern as
/// `create_settings_window` / `create_widgets_window` in `commands.rs`: an
/// `App`-URL window built via `spawn_blocking`, revealed with `SetWindowPos`
/// after construction). The external site is loaded by the `widgets/webapp/window/webapp-window.html`
/// loader page via `window.location.replace`, so we never build a window with
/// a `WebviewUrl::External` target — that path deadlocks on the main thread
/// and hides the window when built off it.
#[tauri::command]
pub async fn open_link(app: tauri::AppHandle, id: String, x: f64, y: f64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_link_inner(&app, &id, x, y))
        .await
        .map_err(|e| e.to_string())?
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
        // A hidden acrylic window loses its `SetWindowCompositionAttribute`
        // accent when re-shown, so it would appear invisible. Reapply the
        // material (and corners) before showing. Also un-minimize in case it
        // was tucked away by a persistent close.
        let _ = window::apply_fixed_acrylic(app, &lbl);
        let _ = window::set_rounded_corners(&win);
        let _ = win.unminimize();
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

/// Load the link's icon from disk as a Tauri `Image`. Returns None when no
/// icon is configured for this link (caller falls back to the default
/// window icon). See `webapp::icons` for storage / format details.
fn link_icon_image(id: &str) -> Option<tauri::image::Image<'static>> {
    crate::webapp::icons::load_link_icon_image(id)
}

#[allow(clippy::too_many_arguments)]
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

    let (cx, cy, _, _) = window::monitor::clamp_to_monitor(
        x.round() as i32,
        y.round() as i32,
        w as i32,
        h as i32,
    );

    let lbl = window_label(id);

    // The loader page reads `__ZENITH_LINK_URL` (set below via an init script,
    // before any page script runs) and does `window.location.replace(url)`.
    let init_js = format!(
        "window.__ZENITH_LINK_URL = {}; window.__ZENITH_LINK_TITLE = {};",
        serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(&label).unwrap_or_else(|_| "\"\"".into()),
    );

    let win = tauri::WebviewWindowBuilder::new(app, &lbl, tauri::WebviewUrl::App("widgets/webapp/window/webapp-window.html".into()))
        .inner_size(w, h)
        .position(cx as f64, cy as f64)
        .resizable(true)
        .decorations(true)
        .transparent(true)
        .visible(false)
        .focused(true)
        .title(label)
        .additional_browser_args("--default-background-color=00000000")
        .initialization_script(&init_js)
        .build()
        .map_err(|e| e.to_string())?;
    eprintln!("[webapp] build() succeeded");

    // Use the link's configured icon (loaded from disk as PNG); fall back to
    // Zenith's default icon when none is set. See `webapp::icons` for why
    // icons are stored on disk instead of inline in config.json.
    match link_icon_image(id) {
        Some(img) => {
            let _ = win.set_icon(img);
            eprintln!("[webapp] icon set from disk");
        }
        None => {
            if let Some(def) = app.default_window_icon() {
                let _ = win.set_icon(def.clone());
                eprintln!("[webapp] icon set from default (fallback)");
            }
        }
    }

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
                save_link_window_state(&ev_app, &ev_id);
                if ev_persist {
                    // Keep the window alive (don't let the close destroy it)...
                    api.prevent_close();
                    // ...but get it off-screen. `hide()` inside/after the close
                    // flow is overridden by `prevent_close()`, so minimize
                    // instead — a normal window state the close handling leaves
                    // alone. The bar button re-shows it (with acrylic reapplied).
                    let min_label = ev_label.clone();
                    let min_app = ev_app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        if let Some(c) = min_app.get_webview_window(&min_label) {
                            let r = c.minimize();
                            eprintln!("[webapp] {} minimize -> {:?}", min_label, r);
                        }
                    });
                }
            }
            _ => {}
        }
    });
    eprintln!("[webapp] on_window_event registered");

    let _ = window::apply_fixed_acrylic(app, &lbl);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);
    eprintln!("[webapp] acrylic+corners done");

    // Reveal AFTER the window is fully built (§13.10b). The window was built
    // `visible(false)`; `decorations(true)` gives a native OS title bar so the
    // user can move and close the window.
    // The position/size are passed explicitly to `SetWindowPos` rather than
    // relying on the builder's `inner_size`/`position` — those may not have
    // been processed by the main-thread event loop yet when `SetWindowPos`
    // runs from a `spawn_blocking` thread, causing the window to appear at a
    // default size (e.g. 160×28).
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER,
    };
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(hwnd, None, cx, cy, w as i32, h as i32, SWP_SHOWWINDOW | SWP_NOZORDER)
    };
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
                        // Only persist sizes that the user could have actually
                        // chosen via window resize — transient 0/tiny values
                        // during initial creation or minimize must never
                        // overwrite the user-configured dimensions.
                        if size.width > 100 && size.height > 100 {
                            obj.insert("width".into(), serde_json::json!(size.width));
                            obj.insert("height".into(), serde_json::json!(size.height));
                        }
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

/// Persist a link's icon to disk as PNG (converted from any supported source
/// format: PNG/JPEG/WebP/GIF/BMP/ICO). Called from the widget-config window
/// on Save — icons never go in config.json (which would bloat every load).
#[tauri::command]
pub fn save_link_icon(id: String, data_url: String) -> Result<bool, String> {
    crate::webapp::icons::save_link_icon(&id, &data_url)?;
    Ok(true)
}

/// Remove a link's icon from disk. Idempotent — no-op when no icon exists.
#[tauri::command]
pub fn delete_link_icon(id: String) -> Result<(), String> {
    crate::webapp::icons::delete_link_icon(&id)
}

/// Return the link's icon as a `data:image/png;base64,...` URL for use as
/// `<img src>` in the bar widget. None when the link has no configured icon.
#[tauri::command]
pub fn get_link_icon_data(id: String) -> Option<String> {
    crate::webapp::icons::read_link_icon_data_url(&id)
}
