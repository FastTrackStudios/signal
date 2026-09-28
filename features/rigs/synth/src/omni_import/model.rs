//! The parsed **patch model** (`OmniPatch` / `OmniLayer`) and the `AmberPart`
//! element walk that fills it.

use signal_proto::block::BlockType;

use super::parse_xml;
use super::xml::XmlNode;

// ── Patch model ──────────────────────────────────────────────────────────────

/// One layer extracted from a patch (a `VOICE` + its `MULTISAMPLE`).
#[derive(Debug, Clone, Default)]
pub struct OmniLayer {
    /// Soundsource name from `MS_IM_0 name=` (empty ⇒ synth mode / none).
    pub soundsource: String,
    /// Soundsource library from `MS_IM_0 library=`.
    pub ss_library: String,
    /// `FILTER NameStr=` display name (e.g. "LPF UVI 3").
    pub filter_name: String,
    /// `FILTER para=` ≠ 0 ⇒ the two filters run in parallel.
    pub filter_parallel: bool,
    /// `FILTER act=` ≠ 0 ⇒ the filter section is engaged.
    pub filter_active: bool,
    /// Filter 1's effective knob setting: the section's master cutoff
    /// (`freq`) plus filter 1's own offset (`freq1`) — see
    /// [`omni_filter_setting`]. Resonance is `res`.
    pub filter_freq: f32,
    /// Filter 1's algorithm (`type1`; see [`filter_model`]).
    pub filter_type1: Option<f32>,
    /// Filter 1's own switch (`act1`); off, only filter 2 shapes the sound.
    pub filter1_on: bool,
    /// Filter 2's algorithm (`type2`), when filter 2 is on.
    pub filter_type2: Option<f32>,
    pub filter_res: f32,
    /// `OSC level` (normalized).
    pub level: f32,
    /// The layer's fixed transposition in semitones, from `OSC oct` / `semi`
    /// / `tune` / `tuneFine` (measured: `oct` 0.04 = 0, each 0.02 lower is an
    /// octave UP; `semi` 0.48 = 0, each 0.02 lower a semitone up; `tune`
    /// ±48 semitones about 0.5; `tuneFine` ±1 semitone about 0.5).
    pub transpose: f32,
    /// `OSC pan`, 0..1 (0.5 centre): a balance law (measured: 0.25 puts
    /// the right side 6 dB down, 0 silences it) — the Amp's own law.
    pub pan: f32,
    /// The amp and filter envelopes' breakpoints (`AENV` / `FENV`), as
    /// [`OmniModEnv::points`]; they sustain at the penultimate point.
    pub amp_points: Vec<(f32, f32, f32, bool)>,
    pub filter_points: Vec<(f32, f32, f32, bool)>,
    /// Glide on (`OSC portAct`); its time is the part's (`OmniPatch::glide_s`).
    pub glide: bool,
    /// Unison: voice count (1..8), detune 0..1, width 0..1, plus the
    /// octave / analog / drift mode amounts (0..1).
    pub unison_count: u32,
    pub unison_detune: f32,
    pub unison_width: f32,
    pub unison_octave: f32,
    pub unison_analog: f32,
    pub unison_drift: f32,
    /// FM modulator waveform morph 0..1 (`OSC fmwf`).
    pub fm_shape: f32,
    /// The oscillator's own waveform, 0..1 (`OSC type`).
    ///
    /// Without this a synthesis-mode patch imported with whatever waveform the
    /// wavetable block defaults to, which is the difference between a PHAT
    /// bass and a sine. Note Omnisphere's `type` is a *selector* over its wave
    /// list while our `shape` is a continuous sine→triangle→saw→square morph,
    /// so passing it straight through is a first approximation, not a match —
    /// calibrating the two axes is its own job.
    pub osc_wave: f32,
    /// `OSC kind=` (kept for reference: measured against Omnisphere it does
    /// not switch the waveform — `waves` does).
    pub osc_kind: u32,
    /// The layer's power switch (`AENVPARAMS onOff`). Measured: in the init
    /// part only layer A's is on, and only layer A sounds; switching B's on
    /// brings B in. An off layer is silent whatever else it holds.
    pub enabled: bool,
    /// `WAVES wf0= wf1=`: the oscillator's two waves, as Omnisphere names
    /// them (`~BundleArchives/<category>/…/<wave>.stmwf`). Measured: `wf0` is
    /// the waveform heard, and Shape (`OSC pdepth`) morphs it into `wf1`.
    pub waves: Option<(String, String)>,
    /// `OSC pdepth` — Shape: the wf0 → wf1 morph (0 = all wf0). Measured:
    /// 0.5 over Jupiter 8 Saw → Square gave the 50/50 mix's even harmonics.
    pub osc_shape: f32,
    /// `OSC pwidth` — Symmetry, a waveform warp (0 = none).
    pub osc_symmetry: f32,
    /// The FM and AM (ring) modulators' waves (`FMWAVES` / `AMWAVES`).
    pub fm_waves: Option<(String, String)>,
    pub am_waves: Option<(String, String)>,
    /// Amplitude AHDSR `(attack_s, decay_s, sustain, release_s)`.
    pub amp_env: Option<(f32, f32, f32, f32)>,
    /// Filter AHDSR `(attack_s, decay_s, sustain, release_s)`.
    pub filter_env: Option<(f32, f32, f32, f32)>,
    /// Filter-envelope → cutoff depth (signed; `FILTER envdpth`, inverted by
    /// `envdpthinv`).
    pub filter_env_depth: f32,
    /// The filter envelope's velocity sensitivity (`FENVPARAMS velsens`):
    /// measured linear — at 1 it scales by `vel/127`.
    pub filter_env_velsens: f32,
    /// Filter 2, when engaged (`act2`): its effective knob setting (master
    /// `freq` plus its `freq2` offset) and effective resonance (master `res`
    /// plus its `res2` offset).
    pub filter2: Option<(f32, f32)>,
    /// `FILTER bal` — in parallel, filter 1 weighs `1 − bal²` and filter 2
    /// `1 − (1 − bal)²` (measured; series ignores it).
    pub filter_balance: f32,
    /// FM depth 0..1 (`OSC fm`).
    pub fm_depth: f32,
    /// Ring/AM mix 0..1 (`OSC am`).
    pub ring_mix: f32,
    /// Active Harmonia voices: (level, interval semitones, pan −1..1, shape).
    pub harmonia: Vec<(f32, f32, f32, f32)>,
    /// Waveshaper when engaged: (drive, crush, reduce, mix).
    pub shaper: Option<(f32, f32, f32, f32)>,
    /// Dual Frequency Shifter when engaged: (`hz_a`, `mix_a`, `hz_b`, `mix_b`, parallel).
    pub dfs: Option<(f32, f32, f32, f32, bool)>,
    /// Layer FX rack: the four `EFFMODULE Type=` names ("No Effect" ⇒ empty).
    pub fx: Vec<String>,
}

/// One part LFO, measured against the real plugin (pitch-tracked).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OmniLfo {
    /// `rate` knob 0..1 (see [`OmniLfo::rate_hz`]).
    pub rate: f32,
    /// `type`: a wave index in steps of 0.02 (see [`OmniLfo::wave`]).
    pub wave_type: f32,
    pub sync: bool,
    pub retrigger: bool,
    /// `swing`: the LFO's amplitude, 0..1 (≈ linear).
    pub swing: f32,
    /// `unidir`: output 0..+1 instead of ±1.
    pub unipolar: bool,
}

impl OmniLfo {
    /// Free-running rate: `48.3 Hz · rate³` (measured 0.2 → 0.39 Hz,
    /// 0.4642 → 4.76 Hz, 0.8 → 25 Hz), floored at the 0.01 Hz our LFOs
    /// run down to (rate 0 is a stopped LFO).
    #[must_use]
    pub fn rate_hz(&self) -> f32 {
        (48.3 * self.rate.clamp(0.0, 1.0).powi(3)).max(0.01)
    }

    /// Signal's LFO wave index (0 sine, 1 triangle, 2 saw, 3 square, 4 S&H,
    /// 5 falling saw) for Omnisphere's `type` (measured: 0 sine, 0.02 smooth
    /// random, 0.04 triangle, 0.06 square, 0.08 rising saw, 0.10 falling
    /// saw, 0.12 stepped, 0.14 sine, 0.16 random). The random shapes read as
    /// sample-and-hold for now.
    #[must_use]
    pub fn wave(&self) -> u32 {
        match (self.wave_type * 50.0).round() as u32 {
            2 => 1,
            3 => 3,
            4 => 2,
            5 => 5,
            1 | 6 | 8 => 4,
            _ => 0,
        }
    }
}

/// One part mod envelope — a free-running breakpoint envelope (measured:
/// it restarts on note-on and runs its points whatever the key does).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OmniModEnv {
    /// `(time, level, curve k, step)`: time in seconds (or beats when
    /// synced) — one `t` unit is 100 of either; `k = 42·(0.5 − c)` (0 =
    /// linear) and `step` (the point's `s` bit 0) shape the segment that
    /// leaves the point.
    pub points: Vec<(f32, f32, f32, bool)>,
    pub looping: bool,
    pub synced: bool,
    pub velsens: f32,
}

/// One mod-matrix route (`sourceN` → `targetN`).
///
/// Measured: the route adds `lo + (hi − lo)·source` to the target in the
/// target knob's own units (a cutoff route moves the filter *setting*, not
/// octaves), unipolar, so `depth` is `hi − lo` and `offset` is `lo`.
#[derive(Debug, Clone)]
pub struct OmniModRoute {
    pub source: String,
    pub target: String,
    pub depth: f32,
    pub offset: f32,
}

/// A parsed `.prt_omn` patch.
#[derive(Debug, Clone, Default)]
pub struct OmniPatch {
    pub name: String,
    pub library: String,
    /// Browser tags from `ENTRYDESCR ATTRIB_VALUE_DATA` (`key=value` pairs).
    pub tags: Vec<(String, String)>,
    pub layers: Vec<OmniLayer>,
    /// Common FX rack module names.
    pub common_fx: Vec<String>,
    /// Aux FX rack module names.
    pub aux_fx: Vec<String>,
    pub mod_routes: Vec<OmniModRoute>,
    /// Glide time (s) for the layers with glide on: `SYNTHENG portV2`,
    /// measured ≈ `12.2 s · v^2.12` (0.2 → 0.4 s, 0.4 → 1.75 s).
    pub glide_s: f32,
    /// The part's six mod envelopes (`MODENV`, with `MODENVPARAMS` then
    /// five `MOD_ENV2_2` for their settings).
    pub mod_envs: Vec<OmniModEnv>,
    /// The part's nine LFOs from `LFO_SET` (LFO9 is the vibrato LFO every
    /// factory patch routes to tuneFine).
    pub lfos: Vec<OmniLfo>,
    pub arp_on: bool,
    /// Arp pattern from `ARPSEQ2`: `(on, velocity, gate 0..1)` per step.
    pub arp_steps: Vec<(bool, u8, f32)>,
    /// Step length in beats (from tick spacing vs `TICKSPERQUARTER`).
    pub arp_step_beats: f32,
}

fn rack_types(rack: &XmlNode) -> Vec<String> {
    // Slot order is preserved; a bypassed module (`Active` ≈ 0) blanks its
    // slot so it never realizes live DSP (a disengaged reverb must stay
    // silent). Empty/"No Effect" slots are dropped downstream by name.
    rack.children_tagged("EFFMODULE")
        .map(|m| {
            let active = m.attr("Active").map_or(0.0, super::omni_num) > 0.5;
            if active {
                m.attr("Type").unwrap_or("").to_string()
            } else {
                String::new()
            }
        })
        .collect()
}

/// Map an Omnisphere FX-unit name onto the nearest **native** DSP block, when
/// one exists ([`crate::omni`]'s racks are otherwise placeholder pass-throughs).
/// `None` ⇒ keep a placeholder slot.
///
/// Keyword-ordered because several names carry two cues: "Chorus Echo" is a
/// tape *echo* with modulation (→ Delay, not Chorus), "Multiband Distortion"
/// is drive not EQ. The high-frequency factory units (verbs, echoes, choruses,
/// EQs, compressors, tremolos, phasers) all resolve; the exotic ones (Imager,
/// Retroplex, amp/console sims, backward FX) stay placeholders until they have
/// DSP. Parameter fidelity is a later pass — this only picks the block type, so
/// units realize with sensible native defaults.
/// The local file for a wave a patch names (`~BundleArchives/<rel>.stmwf`):
/// the extraction's `.wav` of it (else the raw `.stmwf`) under
/// `FTS_OMNI_WAVETABLES`, else `$FTS_SAMPLED_ROOT/Synth/Omnisphere-Wavetables`,
/// else the studio machine's mount. `None` when neither file is there.
#[must_use]
pub fn wavetable_path(name: &str) -> Option<std::path::PathBuf> {
    let rel = name.strip_prefix("~BundleArchives/").unwrap_or(name);
    let root = std::env::var("FTS_OMNI_WAVETABLES")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("FTS_SAMPLED_ROOT")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|r| std::path::PathBuf::from(r).join("Synth/Omnisphere-Wavetables"))
        })
        .unwrap_or_else(|| {
            std::path::PathBuf::from("/run/media/AudioHaven/Sampled/Synth/Omnisphere-Wavetables")
        });
    let raw = root.join(rel);
    let wav = raw.with_extension("wav");
    [wav, raw].into_iter().find(|p| p.exists())
}

pub fn classify_effect(name: &str) -> Option<BlockType> {
    let k = name.to_ascii_lowercase();
    let has = |subs: &[&str]| subs.iter().any(|s| k.contains(s));
    Some(if has(&["echo", "delay"]) {
        BlockType::Delay
    } else if has(&["verb", "innerspace", "shimmer"]) {
        BlockType::Reverb
    } else if has(&["flanger"]) {
        BlockType::Flanger
    } else if has(&["phaser"]) {
        BlockType::Phaser
    } else if has(&["vibrato"]) {
        BlockType::Vibrato
    } else if has(&["chorus", "ensemble", "solina"]) {
        BlockType::Chorus
    } else if has(&["tremolo"]) {
        BlockType::Trem
    } else if has(&["compressor", "limiter", "leveling amp", "1176"]) {
        BlockType::Compressor
    } else if has(&[
        "distortion",
        "overdrive",
        "saturator",
        "fuzz",
        "smasher",
        "mean machine",
        "flame",
        "slammer",
        "drive",
    ]) {
        BlockType::Drive
    } else if has(&[" eq", "-band", "parametric", "graphic"]) {
        BlockType::Eq
    } else if has(&["filter", "resonators"]) {
        BlockType::Filter
    } else if has(&["gate"]) {
        BlockType::Gate
    } else {
        return None;
    })
}

/// `type1` slot → `(mode, poles)` — every slot measured through real
/// Omnisphere 3 (8-band Goertzel fingerprint per slot, sweep at freq=0.5,
/// res=0, keytrack off). Slot = `round(type1 × 50)`, 1-based. Pole counts
/// are lower bounds (the probe's noise floor flattens slopes far past the
/// knee) — a low-cutoff refinement pass can sharpen them. "allpass" slots
/// are the gain/color types that don't shape the spectrum.
const TYPE1_TABLE: [(&str, u32); 50] = [
    ("lowpass", 1),  // 0.02
    ("bandpass", 2), // 0.04
    ("lowpass", 2),  // 0.06
    ("notch", 2),    // 0.08
    ("highpass", 2), // 0.10
    ("lowpass", 2),  // 0.12
    ("highpass", 2), // 0.14
    ("lowpass", 1),  // 0.16
    ("lowpass", 1),  // 0.18
    ("lowpass", 2),  // 0.20
    ("lowpass", 2),  // 0.22 (Classic LPF 4-pole family)
    ("lowpass", 2),  // 0.24
    ("highpass", 2), // 0.26
    ("lowpass", 1),  // 0.28
    ("lowpass", 1),  // 0.30
    ("lowpass", 1),  // 0.32
    ("bandpass", 2), // 0.34
    ("allpass", 0),  // 0.36
    ("lowpass", 2),  // 0.38
    ("allpass", 0),  // 0.40
    ("allpass", 0),  // 0.42
    ("allpass", 0),  // 0.44
    ("lowpass", 2),  // 0.46
    ("lowpass", 1),  // 0.48
    ("lowpass", 1),  // 0.50
    ("lowpass", 1),  // 0.52
    ("highpass", 2), // 0.54
    ("highpass", 3), // 0.56
    ("bandpass", 2), // 0.58
    ("bandpass", 2), // 0.60
    ("lowpass", 1),  // 0.62
    ("highpass", 1), // 0.64
    ("highpass", 3), // 0.66
    ("lowpass", 2),  // 0.68
    ("bandpass", 2), // 0.70
    ("allpass", 0),  // 0.72
    ("allpass", 0),  // 0.74
    ("notch", 2),    // 0.76
    ("notch", 2),    // 0.78
    ("lowpass", 2),  // 0.80
    ("lowpass", 1),  // 0.82
    ("lowpass", 1),  // 0.84
    ("lowpass", 1),  // 0.86 (Basic 12db Lowpass family)
    ("lowpass", 2),  // 0.88
    ("highpass", 1), // 0.90
    ("lowpass", 1),  // 0.92
    ("highpass", 2), // 0.94
    ("bandpass", 2), // 0.96
    ("allpass", 0),  // 0.98
    ("allpass", 0),  // 1.00
];

/// A measured Omnisphere filter model: how Signal's filter reproduces one
/// `type1` algorithm. Fitted to the real plugin's response (a saw through
/// each model at three resonance settings, ÷ the unfiltered saw), see
/// [`FILTER_MODELS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterModel {
    /// The factory preset name most patches using it carry.
    pub name: &'static str,
    pub mode: &'static str,
    pub poles: u32,
    /// Realized as a feedback ladder (else an SVF cascade).
    pub ladder: bool,
    /// Its corner as a multiple of the knob taper's reference corner, at
    /// knob settings 0.15, 0.3, 0.45, 0.6 and 0.75 (models track the knob at
    /// their own rates) — see [`FilterModel::corner_hz`].
    pub taper: [f32; 5],
    /// Resonance knob → Q (SVF) or loop feedback (ladder): `(lo, hi, curve)`
    /// (see `signal_sampler::native::ResonanceMap`).
    pub res: (f32, f32, f32),
    /// Corner multiplier at full resonance (resonance pulls some models'
    /// corners down), arriving as `res_shift^(resonance^res_shift_curve)`.
    pub res_shift: f32,
    pub res_shift_curve: f32,
    /// Passband level.
    pub gain_db: f32,
    /// Ladder passband compensation (0 = the feedback's full level loss).
    pub comp: f32,
}

impl FilterModel {
    /// The model's corner (Hz) at an effective knob setting: the reference
    /// taper × its own scale there (log-interpolated, held past the ends).
    #[must_use]
    pub fn corner_hz(&self, setting: f32) -> f32 {
        const AT: [f32; 5] = [0.15, 0.3, 0.45, 0.6, 0.75];
        let scale = if setting <= AT[0] {
            self.taper[0]
        } else if setting >= AT[4] {
            self.taper[4]
        } else {
            let i = ((setting - AT[0]) / 0.15).floor() as usize;
            let t = (setting - AT[i]) / 0.15;
            (self.taper[i].ln() + t * (self.taper[i + 1].ln() - self.taper[i].ln())).exp()
        };
        omni_cutoff_hz(setting) * scale
    }
}

/// The factory library's filter models by exact `type1` bits — the value is
/// an index, and the analog family near 0.99 differs only in its last bits.
/// These 45 cover 96% of the active filters in the 38k factory patches.
/// Generated from the harness fit (response ÷ an unfiltered saw, fitted
/// below 8 kHz); each comment gives the RMS dB error of the fit at resonance
/// 0, 0.5 and 0.9 and of the corner at the other knob settings.
#[rustfmt::skip]
const FILTER_MODELS: &[(u32, FilterModel)] = &[
    // LPF UVI 2 — 12221 factory uses; fit 0.4 dB at res 0, 1.1/1.0 at res 0.5/0.9; taper fits 0.15:0.5 0.45:0.3 0.6:0.1 0.75:0.1
    (0x3e6147ae, FilterModel { name: "LPF UVI 2", mode: "lowpass", poles: 2, ladder: false, taper: [1.0595, 1.1225, 1.1892, 1.1892, 1.1892], res: (0.600, 13.022, 0.358), res_shift: 0.625, res_shift_curve: 0.150, gain_db: 0.0, comp: 0.00 }),
    // LPF Juicy 12db — 8900 factory uses; fit 0.4 dB at res 0, 0.4/0.5 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.5 0.6:0.5 0.75:0.4
    (0x3f051eb8, FilterModel { name: "LPF Juicy 12db", mode: "lowpass", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.6300], res: (0.707, 7.381, 0.935), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 0.0, comp: 0.00 }),
    // Classic LPF 4-pole — 6513 factory uses; fit 0.5 dB at res 0, 0.3/2.0 at res 0.5/0.9; taper fits 0.15:0.2 0.45:1.0 0.6:0.6 0.75:0.2
    (0x3f5c28f6, FilterModel { name: "Classic LPF 4-pole", mode: "lowpass", poles: 4, ladder: true, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.7937], res: (0.000, 3.936, 0.606), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.3, comp: 0.12 }),
    // LPF Power 24db — 5341 factory uses; fit 0.6 dB at res 0, 5.0/7.2 at res 0.5/0.9; taper fits 0.15:0.5 0.45:0.7 0.6:1.3 0.75:1.1
    (0x3f000000, FilterModel { name: "LPF Power 24db", mode: "lowpass", poles: 3, ladder: true, taper: [0.1180, 0.1114, 0.1768, 0.2973, 0.3746], res: (0.000, 9.600, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -2.0, comp: 0.12 }),
    // LPF Juicy 24db — 3996 factory uses; fit 0.2 dB at res 0, 5.2/7.6 at res 0.5/0.9; taper fits 0.15:0.2 0.45:1.1 0.6:1.0 0.75:0.7
    (0x3ef5c28f, FilterModel { name: "LPF Juicy 24db", mode: "lowpass", poles: 4, ladder: true, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.6300], res: (0.000, 4.800, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -0.1, comp: 1.00 }),
    // Classic LPF 2-pole — 1806 factory uses; fit 0.8 dB at res 0, 2.0/3.0 at res 0.5/0.9; taper fits 0.15:0.2 0.45:0.5 0.6:0.3 0.75:0.1
    (0x3f51eb85, FilterModel { name: "Classic LPF 2-pole", mode: "lowpass", poles: 2, ladder: true, taper: [0.2973, 0.3969, 0.5297, 0.7071, 0.7937], res: (0.000, 3.175, 0.952), res_shift: 0.615, res_shift_curve: 0.488, gain_db: 3.1, comp: 0.38 }),
    // HPF UVI — 1641 factory uses; fit 0.1 dB at res 0, 0.2/0.2 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.2 0.6:0.1 0.75:0.1
    (0x3e851eb8, FilterModel { name: "HPF UVI", mode: "highpass", poles: 2, ladder: false, taper: [1.3348, 1.3348, 1.3348, 1.2599, 1.1225], res: (0.500, 13.011, 0.335), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -0.1, comp: 0.00 }),
    // LPF Power 12db — 1515 factory uses; fit 0.8 dB at res 0, 1.9/4.7 at res 0.5/0.9; taper fits 0.15:0.8 0.45:1.2 0.6:1.1 0.75:1.1
    (0x3f1eb852, FilterModel { name: "LPF Power 12db", mode: "lowpass", poles: 2, ladder: true, taper: [0.2102, 0.1984, 0.3150, 0.4719, 0.5297], res: (0.000, 1.985, 0.689), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -2.0, comp: 0.00 }),
    // Beefy LPF 4-pole — 1326 factory uses; fit 0.8 dB at res 0, 5.7/6.7 at res 0.5/0.9; taper fits 0.15:0.6 0.45:1.0 0.6:1.4 0.75:0.7
    (0x3f7d70b6, FilterModel { name: "Beefy LPF 4-pole", mode: "lowpass", poles: 3, ladder: true, taper: [0.1768, 0.2227, 0.3150, 0.3969, 0.3969], res: (0.000, 9.600, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 4.8, comp: 0.12 }),
    // Classic LPF 1-pole — 1263 factory uses; fit 1.0 dB at res 0, 1.4/2.5 at res 0.5/0.9; taper fits 0.15:0.5 0.45:2.0 0.6:3.0 0.75:3.7
    (0x3f4ccccd, FilterModel { name: "Classic LPF 1-pole", mode: "bandpass", poles: 2, ladder: false, taper: [0.1250, 0.1487, 0.1873, 0.1984, 0.1487], res: (0.500, 4.578, 1.979), res_shift: 2.022, res_shift_curve: 0.150, gain_db: 3.5, comp: 0.00 }),
    // Bandpass Juicy 12db — 953 factory uses; fit 0.1 dB at res 0, 2.2/3.0 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.2 0.6:0.4 0.75:0.5
    (0x3f147ae1, FilterModel { name: "Bandpass Juicy 12db", mode: "bandpass", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.7071], res: (0.707, 0.707, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -2.8, comp: 0.00 }),
    // State Variable 12dB — 770 factory uses; fit 0.7 dB at res 0, 0.6/0.3 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.7 0.6:1.7 0.75:1.6
    (0x3f428f5c, FilterModel { name: "State Variable 12dB", mode: "notch", poles: 2, ladder: false, taper: [0.2973, 0.3536, 0.5000, 0.6300, 0.7492], res: (0.707, 11.514, 1.186), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -1.1, comp: 0.00 }),
    // Classic LPF 8-pole — 753 factory uses; fit 1.0 dB at res 0, 4.0/8.9 at res 0.5/0.9; taper fits 0.15:1.1 0.45:1.2 0.6:1.6 0.75:0.9
    (0x3f7d70a9, FilterModel { name: "Classic LPF 8-pole", mode: "lowpass", poles: 6, ladder: false, taper: [0.2102, 0.2649, 0.3746, 0.5000, 0.5612], res: (0.500, 1.229, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.1, comp: 0.00 }),
    // Classic LPF 3-pole — 726 factory uses; fit 0.3 dB at res 0, 1.6/2.9 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.7 0.6:0.4 0.75:0.2
    (0x3f570a3d, FilterModel { name: "Classic LPF 3-pole", mode: "lowpass", poles: 3, ladder: true, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.7937], res: (0.000, 3.970, 0.689), res_shift: 0.770, res_shift_curve: 1.178, gain_db: 3.2, comp: 0.25 }),
    // Jupiter LPF 4-pole — 712 factory uses; fit 0.5 dB at res 0, 2.6/6.9 at res 0.5/0.9; taper fits 0.15:0.7 0.45:0.9 0.6:0.5 0.75:0.5
    (0x3f7d70b1, FilterModel { name: "Jupiter LPF 4-pole", mode: "lowpass", poles: 4, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.7071, 1.0000], res: (0.707, 3.345, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.3, comp: 0.00 }),
    // LPF UVI 3 — 710 factory uses; fit 0.1 dB at res 0, 0.2/0.1 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.1 0.6:0.1 0.75:0.1
    (0x3e75c28f, FilterModel { name: "LPF UVI 3", mode: "lowpass", poles: 2, ladder: false, taper: [1.3348, 1.3348, 1.3348, 1.2599, 1.1225], res: (0.500, 6.658, 0.439), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 0.0, comp: 0.00 }),
    // FATBOY — 663 factory uses; fit 1.1 dB at res 0, 2.8/3.9 at res 0.5/0.9; taper fits 0.15:1.3 0.45:1.3 0.6:1.0 0.75:0.5
    (0x3e99999a, FilterModel { name: "FATBOY", mode: "lowpass", poles: 4, ladder: false, taper: [0.2806, 0.3536, 0.5000, 0.5946, 0.5946], res: (0.500, 40.000, 0.520), res_shift: 1.124, res_shift_curve: 0.150, gain_db: -0.3, comp: 0.00 }),
    // LPF Gentle 6db — 656 factory uses; fit 0.3 dB at res 0, 0.7/1.1 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.8 0.6:1.4 0.75:2.2
    (0x3e3851ec, FilterModel { name: "LPF Gentle 6db", mode: "lowpass", poles: 1, ladder: true, taper: [0.2227, 0.4719, 0.8409, 1.6818, 1.7818], res: (0.000, 0.410, 0.300), res_shift: 1.136, res_shift_curve: 1.000, gain_db: -0.1, comp: 0.25 }),
    // HPF Juicy 12db — 611 factory uses; fit 0.2 dB at res 0, 0.3/0.5 at res 0.5/0.9; taper fits 0.15:0.3 0.45:0.3 0.6:0.5 0.75:0.8
    (0x3f0a3d71, FilterModel { name: "HPF Juicy 12db", mode: "highpass", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.6300, 0.7071], res: (0.707, 7.381, 0.935), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 0.4, comp: 0.00 }),
    // HPF Crisp — 562 factory uses; fit 0.0 dB at res 0, 0.2/0.2 at res 0.5/0.9; taper fits 0.15:0.2 0.45:0.3 0.6:0.5 0.75:0.4
    (0x3e0f5c29, FilterModel { name: "HPF Crisp", mode: "highpass", poles: 2, ladder: false, taper: [0.2360, 0.2227, 0.2973, 0.3536, 0.3969], res: (0.500, 12.565, 0.936), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 0.2, comp: 0.00 }),
    // Bandpass Juicy 24db — 562 factory uses; fit 1.0 dB at res 0, 5.5/7.7 at res 0.5/0.9; taper fits 0.15:0.8 0.45:1.6 0.6:2.0 0.75:2.1
    (0x3f19999a, FilterModel { name: "Bandpass Juicy 24db", mode: "bandpass", poles: 4, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.7492], res: (0.500, 0.500, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -9.0, comp: 0.00 }),
    // Bandpass — 489 factory uses; fit 1.4 dB at res 0, 1.4/1.5 at res 0.5/0.9; taper fits 0.15:1.9 0.45:2.6 0.6:4.8 0.75:4.5
    (0x3d75c28f, FilterModel { name: "Bandpass", mode: "bandpass", poles: 2, ladder: false, taper: [1.4142, 0.8909, 0.5612, 0.8909, 0.8909], res: (1.000, 3.534, 0.348), res_shift: 1.599, res_shift_curve: 0.150, gain_db: 9.2, comp: 0.00 }),
    // LPF Warm 12db — 435 factory uses; fit 0.3 dB at res 0, 0.4/0.5 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.2 0.6:0.1 0.75:0.1
    (0x3e8f5c29, FilterModel { name: "LPF Warm 12db", mode: "lowpass", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5000, 0.6300, 0.7071], res: (0.500, 4.767, 0.519), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -0.0, comp: 0.00 }),
    // Beefy BPF 4-pole — 427 factory uses; fit 3.0 dB at res 0, 0.5/1.5 at res 0.5/0.9; taper fits 0.15:1.6 0.45:4.4 0.6:6.3 0.75:5.2
    (0x3f7d70cc, FilterModel { name: "Beefy BPF 4-pole", mode: "bandpass", poles: 4, ladder: false, taper: [0.3337, 0.3746, 0.3746, 0.3746, 0.3746], res: (0.500, 11.767, 1.113), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 9.8, comp: 0.00 }),
    // LPF Colorful 24db — 411 factory uses; fit 1.6 dB at res 0, 2.4/5.2 at res 0.5/0.9; taper fits 0.15:2.6 0.45:2.9 0.6:2.2 0.75:2.3
    (0x3d23d70a, FilterModel { name: "LPF Colorful 24db", mode: "lowpass", poles: 6, ladder: false, taper: [0.5000, 0.2500, 0.2227, 0.2649, 0.2806], res: (0.850, 1.774, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -4.4, comp: 0.00 }),
    // HPF Power 12db — 410 factory uses; fit 0.6 dB at res 0, 0.9/1.8 at res 0.5/0.9; taper fits 0.15:0.2 0.45:1.0 0.6:1.7 0.75:2.1
    (0x3f23d70a, FilterModel { name: "HPF Power 12db", mode: "highpass", poles: 2, ladder: true, taper: [0.2360, 0.1984, 0.3337, 0.5612, 0.6674], res: (0.000, 8.000, 1.869), res_shift: 2.192, res_shift_curve: 1.180, gain_db: 0.3, comp: -0.12 }),
    // OB LPF 4-pole — 406 factory uses; fit 0.5 dB at res 0, 4.0/6.5 at res 0.5/0.9; taper fits 0.15:0.7 0.45:0.5 0.6:0.6 0.75:0.3
    (0x3f7d70b3, FilterModel { name: "OB LPF 4-pole", mode: "lowpass", poles: 4, ladder: false, taper: [0.2973, 0.3746, 0.5297, 0.7071, 0.9439], res: (0.600, 7.733, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.5, comp: 0.00 }),
    // Classic LPF 6-pole — 395 factory uses; fit 0.6 dB at res 0, 3.9/6.0 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.7 0.6:0.9 0.75:0.4
    (0x3f7d70a6, FilterModel { name: "Classic LPF 6-pole", mode: "lowpass", poles: 6, ladder: true, taper: [0.2973, 0.3746, 0.5297, 0.6674, 0.7937], res: (0.000, 2.844, 1.607), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.2, comp: -0.25 }),
    // Beefy LPF 2-pole — 379 factory uses; fit 1.3 dB at res 0, 1.7/3.4 at res 0.5/0.9; taper fits 0.15:0.9 0.45:1.5 0.6:1.1 0.75:0.5
    (0x3f7d70b5, FilterModel { name: "Beefy LPF 2-pole", mode: "lowpass", poles: 2, ladder: true, taper: [0.3150, 0.3746, 0.4719, 0.5297, 0.5000], res: (0.000, 2.952, 0.300), res_shift: 0.495, res_shift_curve: 0.150, gain_db: 4.6, comp: 0.25 }),
    // LPF Smooth 24db — 376 factory uses; fit 0.8 dB at res 0, 9.1/11.0 at res 0.5/0.9; taper fits 0.15:2.6 0.45:0.9 0.6:0.6 0.75:0.5
    (0x3ea3d70a, FilterModel { name: "LPF Smooth 24db", mode: "lowpass", poles: 2, ladder: true, taper: [0.0625, 0.0662, 0.0936, 0.1180, 0.1487], res: (0.000, 8.000, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 1.3, comp: 0.50 }),
    // LPF Crisp 12db — 338 factory uses; fit 0.5 dB at res 0, 0.4/0.5 at res 0.5/0.9; taper fits 0.15:0.2 0.45:0.6 0.6:0.5 0.75:0.3
    (0x3eeb851f, FilterModel { name: "LPF Crisp 12db", mode: "lowpass", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5612, 0.6674, 0.6300], res: (0.500, 10.862, 0.869), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -0.0, comp: 0.00 }),
    // HPF Juicy 24db — 316 factory uses; fit 0.1 dB at res 0, 1.2/1.9 at res 0.5/0.9; taper fits 0.15:0.5 0.45:0.8 0.6:0.6 0.75:1.8
    (0x3f0f5c29, FilterModel { name: "HPF Juicy 24db", mode: "highpass", poles: 4, ladder: true, taper: [0.3150, 0.3746, 0.5297, 0.6674, 0.7937], res: (0.000, 4.800, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 0.6, comp: 1.00 }),
    // LPF Edge 24db — 278 factory uses; fit 1.6 dB at res 0, 4.2/7.0 at res 0.5/0.9; taper fits 0.15:4.0 0.45:1.4 0.6:1.9 0.75:2.2
    (0x3eae147b, FilterModel { name: "LPF Edge 24db", mode: "lowpass", poles: 4, ladder: true, taper: [0.4204, 0.4204, 0.5612, 0.7492, 0.9439], res: (0.000, 3.200, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -3.5, comp: 1.00 }),
    // LPF UVI 1 — 275 factory uses; fit 0.7 dB at res 0, 0.9/1.3 at res 0.5/0.9; taper fits 0.15:1.2 0.45:0.6 0.6:0.4 0.75:0.2
    (0x3e4ccccd, FilterModel { name: "LPF UVI 1", mode: "lowpass", poles: 4, ladder: true, taper: [0.7937, 0.8409, 0.8909, 0.9439, 0.9439], res: (0.000, 3.372, 0.869), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 2.6, comp: 0.00 }),
    // ? — 260 factory uses; fit 1.1 dB at res 0, 2.7/3.9 at res 0.5/0.9; taper fits 0.15:1.3 0.45:1.2 0.6:1.0 0.75:0.4
    (0x3e999999, FilterModel { name: "?", mode: "lowpass", poles: 4, ladder: false, taper: [0.2806, 0.3536, 0.5000, 0.5946, 0.5946], res: (0.500, 40.000, 0.520), res_shift: 1.124, res_shift_curve: 0.150, gain_db: -0.3, comp: 0.00 }),
    // Sauce LPF 4-pole — 255 factory uses; fit 0.2 dB at res 0, 5.0/7.2 at res 0.5/0.9; taper fits 0.15:0.2 0.45:1.2 0.6:1.0 0.75:0.7
    (0x3f7d70b0, FilterModel { name: "Sauce LPF 4-pole", mode: "lowpass", poles: 4, ladder: true, taper: [0.2973, 0.3746, 0.5612, 0.6674, 0.6300], res: (0.000, 4.800, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: 3.6, comp: 0.12 }),
    // OB LPF 2-pole — 253 factory uses; fit 0.7 dB at res 0, 2.3/3.3 at res 0.5/0.9; taper fits 0.15:0.7 0.45:0.9 0.6:0.7 0.75:0.2
    (0x3f6b851f, FilterModel { name: "OB LPF 2-pole", mode: "lowpass", poles: 2, ladder: true, taper: [0.2973, 0.3746, 0.5612, 0.7937, 1.1225], res: (0.000, 3.487, 0.300), res_shift: 0.495, res_shift_curve: 0.150, gain_db: 3.0, comp: 0.12 }),
    // Subtle LPF 1-pole — 253 factory uses; fit 0.3 dB at res 0, 0.7/1.1 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.9 0.6:1.4 0.75:2.2
    (0x3f7d70b8, FilterModel { name: "Subtle LPF 1-pole", mode: "lowpass", poles: 1, ladder: true, taper: [0.2227, 0.4719, 0.8409, 1.7818, 1.7818], res: (0.000, 2.036, 3.312), res_shift: 0.564, res_shift_curve: 2.048, gain_db: -0.1, comp: 0.38 }),
    // Jupiter HPF 4-pole — 247 factory uses; fit 1.4 dB at res 0, 2.3/9.7 at res 0.5/0.9; taper fits 0.15:1.1 0.45:2.2 0.6:3.0 0.75:2.0
    (0x3f7d70dd, FilterModel { name: "Jupiter HPF 4-pole", mode: "highpass", poles: 6, ladder: false, taper: [0.2102, 0.2649, 0.3536, 0.2649, 0.5297], res: (1.000, 5.405, 0.300), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -3.6, comp: 0.00 }),
    // Notch — 204 factory uses; fit 0.7 dB at res 0, 0.8/3.6 at res 0.5/0.9; taper fits 0.15:0.1 0.45:0.6 0.6:1.1 0.75:0.2
    (0x3da3d70a, FilterModel { name: "Notch", mode: "notch", poles: 2, ladder: false, taper: [0.2973, 0.3536, 0.5000, 0.6300, 0.7071], res: (0.850, 0.850, 1.000), res_shift: 1.154, res_shift_curve: 2.048, gain_db: 0.1, comp: 0.00 }),
    // Bandpass Power 24db — 201 factory uses; fit 2.2 dB at res 0, 2.4/5.9 at res 0.5/0.9; taper fits 0.15:2.0 0.45:3.0 0.6:3.8 0.75:4.8
    (0x3f333333, FilterModel { name: "Bandpass Power 24db", mode: "bandpass", poles: 4, ladder: false, taper: [0.2102, 0.1984, 0.3150, 0.1984, 0.1984], res: (0.500, 2.829, 1.625), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -12.3, comp: 0.00 }),
    // Jupiter HPF 2-pole — 184 factory uses; fit 1.4 dB at res 0, 1.6/2.5 at res 0.5/0.9; taper fits 0.15:0.7 0.45:2.2 0.6:3.0 0.75:2.4
    (0x3f7d70bf, FilterModel { name: "Jupiter HPF 2-pole", mode: "highpass", poles: 3, ladder: true, taper: [0.1487, 0.2102, 0.2500, 0.2102, 0.3969], res: (0.000, 4.033, 0.300), res_shift: 2.022, res_shift_curve: 0.150, gain_db: -2.2, comp: 0.62 }),
    // Jupiter HPF 1-pole — 180 factory uses; fit 0.8 dB at res 0, 2.0/3.4 at res 0.5/0.9; taper fits 0.15:0.5 0.45:1.6 0.6:2.8 0.75:3.8
    (0x3f666666, FilterModel { name: "Jupiter HPF 1-pole", mode: "highpass", poles: 2, ladder: true, taper: [0.1250, 0.1487, 0.1669, 0.1984, 0.1487], res: (0.000, 2.256, 0.300), res_shift: 1.798, res_shift_curve: 0.150, gain_db: -1.5, comp: 0.38 }),
    // Jupiter LPF 2-pole — 164 factory uses; fit 1.0 dB at res 0, 1.6/3.5 at res 0.5/0.9; taper fits 0.15:0.8 0.45:1.1 0.6:0.9 0.75:0.2
    (0x3f6147ae, FilterModel { name: "Jupiter LPF 2-pole", mode: "lowpass", poles: 2, ladder: true, taper: [0.2973, 0.3969, 0.5946, 0.8909, 1.4142], res: (0.000, 2.544, 0.311), res_shift: 0.495, res_shift_curve: 0.150, gain_db: 3.3, comp: 0.25 }),
    // State Variable OB — 163 factory uses; fit 0.7 dB at res 0, 6.4/10.3 at res 0.5/0.9; taper fits 0.15:0.1 0.45:1.1 0.6:3.0 0.75:0.5
    (0x3f7d70d4, FilterModel { name: "State Variable OB", mode: "notch", poles: 2, ladder: false, taper: [0.2973, 0.3746, 0.5000, 0.3746, 0.7492], res: (0.707, 0.707, 1.000), res_shift: 1.000, res_shift_curve: 1.000, gain_db: -1.9, comp: 0.00 }),
];

/// The model for a `type1` value: exact, else the nearest measured one in
/// the same family (a few ULPs off), else a generic model from the coarse
/// slot table. `None` for the gain/colour types that don't filter.
#[must_use]
pub fn filter_model(type1: f32) -> Option<FilterModel> {
    let bits = type1.to_bits();
    if let Some((_, m)) = FILTER_MODELS.iter().find(|(b, _)| *b == bits) {
        return Some(*m);
    }
    if let Some((_, m)) = FILTER_MODELS
        .iter()
        .filter(|(b, _)| b.abs_diff(bits) <= 64)
        .min_by_key(|(b, _)| b.abs_diff(bits))
    {
        return Some(*m);
    }
    let (mode, poles) = classify_type1(type1)?;
    Some(FilterModel {
        name: "generic",
        mode,
        poles,
        ladder: false,
        taper: [1.0; 5],
        res: (0.5, 12.0, 1.0),
        res_shift: 1.0,
        res_shift_curve: 1.0,
        gain_db: 0.0,
        comp: 0.0,
    })
}

/// Look a `type1` value up in the measured table. `None` for "allpass"
/// (gain/color) slots — callers leave the filter transparent.
pub fn classify_type1(v: f32) -> Option<(&'static str, u32)> {
    let slot = (v * 50.0).round() as usize;
    let (mode, poles) = *TYPE1_TABLE.get(slot.checked_sub(1)?)?;
    (mode != "allpass").then_some((mode, poles))
}

/// The filter knob's taper, measured through real Omnisphere: the
/// Butterworth corner of the plain 12 dB lowpass (`type1` 0.22) at each
/// effective setting (see [`omni_filter_setting`]). Near `17 kHz · v²` low,
/// bending upward above ~0.5. Other models' corners are a fixed ratio of
/// this (their `FilterModel::scale`).
const CUTOFF_TAPER: [(f32, f32); 17] = [
    (0.10, 170.0),
    (0.15, 382.0),
    (0.20, 680.0),
    (0.25, 1062.0),
    (0.30, 1535.0),
    (0.35, 2095.0),
    (0.40, 2741.0),
    (0.45, 3513.0),
    (0.50, 4374.0),
    (0.55, 5373.0),
    (0.60, 6532.0),
    (0.65, 7810.0),
    (0.70, 9527.0),
    (0.75, 11401.0),
    (0.80, 14063.0),
    (0.85, 17550.0),
    (1.00, 34000.0),
];

/// A filter's effective knob setting: the section's master control (`freq`,
/// `res`) plus its own filter's offset (`freq1` / `res2` …, 0.5 = none), two
/// knob units per unit of offset — measured: every (freq, freqN) pair with
/// the same `freq + 2·(freqN − 0.5)` lands on the same corner, and the same
/// law holds for resonance.
#[must_use]
pub fn omni_filter_setting(freq: f32, freq_n: f32) -> f32 {
    freq + 2.0 * (freq_n - 0.5)
}

/// Filter knob setting → the reference corner in Hz ([`CUTOFF_TAPER`]).
#[must_use]
pub fn omni_cutoff_hz(v: f32) -> f32 {
    let (v0, h0) = CUTOFF_TAPER[0];
    if v <= v0 {
        // The low end follows the quadratic law (floored well under audio).
        return (h0 * (v.max(0.0) / v0).powi(2)).max(8.0);
    }
    for w in CUTOFF_TAPER.windows(2) {
        let ((a, ha), (b, hb)) = (w[0], w[1]);
        if v <= b {
            let t = (v - a) / (b - a);
            return (ha.ln() + t * (hb.ln() - ha.ln())).exp();
        }
    }
    CUTOFF_TAPER[CUTOFF_TAPER.len() - 1].1
}

/// Coarse filter classification from the factory preset name (`NameStr`).
///
/// Mode + pole count. The real algorithm enum (`type1`) is undecoded; the
/// names cover the dominant families ("Classic LPF 4-pole", "HPF Juicy
/// 12db", "Bandpass", "Notch", …).
///
/// Defaults: LP 12 dB. Classification including the engine character: the saturating families
/// (Juicy / Moogie / OB / Jupiter / FATBOY / Sauce / Beefy / Warm / Power /
/// French / Brit) map onto the ladder engine.
#[must_use]
pub fn classify_filter_full(name: &str) -> (&'static str, u32, &'static str) {
    let (mode, poles) = classify_filter_inner(name);
    let k = name.to_ascii_lowercase();
    let saturating = [
        "juicy", "moogie", "fatboy", "ob ", "jupiter", "sauce", "beefy", "warm", "power", "french",
        "brit",
    ]
    .iter()
    .any(|f| k.contains(f));
    let character = if saturating && mode == "lowpass" {
        "ladder"
    } else {
        "clean"
    };
    (mode, poles, character)
}

fn classify_filter_inner(name: &str) -> (&'static str, u32) {
    let k = name.to_ascii_lowercase();
    let mode = if k.contains("hpf") || k.contains("hipass") || k.contains("high") {
        "highpass"
    } else if k.contains("bpf") || k.contains("bandpass") {
        "bandpass"
    } else if k.contains("notch") {
        "notch"
    } else {
        "lowpass"
    };
    // "<N>-pole" wins; else "<N>db" → N/6 poles.
    let mut poles = 2u32;
    for (pat, scale) in [("-pole", 1u32), ("db", 6u32)] {
        if let Some(pos) = k.find(pat) {
            let digits: String = k[..pos]
                .chars()
                .rev()
                .take_while(char::is_ascii_digit)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if let Ok(n) = digits.parse::<u32>() {
                if n >= scale {
                    poles = (n / scale).clamp(1, 8);
                    break;
                }
            }
        }
    }
    (mode, poles)
}

/// Normalized envelope time → seconds. CALIBRATE: the exact Omnisphere
/// mapping calibrated against the real UI: the A/D/R time knobs share one
/// curve spanning **0–20 s**, logarithmic-ish — half travel (0.5) ≈ 2 s. A
/// power fit through those points gives `t = 20 · v^3.32` (exponent
/// `ln(0.1)/ln(0.5)`), i.e. 0→0 s, 0.5→2 s, 1.0→20 s. Sustain is a level, not
/// a time, so it never passes through here.
pub fn env_seconds(v: f32) -> f32 {
    // ln(0.1)/ln(0.5) == log2(10) — the same exponent either way.
    20.0 * v.clamp(0.0, 1.0).powf(std::f32::consts::LOG2_10)
}

/// Parse an `AENVPARAMS`/`FENVPARAMS` element into `(a, d, s, r)`.
///
/// FALLBACK ONLY: the engine renders the `AENV`/`FENV` **breakpoint list**
/// (see [`parse_env_breakpoints`]); these attrs are derived UI state —
/// sweeping them through the real plugin changed nothing.
fn parse_env(e: &XmlNode) -> Option<(f32, f32, f32, f32)> {
    if e.num("onOff").unwrap_or(1.0) == 0.0 {
        return None;
    }
    Some((
        env_seconds(e.num("attk").unwrap_or(0.0)),
        env_seconds(e.num("decy").unwrap_or(0.0)),
        e.num("sust").unwrap_or(1.0).clamp(0.0, 1.0),
        env_seconds(e.num("rels").unwrap_or(0.0)),
    ))
}

/// Breakpoint time unit: `t` is absolute time from note-on where
/// **1.0 = 100 seconds** — measured against real Omnisphere 3 via the state
/// injection harness (release Δt of 0.003/0.03 rendered 0.28 s / 2.92 s
/// decays, the −26 dB points of linear 0.3 s / 3.0 s ramps).
const ENV_T_SECONDS: f32 = 100.0;

/// Parse an `AENV`/`FENV` **breakpoint envelope** (`<p l= t= s= c=>` children:
/// `l` = linear level, `t` = absolute time ×[`ENV_T_SECONDS`], `s` = flags
/// (18 marks the terminal point), `c` = segment curve, 0.5 ≈ linear) into an
/// ADSR approximation `(attack_s, decay_s, sustain, release_s)`.
///
/// 4-point envelopes map exactly (start → peak → sustain → end). Longer MSEG
/// lists approximate: attack = first peak, sustain = the point before the
/// terminal one, release = the final segment.
fn parse_env_breakpoints(e: &XmlNode) -> Option<(f32, f32, f32, f32)> {
    let pts: Vec<(f32, f32)> = e
        .children_tagged("p")
        .map(|p| (p.num("l").unwrap_or(0.0), p.num("t").unwrap_or(0.0)))
        .collect();
    if pts.len() < 3 {
        return None;
    }
    // Peak = the FIRST point at the maximum level (`max_by` would pick the
    // last, turning a sustain-at-full envelope's attack into its decay).
    let top = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let peak_idx = pts.iter().position(|p| p.0 >= top)?;
    let last = pts.len() - 1;
    let sus_idx = if last > peak_idx { last - 1 } else { peak_idx };
    let attack = pts[peak_idx].1 * ENV_T_SECONDS;
    let decay = (pts[sus_idx].1 - pts[peak_idx].1).max(0.0) * ENV_T_SECONDS;
    let sustain = pts[sus_idx].0.clamp(0.0, 1.0);
    let release = (pts[last].1 - pts[sus_idx].1).max(0.0) * ENV_T_SECONDS;
    Some((attack, decay, sustain, release))
}

/// An envelope's breakpoints as `(seconds or beats, level, curve k, step)`
/// (measured: one `t` unit is 100; `k = 42·(0.5 − c)`; `s` bit 0 = step).
fn env_points(env: &XmlNode) -> Vec<(f32, f32, f32, bool)> {
    env.children_tagged("p")
        .map(|p| {
            let bits = p
                .attr("s")
                .and_then(|v| u32::from_str_radix(v, 16).ok())
                .unwrap_or(0);
            (
                p.num("t").unwrap_or(0.0).max(0.0) * 100.0,
                p.num("l").unwrap_or(0.0).clamp(0.0, 1.0),
                42.0 * (0.5 - p.num("c").unwrap_or(0.5).clamp(0.0, 1.0)),
                bits & 1 != 0,
            )
        })
        .collect()
}

/// Parse a `.prt_omn` document into an [`OmniPatch`].
///
/// # Errors
///
/// Returns an error if the XML is invalid or does not contain the expected patch structure.
pub fn parse_patch(xml: &str) -> Result<OmniPatch, String> {
    let root = parse_xml(xml)?;
    parse_patch_node(&root)
}

/// Parse one part from any node containing a `SYNTHENG` (a patch document
/// root, or one `SynthEngine` inside a Multi).
pub fn parse_patch_node(root: &XmlNode) -> Result<OmniPatch, String> {
    let engine = root
        .find("SYNTHENG")
        .ok_or("no SYNTHENG element (not an Omnisphere patch?)")?;

    let mut patch = OmniPatch::default();

    if let Some(descr) = engine.child("ENTRYDESCR") {
        patch.name = descr.attr("name").unwrap_or("").to_string();
        patch.library = descr.attr("library").unwrap_or("").to_string();
        if let Some(tags) = descr.attr("ATTRIB_VALUE_DATA") {
            patch.tags = tags
                .split(';')
                .filter_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    Some((k.trim().to_string(), v.trim().to_string()))
                })
                .collect();
        }
    }

    // Layers: VOICE[i] pairs with MULTISAMPLE[i].
    let voices: Vec<&XmlNode> = engine.children_tagged("VOICE").collect();
    let multis: Vec<&XmlNode> = engine.children_tagged("MULTISAMPLE").collect();
    for (i, voice) in voices.iter().enumerate() {
        let mut layer = OmniLayer::default();
        if let Some(ms) = multis.get(i).and_then(|m| m.child("MS_IM_0")) {
            layer.soundsource = ms.attr("name").unwrap_or("").to_string();
            layer.ss_library = ms.attr("library").unwrap_or("").to_string();
        }
        if let Some(f) = voice.child("FILTER") {
            layer.filter_name = f.attr("NameStr").unwrap_or("").to_string();
            layer.filter_parallel = f.num("para").unwrap_or(0.0) != 0.0;
            layer.filter_active = f.num("act").unwrap_or(0.0) != 0.0;
            let master = f.num("freq").unwrap_or(0.5);
            layer.filter_freq = omni_filter_setting(master, f.num("freq1").unwrap_or(0.5));
            layer.filter_type1 = f.num("type1");
            layer.filter1_on = f.num("act1").unwrap_or(1.0) != 0.0;
            // Per-filter resonance is an offset like the cutoff's (0.5 = none).
            let res = f.num("res").unwrap_or(0.0);
            layer.filter_res =
                omni_filter_setting(res, f.num("res1").unwrap_or(0.5)).clamp(0.0, 1.0);
            layer.filter_balance = f.num("bal").unwrap_or(0.5).clamp(0.0, 1.0);
            let depth = f.num("envdpth").unwrap_or(0.0).clamp(0.0, 1.0);
            let inv = f.num("envdpthinv").unwrap_or(0.0) != 0.0;
            layer.filter_env_depth = if inv { -depth } else { depth };
            if f.num("act2").unwrap_or(0.0) != 0.0 {
                layer.filter_type2 = f.num("type2");
                layer.filter2 = Some((
                    omni_filter_setting(master, f.num("freq2").unwrap_or(0.5)),
                    omni_filter_setting(res, f.num("res2").unwrap_or(0.5)).clamp(0.0, 1.0),
                ));
            }
        }
        // The engine renders the breakpoint envelopes; the PARAMS attrs are
        // only a fallback for patches without a breakpoint list.
        layer.amp_env = voice
            .child("AENV")
            .and_then(parse_env_breakpoints)
            .or_else(|| voice.child("AENVPARAMS").and_then(parse_env));
        layer.filter_env_velsens = voice
            .child("FENVPARAMS")
            .and_then(|p| p.num("velsens"))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        layer.amp_points = voice.child("AENV").map(env_points).unwrap_or_default();
        layer.filter_points = voice.child("FENV").map(env_points).unwrap_or_default();
        layer.filter_env = voice
            .child("FENV")
            .and_then(parse_env_breakpoints)
            .or_else(|| voice.child("FENVPARAMS").and_then(parse_env));
        // The wave lists sit inside `OSC` (older files: beside it).
        let pair = |tag: &str| {
            voice
                .child("OSC")
                .and_then(|o| o.child(tag))
                .or_else(|| voice.child(tag))
                .and_then(|w| {
                    let a = w.attr("wf0").filter(|s| !s.is_empty())?.to_string();
                    let b = w
                        .attr("wf1")
                        .filter(|s| !s.is_empty())
                        .map_or_else(|| a.clone(), str::to_string);
                    Some((a, b))
                })
        };
        layer.enabled = voice
            .child("AENVPARAMS")
            .and_then(|e| e.num("onOff"))
            .is_none_or(|v| v != 0.0);
        layer.waves = pair("WAVES");
        layer.fm_waves = pair("FMWAVES");
        layer.am_waves = pair("AMWAVES");
        if let Some(osc) = voice.child("OSC") {
            layer.level = osc.num("level").unwrap_or(0.5);
            let n = |k: &str, d: f32| osc.num(k).unwrap_or(d);
            layer.transpose = ((0.04 - n("oct", 0.04)) / 0.02).round() * 12.0
                + ((0.48 - n("semi", 0.48)) / 0.02).round()
                + (n("tune", 0.5) - 0.5) * 96.0
                + (n("tuneFine", 0.5) - 0.5) * 2.0;
            layer.glide = n("portAct", 0.0) != 0.0;
            layer.pan = n("pan", 0.5).clamp(0.0, 1.0);
            layer.fm_depth = osc.num("fm").unwrap_or(0.0).clamp(0.0, 1.0);
            layer.fm_shape = osc.num("fmwf").unwrap_or(0.0).clamp(0.0, 1.0);
            layer.osc_wave = osc.num("type").unwrap_or(0.0).clamp(0.0, 1.0);
            layer.osc_shape = osc.num("pdepth").unwrap_or(0.0).clamp(0.0, 1.0);
            layer.osc_symmetry = osc.num("pwidth").unwrap_or(0.0).clamp(0.0, 1.0);
            layer.osc_kind = osc
                .attr("kind")
                .and_then(|k| k.trim().parse::<u32>().ok())
                .unwrap_or(0);
            layer.ring_mix = osc.num("am").unwrap_or(0.0).clamp(0.0, 1.0);
            // Unison: the newer UNI element wins; older patches carry the
            // uns*/u* attrs directly on OSC.
            let (on, cnt, dpth, wdth) = match osc.find("UNI") {
                Some(uni) => (
                    uni.num("umix").unwrap_or(1.0) > 0.0,
                    uni.num("ucnt").unwrap_or(0.0),
                    uni.num("udpth").unwrap_or(0.1),
                    uni.num("uwdth").unwrap_or(0.7),
                ),
                None => (
                    osc.num("unsOn").unwrap_or(0.0) > 0.0,
                    osc.num("ucnt").unwrap_or(0.0),
                    osc.num("udpth").unwrap_or(0.1),
                    osc.num("uwdth").unwrap_or(0.7),
                ),
            };
            if on {
                layer.unison_count = 1 + (cnt.clamp(0.0, 1.0) * 7.0).round() as u32;
                layer.unison_detune = dpth.clamp(0.0, 1.0);
                layer.unison_width = wdth.clamp(0.0, 1.0);
                let (src_oct, src_analg, src_drft) = match osc.find("UNI") {
                    Some(uni) => (uni.num("uoct"), uni.num("uanalg"), uni.num("udrft")),
                    None => (osc.num("uoct"), osc.num("uanalg"), osc.num("udrft")),
                };
                layer.unison_octave = src_oct.unwrap_or(0.0).clamp(0.0, 1.0);
                layer.unison_analog = src_analg.unwrap_or(0.0).clamp(0.0, 1.0);
                layer.unison_drift = src_drft.unwrap_or(0.0).clamp(0.0, 1.0);
            } else {
                layer.unison_count = 1;
            }
            // Harmonia: gated by OSC hrmOn, scaled by hrmLv.
            let hrm_on = osc.num("hrmOn").unwrap_or(1.0) > 0.0;
            let hrm_lv = osc.num("hrmLv").unwrap_or(1.0).clamp(0.0, 1.0);
            if hrm_on {
                if let Some(h) = osc.find("HARM") {
                    for i in 1..=4 {
                        let act = h.num(&format!("Act{i}")).unwrap_or(0.0) > 0.0;
                        let level = h.num(&format!("lvl{i}")).unwrap_or(0.0) * hrm_lv;
                        if act && level > 0.0 {
                            // smi normalized 0..1 → ±24 semitones; pan 0..1 → ±1.
                            let smi = (h.num(&format!("smi{i}")).unwrap_or(0.5) - 0.5) * 48.0;
                            let pan = h.num(&format!("pan{i}")).unwrap_or(0.5).mul_add(2.0, -1.0);
                            let shape = h.num(&format!("wfm{i}")).unwrap_or(0.0).clamp(0.0, 1.0);
                            layer
                                .harmonia
                                .push((level.clamp(0.0, 1.0), smi.round(), pan, shape));
                        }
                    }
                }
            }
        }
        if let Some(dfs) = voice.find("DFS") {
            if dfs.num("on").unwrap_or(0.0) != 0.0 {
                // freq normalized 0.5 = no shift → ±2 kHz (CALIBRATE);
                // inv flips the direction.
                let hz = |f: Option<f32>, inv: bool| {
                    let v = (f.unwrap_or(0.5) - 0.5) * 4000.0;
                    if inv { -v } else { v }
                };
                layer.dfs = Some((
                    hz(dfs.num("freqA"), dfs.num("invA").unwrap_or(0.0) != 0.0),
                    dfs.num("mixA").unwrap_or(0.5).clamp(0.0, 1.0),
                    hz(dfs.num("freqB"), dfs.num("invB").unwrap_or(0.0) != 0.0),
                    dfs.num("mixB").unwrap_or(0.5).clamp(0.0, 1.0),
                    dfs.num("parl").unwrap_or(0.0) != 0.0,
                ));
            }
        }
        if let Some(ws) = voice.child("WAVESHAPER") {
            if ws.num("act").unwrap_or(0.0) > 0.0 {
                let drive = ws.num("dpth").unwrap_or(0.0).clamp(0.0, 1.0);
                let crush = ws.num("bc").unwrap_or(0.0).clamp(0.0, 1.0);
                let reduce = ws.num("srrdc").unwrap_or(0.0).clamp(0.0, 1.0);
                let mix = ws.num("mix").unwrap_or(1.0).clamp(0.0, 1.0);
                if drive > 0.0 || crush > 0.0 || reduce > 0.0 {
                    layer.shaper = Some((drive, crush, reduce, mix));
                }
            }
        }
        if let Some(rack) = voice.child("EFFRACK") {
            layer.fx = rack_types(rack);
        }
        patch.layers.push(layer);
    }

    // Part racks: the SYNTHENG-level EFFRACK is the Common rack.
    if let Some(rack) = engine.child("EFFRACK") {
        patch.common_fx = rack_types(rack);
    }
    if let Some(rack) = engine.child("AUXEFFRACK") {
        patch.aux_fx = rack_types(rack);
    }

    // Mod matrix: flat sourceN/targetN attribute pairs.
    if let Some(matrix) = engine.child("MOD_MATRIX") {
        for n in 0..64 {
            let (Some(source), Some(target)) = (
                matrix.attr(&format!("source{n}")),
                matrix.attr(&format!("target{n}")),
            ) else {
                break;
            };
            let off = |s: &str| s.is_empty() || s.eq_ignore_ascii_case("off");
            if off(source) || off(target) || matrix.num(&format!("mute{n}")).unwrap_or(0.0) != 0.0 {
                continue;
            }
            let lo = matrix.num(&format!("lo{n}")).unwrap_or(0.0);
            patch.mod_routes.push(OmniModRoute {
                source: source.to_string(),
                target: target.to_string(),
                depth: matrix.num(&format!("hi{n}")).unwrap_or(0.0) - lo,
                offset: lo,
            });
        }
    }

    patch.glide_s = 12.2
        * engine
            .num("portV2")
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
            .powf(2.12);

    // Mod envelopes: points, plus each one's settings (the first in
    // MODENVPARAMS, the rest in MOD_ENV2_2, in order).
    let settings: Vec<&XmlNode> = engine
        .child("MODENVPARAMS")
        .into_iter()
        .chain(engine.children_tagged("MOD_ENV2_2"))
        .collect();
    for (i, env) in engine.children_tagged("MODENV").enumerate() {
        let points = env
            .children_tagged("p")
            .map(|p| {
                let bits = p
                    .attr("s")
                    .and_then(|v| u32::from_str_radix(v, 16).ok())
                    .unwrap_or(0);
                (
                    p.num("t").unwrap_or(0.0).max(0.0) * 100.0,
                    p.num("l").unwrap_or(0.0).clamp(0.0, 1.0),
                    42.0 * (0.5 - p.num("c").unwrap_or(0.5).clamp(0.0, 1.0)),
                    bits & 1 != 0,
                )
            })
            .collect();
        let set = settings.get(i);
        let flag = |k: &str| set.and_then(|s| s.num(k)).unwrap_or(0.0) != 0.0;
        patch.mod_envs.push(OmniModEnv {
            points,
            looping: flag("lp"),
            synced: flag("sync"),
            velsens: set
                .and_then(|s| s.num("velsens"))
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
        });
    }

    if let Some(set) = engine.child("LFO_SET") {
        for lfo in set.children_tagged("LFO") {
            patch.lfos.push(OmniLfo {
                rate: lfo.num("rate").unwrap_or(0.25).clamp(0.0, 1.0),
                wave_type: lfo.num("type").unwrap_or(0.0).clamp(0.0, 1.0),
                sync: lfo.num("sync").unwrap_or(0.0) != 0.0,
                retrigger: lfo.num("resettr").unwrap_or(0.0) != 0.0,
                swing: lfo.num("swing").unwrap_or(1.0).clamp(0.0, 1.0),
                unipolar: lfo.num("unidir").unwrap_or(0.0) != 0.0,
            });
        }
    }

    if let Some(arp) = root.find("ARP") {
        patch.arp_on = arp.num("ArpOnOff").unwrap_or(0.0) != 0.0;
        if let Some(seq) = arp.find("ARPSEQ2") {
            let tpq = seq.num("TICKSPERQUARTER").unwrap_or(1200.0).max(1.0);
            let mut raw: Vec<(f32, f32, u8)> = seq
                .children_tagged("SLICESEQSTEP")
                .map(|s| {
                    (
                        s.num("BEGIN").unwrap_or(0.0),
                        s.num("END").unwrap_or(0.0),
                        s.num("VEL").unwrap_or(0.0) as u8,
                    )
                })
                .collect();
            raw.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Step length = the spacing between step starts (1/16 = TPQ/4).
            let step_ticks = raw
                .windows(2)
                .map(|w| w[1].0 - w[0].0)
                .find(|d| *d > 0.0)
                .unwrap_or(tpq / 4.0);
            patch.arp_step_beats = step_ticks / tpq;
            patch.arp_steps = raw
                .iter()
                .map(|(b, e, v)| {
                    let gate = ((e - b) / step_ticks).clamp(0.05, 1.0);
                    (*v > 0, (*v).max(1), gate)
                })
                .collect();
        }
    }

    Ok(patch)
}
