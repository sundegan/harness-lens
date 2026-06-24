use std::time::Duration;

use tauri::{PhysicalSize, WebviewWindow};

const REALIZE_WAIT: Duration = Duration::from_millis(200);
const RESIZE_GAP: Duration = Duration::from_millis(100);
const RECONCILE_WAIT: Duration = Duration::from_millis(500);

pub(crate) fn nudge_main_window(window: WebviewWindow) {
    let _ = window.set_focus();

    std::thread::spawn(move || {
        std::thread::sleep(REALIZE_WAIT);
        let _ = window.set_focus();

        let Ok(original) = window.inner_size() else {
            return;
        };

        let bumped = PhysicalSize::new(original.width.saturating_add(1), original.height);
        let _ = window.set_size(bumped);
        std::thread::sleep(RESIZE_GAP);
        let _ = window.set_size(original);

        std::thread::sleep(RECONCILE_WAIT);
        if let Ok(after) = window.inner_size() {
            if after.width != original.width || after.height != original.height {
                let _ = window.set_size(original);
            }
        }
    });
}
