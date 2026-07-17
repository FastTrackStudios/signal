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

/// A rig the watch can drive. Implementations push `WatchState` snapshots
/// through `onState`; commands are fire-and-forget (state comes back via
/// the stream, never via the command result — same shape as the vox
/// `#[subscribe]` events stream). Everything runs on the main actor; the
/// transports hop their IO callbacks over themselves.
@MainActor
protocol RigTransport: AnyObject {
    /// Snapshot sink.
    var onState: ((WatchState) -> Void)? { get set }
    /// Connection health sink.
    var onConnected: ((Bool) -> Void)? { get set }

    func start()
    func stop()

    /// Press footswitch stack `index` (activate current patch / rotate).
    func pressStack(_ index: Int)
    /// Fire a hold-layer / function action.
    func perform(_ action: RigAction)
}
