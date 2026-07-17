// The keyflow page: the current chord + the next three in a line, the
// current section, and the section's lyric line. Swipe-friendly section
// nav via the bottom chevrons.

import SwiftUI

struct ChordsView: View {
    @Environment(RigStore.self) private var store

    var body: some View {
        let s = store.session
        VStack(spacing: 6) {
            // Song + section header with section progress underline.
            VStack(spacing: 2) {
                Text(currentSong)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .opacity(0.7)
                Text(currentSection)
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(RigColors.song)
                ProgressView(value: Double(min(max(s.sectionProgress, 0), 1)))
                    .tint(RigColors.song)
                    .scaleEffect(y: 0.6)
            }

            // The chord window: current + next 3.
            HStack(spacing: 4) {
                if s.chords.isEmpty {
                    Text("—")
                        .font(.system(size: 26, weight: .bold))
                        .opacity(0.3)
                } else {
                    ForEach(Array(s.chords.prefix(4).enumerated()), id: \.offset) { _, chord in
                        Text(chord.symbol)
                            .font(.system(
                                size: chord.isCurrent ? 26 : 17,
                                weight: chord.isCurrent ? .heavy : .semibold,
                                design: .rounded))
                            .foregroundStyle(chord.isCurrent ? .white : .white.opacity(0.45))
                            .lineLimit(1)
                            .minimumScaleFactor(0.5)
                    }
                }
            }
            .frame(maxWidth: .infinity)

            if !s.lyricLine.isEmpty {
                Text(s.lyricLine)
                    .font(.system(size: 11))
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                    .minimumScaleFactor(0.8)
                    .opacity(0.8)
            }

            Spacer(minLength: 0)

            // Section navigation + play/pause.
            HStack {
                Button { store.sessionTransport(.prevSection) } label: {
                    Image(systemName: "chevron.backward.2")
                }
                Button { store.sessionTransport(.toggle) } label: {
                    Image(systemName: s.isPlaying ? "pause.fill" : "play.fill")
                }
                .tint(s.isPlaying ? .orange : .green)
                Button { store.sessionTransport(.nextSection) } label: {
                    Image(systemName: "chevron.forward.2")
                }
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
        }
        .padding(.horizontal, 2)
    }

    private var currentSong: String {
        let s = store.session
        guard s.songIndex >= 0, Int(s.songIndex) < s.songs.count else { return "No song" }
        return s.songs[Int(s.songIndex)]
    }

    private var currentSection: String {
        let s = store.session
        guard s.sectionIndex >= 0, Int(s.sectionIndex) < s.sections.count else { return "—" }
        return s.sections[Int(s.sectionIndex)]
    }
}
