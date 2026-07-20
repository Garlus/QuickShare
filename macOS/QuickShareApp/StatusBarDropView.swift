import AppKit
import UniformTypeIdentifiers

enum DropViewState {
    case normal
    case dragNear
    case popoverShown
}

final class StatusBarDropView: NSView {
    weak var controller: StatusBarController?
    var state: DropViewState = .normal {
        didSet { needsDisplay = true }
    }

    private var popoverTriggered = false
    private let iconImageView: NSImageView

    override init(frame frameRect: NSRect) {
        iconImageView = NSImageView(frame: NSRect(x: 0, y: 0, width: 18, height: 18))
        super.init(frame: frameRect)

        wantsLayer = true
        layer?.cornerRadius = 4

        registerForDraggedTypes([
            .fileURL,
            .init(rawValue: "public.file-url")
        ])

        let icon = NSImage(named: "AirDropIcon")
        icon?.isTemplate = true
        iconImageView.image = icon
        iconImageView.contentTintColor = .controlTextColor
        iconImageView.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]
        addSubview(iconImageView)
    }

    convenience init() {
        self.init(frame: NSRect(x: 0, y: 0, width: 28, height: NSStatusBar.system.thickness))
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }

        switch state {
        case .normal:
            drawNormalState(context: context)
        case .dragNear:
            drawDragNearState(context: context)
        case .popoverShown:
            drawPopoverShownState(context: context)
        }
    }

    override func layout() {
        super.layout()
        centerIcon()
    }

    private func drawNormalState(context: CGContext) {
        context.setFillColor(NSColor.clear.cgColor)
        context.fill(bounds)
    }

    private func drawDragNearState(context: CGContext) {
        let rect = bounds.insetBy(dx: 4, dy: 4)
        let path = NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6)

        context.setFillColor(NSColor.controlAccentColor.withAlphaComponent(0.15).cgColor)
        path.fill()

        path.lineWidth = 1
        context.setStrokeColor(NSColor.controlAccentColor.withAlphaComponent(0.4).cgColor)
        path.stroke()
    }

    private func drawPopoverShownState(context: CGContext) {
        let rect = bounds.insetBy(dx: 4, dy: 4)
        let path = NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6)

        context.setFillColor(NSColor.controlAccentColor.withAlphaComponent(0.25).cgColor)
        path.fill()

        path.lineWidth = 1.5
        context.setStrokeColor(NSColor.controlAccentColor.withAlphaComponent(0.6).cgColor)
        path.stroke()
    }

    private func centerIcon() {
        let iconSize = iconImageView.frame.size
        iconImageView.frame = NSRect(
            x: (bounds.width - iconSize.width) / 2,
            y: (bounds.height - iconSize.height) / 2,
            width: iconSize.width,
            height: iconSize.height
        )
    }

    // MARK: - DraggingDestination

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard canAcceptDrag(sender) else { return [] }

        controller?.expand()
        popoverTriggered = false
        return .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard canAcceptDrag(sender) else { return [] }

        let loc = convert(sender.draggingLocation, from: nil)
        let dropZoneThreshold: CGFloat = bounds.width * 0.35

        if !popoverTriggered, loc.x > dropZoneThreshold {
            popoverTriggered = true
            controller?.showPopover()
        }

        return .copy
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        if popoverTriggered {
            return
        }
        controller?.collapse()
        popoverTriggered = false
    }

    override func draggingEnded(_ sender: NSDraggingInfo) {
        if popoverTriggered {
            controller?.hidePopover()
        }
        popoverTriggered = false
        controller?.collapse()
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let urls = readFileURLs(from: sender)
        guard !urls.isEmpty else { return false }

        NotificationCenter.default.post(
            name: Notification.Name("FilesDroppedOnStatusItem"),
            object: nil,
            userInfo: ["urls": urls]
        )
        return true
    }

    private func canAcceptDrag(_ sender: NSDraggingInfo) -> Bool {
        !readFileURLs(from: sender).isEmpty
    }

    private func readFileURLs(from info: NSDraggingInfo) -> [URL] {
        info.draggingPasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL] ?? []
    }

    // MARK: - Mouse Events

    override func mouseDown(with event: NSEvent) {
        if let window = NSApp.windows.first(where: { $0.identifier?.rawValue == "main" }) {
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
        }
    }
}
