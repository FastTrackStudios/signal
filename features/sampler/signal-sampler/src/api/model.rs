//! The declarative instrument model — `docs/sampler-trait-design.md` §3 + §5.
//!
//! The library *is* data: a tree of articulations → variations → groups →
//! zones, plus the axes that drive it. Phase A defines these as data-only
//! structs (the engine reads them in later phases). The one piece implemented
//! "for real" here is [`VelCurve`] — the velocity-/interval-dependent timing
//! curve that the legato design hinges on (§5 "Legato, in depth").

use std::ops::RangeInclusive;

use super::prim::{
    ArticulationId, Axis, Cents, Db, GroupId, InstrumentId, Interval, MicId, Note, Seconds,
    Velocity, ZoneId,
};

// ── Lerp — linear interpolation for curve values ─────────────────────────

/// Linear interpolation between two values of the same type at `t ∈ 0.0..=1.0`.
/// Implemented for the curve value types ([`Seconds`], [`f32`]) so [`VelCurve`]
/// can interpolate breakpoints generically.
pub trait Lerp {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    #[inline]
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl Lerp for Seconds {
    #[inline]
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        Seconds(a.0 + (b.0 - a.0) * t as f64)
    }
}

impl Lerp for Cents {
    #[inline]
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        Cents(a.0 + (b.0 - a.0) * t)
    }
}

impl Lerp for Db {
    #[inline]
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        Db(a.0 + (b.0 - a.0) * t)
    }
}

// ── VelCurve — the marquee timing type ───────────────────────────────────

/// How a [`VelCurve`] derives its base value from velocity.
enum Shape<T> {
    /// One value for every velocity.
    Constant(T),
    /// Breakpoints sorted ascending by velocity; sampling lerps between the
    /// two bracketing points and clamps outside the range.
    Breakpoints(Vec<(Velocity, T)>),
    /// An arbitrary closure `Velocity -> T`.
    Fn(Box<dyn Fn(Velocity) -> T + Send + Sync>),
}

/// A velocity → value curve, `O(1)`-ish to sample on the audio thread, with an
/// optional interval (leap-size) scale factor. THIS is the type that lets
/// "different velocities have different portamento / transition times" — see
/// the CSS legato example in the design doc §5.
///
/// Build it from a constant, breakpoints, or a closure, then optionally
/// [`scaled_by_interval`](VelCurve::scaled_by_interval) so wider leaps glide
/// longer. Sample with [`at`](VelCurve::at) on the audio thread.
pub struct VelCurve<T> {
    shape: Shape<T>,
    /// `Some(f)` multiplies the base value by `f(interval)`.
    interval_scale: Option<Box<dyn Fn(Interval) -> f32 + Send + Sync>>,
}

impl<T: Lerp + Copy> VelCurve<T> {
    /// One value regardless of velocity (e.g. a fixed crossfade time).
    pub fn constant(v: T) -> Self {
        VelCurve {
            shape: Shape::Constant(v),
            interval_scale: None,
        }
    }

    /// Breakpoints `(velocity, value)`; the curve lerps between adjacent points
    /// and clamps to the endpoints outside the declared range. Input order does
    /// not matter — the points are sorted by velocity.
    pub fn breakpoints(points: impl IntoIterator<Item = (Velocity, T)>) -> Self {
        let mut pts: Vec<(Velocity, T)> = points.into_iter().collect();
        pts.sort_by_key(|(v, _)| v.get());
        VelCurve {
            shape: Shape::Breakpoints(pts),
            interval_scale: None,
        }
    }

    /// An arbitrary `Velocity -> T` closure.
    pub fn from_fn(f: impl Fn(Velocity) -> T + Send + Sync + 'static) -> Self {
        VelCurve {
            shape: Shape::Fn(Box::new(f)),
            interval_scale: None,
        }
    }

    /// Scale the curve's base value by `f(interval)` — e.g.
    /// `|i| 1.0 + i.semitones() as f32 * 0.012` so wide leaps glide longer.
    /// Requires `T` to support scalar multiplication via [`Lerp`] against a
    /// zero (we scale by interpolating `zero..base`), which works for the
    /// numeric value types used here.
    pub fn scaled_by_interval(
        mut self,
        f: impl Fn(Interval) -> f32 + Send + Sync + 'static,
    ) -> Self {
        self.interval_scale = Some(Box::new(f));
        self
    }

    /// Sample the curve at `vel`, applying the interval scale. Realtime-safe
    /// (no allocation); `breakpoints` is a small linear scan.
    pub fn at(&self, vel: Velocity, interval: Interval) -> T
    where
        T: Scale,
    {
        let base = match &self.shape {
            Shape::Constant(v) => *v,
            Shape::Fn(f) => f(vel),
            Shape::Breakpoints(pts) => Self::sample_breakpoints(pts, vel),
        };
        match &self.interval_scale {
            Some(f) => base.scale(f(interval)),
            None => base,
        }
    }

    fn sample_breakpoints(pts: &[(Velocity, T)], vel: Velocity) -> T {
        match pts {
            [] => panic!("VelCurve::breakpoints requires at least one point"),
            [(_, only)] => *only,
            _ => {
                let v = vel.get();
                // Below the first point → clamp to first.
                if v <= pts[0].0.get() {
                    return pts[0].1;
                }
                // Above the last point → clamp to last.
                let last = pts.len() - 1;
                if v >= pts[last].0.get() {
                    return pts[last].1;
                }
                // Find the bracketing segment and lerp.
                for w in pts.windows(2) {
                    let (lo_v, lo_val) = w[0];
                    let (hi_v, hi_val) = w[1];
                    if v >= lo_v.get() && v <= hi_v.get() {
                        let span = (hi_v.get() - lo_v.get()) as f32;
                        let t = if span > 0.0 {
                            (v - lo_v.get()) as f32 / span
                        } else {
                            0.0
                        };
                        return T::lerp(lo_val, hi_val, t);
                    }
                }
                pts[last].1
            }
        }
    }
}

/// Scalar multiply for [`VelCurve::scaled_by_interval`]. Separate from [`Lerp`]
/// so only the genuinely-numeric value types opt in.
pub trait Scale: Copy {
    fn scale(self, factor: f32) -> Self;
}
impl Scale for f32 {
    #[inline]
    fn scale(self, factor: f32) -> Self {
        self * factor
    }
}
impl Scale for Seconds {
    #[inline]
    fn scale(self, factor: f32) -> Self {
        Seconds(self.0 * factor as f64)
    }
}
impl Scale for Cents {
    #[inline]
    fn scale(self, factor: f32) -> Self {
        Cents(self.0 * factor)
    }
}
impl Scale for Db {
    #[inline]
    fn scale(self, factor: f32) -> Self {
        Db(self.0 * factor)
    }
}

// ── The instrument tree ──────────────────────────────────────────────────

/// The whole virtual instrument: a tree of articulations, an output set of
/// mics, the axes that drive it, and its performance config. Pure data.
pub struct InstrumentModel {
    pub id: InstrumentId,
    pub mics: Vec<Mic>,
    /// e.g. `Cc(1)` — crossfades dynamic layers. `None` = velocity-switched.
    pub dynamics: Option<Axis>,
    pub articulations: Vec<Articulation>,
    pub sample_rate: u32,
}

/// A mic position — its id, default mix strip, and RAM residency.
pub struct Mic {
    pub id: MicId,
    pub gain: Db,
    pub pan: f32,
    /// `false` = purged to save RAM (load/unload to trade RAM for flexibility).
    pub loaded: bool,
}

/// A playing technique, selected by keyswitch / CC / program.
pub struct Articulation {
    pub id: ArticulationId,
    pub select: Select,
    pub variations: Vec<Variation>,
    /// `None` = polyphonic; `Some` = monophonic with note→note transitions.
    pub legato: Option<Legato>,
}

/// An alternate take-set within an articulation.
pub struct Variation {
    pub id: GroupId,
    pub select: Select,
    pub groups: Vec<Group>,
}

/// The functional leaf bundle: zones that share polyphony / choke / trigger /
/// round-robin behavior.
pub struct Group {
    pub id: GroupId,
    pub trigger: Trigger,
    pub polyphony: Polyphony,
    /// hi-hat open choked by closed, etc.
    pub choke: Option<GroupId>,
    pub round_robin: RoundRobin,
    pub zones: Vec<Zone>,
}

/// One multi-layer zone: a key×velocity rectangle that already contains its
/// round-robins, mics, and dynamic layers (the KODA headline).
pub struct Zone {
    pub id: ZoneId,
    pub keys: RangeInclusive<Note>,
    pub vel: RangeInclusive<Velocity>,
    pub root: Note,
    pub tune: Cents,
    pub gain: Db,
    pub pan: f32,
    /// Layers indexed by (mic, dynamic, rr) → a sample slice.
    pub layers: Box<dyn ZoneLayers>,
}

/// A declared dynamic-crossfade layer (low→high), e.g. p / mf / f.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DynLayer(pub u16);

/// A reference to a resolved sample slice. Phase A keeps this opaque — the
/// engine owns the real sample bytes via [`crate::SampleMap`]; this is a thin
/// addressing handle so the model surface compiles. Phase B fills it in with
/// the daw `AudioSource` slice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SampleSlice {
    /// Opaque index into the owning library's sample pool.
    pub index: u32,
}

/// Resolve a sample for a (mic, dynamic, round-robin) coordinate. The
/// convention vs zone-mode distinction lives behind this — both implement the
/// same `sample`.
pub trait ZoneLayers: Send + Sync {
    fn sample(&self, mic: MicId, dynamic: DynLayer, rr: u32) -> Option<SampleSlice>;
    /// Declared crossfade layers, low→high.
    fn dynamics(&self) -> &[DynLayer];
    fn round_robins(&self) -> u32;
}

/// What selects an articulation / variation.
#[derive(Clone, Debug)]
pub enum Select {
    /// Keyswitch note (e.g. C-1 selects Sustain).
    Keyswitch(Note),
    /// A CC value range selects this (e.g. CC58 0..=42).
    Cc(super::prim::Cc, RangeInclusive<u8>),
    /// Always active (the default / only articulation).
    Always,
}

impl Select {
    pub fn keyswitch(note: Note) -> Self {
        Select::Keyswitch(note)
    }
}

/// What fires a group's voices.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    NoteOn,
    Release,
    Legato,
}

/// Per-group voice limit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Polyphony {
    Unlimited,
    Voices(u16),
    Mono,
}

/// Round-robin cycling policy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundRobin {
    Off,
    Cycle,
    Random,
}

// ── Legato (design doc §5) ───────────────────────────────────────────────

/// Everything about an articulation's legato. Every timing is a [`VelCurve`],
/// not an `f32` — that's what makes per-velocity portamento/transition timing
/// possible (the CSS requirement).
pub struct Legato {
    /// Recorded note→note transition samples. `None` = synthetic pitch-glide
    /// only (no recorded bow/breath transitions).
    pub transitions: Option<Box<dyn TransitionLayers>>,
    /// Delay before the *target* note speaks — CSS "delayed legato".
    pub pre_delay: VelCurve<Seconds>,
    /// Pitch-glide time, by velocity, optionally interval-scaled.
    pub portamento: VelCurve<Seconds>,
    /// Crossfade from the previous note's tail into the transition/target.
    pub crossfade: VelCurve<Seconds>,
    pub mode: LegatoMode,
}

/// Legato voicing mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LegatoMode {
    Mono,
    PolyLegato,
}

/// Direction of a legato move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    pub fn of(interval: Interval) -> Self {
        if interval.semitones() >= 0 {
            Direction::Up
        } else {
            Direction::Down
        }
    }
}

/// A distinct transition velocity layer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VelLayer(pub u16);

/// Pick the transition sample for one legato move — the role [`ZoneLayers`]
/// plays for sustains, but over the *move* (from→to) rather than the held note.
pub trait TransitionLayers: Send + Sync {
    fn sample(
        &self,
        from: Note,
        to: Note,
        vel: Velocity,
        dir: Direction,
        mic: MicId,
    ) -> Option<SampleSlice>;
    /// Distinct transition recordings.
    fn velocity_layers(&self) -> &[VelLayer];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::prim::Cc;

    fn ms(v: f64) -> Seconds {
        Seconds::from_ms(v)
    }

    #[test]
    fn velcurve_constant() {
        let c = VelCurve::constant(ms(40.0));
        assert_eq!(c.at(Velocity::new(20), Interval(0)), ms(40.0));
        assert_eq!(c.at(Velocity::new(120), Interval(12)), ms(40.0));
    }

    #[test]
    fn velcurve_breakpoint_interpolation() {
        // CSS pre-delay: soft (vel 20) → 120ms, hard (vel 110) → 20ms.
        let c = VelCurve::breakpoints([
            (Velocity::new(20), ms(120.0)),
            (Velocity::new(110), ms(20.0)),
        ]);
        // Endpoints clamp.
        assert_eq!(c.at(Velocity::new(10), Interval(0)), ms(120.0));
        assert_eq!(c.at(Velocity::new(127), Interval(0)), ms(20.0));
        // Midpoint (vel 65) interpolates to 70ms.
        let mid = c.at(Velocity::new(65), Interval(0));
        assert!((mid.as_ms() - 70.0).abs() < 0.5, "mid was {}", mid.as_ms());
    }

    #[test]
    fn velcurve_unsorted_input_sorts() {
        let c = VelCurve::breakpoints([
            (Velocity::new(110), ms(20.0)),
            (Velocity::new(20), ms(120.0)),
        ]);
        assert_eq!(c.at(Velocity::new(10), Interval(0)), ms(120.0));
        assert_eq!(c.at(Velocity::new(127), Interval(0)), ms(20.0));
    }

    #[test]
    fn velcurve_from_fn() {
        let c = VelCurve::from_fn(|v| ms(v.get() as f64));
        assert_eq!(c.at(Velocity::new(50), Interval(0)), ms(50.0));
    }

    #[test]
    fn velcurve_interval_scale_widens() {
        // Wider leaps glide longer: base 100ms scaled by 1 + semis*0.1.
        let c = VelCurve::constant(ms(100.0))
            .scaled_by_interval(|i| 1.0 + i.abs_semitones() as f32 * 0.1);
        let step = c.at(Velocity::new(64), Interval(1)); // 1.1x → 110ms
        let leap = c.at(Velocity::new(64), Interval(12)); // 2.2x → 220ms
        assert!(leap.as_ms() > step.as_ms());
        assert!((step.as_ms() - 110.0).abs() < 0.5);
        assert!((leap.as_ms() - 220.0).abs() < 0.5);
    }

    #[test]
    fn dynamics_axis_is_addressable() {
        let m = InstrumentModel {
            id: InstrumentId::new("test"),
            mics: vec![],
            dynamics: Some(Axis::Cc(Cc::new(1))),
            articulations: vec![],
            sample_rate: 48_000,
        };
        assert!(matches!(m.dynamics, Some(Axis::Cc(_))));
    }
}
