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
    private let networkBrowser = NetworkDiscovery()

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

    struct IncomingFile: Identifiable {
        let id: String
        let requestId: String
        let fileName: String
        let fileSize: Int64
    }

    private var incomingFiles: [IncomingFile] = []
    private var incomingDeviceName: String = ""
    private var incomingDebounceTimer: Timer?

    private func startCore() {
        if core != nil { return }
        core = QuickShare(
            deviceName: deviceName,
            onDeviceFound: { [weak self] device in
                DispatchQueue.main.async {
                    // Deduplicate: check if device already exists
                    if !(self?.discoveredDevices.contains(where: { $0.id == device.id }) ?? true) {
                        self?.discoveredDevices.append(
                            DiscoveredDevice(id: device.id, name: device.name, connectionType: "\(device.connectionType)")
                        )
                    }
                    self?.isDiscovering = true
                }
            },
            onTransfer: { [weak self] progress in
                DispatchQueue.main.async {
                    self?.transfers.append(
                        TransferProgress(id: progress.transferId, fileName: "", bytesSent: progress.bytesSent, bytesTotal: progress.bytesTotal, status: "\(progress.status)")
                    )
                }
            },
            onIncomingTransfer: { [weak self] request in
                DispatchQueue.main.async {
                    self?.handleIncomingTransfer(request)
                }
            }
        )
        // Start Rust core for BLE scanning + advertising
        _ = core?.startDiscovery()
        isDiscovering = true

        // Start native macOS mDNS browser (NWBrowser) for service discovery
        // This works because NWBrowser uses the system's mDNSResponder
        networkBrowser.start { [weak self] deviceName, hostname, ip, port in
            let id = "\(ip):\(port)"
            DispatchQueue.main.async {
                if !(self?.discoveredDevices.contains(where: { $0.id == id }) ?? true) {
                    self?.discoveredDevices.append(
                        DiscoveredDevice(id: id, name: deviceName, connectionType: "mDNS")
                    )
                    self?.isDiscovering = true
                }
            }
        }

        if isActive {
            _ = core?.startAdvertising()
        } else {
            _ = core?.stopAdvertising()
        }
    }

    private func handleIncomingTransfer(_ request: QuickShare.IncomingTransferRequest) {
        let file = IncomingFile(
            id: request.requestId,
            requestId: request.requestId,
            fileName: request.fileName,
            fileSize: request.fileSize
        )
        incomingFiles.append(file)
        incomingDeviceName = request.deviceName

        incomingDebounceTimer?.invalidate()
        incomingDebounceTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
            self?.showBatchedAlert()
        }
    }

    private func showBatchedAlert() {
        let files = incomingFiles
        incomingFiles = []

        guard !files.isEmpty else { return }

        let deviceName = incomingDeviceName
        let alert = NSAlert()
        alert.messageText = "Accept Request"

        if files.count == 1 {
            alert.informativeText = "\(deviceName) wants to share \(files[0].fileName) with you"
        } else {
            alert.informativeText = "\(deviceName) wants to share \(files.count) files with you"
        }

        alert.alertStyle = .informational
        alert.addButton(withTitle: "Accept")
        alert.addButton(withTitle: "Deny")

        NSApp.activate(ignoringOtherApps: true)

        let response = alert.runModal()
        if response == .alertFirstButtonReturn {
            for file in files {
                core?.acceptTransfer(requestId: file.requestId)
            }
        } else {
            for file in files {
                core?.denyTransfer(requestId: file.requestId)
            }
        }

        // Files that arrived while the alert was showing need a new timer
        if !incomingFiles.isEmpty {
            incomingDebounceTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
                self?.showBatchedAlert()
            }
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
        networkBrowser.stop()
        core = nil
        isDiscovering = false
        discoveredDevices.removeAll()
    }

    func startDiscovery() {
        if core == nil { return }
        _ = core?.startDiscovery()
        networkBrowser.start { [weak self] deviceName, hostname, ip, port in
            let id = "\(ip):\(port)"
            DispatchQueue.main.async {
                if !(self?.discoveredDevices.contains(where: { $0.id == id }) ?? true) {
                    self?.discoveredDevices.append(
                        DiscoveredDevice(id: id, name: deviceName, connectionType: "mDNS")
                    )
                    self?.isDiscovering = true
                }
            }
        }
        isDiscovering = true
    }

    func stopDiscovery() {
        _ = core?.stopDiscovery()
        networkBrowser.stop()
        isDiscovering = false
    }
}
