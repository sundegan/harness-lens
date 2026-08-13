use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "linux")]
use crate::linux_fix;

const WINDOW_SCREEN_MARGIN: u32 = 48;
const DEFAULT_WINDOW_LOGICAL_WIDTH: u32 = 1280;
const DEFAULT_WINDOW_LOGICAL_HEIGHT: u32 = 800;
const MIN_WINDOW_LOGICAL_WIDTH: u32 = 960;
const MIN_WINDOW_LOGICAL_HEIGHT: u32 = 640;
const RESTORED_WINDOW_MAX_SCREEN_RATIO: f64 = 0.9;

#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSColor, NSWindow, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility,
};

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;

#[cfg(target_os = "macos")]
static MACOS_THEME_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedWindowState {
    position_x: i32,
    position_y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

pub fn restore_main_window(app: &tauri::AppHandle) {
    let path = crate::data_paths::window_state_path();
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<SavedWindowState>(&contents) else {
        log::warn!("failed to parse saved window state at {}", path.display());
        return;
    };
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    if let Err(error) = window.set_size(PhysicalSize::new(state.width, state.height)) {
        log::warn!("failed to restore window size: {error}");
    }
    if let Err(error) =
        window.set_position(PhysicalPosition::new(state.position_x, state.position_y))
    {
        log::warn!("failed to restore window position: {error}");
    }
    if state.maximized {
        if let Err(error) = window.maximize() {
            log::warn!("failed to restore maximized window state: {error}");
        }
    }
}

pub fn persist_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(position), Ok(size), Ok(maximized)) = (
        window.outer_position(),
        window.inner_size(),
        window.is_maximized(),
    ) else {
        return;
    };
    let state = SavedWindowState {
        position_x: position.x,
        position_y: position.y,
        width: size.width,
        height: size.height,
        maximized,
    };
    let path = crate::data_paths::window_state_path();
    let Some(parent) = path.parent() else {
        return;
    };

    if let Err(error) = (|| -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(parent)?;
        std::fs::write(&path, serde_json::to_string(&state)?)?;
        Ok(())
    })() {
        log::warn!("failed to save window state: {error}");
    }
}

pub fn focus_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
        log::error!("failed to restore regular application policy: {error}");
    }

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

pub fn enter_background_mode(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.hide() {
            log::error!("failed to hide main window: {error}");
        }
    }

    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
        log::error!("failed to enter accessory application policy: {error}");
    }
}

#[cfg(target_os = "macos")]
pub fn apply_macos_native_titlebar(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(ns_window) = window.ns_window() {
            unsafe {
                let ns_window = &*ns_window.cast::<NSWindow>();
                configure_macos_native_titlebar(ns_window);
                layout_macos_titlebar(ns_window);
            };
        }
    }
}

#[cfg(target_os = "macos")]
pub fn apply_macos_window_theme(
    window: &tauri::WebviewWindow,
    is_dark: bool,
) -> Result<(), String> {
    let generation = MACOS_THEME_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let app_handle = window.app_handle().clone();
    let window_label = window.label().to_owned();

    window
        .run_on_main_thread(move || {
            if MACOS_THEME_GENERATION.load(Ordering::Acquire) != generation {
                return;
            }

            let Some(window) = app_handle.get_webview_window(&window_label) else {
                return;
            };
            let Ok(ns_window) = window.ns_window() else {
                return;
            };

            unsafe {
                let ns_window = &*ns_window.cast::<NSWindow>();
                let appearance_name = if is_dark {
                    &NSAppearanceNameDarkAqua
                } else {
                    &NSAppearanceNameAqua
                };

                if let Some(appearance) = NSAppearance::appearanceNamed(appearance_name) {
                    ns_window.setAppearance(Some(&appearance));
                }

                configure_macos_native_titlebar(ns_window);
                layout_macos_titlebar(ns_window);
            }

            // setAppearance can schedule a later AppKit titlebar layout. Keep the
            // second pass outside live-resize handling so AppKit owns that path.
            let follow_up_app = app_handle.clone();
            let follow_up_label = window_label.clone();
            DispatchQueue::main().exec_async(move || {
                if MACOS_THEME_GENERATION.load(Ordering::Acquire) != generation {
                    return;
                }

                let Some(window) = follow_up_app.get_webview_window(&follow_up_label) else {
                    return;
                };
                let Ok(ns_window) = window.ns_window() else {
                    return;
                };

                unsafe {
                    let ns_window = &*ns_window.cast::<NSWindow>();
                    layout_macos_titlebar(ns_window);
                }
            });
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
unsafe fn configure_macos_native_titlebar(ns_window: &NSWindow) {
    let current_style_mask = ns_window.styleMask();
    let required_style_mask = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let style_mask = current_style_mask | required_style_mask;

    if style_mask != current_style_mask {
        ns_window.setStyleMask(style_mask);
    }

    ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    ns_window.setTitlebarAppearsTransparent(true);
    ns_window.setOpaque(false);
    let clear_color = NSColor::clearColor();
    ns_window.setBackgroundColor(Some(&clear_color));

    for button_kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = ns_window.standardWindowButton(button_kind) {
            button.setHidden(false);
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn layout_macos_titlebar(ns_window: &NSWindow) {
    if let Some(content_view) = ns_window.contentView() {
        content_view.layoutSubtreeIfNeeded();
    }
}

fn clamp_axis(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn max_window_axis(work_area_axis: f64, min_axis: f64) -> f64 {
    (work_area_axis * RESTORED_WINDOW_MAX_SCREEN_RATIO)
        .round()
        .min((work_area_axis - WINDOW_SCREEN_MARGIN as f64).max(min_axis))
        .max(work_area_axis / 2.0)
        .max(min_axis)
}

fn restored_window_axis(
    current_axis: f64,
    work_area_axis: f64,
    default_axis: f64,
    min_axis: f64,
) -> f64 {
    let max_axis = max_window_axis(work_area_axis, min_axis);
    if current_axis > work_area_axis - WINDOW_SCREEN_MARGIN as f64 {
        return default_axis.min(max_axis).max(min_axis);
    }

    current_axis.min(max_axis).max(min_axis)
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
    let current_logical_size = current_size.to_logical::<f64>(scale_factor);
    let work_area_logical_size = monitor_size.to_logical::<f64>(scale_factor);
    let clamped_logical_size = LogicalSize {
        width: restored_window_axis(
            current_logical_size.width,
            work_area_logical_size.width,
            DEFAULT_WINDOW_LOGICAL_WIDTH as f64,
            MIN_WINDOW_LOGICAL_WIDTH as f64,
        ),
        height: restored_window_axis(
            current_logical_size.height,
            work_area_logical_size.height,
            DEFAULT_WINDOW_LOGICAL_HEIGHT as f64,
            MIN_WINDOW_LOGICAL_HEIGHT as f64,
        ),
    };
    let clamped_size = clamped_logical_size.to_physical::<u32>(scale_factor);

    if clamped_size != current_size {
        window.set_size(clamped_logical_size)?;
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

pub fn schedule_main_window_bounds_clamp(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = clamp_main_window_to_visible_area(&window);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_window_axis_keeps_restored_windows_below_screen_width() {
        assert_eq!(max_window_axis(1512.0, 960.0), 1361.0);
    }

    #[test]
    fn max_window_axis_handles_tiny_monitors() {
        assert_eq!(max_window_axis(40.0, 1.0), 20.0);
    }

    #[test]
    fn restored_window_axis_uses_default_for_external_display_state() {
        assert_eq!(restored_window_axis(1520.0, 1512.0, 1280.0, 960.0), 1280.0);
    }

    #[test]
    fn restored_window_axis_preserves_reasonable_user_size() {
        assert_eq!(restored_window_axis(1161.0, 1512.0, 1280.0, 960.0), 1161.0);
    }

    #[test]
    fn restored_window_axis_never_drops_below_minimum() {
        assert_eq!(restored_window_axis(520.0, 1512.0, 1280.0, 960.0), 960.0);
    }
}
