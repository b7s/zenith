mod commands;
mod config;
mod log;
mod shared;
mod tray;
mod widgets;
mod window;

use tauri::Manager;

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
        ])
        .setup(|app| {
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Zenith");
}
