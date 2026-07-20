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
            // Custom Toolbar/Header to match mockup exactly
            HStack {
                // Invisible spacer to balance the traffic lights
                Spacer()
                    .frame(width: 70)

                Spacer()

                // Center Off / Share and Recieve Pill Toggle
                HStack(spacing: 0) {
                    Text("Off")
                        .font(.system(size: 13, weight: .medium))
                        .padding(.horizontal, 14)
                        .padding(.vertical, 6)
                        .background(!model.isActive ? Color.white : Color.clear)
                        .clipShape(Capsule())
                        .foregroundColor(!model.isActive ? .primary : .secondary)
                        .shadow(color: !model.isActive ? Color.black.opacity(0.08) : Color.clear, radius: 1, x: 0, y: 1)
                        .onTapGesture {
                            withAnimation(.spring(response: 0.25, dampingFraction: 0.8)) {
                                model.isActive = false
                            }
                        }

                    Divider()
                        .frame(height: 16)
                        .padding(.horizontal, 4)

                    Text("Share and Recieve")
                        .font(.system(size: 13, weight: .medium))
                        .padding(.horizontal, 14)
                        .padding(.vertical, 6)
                        .background(model.isActive ? Color.white : Color.clear)
                        .clipShape(Capsule())
                        .foregroundColor(model.isActive ? .primary : .secondary)
                        .shadow(color: model.isActive ? Color.black.opacity(0.08) : Color.clear, radius: 1, x: 0, y: 1)
                        .onTapGesture {
                            withAnimation(.spring(response: 0.25, dampingFraction: 0.8)) {
                                model.isActive = true
                            }
                        }
                }
                .padding(3)
                .background(Color(.windowBackgroundColor).opacity(0.6))
                .clipShape(Capsule())
                .overlay(
                    Capsule()
                        .stroke(Color.black.opacity(0.05), lineWidth: 0.5)
                )

                Spacer()

                // Right Settings Button
                Button(action: { showSettings = true }) {
                    Image(systemName: "gearshape")
                        .font(.system(size: 16))
                        .foregroundColor(.primary.opacity(0.8))
                        .frame(width: 32, height: 32)
                        .background(Color(.windowBackgroundColor).opacity(0.6))
                        .clipShape(Circle())
                        .overlay(
                            Circle()
                                .stroke(Color.black.opacity(0.05), lineWidth: 0.5)
                        )
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .padding(.top, 16)
            .padding(.bottom, 24)

            // Center Drop Zone
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
            .padding(.bottom, 24)
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
        .background(Color(.windowBackgroundColor))
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
                .navigationBarTitleDisplayMode(.inline)
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

        // Simulate progress for UI feedback (will link to real transfer in future)
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
