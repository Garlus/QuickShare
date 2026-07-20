import SwiftUI

struct SettingsView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        Form {
            Section("General") {
                TextField("Device Name", text: Bindable(model).deviceName)
                    .textFieldStyle(.roundedBorder)
            }

            Section {
                Picker("Type", selection: .constant(0)) {
                    Text("Desktop").tag(0)
                    Text("Laptop").tag(1)
                }
                .pickerStyle(.radioGroup)
            } header: {
                Text("Device Type")
            } footer: {
                Text("Other QuickShare devices will see this device type when discovering your Mac.")
            }

            Section("About") {
                HStack {
                    Text("Version")
                    Spacer()
                    Text("0.1.0")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}
