import Foundation
#if canImport(CQuickShare)
import CQuickShare
#endif

// MARK: - Swift-friendly wrapper

public class QuickShare: @unchecked Sendable {
    public enum ConnectionType: Int32 {
        case ble = 0
        case mdns = 1
        case wifiDirect = 2
    }

    public enum TransferStatus: Int32 {
        case started = 0
        case inProgress = 1
        case completed = 2
        case error = 3
        case cancelled = 4
    }

    public enum DeviceType: Int32 {
        case unknown = 0
        case phone = 1
        case tablet = 2
        case laptop = 3
    }

    public struct DiscoveredDevice {
        public let id: String
        public let name: String
        public let connectionType: ConnectionType
    }

    public struct TransferProgress {
        public let transferId: String
        public let deviceId: String
        public let status: TransferStatus
        public let bytesSent: Int64
        public let bytesTotal: Int64
    }

    public struct IncomingTransferRequest {
        public let requestId: String
        public let deviceName: String
        public let fileName: String
        public let fileSize: Int64
        public let fileNumber: Int32
    }

    private let ctx: OpaquePointer
    private let deviceFoundHandler: ((DiscoveredDevice) -> Void)?
    private let transferHandler: ((TransferProgress) -> Void)?
    private let incomingTransferHandler: ((IncomingTransferRequest) -> Void)?

    public init(
        deviceName: String = "QuickShare Desktop",
        onDeviceFound: ((DiscoveredDevice) -> Void)? = nil,
        onTransfer: ((TransferProgress) -> Void)? = nil,
        onIncomingTransfer: ((IncomingTransferRequest) -> Void)? = nil
    ) {
        self.deviceFoundHandler = onDeviceFound
        self.transferHandler = onTransfer
        self.incomingTransferHandler = onIncomingTransfer

        let userData = Unmanaged.passRetained(QuickShareCallbackContext(
            onDeviceFound: onDeviceFound,
            onTransfer: onTransfer,
            onIncomingTransfer: onIncomingTransfer
        )).toOpaque()

        let namePtr = (deviceName as NSString).utf8String

        // Pass a log callback so Rust logs appear in Xcode console
        self.ctx = qs_init(namePtr, rustLogCallback, userData)!

        if onDeviceFound != nil {
            qs_set_device_found_callback(ctx, deviceFoundCallback, userData)
        }

        if onTransfer != nil {
            qs_set_transfer_callback(ctx, transferCallback, userData)
        }

        if onIncomingTransfer != nil {
            qs_set_incoming_transfer_callback(ctx, incomingTransferCallback, userData)
        }
    }

    deinit {
        qs_shutdown(ctx)
    }

    // MARK: - Public API

    public func startAdvertising(deviceType: DeviceType = .laptop) -> Bool {
        qs_start_advertising(ctx, deviceType.rawValue) == 0
    }

    public func stopAdvertising() -> Bool {
        qs_stop_advertising(ctx) == 0
    }

    public func startDiscovery() -> Bool {
        qs_start_discovery(ctx) == 0
    }

    public func stopDiscovery() -> Bool {
        qs_stop_discovery(ctx) == 0
    }

    public func acceptTransfer(requestId: String) -> Bool {
        requestId.withCString { ptr in
            qs_accept_transfer(ptr) == 0
        }
    }

    public func denyTransfer(requestId: String) -> Bool {
        requestId.withCString { ptr in
            qs_deny_transfer(ptr) == 0
        }
    }

    public var isAdvertising: Bool {
        qs_is_advertising(ctx) != 0
    }

    public var isDiscovering: Bool {
        qs_is_discovering(ctx) != 0
    }

    public static var version: String {
        String(cString: qs_version())
    }

    public func getEndpointId() -> String? {
        let ptr = qs_get_endpoint_id(ctx)
        guard let ptr = ptr else { return nil }
        let result = String(cString: ptr)
        qs_free_string(ptr)
        return result
    }

    // MARK: - TCP Listener (for incoming transfers)

    public func startListener(saveDir: String = NSHomeDirectory() + "/Downloads") -> Bool {
        saveDir.withCString { ptr in
            qs_start_listener(ctx, ptr) == 0
        }
    }

    public func stopListener() -> Bool {
        qs_stop_listener(ctx) == 0
    }

    // MARK: - File Sending (blocking — call from background thread)

    public func sendFile(
        deviceIp: String,
        port: Int32 = 5721,
        endpointId: String,
        filePath: String
    ) -> Bool {
        deviceIp.withCString { ipPtr in
            endpointId.withCString { eidPtr in
                filePath.withCString { pathPtr in
                    qs_send_file(ctx, ipPtr, port, eidPtr, pathPtr) == 0
                }
            }
        }
    }
}

// MARK: - C Callback Trampolines

private let rustLogCallback: qs_log_cb_t = { level, message, userData in
    guard let message = message else { return }
    let msg = String(cString: message)
    let prefix = level <= 1 ? "🔴 ERROR" : level == 2 ? "🟡 WARN" : "🔵 INFO"
    NSLog("[Rust] \(prefix): \(msg)")
}

private let deviceFoundCallback: qs_device_found_cb_t = { deviceId, deviceName, connType, userData in
    guard let userData = userData else { return }
    let context = Unmanaged<QuickShareCallbackContext>.fromOpaque(userData).takeUnretainedValue()

    let id = deviceId.map { String(cString: $0) } ?? ""
    let name = deviceName.map { String(cString: $0) } ?? ""
    let type = QuickShare.ConnectionType(rawValue: connType) ?? .mdns

    context.onDeviceFound?(QuickShare.DiscoveredDevice(id: id, name: name, connectionType: type))
}

private let transferCallback: qs_transfer_cb_t = { transferId, deviceId, status, bytesSent, bytesTotal, userData in
    guard let userData = userData else { return }
    let context = Unmanaged<QuickShareCallbackContext>.fromOpaque(userData).takeUnretainedValue()

    let tId = transferId.map { String(cString: $0) } ?? ""
    let dId = deviceId.map { String(cString: $0) } ?? ""
    let stat = QuickShare.TransferStatus(rawValue: status) ?? .error

    context.onTransfer?(QuickShare.TransferProgress(
        transferId: tId,
        deviceId: dId,
        status: stat,
        bytesSent: bytesSent,
        bytesTotal: bytesTotal
    ))
}

private let incomingTransferCallback: qs_incoming_transfer_cb_t = { requestId, deviceName, fileName, fileSize, fileNumber, userData in
    guard let userData = userData else { return }
    let context = Unmanaged<QuickShareCallbackContext>.fromOpaque(userData).takeUnretainedValue()

    let reqId = requestId.map { String(cString: $0) } ?? ""
    let devName = deviceName.map { String(cString: $0) } ?? ""
    let fName = fileName.map { String(cString: $0) } ?? ""

    context.onIncomingTransfer?(QuickShare.IncomingTransferRequest(
        requestId: reqId,
        deviceName: devName,
        fileName: fName,
        fileSize: fileSize,
        fileNumber: fileNumber
    ))
}

// MARK: - Callback Context

private class QuickShareCallbackContext {
    let onDeviceFound: ((QuickShare.DiscoveredDevice) -> Void)?
    let onTransfer: ((QuickShare.TransferProgress) -> Void)?
    let onIncomingTransfer: ((QuickShare.IncomingTransferRequest) -> Void)?

    init(
        onDeviceFound: ((QuickShare.DiscoveredDevice) -> Void)?,
        onTransfer: ((QuickShare.TransferProgress) -> Void)?,
        onIncomingTransfer: ((QuickShare.IncomingTransferRequest) -> Void)?
    ) {
        self.onDeviceFound = onDeviceFound
        self.onTransfer = onTransfer
        self.onIncomingTransfer = onIncomingTransfer
    }
}
