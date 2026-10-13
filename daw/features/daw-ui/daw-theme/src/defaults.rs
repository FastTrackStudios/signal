//! The authored FastTrackStudio palette, as hex.
//!
//! **This module is the source of truth.** [`crate::Theme::default`] parses
//! these, and consumers that need compile-time `&'static str` colours — the
//! expression editor renders every style inline, because Blitz will not load
//! a stylesheet reliably — use them directly. Change a colour here and both
//! the panels and the generated REAPER theme follow.
//!
//! Hex rather than [`crate::Color`] because a `const` can't call a parser,
//! and because these strings are what a designer reads and edits.
//!
//! # These are ReaperTips' colours, on purpose
//!
//! The chrome here is the source theme's own palette, decoded from the
//! `.ReaperTheme` it shipped with — a mid-grey set (`#3e3e3e` surfaces),
//! not the near-black blue this held before. That is a deliberate,
//! temporary state: with the palette identical, a generated control and
//! the art it replaces can differ only in *shape*, so the comparison
//! sheets and `--example align` measure geometry and nothing else.
//!
//! Restyling to an FTS palette is a matter of changing these values back;
//! nothing downstream assumes greys.

// ── chrome ───────────────────────────────────────────────────────────────

/// The window behind everything — the middle of the surface ladder.
///
/// Four ordered steps, darkest first, and the order is enforced by a test:
///
/// ```text
/// SURFACE_DEEP  →  SURFACE_SUNKEN  →  SURFACE  →  SURFACE_RAISED
///    #08080b          #0a0a0e        #0d0d11       #15151c
/// ```
///
/// Reach down the ladder for something recessed and up for something
/// floating. Keeping it ordered is what makes those words mean anything.
pub const SURFACE: &str = "#3e3e3e";
/// A panel or strip sitting on `SURFACE`.
pub const SURFACE_RAISED: &str = "#424242";
/// Wells, lanes and troughs cut into a panel — the arrange view, an entry
/// field. Genuinely below [`SURFACE`]: it was inherited a shade *lighter*,
/// which made the name a lie and left nothing to reach for when something
/// needed to read as recessed.
pub const SURFACE_SUNKEN: &str = "#333333";
/// Hairlines between regions.
pub const BORDER: &str = "#323232";
/// Primary label text.
pub const TEXT: &str = "#a0a0a0";
/// Secondary text — units, inactive labels.
pub const TEXT_DIM: &str = "#7b7b7b";
/// Tertiary text — watermark level.
pub const TEXT_FAINT: &str = "#5a5a5a";
/// The one colour meaning "live / selected / yours".
///
/// ReaperTips' own blue, sampled from the routing lanes and the FX bypass
/// LED. Held here rather than a nearby Tailwind blue so that a generated
/// control and the art it replaces differ in *shape only* — which is the
/// whole point of the comparison sheets.
pub const ACCENT: &str = "#46b9fe";
/// A control's resting surface — buttons, selects, entry fields.
pub const CONTROL: &str = "#464646";
/// A control that is engaged: the resting surface pulled toward the accent.
pub const CONTROL_ACTIVE: &str = "#2f5f7d";
/// An inset well inside a panel — one step below `SURFACE_RAISED`.
pub const SURFACE_INSET: &str = "#383838";
/// A divider or handle that needs to read above `BORDER`.
pub const BORDER_STRONG: &str = "#565656";
/// Emphasised text, above `TEXT`.
pub const TEXT_BRIGHT: &str = "#c2c6ce";
/// Selection highlight, brighter than the accent.
pub const SELECTED: &str = "#e6f4ff";

/// The deepest step — the bottom of the ladder, e.g. behind a piano-roll
/// gutter. See the ladder note on [`SURFACE`].
pub const SURFACE_DEEP: &str = "#2a2a2a";
/// Toolbars and status bars: a bar sitting across a surface.
pub const SURFACE_BAR: &str = "#2d2d2d";
/// A control that is selected but not engaged — accent-tinted, quieter
/// than [`CONTROL_ACTIVE`].
pub const CONTROL_SELECTED: &str = "#3a4a52";
/// A control under the pointer.
pub const CONTROL_HOVER: &str = "#505050";
/// The groove a slider or scrollbar handle runs in.
pub const CONTROL_GROOVE: &str = "#2e2e2e";
/// A draggable handle — knob pointer, slider thumb.
pub const HANDLE: &str = "#c2c6ce";

// ── hardware controls ────────────────────────────────────────────────────
//
// Neutral, and deliberately not on the surface ladder — see the note on
// `Chrome::hardware`. These are measured off the theme's own artwork:
// `mcp_mute_off` and `mcp_fx_norm` are #3f3f3f faces on #171717 edges, and
// `mcp_pan_knob_small` puts an #a2a2a2 dot on a #393939 body.

/// The resting face of a button, knob body or cap.
///
/// Distinct from [`CONTROL`], which is a *flat* panel control — a select,
/// an entry field — and belongs on the surface ladder. This is moulded
/// plastic sitting on top of a mixer strip.
pub const HARDWARE: &str = "#3f3f3f";
/// The hairline around a hardware control — far darker than [`BORDER`].
pub const HARDWARE_EDGE: &str = "#171717";
/// The light marker *on* a hardware control: a knob's dot, a cap's grip.
pub const HARDWARE_MARK: &str = "#a2a2a2";

// ── banners ──────────────────────────────────────────────────────────────
//
// Tinted strips that carry a message. The tint is the message, so these
// are deliberately related to `ACCENT` and `MICROTONAL` rather than free.

/// Informational strip — accent family, heavily darkened.
pub const BANNER_INFO: &str = "#0b1a24";
/// Advisory strip — the microtonal amber, heavily darkened.
pub const BANNER_WARN: &str = "#422006";

// ── signal ───────────────────────────────────────────────────────────────

pub const METER_SAFE: &str = "#22c55e";
/// The routing button's send lane, sampled from `mcp_io_s`.
pub const METER_WARN: &str = "#f4c54e";
/// Lit red: the routing button's receive lane and the FX bypass LED.
///
/// Distinct from [`REC`], which is the record ring's deeper `#e23b53` —
/// the source theme really does use two reds, and collapsing them makes
/// an armed track and a bypassed chain shout at the same volume.
pub const METER_DANGER: &str = "#ff5260";
/// Sampled from `mcp_solo_on` — duller than the meter amber.
pub const SOLO: &str = "#d3a738";
/// Deeper and less saturated than [`REC`], which is the point: a muted
/// track is a *withdrawn* state, and the source theme draws it a full step
/// darker than record-arm rather than as another bright red. This was a
/// generic `#ef4444` for a while and made every muted track shout louder
/// than an armed one.
pub const MUTE: &str = "#b8394e";
pub const REC: &str = "#e23b53";
/// Waveform / peak body.
pub const PEAKS: &str = "#5b8def";
/// Playhead and edit cursor — the same idea on both surfaces.
pub const PLAYHEAD: &str = "#f8fafc";
/// A track with no colour assigned.
pub const NEUTRAL_TRACK: &str = "#828282";

// ── measured off REAPER ──────────────────────────────────────────────────
//
// Sampled off REAPER's own render during convergence (#240). These were
// inlined where they were measured, which made them invisible to a
// re-palette: everything moved except the most carefully matched parts.
// The measurement each records lives in its doc comment; change the value
// here and the panels and the exported art move together.

/// The strip's own grey — `mcp_namebg`, the mixer strip body, the plate
/// under the track name, and the track panel's name field. Measured off
/// the strip next to REAPER's: a shade darker than [`SURFACE_RAISED`],
/// which read as a light bar across the bottom of every strip.
pub const STRIP_BODY: &str = "#262626";
/// The volume knob's lit ring where the value starts, at the lower left.
/// Sampled round REAPER's ring — it is not one flat blue; see
/// [`VOLUME_RING_LIT_TOP`].
pub const VOLUME_RING_LIT: &str = "#5ec3ff";
/// The same ring by the time it reaches the top — lit from below, like
/// the rest of this theme's hardware.
pub const VOLUME_RING_LIT_TOP: &str = "#4b7994";
/// The unlit side of the volume knob's ring — a slate that stays legible
/// against the body. Derived from the accent it was too dark to see, so
/// the knob looked like a lit arc floating on nothing.
pub const VOLUME_RING_UNLIT: &str = "#3e4a51";
/// The fixed-lanes button's dots when the track is not in fixed-lane
/// mode, traced off `custom_fixed_lanes_off.png`. The on state is
/// [`SOLO`]'s amber.
pub const FIXED_LANES_DOT: &str = "#818181";

// ── editor ───────────────────────────────────────────────────────────────

/// White-key row in the piano roll.
pub const ROW_WHITE: &str = "#1d1d26";
/// Black-key row.
pub const ROW_BLACK: &str = "#131319";
/// The octave (C) divider — heavier than a beat line.
pub const OCTAVE_LINE: &str = "#3a3a4d";
/// Beat gridline.
pub const GRID_BEAT: &str = "#2b2b3a";
/// Subdivision gridline.
pub const GRID_SUB: &str = "#20202a";
/// Piano-key gutter background.
pub const GUTTER: &str = "#101016";
pub const KEY_WHITE: &str = "#d8dce6";
pub const KEY_BLACK: &str = "#26262f";
/// Note-name labels printed on the white keys — dark enough to read on
/// [`KEY_WHITE`] without competing with the notes beside it.
pub const KEY_LABEL: &str = "#33333f";
/// Structural zones and ambiguity warnings — both mean "there are
/// boundaries here you must be aware of".
pub const ZONE: &str = "#ef4444";
/// Microtonal centres and off-ET targets.
pub const MICROTONAL: &str = "#eab308";
/// Razor areas. Deliberately distinct from [`ZONE`]: a razor is a region you
/// are about to operate on, not a warning about one.
pub const RAZOR: &str = "#22d3ee";
/// How well a sung note matches its target pitch, in tune to out of tune.
///
/// Blue reads as correct and red as wrong without needing a legend, and
/// the ramp runs through violet rather than through grey so no step
/// looks like "disabled". Five steps, not a continuous gradient: a
/// singer is deciding whether a note needs fixing, and a handful of
/// distinguishable states answers that faster than a smooth scale where
/// every note is a slightly different colour.
pub const TUNE_RAMP: [&str; 5] = [
    "#3b82f6", // dead on
    "#6366f1", "#a855f7", "#e0398a", "#ef4444", // badly out
];

/// The tracked pitch contour on the audio surface.
///
/// White, and the brightest thing on that screen on purpose: it is the
/// line every edit is aimed at, and it has to stay readable crossing
/// note bodies whose own colour is already reporting tuning.
pub const PITCH_TRACK: &str = "#f8fafc";

/// Notes belonging to another track, shown behind the one being edited.
/// Desaturated on purpose — a reference has to be legible enough to tune
/// against and quiet enough that it is never mistaken for editable.
pub const REFERENCE: &str = "#7c8598";

/// Twelve pitch-class hues, C first.
///
/// Notes are coloured by pitch class so a melodic shape is readable without
/// reading the keys. Selection adds an outline rather than changing the fill,
/// so the hue survives being selected.
pub const PITCH_CLASSES: [&str; 12] = [
    "#ef4444", // C
    "#f97316", // C#
    "#f59e0b", // D
    "#eab308", // D#
    "#84cc16", // E
    "#22c55e", // F
    "#14b8a6", // F#
    "#06b6d4", // G
    "#3b82f6", // G#
    "#6366f1", // A
    "#a855f7", // A#
    "#ec4899", // B
];

// ── multi-tool zones ─────────────────────────────────────────────────────
//
// One hue per transform, so a zone is identifiable before its label is
// readable. A family like the pitch classes: what matters is that
// neighbours stay tellable apart, not any individual value.

/// Compress / expand vertically.
pub const TOOL_COMPRESS: &str = "#f472b6";
/// Scale from an edge.
pub const TOOL_SCALE: &str = "#38bdf8";
/// Tilt around a pivot.
pub const TOOL_TILT: &str = "#fbbf24";
/// Stretch in time.
pub const TOOL_STRETCH: &str = "#a3e635";
/// Warp along a curve.
pub const TOOL_WARP: &str = "#c084fc";
/// Move without reshaping.
pub const TOOL_MOVE: &str = "#2dd4bf";
/// Undo / redo — deliberately neutral, since history is not a transform.
pub const TOOL_HISTORY: &str = "#94a3b8";

// ── per-lane expression curves ───────────────────────────────────────────

pub const LANE_PITCH: &str = "#7dd3fc";
pub const LANE_PRESSURE: &str = "#fda4af";
pub const LANE_TIMBRE: &str = "#a7f3d0";

// ── metrics ──────────────────────────────────────────────────────────────

/// Corner radius, px.
pub const RADIUS: f32 = 5.0;
/// How far a panel surface blends toward its track colour (0–1).
pub const TRACK_TINT: f32 = 0.12;
/// Extra dim on unselected surfaces (0–1).
pub const DIM_UNSELECTED: f32 = 0.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    /// Every colour in this module, so a malformed one can't reach a
    /// consumer as a silently-broken CSS string.
    fn all() -> Vec<(&'static str, &'static str)> {
        let mut v = vec![
            ("SURFACE", SURFACE),
            ("SURFACE_RAISED", SURFACE_RAISED),
            ("SURFACE_SUNKEN", SURFACE_SUNKEN),
            ("BORDER", BORDER),
            ("TEXT", TEXT),
            ("TEXT_DIM", TEXT_DIM),
            ("TEXT_FAINT", TEXT_FAINT),
            ("ACCENT", ACCENT),
            ("CONTROL", CONTROL),
            ("CONTROL_ACTIVE", CONTROL_ACTIVE),
            ("SURFACE_INSET", SURFACE_INSET),
            ("BORDER_STRONG", BORDER_STRONG),
            ("TEXT_BRIGHT", TEXT_BRIGHT),
            ("SURFACE_DEEP", SURFACE_DEEP),
            ("SURFACE_BAR", SURFACE_BAR),
            ("CONTROL_SELECTED", CONTROL_SELECTED),
            ("CONTROL_HOVER", CONTROL_HOVER),
            ("CONTROL_GROOVE", CONTROL_GROOVE),
            ("HANDLE", HANDLE),
            ("HARDWARE", HARDWARE),
            ("HARDWARE_EDGE", HARDWARE_EDGE),
            ("HARDWARE_MARK", HARDWARE_MARK),
            ("BANNER_INFO", BANNER_INFO),
            ("BANNER_WARN", BANNER_WARN),
            ("TOOL_COMPRESS", TOOL_COMPRESS),
            ("TOOL_SCALE", TOOL_SCALE),
            ("TOOL_TILT", TOOL_TILT),
            ("TOOL_STRETCH", TOOL_STRETCH),
            ("TOOL_WARP", TOOL_WARP),
            ("TOOL_MOVE", TOOL_MOVE),
            ("TOOL_HISTORY", TOOL_HISTORY),
            ("SELECTED", SELECTED),
            ("METER_SAFE", METER_SAFE),
            ("METER_WARN", METER_WARN),
            ("METER_DANGER", METER_DANGER),
            ("SOLO", SOLO),
            ("MUTE", MUTE),
            ("REC", REC),
            ("PEAKS", PEAKS),
            ("PLAYHEAD", PLAYHEAD),
            ("NEUTRAL_TRACK", NEUTRAL_TRACK),
            ("ROW_WHITE", ROW_WHITE),
            ("ROW_BLACK", ROW_BLACK),
            ("OCTAVE_LINE", OCTAVE_LINE),
            ("GRID_BEAT", GRID_BEAT),
            ("GRID_SUB", GRID_SUB),
            ("GUTTER", GUTTER),
            ("KEY_WHITE", KEY_WHITE),
            ("KEY_BLACK", KEY_BLACK),
            ("KEY_LABEL", KEY_LABEL),
            ("ZONE", ZONE),
            ("MICROTONAL", MICROTONAL),
            ("RAZOR", RAZOR),
            ("LANE_PITCH", LANE_PITCH),
            ("LANE_PRESSURE", LANE_PRESSURE),
            ("LANE_TIMBRE", LANE_TIMBRE),
            ("STRIP_BODY", STRIP_BODY),
            ("VOLUME_RING_LIT", VOLUME_RING_LIT),
            ("VOLUME_RING_LIT_TOP", VOLUME_RING_LIT_TOP),
            ("VOLUME_RING_UNLIT", VOLUME_RING_UNLIT),
            ("FIXED_LANES_DOT", FIXED_LANES_DOT),
        ];
        for (i, p) in PITCH_CLASSES.iter().enumerate() {
            v.push((Box::leak(format!("PITCH_CLASSES[{i}]").into_boxed_str()), p));
        }
        v
    }

    #[test]
    fn every_default_is_parseable_hex() {
        for (name, hex) in all() {
            assert!(
                Color::hex(hex).is_some(),
                "{name} = {hex:?} is not valid hex"
            );
        }
    }

    #[test]
    fn pitch_classes_are_all_distinct() {
        // Two identical hues means two pitch classes are indistinguishable,
        // which defeats colouring notes by pitch class at all.
        let mut seen = PITCH_CLASSES.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate pitch-class hues");
    }

    #[test]
    fn row_colours_are_dark_enough_to_sit_under_notes() {
        // Piano-roll rows are a backdrop; if one drifts bright, every note
        // on it loses contrast.
        for row in [ROW_WHITE, ROW_BLACK, GUTTER] {
            let l = Color::hex(row).unwrap().luminance();
            assert!(l < 0.1, "{row} is too light for a row backdrop ({l})");
        }
    }

    fn lum(h: &str) -> f32 {
        Color::hex(h).unwrap().luminance()
    }

    #[test]
    fn tool_hues_are_all_distinct() {
        // Same reasoning as the pitch classes: two identical hues means two
        // transforms are indistinguishable before you read the label.
        let mut tools = vec![
            TOOL_COMPRESS,
            TOOL_SCALE,
            TOOL_TILT,
            TOOL_STRETCH,
            TOOL_WARP,
            TOOL_MOVE,
            TOOL_HISTORY,
        ];
        tools.sort_unstable();
        let before = tools.len();
        tools.dedup();
        assert_eq!(before, tools.len(), "duplicate multi-tool hues");
    }

    #[test]
    fn surface_steps_form_a_ladder() {
        // The named steps must actually be ordered, or "sunken" and "raised"
        // stop meaning anything and nobody can reason about which to reach
        // for.
        assert!(lum(SURFACE_DEEP) < lum(SURFACE_SUNKEN), "DEEP below SUNKEN");
        assert!(lum(SURFACE_SUNKEN) <= lum(SURFACE), "SUNKEN below SURFACE");
        assert!(lum(SURFACE) < lum(SURFACE_RAISED), "SURFACE below RAISED");
    }

    #[test]
    fn banners_are_dark_enough_to_carry_text() {
        // A banner is a backdrop for a message; if it drifts bright the text
        // on it stops being readable.
        for b in [BANNER_INFO, BANNER_WARN] {
            assert!(lum(b) < 0.1, "{b} is too light for a banner backdrop");
        }
    }

    #[test]
    fn zone_and_razor_are_not_the_same_colour() {
        // They mean different things — a warning versus a pending edit — and
        // the code comments say so; keep them visually separable.
        assert_ne!(ZONE, RAZOR);
    }
}
