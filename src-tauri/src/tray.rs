use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::window;

const TRAY_ID: &str = "main-tray";
const SHOW_MAIN_MENU_ID: &str = "tray-show-main";
const SETTINGS_MENU_ID: &str = "tray-settings";
const QUIT_MENU_ID: &str = "tray-quit";
const OPEN_SETTINGS_EVENT: &str = "open-settings";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuLabels {
    show_main: String,
    settings: String,
    quit: String,
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
            QUIT_MENU_ID => app.exit(0),
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

pub fn update_menu(app: &tauri::AppHandle, labels: TrayMenuLabels) -> Result<(), String> {
    let menu = build_menu(app, &labels).map_err(|error| error.to_string())?;
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "main tray is not available".to_owned())?;

    tray.set_menu(Some(menu)).map_err(|error| error.to_string())
}

fn default_menu_labels() -> TrayMenuLabels {
    TrayMenuLabels {
        show_main: "Show HarnessLens".to_owned(),
        settings: "Settings".to_owned(),
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
    let quit_item = MenuItem::with_id(app, QUIT_MENU_ID, &labels.quit, true, None::<&str>)?;

    MenuBuilder::new(app)
        .item(&show_main_item)
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()
}

fn open_settings(app: &tauri::AppHandle) {
    window::focus_main_window(app);
    let _ = tauri::Emitter::emit(app, OPEN_SETTINGS_EVENT, ());
}

#[cfg(target_os = "macos")]
fn macos_template_icon() -> tauri::Result<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-macos@2x.png"))
}
