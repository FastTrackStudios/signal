//! Coordinate mapping and hit-testing helpers for the EQ graph.

use super::eq_graph_model::{EqBand, EqBandShape};
use fts_audio_ui::axis::{DbAxis, FreqAxis};

#[derive(Clone, Copy, Debug)]
pub struct GraphMapper {
    pub min_freq: f64,
    pub max_freq: f64,
    pub db_range: f64,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
}

impl GraphMapper {
    #[must_use]
    pub const fn new(
        min_freq: f64,
        max_freq: f64,
        db_range: f64,
        width: f64,
        height: f64,
        padding: f64,
    ) -> Self {
        Self {
            min_freq,
            max_freq,
            db_range,
            width,
            height,
            padding,
        }
    }

    #[must_use]
    pub fn freq_to_x(&self, freq: f64) -> f64 {
        FreqAxis::new(self.min_freq, self.max_freq).freq_to_x(
            freq,
            self.padding,
            self.padding + self.width,
        )
    }

    #[must_use]
    pub fn x_to_freq(&self, x: f64) -> f64 {
        FreqAxis::new(self.min_freq, self.max_freq).x_to_freq(
            x,
            self.padding,
            self.padding + self.width,
        )
    }

    #[must_use]
    pub fn db_to_y(&self, db: f64) -> f64 {
        DbAxis::symmetric(self.db_range).db_to_y(db, self.padding, self.padding + self.height)
    }

    #[must_use]
    pub fn y_to_db(&self, y: f64) -> f64 {
        DbAxis::symmetric(self.db_range).y_to_db(y, self.padding, self.padding + self.height)
    }

    #[must_use]
    pub fn is_inside(&self, x: f64, y: f64) -> bool {
        x >= self.padding
            && x <= self.padding + self.width
            && y >= self.padding
            && y <= self.padding + self.height
    }
}

#[must_use]
pub fn filter_type_for_position(freq: f64, gain: f64, db_range: f64) -> EqBandShape {
    let gain_near_zero = gain.abs() < db_range * 0.2;
    let near_bottom = gain < -db_range * 0.82;

    if near_bottom {
        EqBandShape::Notch
    } else if freq < 30.0 && gain_near_zero {
        EqBandShape::LowCut
    } else if freq > 15000.0 && gain_near_zero {
        EqBandShape::HighCut
    } else if freq < 80.0 {
        EqBandShape::LowShelf
    } else if freq > 8000.0 {
        EqBandShape::HighShelf
    } else {
        EqBandShape::Bell
    }
}

#[must_use]
pub fn drag_gain_for_shape(shape: EqBandShape, current_gain: f32, pointer_gain: f64) -> f32 {
    if shape.uses_gain() {
        let max_gain = if matches!(
            shape,
            EqBandShape::LowShelf
                | EqBandShape::HighShelf
                | EqBandShape::TiltShelf
                | EqBandShape::FlatTilt
        ) {
            15.0
        } else {
            30.0
        };
        let next = pointer_gain.clamp(-max_gain, max_gain) as f32;
        if shape == EqBandShape::BandPass {
            current_gain
        } else {
            next
        }
    } else {
        0.0
    }
}

/// Scroll resonance, or the separate slope control when the model exposes one.
///
/// `fine` scales the resonance step (Shift fine-tune). It is applied as an
/// exponent rather than a factor because Q moves multiplicatively — a quarter
/// of a 1.15x step is 1.15^0.25, not 1.15/4, which would be a *reduction*.
/// Slope is a discrete step and ignores it.
pub fn wheel_band(band: &mut EqBand, delta_y: f64, slope_mode: bool, fine: f64) {
    if let Some(slope) = band.slope.as_mut()
        && (band.shape.uses_slope() || slope_mode)
    {
        let step = if delta_y < 0.0 { 1.0 } else { -1.0 };
        let minimum = if matches!(band.shape, EqBandShape::Bell | EqBandShape::Notch) {
            2.0
        } else {
            1.0
        };
        *slope = (*slope + step).clamp(minimum, 10.0);
    } else {
        let multiplier = if delta_y < 0.0 { 1.15_f64 } else { 0.87 }.powf(fine.max(0.01));
        band.q = (f64::from(band.q) * multiplier).clamp(0.1, 18.0) as f32;
    }
}

#[must_use]
pub fn nearest_band(
    bands: &[EqBand],
    mapper: GraphMapper,
    x: f64,
    y: f64,
    radius: f64,
) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, band) in bands.iter().enumerate() {
        if !band.used {
            continue;
        }
        let bx = mapper.freq_to_x(f64::from(band.frequency));
        let by = mapper.db_to_y(f64::from(band.gain));
        let d = (x - bx).hypot(y - by);
        if d < radius && best.is_none_or(|(_idx, dist)| d < dist) {
            best = Some((idx, d));
        }
    }
    best
}

#[must_use]
pub fn bands_in_rect(
    bands: &[EqBand],
    mapper: GraphMapper,
    sx: f64,
    sy: f64,
    x: f64,
    y: f64,
) -> Vec<usize> {
    let (mnx, mxx) = (sx.min(x), sx.max(x));
    let (mny, mxy) = (sy.min(y), sy.max(y));
    bands
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            if !b.used {
                return false;
            }
            let bx = mapper.freq_to_x(f64::from(b.frequency));
            let by = mapper.db_to_y(f64::from(b.gain));
            bx >= mnx && bx <= mxx && by >= mny && by <= mxy
        })
        .map(|(i, _)| i)
        .collect()
}

// ── Gesture resolution ──────────────────────────────────────────────────────
//
// Pro-Q 4's modifier set, resolved as pure functions so the meaning of a
// chord is testable without a pointer. The event handlers below stay
// dispatch-only: they read modifiers, ask here what the gesture means, and
// apply it. That split is what makes "Alt+Ctrl scroll adjusts gain and range
// together" a unit test rather than something you verify by hand in a DAW.
//
// `cmd` is Ctrl on Windows/Linux and Command on macOS — FabFilter's docs
// write it as "Ctrl (Command on macOS)", and the handlers OR the two so a
// single field carries it here.

/// The modifier keys held during a gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub alt: bool,
    pub shift: bool,
    /// Ctrl, or Command on macOS.
    pub cmd: bool,
}

impl Mods {
    #[must_use]
    pub const fn new(alt: bool, shift: bool, cmd: bool) -> Self {
        Self { alt, shift, cmd }
    }
}

/// What the scroll wheel drives over a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelTarget {
    /// Resonance — the unmodified default on a band with a bell shape.
    Q,
    /// Slope, for shapes where that is the meaningful width control.
    Slope,
    Gain,
    /// The dynamic range the band may travel.
    DynRange,
    /// Gain and range together, so the band's floor stays put while its
    /// ceiling moves.
    GainAndRange,
}

/// Resolve a scroll gesture.
///
/// Unmodified scroll follows the band: a cut filter's meaningful width is its
/// slope, everything else's is Q. The modifier layers on top of that.
#[must_use]
pub const fn wheel_target(mods: Mods, shape_uses_slope: bool) -> WheelTarget {
    match (mods.alt, mods.cmd) {
        (true, true) => WheelTarget::GainAndRange,
        (true, false) => WheelTarget::DynRange,
        (false, true) => WheelTarget::Gain,
        (false, false) => {
            if shape_uses_slope {
                WheelTarget::Slope
            } else {
                WheelTarget::Q
            }
        }
    }
}

/// What a click on a band's dot does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotAction {
    /// Select this band alone and begin dragging it.
    Select,
    /// Add to (or remove from) the current selection.
    AddToSelection,
    /// Extend the selection to cover everything between the anchor and here.
    RangeSelect,
    ToggleBypass,
    CycleShape,
    CycleSlope,
}

/// Resolve a click on a band dot.
///
/// Order matters, and the two-modifier chords have to be tested before their
/// single-modifier prefixes — otherwise Ctrl+Alt would be swallowed by the
/// bare-Alt arm and shape cycling would be unreachable.
#[must_use]
pub const fn dot_action(mods: Mods) -> DotAction {
    match (mods.alt, mods.shift, mods.cmd) {
        (true, false, true) => DotAction::CycleShape,
        (true, true, false) => DotAction::CycleSlope,
        (true, false, false) => DotAction::ToggleBypass,
        (false, _, true) => DotAction::AddToSelection,
        (false, true, false) => DotAction::RangeSelect,
        _ => DotAction::Select,
    }
}

/// The kind of band a create gesture produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateMode {
    Static,
    Dynamic,
    Spectral,
}

/// Resolve a create gesture on empty graph.
#[must_use]
pub const fn create_mode(mods: Mods) -> CreateMode {
    match (mods.alt, mods.shift) {
        (true, true) => CreateMode::Spectral,
        (true, false) => CreateMode::Dynamic,
        _ => CreateMode::Static,
    }
}

/// How a drag on a band is interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    /// Frequency and gain together.
    Free,
    /// Frequency only.
    FreqOnly,
    /// Gain only.
    GainOnly,
    /// Vertical movement drives resonance instead of gain.
    Resonance,
}

/// Resolve a drag.
///
/// Alt constrains to whichever axis the pointer has committed to — decided
/// once from the travel so far, rather than re-decided every frame, which is
/// what stops a constrained drag flipping axis mid-gesture. Below the
/// threshold the drag is still free, so a constrained drag that has not moved
/// yet does not lock to an arbitrary axis.
#[must_use]
pub fn drag_mode(mods: Mods, dx: f64, dy: f64) -> DragMode {
    if mods.cmd {
        return DragMode::Resonance;
    }
    if mods.alt {
        const COMMIT: f64 = 3.0;
        if dx.abs().max(dy.abs()) < COMMIT {
            return DragMode::Free;
        }
        return if dx.abs() >= dy.abs() {
            DragMode::FreqOnly
        } else {
            DragMode::GainOnly
        };
    }
    DragMode::Free
}

/// Shift is the fine-tune modifier everywhere: scroll steps and drag travel
/// are scaled by this.
#[must_use]
pub const fn fine_scale(mods: Mods) -> f64 {
    if mods.shift { 0.25 } else { 1.0 }
}

/// One scroll notch of dynamic range, as a fraction of the range parameter's
/// full 0..1 travel. The parameter spans −30..30 dB, so this is 1 dB a notch
/// before fine-tuning.
#[must_use]
pub fn dyn_range_step(delta_y: f64, mods: Mods) -> f64 {
    let dir = if delta_y < 0.0 { 1.0 } else { -1.0 };
    dir * (1.0 / 60.0) * fine_scale(mods)
}

/// One scroll notch of gain, in dB.
#[must_use]
pub fn gain_step(delta_y: f64, mods: Mods) -> f64 {
    let dir = if delta_y < 0.0 { 1.0 } else { -1.0 };
    dir * fine_scale(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapper() -> GraphMapper {
        GraphMapper::new(20.0, 20_000.0, 24.0, 800.0, 360.0, 0.0)
    }

    #[test]
    fn mapper_round_trips_frequency_and_db() {
        let m = mapper();
        let x = m.freq_to_x(1000.0);
        let freq = m.x_to_freq(x);
        assert!((freq - 1000.0).abs() < 1e-6);

        let y = m.db_to_y(6.0);
        let db = m.y_to_db(y);
        assert!((db - 6.0).abs() < 1e-9);
    }

    #[test]
    fn filter_type_uses_edges_and_gain_position() {
        assert_eq!(
            filter_type_for_position(25.0, 0.0, 24.0),
            EqBandShape::LowCut
        );
        assert_eq!(
            filter_type_for_position(18_000.0, 0.0, 24.0),
            EqBandShape::HighCut
        );
        assert_eq!(
            filter_type_for_position(60.0, 8.0, 24.0),
            EqBandShape::LowShelf
        );
        assert_eq!(
            filter_type_for_position(1000.0, 3.0, 24.0),
            EqBandShape::Bell
        );
        assert_eq!(
            filter_type_for_position(1000.0, -22.0, 24.0),
            EqBandShape::Notch
        );
    }

    #[test]
    fn drag_gain_respects_filter_shape() {
        assert_eq!(drag_gain_for_shape(EqBandShape::LowCut, 3.0, 12.0), 0.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::Notch, -4.0, 9.0), 0.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::LowShelf, 0.0, 24.0), 15.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::Bell, 0.0, 24.0), 24.0);
    }

    #[test]
    fn wheel_controls_keep_q_and_slope_independent() {
        let mut band = EqBand {
            q: 1.0,
            ..Default::default()
        };
        wheel_band(&mut band, -1.0, false, 1.0);
        assert!(band.q > 1.0);
        band.shape = EqBandShape::LowCut;
        band.slope = Some(2.0);
        let q = band.q;
        wheel_band(&mut band, -1.0, false, 1.0);
        assert_eq!(band.slope, Some(3.0));
        assert_eq!(band.q, q);
        band.shape = EqBandShape::Bell;
        wheel_band(&mut band, -1.0, true, 1.0);
        assert_eq!(band.slope, Some(4.0));
        assert_eq!(band.q, q);
    }

    #[test]
    fn nearest_band_and_rect_selection_ignore_unused_bands() {
        let m = mapper();
        let bands = vec![
            EqBand {
                used: true,
                enabled: true,
                frequency: 1000.0,
                gain: 0.0,
                ..Default::default()
            },
            EqBand {
                used: false,
                enabled: true,
                frequency: 1100.0,
                gain: 0.0,
                ..Default::default()
            },
        ];
        let x = m.freq_to_x(1000.0);
        let y = m.db_to_y(0.0);
        assert_eq!(nearest_band(&bands, m, x, y, 10.0).map(|(i, _)| i), Some(0));
        assert_eq!(
            bands_in_rect(&bands, m, x - 5.0, y - 5.0, x + 5.0, y + 5.0),
            vec![0]
        );
    }
}

/// The Pro-Q 4 modifier contract, pinned chord by chord.
///
/// Every case here is a line from FabFilter's own "Display and workflow" help
/// page. They are worth testing exactly because they are conventions rather
/// than derivations — nothing about the code says Alt+Ctrl scroll should move
/// gain and range together, so only a test keeps it that way.
#[cfg(test)]
mod gesture_tests {
    use super::{
        CreateMode, DotAction, DragMode, Mods, WheelTarget, create_mode, dot_action, drag_mode,
        dyn_range_step, fine_scale, gain_step, wheel_target,
    };

    const NONE: Mods = Mods::new(false, false, false);
    const ALT: Mods = Mods::new(true, false, false);
    const SHIFT: Mods = Mods::new(false, true, false);
    const CMD: Mods = Mods::new(false, false, true);
    const ALT_CMD: Mods = Mods::new(true, false, true);
    const ALT_SHIFT: Mods = Mods::new(true, true, false);

    #[test]
    fn scroll_follows_the_band_when_unmodified() {
        assert_eq!(wheel_target(NONE, false), WheelTarget::Q);
        assert_eq!(wheel_target(NONE, true), WheelTarget::Slope);
    }

    #[test]
    fn scroll_modifiers_match_the_manual() {
        assert_eq!(wheel_target(CMD, false), WheelTarget::Gain);
        assert_eq!(wheel_target(ALT, false), WheelTarget::DynRange);
        assert_eq!(wheel_target(ALT_CMD, false), WheelTarget::GainAndRange);
        // The modified meanings do not change with the band's shape — a cut
        // filter's gain is still gain.
        assert_eq!(wheel_target(CMD, true), WheelTarget::Gain);
        assert_eq!(wheel_target(ALT, true), WheelTarget::DynRange);
    }

    #[test]
    fn two_modifier_dot_chords_beat_their_prefixes() {
        // The regression this guards: matching bare Alt first makes Ctrl+Alt
        // and Alt+Shift unreachable, so shape and slope cycling silently
        // become "toggle bypass".
        assert_eq!(dot_action(ALT), DotAction::ToggleBypass);
        assert_eq!(dot_action(ALT_CMD), DotAction::CycleShape);
        assert_eq!(dot_action(ALT_SHIFT), DotAction::CycleSlope);
    }

    #[test]
    fn plain_and_selection_dot_clicks() {
        assert_eq!(dot_action(NONE), DotAction::Select);
        assert_eq!(dot_action(CMD), DotAction::AddToSelection);
        assert_eq!(dot_action(SHIFT), DotAction::RangeSelect);
    }

    #[test]
    fn creating_a_band_carries_its_mode() {
        assert_eq!(create_mode(NONE), CreateMode::Static);
        assert_eq!(create_mode(ALT), CreateMode::Dynamic);
        assert_eq!(create_mode(ALT_SHIFT), CreateMode::Spectral);
        // Shift alone is fine-tune, not a create mode.
        assert_eq!(create_mode(SHIFT), CreateMode::Static);
    }

    #[test]
    fn alt_drag_commits_to_the_dominant_axis() {
        // Below the commit threshold the drag stays free, so a constrained
        // drag that has barely moved does not lock to a coin-flip axis.
        assert_eq!(drag_mode(ALT, 1.0, 1.0), DragMode::Free);
        assert_eq!(drag_mode(ALT, 20.0, 2.0), DragMode::FreqOnly);
        assert_eq!(drag_mode(ALT, 2.0, 20.0), DragMode::GainOnly);
        // And it stays committed as the drag continues along that axis.
        assert_eq!(drag_mode(ALT, 40.0, 6.0), DragMode::FreqOnly);
    }

    #[test]
    fn cmd_drag_is_resonance_and_outranks_constraint() {
        assert_eq!(drag_mode(CMD, 0.0, 30.0), DragMode::Resonance);
        assert_eq!(drag_mode(ALT_CMD, 30.0, 0.0), DragMode::Resonance);
        assert_eq!(drag_mode(NONE, 30.0, 30.0), DragMode::Free);
    }

    #[test]
    fn shift_is_fine_tune_everywhere() {
        assert!((fine_scale(NONE) - 1.0).abs() < 1e-9);
        assert!((fine_scale(SHIFT) - 0.25).abs() < 1e-9);
        assert!((fine_scale(ALT_SHIFT) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn scroll_steps_go_up_when_the_wheel_goes_up() {
        // Negative delta is "wheel away from the user" in the DOM, which must
        // increase the value — getting this backwards is the classic inverted
        // -scroll bug.
        assert!(gain_step(-1.0, NONE) > 0.0);
        assert!(gain_step(1.0, NONE) < 0.0);
        assert!(dyn_range_step(-1.0, NONE) > 0.0);
        assert!(dyn_range_step(1.0, NONE) < 0.0);

        // One notch is 1 dB of gain, and 1 dB of a -30..30 range.
        assert!((gain_step(-1.0, NONE) - 1.0).abs() < 1e-9);
        assert!((dyn_range_step(-1.0, NONE) - 1.0 / 60.0).abs() < 1e-9);

        // Fine-tune quarters both.
        assert!((gain_step(-1.0, SHIFT) - 0.25).abs() < 1e-9);
    }
}
