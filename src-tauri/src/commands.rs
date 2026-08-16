#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn set_window_theme(window: tauri::WebviewWindow, is_dark: bool) -> Result<(), String> {
    let theme = if is_dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    window
        .set_theme(Some(theme))
        .map_err(|error| error.to_string())?;
    window
        .set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    crate::window::apply_macos_window_theme(&window, is_dark)?;

    Ok(())
}

#[tauri::command]
pub fn desktop_platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
pub fn get_database_runtime_status(
    database: tauri::State<'_, crate::database::DatabaseRuntime>,
) -> crate::database::DatabaseRuntimeStatus {
    database.status()
}

#[tauri::command]
pub fn set_tray_menu_labels(
    app: tauri::AppHandle,
    labels: crate::tray::TrayMenuLabels,
) -> Result<(), String> {
    crate::tray::update_menu(&app, labels)
}
