#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn desktop_platform() -> &'static str {
    std::env::consts::OS
}
