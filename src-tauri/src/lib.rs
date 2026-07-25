mod data_paths;
mod commands;
#[cfg(target_os = "linux")]
mod linux_fix;
#[cfg(target_os = "macos")]
mod menu;
mod panic_hook;
mod settings;
mod tray;
mod window;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    panic_hook::install();

    let builder = tauri::Builder::default();

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(menu::build_app_menu)
        .on_menu_event(menu::handle_menu_event);

    #[cfg(not(feature = "e2e"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        window::focus_main_window(app);
    }));

    let app = builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                        path: data_paths::logs_dir(),
                        file_name: Some("harness-lens".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::restart_app,
            commands::set_window_theme,
            commands::desktop_platform,
            commands::set_tray_menu_labels,
            settings::load_settings,
            settings::save_setting
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window::persist_main_window(window.app_handle());

                if let Err(error) = window.hide() {
                    log::error!("failed to hide main window on close: {error}");
                }
            }
        })
        .setup(|app| {
            let app_handle = app.handle();
            tray::setup(app_handle)?;
            window::restore_main_window(app_handle);
            window::schedule_main_window_bounds_clamp(app_handle);
            #[cfg(not(feature = "e2e"))]
            window::focus_main_window(app_handle);
            #[cfg(target_os = "macos")]
            window::apply_macos_native_titlebar(app_handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HarnessLens");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            window::persist_main_window(app_handle);
        }
    });
}
