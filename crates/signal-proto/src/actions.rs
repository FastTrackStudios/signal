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

actions_proto::define_actions! {
    /// Signal navigation action ID constants.
    pub signal_actions {
        prefix: "fts.signal",
        title: "Signal",

        // ── Song navigation ───────────────────────────────────────────

        NEXT_SONG = "next_song" {
            name: "Next Song",
            description: "Advance to the next song in the active setlist",
            category: Session,
            group: "Navigate",
        }

        PREVIOUS_SONG = "previous_song" {
            name: "Previous Song",
            description: "Go back to the previous song in the active setlist",
            category: Session,
            group: "Navigate",
        }

        // ── Section / variant navigation ──────────────────────────────

        NEXT_SECTION = "next_section" {
            name: "Next Section",
            description: "Advance to the next section (or patch/scene) within the active song",
            category: Session,
            group: "Navigate",
        }

        PREVIOUS_SECTION = "previous_section" {
            name: "Previous Section",
            description: "Go back to the previous section within the active song",
            category: Session,
            group: "Navigate",
        }

        // ── Direct variant switch (1–24) ─────────────────────────────
        //
        // Each action switches to the Nth variant of whatever collection
        // is currently active: song sections, profile patches, rig scenes, etc.

        SWITCH_TO_VARIATION_1 = "switch_to_variation.1" {
            name: "Switch to Variation 1",
            description: "Switch to the 1st variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_2 = "switch_to_variation.2" {
            name: "Switch to Variation 2",
            description: "Switch to the 2nd variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_3 = "switch_to_variation.3" {
            name: "Switch to Variation 3",
            description: "Switch to the 3rd variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_4 = "switch_to_variation.4" {
            name: "Switch to Variation 4",
            description: "Switch to the 4th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_5 = "switch_to_variation.5" {
            name: "Switch to Variation 5",
            description: "Switch to the 5th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_6 = "switch_to_variation.6" {
            name: "Switch to Variation 6",
            description: "Switch to the 6th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_7 = "switch_to_variation.7" {
            name: "Switch to Variation 7",
            description: "Switch to the 7th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_8 = "switch_to_variation.8" {
            name: "Switch to Variation 8",
            description: "Switch to the 8th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_9 = "switch_to_variation.9" {
            name: "Switch to Variation 9",
            description: "Switch to the 9th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_10 = "switch_to_variation.10" {
            name: "Switch to Variation 10",
            description: "Switch to the 10th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_11 = "switch_to_variation.11" {
            name: "Switch to Variation 11",
            description: "Switch to the 11th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_12 = "switch_to_variation.12" {
            name: "Switch to Variation 12",
            description: "Switch to the 12th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_13 = "switch_to_variation.13" {
            name: "Switch to Variation 13",
            description: "Switch to the 13th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_14 = "switch_to_variation.14" {
            name: "Switch to Variation 14",
            description: "Switch to the 14th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_15 = "switch_to_variation.15" {
            name: "Switch to Variation 15",
            description: "Switch to the 15th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_16 = "switch_to_variation.16" {
            name: "Switch to Variation 16",
            description: "Switch to the 16th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_17 = "switch_to_variation.17" {
            name: "Switch to Variation 17",
            description: "Switch to the 17th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_18 = "switch_to_variation.18" {
            name: "Switch to Variation 18",
            description: "Switch to the 18th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_19 = "switch_to_variation.19" {
            name: "Switch to Variation 19",
            description: "Switch to the 19th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_20 = "switch_to_variation.20" {
            name: "Switch to Variation 20",
            description: "Switch to the 20th variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_21 = "switch_to_variation.21" {
            name: "Switch to Variation 21",
            description: "Switch to the 21st variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_22 = "switch_to_variation.22" {
            name: "Switch to Variation 22",
            description: "Switch to the 22nd variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_23 = "switch_to_variation.23" {
            name: "Switch to Variation 23",
            description: "Switch to the 23rd variation of the active collection",
            category: Session,
            group: "Variations",
        }
        SWITCH_TO_VARIATION_24 = "switch_to_variation.24" {
            name: "Switch to Variation 24",
            description: "Switch to the 24th variation of the active collection",
            category: Session,
            group: "Variations",
        }

        // ── Dev / Demo ─────────────────────────────────────────────

        LOAD_DEMO_GUITAR_RIG = "dev.load_demo_guitar_rig" {
            name: "Signal - Dev - Load Demo Guitar Rig",
            description: "Create a demo guitar rig with tracks, stock plugins, and macro bindings",
            category: Dev,
            group: "Dev",
        }

        LOAD_DEMO_GUITAR_PROFILE = "dev.load_demo_guitar_profile" {
            name: "Signal - Dev - Load Demo Guitar Profile",
            description: "Create the All-Around guitar profile with 8 scene variations using stock plugins",
            category: Dev,
            group: "Dev",
        }

        GENERATE_SCENE_MIDI_ITEMS = "dev.generate_scene_midi_items" {
            name: "Signal - Dev - Generate Scene MIDI Items",
            description: "Generate colored MIDI items on the profile track for switching between scene variations",
            category: Dev,
            group: "Dev",
        }

        LOAD_DEMO_SETLIST = "dev.load_demo_setlist" {
            name: "Signal - Dev - Load Demo Setlist",
            description: "Create a demo setlist with 8 songs, each with sections and MIDI switching items",
            category: Dev,
            group: "Dev",
        }

        // ── Place Switch (edit cursor) ────────────────────────────────

        PLACE_SECTION_SWITCH = "place_section_switch" {
            name: "Signal - Place Section Switch",
            description: "Place a section-switch MIDI item at the edit cursor for the selected track's scene",
            category: Session,
            group: "Place",
        }

        PLACE_SONG_SWITCH = "place_song_switch" {
            name: "Signal - Place Song Switch",
            description: "Place a song-switch MIDI item at the edit cursor for the selected track's song",
            category: Session,
            group: "Place",
        }

        PLACE_SCENE_SWITCH = "place_scene_switch" {
            name: "Signal - Place Scene Switch",
            description: "Place a scene-switch MIDI item at the edit cursor for the selected track's profile scene",
            category: Session,
            group: "Place",
        }

        // ── Macro Arm / Learn ───────────────────────────────────────────

        MACRO_ARM = "macro_arm" {
            name: "Signal - Macro Arm",
            description: "Arm the next available macro for learning (touch FX params to bind them)",
            category: Session,
            group: "Macro",
        }

        MACRO_DISARM = "macro_disarm" {
            name: "Signal - Macro Disarm",
            description: "Disarm the current macro and finalize all learned bindings",
            category: Session,
            group: "Macro",
        }

        MACRO_SET_POINT = "macro_set_point" {
            name: "Signal - Macro Set Point",
            description: "Set a curve point: captures current macro knob position and last-touched param value",
            category: Session,
            group: "Macro",
        }

        MACRO_REMOVE_LAST_POINT = "macro_remove_last_point" {
            name: "Signal - Macro Remove Last Point",
            description: "Remove the last curve point added for the last-touched parameter",
            category: Session,
            group: "Macro",
        }

        MACRO_SET_MIN = "macro_set_min" {
            name: "Signal - Macro Set Min",
            description: "Set the minimum (macro=0) value for the last-touched parameter",
            category: Session,
            group: "Macro",
        }

        MACRO_SET_MAX = "macro_set_max" {
            name: "Signal - Macro Set Max",
            description: "Set the maximum (macro=1) value for the last-touched parameter",
            category: Session,
            group: "Macro",
        }

        MACRO_CLEAR = "macro_clear" {
            name: "Signal - Macro Clear",
            description: "Clear all pending bindings for the currently armed macro",
            category: Session,
            group: "Macro",
        }

        MACRO_ADD = "macro_add" {
            name: "Signal - Add Macro",
            description: "Add a new macro knob to the active bank",
            category: Session,
            group: "Macro",
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
