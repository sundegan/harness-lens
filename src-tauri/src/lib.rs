mod analytics;
mod commands;
mod data_paths;
pub mod database;
#[cfg(feature = "e2e")]
mod e2e_seed;
#[cfg(target_os = "linux")]
mod linux_fix;
#[cfg(target_os = "macos")]
mod menu;
mod panic_hook;
mod settings;
mod tray;
mod window;

#[cfg(not(feature = "e2e"))]
use std::path::PathBuf;
#[cfg(not(feature = "e2e"))]
use std::thread;

use tauri::Manager;

const AUTOSTART_ARGUMENT: &str = "--autostart";

fn launched_at_login() -> bool {
    std::env::args().any(|argument| argument == AUTOSTART_ARGUMENT)
}

#[cfg(not(feature = "e2e"))]
fn start_database_initialization(
    runtime: database::DatabaseRuntime,
    database_path: PathBuf,
) -> std::io::Result<()> {
    thread::Builder::new()
        .name("harness-lens-database".to_owned())
        .spawn(
            move || match database::Database::initialize(database_path) {
                Ok(database) => {
                    if let Err(error) = database.validate_integrity() {
                        log::error!(
                            "background analytics database integrity check failed: {error}"
                        );
                        runtime.set_result(Err(error.to_string()));
                        return;
                    }
                    runtime.set_result(Ok(database));
                    log::info!("analytics database is ready");
                }
                Err(error) => {
                    log::error!("failed to initialize the analytics database: {error}");
                    runtime.set_result(Err(error.to_string()));
                }
            },
        )
        .map(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    panic_hook::install();

    let builder = tauri::Builder::default().plugin(
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
    );

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
            tauri_plugin_autostart::Builder::new()
                .args([AUTOSTART_ARGUMENT])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::restart_app,
            commands::set_window_theme,
            commands::desktop_platform,
            commands::get_database_runtime_status,
            commands::set_tray_menu_labels,
            analytics::get_skill_analysis,
            analytics::get_sync_status,
            analytics::get_session_page,
            analytics::get_session_detail,
            analytics::get_tool_call_analysis,
            analytics::get_tool_call_filter_options,
            analytics::get_tool_call_page,
            analytics::get_tool_call_detail,
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

                window::hide_main_window(window.app_handle());
            }
        })
        .setup(|app| {
            let app_handle = app.handle();
            let database_path = data_paths::database_path();
            let launched_at_login = launched_at_login();
            #[cfg(feature = "e2e")]
            {
                e2e_seed::reset_database_files(&database_path)?;
                let database = database::Database::initialize(&database_path)?;
                e2e_seed::seed(&database)?;
                app.manage(database::DatabaseRuntime::ready(database));
            }
            #[cfg(not(feature = "e2e"))]
            {
                let database_runtime = database::DatabaseRuntime::pending();
                app.manage(database_runtime.clone());
                app.manage(analytics::AgentDataMonitor::start(
                    database_runtime.clone(),
                    app_handle.clone(),
                )?);
                start_database_initialization(database_runtime, database_path)?;
            }
            tray::setup(app_handle)?;
            if launched_at_login {
                window::enter_background_mode(app_handle);
            } else {
                window::restore_main_window(app_handle);
            }
            window::schedule_main_window_bounds_clamp(app_handle);
            if !launched_at_login {
                #[cfg(not(feature = "e2e"))]
                window::focus_main_window(app_handle);
            }
            #[cfg(target_os = "macos")]
            window::apply_macos_native_titlebar(app_handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HarnessLens");

    app.run(|app_handle, event| match event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            // macOS reuses the resident process when the user opens the app
            // from the Dock or Finder instead of starting a second instance.
            window::focus_main_window(app_handle);
        }
        tauri::RunEvent::ExitRequested { code, api, .. } => {
            window::persist_main_window(app_handle);

            // Ordinary user exits stay resident; only the explicit tray Quit and
            // Tauri's reserved restart request may terminate this process.
            let is_restart = code == Some(tauri::RESTART_EXIT_CODE);
            if !is_restart && !tray::consume_exit_authorization(code) {
                api.prevent_exit();
                window::enter_background_mode(app_handle);
            }
        }
        _ => {}
    });
}
