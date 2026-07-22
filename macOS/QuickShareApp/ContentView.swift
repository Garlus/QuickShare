import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @Bindable var model: QuickShareModel
    @Environment(\.openSettings) private var openSettings
    @State private var isDragging: Bool = false

    @State private var droppedFiles: [URL] = []
    @State private var showDevicePicker: Bool = false

    var body: some View {
        ZStack {
            // Background layer — fills entire window
            ProceduralBackgroundView(dragIntensity: isDragging, purpleMode: !model.isActive)
                .ignoresSafeArea()

            // Content layer
            VStack(spacing: 0) {
                Spacer()

                Image(systemName: "square.and.arrow.up")
                    .font(.system(size: 32))
                    .foregroundColor(isDragging ? .accentColor : .secondary)

                Text(isDragging ? "Drop here!" : "Drop to send")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundColor(isDragging ? .accentColor : .secondary)

                Spacer()
            }
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
        .toolbar {
            ToolbarItemGroup(placement: .principal) {
                Picker("Mode", selection: $model.isActive) {
                    Text("Receive off").tag(false)
                    Text("Receive on").tag(true)
                }
                .pickerStyle(.segmented)
                .frame(width: 180)
            }

            ToolbarItem(placement: .automatic) {
                Button(action: { openSettings() }) {
                    Image(systemName: "gearshape")
                        .font(.system(size: 14))
                        .foregroundColor(.primary.opacity(0.8))
                }
            }
        }
        .sheet(isPresented: $showDevicePicker) {
            DevicePickerSheetView(model: model, files: droppedFiles, isPresented: $showDevicePicker)
        }
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
            model.startDiscovery()
        }
    }

    private func sendToDevice(_ device: QuickShareModel.DiscoveredDevice) {
        selectedDevice = device
        withAnimation {
            isSending = true
        }

        model.sendFiles(to: device, fileURLs: files)

        // Dismiss after a short delay to show progress UI
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
            self.isPresented = false
        }
    }
}
