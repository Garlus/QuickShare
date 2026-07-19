import SwiftUI
import Observation

@Observable
class QuickShareModel {
    var isActive: Bool = false
    var isDiscovering: Bool = false
    var deviceName: String = Host.current().localizedName ?? "Mac"
    var discoveredDevices: [DiscoveredDevice] = []
    var transfers: [TransferProgress] = []

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

    func toggleActive() {
        isActive.toggle()
        // Calls into Rust core via QuickShareBridge
    }

    func startDiscovery() {
        isDiscovering = true
    }

    func stopDiscovery() {
        isDiscovering = false
    }
}
