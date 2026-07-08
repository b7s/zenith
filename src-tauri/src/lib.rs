mod commands;
mod battery;
mod calendar;
mod config;
mod log;
mod quick_toggle;
mod shared;
mod shutdown;
mod system_stats;
mod tray;
mod volume;
mod widgets;
mod window;
mod workspace;

use tauri::{Emitter, Listener, Manager};

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::open_settings,
            commands::open_widgets,
            commands::show_context_menu,
            commands::show_workspace_context_menu,
            commands::show_dialog,
            commands::get_dialog_data,
            commands::open_widget_config,
            config::commands::get_config,
            config::commands::save_config,
            widgets::commands::get_widgets,
            widgets::commands::get_widget_source,
            log::log_write,
            log::log_clear,
            workspace::commands::get_workspaces,
            workspace::commands::get_active_workspace,
            workspace::commands::switch_workspace,
            workspace::commands::move_window_to_desktop,
            workspace::commands::create_desktop,
            workspace::commands::delete_desktop,
            workspace::commands::rename_desktop,
            workspace::commands::toggle_pin_window,
            volume::commands::get_volume,
            volume::commands::set_volume,
            volume::commands::set_muted,
            volume::commands::open_volume_popup,
            calendar::commands::open_calendar,
            shutdown::commands::system_shutdown,
            shutdown::commands::system_restart,
            shutdown::commands::system_sleep,
            shutdown::commands::system_hibernate,
            shutdown::commands::system_lock,
            shutdown::commands::system_logout,
            shutdown::commands::open_shutdown_popup,
            battery::commands::get_battery_status,
            system_stats::commands::get_system_stats,
            quick_toggle::commands::toggle_wifi,
            quick_toggle::commands::toggle_bluetooth,
            quick_toggle::commands::toggle_dark_mode,
            quick_toggle::commands::toggle_focus_assist,
            quick_toggle::commands::toggle_airplane,
            quick_toggle::commands::toggle_night_light,
            quick_toggle::commands::get_quick_toggle_status,
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

            // Initial workspace emit
            let h2 = handle.clone();
            let _ = h2.emit(crate::shared::EVENT_WORKSPACE_CHANGED, workspace::commands::get_active_workspace());

            // Start COM notification listener for instant external switch detection
            workspace::notification::setup(handle.clone());

            // Install EVENT_SYSTEM_FOREGROUND hook to track last real foreground window
            workspace::foreground::install();

            // Start an explorer-restart watcher that re-registers the AppBar
            // when explorer.exe crashes and restarts (broadcasts TaskbarCreated).
            let h3 = handle.clone();
            window::appbar_monitor::install(handle.clone());
            handle.listen("zenith:appbar-restore", move |_event| {
                eprintln!("[zenith:appbar] explorer restarted → re-registering AppBar");
                if let Some(bar) = h3.get_webview_window("bar") {
                    let _ = window::register_appbar(&bar);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Zenith");
}
