//! **Piano voice** — the NI Essential Pianos' Color and Dynamic Range controls,
//! as velocity-domain transforms over the sampler.
//!
//! Spec: `features/rigs/keys/spec/piano-voice.md` (`r[keys.piano.*]`), recovered
//! from the shipped KSP. The short version:
//!
//! - **Color** (−50…+50) is *not* a filter. It offsets the incoming velocity,
//!   so a different recorded velocity layer plays — a genuinely harder or
//!   softer hammer strike — and a compensating gain keeps the level where the
//!   player put it. Timbre moves; loudness does not.
//! - **Dynamic Range** (−200…+200) compresses or expands using a
//!   velocity-derived gain, without ever narrowing the set of samples in play.
//!
//! Both produce `(shifted velocity, gain trim)`. The shifted velocity then
//! drives zone selection, the per-velocity volume table *and* the low-pass
//! cutoff — see `r[keys.piano.color.three-effects]`; doing only the first is
//! the mistake that sounds nearly right.
//!
//! ## Units
//!
//! The KSP arithmetic is integer and its volumes are Kontakt **millidecibels**
//! (`change_vol` units). Everything here keeps the integer math verbatim so it
//! matches the plugin bit-for-bit, then converts once at the boundary — a
//! factor-of-1000 slip is the easiest possible bug here and the loudest.

/// Per-instrument constants from the shared `NI ESSENTIAL PIANOS` script's
/// `on init`, selected there by which sample group is present.
///
/// These are **not** uniform across the four pianos, which is why this is a
/// value rather than a set of constants: The Maverick biases Color by −5 while
/// the others do not. See `r[keys.piano.color.per-instrument-offset]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PianoOffsets {
    /// `$COLOR_OFFSET` — added to the Color knob before anything else.
    pub color: i32,
    /// `$DYN_OFFSET` — added to the Dynamic Range knob to form the helper.
    pub dynamics: i32,
    /// `$KK_DYN_OFFSET` — a further addend, applied only in velocity mode 4
    /// (which is the shipped default).
    pub kk_dynamics: i32,
    /// Whose release script levels the release samples
    /// (see [`crate::piano_release`]).
    pub piano: crate::piano_release::NiPiano,
}

impl PianoOffsets {
    /// The Grandeur.
    pub const GRANDEUR: Self = Self {
        color: 0,
        dynamics: -55,
        kk_dynamics: -25,
        piano: crate::piano_release::NiPiano::Grandeur,
    };
    /// The Maverick — note the biased Color.
    pub const MAVERICK: Self = Self {
        color: -5,
        dynamics: -50,
        kk_dynamics: -25,
        piano: crate::piano_release::NiPiano::Maverick,
    };
    /// The Gentleman.
    pub const GENTLEMAN: Self = Self {
        color: 0,
        dynamics: -50,
        kk_dynamics: -10,
        piano: crate::piano_release::NiPiano::Gentleman,
    };
    /// The Giant, which ships its own script with no offset term at all.
    pub const GIANT: Self = Self {
        color: 0,
        dynamics: 0,
        kk_dynamics: 0,
        piano: crate::piano_release::NiPiano::Giant,
    };

    /// Offsets for a library by name, matching the pack stems
    /// `build_ni_packs` writes (`"The Grandeur - Piano"` → Grandeur).
    ///
    /// Returns `None` for anything that is not one of the four: a pack this
    /// does not recognise should play flat, not borrow another piano's bias.
    #[must_use]
    pub fn for_library(name: &str) -> Option<Self> {
        let n = name.to_ascii_lowercase();
        if n.contains("grandeur") {
            Some(Self::GRANDEUR)
        } else if n.contains("maverick") {
            Some(Self::MAVERICK)
        } else if n.contains("gentleman") {
            Some(Self::GENTLEMAN)
        } else if n.contains("giant") {
            Some(Self::GIANT)
        } else {
            None
        }
    }
}

/// Color / Dynamic Range state for one piano lane.
///
/// Defaults are the instrument's shipped state (`persistent_0.tsv`): both
/// controls at 0, which makes [`PianoVoice::apply`] an exact identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoVoice {
    /// `$Mas_sliToneColor`, −50…+50. 0 = as played.
    pub color: i32,
    /// `$Mas_sliAnaDyn`, −200…+200. Negative compresses, positive expands.
    pub dynamic_range: i32,
    /// `$Ana_mnuVelo == 4` — the shipped default, and the only mode that
    /// brings `kk_dynamics` into the helper.
    pub velo_mode_4: bool,
    /// `$Mas_sliAnaReso` normalised to 0..1 — the level of the sympathetic
    /// string-resonance group, which sounds **only while the sustain pedal is
    /// down** (`r[keys.piano.resonance.pedal-gate]`).
    ///
    /// Shipped default 0.63 (`persistent_0.tsv: $Mas_sliAnaReso 630000`, on
    /// Kontakt's 0..1e6 scale). At 0 the group is silent and need not be
    /// loaded at all — which is the point of shipping it as its own pack.
    pub resonance: f32,
    pub offsets: PianoOffsets,
    /// Which saved state of the instrument's controls this plays with — its
    /// laws are tabulated at it, and the knobs here are changes from it.
    pub snapshot: crate::piano_release::Snapshot,
}

impl Default for PianoVoice {
    fn default() -> Self {
        Self {
            color: 0,
            dynamic_range: 0,
            velo_mode_4: true,
            resonance: 0.63,
            offsets: PianoOffsets::GRANDEUR,
            snapshot: crate::piano_release::Snapshot::Factory,
        }
    }
}

/// What a note-on gets after the piano controls have had their say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceShift {
    /// Velocity to select zones / dynamics / filter cutoff with.
    pub velocity: u8,
    /// Gain trim for this voice, in **decibels**.
    pub trim_db: f32,
}

impl PianoVoice {
    #[must_use]
    pub fn new(offsets: PianoOffsets) -> Self {
        Self {
            offsets,
            ..Self::default()
        }
    }

    /// Whether this leaves every note untouched — the fast path, and the
    /// shipped default.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.color + self.offsets.color == 0 && self.dynamic_range == 0
    }

    /// Effective Color: the knob plus the instrument's bias, clamped to the
    /// knob's own range.
    ///
    /// **Deliberate divergence.** The KSP does not clamp: it `select`s on
    /// `color + COLOR_OFFSET` with arms covering only `-50..=-1` and `1..=50`,
    /// so on The Maverick (offset −5) a knob at −50 yields −55, matches no arm,
    /// and leaves `$ColourVolumeBoost` at whatever the *previous* note set it
    /// to. That is a stale-state bug, audible as a note whose level depends on
    /// the one before it. Clamping is what the code plainly means.
    fn effective_color(&self) -> i32 {
        (self.color + self.offsets.color).clamp(-50, 50)
    }

    /// Whether an articulation id names a sympathetic-resonance group.
    ///
    /// These are the group names `build_ni_packs` writes from the KSP's own
    /// twelve-group table: `Resonance` (the pedal-down string bed) and `SSR`
    /// (sympathetic string resonance / overtones). Both are pedal-gated and
    /// both ride the Resonances control.
    #[must_use]
    pub fn is_resonance_group(articulation: &str) -> bool {
        articulation.eq_ignore_ascii_case("Resonance") || articulation.eq_ignore_ascii_case("SSR")
    }

    /// Gain for a resonance voice, or `None` if it must not sound.
    ///
    /// `r[keys.piano.resonance.pedal-gate]`: silent unless the pedal is down
    /// AND the control is above zero — the KSP's
    /// `if ($Mas_sliAnaReso > 0 and $PedalDown = 1)`.
    #[must_use]
    pub fn resonance_gain_db(&self, pedal_held: bool) -> Option<f32> {
        if !pedal_held || self.resonance <= 0.0 {
            return None;
        }
        // Kontakt's volume scale is not linear in the slider; 0..1 maps onto
        // -inf..0 dB as a straight amplitude ratio here, which keeps the
        // control's *shape* without pretending to Kontakt's exact taper. The
        // taper is a K6 A/B question, not something to guess at.
        Some(20.0 * self.resonance.clamp(0.0, 1.0).log10())
    }

    /// The KSP's Dynamic Range gain, in millidecibels — the literal law,
    /// offsets and all.
    ///
    /// Note this is **non-zero at a knob of 0**: the helper is
    /// `0 + DYN_OFFSET + KK_DYN_OFFSET` = −80 on The Grandeur, so a note at
    /// velocity 64 already carries +5.04 dB. That is the instrument's baseline
    /// voicing, not an effect of the control, and it only makes sense
    /// alongside Kontakt's `%VolumeTabelle` — which is why [`Self::apply`]
    /// uses the *delta* instead. Kept public because A/B-ing against real
    /// Kontakt needs the absolute figure.
    #[must_use]
    pub fn dynamic_mdb_absolute(&self, velocity: u8, knob: i32) -> i32 {
        let vel = velocity.clamp(1, 127) as i32;
        let helper = knob
            + self.offsets.dynamics
            + if self.velo_mode_4 {
                self.offsets.kk_dynamics
            } else {
                0
            };
        match knob {
            d if d <= 0 => (vel - 127) * helper,
            // The KSP writes this as `… * -1`; negation is the same value and
            // reads better.
            _ => -((127 - vel) * helper),
        }
    }

    /// Apply both controls to an incoming note velocity.
    ///
    /// Both gains are computed from the velocity **as played**, before the
    /// Color shift — the KSP computes them at `:2280` and `:2293` and only
    /// shifts at `:2302`. Using the shifted velocity here would double-count
    /// Color and over-compensate.
    ///
    /// **Dynamic Range is applied as a delta**, `law(knob) − law(0)`, so a
    /// control at rest contributes exactly nothing: the baseline tilt (and
    /// Kontakt's per-velocity volume table it belongs with) is the script's
    /// default voicing, which [`crate::piano_note_law`] applies whole — our
    /// packs hold the same raw samples Kontakt shapes, so without it the
    /// velocity response is ~9 dB too steep (A/B against Kontakt). The delta
    /// is what the knob *does* on top. It also collapses: the offsets cancel,
    /// leaving `(vel − 127) × knob` for both arms.
    ///
    /// Color needs no such treatment — its `case 0` arm is already zero, so
    /// the control is inherently relative.
    #[must_use]
    pub fn apply(&self, velocity: u8) -> VoiceShift {
        let vel = velocity.clamp(1, 127) as i32;

        let dynamic_mdb = self.dynamic_mdb_absolute(velocity, self.dynamic_range)
            - self.dynamic_mdb_absolute(velocity, 0);

        // ── Color (`{Color Volume}`) ────────────────────────────────────────
        let color = self.effective_color();
        // Integer arithmetic, in KSP's evaluation order — `* 100 / -50 * 12 / 10`
        // truncates at each division, so reordering it changes the result.
        let factor = (((color * 100) / -50) * 12) / 10;
        let color_mdb = match color {
            0 => 0,
            c if c < 0 => (vel + 20) * factor,
            _ => (-vel + 150) * factor,
        };

        VoiceShift {
            velocity: (vel + color).clamp(1, 127) as u8,
            trim_db: (dynamic_mdb + color_mdb) as f32 / 1000.0,
        }
    }
}
