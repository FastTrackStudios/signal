//! Native **Harmonic** block → the **City Grand** physically-modeled piano.
//!
//! A modal-synthesis voice: each note is a bank of damped sinusoids whose
//! frequency / amplitude / two-stage decay were measured from the owned sample
//! set (the `pm sweep` table), plus register-correct detuned unison strings
//! (beating), per-mode frequency jitter, and a hammer attack-noise burst. This
//! is the realtime form of the research crate's `synth` — quadrature
//! oscillators and exponential-multiplier envelopes, no `sin`/`exp` per sample.
//!
//! The per-note parameter table is loaded once at construction from
//! `$CITY_GRAND_TABLE` or `~/.config/signal/city-grand/table.json`. If absent,
//! the voice falls back to an analytic inharmonic partial series so a
//! `Harmonic` block still makes sound.

use std::collections::HashMap;

use serde::Deserialize;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

const TAU: f32 = std::f32::consts::TAU;

// ── Voicing (the City Grand model config, baked in) ──────────────────────────
const UNISON_TWO_BREAK: u8 = 28; // < this: 1 string
const UNISON_THREE_BREAK: u8 = 40; // < this: 2 strings, else 3
const DETUNE_CENTS: f32 = 0.6;
const JITTER_SIGMA: f32 = 0.0004;
const JITTER_TAU: f32 = 0.020;
const ATTACK_AMP: f32 = 0.05;
const ATTACK_TAU: f32 = 0.004;
const ATTACK_DUR: f32 = 0.015;
const ATTACK_CENTER_MULT: f32 = 5.0;
const MASTER_GAIN: f32 = 0.6;

// ── Parameter table ──────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
struct Partial {
    #[allow(dead_code)]
    k: u32,
    freq: f32,
    amp: f32,
    decay_fast: f32,
    decay_slow: f32,
    mix: f32,
}

/// Stochastic residual (SMS body) — measured broadband noise the sinusoids
/// can't make. `band_gain`/`band_hz` shape it; `level`/`decay` set its envelope.
#[derive(Deserialize, Clone, Default)]
struct Residual {
    band_gain: Vec<f32>,
    band_hz: Vec<f32>,
    decay: f32,
    level: f32,
}

/// One sampled cell. Unknown JSON fields (f0, B, T60, peak_rms) are ignored.
#[derive(Deserialize)]
struct Rec {
    note: u8,
    vel: u8,
    modal: Vec<Partial>,
    #[serde(default)]
    residual: Residual,
}

#[derive(Clone)]
struct Voicing {
    modal: Vec<Partial>,
    residual: Residual,
}

struct Table {
    by_note: HashMap<u8, Vec<(u8, Voicing)>>,
    notes: Vec<u8>,
}

fn midi_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

impl Table {
    fn load() -> Option<Self> {
        let path = std::env::var("CITY_GRAND_TABLE").ok().or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{h}/.config/signal/city-grand/table.json"))
        })?;
        let text = std::fs::read_to_string(&path).ok()?;
        let recs: Vec<Rec> = serde_json::from_str(&text).ok()?;
        if recs.is_empty() {
            return None;
        }
        let mut by_note: HashMap<u8, Vec<(u8, Voicing)>> = HashMap::new();
        for r in recs {
            by_note.entry(r.note).or_default().push((
                r.vel,
                Voicing {
                    modal: r.modal,
                    residual: r.residual,
                },
            ));
        }
        for v in by_note.values_mut() {
            v.sort_by_key(|(vel, _)| *vel);
        }
        let mut notes: Vec<u8> = by_note.keys().copied().collect();
        notes.sort_unstable();
        tracing::info!("City Grand: loaded {} sampled notes from {path}", notes.len());
        Some(Self { by_note, notes })
    }

    /// Voicing for (note, vel) + a frequency scale to transpose notes outside
    /// the sampled set.
    fn lookup(&self, note: u8, vel: u8) -> Option<(&Voicing, f32)> {
        let src = if self.by_note.contains_key(&note) {
            note
        } else {
            *self
                .notes
                .iter()
                .min_by_key(|&&n| (n as i32 - note as i32).abs())?
        };
        let layers = self.by_note.get(&src)?;
        let (_, v) = layers
            .iter()
            .min_by_key(|(lv, _)| (*lv as i32 - vel as i32).abs())?;
        Some((v, midi_hz(note) / midi_hz(src)))
    }
}

/// Analytic fallback: an inharmonic damped partial series for one note.
fn analytic_partials(note: u8) -> Vec<Partial> {
    let f0 = midi_hz(note);
    let b = 3.0e-4; // inharmonicity
    let mut out = Vec::new();
    for k in 1..=24u32 {
        let kf = k as f32;
        let freq = f0 * kf * (1.0 + b * kf * kf).sqrt();
        if freq >= 20_000.0 {
            break;
        }
        let decay = 1.0 + 0.5 * kf; // higher partials decay faster
        out.push(Partial {
            k,
            freq,
            amp: 0.2 / kf,
            decay_fast: decay * 4.0,
            decay_slow: decay,
            mix: 0.7,
        });
    }
    out
}

// ── DSP ──────────────────────────────────────────────────────────────────────

#[inline]
fn lcg(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    let u = (*state >> 1) as f32 / (u32::MAX as f32 / 2.0);
    (u * 2.0 - 1.0) * 1.7320508
}

/// One partial of one string.
struct Osc {
    s: f32,
    c: f32,
    cos_inc: f32,
    sin_inc: f32,
    phase_inc: f32,
    amp: f32,
    mix: f32,
    fast_env: f32,
    slow_env: f32,
    fast_mult: f32,
    slow_mult: f32,
    drift: f32,
    rng: u32,
}

#[derive(Default)]
struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl Biquad {
    fn bandpass(center: f32, q: f32, sr: f32) -> Self {
        let w0 = TAU * center / sr;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: -2.0 * cs / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Realtime SMS residual: white noise through a (band-merged) filterbank,
/// calibrated once at note-on and enveloped by its own decay. Capped at
/// `MAX_RES_BANDS` filters per voice to bound polyphonic CPU.
const MAX_RES_BANDS: usize = 16;

struct ResidualGen {
    filters: Vec<(Biquad, f32)>, // (bandpass, gain)
    scale: f32,                  // level / warmup-RMS calibration
    env: f32,
    decay_mult: f32,
    rng: u32,
}

impl ResidualGen {
    fn build(res: &Residual, freq_scale: f32, sr: f32, seed: u32) -> Option<Self> {
        if res.level <= 0.0 || res.band_gain.is_empty() {
            return None;
        }
        // Merge the stored bands down to at most MAX_RES_BANDS (energy-summed
        // gains, geometric-mean centres).
        let n = res.band_gain.len().min(res.band_hz.len());
        let group = (n + MAX_RES_BANDS - 1) / MAX_RES_BANDS;
        let mut filters = Vec::new();
        let mut i = 0;
        while i < n {
            let end = (i + group).min(n);
            let mut g2 = 0.0f32;
            let mut logf = 0.0f32;
            let mut cnt = 0.0f32;
            for j in i..end {
                g2 += res.band_gain[j] * res.band_gain[j];
                if res.band_hz[j] > 0.0 {
                    logf += res.band_hz[j].ln();
                    cnt += 1.0;
                }
            }
            if cnt > 0.0 {
                let fc = (logf / cnt).exp() * freq_scale;
                if fc > 0.0 && fc < sr * 0.5 {
                    filters.push((Biquad::bandpass(fc, 4.0, sr), g2.sqrt()));
                }
            }
            i = end;
        }
        if filters.is_empty() {
            return None;
        }
        // The stochastic (noise) residual is the WRONG model for piano body:
        // the real inter-partial energy is tonal (sympathetic strings +
        // soundboard ring), so filtered noise reads as hiss. Off by default;
        // the tonal body comes from the SympatheticBank instead. Kept as an
        // optional layer, tunable live via $CITY_GRAND_RESIDUAL.
        let user_gain = std::env::var("CITY_GRAND_RESIDUAL")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0)
            .max(0.0);
        if user_gain <= 0.0 {
            return None;
        }
        let mut gen = Self {
            filters,
            scale: 1.0,
            env: 1.0,
            decay_mult: (-res.decay / sr).exp(),
            rng: seed.max(1),
        };
        // Warmup: measure the filterbank's output RMS on unit noise, calibrate
        // so the residual's onset RMS equals the measured `level`, then apply the
        // perceptual/polyphony gain.
        let warm = 2048usize;
        let mut acc = 0.0f32;
        for _ in 0..warm {
            let v = gen.raw();
            acc += v * v;
        }
        let raw_rms = (acc / warm as f32).sqrt();
        gen.scale = if raw_rms > 1e-9 {
            user_gain * res.level / raw_rms
        } else {
            0.0
        };
        Some(gen)
    }

    #[inline]
    fn raw(&mut self) -> f32 {
        let noise = lcg(&mut self.rng) / 1.7320508;
        let mut s = 0.0;
        for (bp, g) in &mut self.filters {
            s += *g * bp.process(noise);
        }
        s
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        let out = self.scale * self.env * self.raw();
        self.env *= self.decay_mult;
        out
    }
}

// ── Sympathetic resonance ────────────────────────────────────────────────────
// A bank of string resonators tuned to the keyboard's measured frequencies,
// driven by the playing voices. A string rings sympathetically only when it is
// UNDAMPED but NOT being played (sustain pedal down, or the top ~octave which
// has no dampers) — so pedal-up adds nothing and pedal-down blooms the
// un-struck strings, exactly like a real piano. Tonal (rings at string
// frequencies), so no hiss. Driven by the dry voice sum → no feedback.

const SYMP_PARTIALS: usize = 3;

/// A ringing two-pole resonator: y[n] = (1-R)x[n] + 2R·cos(w)·y[n-1] − R²·y[n-2].
/// Unit gain at resonance; rings at `freq` with decay set by pole radius `r`.
struct Reso {
    a1: f32,
    a2: f32,
    ingain: f32,
    y1: f32,
    y2: f32,
}
impl Reso {
    fn new(freq: f32, r: f32, sr: f32) -> Self {
        let w = TAU * freq / sr;
        Self {
            a1: 2.0 * r * w.cos(),
            a2: r * r,
            ingain: 1.0 - r,
            y1: 0.0,
            y2: 0.0,
        }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.ingain * x + self.a1 * self.y1 - self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
    #[inline]
    fn bleed(&mut self, f: f32) {
        self.y1 *= f;
        self.y2 *= f;
    }
}

struct SympString {
    note: u8,
    resos: Vec<Reso>,
    gain: f32, // smoothed damper gain, 0 (damped) .. 1 (ringing)
}

struct SympatheticBank {
    strings: Vec<SympString>,
    held: [bool; 128],
    pedal: bool,
    mix: f32,
    up_coef: f32,
    down_coef: f32,
}

impl SympatheticBank {
    fn build(table: &Table, sr: f32) -> Option<Self> {
        let mix = std::env::var("CITY_GRAND_SYMPATHETIC")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.4)
            .max(0.0);
        if mix <= 0.0 {
            return None;
        }
        let t60 = std::env::var("CITY_GRAND_SYMP_T60")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(2.5)
            .max(0.2);
        let r = 10f32.powf(-3.0 / (t60 * sr));

        let mut strings = Vec::new();
        for (&note, layers) in table.by_note.iter() {
            // representative voicing: the middle velocity layer
            let (_, v) = &layers[layers.len() / 2];
            let mut resos = Vec::new();
            for p in v.modal.iter().take(SYMP_PARTIALS) {
                if p.freq > 20.0 && p.freq < sr * 0.5 {
                    resos.push(Reso::new(p.freq, r, sr));
                }
            }
            if !resos.is_empty() {
                strings.push(SympString {
                    note,
                    resos,
                    gain: 0.0,
                });
            }
        }
        if strings.is_empty() {
            return None;
        }
        tracing::info!(
            "City Grand: sympathetic bank {} strings, mix {mix}, T60 {t60}s",
            strings.len()
        );
        Some(Self {
            strings,
            held: [false; 128],
            pedal: false,
            mix,
            up_coef: (-1.0 / (0.003 * sr)).exp().mul_add(-1.0, 1.0), // ~3ms undamp
            down_coef: (-1.0 / (0.08 * sr)).exp().mul_add(-1.0, 1.0), // ~80ms damper fall
        })
    }

    #[inline]
    fn process(&mut self, drive: f32) -> f32 {
        let mut out = 0.0f32;
        for s in &mut self.strings {
            // ring only if undamped (pedal / no-damper zone) AND not being played
            let undamped = (self.pedal || s.note >= 88) && !self.held[s.note as usize];
            let target = if undamped { 1.0 } else { 0.0 };
            let coef = if target > s.gain { self.up_coef } else { self.down_coef };
            s.gain += (target - s.gain) * coef;
            if s.gain < 1e-4 {
                // damped: bleed stored energy so it doesn't resume on re-pedal
                for r in &mut s.resos {
                    r.bleed(0.9995);
                }
                continue;
            }
            let mut r = 0.0;
            for reso in &mut s.resos {
                r += reso.process(drive);
            }
            out += s.gain * r;
        }
        self.mix * out
    }
}

struct Voice {
    note: u8,
    oscs: Vec<Osc>,
    res: Option<ResidualGen>,
    gain: f32,
    atk_remaining: u32,
    atk_amp: f32,
    atk_decay: f32,
    atk_bpf: Biquad,
    atk_rng: u32,
    rel_env: f32,
    rel_mult: f32,
    releasing: bool,
    sample: u64,
    dead: bool,
}

fn strings_per_note(note: u8) -> usize {
    if note < UNISON_TWO_BREAK {
        1
    } else if note < UNISON_THREE_BREAK {
        2
    } else {
        3
    }
}

fn detune_factors(n: usize) -> Vec<f32> {
    if n <= 1 {
        return vec![1.0];
    }
    (0..n)
        .map(|i| {
            let frac = i as f32 / (n as f32 - 1.0) - 0.5;
            2f32.powf(frac * DETUNE_CENTS / 1200.0)
        })
        .collect()
}

/// The City Grand voice — a polyphonic modal-synthesis `PluginInstance`.
pub struct NativeModal {
    sample_rate: f32,
    table: Option<Table>,
    voices: Vec<Voice>,
    symp: Option<SympatheticBank>,
    prepared: bool,
    jitter_revert: f32,
    jitter_diffusion: f32,
}

impl NativeModal {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(1) as f32;
        let revert = (-1.0 / (JITTER_TAU * sr)).exp();
        let diffusion = JITTER_SIGMA * (1.0 - revert * revert).sqrt();
        let table = Table::load();
        let symp = table.as_ref().and_then(|t| SympatheticBank::build(t, sr));
        Self {
            sample_rate: sr,
            table,
            voices: Vec::new(),
            symp,
            prepared: false,
            jitter_revert: revert,
            jitter_diffusion: diffusion,
        }
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    fn voicing_for(&self, note: u8, vel: u8) -> (Vec<Partial>, Residual, f32) {
        match self.table.as_ref().and_then(|t| t.lookup(note, vel)) {
            Some((v, scale)) => (v.modal.clone(), v.residual.clone(), scale),
            None => (analytic_partials(note), Residual::default(), 1.0),
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        let sr = self.sample_rate;
        let (partials, residual, scale) = self.voicing_for(note, velocity);
        let n_strings = strings_per_note(note);
        let detunes = detune_factors(n_strings);
        let g_string = 1.0 / (n_strings as f32).sqrt();
        let base_seed = ((note as u32) << 8) ^ (velocity as u32).wrapping_mul(2654435761);

        let mut oscs = Vec::with_capacity(partials.len() * n_strings);
        for (si, &d) in detunes.iter().enumerate() {
            for p in &partials {
                let freq = p.freq * d * scale;
                if freq <= 0.0 || freq >= sr * 0.5 {
                    continue;
                }
                let phase_inc = TAU * freq / sr;
                let (sin_inc, cos_inc) = phase_inc.sin_cos();
                oscs.push(Osc {
                    s: 0.0,
                    c: 1.0,
                    cos_inc,
                    sin_inc,
                    phase_inc,
                    amp: p.amp * g_string,
                    mix: p.mix,
                    fast_env: 1.0,
                    slow_env: 1.0,
                    fast_mult: (-p.decay_fast / sr).exp(),
                    slow_mult: (-p.decay_slow / sr).exp(),
                    drift: 0.0,
                    rng: base_seed
                        .wrapping_add((si as u32) << 16)
                        .wrapping_add(p.k.wrapping_mul(40503))
                        .max(1),
                });
            }
        }

        let vel01 = (velocity as f32 / 127.0).clamp(0.0, 1.0);
        let f0 = partials.first().map(|p| p.freq * scale).unwrap_or(440.0);
        let center = (f0 * ATTACK_CENTER_MULT).clamp(200.0, 2000.0);

        let res = ResidualGen::build(&residual, scale, sr, base_seed ^ 0x5bd1_e995);

        if let Some(s) = &mut self.symp {
            s.held[note as usize] = true;
        }

        self.voices.push(Voice {
            note,
            oscs,
            res,
            gain: MASTER_GAIN,
            atk_remaining: (ATTACK_DUR * sr) as u32,
            atk_amp: ATTACK_AMP * vel01 * vel01,
            atk_decay: (-1.0 / (ATTACK_TAU * sr)).exp(),
            atk_bpf: Biquad::bandpass(center, 0.7, sr),
            atk_rng: base_seed ^ 0x9e3779b9,
            rel_env: 1.0,
            rel_mult: 1.0,
            releasing: false,
            sample: 0,
            dead: false,
        });
    }

    fn note_off(&mut self, note: u8) {
        if let Some(s) = &mut self.symp {
            s.held[note as usize] = false;
        }
        let sr = self.sample_rate;
        // register-dependent damper; top notes ring on (no damper).
        let rel_mult = if note >= 100 {
            1.0
        } else {
            let t = if note < 48 {
                0.30
            } else if note < 72 {
                0.18
            } else {
                0.10
            };
            (-1.0 / (t * sr)).exp()
        };
        for v in self.voices.iter_mut().filter(|v| v.note == note && !v.releasing) {
            v.releasing = true;
            v.rel_mult = rel_mult;
        }
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => self.note_on(key.get(), velocity.get()),
            MidiEvent::NoteOff { key, .. } => self.note_off(key.get()),
            MidiEvent::ControlChange { controller, value, .. } => {
                if controller.get() == 64 {
                    if let Some(s) = &mut self.symp {
                        s.pedal = value.get() >= 64;
                    }
                }
            }
            _ => {}
        }
    }
}

impl PluginInstance for NativeModal {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.city_grand".into(),
            name: "City Grand".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        let new_sr = sample_rate.max(1.0) as f32;
        if (new_sr - self.sample_rate).abs() > f32::EPSILON {
            self.sample_rate = new_sr;
            let revert = (-1.0 / (JITTER_TAU * new_sr)).exp();
            self.jitter_revert = revert;
            self.jitter_diffusion = JITTER_SIGMA * (1.0 - revert * revert).sqrt();
            self.voices.clear(); // re-pitching a modal bank is not worth it; drop tails
            self.symp = self
                .table
                .as_ref()
                .and_then(|t| SympatheticBank::build(t, new_sr));
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for ev in events.midi {
            self.apply_midi(&ev.message);
        }
        let frames = out_l.len().min(out_r.len());
        for s in out_l[..frames].iter_mut() {
            *s = 0.0;
        }

        let revert = self.jitter_revert;
        let diffusion = self.jitter_diffusion;

        for v in &mut self.voices {
            let mut alive = false;
            for (i, out_s) in out_l[..frames].iter_mut().enumerate() {
                let mut sum = 0.0f32;
                for o in &mut v.oscs {
                    let env = o.amp * (o.mix * o.fast_env + (1.0 - o.mix) * o.slow_env);
                    sum += env * o.s;
                    if (v.sample + i as u64) & 15 == 0 {
                        o.drift = revert * o.drift + diffusion * lcg(&mut o.rng);
                    }
                    let dph = o.drift * o.phase_inc;
                    let ci = o.cos_inc - dph * o.sin_inc;
                    let si = o.sin_inc + dph * o.cos_inc;
                    let s_new = o.s * ci + o.c * si;
                    let c_new = o.c * ci - o.s * si;
                    o.s = s_new;
                    o.c = c_new;
                    o.fast_env *= o.fast_mult;
                    o.slow_env *= o.slow_mult;
                }
                if v.atk_remaining > 0 {
                    let noise = lcg(&mut v.atk_rng) / 1.7320508;
                    sum += v.atk_amp * v.atk_bpf.process(noise);
                    v.atk_amp *= v.atk_decay;
                    v.atk_remaining -= 1;
                }
                // stochastic residual (SMS body)
                if let Some(res) = &mut v.res {
                    sum += res.tick();
                }
                if v.releasing {
                    v.rel_env *= v.rel_mult;
                }
                let out_v = (sum * v.gain * v.rel_env).tanh();
                *out_s += out_v;
                if out_v.abs() > 1e-5 {
                    alive = true;
                }
            }
            // periodic quadrature renorm (long piano decays)
            for o in &mut v.oscs {
                let r2 = o.s * o.s + o.c * o.c;
                if r2 > 0.0 {
                    let inv = 1.0 / r2.sqrt();
                    o.s *= inv;
                    o.c *= inv;
                }
            }
            v.sample += frames as u64;
            if (v.releasing && v.rel_env < 1e-4)
                || (!alive && v.sample > self.sample_rate as u64 / 4)
            {
                v.dead = true;
            }
        }
        self.voices.retain(|v| !v.dead);

        // Sympathetic resonance: the dry voice sum drives the string-resonator
        // bank; the bloom is added back. State rings across blocks; the drive is
        // recomputed each block, so there's no feedback loop.
        if let Some(symp) = &mut self.symp {
            for s in out_l[..frames].iter_mut() {
                let sym = symp.process(*s);
                *s = (*s + sym).tanh();
            }
        }

        // mono → both channels
        out_r[..frames].copy_from_slice(&out_l[..frames]);
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
        self.voices.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

    fn note_on(note: u8, vel: u8) -> PluginMidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOn {
                channel: Channel::new(0),
                key: KeyNumber::new(note),
                velocity: Velocity::new(vel),
            },
        }
    }

    #[test]
    fn note_on_generates_non_silent_audio() {
        // No table in the test env → analytic fallback, still audible.
        let mut m = NativeModal::new(48_000);
        m.prepare(48_000.0, 512).unwrap();
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(60, 100)];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        m.process_block(&inl, &inr, &mut outl, &mut outr, &ev).unwrap();
        assert_eq!(m.active_voices(), 1);
        let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
        assert!(rms > 1e-4, "modal voice should be audible, rms={rms}");
    }
}
