import SwiftUI

struct MenuBarView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Header
            HStack {
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .foregroundStyle(model.isActive ? .green : .secondary)
                Text("QuickShare")
                    .font(.headline)
            }
            .padding(.bottom, 4)

            Divider()

            // Toggle
            Toggle(isOn: Bindable(model).isActive) {
                Label("Receive Files", systemImage: "arrow.down.doc")
            }
            .toggleStyle(.switch)

            // Status
            HStack {
                Circle()
                    .fill(model.isDiscovering ? Color.green : Color.gray)
                    .frame(width: 8, height: 8)
                Text(model.isDiscovering ? "Discovering devices..." : "Discovery idle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Divider()

            // Device list
            if !model.discoveredDevices.isEmpty {
                Text("Nearby Devices")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                ForEach(model.discoveredDevices) { device in
                    HStack {
                        Image(systemName: "macbook")
                            .foregroundStyle(.blue)
                        Text(device.name)
                            .font(.subheadline)
                        Spacer()
                        Text(device.connectionType)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }

                Divider()
            }

            // Navigation
            Button("Open QuickShare...") {
                NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
            }
            .buttonStyle(.plain)

            Divider()

            Button("Quit") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
        .padding()
        .frame(width: 260)
    }
}
