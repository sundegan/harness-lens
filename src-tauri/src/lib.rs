use std::time::Duration;

use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

#[cfg(target_os = "linux")]
mod linux_fix;
#[cfg(target_os = "macos")]
mod menu;

const WINDOW_SCREEN_MARGIN: u32 = 48;
const DEFAULT_WINDOW_LOGICAL_WIDTH: u32 = 1280;
const DEFAULT_WINDOW_LOGICAL_HEIGHT: u32 = 800;
const RESTORED_WINDOW_MAX_SCREEN_RATIO: f64 = 0.9;

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();

        #[cfg(target_os = "linux")]
        {
            linux_fix::nudge_main_window(window);
        }
    }
}

fn clamp_axis(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn max_window_axis(work_area_axis: u32) -> u32 {
    ((work_area_axis as f64 * RESTORED_WINDOW_MAX_SCREEN_RATIO).round() as u32)
        .min(work_area_axis.saturating_sub(WINDOW_SCREEN_MARGIN))
        .max(work_area_axis / 2)
        .max(1)
}

fn restored_window_axis(
    current_axis: u32,
    work_area_axis: u32,
    scale_factor: f64,
    default_logical_axis: u32,
) -> u32 {
    let max_axis = max_window_axis(work_area_axis);
    if current_axis > work_area_axis.saturating_sub(WINDOW_SCREEN_MARGIN) {
        return ((default_logical_axis as f64 * scale_factor).round() as u32)
            .min(max_axis)
            .max(1);
    }

    current_axis.min(max_axis).max(1)
}

fn clamp_main_window_to_visible_area(window: &WebviewWindow) -> tauri::Result<()> {
    if window.is_maximized()? || window.is_fullscreen()? {
        return Ok(());
    }

    let Some(monitor) = window
        .current_monitor()?
        .or(window.primary_monitor()?)
        .or_else(|| window.available_monitors().ok()?.into_iter().next())
    else {
        return Ok(());
    };

    let work_area = *monitor.work_area();
    let monitor_position = work_area.position;
    let monitor_size = work_area.size;
    let scale_factor = monitor.scale_factor();
    let current_size = window.inner_size()?;
    let clamped_size = PhysicalSize {
        width: restored_window_axis(
            current_size.width,
            monitor_size.width,
            scale_factor,
            DEFAULT_WINDOW_LOGICAL_WIDTH,
        ),
        height: restored_window_axis(
            current_size.height,
            monitor_size.height,
            scale_factor,
            DEFAULT_WINDOW_LOGICAL_HEIGHT,
        ),
    };

    if clamped_size != current_size {
        window.set_size(clamped_size)?;
    }

    let current_position = window.outer_position()?;
    let max_x = monitor_position.x + monitor_size.width.saturating_sub(clamped_size.width) as i32;
    let max_y = monitor_position.y + monitor_size.height.saturating_sub(clamped_size.height) as i32;
    let clamped_position = PhysicalPosition {
        x: clamp_axis(current_position.x, monitor_position.x, max_x),
        y: clamp_axis(current_position.y, monitor_position.y, max_y),
    };

    if clamped_position != current_position {
        window.set_position(clamped_position)?;
    }

    Ok(())
}

fn schedule_main_window_bounds_clamp(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = clamp_main_window_to_visible_area(&window);
        }
    });
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| menu::build_app_menu(app))
        .on_menu_event(|app, event| {
            if event.id().as_ref() == menu::ABOUT_MENU_ID {
                focus_main_window(app);
                let _ = tauri::Emitter::emit(app, menu::SHOW_ABOUT_EVENT, ());
            } else if event.id().as_ref() == menu::CHECK_UPDATES_MENU_ID {
                focus_main_window(app);
                let _ = tauri::Emitter::emit(app, menu::CHECK_UPDATES_EVENT, ());
            }
        });

    builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![restart_app])
        .setup(|app| {
            let app_handle = app.handle();
            schedule_main_window_bounds_clamp(app_handle);
            focus_main_window(app_handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Timeline");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_window_axis_keeps_restored_windows_below_screen_width() {
        assert_eq!(max_window_axis(3024), 2722);
    }

    #[test]
    fn max_window_axis_handles_tiny_monitors() {
        assert_eq!(max_window_axis(40), 20);
    }

    #[test]
    fn restored_window_axis_uses_default_for_external_display_state() {
        assert_eq!(restored_window_axis(3040, 3024, 2.0, 1280), 2560);
    }

    #[test]
    fn restored_window_axis_preserves_reasonable_user_size() {
        assert_eq!(restored_window_axis(1800, 3024, 2.0, 1100), 1800);
    }
}
