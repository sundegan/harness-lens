use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::window;

const TRAY_ID: &str = "main-tray";
const SHOW_MAIN_MENU_ID: &str = "tray-show-main";
const SETTINGS_MENU_ID: &str = "tray-settings";
const CHECK_UPDATES_MENU_ID: &str = "tray-check-updates";
const QUIT_MENU_ID: &str = "tray-quit";
const OPEN_SETTINGS_EVENT: &str = "open-settings";
const CHECK_UPDATES_EVENT: &str = "check-for-updates";
const TRAY_EXIT_CODE: i32 = 0;

static EXIT_AUTHORIZED: AtomicBool = AtomicBool::new(false);
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuLabels {
    pub show_main: String,
    pub settings: String,
    pub check_updates: String,
    pub quit: String,
}

pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &default_menu_labels())?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("HarnessLens")
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW_MAIN_MENU_ID => window::focus_main_window(app),
            SETTINGS_MENU_ID => open_settings(app),
            CHECK_UPDATES_MENU_ID => check_for_updates(app),
            QUIT_MENU_ID => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::focus_main_window(tray.app_handle());
            }
        });

    #[cfg(target_os = "macos")]
    {
        tray_builder = tray_builder
            .icon(macos_template_icon()?)
            .icon_as_template(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon() {
            tray_builder = tray_builder.icon(icon.clone());
        }
    }

    tray_builder.build(app)?;
    Ok(())
}

fn request_quit(app: &tauri::AppHandle) {
    EXIT_AUTHORIZED.store(true, Ordering::Release);
    app.exit(TRAY_EXIT_CODE);
}

pub fn consume_exit_authorization(code: Option<i32>) -> bool {
    if code != Some(TRAY_EXIT_CODE) {
        return false;
    }

    EXIT_AUTHORIZED.swap(false, Ordering::AcqRel)
}

pub fn update_menu(app: &tauri::AppHandle, labels: TrayMenuLabels) -> Result<(), String> {
    let menu = build_menu(app, &labels).map_err(|error| error.to_string())?;
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "main tray is not available".to_owned())?;

    tray.set_menu(Some(menu)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_authorization_is_consumed_once() {
        EXIT_AUTHORIZED.store(true, Ordering::Release);

        assert!(!consume_exit_authorization(None));
        assert!(consume_exit_authorization(Some(TRAY_EXIT_CODE)));
        assert!(!consume_exit_authorization(Some(TRAY_EXIT_CODE)));
    }
}

fn default_menu_labels() -> TrayMenuLabels {
    TrayMenuLabels {
        show_main: "Show HarnessLens".to_owned(),
        settings: "Settings".to_owned(),
        check_updates: "Check for Updates...".to_owned(),
        quit: "Quit".to_owned(),
    }
}

fn build_menu(
    app: &tauri::AppHandle,
    labels: &TrayMenuLabels,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let show_main_item = MenuItem::with_id(
        app,
        SHOW_MAIN_MENU_ID,
        &labels.show_main,
        true,
        None::<&str>,
    )?;
    let settings_item =
        MenuItem::with_id(app, SETTINGS_MENU_ID, &labels.settings, true, None::<&str>)?;
    let check_updates_item = MenuItem::with_id(
        app,
        CHECK_UPDATES_MENU_ID,
        &labels.check_updates,
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, QUIT_MENU_ID, &labels.quit, true, None::<&str>)?;

    MenuBuilder::new(app)
        .item(&show_main_item)
        .item(&settings_item)
        .item(&check_updates_item)
        .separator()
        .item(&quit_item)
        .build()
}

fn open_settings(app: &tauri::AppHandle) {
    window::focus_main_window(app);
    let _ = tauri::Emitter::emit(app, OPEN_SETTINGS_EVENT, ());
}

fn check_for_updates(app: &tauri::AppHandle) {
    window::focus_main_window(app);
    let _ = tauri::Emitter::emit(app, CHECK_UPDATES_EVENT, ());
}

#[cfg(target_os = "macos")]
fn macos_template_icon() -> tauri::Result<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-macos@2x.png"))
}
