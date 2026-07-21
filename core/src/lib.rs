pub mod discovery;
pub mod protocol;
pub mod transfer;
pub mod ffi;

use std::sync::OnceLock;

/// Global log callback for forwarding Rust logs to Swift.
struct LogCallback {
    cb: ffi::QsLogCb,
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for LogCallback {}
unsafe impl Sync for LogCallback {}

static LOG_CALLBACK: OnceLock<LogCallback> = OnceLock::new();

pub fn init_logging(log_cb: Option<(ffi::QsLogCb, *mut std::ffi::c_void)>) {
    if let Some((cb, user_data)) = log_cb {
        let _ = LOG_CALLBACK.set(LogCallback { cb, user_data });
    }

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .with_ansi(true)
        .try_init();
}
