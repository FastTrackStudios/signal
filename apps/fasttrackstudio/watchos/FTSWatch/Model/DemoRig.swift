// Demo mode: a full in-app mock of the rig so the interaction (scene
// switching, rotation, hold functions) can be showcased with no engine.
// Mirrors the real backend's semantics: press an inactive stack = activate
// its current patch; press the active stack = rotate to the next patch;
// boost cycles +1 → +2 → +3 → −1 dB.

import Foundation

@MainActor
final class DemoRig: RigTransport {
    var onState: ((WatchState) -> Void)?
    var onConnected: ((Bool) -> Void)?

    private var stacks: [(name: String, patches: [String], position: Int)] = [
        ("Clean", ["Sparkle", "Compressed", "Glassy"], 0),
        ("Crunch", ["Edge", "Plexi Lite"], 0),
        ("Drive", ["Morning Glory", "Riff Raff", "Tube Screamer"], 0),
        ("Lead", ["Solo Boost", "Liquid Lead"], 0),
        ("Ambient", ["Shimmer", "Swell Pad"], 0),
    ]
    private var activeStack = 0
    private var fxBypass = false
    private var boostDb: Float = 0
    private var tempoBpm: UInt32 = 120
    private var tunerVisible = false
    private var songIndex = 0
    private let songs = ["Great Are You Lord", "What A Beautiful Name", "Firm Foundation"]
    private var revision: UInt64 = 0
    private var lastTap: Date?

    func start() {
        onConnected?(true)
        publish()
    }

    func stop() {}

    func pressStack(_ index: Int) {
        guard stacks.indices.contains(index) else { return }
        if activeStack == index {
            // Pressing the active stack rotates its patch.
            let count = stacks[index].patches.count
            stacks[index].position = (stacks[index].position + 1) % count
        } else {
            activeStack = index
        }
        publish()
    }

    func perform(_ action: RigAction) {
        switch action {
        case .tapTempo:
            let now = Date()
            if let last = lastTap {
                let interval = now.timeIntervalSince(last)
                if interval > 0.2, interval < 2.0 {
                    tempoBpm = UInt32((60.0 / interval).rounded())
                }
            }
            lastTap = now
        case .toggleFx:
            fxBypass.toggle()
        case .toggleBoost:
            boostDb = boostDb == 0 ? 1 : 0
        case .cycleBoost:
            switch boostDb {
            case 0: boostDb = 1
            case 1: boostDb = 2
            case 2: boostDb = 3
            case 3: boostDb = -1
            default: boostDb = 1
            }
        case .toggleTuner:
            tunerVisible.toggle()
        case .nextSong:
            songIndex = (songIndex + 1) % songs.count
        case .prevSong:
            songIndex = (songIndex + songs.count - 1) % songs.count
        }
        publish()
    }

    private func publish() {
        revision += 1
        let state = WatchState(
            profileName: "Worship (Demo)",
            stacks: stacks.enumerated().map { i, s in
                WatchStack(
                    name: s.name,
                    currentPatch: s.patches[s.position],
                    position: UInt32(s.position),
                    patchCount: UInt32(s.patches.count),
                    available: true,
                    isActive: i == activeStack
                )
            },
            fxBypass: fxBypass,
            boostDb: boostDb,
            tempoBpm: tempoBpm,
            tunerVisible: tunerVisible,
            song: songs[songIndex],
            revision: revision
        )
        onState?(state)
    }
}
