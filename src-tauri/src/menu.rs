use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};

pub const ABOUT_MENU_ID: &str = "about";
pub const SHOW_ABOUT_EVENT: &str = "show-about";
pub const CHECK_UPDATES_MENU_ID: &str = "check-updates";
pub const CHECK_UPDATES_EVENT: &str = "check-for-updates";

pub fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let about_item = MenuItem::with_id(
        app,
        ABOUT_MENU_ID,
        "About Codex Timeline",
        true,
        None::<&str>,
    )?;

    let check_updates_item = MenuItem::with_id(
        app,
        CHECK_UPDATES_MENU_ID,
        "Check for Updates...",
        true,
        None::<&str>,
    )?;

    let app_menu = SubmenuBuilder::new(app, "Codex Timeline")
        .item(&about_item)
        .item(&check_updates_item)
        .separator()
        .hide_with_text("Hide Codex Timeline")
        .separator()
        .quit_with_text("Quit Codex Timeline")
        .build()?;

    MenuBuilder::new(app).item(&app_menu).build()
}
