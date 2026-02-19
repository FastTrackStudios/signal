//! Signal domain action ID constants and definitions.
//!
//! Use `signal_actions::definitions()` to register all actions with an
//! `ActionDispatcher` or command palette. The constants are `StaticActionId`
//! values usable as compile-time keys in handler maps.
//!
//! # Navigation model
//!
//! Actions are context-free: they operate on whatever is currently active in
//! the UI. "Load Variant N" maps to the Nth variant of whatever collection is
//! loaded — sections if a song is active, patches if a profile is active,
//! scenes if a rig is active, etc. The UI layer owns the active-context state
//! and resolves N to a concrete entity when the action fires.

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

        // ── Direct variant load (1–24) ────────────────────────────────
        //
        // Each action loads the Nth variant of whatever collection is
        // currently active: song sections, profile patches, rig scenes, etc.

        LOAD_VARIANT_1 = "fts.signal.load_variant.1" {
            name: "Load Variant 1",
            description: "Load the 1st variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_2 = "fts.signal.load_variant.2" {
            name: "Load Variant 2",
            description: "Load the 2nd variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_3 = "fts.signal.load_variant.3" {
            name: "Load Variant 3",
            description: "Load the 3rd variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_4 = "fts.signal.load_variant.4" {
            name: "Load Variant 4",
            description: "Load the 4th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_5 = "fts.signal.load_variant.5" {
            name: "Load Variant 5",
            description: "Load the 5th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_6 = "fts.signal.load_variant.6" {
            name: "Load Variant 6",
            description: "Load the 6th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_7 = "fts.signal.load_variant.7" {
            name: "Load Variant 7",
            description: "Load the 7th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_8 = "fts.signal.load_variant.8" {
            name: "Load Variant 8",
            description: "Load the 8th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_9 = "fts.signal.load_variant.9" {
            name: "Load Variant 9",
            description: "Load the 9th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_10 = "fts.signal.load_variant.10" {
            name: "Load Variant 10",
            description: "Load the 10th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_11 = "fts.signal.load_variant.11" {
            name: "Load Variant 11",
            description: "Load the 11th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_12 = "fts.signal.load_variant.12" {
            name: "Load Variant 12",
            description: "Load the 12th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_13 = "fts.signal.load_variant.13" {
            name: "Load Variant 13",
            description: "Load the 13th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_14 = "fts.signal.load_variant.14" {
            name: "Load Variant 14",
            description: "Load the 14th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_15 = "fts.signal.load_variant.15" {
            name: "Load Variant 15",
            description: "Load the 15th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_16 = "fts.signal.load_variant.16" {
            name: "Load Variant 16",
            description: "Load the 16th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_17 = "fts.signal.load_variant.17" {
            name: "Load Variant 17",
            description: "Load the 17th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_18 = "fts.signal.load_variant.18" {
            name: "Load Variant 18",
            description: "Load the 18th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_19 = "fts.signal.load_variant.19" {
            name: "Load Variant 19",
            description: "Load the 19th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_20 = "fts.signal.load_variant.20" {
            name: "Load Variant 20",
            description: "Load the 20th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_21 = "fts.signal.load_variant.21" {
            name: "Load Variant 21",
            description: "Load the 21st variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_22 = "fts.signal.load_variant.22" {
            name: "Load Variant 22",
            description: "Load the 22nd variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_23 = "fts.signal.load_variant.23" {
            name: "Load Variant 23",
            description: "Load the 23rd variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
        LOAD_VARIANT_24 = "fts.signal.load_variant.24" {
            name: "Load Variant 24",
            description: "Load the 24th variant of the active collection",
            category: Session,
            menu_path: "FTS/Signal/Variants",
        }
    }
}

/// All 24 Load Variant action IDs as a const array, indexed 0–23.
///
/// `LOAD_VARIANT_BY_INDEX[0]` == `LOAD_VARIANT_1`, etc. Useful when
/// a MIDI dispatcher or UI needs to map variant index → action at runtime.
pub const LOAD_VARIANT_BY_INDEX: [actions_proto::ids::StaticActionId; 24] = [
    signal_actions::LOAD_VARIANT_1,
    signal_actions::LOAD_VARIANT_2,
    signal_actions::LOAD_VARIANT_3,
    signal_actions::LOAD_VARIANT_4,
    signal_actions::LOAD_VARIANT_5,
    signal_actions::LOAD_VARIANT_6,
    signal_actions::LOAD_VARIANT_7,
    signal_actions::LOAD_VARIANT_8,
    signal_actions::LOAD_VARIANT_9,
    signal_actions::LOAD_VARIANT_10,
    signal_actions::LOAD_VARIANT_11,
    signal_actions::LOAD_VARIANT_12,
    signal_actions::LOAD_VARIANT_13,
    signal_actions::LOAD_VARIANT_14,
    signal_actions::LOAD_VARIANT_15,
    signal_actions::LOAD_VARIANT_16,
    signal_actions::LOAD_VARIANT_17,
    signal_actions::LOAD_VARIANT_18,
    signal_actions::LOAD_VARIANT_19,
    signal_actions::LOAD_VARIANT_20,
    signal_actions::LOAD_VARIANT_21,
    signal_actions::LOAD_VARIANT_22,
    signal_actions::LOAD_VARIANT_23,
    signal_actions::LOAD_VARIANT_24,
];

/// Resolve a 1-based variant index to its `StaticActionId`.
///
/// Returns `None` if `n` is outside the range 1–24.
pub const fn load_variant_action(n: usize) -> Option<actions_proto::ids::StaticActionId> {
    if n >= 1 && n <= 24 {
        Some(LOAD_VARIANT_BY_INDEX[n - 1])
    } else {
        None
    }
}
