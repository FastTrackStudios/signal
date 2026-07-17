// Color mapping — a 1:1 mirror of the Dioxus web perform grid
// (`features/rigs/guitar/ui/src/perform.rs`): `folder_color()` for stack
// tiles, the hardcoded function-tile colors for the hold layer.

import SwiftUI

extension Color {
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255
        )
    }
}

enum RigColors {
    /// `folder_color()` — (background, text) by stack name.
    static func stack(_ name: String) -> (bg: Color, text: Color) {
        switch name.lowercased() {
        case "clean": (Color(hex: 0x38BDF8), Color(hex: 0x082F49))
        case "crunch": (Color(hex: 0x2563EB), .white)
        case "drive": (Color(hex: 0xF97316), .white)
        case "lead": (Color(hex: 0xEF4444), .white)
        case "ambient": (Color(hex: 0x06B6D4), Color(hex: 0x04222A))
        default: (Color(hex: 0x3F3F46), Color(hex: 0xE4E4E7))
        }
    }

    /// Function-tile colors (web perform grid hold layer).
    static let fxToggle = Color(hex: 0xEC4899) // pink
    static let song = Color(hex: 0xA78BFA) // violet
    static let boost = Color(hex: 0xFAFAFA) // near-white
    static let tapTempo = Color(hex: 0x27272A) // zinc
    static let tapTempoText = Color(hex: 0xD4D4D8)
    static let tunerGreen = Color(hex: 0x22C55E)
    static let loadingDot = Color(hex: 0xFDE047) // amber
}
