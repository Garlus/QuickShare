use crate::discovery::BleDiscovery;
use crate::init_logging;
use crate::transfer::connection::{Connection, ConnectionListener, DEFAULT_PORT};
use crate::transfer::sender::{FileSender, set_progress_callback};
use crate::transfer::encryption::CryptoContext;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::sync::watch;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use parking_lot::Mutex;

// === Opaque handle for C API ===
pub struct QsContext {
    #[allow(dead_code)]
    ble: Option<BleDiscovery>,
    device_name: String,
    initialized: AtomicBool,
    endpoint_id: parking_lot::Mutex<Option<[u8; 4]>>,
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

// === Progress callback bridge ===

static CURRENT_TRANSFER_ID: Mutex<Option<String>> = Mutex::new(None);
static CURRENT_DEVICE_ID: Mutex<Option<String>> = Mutex::new(None);

fn native_progress_callback(sent: i64, total: i64, _file_name: &str) {
    let tid_guard = CURRENT_TRANSFER_ID.lock();
    let did_guard = CURRENT_DEVICE_ID.lock();
    let callbacks = CALLBACKS.lock();

    if let Some((cb, user_data)) = callbacks.transfer {
        let tid = CString::new(tid_guard.as_deref().unwrap_or("")).unwrap();
        let did = CString::new(did_guard.as_deref().unwrap_or("")).unwrap();
        cb(tid.as_ptr(), did.as_ptr(), 1, sent, total, user_data);
    }
}

/// Must be called once after qs_init to wire up progress reporting.
pub fn init_progress_bridge() {
    let _ = set_progress_callback(native_progress_callback);
}

// === Listener management ===

static LISTENER_SHUTDOWN: Mutex<Option<watch::Sender<bool>>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn qs_start_listener(
    ctx: *mut QsContext,
    save_dir: *const c_char,
) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    let dir = c_str_to_string(save_dir);
    let save_path = if dir.is_empty() {
        std::path::PathBuf::from("/tmp/QuickShare")
    } else {
        std::path::PathBuf::from(&dir)
    };
    std::fs::create_dir_all(&save_path).ok();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    {
        let mut guard = LISTENER_SHUTDOWN.lock();
        if guard.is_some() {
            tracing::warn!("Listener already running, stopping first");
            if let Some(tx) = guard.take() {
                let _ = tx.send(true);
            }
        }
        *guard = Some(shutdown_tx);
    }

    let device_name = ctx.device_name.clone();

    get_runtime().spawn(async move {
        let listener = match ConnectionListener::new(DEFAULT_PORT).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to start TCP listener: {}", e);
                return;
            }
        };

        tracing::info!("TCP listener running on port {}", DEFAULT_PORT);

        let mut rx = shutdown_rx;
        loop {
            tokio::select! {
                result = listener.accept(&device_name, 3, CryptoContext::new()) => {
                    match result {
                        Ok(conn) => {
                            let peer = conn.peer_addr();
                            let device_id = conn.endpoint_id().to_string();
                            tracing::info!("Accepted connection from {} (endpoint: {})", peer, device_id);
                            let save_dir = save_path.clone();
                            tokio::spawn(async move {
                                let mut receiver = crate::transfer::receiver::FileReceiver::new(
                                    conn,
                                    CryptoContext::new(),
                                    save_dir,
                                    device_id,
                                    0,
                                );
                                if let Err(e) = receiver.receive_file().await {
                                    tracing::error!("Failed to receive file: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = rx.changed() => {
                    if *rx.borrow() {
                        tracing::info!("Listener shutdown signal received");
                        break;
                    }
                }
            }
        }

        tracing::info!("TCP listener stopped");
    });

    tracing::info!("TCP listener started on port {}", DEFAULT_PORT);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_listener(_ctx: *mut QsContext) -> i32 {
    let mut guard = LISTENER_SHUTDOWN.lock();
    if let Some(tx) = guard.take() {
        let _ = tx.send(true);
        tracing::info!("Listener shutdown initiated");
        0
    } else {
        tracing::warn!("No listener running");
        -1
    }
}

// === File sending ===

#[unsafe(no_mangle)]
pub extern "C" fn qs_send_file(
    ctx: *mut QsContext,
    device_ip: *const c_char,
    port: i32,
    endpoint_id: *const c_char,
    file_path: *const c_char,
) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    let ip = c_str_to_string(device_ip);
    let eid = c_str_to_string(endpoint_id);
    let path_str = c_str_to_string(file_path);
    let port = port as u16;

    let path = std::path::Path::new(&path_str);
    if !path.exists() {
        tracing::error!("File not found: {}", path_str);
        return -2;
    }

    let addr: SocketAddr = match format!("{}:{}", ip, port).parse() {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Invalid address {}:{}: {}", ip, port, e);
            return -3;
        }
    };

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let device_id = eid.clone();

    // Store for progress callback
    {
        *CURRENT_TRANSFER_ID.lock() = Some(transfer_id.clone());
        *CURRENT_DEVICE_ID.lock() = Some(device_id.clone());
    }

    tracing::info!("Sending file to {} ({}): {}", addr, eid, path_str);

    let result = get_runtime().block_on(async {
        let crypto = CryptoContext::new();
        let conn = match Connection::connect(addr, eid, &ctx.device_name, 3, crypto).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to connect to {}: {}", addr, e);
                return Err(e);
            }
        };

        let crypto2 = CryptoContext::new();
        let mut sender = FileSender::new(conn, crypto2);
        sender.send_file(path).await
    });

    // Report completion or error
    let callbacks = CALLBACKS.lock();
    if let Some((cb, user_data)) = callbacks.transfer {
        let tid = CString::new(transfer_id).unwrap();
        let did = CString::new(device_id).unwrap();
        match &result {
            Ok(()) => {
                tracing::info!("File sent successfully");
                cb(tid.as_ptr(), did.as_ptr(), 2, 0, 0, user_data);
                0
            }
            Err(e) => {
                tracing::error!("Send failed: {}", e);
                cb(tid.as_ptr(), did.as_ptr(), 3, 0, 0, user_data);
                -4
            }
        }
    } else {
        match &result {
            Ok(()) => 0,
            Err(_) => -4,
        }
    }
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

    // Initialize progress callback bridge
    init_progress_bridge();

    tracing::info!("qs_init: initializing with device_name='{}'", name);

    let runtime = get_runtime();

    let ble = runtime.block_on(async {
        BleDiscovery::new().await.ok()
    });

    let ctx = Box::new(QsContext {
        ble,
        device_name: name,
        initialized: AtomicBool::new(true),
        endpoint_id: parking_lot::Mutex::new(None),
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
pub extern "C" fn qs_start_advertising(ctx: *mut QsContext, _device_type: i32) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    // Generate endpoint_id on first call, reuse on subsequent calls.
    // mDNS advertising is handled by Swift's NWListener (system Bonjour).
    {
        let mut guard = ctx.endpoint_id.lock();
        if guard.is_none() {
            let mut id = [0u8; 4];
            rand::Rng::fill(&mut rand::thread_rng(), &mut id);
            *guard = Some(id);
        }
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_get_endpoint_id(ctx: *mut QsContext) -> *mut c_char {
    if ctx.is_null() { return std::ptr::null_mut(); }
    let ctx = unsafe { &*ctx };
    let guard = ctx.endpoint_id.lock();
    match *guard {
        Some(id) => {
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                id,
            );
            CString::new(encoded).unwrap_or_default().into_raw()
        }
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_advertising(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_start_discovery(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return -1; }

    // BLE scanning is the primary discovery mechanism on macOS.
    // mDNS browsing is handled by Swift's NWBrowser (system Bonjour stack).
    if let Some(ref ble) = ctx.ble {
        let cb_data = CALLBACKS.lock().device_found.map(|(cb, ud)| SendCallbackPair { cb, user_data: ud });
        get_runtime().block_on(async {
            let _ = ble.start_scanning(
                move |id, name, _data| {
                    if let Some(ref cb_data) = cb_data {
                        let id_c = CString::new(id).unwrap();
                        let name_c = CString::new(name).unwrap();
                        (cb_data.cb)(id_c.as_ptr(), name_c.as_ptr(), 0, cb_data.user_data);
                    }
                },
            ).await;
        });
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_stop_discovery(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return -1; }
    let ctx = unsafe { &*ctx };

    if let Some(ref ble) = ctx.ble {
        get_runtime().block_on(async {
            ble.stop_scanning().await.ok();
        });
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_is_advertising(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return 0; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return 0; }

    // Advertising state is managed by Swift (BleAdvertiser + MdnsAdvertiser).
    // We report "advertising" if the endpoint_id has been generated.
    ctx.endpoint_id.lock().is_some() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn qs_is_discovering(ctx: *mut QsContext) -> i32 {
    if ctx.is_null() { return 0; }
    let ctx = unsafe { &*ctx };
    if !ctx.initialized.load(Ordering::SeqCst) { return 0; }

    if let Some(ref ble) = ctx.ble {
        get_runtime().block_on(async { ble.is_scanning().await }) as i32
    } else {
        0
    }
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
