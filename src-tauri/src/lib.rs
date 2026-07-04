mod commands;
mod config;
mod shared;
mod tray;
mod window;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            config::commands::get_config,
            config::commands::save_config,
            commands::open_settings,
            commands::open_widgets,
        ])
        .setup(|app| {
            let handle = app.handle();
            if let Some(bar) = handle.get_webview_window("bar") {
                let _ = window::apply_material(handle, "bar");
                let _ = window::set_rounded_corners(&bar);
                let _ = window::register_appbar(&bar);
            }

            let _ = tray::create(handle);

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_dark = window::is_dark_mode();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let dark = window::is_dark_mode();
                    if dark != last_dark {
                        last_dark = dark;
                        window::apply_material(&handle, "bar").ok();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Zenith");
}
