//! Modulation profiles — the five circuits, and what each one modulates.
//!
//! Modulation is usually sold as a pedalboard: chorus, flanger, phaser,
//! tremolo, wah, rotary, a dozen names. Underneath there are only three
//! things you can modulate, and which one a box moves decides almost
//! everything about how it sounds.
//!
//! Chorus · Flanger · Vibrato · Tremolo · Wah
//!
//! - **Chorus** modulates *delay time* and mixes the result back with the dry
//!   signal. The pitch wobble is a side effect of the delay moving; the
//!   thickening is the comb filter between wet and dry.
//! - **Flanger** is the same mechanism with a much shorter delay and
//!   feedback. The comb notches are sparse enough to hear individually, and
//!   the feedback is what turns them into a resonant sweep — it is not a
//!   "deeper chorus", it is a different filter.
//! - **Vibrato** is the same delay modulation with the dry signal *removed*.
//!   No comb, no thickening: just pitch. That is why it is here as its own
//!   circuit and not as a chorus with the mix turned up.
//! - **Tremolo** modulates *amplitude*. Nothing moves in pitch and nothing
//!   combs — which is why it survives on a mono source where the other three
//!   go strange.
//! - **Wah** modulates *filter cutoff*, and the interesting part is what
//!   drives it: an envelope follower, a pedal, or a pattern.
//!
//! Rate and depth are deliberately not families. They are the two controls
//! every one of these has, the way drive is on every saturator.
//!
//! Each profile carries the [`Voicing`] it *is* — which chain, which engine,
//! and where its own controls rest — and [`apply`] is the one place that
//! knows how a knob reaches the DSP.
//!
//! Pure data and arithmetic — no GUI, no framework deps.

pub use modulation::chorus::chain::ChorusChain;
pub use modulation::chorus::engine::{EffectType, EngineType};
pub use modulation::trem::chain::TremChain;
pub use modulation::trem::tremolo::{AnalogStyle, TremMode};
pub use modulation::wah::chain::{WahChain, WahSource};
pub use modulation::wah::filter::WahMode;

/// Which of the three chains a profile runs.
///
/// Not a cosmetic distinction: the three are different processors with
/// different state, and the plugin dispatches on this every block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chain {
    Delay,
    Tremolo,
    Wah,
}

/// The circuit a profile is, and how it is configured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Circuit {
    /// Delay-line modulation. `effect` decides whether the wet signal is
    /// combed against the dry (chorus), combed with feedback (flanger), or
    /// used alone (vibrato); `engine` decides what the delay line is made of.
    Delay {
        engine: EngineType,
        effect: EffectType,
    },
    /// Amplitude modulation.
    Tremolo { mode: TremMode },
    /// Filter modulation. `source` is what drives the cutoff, `filter` is
    /// what the cutoff is doing.
    Wah { source: WahSource, filter: WahMode },
}

impl Circuit {
    pub fn chain(self) -> Chain {
        match self {
            Circuit::Delay { .. } => Chain::Delay,
            Circuit::Tremolo { .. } => Chain::Tremolo,
            Circuit::Wah { .. } => Chain::Wah,
        }
    }
}

/// What a circuit's own knobs are wired to.
///
/// The panels name these per profile — a Juno's second knob is "Brightness",
/// a BBD's is "Clock" — but a name is not a mapping. This is the single
/// vocabulary: the panel decides what to *call* a knob, this decides what it
/// *does*. Each variant's natural range is given, because that is what
/// [`Knob::at_noon`] is expressed in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Character {
    /// Not offered on this circuit.
    None,
    /// Chorus voices per channel, 1..4. More voices is a thicker comb, not a
    /// deeper one.
    Voices,
    /// The engine's own colour, 0..1 — and it means something different on
    /// each, which is exactly why it is a per-profile legend: BBD clock
    /// frequency, tape drive and brightness, orbit eccentricity, Juno filter
    /// cutoff. On the clean Cubic engine it does nothing at all, which is
    /// also worth knowing.
    Colour,
    /// Delay-line feedback, 0..1. The control that makes a flanger a flanger.
    Feedback,
    /// Stereo spread, 0..1.
    Width,
    /// Swing/shuffle, −1..1. Negative shortens the first beat.
    Groove,
    /// Dragging/rushing, −1..1 — up to ±50 ms of push or lay-back.
    Feel,
    /// Downbeat emphasis, −1..1.
    Accent,
    /// Which saturation the amplitude modulator runs through, 0..6 as an
    /// index into [`ANALOG_STYLES`].
    Analog,
    /// Harmonic tremolo's band split, in Hz.
    Crossover,
    /// Pedal position, 0..1 — where the filter sits before anything sweeps it.
    Position,
    /// Filter Q, 1..20. Higher is more vocal.
    Resonance,
    /// Envelope sensitivity, 0..1.
    Sensitivity,
    /// Cascaded filter stages, 1..4.
    Stages,
    /// How much of the cutoff comes from the pattern rather than the
    /// envelope, 0..1.
    Pattern,
    /// Which filter shape the wah sweeps, 0..3 as an index into [`WahMode`].
    Shape,
}

/// One circuit knob: what it does, and where this profile leaves it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Knob {
    pub role: Character,
    /// Where the knob sits at noon, in the role's own natural units.
    pub at_noon: f32,
}

const fn k(role: Character, at_noon: f32) -> Knob {
    Knob { role, at_noon }
}

const NO_KNOB: Knob = Knob {
    role: Character::None,
    at_noon: 0.0,
};

/// The engine settings a profile *is*.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Voicing {
    pub circuit: Circuit,
    /// Resting LFO rate, Hz.
    pub rate_hz: f32,
    /// Resting depth, 0..1.
    pub depth: f32,
    /// Resting dry/wet, 0..1.
    pub mix: f32,
    /// A circuit with no dry path. Vibrato is the only one, and it is what
    /// makes it vibrato rather than chorus — so the panel does not offer a
    /// Mix knob that would be a lie.
    pub wet_only: bool,
    pub knobs: [Knob; 4],
}

impl Voicing {
    const fn base(circuit: Circuit) -> Self {
        Self {
            circuit,
            rate_hz: 0.8,
            depth: 0.5,
            mix: 0.5,
            wet_only: false,
            knobs: [NO_KNOB, NO_KNOB, NO_KNOB, NO_KNOB],
        }
    }
}

/// One selectable modulator.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Profile {
    /// Stable id. **Persisted** — a session records this, never an index, so
    /// adding or reordering profiles cannot change what an old project opens
    /// with.
    pub id: &'static str,
    pub name: &'static str,
    pub voicing: Voicing,
    /// One line on what it is for.
    pub voice: &'static str,
}

/// A rail entry: one circuit, and the variants that cycle inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    pub badge: &'static str,
    pub profiles: &'static [&'static str],
}

pub static PROFILES: &[Profile] = &[
    // ── Chorus: delay modulation, combed against the dry ─────────────────
    Profile {
        id: "juno",
        name: "Juno",
        // Triangle LFO through an allpass delay, and the reason a Juno-60
        // sounds like one is that it is barely a chorus at all: one slow,
        // shallow voice with the mix well up. Turning the depth up does not
        // make it more Juno, it makes it less.
        voicing: Voicing {
            rate_hz: 0.6,
            depth: 0.32,
            mix: 0.6,
            knobs: [
                k(Character::Voices, 2.0),
                k(Character::Colour, 0.55), // filter cutoff on this engine
                k(Character::Feedback, 0.0),
                k(Character::Width, 0.85),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Juno,
                effect: EffectType::Chorus,
            })
        },
        voice: "One slow shallow voice and a lot of it. The sound of a Juno-60, which is a smaller effect than people remember.",
    },
    Profile {
        id: "bbd",
        name: "Bucket Brigade",
        // A clocked analogue delay line: the clock rate sets both the delay
        // and the bandwidth, so Colour here is genuinely one control doing
        // two things — which is what makes it sound like a pedal.
        voicing: Voicing {
            rate_hz: 0.5,
            depth: 0.45,
            mix: 0.5,
            knobs: [
                k(Character::Voices, 2.0),
                k(Character::Colour, 0.45), // clock frequency → bandwidth
                k(Character::Feedback, 0.1),
                k(Character::Width, 0.7),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Bbd,
                effect: EffectType::Chorus,
            })
        },
        voice: "A clocked analogue line. The clock sets the delay and the bandwidth at once, so it darkens as it deepens.",
    },
    Profile {
        id: "tape",
        name: "Tape",
        // Wow and flutter rather than an LFO, plus saturation. The slowest
        // and least periodic of the five: what you notice is that it never
        // quite repeats.
        voicing: Voicing {
            rate_hz: 0.35,
            depth: 0.5,
            mix: 0.5,
            knobs: [
                k(Character::Voices, 2.0),
                k(Character::Colour, 0.4), // drive + brightness together
                k(Character::Feedback, 0.05),
                k(Character::Width, 0.6),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Tape,
                effect: EffectType::Chorus,
            })
        },
        voice: "Wow and flutter instead of an LFO. Slow, saturated, and never quite repeating.",
    },
    Profile {
        id: "orbit",
        name: "Orbit",
        // Two taps on an elliptical path rather than one on a sine. Colour is
        // the eccentricity: round is a plain chorus, flat is nearly a
        // ping-pong.
        voicing: Voicing {
            rate_hz: 0.25,
            depth: 0.6,
            mix: 0.5,
            knobs: [
                k(Character::Voices, 3.0),
                k(Character::Colour, 0.5), // eccentricity of the orbit
                k(Character::Feedback, 0.0),
                k(Character::Width, 1.0),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Orbit,
                effect: EffectType::Chorus,
            })
        },
        voice: "Two taps on an elliptical path. Round is a chorus; flatten it and the image starts to swing.",
    },
    Profile {
        id: "cubic",
        name: "Clean",
        // Catmull-Rom interpolation and nothing else — no clock, no tape, no
        // filter. Its Colour knob does nothing, which the panel says by not
        // drawing one.
        voicing: Voicing {
            rate_hz: 0.9,
            depth: 0.4,
            mix: 0.5,
            knobs: [
                k(Character::Voices, 2.0),
                k(Character::Feedback, 0.0),
                k(Character::Width, 0.75),
                NO_KNOB,
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Cubic,
                effect: EffectType::Chorus,
            })
        },
        voice: "Interpolation and nothing else. The one to reach for when the effect should not have a character.",
    },
    // ── Flanger: the same mechanism, shorter and fed back ────────────────
    Profile {
        id: "flanger",
        name: "Flanger",
        // The feedback is the effect. At zero this is a chorus with a short
        // delay; the resonant sweep only exists because the comb is being
        // fed back into itself.
        voicing: Voicing {
            rate_hz: 0.3,
            depth: 0.6,
            mix: 0.5,
            knobs: [
                k(Character::Feedback, 0.6),
                k(Character::Colour, 0.5),
                k(Character::Width, 0.5),
                k(Character::Voices, 1.0),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Cubic,
                effect: EffectType::Flanger,
            })
        },
        voice: "A short comb fed back into itself. Turn the feedback down and you have a chorus; the sweep is the feedback.",
    },
    Profile {
        id: "flanger_bbd",
        name: "Analog Flanger",
        // The same circuit built out of a bucket brigade, which is how every
        // flanger pedal worth owning was built. Darker, and the feedback path
        // loses top end each time round.
        voicing: Voicing {
            rate_hz: 0.22,
            depth: 0.7,
            mix: 0.5,
            knobs: [
                k(Character::Feedback, 0.7),
                k(Character::Colour, 0.4),
                k(Character::Width, 0.45),
                k(Character::Voices, 1.0),
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Bbd,
                effect: EffectType::Flanger,
            })
        },
        voice: "The same comb through a bucket brigade. Each pass round the feedback loop loses a little more top.",
    },
    // ── Vibrato: delay modulation with the dry path removed ──────────────
    Profile {
        id: "vibrato",
        name: "Vibrato",
        // No dry signal, so no comb: this is pitch and only pitch. Shallower
        // and faster than a chorus, because without the dry reference the ear
        // hears the full excursion.
        voicing: Voicing {
            rate_hz: 5.0,
            depth: 0.25,
            mix: 1.0,
            wet_only: true,
            knobs: [
                k(Character::Colour, 0.5),
                k(Character::Width, 0.3),
                k(Character::Voices, 1.0),
                NO_KNOB,
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Cubic,
                effect: EffectType::Vibrato,
            })
        },
        voice: "Pitch, with no dry signal to comb against it. Shallower than a chorus, because you hear all of it.",
    },
    Profile {
        id: "vibrato_juno",
        name: "Juno Vibrato",
        voicing: Voicing {
            rate_hz: 4.2,
            depth: 0.3,
            mix: 1.0,
            wet_only: true,
            knobs: [
                k(Character::Colour, 0.6),
                k(Character::Width, 0.4),
                k(Character::Voices, 1.0),
                NO_KNOB,
            ],
            ..Voicing::base(Circuit::Delay {
                engine: EngineType::Juno,
                effect: EffectType::Vibrato,
            })
        },
        voice: "The Juno's allpass line with the dry path pulled. Rounder than the clean one, and a little darker.",
    },
    // ── Tremolo: amplitude, and nothing else moves ───────────────────────
    Profile {
        id: "trem_opto",
        name: "Opto",
        // One lamp behind one cell: both channels together. Slow-ish, deep,
        // and slightly dirty — the amplifier tremolos that this models were
        // never clean.
        voicing: Voicing {
            rate_hz: 4.5,
            depth: 0.6,
            mix: 1.0,
            knobs: [
                k(Character::Groove, 0.0),
                k(Character::Feel, 0.0),
                k(Character::Accent, 0.0),
                k(Character::Analog, AnalogStyle::Fat as u8 as f32),
            ],
            ..Voicing::base(Circuit::Tremolo {
                mode: TremMode::Mono,
            })
        },
        voice: "One lamp, one cell, both channels together. The amp tremolo, and it was never a clean effect.",
    },
    Profile {
        id: "trem_stereo",
        name: "Stereo",
        // The same modulation 90° apart on the two channels: the level never
        // drops, it moves. Only makes sense in stereo, and makes a mono
        // source into one.
        voicing: Voicing {
            rate_hz: 3.0,
            depth: 0.7,
            mix: 1.0,
            knobs: [
                k(Character::Groove, 0.0),
                k(Character::Feel, 0.0),
                k(Character::Accent, 0.0),
                k(Character::Analog, AnalogStyle::Clean as u8 as f32),
            ],
            ..Voicing::base(Circuit::Tremolo {
                mode: TremMode::Stereo,
            })
        },
        voice: "Ninety degrees apart on the two channels: the level never drops, it crosses the room.",
    },
    Profile {
        id: "trem_harmonic",
        name: "Harmonic",
        // Brownface: the bands move out of phase with each other, so the
        // *tone* wobbles while the level barely does. Not a filter sweep and
        // not a tremolo — the third thing, and the crossover is where it
        // lives.
        voicing: Voicing {
            rate_hz: 3.5,
            depth: 0.8,
            mix: 1.0,
            knobs: [
                k(Character::Groove, 0.0),
                k(Character::Feel, 0.0),
                k(Character::Crossover, 800.0),
                k(Character::Analog, AnalogStyle::Fat as u8 as f32),
            ],
            ..Voicing::base(Circuit::Tremolo {
                mode: TremMode::Harmonic,
            })
        },
        voice: "Low and high moving against each other. The level holds still and the tone sways — the brownface trick.",
    },
    // ── Wah: filter cutoff, and what drives it ───────────────────────────
    Profile {
        id: "wah_auto",
        name: "Auto Wah",
        // The envelope is the pedal. Sensitivity is the control that matters
        // and the one every auto-wah hides.
        voicing: Voicing {
            rate_hz: 1.0,
            depth: 0.7,
            mix: 1.0,
            knobs: [
                k(Character::Position, 0.3),
                k(Character::Resonance, 6.0),
                k(Character::Sensitivity, 0.5),
                k(Character::Shape, WahMode::Classic as u8 as f32),
            ],
            ..Voicing::base(Circuit::Wah {
                source: WahSource::Envelope,
                filter: WahMode::Classic,
            })
        },
        voice: "The envelope is the pedal. How hard you play is where the filter goes.",
    },
    Profile {
        id: "wah_pedal",
        name: "Pedal",
        // No envelope, no pattern: Position IS the effect, and it is there to
        // be automated or ridden by a controller. Cascaded stages are what
        // separate a real inductor pedal from a bandpass.
        voicing: Voicing {
            rate_hz: 1.0,
            depth: 0.0,
            mix: 1.0,
            knobs: [
                k(Character::Position, 0.4),
                k(Character::Resonance, 9.0),
                k(Character::Stages, 2.0),
                k(Character::Shape, WahMode::Mutron as u8 as f32),
            ],
            ..Voicing::base(Circuit::Wah {
                source: WahSource::Envelope,
                filter: WahMode::Mutron,
            })
        },
        voice: "Position is the effect. Nothing sweeps it but you — automate it, or ride it from a controller.",
    },
    Profile {
        id: "wah_pattern",
        name: "Rhythmic",
        // The pattern drives the cutoff, so this is the one wah that is a
        // modulator rather than a response. Everything else on the rail is
        // periodic; this is the only one where that is the point.
        voicing: Voicing {
            rate_hz: 2.0,
            depth: 0.6,
            mix: 1.0,
            knobs: [
                k(Character::Position, 0.35),
                k(Character::Resonance, 8.0),
                k(Character::Pattern, 0.8),
                k(Character::Shape, WahMode::Lowpass as u8 as f32),
            ],
            ..Voicing::base(Circuit::Wah {
                source: WahSource::Both,
                filter: WahMode::Lowpass,
            })
        },
        voice: "The pattern drives the filter. The only wah here that is a modulator rather than a response.",
    },
];

/// The rail: thickening first, then the two that only move one thing.
pub static CATEGORIES: &[Category] = &[
    Category {
        id: "chorus",
        label: "Chorus",
        badge: "CHOR",
        profiles: &["juno", "bbd", "tape", "orbit", "cubic"],
    },
    Category {
        id: "flanger",
        label: "Flanger",
        badge: "FLNG",
        profiles: &["flanger", "flanger_bbd"],
    },
    Category {
        id: "vibrato",
        label: "Vibrato",
        badge: "VIB",
        profiles: &["vibrato", "vibrato_juno"],
    },
    Category {
        id: "tremolo",
        label: "Tremolo",
        badge: "TREM",
        profiles: &["trem_opto", "trem_stereo", "trem_harmonic"],
    },
    Category {
        id: "wah",
        label: "Wah",
        badge: "WAH",
        profiles: &["wah_auto", "wah_pedal", "wah_pattern"],
    },
];

pub fn profile_by_id(id: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.id == id)
}

pub fn profile_index(id: &str) -> Option<usize> {
    PROFILES.iter().position(|p| p.id == id)
}

pub fn category_of(profile_id: &str) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .profiles
            .iter()
            .position(|id| *id == profile_id)
            .map(|vi| (ci, vi))
    })
}

/// Clicking the family you are in advances through it and wraps; clicking
/// another lands on its first.
pub fn rail_click_target(current_index: usize, clicked_category: usize) -> usize {
    let current_id = PROFILES.get(current_index).map(|p| p.id).unwrap_or("");
    let Some(category) = CATEGORIES.get(clicked_category) else {
        return current_index;
    };
    let next_id = match category_of(current_id) {
        Some((ci, vi)) if ci == clicked_category => {
            category.profiles[(vi + 1) % category.profiles.len()]
        }
        _ => category.profiles[0],
    };
    profile_index(next_id).unwrap_or(current_index)
}

// ── The mapping ──────────────────────────────────────────────────────────

/// The panel's knobs, normalised the way the parameters are.
///
/// **0.5 is the circuit as designed** on the four circuit knobs, exactly as
/// the saturator's are: a panel at noon is the voicing above and nothing
/// else. `rate`, `depth` and `mix` are absolute, because they mean the same
/// thing on every modulator ever built.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Controls {
    /// Normalised rate — see [`rate_hz_from`] for the taper.
    pub rate: f32,
    pub depth: f32,
    pub mix: f32,
    pub knobs: [f32; 4],
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            rate: 0.5,
            depth: 0.5,
            mix: 0.5,
            knobs: [0.5; 4],
        }
    }
}

/// The rate knob's range. Modulation rate is heard logarithmically — the step
/// from 0.1 to 0.2 Hz is as big a musical move as 5 to 10 — so the knob is
/// exponential and a linear one would spend most of its travel above the
/// useful range.
pub const RATE_MIN_HZ: f32 = 0.05;
pub const RATE_MAX_HZ: f32 = 20.0;

/// Normalised knob → Hz.
pub fn rate_hz_from(knob: f32) -> f32 {
    let k = knob.clamp(0.0, 1.0);
    // Clamped rather than trusted: the round trip through log/exp lands a
    // few ulps past the top, and a rate that is 20.000002 Hz would fail its
    // own range check.
    (RATE_MIN_HZ * exp2(k * log2(RATE_MAX_HZ / RATE_MIN_HZ))).clamp(RATE_MIN_HZ, RATE_MAX_HZ)
}

/// Hz → normalised knob, so a profile's resting rate can be shown on the
/// same control the user turns.
pub fn rate_knob_from(hz: f32) -> f32 {
    let hz = hz.clamp(RATE_MIN_HZ, RATE_MAX_HZ);
    (log2(hz / RATE_MIN_HZ) / log2(RATE_MAX_HZ / RATE_MIN_HZ)).clamp(0.0, 1.0)
}

fn log2(x: f32) -> f32 {
    x.max(1.0e-9).ln() / std::f32::consts::LN_2
}

fn exp2(x: f32) -> f32 {
    (x * std::f32::consts::LN_2).exp()
}

/// A centred trim: 0.5 is `at_noon`, and the ends reach `lo` and `hi`.
fn trim(knob: f32, at_noon: f32, lo: f32, hi: f32) -> f32 {
    let k = knob.clamp(0.0, 1.0);
    if k >= 0.5 {
        at_noon + (hi - at_noon) * (k - 0.5) * 2.0
    } else {
        lo + (at_noon - lo) * k * 2.0
    }
}

/// A centred trim read logarithmically — for the controls where the ear hears
/// ratios rather than differences (a crossover frequency, a filter Q).
fn trim_ratio(knob: f32, at_noon: f32, octaves: f32, lo: f32, hi: f32) -> f32 {
    let k = (knob.clamp(0.0, 1.0) - 0.5) * 2.0 * octaves;
    (at_noon * exp2(k)).clamp(lo, hi)
}

/// Pick a variant by index from a knob, over `count` choices.
fn pick(knob: f32, at_noon: f32, count: usize) -> usize {
    let n = count.max(1);
    // The knob spans every choice; noon is wherever the profile put it only
    // in the sense that a fresh panel opens there, so the taper is plain.
    let idx = (knob.clamp(0.0, 1.0) * n as f32) as usize;
    if knob >= 0.999 {
        n - 1
    } else if (knob - 0.5).abs() < 1.0e-6 {
        (at_noon as usize).min(n - 1)
    } else {
        idx.min(n - 1)
    }
}

/// The saturation styles the tremolo can run through, in [`AnalogStyle`]
/// order. Kept here rather than derived because the DSP enum has no
/// iteration and a wrong count would silently make the last style
/// unreachable.
pub const ANALOG_STYLES: [AnalogStyle; 7] = [
    AnalogStyle::Clean,
    AnalogStyle::Fat,
    AnalogStyle::Squash,
    AnalogStyle::Dirt,
    AnalogStyle::Crunch,
    AnalogStyle::Shred,
    AnalogStyle::Pump,
];

/// The wah's filter shapes, in [`WahMode`] order.
pub const WAH_SHAPES: [WahMode; 4] = [
    WahMode::Classic,
    WahMode::Mutron,
    WahMode::Lowpass,
    WahMode::Phaser,
];

/// Point the engines at a profile.
///
/// This is the only place that knows how a knob reaches the DSP. The plugin
/// calls it every block; the editor calls it to draw the modulation shape,
/// which is why the picture on the panel is the movement the audio thread
/// will actually apply rather than a drawing of one.
///
/// Only the profile's own chain is touched — the other two keep whatever they
/// had, so switching back to a family finds it as you left it.
///
/// **Not allocation-free**: [`ChorusChain::set_engine`] rebuilds its voice
/// vectors, and it is called whenever the selected engine changes. Same
/// contract as the reverb's `set_algorithm_variant`. Everything else here is
/// plain field writes.
pub fn apply(
    profile: &Profile,
    controls: &Controls,
    chorus: &mut ChorusChain,
    trem: &mut TremChain,
    wah: &mut WahChain,
) {
    let v = profile.voicing;
    let rate = rate_hz_from(controls.rate) as f64;
    let depth = controls.depth.clamp(0.0, 1.0) as f64;
    // A wet-only circuit ignores the mix knob rather than pretending: see
    // `Voicing::wet_only`.
    let mix = if v.wet_only {
        1.0
    } else {
        controls.mix.clamp(0.0, 1.0) as f64
    };

    match v.circuit {
        Circuit::Delay { engine, effect } => {
            chorus.set_engine(engine);
            chorus.effect_type = effect;
            chorus.rate_hz = rate;
            chorus.depth = depth;
            chorus.mix = mix;
            // Defaults for anything this profile does not wire, so switching
            // profiles never inherits the last one's feedback.
            chorus.feedback = 0.0;
            chorus.color = 0.5;
            chorus.width = 0.5;
            chorus.num_voices = 2;
        }
        Circuit::Tremolo { mode } => {
            trem.set_mode(mode);
            trem.set_depth(depth);
            trem.mix = mix;
            free_running(&mut trem.modulator.trigger, rate);
            // Stereo voicing is a 90° offset; the chain only reads its stereo
            // path when `stereo_phase` is nonzero.
            let (phase, offset) = if mode == TremMode::Stereo {
                (90.0, 0.25)
            } else {
                (0.0, 0.0)
            };
            trem.stereo_phase = phase;
            trem.modulator.stereo_offset = offset;
            trem.groove = 0.0;
            trem.feel = 0.0;
            trem.accent = 0.0;
            trem.analog_l.style = AnalogStyle::Clean;
            trem.analog_r.style = AnalogStyle::Clean;
        }
        Circuit::Wah { source, filter } => {
            wah.source = source;
            wah.env_amount = depth;
            wah.mix = mix;
            free_running(&mut wah.modulator.trigger, rate);
            wah.filter_l.mode = filter;
            wah.filter_r.mode = filter;
            wah.base_position = 0.3;
            wah.sensitivity = 0.5;
            wah.pattern_amount = if source == WahSource::Envelope {
                0.0
            } else {
                0.5
            };
            wah.filter_l.q = 6.0;
            wah.filter_r.q = 6.0;
            wah.filter_l.stages = 2;
            wah.filter_r.stages = 2;
        }
    }

    for (knob, value) in v.knobs.iter().zip(controls.knobs.iter()) {
        apply_knob(*knob, *value, chorus, trem, wah);
    }
}

/// Put a modulator on its own clock at `rate`.
///
/// Belongs here rather than in the plugin's constructor: a chain that has a
/// rate but is still waiting for a transport does not move at all, and the
/// editor builds its own chains to draw from. Setting it in one place and not
/// the other is how the panel ended up drawing a flat line for a tremolo.
///
/// Tempo sync is a follow-up — the host transport is not plumbed through yet.
fn free_running(trigger: &mut modulation::trem::fts_modulation::TriggerEngine, rate: f64) {
    trigger.mode = modulation::trem::fts_modulation::TriggerMode::Free;
    trigger.sync_index = 0;
    trigger.rate_hz = rate;
}

fn apply_knob(
    knob: Knob,
    value: f32,
    chorus: &mut ChorusChain,
    trem: &mut TremChain,
    wah: &mut WahChain,
) {
    let v = value.clamp(0.0, 1.0);
    match knob.role {
        Character::None => {}
        Character::Voices => {
            chorus.num_voices = trim(v, knob.at_noon, 1.0, 4.0).round().clamp(1.0, 4.0) as usize
        }
        Character::Colour => chorus.color = trim(v, knob.at_noon, 0.0, 1.0) as f64,
        Character::Feedback => chorus.feedback = trim(v, knob.at_noon, 0.0, 0.95) as f64,
        Character::Width => chorus.width = trim(v, knob.at_noon, 0.0, 1.0) as f64,
        Character::Groove => trem.groove = trim(v, knob.at_noon, -1.0, 1.0) as f64,
        Character::Feel => trem.feel = trim(v, knob.at_noon, -1.0, 1.0) as f64,
        Character::Accent => trem.accent = trim(v, knob.at_noon, -1.0, 1.0) as f64,
        Character::Analog => {
            let style = ANALOG_STYLES[pick(v, knob.at_noon, ANALOG_STYLES.len())];
            trem.analog_l.style = style;
            trem.analog_r.style = style;
        }
        Character::Crossover => {
            let hz = trim_ratio(v, knob.at_noon, 1.5, 100.0, 4_000.0) as f64;
            trem.tremolo_l.crossover_freq = hz;
            trem.tremolo_r.crossover_freq = hz;
        }
        Character::Position => wah.base_position = trim(v, knob.at_noon, 0.0, 1.0) as f64,
        Character::Resonance => {
            let q = trim_ratio(v, knob.at_noon, 1.2, 1.0, 20.0) as f64;
            wah.filter_l.q = q;
            wah.filter_r.q = q;
        }
        Character::Sensitivity => wah.sensitivity = trim(v, knob.at_noon, 0.0, 1.0) as f64,
        Character::Stages => {
            let n = trim(v, knob.at_noon, 1.0, 4.0).round().clamp(1.0, 4.0) as usize;
            wah.filter_l.stages = n;
            wah.filter_r.stages = n;
        }
        Character::Pattern => wah.pattern_amount = trim(v, knob.at_noon, 0.0, 1.0) as f64,
        Character::Shape => {
            let shape = WAH_SHAPES[pick(v, knob.at_noon, WAH_SHAPES.len())];
            wah.filter_l.mode = shape;
            wah.filter_r.mode = shape;
        }
    }
}

// ── The shape ────────────────────────────────────────────────────────────

/// Sample one cycle of what a profile *moves*, normalised to 0..1.
///
/// This is the modulator's equivalent of the saturator's transfer curve, and
/// it is taken from the engines for the same reason: a drawn sine is a
/// picture of a chorus rather than of this one. The tape engine wanders on
/// wow and flutter that are not locked to the rate; the orbit engine's line
/// depends on a second slow rotation; a tremolo with groove on it is not
/// symmetrical any more. All of that is in here because all of it is audible.
///
/// What is being plotted differs by circuit, and honestly so:
///
/// - **Delay circuits** plot *delay time*, which is what the LFO actually
///   moves. The pitch wobble and the comb are both downstream of this line.
/// - **Tremolo** plots the gain the amplitude modulator applies — the shape
///   the level literally follows.
/// - **Wah** plots the modulator driving the cutoff. On an envelope-driven
///   profile there is nothing periodic to draw, and the line is flat at the
///   pedal position: correct, and worth seeing, because that circuit responds
///   to playing rather than to time.
///
/// Normalisation is per-circuit against its own range, so the drawing fills
/// the box whatever the units are. `out` may be any length ≥ 2.
pub fn shape(profile: &Profile, controls: &Controls, out: &mut [f64]) {
    if out.is_empty() {
        return;
    }
    let mut chorus = ChorusChain::new();
    let mut trem = TremChain::new();
    let mut wah = WahChain::new();
    apply(profile, controls, &mut chorus, &mut trem, &mut wah);

    match profile.voicing.circuit {
        Circuit::Delay { engine, effect } => {
            modulation::chorus::analysis::delay_cycle(
                engine,
                effect,
                chorus.rate_hz,
                chorus.depth,
                chorus.color,
                chorus.feedback,
                0.0,
                out,
            );
        }
        Circuit::Tremolo { .. } => {
            let n = out.len().max(2);
            // The same pretend-sample-rate trick the chorus analysis uses:
            // one cycle in `n` ticks whatever the real rate is.
            let rate = trem.modulator.trigger.rate_hz.max(1.0e-6);
            trem.modulator.update(n as f64 * rate.max(1.0));
            trem.modulator.reset();
            let transport = TransportInfo::default();
            for slot in out.iter_mut() {
                trem.modulator.tick(&transport, 0.0);
                *slot = trem.modulator.output();
            }
        }
        Circuit::Wah { source, .. } => {
            let n = out.len().max(2);
            if source == WahSource::Envelope {
                // Nothing periodic drives this one — the line is where the
                // pedal is sitting, and it does not move until you play.
                out.fill(wah.base_position);
                // Already in 0..1, and a flat line has no range to normalise
                // against: drawn where the pedal actually is.
                return;
            } else {
                let rate = wah.modulator.trigger.rate_hz.max(1.0e-6);
                wah.modulator.update(n as f64 * rate.max(1.0));
                wah.modulator.reset();
                let transport = TransportInfo::default();
                for slot in out.iter_mut() {
                    wah.modulator.tick(&transport, 0.0);
                    *slot = wah.base_position + wah.modulator.output() * wah.pattern_amount;
                }
            }
        }
    }
    // A circuit that is not moving has no range to scale against, so the
    // caller says where to draw the line instead. Mid-height for anything
    // measured in its own units (a delay in ms means nothing as a height);
    // the wah's own position when it is a position.
    normalise(out, 0.5);
}

/// Scale a sampled cycle into 0..1 against its own range.
///
/// A nearly-flat line stays flat rather than being stretched into a wobble it
/// does not have: a vibrato at 2% depth and one at 100% should not draw the
/// same picture, and neither should a wah that is not moving.
fn normalise(out: &mut [f64], flat_at: f64) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in out.iter() {
        if v.is_finite() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        out.fill(flat_at);
        return;
    }
    // Below this the movement is not worth drawing as movement. The span is
    // in the circuit's own units, so the floor is relative to the level.
    let span = hi - lo;
    let reference = hi.abs().max(1.0e-6);
    if span <= reference * 0.005 {
        out.fill(flat_at.clamp(0.0, 1.0));
        return;
    }
    for v in out.iter_mut() {
        *v = ((*v - lo) / span).clamp(0.0, 1.0);
    }
}

use modulation::trem::fts_modulation::TransportInfo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_id_that_round_trips() {
        let mut seen = Vec::new();
        for profile in PROFILES {
            assert!(!seen.contains(&profile.id), "duplicate id {}", profile.id);
            seen.push(profile.id);
            assert_eq!(profile_by_id(profile.id).map(|p| p.id), Some(profile.id));
        }
        assert_eq!(profile_by_id("rotary"), None);
    }

    #[test]
    fn every_profile_is_in_exactly_one_family() {
        for profile in PROFILES {
            let found: Vec<_> = CATEGORIES
                .iter()
                .filter(|c| c.profiles.contains(&profile.id))
                .map(|c| c.id)
                .collect();
            assert_eq!(found.len(), 1, "{} is in {found:?}", profile.id);
        }
    }

    #[test]
    fn every_family_names_profiles_that_exist() {
        for category in CATEGORIES {
            assert!(!category.profiles.is_empty(), "{} is empty", category.id);
            for id in category.profiles {
                assert!(profile_by_id(id).is_some(), "{} names {id}", category.id);
            }
        }
    }

    /// Every chorus engine the DSP implements is reachable from the rail.
    ///
    /// This is the test that would have caught the bug this crate exists to
    /// fix: the plugin ran `EngineType::Cubic` and never switched, so four of
    /// the five engines had no way to be heard at all.
    #[test]
    fn every_chorus_engine_is_reachable() {
        for engine in [
            EngineType::Cubic,
            EngineType::Bbd,
            EngineType::Tape,
            EngineType::Orbit,
            EngineType::Juno,
        ] {
            assert!(
                PROFILES.iter().any(|p| matches!(
                    p.voicing.circuit,
                    Circuit::Delay { engine: e, .. } if e == engine
                )),
                "{engine:?} is implemented and unreachable",
            );
        }
    }

    /// …and so is every tremolo voicing and every wah source.
    #[test]
    fn every_tremolo_and_wah_variant_is_reachable() {
        for mode in [TremMode::Mono, TremMode::Stereo, TremMode::Harmonic] {
            assert!(
                PROFILES
                    .iter()
                    .any(|p| p.voicing.circuit == Circuit::Tremolo { mode }),
                "{mode:?} unreachable",
            );
        }
        for source in [WahSource::Envelope, WahSource::Both] {
            assert!(
                PROFILES.iter().any(|p| matches!(
                    p.voicing.circuit,
                    Circuit::Wah { source: s, .. } if s == source
                )),
                "{source:?} unreachable",
            );
        }
    }

    /// Vibrato is the one circuit with no dry path, and that is the whole
    /// difference between it and chorus. If it ever gains one it has stopped
    /// being vibrato and the rail is lying about having three delay circuits.
    #[test]
    fn vibrato_is_the_only_wet_only_circuit() {
        for profile in PROFILES {
            let vibrato = matches!(
                profile.voicing.circuit,
                Circuit::Delay {
                    effect: EffectType::Vibrato,
                    ..
                }
            );
            assert_eq!(
                profile.voicing.wet_only, vibrato,
                "{} disagrees about having a dry path",
                profile.id,
            );
        }
    }

    /// A flanger without feedback is a chorus with a short delay. Every
    /// profile in that family has to actually be one.
    #[test]
    fn every_flanger_is_fed_back() {
        for id in CATEGORIES
            .iter()
            .find(|c| c.id == "flanger")
            .unwrap()
            .profiles
        {
            let v = profile_by_id(id).unwrap().voicing;
            let fb = v
                .knobs
                .iter()
                .find(|k| k.role == Character::Feedback)
                .unwrap_or_else(|| panic!("{id} has no feedback control"));
            assert!(fb.at_noon > 0.3, "{id} rests at {} feedback", fb.at_noon);
        }
    }

    /// Two circuits that are the same point in the engine are one circuit
    /// with two names.
    #[test]
    fn no_two_profiles_are_the_same_voicing() {
        for (i, a) in PROFILES.iter().enumerate() {
            for b in &PROFILES[i + 1..] {
                assert_ne!(
                    a.voicing, b.voicing,
                    "{} and {} are the same circuit under two names",
                    a.id, b.id,
                );
            }
        }
    }

    /// A knob wired to a control on a chain the profile does not run would
    /// silently do nothing — the exact failure this table exists to prevent.
    #[test]
    fn every_knob_belongs_to_the_chain_its_profile_runs() {
        use Character::*;
        for profile in PROFILES {
            for knob in profile.voicing.knobs {
                let wants = match knob.role {
                    None => continue,
                    Voices | Colour | Feedback | Width => Chain::Delay,
                    Groove | Feel | Accent | Analog | Crossover => Chain::Tremolo,
                    Position | Resonance | Sensitivity | Stages | Pattern | Shape => Chain::Wah,
                };
                assert_eq!(
                    wants,
                    profile.voicing.circuit.chain(),
                    "{} wires {:?}, which lives on another chain",
                    profile.id,
                    knob.role,
                );
            }
        }
    }

    /// No profile may wire the same control twice — two knobs fighting over
    /// one field is a panel where one of them appears to do nothing.
    #[test]
    fn no_profile_wires_the_same_control_twice() {
        for profile in PROFILES {
            let mut seen = Vec::new();
            for knob in profile.voicing.knobs {
                if knob.role == Character::None {
                    continue;
                }
                assert!(
                    !seen.contains(&knob.role),
                    "{} wires {:?} twice",
                    profile.id,
                    knob.role,
                );
                seen.push(knob.role);
            }
        }
    }

    /// Every profile draws something, and nothing draws a NaN.
    #[test]
    fn every_profile_has_a_shape() {
        for profile in PROFILES {
            let mut buf = [0.0f64; 96];
            shape(profile, &Controls::default(), &mut buf);
            for v in buf {
                assert!(
                    v.is_finite() && (0.0..=1.0).contains(&v),
                    "{} drew {v}",
                    profile.id,
                );
            }
        }
    }

    /// The shape is the engine's, so a control that changes the movement has
    /// to change the drawing. Depth is the plainest case: a deeper chorus
    /// sweeps further, and if the picture does not move with it the panel is
    /// drawing a sine and calling it a chorus.
    #[test]
    fn depth_changes_what_a_chorus_draws() {
        let span = |depth: f32| {
            let mut buf = [0.0f64; 128];
            let mut raw = [0.0f64; 128];
            let profile = profile_by_id("cubic").unwrap();
            let controls = Controls {
                depth,
                ..Default::default()
            };
            shape(profile, &controls, &mut buf);
            // Normalised output always fills the box, so measure the RAW
            // excursion the engine chose rather than the drawing.
            let (mut c, mut t, mut w) = engines();
            apply(profile, &controls, &mut c, &mut t, &mut w);
            modulation::chorus::analysis::delay_cycle(
                EngineType::Cubic,
                EffectType::Chorus,
                c.rate_hz,
                c.depth,
                c.color,
                c.feedback,
                0.0,
                &mut raw,
            );
            raw.iter().cloned().fold(f64::MIN, f64::max)
                - raw.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(
            span(0.9) > span(0.2) * 2.0,
            "deeper must sweep further: {} vs {}",
            span(0.2),
            span(0.9),
        );
    }

    /// Every periodic circuit must actually draw movement.
    ///
    /// The one that caught the real bug: a fresh chain's modulator is waiting
    /// for a transport, not free-running, so setting only its rate left the
    /// tremolo perfectly still — and the panel drew a flat line for an effect
    /// whose entire job is to move.
    #[test]
    fn every_periodic_circuit_draws_movement() {
        for profile in PROFILES {
            // The envelope wah is the one honest exception; it has its own test.
            if matches!(
                profile.voicing.circuit,
                Circuit::Wah {
                    source: WahSource::Envelope,
                    ..
                }
            ) {
                continue;
            }
            let mut buf = [0.0f64; 128];
            shape(profile, &Controls::default(), &mut buf);
            let lo = buf.iter().cloned().fold(f64::MAX, f64::min);
            let hi = buf.iter().cloned().fold(f64::MIN, f64::max);
            assert!(
                hi - lo > 0.25,
                "{} drew a still line ({lo:.3}..{hi:.3}) — is its modulator running?",
                profile.id,
            );
        }
    }

    /// An envelope-driven wah has nothing periodic to draw, and drawing a
    /// wobble there would be inventing movement the circuit does not have.
    /// The flat line sits where the pedal actually is — pinning it to the top
    /// of the box would be just as wrong as drawing a wobble.
    #[test]
    fn an_envelope_wah_draws_a_flat_line_at_its_pedal_position() {
        for position in [0.2f32, 0.5, 0.8] {
            let mut buf = [0.0f64; 64];
            let controls = Controls {
                knobs: [position, 0.5, 0.5, 0.5],
                ..Default::default()
            };
            let profile = profile_by_id("wah_auto").unwrap();
            shape(profile, &controls, &mut buf);
            let lo = buf.iter().cloned().fold(f64::MAX, f64::min);
            let hi = buf.iter().cloned().fold(f64::MIN, f64::max);
            assert!((hi - lo) < 1.0e-6, "auto-wah drew movement: {lo}..{hi}");

            // …and at the height the pedal is set to.
            let (mut c, mut t, mut w) = engines();
            apply(profile, &controls, &mut c, &mut t, &mut w);
            assert!(
                (lo - w.base_position).abs() < 1.0e-6,
                "drew at {lo}, pedal is at {}",
                w.base_position,
            );
        }
    }

    #[test]
    fn the_rate_taper_round_trips() {
        for hz in [0.05f32, 0.2, 0.8, 3.0, 12.0, 20.0] {
            let back = rate_hz_from(rate_knob_from(hz));
            assert!((back - hz).abs() < hz * 0.01, "{hz} → {back}");
        }
    }

    #[test]
    fn clicking_a_family_lands_on_it_and_clicking_again_cycles_inside_it() {
        let chorus = CATEGORIES.iter().position(|c| c.id == "chorus").unwrap();
        let first = rail_click_target(profile_index("trem_opto").unwrap(), chorus);
        assert_eq!(PROFILES[first].id, "juno");
        let second = rail_click_target(first, chorus);
        assert_eq!(PROFILES[second].id, "bbd");
    }

    // ── The mapping ──────────────────────────────────────────────────────

    fn engines() -> (ChorusChain, TremChain, WahChain) {
        (ChorusChain::new(), TremChain::new(), WahChain::new())
    }

    /// Noon is the circuit as designed — the contract the whole panel rests
    /// on, same as the saturator's.
    #[test]
    fn every_knob_at_noon_is_the_voicing_and_nothing_else() {
        for profile in PROFILES {
            let (mut c, mut t, mut w) = engines();
            apply(profile, &Controls::default(), &mut c, &mut t, &mut w);
            for knob in profile.voicing.knobs {
                let got = match knob.role {
                    Character::None => continue,
                    Character::Voices => c.num_voices as f32,
                    Character::Colour => c.color as f32,
                    Character::Feedback => c.feedback as f32,
                    Character::Width => c.width as f32,
                    Character::Groove => t.groove as f32,
                    Character::Feel => t.feel as f32,
                    Character::Accent => t.accent as f32,
                    Character::Analog => ANALOG_STYLES
                        .iter()
                        .position(|s| *s == t.analog_l.style)
                        .unwrap() as f32,
                    Character::Crossover => t.tremolo_l.crossover_freq as f32,
                    Character::Position => w.base_position as f32,
                    Character::Resonance => w.filter_l.q as f32,
                    Character::Sensitivity => w.sensitivity as f32,
                    Character::Stages => w.filter_l.stages as f32,
                    Character::Pattern => w.pattern_amount as f32,
                    Character::Shape => WAH_SHAPES
                        .iter()
                        .position(|s| *s == w.filter_l.mode)
                        .unwrap() as f32,
                };
                let tol = (knob.at_noon.abs() * 0.02).max(0.01);
                assert!(
                    (got - knob.at_noon).abs() <= tol,
                    "{}'s {:?} rests at {got}, not {}",
                    profile.id,
                    knob.role,
                    knob.at_noon,
                );
            }
        }
    }

    /// Switching profiles must not inherit the last one's settings. A
    /// flanger's feedback left behind on a chorus is a howl nobody asked for.
    #[test]
    fn switching_profiles_does_not_inherit_the_last_ones_feedback() {
        let (mut c, mut t, mut w) = engines();
        let hot = Controls {
            knobs: [1.0, 1.0, 1.0, 1.0],
            ..Default::default()
        };
        apply(
            profile_by_id("flanger").unwrap(),
            &hot,
            &mut c,
            &mut t,
            &mut w,
        );
        assert!(c.feedback > 0.5, "the flanger should be fed back");

        apply(
            profile_by_id("juno").unwrap(),
            &Controls::default(),
            &mut c,
            &mut t,
            &mut w,
        );
        assert_eq!(c.feedback, 0.0, "the Juno inherited the flanger's feedback");
    }

    /// A wet-only circuit ignores the mix knob at every position, rather than
    /// offering one that quietly does nothing.
    #[test]
    fn vibrato_ignores_the_mix_knob() {
        for mix in [0.0, 0.25, 0.5, 1.0] {
            let (mut c, mut t, mut w) = engines();
            apply(
                profile_by_id("vibrato").unwrap(),
                &Controls {
                    mix,
                    ..Default::default()
                },
                &mut c,
                &mut t,
                &mut w,
            );
            assert_eq!(c.mix, 1.0, "vibrato went dry at mix={mix}");
        }
    }

    /// Every knob, at both ends, on every profile — nothing may leave the
    /// range its chain clamps to, and nothing may produce a non-finite value.
    #[test]
    fn no_knob_position_puts_a_chain_out_of_range() {
        for profile in PROFILES {
            for end in [0.0f32, 0.5, 1.0] {
                let (mut c, mut t, mut w) = engines();
                apply(
                    profile,
                    &Controls {
                        rate: end,
                        depth: end,
                        mix: end,
                        knobs: [end; 4],
                    },
                    &mut c,
                    &mut t,
                    &mut w,
                );
                let id = profile.id;
                assert!(
                    (1..=4).contains(&c.num_voices),
                    "{id} voices {}",
                    c.num_voices
                );
                assert!((0.0..=1.0).contains(&c.color), "{id} colour {}", c.color);
                assert!(
                    (0.0..1.0).contains(&c.feedback),
                    "{id} feedback {}",
                    c.feedback
                );
                assert!((0.0..=1.0).contains(&c.width), "{id} width {}", c.width);
                assert!((-1.0..=1.0).contains(&t.groove), "{id} groove {}", t.groove);
                assert!((-1.0..=1.0).contains(&t.feel), "{id} feel {}", t.feel);
                assert!((-1.0..=1.0).contains(&t.accent), "{id} accent {}", t.accent);
                assert!(
                    (1.0..=20.0).contains(&w.filter_l.q),
                    "{id} q {}",
                    w.filter_l.q
                );
                assert!((1..=4).contains(&w.filter_l.stages), "{id} stages");
                assert!(
                    (0.0..=1.0).contains(&w.base_position),
                    "{id} position {}",
                    w.base_position,
                );
                let rate = rate_hz_from(end);
                assert!(rate.is_finite() && (RATE_MIN_HZ..=RATE_MAX_HZ).contains(&rate));
            }
        }
    }

    /// The two enum knobs must be able to reach every choice — a style you
    /// cannot select is a style that does not exist.
    #[test]
    fn the_enum_knobs_reach_every_choice() {
        let mut styles = Vec::new();
        let mut shapes = Vec::new();
        for step in 0..=64 {
            let v = step as f32 / 64.0;
            let (mut c, mut t, mut w) = engines();
            apply(
                profile_by_id("trem_opto").unwrap(),
                &Controls {
                    knobs: [0.5, 0.5, 0.5, v],
                    ..Default::default()
                },
                &mut c,
                &mut t,
                &mut w,
            );
            if !styles.contains(&t.analog_l.style) {
                styles.push(t.analog_l.style);
            }
            let (mut c, mut t, mut w) = engines();
            apply(
                profile_by_id("wah_auto").unwrap(),
                &Controls {
                    knobs: [0.5, 0.5, 0.5, v],
                    ..Default::default()
                },
                &mut c,
                &mut t,
                &mut w,
            );
            if !shapes.contains(&w.filter_l.mode) {
                shapes.push(w.filter_l.mode);
            }
        }
        assert_eq!(styles.len(), ANALOG_STYLES.len(), "reached {styles:?}");
        assert_eq!(shapes.len(), WAH_SHAPES.len(), "reached {shapes:?}");
    }
}
