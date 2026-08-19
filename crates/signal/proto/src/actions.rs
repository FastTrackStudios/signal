//! Signal domain actions.
//!
//! `SignalActionsActions::all()` is the whole list, for registering with a
//! host (REAPER's action list, a command palette) or resolving an id at
//! runtime. The per-action `ActionMeta` constants beside the trait are
//! compile-time keys usable in handler maps.
//!
//! Declaration only: signal-proto has no `Daw` and no controller, so the
//! handlers live with the host that owns them — today
//! `signal-extension`'s `handle_action`. Keeping the declaration here means
//! every host registers the same ids, names and descriptions.
//!
//! # Navigation model
//!
//! Actions are context-free: they operate on whatever is currently active in
//! the UI. "Switch to Variation N" switches to the Nth variant of whatever
//! collection is active — sections if a song is active, patches if a profile
//! is active, scenes if a rig is active, etc. The UI layer owns the
//! active-context state and resolves N to a concrete entity when the action
//! fires.

use architect::action::ActionMeta;

#[architect::actions(namespace = "FTS_SIGNAL")]
pub trait SignalActions {
    // ── Song navigation ───────────────────────────────────────────────

    #[action(
        description = "Advance to the next song in the active setlist",
        category = "Session",
        group = "Navigate"
    )]
    fn next_song(&self);

    #[action(
        description = "Go back to the previous song in the active setlist",
        category = "Session",
        group = "Navigate"
    )]
    fn previous_song(&self);

    // ── Section / variant navigation ──────────────────────────────────

    #[action(
        description = "Advance to the next section (or patch/scene) within the active song",
        category = "Session",
        group = "Navigate"
    )]
    fn next_section(&self);

    #[action(
        description = "Go back to the previous section within the active song",
        category = "Session",
        group = "Navigate"
    )]
    fn previous_section(&self);

    // ── Direct variant switch (1–24) ──────────────────────────────────
    //
    // Each action switches to the Nth variant of whatever collection is
    // currently active: song sections, profile patches, rig scenes, etc.

    #[action(
        display_name = "Switch to Variation 1",
        description = "Switch to the 1st variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_1(&self);

    #[action(
        display_name = "Switch to Variation 2",
        description = "Switch to the 2nd variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_2(&self);

    #[action(
        display_name = "Switch to Variation 3",
        description = "Switch to the 3rd variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_3(&self);

    #[action(
        display_name = "Switch to Variation 4",
        description = "Switch to the 4th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_4(&self);

    #[action(
        display_name = "Switch to Variation 5",
        description = "Switch to the 5th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_5(&self);

    #[action(
        display_name = "Switch to Variation 6",
        description = "Switch to the 6th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_6(&self);

    #[action(
        display_name = "Switch to Variation 7",
        description = "Switch to the 7th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_7(&self);

    #[action(
        display_name = "Switch to Variation 8",
        description = "Switch to the 8th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_8(&self);

    #[action(
        display_name = "Switch to Variation 9",
        description = "Switch to the 9th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_9(&self);

    #[action(
        display_name = "Switch to Variation 10",
        description = "Switch to the 10th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_10(&self);

    #[action(
        display_name = "Switch to Variation 11",
        description = "Switch to the 11th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_11(&self);

    #[action(
        display_name = "Switch to Variation 12",
        description = "Switch to the 12th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_12(&self);

    #[action(
        display_name = "Switch to Variation 13",
        description = "Switch to the 13th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_13(&self);

    #[action(
        display_name = "Switch to Variation 14",
        description = "Switch to the 14th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_14(&self);

    #[action(
        display_name = "Switch to Variation 15",
        description = "Switch to the 15th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_15(&self);

    #[action(
        display_name = "Switch to Variation 16",
        description = "Switch to the 16th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_16(&self);

    #[action(
        display_name = "Switch to Variation 17",
        description = "Switch to the 17th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_17(&self);

    #[action(
        display_name = "Switch to Variation 18",
        description = "Switch to the 18th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_18(&self);

    #[action(
        display_name = "Switch to Variation 19",
        description = "Switch to the 19th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_19(&self);

    #[action(
        display_name = "Switch to Variation 20",
        description = "Switch to the 20th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_20(&self);

    #[action(
        display_name = "Switch to Variation 21",
        description = "Switch to the 21st variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_21(&self);

    #[action(
        display_name = "Switch to Variation 22",
        description = "Switch to the 22nd variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_22(&self);

    #[action(
        display_name = "Switch to Variation 23",
        description = "Switch to the 23rd variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_23(&self);

    #[action(
        display_name = "Switch to Variation 24",
        description = "Switch to the 24th variation of the active collection",
        category = "Session",
        group = "Variations"
    )]
    fn switch_to_variation_24(&self);

    // ── Dev / demo content ────────────────────────────────────────────

    #[action(
        display_name = "Signal - Dev - Load Demo Guitar Rig",
        description = "Create a demo guitar rig with tracks, stock plugins, and macro bindings",
        category = "Session",
        group = "Dev"
    )]
    fn dev_load_demo_guitar_rig(&self);

    #[action(
        display_name = "Signal - Dev - Load Demo Guitar Profile",
        description = "Create the All-Around guitar profile with 8 scene variations using stock plugins",
        category = "Session",
        group = "Dev"
    )]
    fn dev_load_demo_guitar_profile(&self);

    #[action(
        display_name = "Signal - Dev - Generate Scene MIDI Items",
        description = "Generate colored MIDI items on the profile track for switching between scene variations",
        category = "Session",
        group = "Dev"
    )]
    fn dev_generate_scene_midi_items(&self);

    #[action(
        display_name = "Signal - Dev - Load Demo Setlist",
        description = "Create a demo setlist with 8 songs, each with sections and MIDI switching items",
        category = "Session",
        group = "Dev"
    )]
    fn dev_load_demo_setlist(&self);

    // ── Place switch items ────────────────────────────────────────────

    #[action(
        display_name = "Signal - Place Section Switch",
        description = "Place a section-switch MIDI item at the edit cursor for the selected track's scene",
        category = "Session",
        group = "Place"
    )]
    fn place_section_switch(&self);

    #[action(
        display_name = "Signal - Place Song Switch",
        description = "Place a song-switch MIDI item at the edit cursor for the selected track's song",
        category = "Session",
        group = "Place"
    )]
    fn place_song_switch(&self);

    #[action(
        display_name = "Signal - Place Scene Switch",
        description = "Place a scene-switch MIDI item at the edit cursor for the selected track's profile scene",
        category = "Session",
        group = "Place"
    )]
    fn place_scene_switch(&self);

    // ── Macro learn ───────────────────────────────────────────────────

    #[action(
        display_name = "Signal - Macro Arm",
        description = "Arm the next available macro for learning (touch FX params to bind them)",
        category = "Session",
        group = "Macro"
    )]
    fn macro_arm(&self);

    #[action(
        display_name = "Signal - Macro Disarm",
        description = "Disarm the current macro and finalize all learned bindings",
        category = "Session",
        group = "Macro"
    )]
    fn macro_disarm(&self);

    #[action(
        display_name = "Signal - Macro Set Point",
        description = "Set a curve point: captures current macro knob position and last-touched param value",
        category = "Session",
        group = "Macro"
    )]
    fn macro_set_point(&self);

    #[action(
        display_name = "Signal - Macro Remove Last Point",
        description = "Remove the last curve point added for the last-touched parameter",
        category = "Session",
        group = "Macro"
    )]
    fn macro_remove_last_point(&self);

    #[action(
        display_name = "Signal - Macro Set Min",
        description = "Set the minimum (macro=0) value for the last-touched parameter",
        category = "Session",
        group = "Macro"
    )]
    fn macro_set_min(&self);

    #[action(
        display_name = "Signal - Macro Set Max",
        description = "Set the maximum (macro=1) value for the last-touched parameter",
        category = "Session",
        group = "Macro"
    )]
    fn macro_set_max(&self);

    #[action(
        display_name = "Signal - Macro Clear",
        description = "Clear all pending bindings for the currently armed macro",
        category = "Session",
        group = "Macro"
    )]
    fn macro_clear(&self);

    #[action(
        display_name = "Signal - Add Macro",
        description = "Add a new macro knob to the active bank",
        category = "Session",
        group = "Macro"
    )]
    fn macro_add(&self);
}

/// All 24 Switch to Variation actions, indexed 0–23.
///
/// `SWITCH_TO_VARIATION_BY_INDEX[0]` is variation 1, etc. Useful when a MIDI
/// dispatcher or UI needs to map a variation index to an action at runtime.
pub const SWITCH_TO_VARIATION_BY_INDEX: [&ActionMeta; 24] = [
    &SWITCH_TO_VARIATION_1,
    &SWITCH_TO_VARIATION_2,
    &SWITCH_TO_VARIATION_3,
    &SWITCH_TO_VARIATION_4,
    &SWITCH_TO_VARIATION_5,
    &SWITCH_TO_VARIATION_6,
    &SWITCH_TO_VARIATION_7,
    &SWITCH_TO_VARIATION_8,
    &SWITCH_TO_VARIATION_9,
    &SWITCH_TO_VARIATION_10,
    &SWITCH_TO_VARIATION_11,
    &SWITCH_TO_VARIATION_12,
    &SWITCH_TO_VARIATION_13,
    &SWITCH_TO_VARIATION_14,
    &SWITCH_TO_VARIATION_15,
    &SWITCH_TO_VARIATION_16,
    &SWITCH_TO_VARIATION_17,
    &SWITCH_TO_VARIATION_18,
    &SWITCH_TO_VARIATION_19,
    &SWITCH_TO_VARIATION_20,
    &SWITCH_TO_VARIATION_21,
    &SWITCH_TO_VARIATION_22,
    &SWITCH_TO_VARIATION_23,
    &SWITCH_TO_VARIATION_24,
];

/// Resolve a 1-based variation index to its action.
///
/// Returns `None` if `n` is outside the range 1–24.
pub const fn switch_to_variation_action(n: usize) -> Option<&'static ActionMeta> {
    if n >= 1 && n <= 24 {
        Some(SWITCH_TO_VARIATION_BY_INDEX[n - 1])
    } else {
        None
    }
}
