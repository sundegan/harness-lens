use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};

use crate::window;

const ABOUT_MENU_ID: &str = "about";
const SHOW_ABOUT_EVENT: &str = "show-about";
const SETTINGS_MENU_ID: &str = "settings";
const OPEN_SETTINGS_EVENT: &str = "open-settings";
const CHECK_UPDATES_MENU_ID: &str = "check-updates";
const CHECK_UPDATES_EVENT: &str = "check-for-updates";

pub fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let about_item =
        MenuItem::with_id(app, ABOUT_MENU_ID, "About HarnessLens", true, None::<&str>)?;

    let check_updates_item = MenuItem::with_id(
        app,
        CHECK_UPDATES_MENU_ID,
        "Check for Updates...",
        true,
        None::<&str>,
    )?;

    let settings_item = MenuItem::with_id(
        app,
        SETTINGS_MENU_ID,
        "Settings...",
        true,
        Some("CmdOrCtrl+,"),
    )?;

    let app_menu = SubmenuBuilder::new(app, "HarnessLens")
        .item(&about_item)
        .item(&settings_item)
        .item(&check_updates_item)
        .separator()
        .hide_with_text("Hide HarnessLens")
        .separator()
        .quit_with_text("Quit HarnessLens")
        .build()?;

    MenuBuilder::new(app).item(&app_menu).build()
}

pub fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let event_name = match event.id().as_ref() {
        ABOUT_MENU_ID => SHOW_ABOUT_EVENT,
        SETTINGS_MENU_ID => OPEN_SETTINGS_EVENT,
        CHECK_UPDATES_MENU_ID => CHECK_UPDATES_EVENT,
        _ => return,
    };

    window::focus_main_window(app);
    let _ = tauri::Emitter::emit(app, event_name, ());
}
