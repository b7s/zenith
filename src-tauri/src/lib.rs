mod commands;
mod config;
mod log;
mod shared;
mod tray;
mod widgets;
mod window;
mod workspace;

use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::open_settings,
            commands::open_widgets,
            commands::show_context_menu,
            config::commands::get_config,
            config::commands::save_config,
            widgets::commands::get_widgets,
            widgets::commands::get_widget_source,
            log::log_write,
            log::log_clear,
            workspace::commands::get_workspaces,
            workspace::commands::get_active_workspace,
            workspace::commands::switch_workspace,
        ])
        .setup(|app| {
            // Initialize COM once for the main thread (used by workspace domain)
            unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
            let handle = app.handle().clone();
            if let Some(bar) = handle.get_webview_window("bar") {
                let h = handle.clone();
                bar.on_menu_event(move |_window, event| {
                    commands::handle_menu_event(&h, event.id().as_ref());
                });
                window::apply_material(&handle, "bar").ok();
                window::register_appbar(&bar).ok(); // also shows the window via SWP_SHOWWINDOW

                // Unregister the AppBar when the window is destroyed so the work area is restored.
                let bar_clone = bar.clone();
                bar.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Destroyed) {
                        window::unregister_appbar(&bar_clone);
                    }
                });
            }

            let _ = tray::create(&handle);

            let h = handle.clone();
            std::thread::spawn(move || {
                let mut last_dark = window::is_dark_mode();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    if h.get_webview_window("bar").is_none() {
                        continue;
                    }
                    let dark = window::is_dark_mode();
                    if dark != last_dark {
                        last_dark = dark;
                        let _ = window::apply_material(&h, "bar");
                    }
                }
            });

            let h2 = handle.clone();
            std::thread::spawn(move || {
                unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
                let mut last_active = workspace::commands::get_active_workspace();
                let _ = h2.emit(crate::shared::EVENT_WORKSPACE_CHANGED, last_active);
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if h2.get_webview_window("bar").is_none() {
                        continue;
                    }
                    let active = workspace::commands::get_active_workspace();
                    if active != last_active {
                        eprintln!("[zenith:ws] external switch detected: {} -> {}", last_active, active);
                        last_active = active;
                        let _ = h2.emit(crate::shared::EVENT_WORKSPACE_CHANGED, active);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Zenith");
}
