use crate::discovery::{BleDiscovery, MdnsDiscovery, utils::DeviceType};
use crate::init_logging;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use tokio::sync::{oneshot, mpsc};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use parking_lot::Mutex;

// === Opaque handle for C API ===
pub struct QsContext {
    #[allow(dead_code)]
    ble: Option<BleDiscovery>,
    mdns: Option<MdnsDiscovery>,
    device_name: String,
    initialized: AtomicBool,
    /// Sender end of the BLE->mDNS re-broadcast channel.
    ble_mdns_sender: Option<mpsc::Sender<()>>,
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

pub type QsIncomingTransferCb = extern "C" fn(
    request_id: *const c_char,
    device_name: *const c_char,
    file_name: *const c_char,
    file_size: i64,
    file_number: i32,
    user_data: *mut std::ffi::c_void,
);

/// Send-safe wrapper for callback function pointer + user_data pairs.
struct SendCallbackPair {
    cb: QsDeviceFoundCb,
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for SendCallbackPair {}

// === Global callbacks (protected by parking_lot::Mutex for Rust 2024 compat) ===
struct GlobalCallbacks {
    device_found: Option<(QsDeviceFoundCb, *mut std::ffi::c_void)>,
    transfer: Option<(QsTransferCb, *mut std::ffi::c_void)>,
    incoming_transfer: Option<(QsIncomingTransferCb, *mut std::ffi::c_void)>,
}

unsafe impl Send for GlobalCallbacks {}
unsafe impl Sync for GlobalCallbacks {}

static CALLBACKS: Mutex<GlobalCallbacks> = Mutex::new(GlobalCallbacks {
    device_found: None,
    transfer: None,
    incoming_transfer: None,
});

// === Incoming transfer state ===
static PENDING_SENDERS: Mutex<Option<HashMap<String, oneshot::Sender<bool>>>> = Mutex::new(None);

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
    log_cb: Option<QsLogCb>,
    log_user_data: *mut std::ffi::c_void,
) -> *mut QsContext {
    let name = c_str_to_string(device_name);
    let name = if name.is_empty() { "QuickShare Desktop".to_string() } else { name };

    // Initialize Rust tracing subscriber — logs go to stderr + Swift callback
    let cb_pair = log_cb.map(|cb| (cb, log_user_data));
    init_logging(cb_pair);

    tracing::info!("qs_init: initializing with device_name='{}'", name);

    let runtime = get_runtime();

    // Create BLE->mDNS re-broadcast channel
    let (ble_mdns_sender, ble_mdns_receiver) = mpsc::channel::<()>(1);

    let ble = runtime.block_on(async {
        BleDiscovery::new().await.ok()
    });

    let mdns = match MdnsDiscovery::new(Some(ble_mdns_receiver)) {
        Ok(mdns) => Some(mdns),
        Err(_) => None,
    };

    let ctx = Box::new(QsContext {
        ble,
        mdns,
        device_name: name,
        initialized: AtomicBool::new(true),
        ble_mdns_sender: Some(ble_mdns_sender),
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
    CALLBACKS.lock().device_found = Some((cb, user_data));
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_set_transfer_callback(
    _ctx: *mut QsContext,
    cb: QsTransferCb,
    user_data: *mut std::ffi::c_void,
) {
    CALLBACKS.lock().transfer = Some((cb, user_data));
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_set_incoming_transfer_callback(
    _ctx: *mut QsContext,
    cb: QsIncomingTransferCb,
    user_data: *mut std::ffi::c_void,
) {
    CALLBACKS.lock().incoming_transfer = Some((cb, user_data));
}

/// Register an incoming transfer and return (request_id, receiver).
pub fn register_incoming_transfer(device_name: &str, file_name: &str, file_size: i64, file_number: i32) -> (String, oneshot::Receiver<bool>) {
    let has_callback = CALLBACKS.lock().incoming_transfer.is_some();

    if !has_callback {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(true);
        return (String::new(), rx);
    }

    let (tx, rx) = oneshot::channel();
    let request_id = uuid::Uuid::new_v4().to_string();

    {
        let mut senders = PENDING_SENDERS.lock();
        if senders.is_none() {
            *senders = Some(HashMap::new());
        }
        senders.as_mut().unwrap().insert(request_id.clone(), tx);
    }

    // Fire the callback to Swift
    let cb_opt = CALLBACKS.lock().incoming_transfer;
    if let Some((cb, user_data)) = cb_opt {
        let req_id = CString::new(request_id.clone()).unwrap();
        let dev_name = CString::new(device_name).unwrap();
        let fname = CString::new(file_name).unwrap();
        cb(req_id.as_ptr(), dev_name.as_ptr(), fname.as_ptr(), file_size, file_number, user_data);
    }

    (request_id, rx)
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_accept_transfer(request_id: *const c_char) -> i32 {
    let id = c_str_to_string(request_id);
    let mut senders = PENDING_SENDERS.lock();
    if let Some(ref mut map) = *senders {
        if let Some(tx) = map.remove(&id) {
            let _ = tx.send(true);
            return 0;
        }
    }
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_deny_transfer(request_id: *const c_char) -> i32 {
    let id = c_str_to_string(request_id);
    let mut senders = PENDING_SENDERS.lock();
    if let Some(ref mut map) = *senders {
        if let Some(tx) = map.remove(&id) {
            let _ = tx.send(false);
            return 0;
        }
    }
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_start_advertising(ctx: *mut QsContext, device_type: i32) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    let device_name = ctx.device_name.clone();
    let dt = match device_type {
        1 => DeviceType::Phone,
        2 => DeviceType::Tablet,
        3 => DeviceType::Laptop,
        _ => DeviceType::Laptop,
    };

    if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async {
            if mdns.start_advertising(&device_name, 5721, dt).await.is_err() {
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

    // Clone the BLE->mDNS sender for the scanning task
    let ble_mdns_sender = ctx.ble_mdns_sender.clone();

    // Start mDNS discovery
    let mdns_result = if let Some(ref mdns) = ctx.mdns {
        let cb_data = CALLBACKS.lock().device_found.map(|(cb, ud)| SendCallbackPair { cb, user_data: ud });
        get_runtime().block_on(async {
            mdns.start_discovery(move |device_name, _hostname, ip, port| {
                if let Some(ref cb_data) = cb_data {
                    let id_c = CString::new(format!("{}:{}", ip, port)).unwrap();
                    let name_c = CString::new(device_name).unwrap();
                    (cb_data.cb)(id_c.as_ptr(), name_c.as_ptr(), 1, cb_data.user_data);
                }
            }).await
        })
    } else {
        Err(anyhow::anyhow!("No mDNS context"))
    };

    // Start BLE scanning (non-blocking, runs in background)
    if let Some(ref ble) = ctx.ble {
        let cb_data = CALLBACKS.lock().device_found.map(|(cb, ud)| SendCallbackPair { cb, user_data: ud });
        get_runtime().block_on(async {
            let _ = ble.start_scanning(
                move |id, name, _data| {
                    if let Some(ref cb_data) = cb_data {
                        let id_c = CString::new(id).unwrap();
                        let name_c = CString::new(name).unwrap();
                        // connection_type 0 = BLE
                        (cb_data.cb)(id_c.as_ptr(), name_c.as_ptr(), 0, cb_data.user_data);
                    }
                },
                ble_mdns_sender,
            ).await;
        });
    }

    if mdns_result.is_err() { -1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_discovery(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };

    // Stop BLE scanning
    if let Some(ref ble) = ctx.ble {
        get_runtime().block_on(async {
            ble.stop_scanning().await.ok();
        });
    }

    // Stop mDNS discovery
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
    if !ctx.initialized.load(Ordering::SeqCst) { return 0; }

    if let Some(ref mdns) = ctx.mdns {
        if get_runtime().block_on(async { mdns.is_advertising().await }) {
            return 1;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_is_discovering(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return 0; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return 0; }

    let mdns_discovering = if let Some(ref mdns) = ctx.mdns {
        get_runtime().block_on(async { mdns.is_discovering().await })
    } else {
        false
    };

    let ble_scanning = if let Some(ref ble) = ctx.ble {
        get_runtime().block_on(async { ble.is_scanning().await })
    } else {
        false
    };

    if mdns_discovering || ble_scanning { 1 } else { 0 }
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
