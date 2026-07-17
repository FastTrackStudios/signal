// The six-switch perform grid, floor-pedal ordering (bottom row first):
//
//   5 6      5 = Ambient stack     6 = Tap Tempo (hold: Tuner)
//   3 4      3 = Drive stack       4 = Lead stack
//   1 2      1 = Clean stack       2 = Crunch stack
//
// Taps press the footswitch stack (activate / rotate); holds fire the
// counterpart function ring: 1 prev song, 2 FX toggle, 3 next song,
// 4 boost cycle, 5 tuner, 6 tuner — mirroring the web grid's hold layer.

import SwiftUI

struct PerformGridView: View {
    @Environment(RigStore.self) private var store

    var body: some View {
        VStack(spacing: 3) {
            header
            grid
        }
        // Full-bleed: reclaim the top status-bar band (the clock floats over
        // our header row, which keeps its right side clear for it).
        .ignoresSafeArea()
        .overlay {
            if store.state.tunerVisible { tunerOverlay }
        }
    }

    private var header: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(store.connected ? RigColors.tunerGreen : .red)
                .frame(width: 5, height: 5)
            Text(store.state.song.isEmpty ? store.state.profileName : store.state.song)
                .font(.system(size: 11, weight: .medium))
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 36) // stay clear of the corner clock
        .padding(.top, 2)
    }

    private var grid: some View {
        // Rows top→bottom are 5 6 / 3 4 / 1 2 (floor-pedal order).
        Grid(horizontalSpacing: 3, verticalSpacing: 3) {
            GridRow {
                stackButton(4, hold: .toggleTuner, holdColor: RigColors.tunerGreen)
                tapTempoButton
            }
            GridRow {
                stackButton(2, hold: .nextSong, holdColor: RigColors.song)
                stackButton(3, hold: .cycleBoost, holdColor: RigColors.boost)
            }
            GridRow {
                stackButton(0, hold: .prevSong, holdColor: RigColors.song)
                stackButton(1, hold: .toggleFx, holdColor: RigColors.fxToggle)
            }
        }
    }

    private func stackButton(_ index: Int, hold: RigAction, holdColor: Color) -> some View {
        let stack = store.state.stacks.indices.contains(index) ? store.state.stacks[index] : nil
        let colors = RigColors.stack(stack?.name ?? "")
        return SwitchButton(
            label: stack?.name ?? "—",
            sublabel: stack?.currentPatch,
            background: colors.bg,
            foreground: colors.text,
            isActive: stack?.isActive ?? false,
            holdColor: holdColor,
            rotation: stack.map { (Int($0.position), Int($0.patchCount)) },
            onTap: { store.pressStack(index) },
            onHold: { store.perform(hold) }
        )
    }

    private var tapTempoButton: some View {
        SwitchButton(
            label: "TAP",
            sublabel: store.state.fxBypass
                ? "FX OFF"
                : (boostLabel.isEmpty ? "\(store.state.tempoBpm) bpm" : boostLabel),
            background: RigColors.tapTempo,
            foreground: RigColors.tapTempoText,
            isActive: true,
            holdColor: RigColors.tunerGreen,
            rotation: nil,
            onTap: { store.perform(.tapTempo) },
            onHold: { store.perform(.toggleTuner) }
        )
    }

    private var boostLabel: String {
        let db = store.state.boostDb
        return db == 0 ? "" : String(format: "%+.0f dB", db)
    }

    private var tunerOverlay: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 14)
                .fill(Color(hex: 0x14532D).opacity(0.95))
            VStack(spacing: 6) {
                Image(systemName: "tuningfork")
                    .font(.system(size: 28))
                Text("Tuner")
                    .font(.headline)
                Text("Tap to close")
                    .font(.system(size: 10))
                    .opacity(0.7)
            }
            .foregroundStyle(RigColors.tunerGreen)
        }
        .onTapGesture { store.perform(.toggleTuner) }
    }
}
