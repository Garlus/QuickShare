import SwiftUI
import Observation

@Observable
class QuickShareModel {
    var isActive: Bool = false {
        didSet {
            updateCoreState()
        }
    }
    var isDiscovering: Bool = false
    var deviceName: String = Host.current().localizedName ?? "Mac"
    var discoveredDevices: [DiscoveredDevice] = []
    var transfers: [TransferProgress] = []

    private var core: QuickShare?

    init() {
        startCore()
    }

    struct DiscoveredDevice: Identifiable {
        let id: String
        let name: String
        let connectionType: String
    }

    struct TransferProgress: Identifiable {
        let id: String
        let fileName: String
        let bytesSent: Int64
        let bytesTotal: Int64
        let status: String
    }

    private func startCore() {
        if core != nil { return }
        core = QuickShare(
            deviceName: deviceName,
            onDeviceFound: { [weak self] device in
                DispatchQueue.main.async {
                    self?.discoveredDevices.append(
                        DiscoveredDevice(id: device.id, name: device.name, connectionType: "\(device.connectionType)")
                    )
                    self?.isDiscovering = true
                }
            },
            onTransfer: { [weak self] progress in
                DispatchQueue.main.async {
                    self?.transfers.append(
                        TransferProgress(id: progress.transferId, fileName: "", bytesSent: progress.bytesSent, bytesTotal: progress.bytesTotal, status: "\(progress.status)")
                    )
                }
            }
        )
        _ = core?.startDiscovery()
        isDiscovering = true
        
        if isActive {
            _ = core?.startAdvertising()
        } else {
            _ = core?.stopAdvertising()
        }
    }

    private func updateCoreState() {
        if core == nil {
            startCore()
            return
        }
        if isActive {
            _ = core?.startAdvertising()
        } else {
            _ = core?.stopAdvertising()
        }
    }

    private func stopCore() {
        _ = core?.stopAdvertising()
        _ = core?.stopDiscovery()
        core = nil
        isDiscovering = false
        discoveredDevices.removeAll()
    }

    func startDiscovery() {
        if core == nil { return }
        _ = core?.startDiscovery()
        isDiscovering = true
    }

    func stopDiscovery() {
        _ = core?.stopDiscovery()
        isDiscovering = false
    }
}
