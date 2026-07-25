use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::time::{SystemTime, UNIX_EPOCH};

fn panic_message(info: &panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        info.to_string()
    }
}

pub fn install() {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        let log_path = crate::data_paths::crash_log_path();
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| format!("{}.{:03}", duration.as_secs(), duration.subsec_millis()))
            .unwrap_or_else(|_| "unknown".to_owned());
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let report = format!(
            "\n{separator}\n[CRASH] {timestamp}\nVersion: {version}\nOS: {os}\nArchitecture: {arch}\nThread: {thread}\nMessage: {message}\nLocation: {location}\n\nBacktrace:\n{backtrace}\n",
            separator = "=".repeat(80),
            version = env!("CARGO_PKG_VERSION"),
            os = std::env::consts::OS,
            arch = std::env::consts::ARCH,
            thread = std::thread::current().name().unwrap_or("unnamed"),
            message = panic_message(info),
            backtrace = Backtrace::force_capture(),
        );

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = file.write_all(report.as_bytes());
            let _ = file.flush();
        }
        eprintln!("Crash report saved to {}", log_path.display());

        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use crate::data_paths::crash_log_path;

    #[test]
    fn crash_log_path_uses_the_application_data_directory() {
        assert_eq!(
            crash_log_path(),
            crate::data_paths::root_dir().join("crash.log")
        );
    }
}
