//! Wheel gestures, resolved from the shared input configuration.
//!
//! The editor used to hardcode a `match` on modifiers, and it disagreed
//! with both the REAPER config and the arrange view about what the same
//! gesture meant — Shift did nothing here, Alt scrolled sideways instead
//! of zooming, and plain wheel zoomed instead of scrolling. Three
//! schemes for one set of gestures.
//!
//! So the mapping is not written here at all. It comes from
//! `input`'s keymap, under the [`SURFACE`] key, and REAPER binds the same
//! gesture names to its own command ids. Changing what Shift+wheel does
//! is a config edit, in one file, for every surface at once.
//!
//! The editor is its own *surface* rather than reusing the arrange
//! view's because it does not fully exist in the DAW: it has gestures the
//! arrange view has no meaning for (nudging notes off-grid). A surface
//! may add to the shared scheme; it must not redefine it, which is what
//! `input`'s own tests pin.

use input::{ActionContext, ActionId, KeymapConfig, ScrollBindingTable, ScrollEvent};

use expression_editor_core::tools::Mods;

/// The editor's key in the keymap's `scroll` table.
pub const SURFACE: &str = "editor";

/// The resolved bindings, built once.
///
/// A `OnceLock` rather than a `use_signal`, because the table is process
/// state, not component state: every canvas in every panel resolves the
/// same gestures, and rebuilding it per mount would parse the keymap on
/// every remount.
static TABLE: std::sync::OnceLock<ScrollBindingTable> = std::sync::OnceLock::new();

/// Load the shipped defaults, overlaid with the user's keymap where
/// there is one.
///
/// A broken user keymap falls back to the defaults rather than leaving
/// the editor with no scroll at all — a config typo should cost you your
/// customisation, not the ability to scroll.
fn build() -> ScrollBindingTable {
    let mut config = match input::config::load_default_config() {
        Ok(c) => c,
        // The defaults are `include_str!`d and tested, so this is
        // unreachable short of a bad edit to the crate itself.
        Err(_) => KeymapConfig::default(),
    };

    // User overrides are a filesystem read, which a browser build has no
    // business attempting.
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(Some(user)) = input::config::load_user_config() {
        config = KeymapConfig::merge(config, user);
    }

    input::table_for_surface(&config, SURFACE).unwrap_or_default()
}

fn table() -> &'static ScrollBindingTable {
    TABLE.get_or_init(build)
}

/// One wheel notch, in pixels.
///
/// The conversion a touchpad needs. A mouse reports scroll in *lines* —
/// winit gives about 1.0 per notch — and every constant downstream is
/// tuned for that. A touchpad reports *pixels*, tens of them per event,
/// and the two were being passed on interchangeably: `strip_units()`
/// discards which unit arrived and hands over the bare number.
///
/// The result was not a subtle difference in feel. Zoom is
/// `e^(travel / 3)`, so a forty-pixel two-finger gesture asked for a
/// six-hundred-thousand-fold zoom, and a two-finger scroll panned five
/// thousand pixels an event. Touchpad input was unusable rather than
/// merely wrong.
const PIXELS_PER_NOTCH: f64 = 20.0;

/// A wheel delta in notches, whatever unit it arrived in.
///
/// Normalising here rather than at each use means the gain constants
/// stay tuned for one unit, and a device that reports a new one is a
/// change in this function alone.
pub fn notches(delta: &dioxus::html::geometry::WheelDelta) -> (f64, f64) {
    use dioxus::html::geometry::WheelDelta;
    match delta {
        WheelDelta::Lines(v) => (v.x, v.y),
        WheelDelta::Pixels(v) => (v.x / PIXELS_PER_NOTCH, v.y / PIXELS_PER_NOTCH),
        // A page is a screenful; treat it as a firm push rather than
        // scaling it into the thousands.
        WheelDelta::Pages(v) => (v.x * 4.0, v.y * 4.0),
    }
}

/// What the shared configuration says this gesture does here.
///
/// `None` when nothing is bound, which the caller should treat as "leave
/// the view alone" rather than falling back to a guess — an unbound
/// gesture that still did something is how the schemes drifted apart.
pub fn action_for(dx: f64, dy: f64, mods: Mods) -> Option<ActionId> {
    let event = ScrollEvent {
        delta_x: dx,
        delta_y: dy,
        modifiers: input::Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    };
    table().match_event(&event, &ActionContext::new())
}

/// A one-line summary of the wheel gestures, built from the bindings.
///
/// The empty roll used to state them as literal text, which went stale
/// the moment the config changed. Describing the table instead means the
/// hint cannot disagree with what the wheel actually does.
pub fn hint() -> String {
    let mut parts = Vec::new();
    for (label, mods) in [
        (
            "scroll",
            Mods {
                ctrl: false,
                alt: false,
                shift: false,
            },
        ),
        (
            "shift",
            Mods {
                ctrl: false,
                alt: false,
                shift: true,
            },
        ),
        (
            "alt",
            Mods {
                ctrl: false,
                alt: true,
                shift: false,
            },
        ),
        (
            "alt+shift",
            Mods {
                ctrl: false,
                alt: true,
                shift: true,
            },
        ),
    ] {
        let Some(action) = action_for(0.0, 1.0, mods) else {
            continue;
        };
        let what = match action.0.as_str() {
            "view.vscroll" => "scrolls",
            "view.hscroll" => "scrolls sideways",
            "view.zoom_v" => "zooms pitch",
            "view.zoom_h" => "zooms time",
            "view.zoom_both" => "zooms",
            other => other,
        };
        parts.push(format!("{label} {what}"));
    }
    parts.join(" \u{b7} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(dx: f64, dy: f64, mods: Mods) -> Option<String> {
        action_for(dx, dy, mods).map(|a| a.0)
    }

    fn mods(ctrl: bool, alt: bool, shift: bool) -> Mods {
        Mods { ctrl, alt, shift }
    }

    /// The bug this whole module exists for: Shift+wheel scrolled
    /// nothing, because the editor's hardcoded match had no arm for it.
    #[test]
    fn shift_wheel_scrolls_horizontally() {
        assert_eq!(
            act(0.0, 10.0, mods(false, false, true)).as_deref(),
            Some("view.hscroll")
        );
    }

    /// The editor scrolls with the shared scheme, and zooms on its own.
    ///
    /// `Alt+Scroll` zoomed here until `Alt` became "create". A modifier
    /// that means one thing on a drag and another on a wheel is no more
    /// derivable than one that changes meaning with the key held beside
    /// it — so the editor's table keeps the scrolling half of the shared
    /// scheme and drops the zooming half, which moved to held `Z`.
    ///
    /// The arrange view's `normal` table is untouched: it has no `Z`
    /// tool and no `Alt`-creates rule, so nothing there needs to move.
    #[test]
    fn the_editor_scrolls_with_the_shared_scheme() {
        assert_eq!(
            act(0.0, 10.0, mods(false, false, false)).as_deref(),
            Some("view.vscroll")
        );
        assert_eq!(
            act(0.0, 10.0, mods(false, false, true)).as_deref(),
            Some("view.hscroll")
        );
    }

    /// And `Alt` is not a zoom here any more, in any combination.
    ///
    /// The bug this pins is the one that was reported: holding Option to
    /// insert a note and scrolling zoomed the view instead.
    #[test]
    fn alt_no_longer_zooms_the_editor() {
        for m in [
            mods(false, true, false),
            mods(false, true, true),
            mods(true, true, false),
        ] {
            assert_eq!(
                act(0.0, 10.0, m),
                None,
                "Alt is the create modifier now, not a zoom: {m:?}"
            );
        }
    }

    #[test]
    fn the_editor_keeps_its_own_off_grid_nudge() {
        assert_eq!(
            act(0.0, 10.0, mods(true, false, true)).as_deref(),
            Some("edit.nudge_time")
        );
    }

    #[test]
    fn the_hint_describes_the_bindings_rather_than_a_fixed_string() {
        let h = hint();
        assert!(h.contains("shift scrolls sideways"), "{h}");
        // The failure this catches is the old one: a hint that says the
        // plain wheel zooms when the config says it scrolls.
        assert!(!h.contains("scroll zooms"), "stale hint: {h}");
    }

    /// A touchpad and a mouse must ask for comparable amounts.
    ///
    /// The bug this pins was not a difference in feel. Every gain
    /// downstream is tuned for a mouse's line deltas — zoom is
    /// `e^(travel / 3)` — and a touchpad's pixel deltas were handed over
    /// unconverted, so a forty-pixel two-finger gesture asked for a
    /// six-hundred-thousand-fold zoom.
    #[test]
    fn a_touchpads_pixels_and_a_mouses_lines_are_comparable() {
        use dioxus::html::geometry::{LinesVector, PixelsVector3D, WheelDelta};

        // One notch of a mouse wheel.
        let mouse = notches(&WheelDelta::Lines(LinesVector::new(0.0, 1.0, 0.0)));
        // A touchpad flick of about one notch's worth of travel.
        let pad = notches(&WheelDelta::Pixels(PixelsVector3D::new(
            0.0,
            PIXELS_PER_NOTCH,
            0.0,
        )));
        assert!(
            (mouse.1 - pad.1).abs() < 1e-9,
            "a notch and {PIXELS_PER_NOTCH}px should agree: {mouse:?} vs {pad:?}"
        );

        // And the thing that made it unusable: a raw pixel delta must
        // never reach the gain constants at full size.
        let big = notches(&WheelDelta::Pixels(PixelsVector3D::new(0.0, 40.0, 0.0)));
        assert!(
            big.1 < 4.0,
            "40px of travel became {} notches — the zoom would be e^{}",
            big.1,
            big.1 / 3.0
        );
    }

    #[test]
    fn an_unbound_gesture_resolves_to_nothing() {
        // Ctrl alone is deliberately unbound, matching REAPER. It must
        // report that rather than falling through to a default.
        assert_eq!(act(0.0, 10.0, mods(true, false, false)), None);
    }
}
