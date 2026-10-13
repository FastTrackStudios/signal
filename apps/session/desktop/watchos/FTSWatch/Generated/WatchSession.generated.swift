// GENERATED — do not edit. Mirrors the facet shapes in
// crates/session/proto/src/watch.rs (the `/watch/v1` wire DTOs).
// Regenerate: cargo run -p session-proto --example gen_watch_swift
//   > apps/fasttrackstudio/watchos/FTSWatch/Generated/WatchSession.generated.swift

import Foundation

public struct WatchChord: Codable, Equatable, Sendable {
    public var symbol: String
    public var measure: Int32
    public var beat: Int32
    public var isCurrent: Bool

    public init(
        symbol: String,
        measure: Int32,
        beat: Int32,
        isCurrent: Bool
    ) {
        self.symbol = symbol
        self.measure = measure
        self.beat = beat
        self.isCurrent = isCurrent
    }

    enum CodingKeys: String, CodingKey {
        case symbol = "symbol"
        case measure = "measure"
        case beat = "beat"
        case isCurrent = "is_current"
    }
}

public struct WatchSessionState: Codable, Equatable, Sendable {
    public var songs: [String]
    public var songIndex: Int32
    public var sections: [String]
    public var sectionIndex: Int32
    public var isPlaying: Bool
    public var songProgress: Float
    public var sectionProgress: Float
    public var chords: [WatchChord]
    public var lyricLine: String
    public var tracks: [WatchTrack]
    public var revision: UInt64

    public init(
        songs: [String],
        songIndex: Int32,
        sections: [String],
        sectionIndex: Int32,
        isPlaying: Bool,
        songProgress: Float,
        sectionProgress: Float,
        chords: [WatchChord],
        lyricLine: String,
        tracks: [WatchTrack],
        revision: UInt64
    ) {
        self.songs = songs
        self.songIndex = songIndex
        self.sections = sections
        self.sectionIndex = sectionIndex
        self.isPlaying = isPlaying
        self.songProgress = songProgress
        self.sectionProgress = sectionProgress
        self.chords = chords
        self.lyricLine = lyricLine
        self.tracks = tracks
        self.revision = revision
    }

    enum CodingKeys: String, CodingKey {
        case songs = "songs"
        case songIndex = "song_index"
        case sections = "sections"
        case sectionIndex = "section_index"
        case isPlaying = "is_playing"
        case songProgress = "song_progress"
        case sectionProgress = "section_progress"
        case chords = "chords"
        case lyricLine = "lyric_line"
        case tracks = "tracks"
        case revision = "revision"
    }
}

public struct WatchTrack: Codable, Equatable, Sendable {
    public var guid: String
    public var name: String
    public var index: UInt32
    public var muted: Bool
    public var soloed: Bool
    public var volume: Float
    public var pan: Float
    public var isFolder: Bool
    public var color: UInt32

    public init(
        guid: String,
        name: String,
        index: UInt32,
        muted: Bool,
        soloed: Bool,
        volume: Float,
        pan: Float,
        isFolder: Bool,
        color: UInt32
    ) {
        self.guid = guid
        self.name = name
        self.index = index
        self.muted = muted
        self.soloed = soloed
        self.volume = volume
        self.pan = pan
        self.isFolder = isFolder
        self.color = color
    }

    enum CodingKeys: String, CodingKey {
        case guid = "guid"
        case name = "name"
        case index = "index"
        case muted = "muted"
        case soloed = "soloed"
        case volume = "volume"
        case pan = "pan"
        case isFolder = "is_folder"
        case color = "color"
    }
}

