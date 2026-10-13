//! How the strip collapses as it gets shorter.
//!
//! A short mixer has to stay usable, not become a crushed copy of a tall
//! one, so REAPER drops parts of the strip at fixed heights. Those heights
//! are the target — the map's standing rule is that REAPER is the source of
//! truth while mapping — and they are already written down in the theme, so
//! this is data entry rather than design.
//!
//! # It is not one breakpoint ladder
//!
//! Five different kinds of thing, and only the first is a plain height
//! comparison:
//!
//! 1. **Container height.** The input-FX row, the record input, the pan
//!    labels, the pan section and the fader's dB readout each disappear at
//!    their own height.
//! 2. **A derived residual.** What is left after the fixed bands — the
//!    stretch section. The IO, envelope and phase buttons drop at their own
//!    values *of that*, and padding steps down in three stages.
//! 3. **A gap between two resolved siblings.** Not modelled here yet; see
//!    the note on [`Collapse::fader`].
//! 4. **A widget-type switch.** Below a threshold the fader stops being a
//!    fader and becomes a knob — in Dioxus a Rust conditional, not CSS.
//! 5. **A re-anchor.** Below the pan-section threshold the pan *section* is
//!    gone but the pan *control* is not: it re-parents into the input area.
//!    Genuine re-anchoring, not repositioning.
//!
//! The numbers themselves live in [`daw_theme_art::collapse`] — they are
//! facts about the theme, and `fts-themer` needs the same ones to keep the
//! layout file in step. This module is what a strip *does* with them.
//!
//! # Why this is Rust and not a container query
//!
//! A container query can only ask about a container, so (2) and (3) are
//! inexpressible in CSS until the stretch section and the gaps are real
//! boxes. They are real boxes in the tree — that decomposition is most of
//! what this module is for — but the *decision* still has to read a derived
//! number, and a derived number is a Rust expression. Being Rust also makes
//! it testable by sweeping a height across a boundary, which is the only way
//! to prove a threshold fires where REAPER's does.

use daw_theme_art::collapse::{
    BOTTOM_SECTION, FX_SECTION, INPUT_SECTION_FULL, INPUT_SECTION_MINIMAL, INPUT_SECTION_NO_FX,
    PAN_SECTION_COLLAPSED, PAN_SECTION_UNLABELLED,
};
pub use daw_theme_art::collapse::{Bands, REAPER, REAPER_BANDS, Thresholds};

/// Where the pan control lives at a given height.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanAnchor {
    /// Its own section, under the FX row.
    PanSection,
    /// Re-parented into the input area, because the pan section is gone.
    /// Record mode gives up its place to it.
    InputArea,
}

/// What the volume control is drawn as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VolumeWidget {
    /// A fader and its rail.
    Fader,
    /// A knob — below the swap threshold there is no room for travel.
    Knob,
}

/// The shape of the strip at one height.
///
/// Everything the strip renders differently is decided here, once, so the
/// markup asks a question rather than doing arithmetic in six places.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Collapse {
    pub show_input_fx: bool,
    pub show_record_input: bool,
    pub show_pan_labels: bool,
    pub show_volume_label: bool,
    pub show_io: bool,
    pub show_envelope: bool,
    pub show_phase: bool,
    pub show_record_mode: bool,
    pub pan: PanAnchor,
    pub volume: VolumeWidget,
    /// Vertical padding between the stacked controls, in px.
    pub padding: f32,
    /// The pan section's own height — 6, 33 or 50, never anything
    /// between. `rtconfig` writes it as a three-way choice and the stretch
    /// section absorbs every other pixel.
    pub pan_band: f32,
    /// The input section's, the same way: 22, 42 or 54.
    pub input_band: f32,
    /// The height left for the fader and meter after the fixed bands.
    pub stretch: f32,
}

impl Collapse {
    /// Resolve the strip's shape at `height`, against REAPER's thresholds.
    pub fn at(height: f32) -> Self {
        Self::with(height, REAPER)
    }

    /// The same, against an arbitrary set — for tests that sweep a boundary
    /// and for a scale other than 1.
    pub fn with(height: f32, t: Thresholds) -> Self {
        let show_pan_labels = height >= t.pan_labels;
        let show_pan_section = height >= t.pan_section;
        let show_record_input = height >= t.record_input;
        let show_input_fx = height >= t.input_fx;

        // The stretch section is a *residual*: what the height has left
        // after the bands above and below it. Which is why the collapses
        // that key off it cannot be written as a query on the strip.
        // 6 or 33, never 50. `rtconfig` writes the section as
        //
        //     !panLabelsMode (h<hide_pan_s ? 6 : 33) : 50
        //
        // and `panLabelsMode` is a *mode* the user turns on, not a height —
        // the 50 belongs to it and to nothing else. Reading it as "tall
        // strips get 50" made the band grow by seventeen rows at exactly
        // the heights where there is most room for it to look wrong, which
        // is why a tall strip was mostly colour.
        //
        // `show_pan_labels` still decides whether the word "pan" is
        // printed; that is `hide_pan_labels`, a different line.
        let pan_band = if show_pan_section {
            PAN_SECTION_UNLABELLED
        } else {
            PAN_SECTION_COLLAPSED
        };
        let input_band = if !show_record_input {
            INPUT_SECTION_MINIMAL
        } else if !show_input_fx {
            INPUT_SECTION_NO_FX
        } else {
            INPUT_SECTION_FULL
        };
        let stretch = (height - FX_SECTION - pan_band - input_band - BOTTOM_SECTION).max(0.0);

        Self {
            show_input_fx,
            show_record_input,
            show_pan_labels,
            show_volume_label: height >= t.volume_label,
            show_io: stretch >= t.io,
            show_envelope: stretch >= t.envelope,
            show_phase: stretch >= t.phase,
            // The other half of the re-anchor: when pan moves into the
            // input area it takes record mode's place, so record mode goes.
            show_record_mode: show_pan_section,
            pan_band,
            input_band,
            pan: if show_pan_section {
                PanAnchor::PanSection
            } else {
                PanAnchor::InputArea
            },
            // Asked of the fader area, which is what REAPER asks it of.
            volume: if daw_theme_art::collapse::fader_area(
                stretch,
                stretch >= t.io,
                stretch >= t.envelope,
                stretch >= t.phase,
            ) >= t.fader_swap
            {
                VolumeWidget::Fader
            } else {
                VolumeWidget::Knob
            },
            padding: if stretch < t.padding_steps.1 {
                2.0
            } else if stretch < t.padding_steps.0 {
                3.0
            } else {
                4.0
            },
            stretch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sweeping the boundary is the only honest test of a threshold: one
    /// sample on each side, and the claim is about where it *changes*.
    fn sweep(f: impl Fn(Collapse) -> bool, at: f32) {
        assert!(!f(Collapse::at(at - 1.0)), "still on below {at}");
        assert!(f(Collapse::at(at)), "not on at {at}");
    }

    #[test]
    fn the_container_thresholds_are_reapers() {
        sweep(|c| c.show_input_fx, REAPER.input_fx);
        sweep(|c| c.show_record_input, REAPER.record_input);
        sweep(|c| c.show_pan_labels, REAPER.pan_labels);
        sweep(|c| c.show_volume_label, REAPER.volume_label);
        sweep(|c| c.pan == PanAnchor::PanSection, REAPER.pan_section);
    }

    /// The fader becomes a knob rather than becoming a very short fader —
    /// and it is the *fader area* that decides, not the strip.
    ///
    /// Read as a strip height the threshold was 280, which drew a knob on a
    /// 228-row strip where REAPER draws a fader. The reference screenshot is
    /// a 228-row strip with a fader in it.
    #[test]
    fn the_volume_widget_switches_on_the_fader_area() {
        assert_eq!(Collapse::at(228.0).volume, VolumeWidget::Fader);

        // Walk down and find where it flips; the fader area either side of
        // that height must straddle REAPER's `min_vol_h`.
        let flip = (60..=400)
            .rev()
            .find(|h| Collapse::at(*h as f32).volume == VolumeWidget::Knob)
            .expect("the fader swaps somewhere");
        let area = |h: f32| {
            let c = Collapse::at(h);
            daw_theme_art::collapse::fader_area(c.stretch, c.show_io, c.show_envelope, c.show_phase)
        };
        assert!(
            area(flip as f32) < REAPER.fader_swap,
            "swapped with room to spare"
        );
        assert!(
            area(flip as f32 + 1.0) >= REAPER.fader_swap,
            "swapped a row late"
        );
    }

    /// The re-anchor: the pan section goes, the pan control does not, and
    /// record mode gives up its place to it.
    #[test]
    fn pan_re_anchors_into_the_input_area() {
        let tall = Collapse::at(REAPER.pan_section);
        assert_eq!(tall.pan, PanAnchor::PanSection);
        assert!(tall.show_record_mode);

        let short = Collapse::at(REAPER.pan_section - 1.0);
        assert_eq!(
            short.pan,
            PanAnchor::InputArea,
            "the pan control vanished with its section"
        );
        assert!(
            !short.show_record_mode,
            "record mode kept a place it had given away"
        );
    }

    /// The residual-driven collapses key off the stretch section, not the
    /// strip: at the same strip height a different set of bands above it
    /// gives a different answer, which is the whole reason they are
    /// separate.
    #[test]
    fn the_residual_collapses_key_off_the_stretch_section() {
        // Tall enough that the stretch section is generous.
        let tall = Collapse::at(600.0);
        assert!(tall.show_io && tall.show_envelope && tall.show_phase);

        // Walk down until the stretch section crosses each threshold and
        // check the button goes with it rather than with the strip.
        for h in (150..=600).rev() {
            let c = Collapse::at(h as f32);
            assert_eq!(c.show_io, c.stretch >= REAPER.io, "io disagreed at h={h}");
            assert_eq!(
                c.show_envelope,
                c.stretch >= REAPER.envelope,
                "envelope disagreed at h={h}"
            );
            assert_eq!(
                c.show_phase,
                c.stretch >= REAPER.phase,
                "phase disagreed at h={h}"
            );
        }
    }

    #[test]
    fn padding_steps_down_in_three_stages() {
        let stages: std::collections::BTreeSet<i32> = (120..=800)
            .map(|h| Collapse::at(h as f32).padding as i32)
            .collect();
        assert_eq!(
            stages,
            [2, 3, 4].into_iter().collect(),
            "padding does not step through its three stages"
        );
    }

    /// A strip cannot have a negative stretch section, however short it is.
    #[test]
    fn a_very_short_strip_still_resolves() {
        let c = Collapse::at(40.0);
        assert!(c.stretch >= 0.0);
        assert_eq!(c.volume, VolumeWidget::Knob);
        assert_eq!(c.pan, PanAnchor::InputArea);
    }
}
