// The session page: transport + the mixer. Tap a track to mute it; open
// its detail (chevron) for solo + crown-driven volume.

import SwiftUI

struct SessionView: View {
    @Environment(RigStore.self) private var store

    var body: some View {
        let s = store.session
        NavigationStack {
            List {
                Section {
                    // Transport row.
                    HStack {
                        Button { store.sessionTransport(.prevSong) } label: {
                            Image(systemName: "backward.end.fill")
                        }
                        Button { store.sessionTransport(.toggle) } label: {
                            Image(systemName: s.isPlaying ? "pause.fill" : "play.fill")
                        }
                        .tint(s.isPlaying ? .orange : .green)
                        Button { store.sessionTransport(.stop) } label: {
                            Image(systemName: "stop.fill")
                        }
                        Button { store.sessionTransport(.nextSong) } label: {
                            Image(systemName: "forward.end.fill")
                        }
                    }
                    .buttonStyle(.borderless)
                    .listRowBackground(Color.clear)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(currentSong)
                            .font(.system(size: 12, weight: .bold))
                            .lineLimit(1)
                        ProgressView(value: Double(min(max(s.songProgress, 0), 1)))
                            .scaleEffect(y: 0.6)
                            .animation(.linear(duration: 0.5), value: s.songProgress)
                    }
                    .listRowBackground(Color.clear)
                }

                Section("Tracks") {
                    ForEach(s.tracks, id: \.guid) { track in
                        NavigationLink(value: track.guid) {
                            TrackRow(track: track)
                        }
                    }
                }
            }
            .navigationDestination(for: String.self) { guid in
                if let track = s.tracks.first(where: { $0.guid == guid }) {
                    TrackDetailView(guid: guid, initial: track)
                }
            }
        }
    }

    private var currentSong: String {
        let s = store.session
        guard s.songIndex >= 0, Int(s.songIndex) < s.songs.count else { return "No song" }
        return s.songs[Int(s.songIndex)]
    }
}

private struct TrackRow: View {
    @Environment(RigStore.self) private var store
    let track: WatchTrack

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(trackColor)
                .frame(width: 6, height: 6)
            Text(track.name)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .strikethrough(track.muted, color: .red)
                .opacity(track.muted ? 0.5 : 1)
            Spacer()
            if track.soloed {
                Text("S")
                    .font(.system(size: 10, weight: .black))
                    .foregroundStyle(.yellow)
            }
            Button {
                store.toggleTrackMute(track.guid)
            } label: {
                Image(systemName: track.muted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(track.muted ? .red : .secondary)
            }
            .buttonStyle(.plain)
        }
    }

    private var trackColor: Color {
        track.color == 0 ? Color(hex: 0x3F3F46) : Color(hex: track.color & 0xFF_FFFF)
    }
}

/// Solo + mute + crown-driven volume for one track.
private struct TrackDetailView: View {
    @Environment(RigStore.self) private var store
    let guid: String
    let initial: WatchTrack

    @State private var volume: Double = 0
    @State private var crownSetup = false

    private var track: WatchTrack {
        store.session.tracks.first(where: { $0.guid == guid }) ?? initial
    }

    var body: some View {
        VStack(spacing: 10) {
            Text(track.name)
                .font(.headline)

            Gauge(value: volume, in: 0...1) {
                Text("Vol")
            } currentValueLabel: {
                Text("\(Int(volume * 100))")
            }
            .gaugeStyle(.accessoryCircular)
            .tint(.green)

            HStack {
                Button {
                    store.toggleTrackMute(guid)
                } label: {
                    Text("M")
                        .font(.system(size: 14, weight: .black))
                }
                .tint(track.muted ? .red : .gray)
                Button {
                    store.toggleTrackSolo(guid)
                } label: {
                    Text("S")
                        .font(.system(size: 14, weight: .black))
                }
                .tint(track.soloed ? .yellow : .gray)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
        }
        .focusable(true)
        .digitalCrownRotation(
            $volume, from: 0, through: 1, by: 0.02,
            sensitivity: .medium, isContinuous: false, isHapticFeedbackEnabled: true
        )
        .onAppear {
            volume = Double(track.volume)
            crownSetup = true
        }
        .onChange(of: volume) { _, v in
            guard crownSetup else { return }
            store.setTrackVolume(guid, v)
        }
    }
}
