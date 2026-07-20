import SwiftUI

struct MenuBarView: View {
    @Bindable var model: QuickShareModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("QuickShare")
                .font(.headline)

            Divider()

            Button("Open QuickShare") {
                openWindow(id: "main")
                NSApp.activate(ignoringOtherApps: true)
            }
            .buttonStyle(.plain)

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
        .frame(width: 200)
        .onReceive(NotificationCenter.default.publisher(for: Notification.Name("ReopenMainWindow"))) { _ in
            openWindow(id: "main")
            NSApp.activate(ignoringOtherApps: true)
        }
    }
}
