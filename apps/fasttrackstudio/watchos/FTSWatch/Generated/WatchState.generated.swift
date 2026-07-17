// GENERATED — do not edit. Mirrors the facet shapes in
// features/rigs/guitar/proto/src/watch.rs (the `/watch/v1` wire DTOs).
// Regenerate: cargo run -p signal-guitar-proto --example gen_watch_swift
//   > apps/fasttrackstudio/watchos/FTSWatch/Generated/WatchState.generated.swift

import Foundation

public struct WatchStack: Codable, Equatable, Sendable {
    public var name: String
    public var currentPatch: String
    public var position: UInt32
    public var patchCount: UInt32
    public var available: Bool
    public var isActive: Bool

    public init(
        name: String,
        currentPatch: String,
        position: UInt32,
        patchCount: UInt32,
        available: Bool,
        isActive: Bool
    ) {
        self.name = name
        self.currentPatch = currentPatch
        self.position = position
        self.patchCount = patchCount
        self.available = available
        self.isActive = isActive
    }

    enum CodingKeys: String, CodingKey {
        case name = "name"
        case currentPatch = "current_patch"
        case position = "position"
        case patchCount = "patch_count"
        case available = "available"
        case isActive = "is_active"
    }
}

public struct WatchState: Codable, Equatable, Sendable {
    public var profileName: String
    public var stacks: [WatchStack]
    public var fxBypass: Bool
    public var boostDb: Float
    public var tempoBpm: UInt32
    public var tunerVisible: Bool
    public var song: String
    public var revision: UInt64

    public init(
        profileName: String,
        stacks: [WatchStack],
        fxBypass: Bool,
        boostDb: Float,
        tempoBpm: UInt32,
        tunerVisible: Bool,
        song: String,
        revision: UInt64
    ) {
        self.profileName = profileName
        self.stacks = stacks
        self.fxBypass = fxBypass
        self.boostDb = boostDb
        self.tempoBpm = tempoBpm
        self.tunerVisible = tunerVisible
        self.song = song
        self.revision = revision
    }

    enum CodingKeys: String, CodingKey {
        case profileName = "profile_name"
        case stacks = "stacks"
        case fxBypass = "fx_bypass"
        case boostDb = "boost_db"
        case tempoBpm = "tempo_bpm"
        case tunerVisible = "tuner_visible"
        case song = "song"
        case revision = "revision"
    }
}

