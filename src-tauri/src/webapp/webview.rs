//! WebView2 title-watch helper for the webapp link windows.
//!
//! Used to surface unread/notification badges via the `zenith:link-notification`
//! event. Kept as the single owner of this logic; wired back once the link
//! window is stable.

#![allow(dead_code)]

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::DocumentTitleChangedEventHandler;
use windows::core::*;
use windows::Win32::System::Com::CoTaskMemFree;

use tauri::{AppHandle, Emitter};
use tauri::webview::PlatformWebview;

use crate::shared::EVENT_LINK_NOTIFICATION;

pub fn parse_badge(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return false;
    }
    if t.contains('\u{25CF}') {
        return true;
    }
    if let Some(rest) = t.strip_prefix('(') {
        if let Some(num) = rest.split(')').next() {
            if let Ok(n) = num.trim().parse::<i32>() {
                return n > 0;
            }
        }
    }
    let lower = t.to_lowercase();
    if lower.contains("unread") || lower.contains("new message") {
        if let Some(n) = t
            .split_whitespace()
            .next()
            .and_then(|w| w.parse::<i32>().ok())
        {
            return n > 0;
        }
        return true;
    }
    false
}

pub fn watch_title(wv: PlatformWebview, id: String, app: AppHandle) {
    let controller = wv.controller();
    if let Ok(core) = unsafe { controller.CoreWebView2() } {
        let handler = DocumentTitleChangedEventHandler::create(Box::new(
            move |sender: Option<ICoreWebView2>, _: Option<IUnknown>| {
                if let Some(ref sender) = sender {
                    let mut title = PWSTR::null();
                    if unsafe { sender.DocumentTitle(&mut title) }.is_ok() {
                        let title_str = unsafe { title.to_string() }.unwrap_or_default();
                        if !title.is_null() {
                            unsafe {
                                CoTaskMemFree(Some(title.as_ptr() as *const core::ffi::c_void))
                            };
                        }
                        let has = parse_badge(&title_str);
                        let _ = app.emit(
                            EVENT_LINK_NOTIFICATION,
                            serde_json::json!({ "id": id, "has": has }),
                        );
                    }
                }
                Ok(())
            },
        ));
        let mut token = 0i64;
        let _ = unsafe { core.add_DocumentTitleChanged(&handler, &mut token as *mut i64) };
    }
}
