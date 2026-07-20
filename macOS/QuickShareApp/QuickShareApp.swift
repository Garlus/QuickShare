import SwiftUI
import Observation

class AppDelegate: NSObject, NSApplicationDelegate {
    var statusBarController: StatusBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusBarController = StatusBarController()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            if let existing = sender.windows.first(where: { $0.identifier?.rawValue == "main" }) {
                existing.makeKeyAndOrderFront(nil)
            }
        } else {
            sender.windows.forEach { window in
                if window.identifier?.rawValue == "main" {
                    window.makeKeyAndOrderFront(nil)
                }
            }
        }
        sender.activate(ignoringOtherApps: true)
        return true
    }
}

@main
struct QuickShareApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var model = QuickShareModel()

    var body: some Scene {
        Window("QuickShare", id: "main") {
            ContentView(model: model)
                .frame(minWidth: 320, minHeight: 320)
        }
        .windowStyle(.hiddenTitleBar)

        Settings {
            SettingsView(model: model)
                .frame(width: 400, height: 300)
        }
    }
}
