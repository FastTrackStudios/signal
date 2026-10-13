// One footswitch tile: tap = primary action, hold (500 ms, matching the web
// perform grid's HOLD_MS) = counterpart function. The secondary ring around
// the tile previews the hold function's color; while holding, the ring
// fills as a progress cue and fires with haptics at the threshold.

import SwiftUI
import WatchKit

struct SwitchButton: View {
    let label: String
    let sublabel: String?
    let background: Color
    let foreground: Color
    let isActive: Bool
    /// Color of the secondary (hold-function) ring.
    let holdColor: Color
    /// Rotation dots: (position, count) — shown for stacks with >1 patch.
    let rotation: (Int, Int)?
    let onTap: () -> Void
    let onHold: () -> Void

    @State private var holdProgress: CGFloat = 0
    @State private var holdFired = false
    @GestureState private var pressing = false

    private static let holdSeconds = 0.5

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12)
                .fill(background)
                .opacity(isActive ? 1.0 : 0.35)
                .saturation(isActive ? 1.0 : 0.6)

            // Secondary ring: the hold counterpart. Faint at rest, fills
            // while holding.
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(holdColor.opacity(0.35), lineWidth: 2)
            RoundedRectangle(cornerRadius: 12)
                .trim(from: 0, to: holdProgress)
                .stroke(holdColor, style: StrokeStyle(lineWidth: 3, lineCap: .round))

            if isActive {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(.white.opacity(0.9), lineWidth: 2)
                    .padding(2)
            }

            VStack(spacing: 1) {
                Text(label)
                    .font(.system(size: 13, weight: .bold))
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                if let sublabel, !sublabel.isEmpty {
                    Text(sublabel)
                        .font(.system(size: 9))
                        .minimumScaleFactor(0.6)
                        .lineLimit(1)
                        .opacity(0.8)
                }
                if let (position, count) = rotation, count > 1 {
                    HStack(spacing: 2) {
                        ForEach(0..<count, id: \.self) { i in
                            Circle()
                                .fill(i == position ? foreground : foreground.opacity(0.3))
                                .frame(width: 3, height: 3)
                        }
                    }
                }
            }
            .foregroundStyle(foreground)
            .padding(4)
        }
        .contentShape(RoundedRectangle(cornerRadius: 12))
        .gesture(
            LongPressGesture(minimumDuration: Self.holdSeconds)
                .updating($pressing) { value, state, _ in state = value }
                .onEnded { _ in
                    holdFired = true
                    WKInterfaceDevice.current().play(.directionUp)
                    onHold()
                }
                .simultaneously(
                    with: TapGesture().onEnded {
                        if !holdFired { onTap() }
                        holdFired = false
                    })
        )
        .onChange(of: pressing) { _, isPressing in
            if isPressing {
                holdFired = false
                withAnimation(.linear(duration: Self.holdSeconds)) { holdProgress = 1 }
            } else {
                withAnimation(.easeOut(duration: 0.15)) { holdProgress = 0 }
            }
        }
    }
}
