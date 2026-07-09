//! Domain primitives — newtypes that make illegal states unrepresentable.
//!
//! These are the `Copy`, cheap, self-documenting types from
//! `docs/sampler-trait-design.md` §2. `U7`/`U14` enforce MIDI ranges at the
//! type level (their constructors clamp); the interned ids compare/hash by an
//! `Arc<str>` so authoring uses `&str` and prints by name.
//!
//! Where signal already has helpers we reuse them rather than duplicate:
//! note-name parsing lives in [`crate::midi`] and is re-exported here via the
//! [`Note::from_name`] convenience, so callers can write `Note::from_name("C4")`.

use std::sync::Arc;

// ── Bounded MIDI scalars ──────────────────────────────────────────────────

/// A 7-bit MIDI value, `0..=127`. The constructor clamps, so an out-of-range
/// `u8` can never be smuggled in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct U7(u8);

impl U7 {
    pub const MIN: U7 = U7(0);
    pub const MAX: U7 = U7(127);

    /// Clamp `v` into `0..=127`.
    #[inline]
    pub const fn new(v: u8) -> Self {
        U7(if v > 127 { 127 } else { v })
    }

    #[inline]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Normalize to `0.0..=1.0` (127 → 1.0). Handy for gain/curve math.
    #[inline]
    pub fn unit(self) -> f32 {
        self.0 as f32 / 127.0
    }
}

impl From<u8> for U7 {
    #[inline]
    fn from(v: u8) -> Self {
        U7::new(v)
    }
}

/// A 14-bit MIDI value, `0..=16383` — pitch bend. Centre is `8192`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct U14(u16);

impl U14 {
    pub const MIN: U14 = U14(0);
    pub const CENTER: U14 = U14(8192);
    pub const MAX: U14 = U14(16383);

    #[inline]
    pub const fn new(v: u16) -> Self {
        U14(if v > 16383 { 16383 } else { v })
    }

    #[inline]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Signed bipolar position `-1.0..=1.0`, centre = 0.0.
    #[inline]
    pub fn bipolar(self) -> f32 {
        (self.0 as f32 - 8192.0) / 8192.0
    }
}

impl Default for U14 {
    #[inline]
    fn default() -> Self {
        U14::CENTER
    }
}

impl From<u16> for U14 {
    #[inline]
    fn from(v: u16) -> Self {
        U14::new(v)
    }
}

// ── MIDI domain newtypes ──────────────────────────────────────────────────

/// A MIDI key number, `0..=127`. Comparable/orderable so key ranges are
/// trivial. Reuses [`crate::midi`] for name parsing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Note(pub u8);

impl Note {
    pub const MIDDLE_C: Note = Note(60);

    #[inline]
    pub const fn new(v: u8) -> Self {
        Note(if v > 127 { 127 } else { v })
    }

    #[inline]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Parse scientific pitch notation (`"C4"`, `"C#-1"`) via [`crate::midi`].
    pub fn from_name(name: &str) -> Result<Self, crate::SamplerError> {
        crate::midi::note_name_to_midi(name).map(Note)
    }

    /// Semitone interval to another note (signed, `to - self`).
    #[inline]
    pub fn interval_to(self, to: Note) -> Interval {
        Interval(to.0 as i32 - self.0 as i32)
    }
}

impl From<u8> for Note {
    #[inline]
    fn from(v: u8) -> Self {
        Note::new(v)
    }
}

/// Note-on velocity, `0..=127` (0 = note-off by convention).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Velocity(pub U7);

impl Velocity {
    #[inline]
    pub const fn new(v: u8) -> Self {
        Velocity(U7::new(v))
    }
    #[inline]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
    #[inline]
    pub fn unit(self) -> f32 {
        self.0.unit()
    }
}

impl From<u8> for Velocity {
    #[inline]
    fn from(v: u8) -> Self {
        Velocity::new(v)
    }
}

/// A MIDI continuous-controller *number*, `0..=127` (e.g. `Cc::new(1)` =
/// mod wheel, `Cc::new(64)` = sustain pedal).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Cc(pub U7);

impl Cc {
    pub const MOD_WHEEL: Cc = Cc(U7::new(1));
    pub const SUSTAIN: Cc = Cc(U7::new(64));

    #[inline]
    pub const fn new(n: u8) -> Self {
        Cc(U7::new(n))
    }
    #[inline]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl From<u8> for Cc {
    #[inline]
    fn from(v: u8) -> Self {
        Cc::new(v)
    }
}

/// A MIDI channel, `0..=15`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct MidiCh(u8);

impl MidiCh {
    #[inline]
    pub const fn new(v: u8) -> Self {
        MidiCh(if v > 15 { 15 } else { v })
    }
    #[inline]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl From<u8> for MidiCh {
    #[inline]
    fn from(v: u8) -> Self {
        MidiCh::new(v)
    }
}

// ── Continuous quantities ─────────────────────────────────────────────────

/// Gain in decibels (`0.0` = unity).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Db(pub f32);

impl Db {
    pub const UNITY: Db = Db(0.0);

    /// Linear amplitude factor (`10^(dB/20)`).
    #[inline]
    pub fn linear(self) -> f32 {
        10f32.powf(self.0 / 20.0)
    }
}

/// Pitch offset in cents (`100` cents = 1 semitone).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Cents(pub f32);

/// A duration in seconds (`f64` so sub-millisecond legato timings stay exact).
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default)]
pub struct Seconds(pub f64);

impl Seconds {
    /// Construct from milliseconds — the unit the design doc's `ms(120)`
    /// breakpoints are written in.
    #[inline]
    pub fn from_ms(ms: f64) -> Self {
        Seconds(ms / 1000.0)
    }
    #[inline]
    pub fn as_ms(self) -> f64 {
        self.0 * 1000.0
    }
    /// Convert to a frame count at `sample_rate`.
    #[inline]
    pub fn to_frames(self, sample_rate: u32) -> Frames {
        Frames((self.0 * sample_rate as f64).round() as u64)
    }
}

/// A sample-frame count (one frame = one sample per channel).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Frames(pub u64);

/// A signed semitone interval between two notes (legato leap size).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Interval(pub i32);

impl Interval {
    #[inline]
    pub const fn semitones(self) -> i32 {
        self.0
    }
    #[inline]
    pub const fn abs_semitones(self) -> u32 {
        self.0.unsigned_abs()
    }
}

// ── Axes ──────────────────────────────────────────────────────────────────

/// A continuous performance axis that selects/crossfades zones. Zones declare
/// which axis drives them rather than hardcoding "CC1".
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Axis {
    Velocity,
    Cc(Cc),
    ChannelPressure,
    Custom(Interned),
}

// ── Interned, Copy-ish ids ────────────────────────────────────────────────

/// A cheaply-clonable interned string. Phase A uses an `Arc<str>` (clone is a
/// refcount bump); equality/hash are by string contents. A future phase can
/// swap this for an index into a global intern table without touching callers.
#[derive(Clone, Debug)]
pub struct Interned(Arc<str>);

impl Interned {
    pub fn new(s: impl AsRef<str>) -> Self {
        Interned(Arc::from(s.as_ref()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for Interned {
    fn eq(&self, other: &Self) -> bool {
        // Pointer-equal fast path, falling back to contents.
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}
impl Eq for Interned {}

impl std::hash::Hash for Interned {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl std::fmt::Display for Interned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Interned {
    fn from(s: &str) -> Self {
        Interned::new(s)
    }
}
impl From<String> for Interned {
    fn from(s: String) -> Self {
        Interned::new(s)
    }
}

/// Generate an interned id newtype with `new`/`as_str` and `&str`/`String`
/// conversions. These are `Clone` (Arc bump), `Eq`, `Hash` — "Copy-ish".
macro_rules! interned_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(pub Interned);

        impl $name {
            #[inline]
            pub fn new(s: impl AsRef<str>) -> Self {
                $name(Interned::new(s))
            }
            #[inline]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
        impl From<&str> for $name {
            fn from(s: &str) -> Self { $name::new(s) }
        }
        impl From<String> for $name {
            fn from(s: String) -> Self { $name::new(s) }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

interned_id!(
    /// A mic position id — `"Close"`, `"Room"`, `"Decca Tree"`.
    MicId
);
interned_id!(
    /// A playing-technique id — `"Sustain"`, `"Staccato"`, `"Legato"`.
    ArticulationId
);
interned_id!(
    /// A functional group / variation id.
    GroupId
);
interned_id!(
    /// An instrument id.
    InstrumentId
);

/// A zone id — a plain `u32` index (zones are anonymous, addressed positionally).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ZoneId(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u7_clamps() {
        assert_eq!(U7::new(200).get(), 127);
        assert_eq!(U7::new(50).get(), 50);
        assert!((U7::new(127).unit() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn u14_bipolar_centre_is_zero() {
        assert!(U14::CENTER.bipolar().abs() < 1e-6);
        assert!(U14::new(20000).get() == 16383);
    }

    #[test]
    fn note_from_name_reuses_midi() {
        assert_eq!(Note::from_name("C4").unwrap(), Note(60));
        assert_eq!(Note(60).interval_to(Note(64)), Interval(4));
    }

    #[test]
    fn interned_eq_by_contents() {
        let a = MicId::new("Close");
        let b = MicId::from("Close");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "Close");
    }

    #[test]
    fn seconds_ms_roundtrip() {
        assert!((Seconds::from_ms(120.0).as_ms() - 120.0).abs() < 1e-9);
        assert_eq!(Seconds(1.0).to_frames(48_000), Frames(48_000));
    }
}
