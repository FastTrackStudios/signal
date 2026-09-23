//! What a native block's parameter values *mean* — the missing half that
//! lets an audio-side `RigBlock` become a domain `Node`.
//!
//! # The problem this solves
//!
//! A [`RigBlock`] parameter is a real value in its backend's own units:
//! `threshold` is −18 dB, `attack` is 10 ms, `b1_freq` is 1000 Hz. A domain
//! [`BlockParameter`] is a 0..=1 knob position plus the
//! [`ParameterRange`] that gives it meaning, and that split is right — a
//! normalized number is what automation, modulation and MIDI learn all want.
//!
//! Crossing from one to the other needs the range, and without it the value
//! is silently clamped: `("rate", "2.5")` becomes 1.0, which is the bug
//! ranges were introduced to fix. So this module answers, for any native
//! block type and parameter name, what its range is.
//!
//! # Where the numbers come from
//!
//! **Not from a table.** Every native declares its own `min`/`max` through
//! `PluginInstance::params()`, so those are read from the DSP itself — one
//! instance per block type, built once and cached. A hand-written table of
//! 800-odd parameters would be wrong within a month, and wrong silently.
//!
//! **Taper and unit are declared here**, by name pattern, because they are
//! not in the DSP's metadata and cannot be derived from it: `min`/`max` say
//! a parameter runs 10 to 30000, not that it is a frequency that must be
//! heard logarithmically. The patterns are grounded in what the natives
//! actually expose (`*_freq`, `*_q`, `attack`, `rate`, …); anything
//! unmatched is linear and unitless, which is the right answer for a `mix`,
//! a `depth` or a `ratio`.
//!
//! # Taper is part of the stored value
//!
//! A normalized position means what its range says it means, so changing a
//! parameter's taper later *moves* every stored value of it — 632 Hz sits at
//! 0.5 on a logarithmic 20 Hz–20 kHz range and at 0.03 on a linear one.
//! Declaring the taper is therefore not cosmetic and not deferrable: it has
//! to be right before values are stored, or the next change to it is a
//! migration. That is why the patterns below are declared now rather than
//! left to whoever first draws a knob.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use signal_proto::block::BlockType;
use signal_proto::{ParameterRange, Taper, Unit};

use crate::rig::RigBlock;

/// One native's parameters, as the DSP reports them: `(name, min, max,
/// default)`.
type Spans = Vec<(String, f32, f32, f32)>;

fn cache() -> &'static Mutex<HashMap<BlockType, Spans>> {
    static CACHE: OnceLock<Mutex<HashMap<BlockType, Spans>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The spans this block type's native DSP declares.
///
/// Builds one instance per type, at a nominal 48 kHz, and keeps it. A
/// parameter's span does not depend on sample rate — and this is a control-
/// path lookup, never touched from the audio thread.
fn spans(block_type: BlockType) -> Spans {
    if let Some(found) = cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&block_type).cloned())
    {
        return found;
    }
    let block = RigBlock::of_type(block_type);
    let spans: Spans = super::build_native(&block, 48_000)
        .map(|mut inst| {
            inst.params()
                .into_iter()
                .map(|p| (p.name, narrow(p.min), narrow(p.max), narrow(p.default)))
                .collect()
        })
        .unwrap_or_default();
    if let Ok(mut c) = cache().lock() {
        c.insert(block_type, spans.clone());
    }
    spans
}

/// A parameter bound from the DSP's `f64` to the domain's `f32`.
///
/// The narrowing is inherent, not incidental: `PluginInstance::params()`
/// reports `f64` and a [`ParameterRange`] is `f32`, because a normalized
/// position does not need more than 24 bits of mantissa and a rig holds
/// hundreds of thousands of them. In one function so it is one decision
/// rather than three casts, and so the loss is stated where it happens.
///
/// A bound beyond `f32`'s range saturates rather than becoming an infinity
/// that would poison every `normalize` through it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "the whole purpose of this function, documented above"
)]
fn narrow(bound: f64) -> f32 {
    bound.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}

/// Ranges for the control-rate blocks, which have no `PluginInstance` to ask.
///
/// An envelope, an LFO and an arpeggiator are not processors — they are
/// compiled into the mod engine by `node_render::modmatrix`, which reads
/// their parameters directly. So there is no DSP metadata to read and these
/// are declared by hand, against that code.
///
/// The reason this cannot be folded into the name patterns: an envelope's
/// `attack` is in **seconds** (`AdsrParams::attack_s = v`) while a
/// compressor's `attack` is in **milliseconds**. Same name, same kind of
/// control, three orders of magnitude apart. Whatever a parameter is called,
/// only the code reading it knows what the number means — which is the whole
/// argument for keying a range by block type and not by name.
fn declared(block_type: BlockType, param: &str) -> Option<ParameterRange> {
    let seconds = |max: f32| ParameterRange {
        min: 0.0,
        max,
        taper: Taper::Linear,
        unit: Unit::Seconds,
    };
    let unit_interval = ParameterRange::default();
    let steps = |count: u32, max: f32| ParameterRange {
        min: 0.0,
        max,
        taper: Taper::Stepped { steps: count },
        unit: Unit::None,
    };

    match block_type {
        BlockType::Envelope | BlockType::MultisegEnvelope => match param {
            // Linear rather than logarithmic on purpose: these start at zero
            // — an instant attack is the common case — and a logarithmic
            // range needs a positive minimum. Declaring log here would fall
            // back to linear anyway and leave the taper field lying about it.
            "attack" | "decay" | "release" => Some(seconds(10.0)),
            "sustain" => Some(unit_interval),
            _ => None,
        },
        BlockType::Lfo => match param {
            // The clamp `modmatrix` applies, declared.
            "rate" => Some(ParameterRange::logarithmic(0.01, 40.0, Unit::Hz)),
            "wave" => Some(steps(5, 4.0)),
            "sync_beats" => Some(ParameterRange::linear(0.0, 16.0, Unit::Beats)),
            "retrigger" => Some(steps(2, 1.0)),
            _ => None,
        },
        BlockType::Arpeggiator => match param {
            "on" => Some(steps(2, 1.0)),
            "steps" => Some(steps(65, 64.0)),
            "step_beats" => Some(ParameterRange::linear(0.0, 4.0, Unit::Beats)),
            // Per-step state: 64 steps × 3 fields, by pattern rather than
            // by enumeration.
            _ => step_range(param),
        },
        _ => None,
    }
}

/// One arpeggiator step's `on` / `vel` / `gate`.
fn step_range(param: &str) -> Option<ParameterRange> {
    let rest = param.strip_prefix("step")?;
    let (index, field) = rest.split_once('_')?;
    index.parse::<u32>().ok()?;
    match field {
        "on" => Some(ParameterRange {
            min: 0.0,
            max: 1.0,
            taper: Taper::Stepped { steps: 2 },
            unit: Unit::None,
        }),
        "vel" => Some(ParameterRange::stepped(1.0, 127.0, 127, Unit::None)),
        "gate" => Some(ParameterRange::linear(0.05, 1.0, Unit::None)),
        _ => None,
    }
}

/// The range for one parameter of a native block, or `None` when this block
/// type has no native DSP or does not declare that parameter.
///
/// `None` is the honest answer, not a default: a caller lifting a value it
/// cannot range must say so rather than clamp it.
#[must_use]
pub fn range_of(block_type: BlockType, param: &str) -> Option<ParameterRange> {
    if let Some(range) = declared(block_type, param) {
        return Some(range);
    }
    let spans = spans(block_type);
    let (_, min, max, _) = spans.iter().find(|(name, ..)| name == param)?;
    let (taper, unit) = taper_and_unit(param);
    Some(ParameterRange {
        min: *min,
        max: *max,
        taper,
        unit,
    })
}

/// What a parameter sits at when nobody has set it — in its own units, as
/// the DSP declares it.
///
/// A rig only stores the parameters it has an opinion about, so a patch that
/// bends one the chain never set needs somewhere for the value to land. This
/// is that starting point: the DSP's own default, not a guess and not zero,
/// which for a reverb algorithm or a delay style is a different effect.
#[must_use]
pub fn default_of(block_type: BlockType, param: &str) -> Option<f32> {
    spans(block_type)
        .iter()
        .find(|(name, ..)| name == param)
        .map(|(.., default)| *default)
}

/// Every parameter this block type's native DSP declares, with its range.
#[must_use]
pub fn ranges_of(block_type: BlockType) -> Vec<(String, ParameterRange)> {
    spans(block_type)
        .into_iter()
        .map(|(name, min, max, _)| {
            let (taper, unit) = taper_and_unit(&name);
            (
                name,
                ParameterRange {
                    min,
                    max,
                    taper,
                    unit,
                },
            )
        })
        .collect()
}

/// How a parameter is heard, by what it is called.
///
/// Matched on the name's tail so band- and slot-prefixed parameters
/// (`b7_freq`, `dband3_q`, `r2_predelay`) fall out for free — there are 24 EQ
/// bands and 8 dynamic bands, and they are the same controls.
///
/// Frequency, time and Q are ratios to the ear, so they take
/// [`Taper::Logarithmic`]; the range's own guard falls back to linear where a
/// span touches zero, which is why `high_pass` (0 Hz upward) is safe to
/// declare as logarithmic. Decibels are *already* logarithmic in the ear and
/// so take [`Taper::Linear`] — a dB range with a log taper is doubly bent.
fn taper_and_unit(param: &str) -> (Taper, Unit) {
    let name = param.rsplit('_').next().unwrap_or(param);
    let ends = |suffix: &str| param == suffix || param.ends_with(&format!("_{suffix}"));

    // Frequency.
    if ends("freq") || ends("cutoff") || param == "low_cut" || param == "high_cut" || param == "high_pass" || param == "low_pass" {
        return (Taper::Logarithmic, Unit::Hz);
    }
    // A side-chain listen band is a frequency too, named for its end.
    if param.contains("side_lo") || param.contains("side_hi") {
        return (Taper::Logarithmic, Unit::Hz);
    }
    // Modulation and tremolo speeds.
    if ends("rate") {
        return (Taper::Logarithmic, Unit::Hz);
    }
    // Resonance and compression ratio: both read as ratios, both want log.
    if ends("q") || ends("ratio") {
        return (Taper::Logarithmic, Unit::Ratio);
    }
    // Decibels — linear taper on purpose (see the doc comment).
    if ends("gain") || ends("threshold") || ends("thr") || param.starts_with("gain_db") || param == "level" {
        return (Taper::Linear, Unit::Decibels);
    }
    // Times. The natives are in milliseconds except `decay_time`, which the
    // reverb declares in seconds (0.05..60) — a 60 ms reverb tail would be
    // a different effect entirely.
    if param == "decay_time" {
        return (Taper::Logarithmic, Unit::Seconds);
    }
    if matches!(
        name,
        "attack" | "release" | "atk" | "rel" | "predelay" | "hold" | "smooth" | "ms"
    ) || ends("time")
    {
        return (Taper::Logarithmic, Unit::Milliseconds);
    }
    // Pitch offsets, in semitones.
    if param.contains("shift") || ends("detune") || ends("semis") {
        return (Taper::Linear, Unit::Semitones);
    }
    // Everything else: a mix, a depth, a mode index, a style. Linear and
    // unitless is what those are.
    (Taper::Linear, Unit::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spans are the DSP's, not a copy of them — so this asserts against
    /// what the compressor actually declares rather than a literal that
    /// could drift away from it.
    #[test]
    fn spans_come_from_the_dsp() {
        let threshold = range_of(BlockType::Compressor, "threshold").expect("declared");
        assert!(
            threshold.min < 0.0 && threshold.max <= 0.0,
            "a threshold runs from quiet to full scale, got {}..{}",
            threshold.min,
            threshold.max
        );
        assert_eq!(threshold.unit, Unit::Decibels);

        assert!(
            range_of(BlockType::Compressor, "not_a_parameter").is_none(),
            "an undeclared parameter has no range, rather than a default one"
        );
    }

    /// A block type with no native DSP and no declaration has no ranges —
    /// and says so.
    #[test]
    fn a_type_without_native_dsp_has_no_ranges() {
        assert!(range_of(BlockType::Cabinet, "anything").is_none());
    }

    /// The case that proves a range cannot be inferred from a parameter's
    /// name: an envelope's attack is in seconds, a compressor's is in
    /// milliseconds, and both are called `attack`.
    #[test]
    fn the_same_name_means_different_things_on_different_blocks() {
        let env = range_of(BlockType::Envelope, "attack").expect("declared");
        assert_eq!(env.unit, Unit::Seconds);
        assert!((env.max - 10.0).abs() < f32::EPSILON);

        let comp = range_of(BlockType::Compressor, "attack").expect("declared");
        assert_eq!(comp.unit, Unit::Milliseconds);
        assert!(comp.max > 50.0, "a compressor attack runs to {}", comp.max);
    }

    /// An envelope's times start at zero, so they are linear — a
    /// logarithmic range needs a positive minimum, and an instant attack is
    /// the common case, not an edge one.
    #[test]
    fn an_envelope_time_holds_zero_exactly() {
        let attack = range_of(BlockType::Envelope, "attack").expect("declared");
        assert_eq!(attack.taper, Taper::Linear);
        assert!(
            attack.denormalize(attack.normalize(0.0)) < f32::EPSILON,
            "an instant attack stays instant"
        );
        for seconds in [0.0, 0.005, 0.5, 4.0, 10.0] {
            let back = attack.denormalize(attack.normalize(seconds));
            assert!((back - seconds).abs() < 0.001, "{seconds} s → {back} s");
        }
    }

    /// The arpeggiator's per-step parameters are 64 steps × 3 fields, so
    /// they are matched rather than listed.
    #[test]
    fn arpeggiator_steps_are_matched_by_pattern() {
        for step in [0, 7, 63] {
            assert!(
                range_of(BlockType::Arpeggiator, &format!("step{step}_on")).is_some(),
                "step {step} on"
            );
            let vel = range_of(BlockType::Arpeggiator, &format!("step{step}_vel"))
                .expect("velocity is declared");
            assert!(
                (vel.min - 1.0).abs() < f32::EPSILON,
                "MIDI velocity 0 is a note-off"
            );
            assert!((vel.max - 127.0).abs() < f32::EPSILON);
        }
        assert!(range_of(BlockType::Arpeggiator, "step_nonsense").is_none());
        assert!(range_of(BlockType::Arpeggiator, "stepx_on").is_none());
    }

    /// The reason this module exists: a real value has to survive the trip
    /// into a normalized position and back.
    #[test]
    fn a_real_value_round_trips_through_its_range() {
        let freq = range_of(BlockType::Eq, "b1_freq").expect("the EQ declares its bands");
        assert_eq!(freq.unit, Unit::Hz);
        assert_eq!(freq.taper, Taper::Logarithmic);

        for hz in [80.0, 212.0, 1400.0, 5500.0] {
            let back = freq.denormalize(freq.normalize(hz));
            assert!(
                (back - hz).abs() < hz * 0.001,
                "{hz} Hz came back as {back}"
            );
        }
    }

    /// Frequencies are heard as ratios: the midpoint of a log range is the
    /// geometric mean, not the arithmetic one. This is the difference between
    /// a usable filter knob and one whose whole bottom octave is unreachable.
    #[test]
    fn a_frequency_is_logarithmic_and_a_mix_is_not() {
        let freq = range_of(BlockType::Eq, "b1_freq").expect("declared");
        let mid = freq.denormalize(0.5);
        assert!(
            (mid - (freq.min * freq.max).sqrt()).abs() < mid * 0.01,
            "midpoint {mid} is not the geometric mean"
        );

        let mix = range_of(BlockType::Reverb, "mix").expect("declared");
        assert_eq!(mix.taper, Taper::Linear);
        assert_eq!(mix.unit, Unit::None);
    }

    /// Times differ by three orders of magnitude between parameters that
    /// share a name, so the unit is not guessable from the number. Pinned
    /// because getting it wrong shows the player "10 s" for a 10 ms attack.
    #[test]
    fn times_carry_the_unit_their_dsp_uses() {
        let attack = range_of(BlockType::Compressor, "attack").expect("declared");
        assert_eq!(attack.unit, Unit::Milliseconds);

        let decay = range_of(BlockType::Reverb, "decay_time").expect("declared");
        assert_eq!(decay.unit, Unit::Seconds, "a reverb tail is not in ms");
    }

    /// Band-prefixed parameters are the same control 24 times over, so the
    /// patterns match on the tail rather than listing them.
    #[test]
    fn band_prefixes_fall_out_of_the_patterns() {
        for band in [1, 7, 24] {
            let freq = range_of(BlockType::Eq, &format!("b{band}_freq"))
                .unwrap_or_else(|| panic!("band {band} declares a frequency"));
            assert_eq!(freq.unit, Unit::Hz, "band {band}");
            assert_eq!(freq.taper, Taper::Logarithmic, "band {band}");

            let gain = range_of(BlockType::Eq, &format!("b{band}_gain")).expect("gain");
            assert_eq!(gain.unit, Unit::Decibels, "band {band}");
            assert_eq!(
                gain.taper,
                Taper::Linear,
                "dB is already logarithmic in the ear"
            );
        }
    }

    /// Whatever a native declares, every one of its parameters gets a range —
    /// otherwise a lift of that block would lose the ones that did not.
    #[test]
    fn every_declared_parameter_of_every_native_has_a_range() {
        let mut total = 0;
        for &block_type in signal_proto::block::ALL_BLOCK_TYPES {
            if !super::super::native_dsp_available(block_type) {
                continue;
            }
            let ranges = ranges_of(block_type);
            for (name, range) in &ranges {
                assert!(
                    range.max >= range.min,
                    "{block_type:?}.{name} has an inverted span"
                );
                let default = default_of(block_type, name)
                    .unwrap_or_else(|| panic!("{block_type:?}.{name} has no default"));
                assert!(
                    default >= range.min && default <= range.max,
                    "{block_type:?}.{name} defaults to {default}, outside {}..{}",
                    range.min,
                    range.max
                );
                assert!(
                    range_of(block_type, name).is_some(),
                    "{block_type:?}.{name} is listed but not findable"
                );
                total += 1;
            }
        }
        assert!(
            total > 500,
            "the natives declare hundreds of parameters; got {total}"
        );
    }
}
