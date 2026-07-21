mod events;
mod commands;
mod battery;
mod calendar;
mod calendar_sync;
mod color_picker;
mod config;
mod git;
mod log;
mod media;
mod quick_toggle;
mod shared;
mod shutdown;
mod system_stats;
mod tray;
mod updates;
mod volume;
mod weather;
mod widgets;
mod window;
mod workspace;
mod webapp;
mod ai_cli;

use tauri::{Emitter, Listener, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            weather::commands::open_weather,
            weather::commands::weather_refresh,
            weather::commands::weather_get_cache,
            weather::commands::weather_geocode_suggestions,
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
            webapp::commands::open_link,
            webapp::commands::close_link,
            webapp::commands::reload_link,
            webapp::commands::show_link_menu,
            webapp::commands::save_link_icon,
            webapp::commands::delete_link_icon,
            webapp::commands::get_link_icon_data,
            updates::get_update_status,
            updates::check_update,
            updates::open_releases_page,
            calendar_sync::commands::calendar_accounts_list,
            calendar_sync::commands::calendar_connect,
            calendar_sync::commands::calendar_poll_auth,
            calendar_sync::commands::calendar_abort_auth,
            calendar_sync::commands::calendar_disconnect,
            calendar_sync::commands::calendar_sync_now,
            calendar_sync::commands::calendar_save_accounts,
            calendar_sync::commands::calendar_set_enabled,
            color_picker::commands::start_eyedropper,
            color_picker::commands::eyedropper_pixel,
            color_picker::commands::get_eyedropper_frames,
            color_picker::commands::end_eyedropper,
            color_picker::commands::open_eyedropper,
            color_picker::commands::open_color_picker,
            color_picker::commands::get_cursor_position,
            color_picker::commands::read_live_pixel,
            ai_cli::commands::get_ai_cli_state,
            ai_cli::commands::detect_ai_clis,
            ai_cli::commands::install_ai_cli_hooks,
            ai_cli::commands::uninstall_ai_cli_hooks,
            ai_cli::commands::ack_ai_cli_failures,
            ai_cli::commands::open_ai_cli_manager,
        ])
        .setup(|app| {
            // Initialize COM once for the main thread (used by workspace domain)
            unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
            let handle = app.handle().clone();
            crate::shared::set_app_handle(handle.clone());

            // Reconcile OS autostart with the persisted config intent
            // (defaults to true on first run).
            commands::sync_start_with_windows(&handle);

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
            // thread (every 24h).
            events::repository::startup_sync();
            events::alarm_fire::spawn(handle.clone());
            events::cleanup::spawn(handle.clone());

            // Webapp icons — migrate any legacy `data:` URLs in config.json
            // to disk (one PNG per link), then clear the field. Idempotent.
            webapp::icons::migrate_legacy_data_urls();

            // Media widget — poll SMTC current-session state every 2s and
            // emit `zenith:media-changed` when it actually changes.
            media::listen::spawn(handle.clone());

            // Git manager widget — sequential per-account HTTPS fan-out
            // on a worker thread; emits `zenith:git-changed` on totals
            // change. Sleeps 30s between cycles; per-account poll_mins
            // governs which accounts actually fire on a given cycle.
            git::listen::spawn(handle.clone());

            // AI CLI widget — start the bridge HTTP server for hook
            // events from claude/codex, and the opencode SSE client.
            // The server writes its port to %APPDATA%/zenith/ai-cli-bridge.json.
            let agg = ai_cli::aggregator();
            ai_cli::server::spawn(agg.clone());
            ai_cli::opencode_client::spawn(agg);

            // Calendar sync — background periodic sync of connected Google /
            // Outlook accounts into the shared events store.
            calendar_sync::poll::start();

            // Update checker — background 24h poll (gated by config.auto_update).
            updates::spawn(handle.clone());

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
