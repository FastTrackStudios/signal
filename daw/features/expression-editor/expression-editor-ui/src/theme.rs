//! Colors and inline-style helpers.
//!
//! Every style in this crate is inline. Blitz does not load external
//! stylesheets reliably, and the same components have to render
//! identically standalone, as a VST3/CLAP editor, and in the browser —
//! so there is nowhere to put a stylesheet that all three would agree
//! on. Tailwind classes may be added on top, but never depended on.
//!
//! The colours themselves are **not** defined here. They are re-exported
//! from [`daw_theme::defaults`], the canonical FastTrackStudio palette that
//! also drives the Dioxus panels and generates the REAPER theme. Change a
//! colour there and this editor, the mixer and REAPER all move together —
//! that shared vocabulary is the point, and a local literal would quietly
//! opt this surface out of it.
//!
//! These stay `&'static str` consts (rather than resolving a theme at
//! runtime) because inline styles are built in `rsx!` at every call site;
//! a runtime theme is the next step, and wants a context, not a global.

use daw_theme::defaults as d;

/// Canvas background.
pub const BG: &str = d::SURFACE;
/// Piano-roll white-key row.
pub const ROW_WHITE: &str = d::ROW_WHITE;
/// Piano-roll black-key row.
pub const ROW_BLACK: &str = d::ROW_BLACK;
/// Octave (C) divider.
pub const OCTAVE_LINE: &str = d::OCTAVE_LINE;
/// Beat gridline.
pub const GRID_BEAT: &str = d::GRID_BEAT;
/// Subdivision gridline.
pub const GRID_SUB: &str = d::GRID_SUB;

pub const PANEL: &str = d::SURFACE_RAISED;
pub const PANEL_BORDER: &str = d::BORDER;
pub const TEXT: &str = d::TEXT;
pub const TEXT_DIM: &str = d::TEXT_DIM;
pub const ACCENT: &str = d::ACCENT;
pub const SELECTED: &str = d::SELECTED;

/// Q-zone structure and the ambiguity warning share red on purpose:
/// both mean "this region has boundaries you must be aware of".
pub const ZONE: &str = d::ZONE;
/// Microtonal centers and off-ET targets.
pub const GOLD: &str = d::MICROTONAL;
/// Transport position.
pub const PLAYHEAD: &str = d::PLAYHEAD;
/// Piano-key gutter.
pub const KEY_WHITE: &str = d::KEY_WHITE;
pub const KEY_BLACK: &str = d::KEY_BLACK;
pub const GUTTER_BG: &str = d::GUTTER;
/// Razor areas. Distinct from ZONE red — a razor is a region you are
/// about to operate on, not a warning.
pub const RAZOR: &str = d::RAZOR;
/// The tracked pitch contour on the audio surface — white, and the
/// brightest line there.
pub const PITCH_TRACK: &str = d::PITCH_TRACK;

/// Notes from another track, drawn behind the active one.
pub const REFERENCE: &str = d::REFERENCE;

/// How well a sung note matches its target, in tune to out of tune.
pub const TUNE_RAMP: [&str; 5] = d::TUNE_RAMP;

/// The ramp step for a deviation of `cents`.
///
/// Twenty cents is about where a held note starts sounding wrong to a
/// listener, so that is where the ramp reaches its far end. Below five
/// it reads as dead on — chasing the last few cents of a sung note is
/// how vocals end up sounding synthetic.
pub fn tune_color(cents: f64) -> &'static str {
    let c = cents.abs();
    let step = if c < 5.0 {
        0
    } else if c < 10.0 {
        1
    } else if c < 15.0 {
        2
    } else if c < 20.0 {
        3
    } else {
        4
    };
    TUNE_RAMP[step]
}

/// A control's resting surface, and the engaged variant.
pub const CONTROL: &str = d::CONTROL;
pub const CONTROL_ACTIVE: &str = d::CONTROL_ACTIVE;
/// An inset well inside a panel.
pub const SURFACE_INSET: &str = d::SURFACE_INSET;
/// A divider that must read above [`PANEL_BORDER`].
pub const BORDER_STRONG: &str = d::BORDER_STRONG;
/// Emphasised text, above [`TEXT`].
pub const TEXT_BRIGHT: &str = d::TEXT_BRIGHT;
/// Faintest text — placeholders, disabled labels.
pub const TEXT_FAINT: &str = d::TEXT_FAINT;
/// The deepest surface step, below [`BG`].
pub const SURFACE_DEEP: &str = d::SURFACE_DEEP;
/// A well or dimension cut into a panel.
pub const SURFACE_SUNKEN: &str = d::SURFACE_SUNKEN;
/// Toolbars and status bars.
pub const SURFACE_BAR: &str = d::SURFACE_BAR;
/// Audio waveform bodies — the blue-steel every DAW draws peaks in.
pub const PEAKS: &str = d::PEAKS;
/// A selected-but-not-engaged control.
pub const CONTROL_SELECTED: &str = d::CONTROL_SELECTED;
/// A control under the pointer.
pub const CONTROL_HOVER: &str = d::CONTROL_HOVER;
/// The groove a slider handle runs in.
pub const CONTROL_GROOVE: &str = d::CONTROL_GROOVE;
/// A draggable handle — knob pointer, slider thumb.
pub const HANDLE: &str = d::HANDLE;
/// White-key labels in the gutter.
pub const KEY_LABEL: &str = d::KEY_LABEL;
/// Informational and advisory banner backdrops.
pub const BANNER_INFO: &str = d::BANNER_INFO;
pub const BANNER_WARN: &str = d::BANNER_WARN;

/// Multi-tool zone hues — one per transform, so a zone is identifiable
/// before its label is readable.
pub const TOOL_COMPRESS: &str = d::TOOL_COMPRESS;
pub const TOOL_SCALE: &str = d::TOOL_SCALE;
pub const TOOL_TILT: &str = d::TOOL_TILT;
pub const TOOL_STRETCH: &str = d::TOOL_STRETCH;
pub const TOOL_WARP: &str = d::TOOL_WARP;
pub const TOOL_MOVE: &str = d::TOOL_MOVE;
pub const TOOL_HISTORY: &str = d::TOOL_HISTORY;

/// Twelve pitch-class hues. Notes are colored by pitch class so a
/// melodic shape is readable at a glance without reading the piano
/// keys; selection adds a bright outline rather than a fill change, so
/// the hue survives selection.
pub const PITCH_CLASS: [&str; 12] = d::PITCH_CLASSES;

pub fn pitch_class_color(row: i32) -> &'static str {
    PITCH_CLASS[row.rem_euclid(12) as usize]
}

/// Per-dimension curve color.
pub fn lane_color(dimension: expression_editor_core::Dimension) -> &'static str {
    use expression_editor_core::Dimension;
    match dimension {
        Dimension::Pitch => d::LANE_PITCH,
        Dimension::Pressure => d::LANE_PRESSURE,
        Dimension::Timbre => d::LANE_TIMBRE,
    }
}

pub fn lane_label(dimension: expression_editor_core::Dimension) -> &'static str {
    use expression_editor_core::Dimension;
    match dimension {
        Dimension::Pitch => "Pitch",
        Dimension::Pressure => "Pressure",
        Dimension::Timbre => "Timbre",
    }
}

pub fn is_black_key(row: i32) -> bool {
    matches!(row.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
}

/// A toolbar button, active or not.
pub fn button_style(active: bool) -> String {
    let (bg, border, fg) = if active {
        (d::CONTROL_ACTIVE, ACCENT, SELECTED)
    } else {
        (d::CONTROL, PANEL_BORDER, TEXT)
    };
    format!(
        "display: flex; align-items: center; justify-content: center; \
         gap: 4px; min-width: 26px; height: 26px; padding: 0 7px; \
         background: {bg}; border: 1px solid {border}; border-radius: 5px; \
         color: {fg}; font-size: 11px; line-height: 1; cursor: pointer; \
         user-select: none;"
    )
}

/// A labelled group in the toolbar.
pub fn group_style() -> String {
    format!(
        "display: flex; align-items: center; gap: 4px; padding: 0 8px; \
         border-right: 1px solid {PANEL_BORDER};"
    )
}

pub fn group_label_style() -> String {
    format!(
        "color: {TEXT_DIM}; font-size: 9px; letter-spacing: 0.08em; \
         text-transform: uppercase; margin-right: 2px; user-select: none;"
    )
}

pub fn select_style() -> String {
    format!(
        "height: 26px; background: {CONTROL}; border: 1px solid {PANEL_BORDER}; \
         border-radius: 5px; color: {TEXT}; font-size: 11px; padding: 0 6px;"
    )
}

/// Mode icons, as inline SVG path data on a 16×16 grid.
///
/// Drawn rather than typed: the glyphs a font would give us
/// (🎸 🥁 🎤) render inconsistently across Blitz, wasm and a plugin
/// window, and emoji colour fights the bar's palette. These inherit
/// `currentColor`, so an active mode's icon lights with its label.
pub fn mode_icon(mode: expression_editor_core::Mode) -> &'static str {
    use expression_editor_core::Mode;
    match mode {
        // Piano keys.
        Mode::Midi => "M1 4h14v8H1z M4.5 4v5 M7 4v5 M10.5 4v5 M13 4v5",
        // Keys with a bend arrow over them — per-note expression.
        Mode::Mpe => {
            "M1 7h14v5H1z M4.5 7v3 M7 7v3 M10.5 7v3 M13 7v3 \
                      M2 4.5c3-3 5 1 7-1s3-1 4 0"
        }
        // Kick drum: a circle with lugs and a beater line.
        Mode::Drums => {
            "M8 2.5a5.5 5.5 0 100 11 5.5 5.5 0 100-11z \
                        M8 5.5a2.5 2.5 0 100 5 2.5 2.5 0 100-5z \
                        M2.6 5.3l2.2 1 M13.4 5.3l-2.2 1 \
                        M4.8 12.6l1-2.2 M11.2 12.6l-1-2.2"
        }
        // Guitar: body, neck, and strings.
        Mode::Guitar => {
            "M5 14a3 3 0 100-6 3 3 0 100 6z M6.6 9.6l6-6 \
                         M11.4 2.2l2.4 2.4 M12.2 5.4l-1.6-1.6"
        }
        // Microphone.
        Mode::Vocals => {
            "M8 2a2 2 0 012 2v4a2 2 0 01-4 0V4a2 2 0 012-2z \
                         M4.5 7.5a3.5 3.5 0 007 0 M8 11v3 M6 14h4"
        }
        // Waveform.
        Mode::PitchedAudio => {
            "M1 8h1.5 M2.5 8v0 M3.5 5v6 M5.5 3v10 M7.5 6v4 \
                        M9.5 2v12 M11.5 5v6 M13.5 7v2 M15 8h0"
        }
        // Transients: a baseline with decaying spikes on it. The same
        // family as the pitched-audio waveform, but struck rather than
        // sung — which is the distinction the mode exists to make.
        Mode::UnpitchedAudio => "M1 13h14 M3 13V4l1.2 9 M7.5 13V6l1.2 7 M12 13V2l1.2 11",
    }
}

#[cfg(test)]
mod tests {
    /// Every colour this module exposes must come from the canonical
    /// palette, never a local literal.
    ///
    /// A literal here is how a surface quietly opts out of the shared theme:
    /// it keeps rendering, looks fine in isolation, and only surfaces much
    /// later as "the editor doesn't quite match REAPER". Two such duplicates
    /// existed before this migration — one of them was the accent, re-typed
    /// by hand.
    #[test]
    fn no_colour_is_defined_locally() {
        for line in include_str!("theme.rs").lines() {
            let line = line.trim();
            if !line.starts_with("pub const") || !line.contains(": &str") {
                continue;
            }
            assert!(
                line.contains("d::"),
                "colour defined locally instead of in daw_theme::defaults:\n  {line}"
            );
        }
    }

    /// No module in this crate may write a colour literal.
    ///
    /// The narrower test above only guards `theme.rs`; the drift that
    /// actually happened was 31 literals scattered through the *other*
    /// modules, where a `background: #1a1a22` in an rsx style reads as
    /// perfectly ordinary code. Six near-identical darks had accumulated
    /// that way, doing the same job under different values.
    ///
    /// Every module is listed explicitly rather than walked: a new file
    /// should have to opt in here, which is a moment to ask whether it
    /// needs a new named colour or an existing one.
    #[test]
    fn no_module_writes_a_hex_literal() {
        const MODULES: [(&str, &str); 9] = [
            ("theme.rs", include_str!("theme.rs")),
            ("lib.rs", include_str!("lib.rs")),
            ("canvas.rs", include_str!("canvas.rs")),
            ("drawer.rs", include_str!("drawer.rs")),
            ("inspector.rs", include_str!("inspector.rs")),
            ("interaction.rs", include_str!("interaction.rs")),
            ("multitool_ui.rs", include_str!("multitool_ui.rs")),
            ("toolbar.rs", include_str!("toolbar.rs")),
            ("widgets.rs", include_str!("widgets.rs")),
        ];

        let mut found = Vec::new();
        for (name, src) in MODULES {
            for (n, line) in src.lines().enumerate() {
                let trimmed = line.trim_start();
                // Doc comments legitimately name colours when explaining a
                // choice; only real code counts.
                if trimmed.starts_with("//") {
                    continue;
                }
                if is_hex_literal(line) {
                    found.push(format!("{name}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(
            found.is_empty(),
            "hex literals must come from daw_theme::defaults:\n  {}",
            found.join("\n  ")
        );
    }

    /// A `#` followed by exactly 3, 6 or 8 hex digits — a CSS colour.
    ///
    /// Deliberately not a regex: SVG path data is full of `#`-free hex-ish
    /// tokens, and CSS ids would false-positive on a looser rule.
    fn is_hex_literal(line: &str) -> bool {
        let bytes = line.as_bytes();
        for (i, _) in line.match_indices('#') {
            let rest = &bytes[i + 1..];
            let n = rest.iter().take_while(|b| b.is_ascii_hexdigit()).count();
            let terminated = rest.get(n).is_none_or(|b| !b.is_ascii_alphanumeric());
            if matches!(n, 3 | 6 | 8) && terminated {
                return true;
            }
        }
        false
    }
}
