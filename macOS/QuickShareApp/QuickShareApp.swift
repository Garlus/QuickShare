import SwiftUI
import Observation

@main
struct QuickShareApp: App {
    @State private var model = QuickShareModel()

    var body: some Scene {
        MenuBarExtra {
            MenuBarView(model: model)
        } label: {
            Image(systemName: model.isActive
                  ? "antenna.radiowaves.left.and.right"
                  : "antenna.radiowaves.left.and.right.slash")
        }
        .menuBarExtraStyle(.menu)

        Window("QuickShare", id: "main") {
            ContentView(model: model)
                .frame(minWidth: 460, minHeight: 320)
                .background(WindowAccessor()) // make title bar transparent
        }
        .windowStyle(.hiddenTitleBar) // hides standard title text

        Settings {
            SettingsView(model: model)
                .frame(width: 400, height: 300)
        }
    }
}

// Helper to access the NSWindow and make the titlebar completely transparent
struct WindowAccessor: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            if let window = view.window {
                window.titlebarAppearsTransparent = true
                window.titleVisibility = .hidden
                window.backgroundColor = .windowBackgroundColor
                window.isMovableByWindowBackground = true
            }
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}
