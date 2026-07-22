import Foundation
import Network

/// Native macOS mDNS discovery using NWBrowser (uses system mDNSResponder).
/// This replaces the mdns-sd crate's browse functionality which doesn't work on macOS.
class NetworkDiscovery {
    private var browser: NWBrowser?
    private var onDeviceFound: ((String, String, String, UInt16) -> Void)?
    private let queue = DispatchQueue(label: "com.quickshare.mdns-browser", qos: .userInitiated)

    func start(onDeviceFound: @escaping (String, String, String, UInt16) -> Void) {
        guard browser == nil else {
            NSLog("[MdnsBrowser] Already running")
            return
        }

        self.onDeviceFound = onDeviceFound

        let params = NWParameters()
        params.includePeerToPeer = true

        let browser = NWBrowser(
            for: .bonjour(type: "_FC9F5ED42C8A._tcp", domain: nil),
            using: params
        )

        browser.stateUpdateHandler = { [weak self] state in
            switch state {
            case .ready:
                NSLog("[MdnsBrowser] Browser ready, scanning for QuickShare services...")
            case .failed(let error):
                let nsError = error as NSError
                if nsError.code == -65555 {
                    NSLog("[MdnsBrowser] Browser failed: NoAuth (-65555) — Local Network permission denied.")
                    NSLog("[MdnsBrowser] → System Settings > Privacy & Security > Local Network > enable QuickShare")
                } else {
                    NSLog("[MdnsBrowser] Browser failed: \(error) (code=\(nsError.code))")
                }
                self?.browser = nil
            case .cancelled:
                NSLog("[MdnsBrowser] Browser cancelled")
                self?.browser = nil
            case .waiting(let error):
                NSLog("[MdnsBrowser] Browser waiting: \(error)")
            default:
                NSLog("[MdnsBrowser] Browser state: \(String(describing: state))")
            }
        }

        browser.browseResultsChangedHandler = { [weak self] results, changes in
            NSLog("[MdnsBrowser] Results changed: \(results.count) total, \(changes.count) changes")

            for change in changes {
                switch change {
                case .added(let result):
                    NSLog("[MdnsBrowser] Service added: \(result.endpoint)")
                    self?.handleServiceFound(result)
                case .removed(let result):
                    NSLog("[MdnsBrowser] Service removed: \(result.endpoint)")
                @unknown default:
                    break
                }
            }
        }

        browser.start(queue: queue)
        self.browser = browser
        NSLog("[MdnsBrowser] Started browsing for _FC9F5ED42C8A._tcp")
    }

    func stop() {
        browser?.cancel()
        browser = nil
        NSLog("[MdnsBrowser] Stopped")
    }

    private func handleServiceFound(_ result: NWBrowser.Result) {
        // Extract TXT record from bonjour metadata
        let metadata = result.metadata
        guard case .bonjour(let txtRecord) = metadata else {
            NSLog("[MdnsBrowser] No bonjour metadata")
            return
        }

        // Get the "n" property from the TXT record
        guard let entry = txtRecord.getEntry(for: "n") else {
            NSLog("[MdnsBrowser] No 'n' TXT record entry")
            return
        }

        let nValue: String
        switch entry {
        case .string(let str):
            nValue = str
        case .data(let data):
            nValue = String(data: data, encoding: .utf8) ?? ""
        @unknown default:
            NSLog("[MdnsBrowser] Unknown TXT entry type")
            return
        }

        guard !nValue.isEmpty else {
            NSLog("[MdnsBrowser] Empty 'n' TXT record value")
            return
        }

        NSLog("[MdnsBrowser] TXT n='\(nValue)'")

        // Decode endpoint info to get device name and type
        guard let (deviceType, deviceName) = Self.parseEndpointInfo(nValue) else {
            NSLog("[MdnsBrowser] Failed to parse endpoint info from '\(nValue)'")
            return
        }

        let endpointId = Self.extractEndpointId(from: result.endpoint) ?? ""
        NSLog("[MdnsBrowser] Device: name='\(deviceName)' type=\(deviceType) endpointId=\(endpointId)")

        // Resolve the endpoint to get IP:port
        resolveEndpoint(result.endpoint) { [weak self] ip, port in
            guard let ip = ip, port > 0 else {
                NSLog("[MdnsBrowser] Failed to resolve endpoint to IP:port")
                return
            }

            NSLog("[MdnsBrowser] Resolved to \(ip):\(port)")
            self?.onDeviceFound?(deviceName, endpointId, ip, port)
        }
    }

    /// Extract 4-byte base64url endpoint ID from Bonjour service instance name.
    static func extractEndpointId(from endpoint: NWEndpoint) -> String? {
        guard case .service(let name, _, _, _) = endpoint else { return nil }
        var padded = name
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let remainder = padded.count % 4
        if remainder != 0 {
            padded += String(repeating: "=", count: 4 - remainder)
        }
        guard let data = Data(base64Encoded: padded), data.count >= 5, data[0] == 0x23 else {
            return nil
        }
        let epIdData = data[1..<5]
        return epIdData.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private func resolveEndpoint(_ endpoint: NWEndpoint, completion: @escaping (String?, UInt16) -> Void) {
        let params = NWParameters.tcp
        let connection = NWConnection(to: endpoint, using: params)

        var resolved = false
        let timeout = DispatchWorkItem { [weak connection] in
            guard !resolved else { return }
            resolved = true
            NSLog("[MdnsBrowser] Resolve timed out")
            connection?.cancel()
            completion(nil, 0)
        }
        queue.asyncAfter(deadline: .now() + 3, execute: timeout)

        connection.stateUpdateHandler = { state in
            guard !resolved else { return }
            switch state {
            case .ready:
                resolved = true
                timeout.cancel()
                connection.cancel()

                if let path = connection.currentPath {
                    let remoteEndpoint = path.remoteEndpoint
                    switch remoteEndpoint {
                    case .hostPort(let host, let port):
                        let ip: String
                        switch host {
                        case .ipv4(let addr):
                            ip = "\(addr)"
                        case .ipv6(let addr):
                            ip = "\(addr)"
                        case .name(let name, _):
                            ip = name
                        @unknown default:
                            ip = ""
                        }
                        let portNum = port.rawValue
                        NSLog("[MdnsBrowser] Resolved host=\(ip) port=\(portNum)")
                        completion(ip, portNum)
                    default:
                        NSLog("[MdnsBrowser] Unexpected remote endpoint")
                        completion(nil, 0)
                    }
                } else {
                    NSLog("[MdnsBrowser] No network path")
                    completion(nil, 0)
                }
            case .failed(let error):
                resolved = true
                timeout.cancel()
                NSLog("[MdnsBrowser] Connection failed: \(error)")
                completion(nil, 0)
            case .cancelled:
                if !resolved {
                    resolved = true
                    timeout.cancel()
                }
            default:
                break
            }
        }

        connection.start(queue: queue)
    }

    /// Parse base64url-encoded endpoint info to extract device type and name.
    /// Format: [device_type << 1, random[16], name_len, name_bytes...]
    static func parseEndpointInfo(_ encoded: String) -> (String, String)? {
        // Add padding for base64url
        var padded = encoded
        let remainder = padded.count % 4
        if remainder != 0 {
            padded += String(repeating: "=", count: 4 - remainder)
        }

        // Replace URL-safe characters
        let base64 = padded
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")

        guard let data = Data(base64Encoded: base64), data.count >= 19 else {
            return nil
        }

        let deviceTypeValue = (data[0] >> 1) & 0x7
        let nameLength = Int(data[17])

        guard 18 + nameLength <= data.count else {
            return nil
        }

        let nameData = data[18..<(18 + nameLength)]
        guard let name = String(data: nameData, encoding: .utf8) else {
            return nil
        }

        let typeName: String
        switch deviceTypeValue {
        case 1: typeName = "Phone"
        case 2: typeName = "Tablet"
        case 3: typeName = "Laptop"
        default: typeName = "Unknown"
        }

        return (typeName, name)
    }
}
