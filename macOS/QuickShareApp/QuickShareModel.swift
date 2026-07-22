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
    private let bleAdvertiser = BleAdvertiser()
    private let mdnsAdvertiser = MdnsAdvertiser()

    init() {
        startCore()
    }

    struct DiscoveredDevice: Identifiable {
        let id: String
        let name: String
        let endpointId: String
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
    private var isSending = false

    private func startCore() {
        if core != nil { return }
        core = QuickShare(
            deviceName: deviceName,
            onDeviceFound: { [weak self] device in
                DispatchQueue.main.async {
                    // Deduplicate: check if device already exists
                    if !(self?.discoveredDevices.contains(where: { $0.id == device.id }) ?? true) {
                        self?.discoveredDevices.append(
                            DiscoveredDevice(id: device.id, name: device.name, endpointId: device.id, connectionType: "\(device.connectionType)")
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
        networkBrowser.start { [weak self] deviceName, endpointId, ip, port in
            let id = "\(ip):\(port)"
            let epId = endpointId.isEmpty ? Data(deviceName.utf8).base64EncodedString()
                .replacingOccurrences(of: "+", with: "-")
                .replacingOccurrences(of: "/", with: "_")
                .replacingOccurrences(of: "=", with: "") : endpointId
            DispatchQueue.main.async {
                if !(self?.discoveredDevices.contains(where: { $0.id == id }) ?? true) {
                    self?.discoveredDevices.append(
                        DiscoveredDevice(id: id, name: deviceName, endpointId: epId, connectionType: "mDNS")
                    )
                    self?.isDiscovering = true
                }
            }
        }

        if isActive {
            startAdvertising()
        } else {
            stopAdvertising()
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
            startAdvertising()
        } else {
            stopAdvertising()
        }
    }

    private func startAdvertising() {
        guard let core = core else {
            NSLog("[Model] startAdvertising: core is nil")
            return
        }

        let advResult = core.startAdvertising()
        NSLog("[Model] core.startAdvertising() returned: \(advResult)")
        guard advResult else {
            NSLog("[Model] startAdvertising: core.startAdvertising() returned false")
            return
        }

        guard let endpointIdStr = core.getEndpointId() else {
            NSLog("[Model] startAdvertising: getEndpointId() returned nil")
            return
        }
        NSLog("[Model] endpointIdStr: \(endpointIdStr)")

        guard let endpointId = decodeEndpointId(endpointIdStr) else {
            NSLog("[Model] Failed to decode endpointId: \(endpointIdStr)")
            return
        }

        // Start TCP listener for incoming transfers
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first?.path ?? "/tmp"
        _ = core.startListener(saveDir: downloads)
        NSLog("[Model] TCP listener started")

        bleAdvertiser.startAdvertising(endpointId: endpointId)
        mdnsAdvertiser.start(endpointId: endpointId, deviceName: deviceName)

        NSLog("[Model] Advertising started: BLE + mDNS")
    }

    private func stopAdvertising() {
        bleAdvertiser.stopAdvertising()
        mdnsAdvertiser.stop()
        _ = core?.stopListener()
        _ = core?.stopAdvertising()
        NSLog("[Model] Advertising + listener stopped")
    }

    private func decodeEndpointId(_ str: String) -> [UInt8]? {
        let base64 = str
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let paddedLen = ((base64.count + 3) / 4) * 4
        let padded = base64.padding(toLength: paddedLen, withPad: "=", startingAt: 0)
        guard let data = Data(base64Encoded: padded), data.count == 4 else { return nil }
        return [UInt8](data)
    }

    private func stopCore() {
        bleAdvertiser.stopAdvertising()
        mdnsAdvertiser.stop()
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
        networkBrowser.start { [weak self] deviceName, endpointId, ip, port in
            let id = "\(ip):\(port)"
            let epId = endpointId.isEmpty ? Data(deviceName.utf8).base64EncodedString()
                .replacingOccurrences(of: "+", with: "-")
                .replacingOccurrences(of: "/", with: "_")
                .replacingOccurrences(of: "=", with: "") : endpointId
            DispatchQueue.main.async {
                if !(self?.discoveredDevices.contains(where: { $0.id == id }) ?? true) {
                    self?.discoveredDevices.append(
                        DiscoveredDevice(id: id, name: deviceName, endpointId: epId, connectionType: "mDNS")
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

    func sendFiles(to device: DiscoveredDevice, fileURLs: [URL]) {
        guard !isSending, let core = core else { return }
        isSending = true

        let parts = device.id.split(separator: ":")
        let ip: String
        let port: Int32
        if parts.count == 2 {
            ip = String(parts[0])
            port = Int32(parts[1]) ?? 5721
        } else {
            ip = device.id
            port = 5721
        }

        let endpointId = device.endpointId.isEmpty ? Data(device.name.utf8).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "") : device.endpointId

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            for url in fileURLs {
                let path = url.path
                let fileName = url.lastPathComponent

                DispatchQueue.main.async {
                    self.transfers.append(
                        TransferProgress(id: UUID().uuidString, fileName: fileName, bytesSent: 0, bytesTotal: 1, status: "Sending")
                    )
                }

                NSLog("[Model] Sending \(path) to \(ip):\(port)")
                let result = core.sendFile(deviceIp: ip, port: port, endpointId: endpointId, filePath: path)

                DispatchQueue.main.async {
                    if result {
                        NSLog("[Model] Successfully sent \(fileName)")
                        self.transfers.append(
                            TransferProgress(id: UUID().uuidString, fileName: fileName, bytesSent: 1, bytesTotal: 1, status: "Completed")
                        )
                    } else {
                        NSLog("[Model] Failed to send \(fileName)")
                        self.transfers.append(
                            TransferProgress(id: UUID().uuidString, fileName: fileName, bytesSent: 0, bytesTotal: 1, status: "Failed")
                        )
                    }
                }
            }

            DispatchQueue.main.async {
                self.isSending = false
            }
        }
    }
}
