// The app's single observable state: current WatchState + transport choice.

import Foundation
import SwiftUI
import WatchKit

enum TransportKind: String, CaseIterable, Identifiable {
    case demo = "Demo"
    case engine = "Engine"
    case bleMidi = "BLE MIDI"
    var id: String { rawValue }
}

@MainActor
@Observable
final class RigStore {
    var state: WatchState = .empty
    var session: WatchSessionState = .empty
    var connected = false

    var transportKind: TransportKind {
        didSet {
            UserDefaults.standard.set(transportKind.rawValue, forKey: "transport")
            reconnect()
        }
    }
    var engineHost: String {
        didSet { UserDefaults.standard.set(engineHost, forKey: "engineHost") }
    }

    private var transport: RigTransport?

    init() {
        let defaults = UserDefaults.standard
        transportKind =
            TransportKind(rawValue: defaults.string(forKey: "transport") ?? "") ?? .demo
        engineHost = defaults.string(forKey: "engineHost") ?? "signal.local:4040"
        reconnect()
    }

    func reconnect() {
        transport?.stop()
        let t: RigTransport? =
            switch transportKind {
            case .demo: DemoRig()
            case .engine: HttpRig(host: engineHost)
            case .bleMidi: BleMidiRig()
            }
        connected = false
        transport = t
        t?.onState = { [weak self] state in self?.state = state }
        t?.onSession = { [weak self] state in self?.session = state }
        t?.onConnected = { [weak self] up in self?.connected = up }
        t?.start()
    }

    func pressStack(_ index: Int) {
        transport?.pressStack(index)
        WKInterfaceDevice.current().play(.click)
    }

    func perform(_ action: RigAction) {
        transport?.perform(action)
        WKInterfaceDevice.current().play(.success)
    }

    // ── Session ──

    func sessionTransport(_ cmd: SessionTransportCommand) {
        transport?.sessionTransport(cmd)
        WKInterfaceDevice.current().play(.click)
    }

    func seekSection(song: Int, section: Int) {
        transport?.seekSection(song: song, section: section)
        WKInterfaceDevice.current().play(.click)
    }

    func toggleTrackMute(_ guid: String) {
        transport?.toggleTrackMute(guid)
        WKInterfaceDevice.current().play(.click)
    }

    func toggleTrackSolo(_ guid: String) {
        transport?.toggleTrackSolo(guid)
        WKInterfaceDevice.current().play(.click)
    }

    func setTrackVolume(_ guid: String, _ volume: Double) {
        transport?.setTrackVolume(guid, volume)
    }
}

extension WatchState {
    static let empty = WatchState(
        profileName: "", stacks: [], fxBypass: false, boostDb: 0,
        tempoBpm: 120, tunerVisible: false, song: "", revision: 0)
}

extension WatchSessionState {
    static let empty = WatchSessionState(
        songs: [], songIndex: -1, sections: [], sectionIndex: -1,
        isPlaying: false, songProgress: 0, sectionProgress: 0,
        chords: [], lyricLine: "", tracks: [], revision: 0)
}
