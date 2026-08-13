mod avatars;
mod backups;
mod catalog;
mod commands;
mod console;
mod databases;
mod db;
mod ephemeral;
mod error;
mod files;
mod java;
mod models;
mod modpacks;
mod mods;
mod operations;
mod paths;
mod plugins;
mod process;
mod properties;
mod recycle;
mod sharing;
mod state;
mod tasks;
mod worlds;

use std::sync::Arc;

use models::AppEvent;
use state::AppState;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(AppState::new(handle.clone()))
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            tasks::spawn_background_tasks(state.clone());
            app.manage(state);

            let show = MenuItemBuilder::with_id("show", "Show Nooki").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
            TrayIconBuilder::with_id("main-tray")
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .expect("Nooki has a window icon"),
                )
                .tooltip("Nooki")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, tauri::tray::TrayIconEvent::DoubleClick { .. }) {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => request_quit(app),
                    _ => {}
                })
                .build(app)?;

            if std::env::args().any(|argument| argument == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<Arc<AppState>>();
                let minimize = tauri::async_runtime::block_on(async {
                    state.settings.read().await.minimize_to_tray
                });
                let running = tauri::async_runtime::block_on(async {
                    let ids = state
                        .servers
                        .read()
                        .await
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>();
                    let mut count = 0;
                    for id in ids {
                        if state.processes.is_running(&id).await {
                            count += 1;
                        }
                    }
                    count
                });
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                } else if running > 0 {
                    api.prevent_close();
                    state.emit(AppEvent::QuitRequested {
                        running_servers: running,
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::initialize,
            commands::list_software_versions,
            commands::scan_server_folder,
            commands::create_server,
            commands::import_server,
            commands::server_action,
            commands::send_console_command,
            commands::save_server_settings,
            commands::dismiss_server_alert,
            commands::player_action,
            commands::create_backup,
            commands::restore_backup,
            commands::delete_backup,
            commands::save_backup_schedule,
            commands::remove_server,
            commands::detect_java_runtimes,
            commands::install_java_runtime,
            commands::remove_java_runtime,
            commands::list_log_sessions,
            commands::read_log_session,
            commands::export_log,
            commands::save_app_settings,
            commands::activate_relay,
            commands::reveal_path,
            commands::check_server_updates,
            commands::change_server_software,
            commands::cancel_operation,
            commands::load_server_icon,
            avatars::load_player_avatar,
            commands::quit_application,
            databases::database_environment,
            databases::list_databases,
            databases::create_database,
            databases::database_action,
            databases::delete_database,
            worlds::list_worlds,
            worlds::save_world_settings,
            worlds::regenerate_world,
            worlds::delete_world,
            ephemeral::scan_ephemeral_world,
            ephemeral::create_ephemeral_server,
            files::list_server_files,
            files::read_server_text_file,
            files::save_server_text_file,
            files::create_server_file,
            files::create_server_folder,
            files::rename_server_file,
            files::delete_server_file,
            plugins::list_plugins,
            plugins::set_plugin_enabled,
            plugins::delete_plugin,
            plugins::add_plugin_files,
            plugins::search_plugins,
            plugins::load_plugin_icon,
            plugins::list_plugin_versions,
            plugins::install_plugin,
            mods::list_mods,
            mods::set_mod_enabled,
            mods::delete_mod,
            mods::add_mod_files,
            mods::search_mods,
            mods::load_mod_icon,
            mods::list_mod_versions,
            mods::install_mod,
            mods::check_manual_mod_download,
            mods::cancel_manual_mod_download,
            mods::open_manual_mod_download,
            modpacks::search_modpacks,
            modpacks::list_modpack_versions,
            modpacks::create_modpack_server,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running Nooki");
}

fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let ids = tauri::async_runtime::block_on(async {
        state
            .servers
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    });
    let running = tauri::async_runtime::block_on(async {
        let mut count = 0;
        for id in ids {
            if state.processes.is_running(&id).await {
                count += 1;
            }
        }
        count
    });
    if running == 0 {
        app.exit(0);
    } else {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        state.emit(AppEvent::QuitRequested {
            running_servers: running,
        });
    }
}
