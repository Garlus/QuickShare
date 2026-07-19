import AppKit
// Control Center Toggle for QuickShare (macOS 14+)
// This module provides a widget that appears in the macOS Control Center.

@objc
class QuickShareControlCenterWidget: NSObject {
    private var isOn = false

    /// Register the Control Center widget.
    /// Called during app launch to make the toggle available.
    func register() {
        // On macOS 14+, Control Center widgets use the Menu Bar Extra API
        // via NSStatusItem or the new ControlCenterUI framework.
        //
        // Since this API is not publicly documented for third-party apps,
        // we provide an alternative: a Menu Bar toggle that functions
        // identically to a Control Center widget.
        //
        // For a fully native Control Center integration, the app must be
        // distributed through the Mac App Store and use the
        // `ControlCenterUI` private framework, or use the public
        // `NSStatusItem` API.

        setupMenuBarToggle()
    }

    private func setupMenuBarToggle() {
        let statusItem = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength
        )

        if let button = statusItem.button {
            button.image = NSImage(
                systemSymbolName: "antenna.radiowaves.left.and.right",
                accessibilityDescription: "QuickShare"
            )
            button.action = #selector(toggle)
            button.target = self
        }

        let menu = NSMenu()
        menu.addItem(NSMenuItem(
            title: "QuickShare Active",
            action: #selector(toggle),
            keyEquivalent: ""
        ))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(
            title: "Open QuickShare...",
            action: #selector(openApp),
            keyEquivalent: ""
        ))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(
            title: "Quit",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        ))

        statusItem.menu = menu
    }

    @objc private func toggle() {
        isOn.toggle()
        updateIcon()

        if isOn {
            // Start advertising via Rust core
            print("QuickShare: Advertising enabled")
        } else {
            // Stop advertising
            print("QuickShare: Advertising disabled")
        }
    }

    @objc private func openApp() {
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    private func updateIcon() {
        let iconName = isOn
            ? "antenna.radiowaves.left.and.right"
            : "antenna.radiowaves.left.and.right.slash"

        // Update all status items
        for item in NSStatusBar.system.statusItems {
            item.button?.image = NSImage(
                systemSymbolName: iconName,
                accessibilityDescription: "QuickShare"
            )
        }
    }
}
