//! Tempo Map Override Layer
//!
//! Keybindings active during tempo mapping workflow.
//! Enable with `FTS_KEYBIND_TOGGLE_TEMPO_MAP_OVERLAY` action.

use crate::input::keybinds::{Keybind, KeybindOverride, WheelBind};

/// Get wheel bindings for the tempo map overlay
fn tempo_map_wheel_bindings() -> Vec<WheelBind> {
    vec![
        // === Scroll with Wheel ===
        // Action 989: View: Scroll vertically (MIDI CC relative/mousewheel)
        WheelBind::new("", "989").with_description("Scroll view vertically"),
        // Shift + wheel / horizwheel = horizontal scroll reversed (action 977)
        // Action 977: View: Scroll horizontally reversed (MIDI CC relative/mousewheel)
        WheelBind::new("<S->", "977")
            .with_description("Scroll horizontally reversed (Shift+wheel)"),
        WheelBind::new("<S->", "977")
            .with_horizontal()
            .with_description("Scroll horizontally reversed (Shift+horizwheel)"),
        WheelBind::new("", "977")
            .with_horizontal()
            .with_description("Scroll horizontally reversed (horizwheel)"),
    ]
}

/// Create the Tempo Map override layer
pub fn tempo_map_override() -> KeybindOverride {
    KeybindOverride::new(
        "tempo-map",
        "Tempo mapping workflow keybindings - grid and transient manipulation",
    )
    .with_priority(100) // High priority to override base bindings
    .with_bindings(vec![
        // === Move Grid to Mouse ===
        Keybind::new("g", "FTS_TEMPO_MOVE_MEASURE_GRID_TO_MOUSE")
            .with_description("Move closest measure grid line to mouse cursor"),
        Keybind::new("<S-g>", "FTS_TEMPO_MOVE_MEASURE_GRID_TO_MOUSE_CONSTRAINED")
            .with_description("Move grid (constrained - anchors measure before)"),
        Keybind::new(
            "<A-g>",
            "FTS_TEMPO_MOVE_MEASURE_GRID_TO_MOUSE_FULLY_CONSTRAINED",
        )
        .with_description("Move grid (fully constrained - anchors both sides)"),
        // === Move Grid (any grid division) ===
        Keybind::new("<C-g>", "FTS_TEMPO_MOVE_GRID_TO_MOUSE")
            .with_description("Move closest grid line (any division) to mouse cursor"),
        // === Move Tempo Marker ===
        Keybind::new("<C-S-g>", "FTS_TEMPO_MOVE_MARKER_TO_MOUSE")
            .with_description("Move closest tempo marker to mouse cursor"),
        // === Snap to Transient ===
        Keybind::new("t", "FTS_TEMPO_SNAP_GRID_TO_TRANSIENT")
            .with_description("Snap closest measure grid line to next transient"),
        Keybind::new("<S-t>", "FTS_TEMPO_SNAP_GRID_TO_TRANSIENT_CONSTRAINED")
            .with_description("Snap to transient (constrained - anchors measure before)"),
        Keybind::new(
            "<A-t>",
            "FTS_TEMPO_SNAP_GRID_TO_TRANSIENT_FULLY_CONSTRAINED",
        )
        .with_description("Snap to transient (fully constrained - anchors both sides)"),
    ])
    .with_wheel_bindings(tempo_map_wheel_bindings())
}
