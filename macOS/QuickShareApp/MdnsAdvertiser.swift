import Foundation

class MdnsAdvertiser: NSObject, NetServiceDelegate {
    private var service: NetService?

    func start(endpointId: [UInt8], deviceName: String, deviceType: Int32 = 3) {
        stop()

        let mdnsName = genMdnsName(endpointId)
        let endpointInfo = genEndpointInfo(deviceType: deviceType, deviceName: deviceName)

        let txtDict = ["n": endpointInfo.data(using: .utf8)!]
        let txtData = NetService.data(fromTXTRecord: txtDict)

        let svc = NetService(domain: "local.", type: "_FC9F5ED42C8A._tcp", name: mdnsName, port: 5721)
        svc.delegate = self
        svc.includesPeerToPeer = true
        svc.setTXTRecord(txtData)
        svc.publish()
        self.service = svc

        NSLog("[MdnsAdvertiser] Started mDNS advertising: name='\(mdnsName)' type='_FC9F5ED42C8A._tcp' port=5721")
    }

    func stop() {
        service?.stop()
        service?.delegate = nil
        service = nil
        NSLog("[MdnsAdvertiser] Stopped mDNS advertising")
    }

    var isAdvertising: Bool {
        service != nil
    }

    // MARK: - NetServiceDelegate

    func netServiceDidPublish(_ sender: NetService) {
        NSLog("[MdnsAdvertiser] mDNS advertising ready: name='\(sender.name)' type='\(sender.type)' port=\(sender.port)")
    }

    func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
        NSLog("[MdnsAdvertiser] mDNS publish failed: \(errorDict)")
        service = nil
    }
}

private func genMdnsName(_ endpointId: [UInt8]) -> String {
    var data = Data(capacity: 10)
    data.append(0x23)
    data.append(contentsOf: endpointId)
    data.append(contentsOf: [0xFC, 0x9F, 0x5E, 0x00, 0x00])
    return base64urlEncode(data)
}

private func genEndpointInfo(deviceType: Int32, deviceName: String) -> String {
    var data = Data()
    data.append(UInt8(deviceType << 1))

    var random = [UInt8](repeating: 0, count: 16)
    let result = random.withUnsafeMutableBytes { ptr in
        SecRandomCopyBytes(kSecRandomDefault, 16, ptr.baseAddress!)
    }
    if result != errSecSuccess {
        NSLog("[MdnsAdvertiser] Failed to generate random bytes for endpoint info")
    }
    data.append(contentsOf: random)

    let nameBytes = [UInt8](deviceName.utf8)
    data.append(UInt8(nameBytes.count))
    data.append(contentsOf: nameBytes)

    return base64urlEncode(data)
}

private func base64urlEncode(_ data: Data) -> String {
    let b64 = data.base64EncodedString()
    return b64
        .replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_")
        .replacingOccurrences(of: "=", with: "")
}
