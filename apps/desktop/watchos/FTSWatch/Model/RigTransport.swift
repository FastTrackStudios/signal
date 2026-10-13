// The transport abstraction: everything the perform grid can do to a rig.
// Three implementations — DemoRig (offline mock), HttpRig (the engine's
// /watch/v1 HTTP+SSE bridge), BleMidiRig (BLE MIDI GATT, MidiWrist-style).

import Foundation

/// The hold-layer / function actions (everything that isn't a stack press).
enum RigAction: String, CaseIterable, Sendable {
    case tapTempo = "tap-tempo"
    case toggleFx = "toggle-fx"
    case toggleBoost = "toggle-boost"
    case cycleBoost = "cycle-boost"
    case toggleTuner = "toggle-tuner"
    case nextSong = "next-song"
    case prevSong = "prev-song"
}

/// Session transport commands (`/watch/v1/session/transport/{cmd}`).
enum SessionTransportCommand: String, CaseIterable, Sendable {
    case play, pause, stop, toggle
    case nextSong = "next-song"
    case prevSong = "prev-song"
    case nextSection = "next-section"
    case prevSection = "prev-section"
}

/// A rig the watch can drive. Implementations push `WatchState` snapshots
/// through `onState`; commands are fire-and-forget (state comes back via
/// the stream, never via the command result — same shape as the vox
/// `#[subscribe]` events stream). Everything runs on the main actor; the
/// transports hop their IO callbacks over themselves.
@MainActor
protocol RigTransport: AnyObject {
    /// Snapshot sink.
    var onState: ((WatchState) -> Void)? { get set }
    /// Session (setlist / transport / mixer / chords) snapshot sink.
    var onSession: ((WatchSessionState) -> Void)? { get set }
    /// Connection health sink.
    var onConnected: ((Bool) -> Void)? { get set }

    func start()
    func stop()

    /// Press footswitch stack `index` (activate current patch / rotate).
    func pressStack(_ index: Int)
    /// Fire a hold-layer / function action.
    func perform(_ action: RigAction)

    // ── Session ──
    func sessionTransport(_ cmd: SessionTransportCommand)
    func seekSection(song: Int, section: Int)
    func toggleTrackMute(_ guid: String)
    func toggleTrackSolo(_ guid: String)
    func setTrackVolume(_ guid: String, _ volume: Double)
}
