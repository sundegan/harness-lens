use std::time::Duration;

use tauri::{LogicalSize, Manager, PhysicalPosition, WebviewWindow};

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
const NATIVE_TRAFFIC_LIGHT_X: f64 = 12.0;

#[cfg(target_os = "macos")]
const NATIVE_TRAFFIC_LIGHT_TOP_INSET: f64 = 19.0;

pub fn focus_main_window(app: &tauri::AppHandle) {
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

#[cfg(target_os = "macos")]
pub fn apply_macos_native_titlebar(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(ns_window) = window.ns_window() {
            unsafe { configure_macos_native_titlebar(ns_window) };
        }
    }
}

#[cfg(target_os = "macos")]
pub fn apply_macos_window_theme(
    window: &tauri::WebviewWindow,
    is_dark: bool,
) -> Result<(), String> {
    let ns_window = window.ns_window().map_err(|error| error.to_string())?;

    unsafe {
        let ns_window_ref = &*ns_window.cast::<NSWindow>();
        let appearance_name = if is_dark {
            &NSAppearanceNameDarkAqua
        } else {
            &NSAppearanceNameAqua
        };

        if let Some(appearance) = NSAppearance::appearanceNamed(appearance_name) {
            ns_window_ref.setAppearance(Some(&appearance));
        }

        configure_macos_native_titlebar(ns_window);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn reposition_macos_native_traffic_lights(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(ns_window) = window.ns_window() {
            unsafe {
                position_macos_native_traffic_lights(&*ns_window.cast::<NSWindow>());
            }
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn configure_macos_native_titlebar(ns_window: *mut std::ffi::c_void) {
    let ns_window = &*ns_window.cast::<NSWindow>();
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

    position_macos_native_traffic_lights(ns_window);
}

#[cfg(target_os = "macos")]
unsafe fn position_macos_native_traffic_lights(ns_window: &NSWindow) {
    let Some(close_button) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
        return;
    };
    let Some(minimize_button) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
    else {
        return;
    };
    let Some(zoom_button) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
        return;
    };
    let Some(button_superview) = close_button.superview() else {
        return;
    };
    let Some(titlebar_container) = button_superview.superview() else {
        return;
    };

    let close_frame = close_button.frame();
    let mut titlebar_frame = titlebar_container.frame();
    titlebar_frame.size.height = close_frame.size.height + NATIVE_TRAFFIC_LIGHT_TOP_INSET;
    titlebar_frame.origin.y = ns_window.frame().size.height - titlebar_frame.size.height;
    titlebar_container.setFrame(titlebar_frame);

    let button_gap = minimize_button.frame().origin.x - close_frame.origin.x;
    for (index, button) in [close_button, minimize_button, zoom_button]
        .into_iter()
        .enumerate()
    {
        let mut frame = button.frame();
        frame.origin.x = NATIVE_TRAFFIC_LIGHT_X + index as f64 * button_gap;
        button.setFrameOrigin(frame.origin);
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
