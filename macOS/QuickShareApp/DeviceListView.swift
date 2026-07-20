import SwiftUI
import UniformTypeIdentifiers

struct DeviceListView: View {
    struct DeviceItem: Identifiable {
        let id: String
        let name: String
        let iconName: String
    }

    let devices: [DeviceItem]
    let onSend: (String, [URL]) -> Void
    @State private var hoveredDeviceId: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Send to:")
                .font(.subheadline.weight(.semibold))
                .foregroundColor(.secondary)
                .padding(.horizontal, 16)
                .padding(.top, 16)
                .padding(.bottom, 8)

            Divider()
                .padding(.horizontal, 8)

            if devices.isEmpty {
                VStack(spacing: 8) {
                    Spacer()
                    ProgressView()
                        .scaleEffect(0.8)
                    Text("Looking for devices...")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Spacer()
                }
            } else {
                ScrollView {
                    VStack(spacing: 4) {
                        ForEach(devices) { device in
                            DeviceRow(
                                id: device.id,
                                name: device.name,
                                iconName: device.iconName,
                                isHovered: hoveredDeviceId == device.id,
                                onDrop: { urls in
                                    onSend(device.id, urls)
                                }
                            )
                            .onHover { hovering in
                                withAnimation(.easeInOut(duration: 0.15)) {
                                    hoveredDeviceId = hovering ? device.id : nil
                                }
                            }
                        }
                    }
                    .padding(8)
                }
            }
        }
        .frame(width: 240, height: 280)
        .background(Color(.windowBackgroundColor))
    }
}

private struct DeviceRow: View {
    let id: String
    let name: String
    let iconName: String
    let isHovered: Bool
    let onDrop: ([URL]) -> Void

    @State private var isDragTarget: Bool = false

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: iconName)
                .font(.title3)
                .foregroundColor(.accentColor)
                .frame(width: 28)

            Text(name)
                .font(.body)

            Spacer()

            Image(systemName: "arrow.up.doc")
                .font(.caption)
                .foregroundColor(.secondary.opacity(isDragTarget ? 1 : 0))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(isDragTarget
                      ? Color.accentColor.opacity(0.15)
                      : (isHovered
                         ? Color.primary.opacity(0.06)
                         : Color.clear))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(
                    isDragTarget
                    ? Color.accentColor.opacity(0.6)
                    : Color.clear,
                    lineWidth: 1.5
                )
        )
        .contentShape(RoundedRectangle(cornerRadius: 8))
        .onDrop(of: [.fileURL], isTargeted: $isDragTarget) { providers in
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
                    onDrop(urls)
                }
            }
            return true
        }
    }
}
