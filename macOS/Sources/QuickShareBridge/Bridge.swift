import Foundation
import CQuickShare

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
        case phone = 0
        case tablet = 1
        case laptop = 2
        case desktop = 3
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

    private let ctx: OpaquePointer
    private let deviceFoundHandler: ((DiscoveredDevice) -> Void)?
    private let transferHandler: ((TransferProgress) -> Void)?

    public init(
        deviceName: String = "QuickShare Desktop",
        onDeviceFound: ((DiscoveredDevice) -> Void)? = nil,
        onTransfer: ((TransferProgress) -> Void)? = nil
    ) {
        self.deviceFoundHandler = onDeviceFound
        self.transferHandler = onTransfer

        let userData = Unmanaged.passRetained(QuickShareCallbackContext(
            onDeviceFound: onDeviceFound,
            onTransfer: onTransfer
        )).toOpaque()

        let namePtr = (deviceName as NSString).utf8String
        self.ctx = qs_init(namePtr, nil)!

        if onDeviceFound != nil {
            qs_set_device_found_callback(ctx, deviceFoundCallback, userData)
        }

        if onTransfer != nil {
            qs_set_transfer_callback(ctx, transferCallback, userData)
        }
    }

    deinit {
        qs_shutdown(ctx)
    }

    // MARK: - Public API

    public func startAdvertising(deviceType: DeviceType = .desktop) -> Bool {
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

    public var isAdvertising: Bool {
        qs_is_advertising(ctx) != 0
    }

    public var isDiscovering: Bool {
        qs_is_discovering(ctx) != 0
    }

    public static var version: String {
        String(cString: qs_version())
    }
}

// MARK: - C Callback Trampolines

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

// MARK: - Callback Context

private class QuickShareCallbackContext {
    let onDeviceFound: ((QuickShare.DiscoveredDevice) -> Void)?
    let onTransfer: ((QuickShare.TransferProgress) -> Void)?

    init(
        onDeviceFound: ((QuickShare.DiscoveredDevice) -> Void)?,
        onTransfer: ((QuickShare.TransferProgress) -> Void)?
    ) {
        self.onDeviceFound = onDeviceFound
        self.onTransfer = onTransfer
    }
}
