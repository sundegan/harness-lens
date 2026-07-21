mod commands;
#[cfg(target_os = "linux")]
mod linux_fix;
#[cfg(target_os = "macos")]
mod menu;
mod window;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

    #[cfg(not(feature = "e2e"))]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    let app = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::restart_app,
            commands::set_window_theme,
            commands::desktop_platform
        ])
        .setup(|app| {
            let app_handle = app.handle();
            window::schedule_main_window_bounds_clamp(app_handle);
            #[cfg(not(feature = "e2e"))]
            window::focus_main_window(app_handle);
            #[cfg(target_os = "macos")]
            window::apply_macos_native_titlebar(app_handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Codex Timeline");

    app.run(move |app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::WindowEvent { label, event, .. } = event {
            if label == "main" {
                match event {
                    tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        window::reposition_macos_native_traffic_lights(app_handle);
                    }
                    _ => {}
                }
            }
        }
    });
}
