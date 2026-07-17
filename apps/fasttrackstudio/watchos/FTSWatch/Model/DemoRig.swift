// Demo mode: a full in-app mock of the rig so the interaction (scene
// switching, rotation, hold functions) can be showcased with no engine.
// Mirrors the real backend's semantics: press an inactive stack = activate
// its current patch; press the active stack = rotate to the next patch;
// boost cycles +1 → +2 → +3 → −1 dB.

import Foundation

@MainActor
final class DemoRig: RigTransport {
    var onState: ((WatchState) -> Void)?
    var onSession: ((WatchSessionState) -> Void)?
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

    // ── Session mock state ──
    private var isPlaying = false
    private var songProgress: Double = 0.12
    private var sessionTimer: Timer?
    private let sectionNames = ["Intro", "Verse 1", "Chorus 1", "Verse 2", "Chorus 2", "Bridge"]
    /// One looping progression per song (chords land one per measure).
    private let progressions: [[String]] = [
        ["G", "D", "Em7", "C", "G/B", "D/F#", "Em7", "Csus2"],
        ["D", "A/C#", "Bm7", "G", "D/F#", "A", "Gmaj7", "Asus4"],
        ["C", "G/B", "Am7", "F", "C/E", "G", "F2", "Gsus4"],
    ]
    private var mockTracks: [WatchTrack] = [
        ("Click", 0x808080), ("Guide", 0xA78BFA), ("Drums", 0xEF4444),
        ("Bass", 0x2563EB), ("Keys", 0x06B6D4), ("EG 1", 0xF97316),
        ("EG 2", 0xFB923C), ("Vox", 0xEC4899),
    ].enumerated().map { i, t in
        WatchTrack(
            guid: "demo-\(i)", name: t.0, index: UInt32(i), muted: false,
            soloed: false, volume: 0.8, pan: 0, isFolder: false, color: UInt32(t.1))
    }

    func start() {
        onConnected?(true)
        publish()
        publishSession()
    }

    func stop() {
        sessionTimer?.invalidate()
        sessionTimer = nil
    }

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

    // ── Session mock behavior ──

    func sessionTransport(_ cmd: SessionTransportCommand) {
        switch cmd {
        case .play: setPlaying(true)
        case .pause: setPlaying(false)
        case .stop:
            setPlaying(false)
            songProgress = 0
        case .toggle: setPlaying(!isPlaying)
        case .nextSong:
            songIndex = (songIndex + 1) % songs.count
            songProgress = 0
        case .prevSong:
            songIndex = (songIndex + songs.count - 1) % songs.count
            songProgress = 0
        case .nextSection:
            songProgress = min(1.0, (sectionFraction(after: songProgress)))
        case .prevSection:
            songProgress = max(0.0, (sectionFraction(before: songProgress)))
        }
        publishSession()
    }

    func seekSection(song: Int, section: Int) {
        songIndex = song % songs.count
        songProgress = Double(section) / Double(sectionNames.count)
        publishSession()
    }

    func toggleTrackMute(_ guid: String) {
        if let i = mockTracks.firstIndex(where: { $0.guid == guid }) {
            mockTracks[i].muted.toggle()
            publishSession()
        }
    }

    func toggleTrackSolo(_ guid: String) {
        if let i = mockTracks.firstIndex(where: { $0.guid == guid }) {
            mockTracks[i].soloed.toggle()
            publishSession()
        }
    }

    func setTrackVolume(_ guid: String, _ volume: Double) {
        if let i = mockTracks.firstIndex(where: { $0.guid == guid }) {
            mockTracks[i].volume = Float(min(max(volume, 0), 1))
            publishSession()
        }
    }

    private func setPlaying(_ playing: Bool) {
        isPlaying = playing
        sessionTimer?.invalidate()
        sessionTimer = nil
        if playing {
            // A ~4 minute mock song: creep progress and re-publish.
            sessionTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) {
                [weak self] _ in
                Task { @MainActor in
                    guard let self, self.isPlaying else { return }
                    self.songProgress += 0.5 / 240.0
                    if self.songProgress >= 1.0 {
                        self.songProgress = 0
                        self.songIndex = (self.songIndex + 1) % self.songs.count
                    }
                    self.publishSession()
                }
            }
        }
    }

    private func sectionFraction(after p: Double) -> Double {
        let step = 1.0 / Double(sectionNames.count)
        return (Double(Int(p / step)) + 1) * step
    }

    private func sectionFraction(before p: Double) -> Double {
        let step = 1.0 / Double(sectionNames.count)
        return (Double(Int(p / step)) - 1) * step
    }

    private func publishSession() {
        let sectionCount = sectionNames.count
        let sectionIdx = min(Int(songProgress * Double(sectionCount)), sectionCount - 1)
        let sectionProgress = (songProgress * Double(sectionCount)).truncatingRemainder(
            dividingBy: 1.0)

        // 8-measure loop, ~2s per measure at the mock pace.
        let progression = progressions[songIndex % progressions.count]
        let measure = Int(songProgress * 120)  // ~120 measures per mock song
        let window = (0..<4).map { k -> WatchChord in
            let idx = (measure + k) % progression.count
            return WatchChord(
                symbol: progression[idx], measure: Int32(measure + k), beat: 0,
                isCurrent: k == 0)
        }

        revision += 1
        onSession?(
            WatchSessionState(
                songs: songs,
                songIndex: Int32(songIndex),
                sections: sectionNames,
                sectionIndex: Int32(sectionIdx),
                isPlaying: isPlaying,
                songProgress: Float(songProgress),
                sectionProgress: Float(sectionProgress),
                chords: window,
                lyricLine: "You give life, You are love, You bring light to the darkness",
                tracks: mockTracks,
                revision: revision
            ))
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
