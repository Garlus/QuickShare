import SwiftUI

struct MenuBarView: View {
    @Bindable var model: QuickShareModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // Header
            HStack(spacing: 6) {
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .foregroundStyle(model.isActive ? .green : .secondary)
                Text("QuickShare")
                    .font(.headline)
                Spacer()
                if model.isActive {
                    Circle()
                        .fill(.green)
                        .frame(width: 8, height: 8)
                }
            }
            .padding(.bottom, 2)

            Divider()

            // Toggle
            Toggle(isOn: Bindable(model).isActive) {
                Label("Receive Files", systemImage: "arrow.down.doc")
            }
            .toggleStyle(.switch)

            // Status
            HStack(spacing: 4) {
                Circle()
                    .fill(model.isDiscovering ? Color.green : Color.gray)
                    .frame(width: 6, height: 6)
                Text(model.isDiscovering ? "Discovering devices..." : "Discovery idle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            // Device list
            if !model.discoveredDevices.isEmpty {
                Divider()

                Text("Nearby Devices")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                ForEach(model.discoveredDevices) { device in
                    HStack(spacing: 6) {
                        Image(systemName: "macbook")
                            .foregroundStyle(.blue)
                            .frame(width: 16)
                        Text(device.name)
                            .font(.subheadline)
                            .lineLimit(1)
                        Spacer()
                        Text(device.connectionType)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 1)
                }
            }

            Divider()

            // Open QuickShare window
            Button("Open QuickShare...") {
                openWindow(id: "main")
                NSApp.activate(ignoringOtherApps: true)
            }
            .buttonStyle(.plain)

            // Settings
            SettingsLink {
                Label("Settings...", systemImage: "gear")
            }
            .buttonStyle(.plain)

            Divider()

            Button("Quit") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(width: 240)
    }
}
