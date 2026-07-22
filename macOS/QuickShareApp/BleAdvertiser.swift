import Foundation
import CoreBluetooth

class BleAdvertiser: NSObject, CBPeripheralManagerDelegate {
    private var peripheralManager: CBPeripheralManager!
    private let serviceUuid = CBUUID(string: "FE2C")
    private var pendingEndpointId: [UInt8]?
    private var isAdvertisingActive = false

    override init() {
        super.init()
        peripheralManager = CBPeripheralManager(delegate: self, queue: nil)
    }

    func startAdvertising(endpointId: [UInt8]) {
        guard endpointId.count == 4 else {
            NSLog("[BleAdvertiser] Invalid endpoint ID length: \(endpointId.count)")
            return
        }

        guard peripheralManager.state == .poweredOn else {
            pendingEndpointId = endpointId
            return
        }

        publishAndAdvertise(endpointId: endpointId)
    }

    func stopAdvertising() {
        pendingEndpointId = nil
        if isAdvertisingActive && peripheralManager.state == .poweredOn {
            peripheralManager.stopAdvertising()
        }
        isAdvertisingActive = false
    }

    private func publishAndAdvertise(endpointId: [UInt8]) {
        guard !isAdvertisingActive else { return }
        guard peripheralManager.state == .poweredOn else {
            pendingEndpointId = endpointId
            return
        }

        // On macOS, CBAdvertisementDataServiceDataKey is not supported.
        // Advertise only the service UUID (0xFE2C) — Android detects us
        // via the UUID and resolves details via mDNS (NWListener).
        let advertisementData: [String: Any] = [
            CBAdvertisementDataServiceUUIDsKey: [serviceUuid],
        ]

        isAdvertisingActive = true
        peripheralManager.startAdvertising(advertisementData)
        NSLog("[BleAdvertiser] Started BLE advertising with endpoint_id: \(endpointId.map { String(format: "%02x", $0) }.joined()) (service data omitted — macOS limitation)")
    }

    // MARK: - CBPeripheralManagerDelegate

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        switch peripheral.state {
        case .poweredOn:
            guard let ep = pendingEndpointId else { return }
            pendingEndpointId = nil
            // Small delay to ensure CoreBluetooth is fully ready after state change
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                self?.publishAndAdvertise(endpointId: ep)
            }
        case .poweredOff:
            isAdvertisingActive = false
            NSLog("[BleAdvertiser] Bluetooth is powered off — BLE advertising unavailable")
        case .unauthorized:
            NSLog("[BleAdvertiser] Bluetooth not authorized — check System Settings > Privacy > Bluetooth")
        case .unsupported:
            NSLog("[BleAdvertiser] Bluetooth LE not supported on this device")
        case .unknown:
            break
        @unknown default:
            break
        }
    }
}
