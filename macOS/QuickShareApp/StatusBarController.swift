import AppKit
import SwiftUI

final class StatusBarController: NSObject {
    let statusItem: NSStatusItem
    let popover: NSPopover
    let dropView: StatusBarDropView
    private var hostingController: NSHostingController<DeviceListView>?

    override init() {
        statusItem = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength
        )
        popover = NSPopover()
        dropView = StatusBarDropView()

        super.init()

        statusItem.view = dropView
        dropView.controller = self

        popover.contentSize = NSSize(width: 240, height: 280)
        popover.behavior = .transient
        popover.delegate = self
    }

    func expand() {
        statusItem.length = 56
        dropView.state = .dragNear
        dropView.needsDisplay = true
    }

    func collapse() {
        statusItem.length = 28
        dropView.state = .normal
        dropView.needsDisplay = true
    }

    func showPopover() {
        guard popover.isShown == false else { return }

        let deviceView = DeviceListView(
            devices: sampleDevices(),
            onSend: { [weak self] deviceId, urls in
                self?.handleSend(to: deviceId, files: urls)
                self?.hidePopover()
            }
        )
        hostingController = NSHostingController(rootView: deviceView)
        popover.contentViewController = hostingController
        popover.show(relativeTo: dropView.bounds,
                     of: dropView,
                     preferredEdge: .minY)
        dropView.state = .popoverShown
        dropView.needsDisplay = true
    }

    private func sampleDevices() -> [DeviceListView.DeviceItem] {
        [
            .init(id: "phone", name: "Phone", iconName: "iphone"),
            .init(id: "pc", name: "PC", iconName: "desktopcomputer"),
        ]
    }

    func hidePopover() {
        popover.performClose(nil)
        hostingController = nil
        dropView.state = .normal
        dropView.needsDisplay = true
        collapse()
    }

    private func handleSend(to deviceId: String, files: [URL]) {
        NotificationCenter.default.post(
            name: Notification.Name("SendFilesToDevice"),
            object: nil,
            userInfo: ["deviceId": deviceId, "urls": files]
        )
    }
}

extension StatusBarController: NSPopoverDelegate {
    func popoverDidClose(_ notification: Notification) {
        dropView.state = .normal
        dropView.needsDisplay = true
        collapse()
    }
}
