import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @Bindable var model: QuickShareModel
    @State private var isDragging: Bool = false
    @State private var showSettings: Bool = false
    @State private var droppedFiles: [URL] = []
    @State private var showDevicePicker: Bool = false

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                RoundedRectangle(cornerRadius: 16)
                    .fill(isDragging ? Color.accentColor.opacity(0.05) : Color.clear)
                    .overlay(
                        RoundedRectangle(cornerRadius: 16)
                            .strokeBorder(isDragging ? Color.accentColor : Color.clear, style: StrokeStyle(lineWidth: 2, dash: [6]))
                    )
                    .animation(.easeInOut(duration: 0.2), value: isDragging)

                VStack(spacing: 12) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.system(size: 32))
                        .foregroundColor(isDragging ? .accentColor : .primary.opacity(0.75))
                        .scaleEffect(isDragging ? 1.1 : 1.0)
                        .animation(.spring(response: 0.3, dampingFraction: 0.7), value: isDragging)

                    Text(isDragging ? "Drop here!" : "Drop to send")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundColor(isDragging ? .accentColor : .secondary)
                }
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 24)
            .onDrop(of: [.fileURL], isTargeted: $isDragging) { providers in
                let group = DispatchGroup()
                var urls: [URL] = []

                for provider in providers {
                    group.enter()
                    _ = provider.loadObject(ofClass: URL.self) { url, _ in
                        if let url = url {
                            urls.append(url)
                        }
                        group.leave()
                    }
                }

                group.notify(queue: .main) {
                    if !urls.isEmpty {
                        self.droppedFiles = urls
                        self.showDevicePicker = true
                    }
                }
                return true
            }
        }
        .background {
            if model.isActive {
                ProceduralBackgroundView()
                    .ignoresSafeArea()
            } else {
                Color(.windowBackgroundColor)
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .principal) {
                SingleSelectionSegmentedControl(
                    ["Off", "Share and Receive"],
                    selection: $model.isActive
                )
            }

            ToolbarItem(placement: .automatic) {
                Button(action: { showSettings = true }) {
                    Image(systemName: "gearshape")
                        .font(.system(size: 14))
                        .foregroundColor(.primary.opacity(0.8))
                }
            }
        }
        .sheet(isPresented: $showSettings) {
            SettingsSheetView(model: model, isPresented: $showSettings)
        }
        .sheet(isPresented: $showDevicePicker) {
            DevicePickerSheetView(model: model, files: droppedFiles, isPresented: $showDevicePicker)
        }
    }
}

// MARK: - Settings Sheet View

struct SettingsSheetView: View {
    @Bindable var model: QuickShareModel
    @Binding var isPresented: Bool

    var body: some View {
        NavigationStack {
            SettingsView(model: model)
                .navigationTitle("Settings")

                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") {
                            isPresented = false
                        }
                    }
                }
        }
        .frame(width: 420, height: 340)
    }
}

// MARK: - Device Picker Sheet View

struct DevicePickerSheetView: View {
    @Bindable var model: QuickShareModel
    let files: [URL]
    @Binding var isPresented: Bool
    @State private var selectedDevice: QuickShareModel.DiscoveredDevice?
    @State private var isSending: Bool = false
    @State private var sendProgress: Double = 0.0

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(isSending ? "Sending File..." : "Choose Target Device")
                    .font(.headline)
                Spacer()
                if !isSending {
                    Button("Cancel") {
                        isPresented = false
                    }
                    .buttonStyle(.plain)
                    .foregroundColor(.secondary)
                }
            }
            .padding()

            Divider()

            if isSending {
                VStack(spacing: 16) {
                    ProgressView(value: sendProgress, total: 1.0)
                        .progressViewStyle(.linear)
                        .padding(.horizontal)

                    Text("Sending \(files.first?.lastPathComponent ?? "File") to \(selectedDevice?.name ?? "Device")...")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
                .padding(.vertical, 40)
            } else {
                if model.discoveredDevices.isEmpty {
                    VStack(spacing: 16) {
                        ProgressView()
                            .scaleEffect(0.8)
                        Text("Looking for nearby QuickShare devices...")
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                    }
                    .padding(.vertical, 40)
                    .frame(maxWidth: .infinity)
                } else {
                    List(model.discoveredDevices) { device in
                        HStack {
                            Image(systemName: "macbook")
                                .font(.title3)
                                .foregroundColor(.accentColor)
                            VStack(alignment: .leading) {
                                Text(device.name)
                                    .font(.headline)
                                Text(device.connectionType)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                            Spacer()
                            Button("Send") {
                                sendToDevice(device)
                            }
                            .buttonStyle(.borderedProminent)
                        }
                        .padding(.vertical, 4)
                    }
                    .listStyle(.sidebar)
                }
            }
        }
        .frame(width: 360, height: 280)
        .onAppear {
            if !model.isActive {
                model.isActive = true
            }
            model.startDiscovery()
        }
    }

    private func sendToDevice(_ device: QuickShareModel.DiscoveredDevice) {
        selectedDevice = device
        withAnimation {
            isSending = true
        }

        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { timer in
            if sendProgress < 1.0 {
                sendProgress += 0.05
            } else {
                timer.invalidate()
                isPresented = false
            }
        }
    }
}
