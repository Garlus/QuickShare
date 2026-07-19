import SwiftUI

struct ContentView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        TabView {
            DevicesView(model: model)
                .tabItem {
                    Label("Devices", systemImage: "macbook.and.iphone")
                }

            TransfersView(model: model)
                .tabItem {
                    Label("Transfers", systemImage: "arrow.up.arrow.down")
                }

            SettingsView(model: model)
                .tabItem {
                    Label("Settings", systemImage: "gear")
                }
        }
        .padding()
    }
}

struct DevicesView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        VStack {
            HStack {
                Text("Nearby Devices")
                    .font(.title2)
                Spacer()
                Button(model.isDiscovering ? "Stop" : "Scan") {
                    if model.isDiscovering {
                        model.stopDiscovery()
                    } else {
                        model.startDiscovery()
                    }
                }
                .buttonStyle(.borderedProminent)
            }
            .padding(.bottom)

            if model.discoveredDevices.isEmpty {
                ContentUnavailableView(
                    "No Devices Found",
                    systemImage: "antenna.radiowaves.left.and.right.slash",
                    description: Text("Make sure QuickShare is enabled on nearby devices.")
                )
            } else {
                List(model.discoveredDevices) { device in
                    HStack {
                        Image(systemName: "macbook")
                            .font(.title2)
                        VStack(alignment: .leading) {
                            Text(device.name)
                                .font(.headline)
                            Text(device.connectionType)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Send") {
                            // TODO: Send file
                        }
                        .buttonStyle(.borderless)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }
}

struct TransfersView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        VStack {
            Text("Transfers")
                .font(.title2)
                .padding(.bottom)

            if model.transfers.isEmpty {
                ContentUnavailableView(
                    "No Transfers",
                    systemImage: "arrow.up.arrow.down",
                    description: Text("Send or receive files to see transfer progress.")
                )
            } else {
                List(model.transfers) { transfer in
                    VStack(alignment: .leading) {
                        Text(transfer.fileName)
                            .font(.headline)
                        ProgressView(
                            value: Double(transfer.bytesSent),
                            total: Double(transfer.bytesTotal)
                        )
                        Text("\(transfer.bytesSent) / \(transfer.bytesTotal) bytes")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }
}

struct SettingsView: View {
    @Bindable var model: QuickShareModel

    var body: some View {
        Form {
            Section("General") {
                TextField("Device Name", text: Bindable(model).deviceName)
                    .textFieldStyle(.roundedBorder)

                Toggle("Receive Files", isOn: Bindable(model).isActive)
            }

            Section("Device Type") {
                Picker("Type", selection: .constant(0)) {
                    Text("Desktop").tag(0)
                    Text("Laptop").tag(1)
                }
                .pickerStyle(.radioGroup)
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
        .frame(width: 400)
    }
}
