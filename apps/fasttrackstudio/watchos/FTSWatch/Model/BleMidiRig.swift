// BLE MIDI transport: CoreBluetooth speaking the Bluetooth-LE MIDI GATT
// profile straight to any advertised BLE MIDI device — no iPhone proxy
// (the MidiWrist model; CoreBluetooth is the one radio watchOS doesn't
// restrict). One-way controller: presses go out as MIDI, state stays local
// (optimistically mirrored via a DemoRig so the grid still animates).
//
// MIDI mapping (channel 1):
//   stack press i   → Program Change i (0-based)
//   toggle FX       → CC 80, value 127
//   toggle boost    → CC 81, value 127
//   cycle boost     → CC 82, value 127
//   toggle tuner    → CC 83, value 127
//   prev / next song→ CC 84 / 85, value 127
//   tap tempo       → CC 86, value 127

import CoreBluetooth
import Foundation

@MainActor
final class BleMidiRig: NSObject, RigTransport {
    // CBUUID is immutable in practice but not marked Sendable; these are
    // constants, so the unsafe opt-out is sound.
    nonisolated(unsafe) static let midiService = CBUUID(
        string: "03B80E5A-EDE8-4B33-A751-6CE34EC4C700")
    nonisolated(unsafe) static let midiCharacteristic = CBUUID(
        string: "7772E5DB-3868-4112-A1A9-F2669D106BF9")

    var onState: ((WatchState) -> Void)?
    var onConnected: ((Bool) -> Void)?

    /// Local optimistic state so the grid animates while MIDI is one-way.
    private let mirror = DemoRig()

    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var characteristic: CBCharacteristic?

    func start() {
        mirror.onState = onState
        mirror.start()
        // nil queue = callbacks on the main queue, matching our actor.
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func stop() {
        if let peripheral {
            central?.cancelPeripheralConnection(peripheral)
        }
        central?.stopScan()
        central = nil
        peripheral = nil
        characteristic = nil
    }

    func pressStack(_ index: Int) {
        sendMidi([0xC0, UInt8(clamping: index)]) // Program Change, ch 1
        mirror.pressStack(index)
    }

    func perform(_ action: RigAction) {
        let cc: UInt8 =
            switch action {
            case .toggleFx: 80
            case .toggleBoost: 81
            case .cycleBoost: 82
            case .toggleTuner: 83
            case .prevSong: 84
            case .nextSong: 85
            case .tapTempo: 86
            }
        sendMidi([0xB0, cc, 127]) // Control Change, ch 1
        mirror.perform(action)
    }

    /// Wrap a MIDI message in the BLE-MIDI packet frame (header + timestamp
    /// bytes carrying the low 13 bits of a millisecond clock).
    private func sendMidi(_ message: [UInt8]) {
        guard let peripheral, let characteristic else { return }
        let millis = UInt16(UInt64(Date().timeIntervalSince1970 * 1000) & 0x1FFF)
        let header = UInt8(0x80 | ((millis >> 7) & 0x3F))
        let timestamp = UInt8(0x80 | (millis & 0x7F))
        let packet = Data([header, timestamp] + message)
        let type: CBCharacteristicWriteType =
            characteristic.properties.contains(.writeWithoutResponse)
            ? .withoutResponse : .withResponse
        peripheral.writeValue(packet, for: characteristic, type: type)
    }
}

// CoreBluetooth delegate callbacks arrive on the main queue (central was
// created with queue: nil), so the conformance is declared @preconcurrency:
// the methods stay MainActor-isolated and the runtime asserts the hop.
extension BleMidiRig: @preconcurrency CBCentralManagerDelegate,
    @preconcurrency CBPeripheralDelegate
{
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state == .poweredOn {
            central.scanForPeripherals(withServices: [Self.midiService])
        } else {
            onConnected?(false)
        }
    }

    func centralManager(
        _ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any], rssi RSSI: NSNumber
    ) {
        // First advertised BLE MIDI device wins — the rig floor setup has
        // exactly one. A picker can come later.
        self.peripheral = peripheral
        central.stopScan()
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.delegate = self
        peripheral.discoverServices([Self.midiService])
    }

    func centralManager(
        _ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        onConnected?(false)
        characteristic = nil
        central.scanForPeripherals(withServices: [Self.midiService])
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        for service in peripheral.services ?? [] where service.uuid == Self.midiService {
            peripheral.discoverCharacteristics([Self.midiCharacteristic], for: service)
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        for c in service.characteristics ?? [] where c.uuid == Self.midiCharacteristic {
            characteristic = c
            onConnected?(true)
        }
    }
}
