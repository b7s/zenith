mod events;
mod commands;
mod battery;
mod calendar;
mod calendar_sync;
mod config;
mod git;
mod log;
mod media;
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
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::open_settings,
            commands::open_widgets,
            commands::set_start_with_windows,
            commands::is_start_with_windows,
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
            volume::commands::get_app_sessions,
            volume::commands::set_app_volume,
            volume::commands::set_app_muted,
            volume::commands::open_volume_popup,
            calendar::commands::open_calendar,
            calendar::commands::get_calendar_view,
            calendar::commands::get_calendar_single,
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
            events::commands::get_events,
            events::commands::add_event,
            events::commands::update_event,
            events::commands::delete_event,
            events::commands::sync_events,
            media::commands::get_media,
            media::commands::media_play,
            media::commands::media_pause,
            media::commands::media_toggle_play_pause,
            media::commands::media_next,
            media::commands::media_previous,
            media::commands::media_seek,
            git::commands::open_git_manager,
            git::commands::get_git_state,
            git::commands::git_refresh,
            git::commands::protect_secret,
            git::commands::unprotect_secret_for_selftest,
            git::commands::get_git_selected_account,
            git::commands::get_git_widget_config,
            git::commands::open_url,
            git::commands::send_to_ai,
            git::commands::fetch_git_content,
            calendar_sync::commands::calendar_accounts_list,
            calendar_sync::commands::calendar_connect,
            calendar_sync::commands::calendar_poll_auth,
            calendar_sync::commands::calendar_abort_auth,
            calendar_sync::commands::calendar_disconnect,
            calendar_sync::commands::calendar_sync_now,
            calendar_sync::commands::calendar_save_accounts,
            calendar_sync::commands::calendar_set_enabled,
        ])
        .setup(|app| {
            // Initialize COM once for the main thread (used by workspace domain)
            unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
            let handle = app.handle().clone();
            crate::shared::set_app_handle(handle.clone());
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

            // Events domain — startup sync picks newer of local vs OneDrive,
            // then spawn the alarm-firing thread (every 30s) and the cleanup
            // thread (every 12h).
            events::repository::startup_sync(&handle);
            events::alarm_fire::spawn(handle.clone());
            events::cleanup::spawn(handle.clone());

            // Media widget — poll SMTC current-session state every 2s and
            // emit `zenith:media-changed` when it actually changes.
            media::listen::spawn(handle.clone());

            // Git manager widget — sequential per-account HTTPS fan-out
            // on a worker thread; emits `zenith:git-changed` on totals
            // change. Sleeps 30s between cycles; per-account poll_mins
            // governs which accounts actually fire on a given cycle.
            git::listen::spawn(handle.clone());

            // Calendar sync — background periodic sync of connected Google /
            // Outlook accounts into the shared events store.
            calendar_sync::poll::start();

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
