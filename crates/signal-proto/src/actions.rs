//! Signal domain action ID constants and definitions.
//!
//! Use `signal_actions::definitions()` to register all actions with an
//! `ActionDispatcher` or command palette. The constants are `StaticActionId`
//! values usable as compile-time keys in handler maps.
//!
//! # Navigation model
//!
//! Actions are context-free: they operate on whatever is currently active in
//! the UI. "Switch to Variation N" switches to the Nth variant of whatever
//! collection is active — sections if a song is active, patches if a profile
//! is active, scenes if a rig is active, etc. The UI layer owns the
//! active-context state and resolves N to a concrete entity when the action
//! fires.

actions_proto::declare_actions! {
    /// Signal navigation action ID constants.
    pub signal_actions {

        // ── Song navigation ───────────────────────────────────────────

        NEXT_SONG = "fts.signal.next_song" {
            name: "Next Song",
            description: "Advance to the next song in the active setlist",
            category: Session,
            menu_path: "FTS/Signal/Navigate",
        }

        PREVIOUS_SONG = "fts.signal.previous_song" {
            name: "Previous Song",
            description: "Go back to the previous song in the active setlist",
            category: Session,
            menu_path: "FTS/Signal/Navigate",
        }

        // ── Section / variant navigation ──────────────────────────────

        NEXT_SECTION = "fts.signal.next_section" {
            name: "Next Section",
            description: "Advance to the next section (or patch/scene) within the active song",
            category: Session,
            menu_path: "FTS/Signal/Navigate",
        }

        PREVIOUS_SECTION = "fts.signal.previous_section" {
            name: "Previous Section",
            description: "Go back to the previous section within the active song",
            category: Session,
            menu_path: "FTS/Signal/Navigate",
        }

        // ── Direct variant switch (1–24) ─────────────────────────────
        //
        // Each action switches to the Nth variant of whatever collection
        // is currently active: song sections, profile patches, rig scenes, etc.

        SWITCH_TO_VARIATION_1 = "fts.signal.switch_to_variation.1" {
            name: "Switch to Variation 1",
            description: "Switch to the 1st variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_2 = "fts.signal.switch_to_variation.2" {
            name: "Switch to Variation 2",
            description: "Switch to the 2nd variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_3 = "fts.signal.switch_to_variation.3" {
            name: "Switch to Variation 3",
            description: "Switch to the 3rd variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_4 = "fts.signal.switch_to_variation.4" {
            name: "Switch to Variation 4",
            description: "Switch to the 4th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_5 = "fts.signal.switch_to_variation.5" {
            name: "Switch to Variation 5",
            description: "Switch to the 5th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_6 = "fts.signal.switch_to_variation.6" {
            name: "Switch to Variation 6",
            description: "Switch to the 6th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_7 = "fts.signal.switch_to_variation.7" {
            name: "Switch to Variation 7",
            description: "Switch to the 7th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_8 = "fts.signal.switch_to_variation.8" {
            name: "Switch to Variation 8",
            description: "Switch to the 8th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_9 = "fts.signal.switch_to_variation.9" {
            name: "Switch to Variation 9",
            description: "Switch to the 9th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_10 = "fts.signal.switch_to_variation.10" {
            name: "Switch to Variation 10",
            description: "Switch to the 10th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_11 = "fts.signal.switch_to_variation.11" {
            name: "Switch to Variation 11",
            description: "Switch to the 11th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_12 = "fts.signal.switch_to_variation.12" {
            name: "Switch to Variation 12",
            description: "Switch to the 12th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_13 = "fts.signal.switch_to_variation.13" {
            name: "Switch to Variation 13",
            description: "Switch to the 13th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_14 = "fts.signal.switch_to_variation.14" {
            name: "Switch to Variation 14",
            description: "Switch to the 14th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_15 = "fts.signal.switch_to_variation.15" {
            name: "Switch to Variation 15",
            description: "Switch to the 15th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_16 = "fts.signal.switch_to_variation.16" {
            name: "Switch to Variation 16",
            description: "Switch to the 16th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_17 = "fts.signal.switch_to_variation.17" {
            name: "Switch to Variation 17",
            description: "Switch to the 17th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_18 = "fts.signal.switch_to_variation.18" {
            name: "Switch to Variation 18",
            description: "Switch to the 18th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_19 = "fts.signal.switch_to_variation.19" {
            name: "Switch to Variation 19",
            description: "Switch to the 19th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_20 = "fts.signal.switch_to_variation.20" {
            name: "Switch to Variation 20",
            description: "Switch to the 20th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_21 = "fts.signal.switch_to_variation.21" {
            name: "Switch to Variation 21",
            description: "Switch to the 21st variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_22 = "fts.signal.switch_to_variation.22" {
            name: "Switch to Variation 22",
            description: "Switch to the 22nd variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_23 = "fts.signal.switch_to_variation.23" {
            name: "Switch to Variation 23",
            description: "Switch to the 23rd variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
        SWITCH_TO_VARIATION_24 = "fts.signal.switch_to_variation.24" {
            name: "Switch to Variation 24",
            description: "Switch to the 24th variation of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variations",
        }
    }
}

/// All 24 Switch to Variation action IDs as a const array, indexed 0–23.
///
/// `SWITCH_TO_VARIATION_BY_INDEX[0]` == `SWITCH_TO_VARIATION_1`, etc. Useful
/// when a MIDI dispatcher or UI needs to map variation index → action at runtime.
pub const SWITCH_TO_VARIATION_BY_INDEX: [actions_proto::ids::StaticActionId; 24] = [
    signal_actions::SWITCH_TO_VARIATION_1,
    signal_actions::SWITCH_TO_VARIATION_2,
    signal_actions::SWITCH_TO_VARIATION_3,
    signal_actions::SWITCH_TO_VARIATION_4,
    signal_actions::SWITCH_TO_VARIATION_5,
    signal_actions::SWITCH_TO_VARIATION_6,
    signal_actions::SWITCH_TO_VARIATION_7,
    signal_actions::SWITCH_TO_VARIATION_8,
    signal_actions::SWITCH_TO_VARIATION_9,
    signal_actions::SWITCH_TO_VARIATION_10,
    signal_actions::SWITCH_TO_VARIATION_11,
    signal_actions::SWITCH_TO_VARIATION_12,
    signal_actions::SWITCH_TO_VARIATION_13,
    signal_actions::SWITCH_TO_VARIATION_14,
    signal_actions::SWITCH_TO_VARIATION_15,
    signal_actions::SWITCH_TO_VARIATION_16,
    signal_actions::SWITCH_TO_VARIATION_17,
    signal_actions::SWITCH_TO_VARIATION_18,
    signal_actions::SWITCH_TO_VARIATION_19,
    signal_actions::SWITCH_TO_VARIATION_20,
    signal_actions::SWITCH_TO_VARIATION_21,
    signal_actions::SWITCH_TO_VARIATION_22,
    signal_actions::SWITCH_TO_VARIATION_23,
    signal_actions::SWITCH_TO_VARIATION_24,
];

/// Resolve a 1-based variation index to its `StaticActionId`.
///
/// Returns `None` if `n` is outside the range 1–24.
pub const fn switch_to_variation_action(n: usize) -> Option<actions_proto::ids::StaticActionId> {
    if n >= 1 && n <= 24 {
        Some(SWITCH_TO_VARIATION_BY_INDEX[n - 1])
    } else {
        None
    }
}
