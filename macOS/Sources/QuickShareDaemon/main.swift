import Foundation
import QuickShareBridge

/// QuickShare background daemon for macOS.
/// Runs as a LaunchAgent, handles file transfers and device discovery.
@main
struct QuickShareDaemon {
    static func main() async {
        print("QuickShare Daemon v\(QuickShare.version) starting...")

        let quickshare = QuickShare(
            deviceName: Host.current().localizedName ?? "Mac",
            onDeviceFound: { device in
                print("[Discovery] Found device: \(device.name) (\(device.id)) via \(device.connectionType)")
            },
            onTransfer: { progress in
                print("[Transfer] \(progress.transferId): \(progress.bytesSent)/\(progress.bytesTotal) bytes [\(progress.status)]")
            }
        )

        // Start discovery and advertising
        let discoveryStarted = quickshare.startDiscovery()
        let advertisingStarted = quickshare.startAdvertising()

        print("Discovery: \(discoveryStarted ? "active" : "failed")")
        print("Advertising: \(advertisingStarted ? "active" : "failed")")

        // Keep the daemon alive
        print("Daemon running. Press Ctrl+C to stop.")

        // Signal handling for graceful shutdown
        let signalSource = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
        signalSource.setEventHandler {
            print("\nShutting down...")
            _ = quickshare.stopDiscovery()
            _ = quickshare.stopAdvertising()
            exit(0)
        }
        signalSource.resume()

        // Run loop
        dispatchMain()
    }
}
