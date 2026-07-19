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

        Window("QuickShare", id: "main") {
            ContentView(model: model)
                .frame(minWidth: 420, minHeight: 320)
        }
    }
}
