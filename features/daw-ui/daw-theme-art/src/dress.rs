//! How a control is dressed, stated once for both renderings.
//!
//! A [`crate::vector_controls::LabelButton`] is a shape with a dozen knobs
//! on it — how far the face lifts under the pointer, how bright the legend
//! prints, where the body sits in its cell — and every one of those numbers
//! was measured off the art it replaces. They are not the component's
//! defaults, because the mixer's mute and the track panel's mute disagree
//! about most of them; they are properties of *the image being drawn*.
//!
//! They used to live in [`crate::export`], as closures inside
//! `cell_markup`. That was fine while the exporter was the only caller. It
//! stops being fine the moment the app draws the same button live: the
//! exporter is behind the `render` feature, so a `default-features = false`
//! consumer — which is exactly what the GUI is — cannot see those numbers
//! and would have to restate them. Two statements of a measured value drift,
//! and the drift shows up as the app's mute button and REAPER's mute button
//! being subtly different buttons.
//!
//! So the dressing lives here, unconditionally compiled, and both callers
//! read it.

use daw_theme::{Color, Theme};

use crate::slice::NamedArt;
use crate::vector_controls::{Interaction, ToggleProps};

/// Which of REAPER's two control families an image belongs to.
///
/// The same components draw both, at different boxes and with different
/// measurements — the mixer's mute is 21x20 and fills its cell, the track
/// panel's is 21x24 with the button inset. Every function here keys off
/// this rather than off a `bool`, because `mute_art(true, false)` says
/// nothing at a call site and this is the distinction the whole table turns
/// on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Panel {
    /// The mixer strip — `mcp_*`.
    #[default]
    Mixer,
    /// The track control panel — `track_*`, and a handful of `tcp_*`.
    Track,
}

impl Panel {
    /// Which family an image name belongs to.
    ///
    /// REAPER names most track-panel controls `track_*` and a handful
    /// `tcp_*`; where both exist for one control they are the same drawing.
    pub fn of(name: &str) -> Self {
        if name.starts_with("track_") || name.starts_with("tcp_") {
            Self::Track
        } else {
            Self::Mixer
        }
    }

    /// Track panel or mixer, as the `bool` the measured tables are keyed
    /// by. Public because the tables are: `label_legend`'s seven-arm match
    /// is measured data, not a decision that wants an enum per axis.
    pub fn is_track(self) -> bool {
        self == Self::Track
    }
}

/// The resting face of a label button.
///
/// Measured off the `off` cells: #464646 in the mixer, #4e4e4e in the track
/// panel, both falling about 12% over the button's height.
pub fn label_unlit(panel: Panel) -> Option<Color> {
    Some(
        Theme::default()
            .chrome
            .hardware
            .shade(if panel.is_track() { 0.078 } else { 0.036 }),
    )
}

/// The printed letter's colour.
///
/// Not one grey. It brightens when the button lights, and the track panel
/// swings much further than the mixer:
///
/// ```text
///     mixer   mute 204/204   solo 204/217   defeat 217
///     track   mute 183/242   solo 203/255   defeat 255
/// ```
///
/// Read as a single #f2f2f2 the track panel's unlit buttons came out sixty
/// levels hot, which is most of why they were the worst two images in the
/// set by pixel error.
pub fn label_legend(panel: Panel, lit: bool, solo: bool) -> Option<Color> {
    let up = match (panel.is_track(), lit, solo) {
        (false, _, false) => 0.45,
        (false, false, true) => 0.45,
        (false, true, true) => 0.59,
        (true, false, false) => 0.23,
        (true, true, false) => 0.86,
        (true, false, true) => 0.44,
        (true, true, true) => 1.0,
    };
    Some(Theme::default().chrome.hardware_mark.shade(up))
}

/// Where the button body sits in its cell.
///
/// The track panel's buttons occupy rows 1..20 of a 24-row cell; the
/// mixer's fill theirs.
pub fn label_body(panel: Panel) -> (f32, f32) {
    if panel.is_track() {
        (1.0 / 24.0, 20.0 / 24.0)
    } else {
        (0.0, 1.0)
    }
}

/// Everything a mute button needs, for the image `art` names.
///
/// `width`, `height` and `at` are left at their defaults — the caller owns
/// those: the exporter draws at the source box in three fixed pointer
/// states, the GUI draws at whatever size CSS gave it in whatever state the
/// pointer is actually in. Both take the rest by struct update:
///
/// ```ignore
/// ToggleProps { at, width, height, ..dress::mute(art, on) }
/// ```
pub fn mute(art: NamedArt, on: bool) -> ToggleProps {
    let panel = Panel::of(art.name);
    let track = panel.is_track();
    ToggleProps {
        // Mute's hover is the gentlest of the three — see
        // `vector_controls::ink`.
        hover: 0.25,
        // 0.11, and applied as a scale — measured 0.89 on every channel
        // from the top of the face to the bottom.
        depth: 0.11,
        on,
        art,
        body: label_body(panel),
        legend: label_legend(panel, on, false),
        unlit: label_unlit(panel),
        // The track panel's pressed cell is identical to its normal one;
        // the mixer's is darker.
        sinks: !track,
        width: None,
        height: None,
        at: Interaction::Normal,
    }
}

/// The image a mute button draws, given the panel it is in and its state.
///
/// The state is part of the *name* — REAPER ships `mcp_mute_on` and
/// `mcp_mute_off` as separate three-cell strips — so a live wrapper picks
/// its `NamedArt` per render rather than passing `on` to one image.
pub fn mute_art(panel: Panel, on: bool) -> NamedArt {
    crate::slice::expect_art(match (panel, on) {
        (Panel::Mixer, false) => "mcp_mute_off",
        (Panel::Mixer, true) => "mcp_mute_on",
        (Panel::Track, false) => "track_mute_off",
        (Panel::Track, true) => "track_mute_on",
    })
}

/// The track colour, as REAPER actually paints it on a panel.
///
/// REAPER does not paint the raw track colour: it darkens it. Measured in
/// one screenshot holding both renders of the same project — REAPER's TCP
/// painted its Kick row `#9D3C55` where our mixer band painted the track's
/// own `#E0567A`. All three channels land on 0.700, 0.698 and 0.697 of the
/// original, which is a flat 30% mix toward black and nothing subtler.
///
/// Everything track-coloured goes through here: the mixer's band and index
/// plate, the track panel's row. Painting the raw colour made every panel
/// read a stop brighter than REAPER's, and it was the largest single block
/// of difference left in the strip comparison.
pub fn panel_tint(colour: daw_theme::Color) -> daw_theme::Color {
    colour.shade(-0.30)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_families_are_dressed_differently() {
        let mixer = mute(mute_art(Panel::Mixer, true), true);
        let track = mute(mute_art(Panel::Track, true), true);
        assert_ne!(mixer.body, track.body, "the track panel's button is inset");
        assert_ne!(mixer.legend, track.legend);
        assert!(mixer.sinks, "the mixer's pressed cell is darker");
        assert!(!track.sinks, "the track panel's pressed cell is not");
    }

    /// The two ways of naming a family must agree: `Panel::of` reads the
    /// prefix, `mute_art` writes it.
    #[test]
    fn the_family_survives_the_round_trip() {
        for panel in [Panel::Mixer, Panel::Track] {
            for on in [false, true] {
                assert_eq!(Panel::of(mute_art(panel, on).name), panel);
            }
        }
    }

    /// The numbers are measurements, not taste. A drive-by tidy that
    /// rounds them is a button that stops matching the art it replaces.
    #[test]
    fn the_measured_numbers_are_the_measured_numbers() {
        let m = mute(mute_art(Panel::Mixer, true), true);
        assert_eq!(m.hover, 0.25, "mute's hover is the gentlest of the three");
        assert_eq!(
            m.depth, 0.11,
            "measured 0.89 on every channel, top to bottom"
        );
    }

    #[test]
    fn each_state_names_its_own_image() {
        assert_eq!(mute_art(Panel::Mixer, true).name, "mcp_mute_on");
        assert_eq!(mute_art(Panel::Mixer, false).name, "mcp_mute_off");
        assert_eq!(mute_art(Panel::Track, true).name, "track_mute_on");
        assert_eq!(mute_art(Panel::Track, false).name, "track_mute_off");
        // The two families are drawn at different boxes, which is why the
        // wrapper cannot assume one MCP_MUTE constant.
        assert_eq!(mute_art(Panel::Mixer, true).source, (21.0, 20.0));
        assert_eq!(mute_art(Panel::Track, true).source, (21.0, 24.0));
    }
}
