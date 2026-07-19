import AppKit
import Social
import UniformTypeIdentifiers

class ShareViewController: NSViewController {
    override func loadView() {
        view = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 200))
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        guard let extensionItems = extensionContext?.inputItems as? [NSExtensionItem] else {
            cancel()
            return
        }

        var files: [URL] = []

        for item in extensionItems {
            guard let attachments = item.attachments else { continue }

            for provider in attachments {
                guard provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else {
                    continue
                }

                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { [weak self] (urlData, error) in
                    guard let url = urlData as? URL else { return }
                    files.append(url)

                    // When all items are loaded, send via QuickShare
                    DispatchQueue.main.async {
                        self?.sendFiles(files)
                    }
                }
            }
        }
    }

    private func sendFiles(_ urls: [URL]) {
        // Show UI for selecting target device
        let alert = NSAlert()
        alert.messageText = "QuickShare"
        alert.informativeText = "Send \(urls.count) file(s) via QuickShare?"
        alert.addButton(withTitle: "Send")
        alert.addButton(withTitle: "Cancel")

        let response = alert.runModal()
        if response == .alertFirstButtonReturn {
            // TODO: Send to discovered device (needs device picker)
            // For now, call daemon via XPC or direct bridge
            cancel()
        } else {
            cancel()
        }
    }

    private func cancel() {
        extensionContext?.cancelRequest(withError: NSError(
            domain: "com.quickshare",
            code: 0,
            userInfo: [NSLocalizedDescriptionKey: "Cancelled"]
        ))
    }

    private func complete() {
        extensionContext?.completeRequest(returningItems: nil, completionHandler: nil)
    }
}
