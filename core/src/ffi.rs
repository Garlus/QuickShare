use crate::discovery::{BleDiscovery, MdnsDiscovery};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// === Opaque handle for C API ===
pub struct QsContext {
    #[allow(dead_code)]
    ble: Option<BleDiscovery>,
    mdns: Option<MdnsDiscovery>,
    device_name: String,
    initialized: AtomicBool,
}

// === C-compatible callback types ===

pub type QsDeviceFoundCb = extern "C" fn(
    device_id: *const c_char,
    device_name: *const c_char,
    connection_type: i32,
    user_data: *mut std::ffi::c_void,
);

pub type QsTransferCb = extern "C" fn(
    transfer_id: *const c_char,
    device_id: *const c_char,
    status: i32,
    bytes_sent: i64,
    bytes_total: i64,
    user_data: *mut std::ffi::c_void,
);

pub type QsLogCb = extern "C" fn(
    level: i32,
    message: *const c_char,
    user_data: *mut std::ffi::c_void,
);

/// Wrapper to make raw pointer Send-safe for callbacks.
struct CallbackData {
    cb: QsDeviceFoundCb,
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for CallbackData {}

// === Global callbacks ===
static mut DEVICE_FOUND_CB: Option<(QsDeviceFoundCb, *mut std::ffi::c_void)> = None;
static mut TRANSFER_CB: Option<(QsTransferCb, *mut std::ffi::c_void)> = None;

// === Helpers ===

fn c_str_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}

fn get_runtime() -> &'static Runtime {
    static RUNTIME: std::sync::OnceLock<Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Failed to create Tokio runtime"))
}

// === Public FFI API ===

#[unsafe(no_mangle)]
pub extern "C" fn qs_init(
    device_name: *const c_char,
    _log_cb: Option<QsLogCb>,
) -> *mut QsContext {
    let name = c_str_to_string(device_name);
    let name = if name.is_empty() { "QuickShare Desktop".to_string() } else { name };

    let runtime = get_runtime();

    let ble = runtime.block_on(async {
        BleDiscovery::new().await.ok()
    });

    let mdns = match MdnsDiscovery::new() {
        Ok(mdns) => Some(mdns),
        Err(_) => None,
    };

    let ctx = Box::new(QsContext {
        ble,
        mdns,
        device_name: name,
        initialized: AtomicBool::new(true),
    });

    Box::into_raw(ctx)
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_shutdown(ctx: *mut QsContext) {
    if ctx.is_null() { return; }
    let ctx = unsafe { Box::from_raw(ctx) };
    ctx.initialized.store(false, Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_set_device_found_callback(
    _ctx: *mut QsContext,
    cb: QsDeviceFoundCb,
    user_data: *mut std::ffi::c_void,
) {
    unsafe {
        DEVICE_FOUND_CB = Some((cb, user_data));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_set_transfer_callback(
    _ctx: *mut QsContext,
    cb: QsTransferCb,
    user_data: *mut std::ffi::c_void,
) {
    unsafe {
        TRANSFER_CB = Some((cb, user_data));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_start_advertising(ctx: *mut QsContext, _device_type: i32) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    let device_name = ctx.device_name.clone();
    if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async {
            if mdns.start_advertising(&device_name, 5721).await.is_err() {
                return -1;
            }
            0
        })
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_advertising(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async {
            mdns.stop_advertising().await.ok();
            0
        })
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_start_discovery(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    let cb_data = unsafe {
        DEVICE_FOUND_CB.map(|(cb, data)| CallbackData { cb, user_data: data })
    };

    if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async {
            let result = mdns.start_discovery(move |name, _hostname, ip, port| {
                if let Some(ref cb_data) = cb_data {
                    let id_c = CString::new(format!("{}:{}", ip, port)).unwrap();
                    let name_c = CString::new(name).unwrap();
                    (cb_data.cb)(id_c.as_ptr(), name_c.as_ptr(), 1, cb_data.user_data);
                }
            }).await;
            if result.is_err() {
                return -1;
            }
            0
        })
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_discovery(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async {
            mdns.stop_discovery().await.ok();
            0
        })
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_is_advertising(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return 0; }
    let ctx = unsafe { &*ctx };
    if ctx.initialized.load(Ordering::SeqCst) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_is_discovering(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return 0; }
    let ctx = unsafe { &*ctx };
    if ctx.initialized.load(Ordering::SeqCst) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_version() -> *const c_char {
    "0.1.0\0".as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { let _ = CString::from_raw(s); }
    }
}
