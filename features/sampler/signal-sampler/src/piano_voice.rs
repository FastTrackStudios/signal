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
}

impl PianoOffsets {
    /// The Grandeur.
    pub const GRANDEUR: Self = Self {
        color: 0,
        dynamics: -55,
        kk_dynamics: -25,
    };
    /// The Maverick — note the biased Color.
    pub const MAVERICK: Self = Self {
        color: -5,
        dynamics: -50,
        kk_dynamics: -25,
    };
    /// The Gentleman.
    pub const GENTLEMAN: Self = Self {
        color: 0,
        dynamics: -50,
        kk_dynamics: -10,
    };
    /// The Giant, which ships its own script with no offset term at all.
    pub const GIANT: Self = Self {
        color: 0,
        dynamics: 0,
        kk_dynamics: 0,
    };

    /// Offsets for a library by name, matching the pack stems
    /// `build_ni_packs` writes (`"The Grandeur - Piano"` → Grandeur).
    ///
    /// Returns `None` for anything that is not one of the four: a pack this
    /// does not recognise should play flat, not borrow another piano's bias.
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
    pub offsets: PianoOffsets,
}

impl Default for PianoVoice {
    fn default() -> Self {
        Self {
            color: 0,
            dynamic_range: 0,
            velo_mode_4: true,
            offsets: PianoOffsets::GRANDEUR,
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
    pub fn new(offsets: PianoOffsets) -> Self {
        Self {
            offsets,
            ..Self::default()
        }
    }

    /// Whether this leaves every note untouched — the fast path, and the
    /// shipped default.
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
    /// control at rest contributes exactly nothing. The absolute law carries a
    /// baseline tilt that presupposes Kontakt's per-velocity volume table; our
    /// packs carry their own recorded levels instead, so importing the tilt
    /// would apply that voicing twice. The delta is what the knob *does*, and
    /// it is the part that transfers. Pleasingly it also collapses: the
    /// offsets cancel, leaving `(vel − 127) × knob` for both arms.
    ///
    /// Color needs no such treatment — its `case 0` arm is already zero, so
    /// the control is inherently relative.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_an_exact_identity() {
        let v = PianoVoice::default();
        assert!(v.is_identity());
        for vel in 1..=127u8 {
            let s = v.apply(vel);
            assert_eq!(s.velocity, vel, "velocity untouched at vel {vel}");
            assert_eq!(s.trim_db, 0.0, "no trim at vel {vel}");
        }
    }

    /// The worked example from the spec: at velocity 64, Color moves the
    /// sample it plays by ±50 steps while pulling the level the other way, so
    /// timbre changes and loudness roughly does not.
    #[test]
    fn color_trades_timbre_against_a_compensating_trim() {
        let mut v = PianoVoice::new(PianoOffsets::GRANDEUR);

        v.color = 50;
        let hard = v.apply(64);
        assert_eq!(hard.velocity, 114, "plays the hard-struck layer");
        assert!(
            (hard.trim_db - -10.32).abs() < 0.01,
            "pulled back down, got {}",
            hard.trim_db
        );

        v.color = -50;
        let soft = v.apply(64);
        assert_eq!(soft.velocity, 14, "plays the soft layer");
        assert!(
            (soft.trim_db - 10.08).abs() < 0.01,
            "pushed back up, got {}",
            soft.trim_db
        );

        // The two arms are NOT mirror images — a single signed formula would
        // make these equal and opposite, and they are not.
        assert!((hard.trim_db.abs() - soft.trim_db.abs()).abs() > 0.1);
    }

    #[test]
    fn color_saturates_rather_than_wrapping() {
        let mut v = PianoVoice::new(PianoOffsets::GRANDEUR);
        v.color = 50;
        assert_eq!(v.apply(120).velocity, 127, "clamps at the top");
        v.color = -50;
        assert_eq!(v.apply(10).velocity, 1, "clamps at the bottom, never 0");
    }

    /// The Maverick's −5 bias is the whole reason offsets are per-instrument.
    #[test]
    fn the_maverick_is_biased_five_steps_soft() {
        let grandeur = PianoVoice::new(PianoOffsets::GRANDEUR);
        let maverick = PianoVoice::new(PianoOffsets::MAVERICK);

        // Both knobs at 0: the Grandeur is neutral, the Maverick is not.
        assert!(grandeur.is_identity());
        assert!(!maverick.is_identity());
        assert_eq!(grandeur.apply(64).velocity, 64);
        assert_eq!(maverick.apply(64).velocity, 59, "five steps softer at 0");
        assert!(
            maverick.apply(64).trim_db > 0.0,
            "and compensated back up, not just quieter"
        );
    }

    #[test]
    fn maverick_color_clamps_instead_of_going_stale() {
        let mut v = PianoVoice::new(PianoOffsets::MAVERICK);
        // Knob at -50 + offset -5 = -55, outside the KSP's select arms.
        v.color = -50;
        let s = v.apply(64);
        assert_eq!(s.velocity, 14, "clamped to -50, not -55");
        // Deterministic: the same input always gives the same output, which is
        // exactly what the plugin's stale-variable path does not guarantee.
        assert_eq!(v.apply(64), s);
    }

    #[test]
    fn dynamic_range_compresses_and_expands_around_the_top() {
        let mut v = PianoVoice::new(PianoOffsets::GRANDEUR);

        // Compression: quiet notes come up relative to loud ones.
        v.dynamic_range = -200;
        let soft = v.apply(20).trim_db;
        let loud = v.apply(120).trim_db;
        assert!(soft > loud, "soft raised relative to loud ({soft} > {loud})");

        // Expansion pushes them apart the other way.
        v.dynamic_range = 200;
        let soft_x = v.apply(20).trim_db;
        let loud_x = v.apply(120).trim_db;
        assert!(soft_x < loud_x, "soft pushed down ({soft_x} < {loud_x})");

        // Velocity 127 is the pivot in both directions: nothing to move.
        v.dynamic_range = -200;
        assert!(v.apply(127).trim_db.abs() < 1e-6);
        v.dynamic_range = 200;
        assert!(v.apply(127).trim_db.abs() < 1e-6);
    }

    /// The absolute law is tilted at a knob of 0 — that is the instrument's
    /// baseline voicing, and the reason `apply` uses the delta. Pinned so a
    /// "just use the KSP formula directly" change has to argue with it.
    #[test]
    fn the_absolute_dynamic_law_is_tilted_at_rest_but_apply_is_not() {
        let v = PianoVoice::new(PianoOffsets::GRANDEUR);
        let helper = -55 + -25; // DYN_OFFSET + KK_DYN_OFFSET, knob at 0
        assert_eq!(v.dynamic_mdb_absolute(64, 0), (64 - 127) * helper);
        assert_eq!(v.dynamic_mdb_absolute(64, 0), 5040, "+5.04 dB baseline");
        // Which apply() must NOT import — our packs already carry their own
        // recorded levels.
        assert_eq!(v.apply(64).trim_db, 0.0);
    }

    /// The delta collapses to `(vel − 127) × knob`: the offsets cancel, and
    /// both arms agree. Worth pinning because it is not obvious from the two
    /// asymmetric-looking KSP branches.
    #[test]
    fn the_dynamic_delta_is_offset_independent() {
        for offsets in [
            PianoOffsets::GRANDEUR,
            PianoOffsets::MAVERICK,
            PianoOffsets::GENTLEMAN,
        ] {
            let at_rest = PianoVoice::new(offsets);
            let mut v = at_rest;
            for knob in [-200, -75, -1, 1, 75, 200] {
                v.dynamic_range = knob;
                for vel in [1u8, 40, 64, 100, 127] {
                    // Isolate the dynamic contribution: on The Maverick the
                    // Color bias is also in `trim_db`, and it is not what this
                    // test is about.
                    let got = v.apply(vel).trim_db - at_rest.apply(vel).trim_db;
                    let expect = ((vel as i32 - 127) * knob) as f32 / 1000.0;
                    // 1e-3 dB, i.e. a thousandth of a decibel — f32 cannot hold
                    // 25.2 exactly and nothing here needs it to.
                    assert!(
                        (got - expect).abs() < 1e-3,
                        "offsets {offsets:?} knob {knob} vel {vel}: {got} != {expect}"
                    );
                }
            }
        }
    }

    /// `$Ana_mnuVelo` moves the absolute law but NOT what the knob does —
    /// another consequence of `apply` being a delta. If a future change makes
    /// velo mode audible through `apply`, that is a real behaviour change and
    /// this test should be the thing that objects.
    #[test]
    fn velo_mode_moves_the_absolute_law_but_not_the_delta() {
        let mut a = PianoVoice::new(PianoOffsets::GENTLEMAN);
        a.dynamic_range = -100;
        let mut b = a;
        b.velo_mode_4 = false;

        assert_ne!(
            a.dynamic_mdb_absolute(40, -100),
            b.dynamic_mdb_absolute(40, -100),
            "the absolute law sees KK_DYN_OFFSET"
        );
        assert_eq!(
            a.apply(40).trim_db,
            b.apply(40).trim_db,
            "but the knob's effect is the same either way"
        );
    }

    #[test]
    fn library_names_map_to_their_own_offsets() {
        assert_eq!(
            PianoOffsets::for_library("The Grandeur - Piano"),
            Some(PianoOffsets::GRANDEUR)
        );
        assert_eq!(
            PianoOffsets::for_library("The Maverick - Resonance"),
            Some(PianoOffsets::MAVERICK)
        );
        assert_eq!(
            PianoOffsets::for_library("The Giant Cinematic - Piano"),
            Some(PianoOffsets::GIANT)
        );
        // A Keyscape pack is not one of these and must not inherit a bias.
        assert_eq!(PianoOffsets::for_library("Double Felt Grand"), None);
    }
}
