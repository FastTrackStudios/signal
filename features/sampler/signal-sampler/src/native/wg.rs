//! Native **Harmonic** block → the **City Grand** coupled-waveguide piano.
//!
//! The realtime form of the research crate's waveguide engine
//! (`research/piano-model/src/waveguide.rs`) — a coupled, nonlinear physical
//! simulation (the Pianoteq paradigm: sound is computed, not stored):
//!
//! - stiff-string digital waveguide per string (delay + loss LP + dispersion
//!   allpass cascade + exact-tuning allpass, all designed numerically from
//!   exact phase delays);
//! - 1–3 unison strings per note through a **passive bridge junction**
//!   (Smith PASP) — the two-stage prompt/aftersound decay and unison beating
//!   EMERGE from the coupling (Weinreich 1977);
//! - a **nonlinear felt hammer** ODE integrated at note-on (Chaigne &
//!   Askenfelt) — velocity→brightness comes from physics, not curves.
//!
//! Per-note parameters (stretch-tuned f0, inharmonicity B, dispersion order,
//! aftersound t60, bridge impedance zb, brightness) are trained against the
//! owned Keyscape library by `pm wg-table` and loaded from
//! `$CITY_GRAND_WG_TABLE` or `~/.config/signal/city-grand/wg-table.json`.
//! Without a table the voice falls back to analytic register curves.

use serde::Deserialize;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

const TAU: f32 = std::f32::consts::TAU;
/// Output level: v_J = g·Σ with g = 2/(N+zb), so raw bridge velocity varies
/// ~400× across the table's zb range. Normalize by 1/g (zb then shapes decay,
/// not loudness) and scale to a sane instrument level.
const MASTER_GAIN: f32 = 0.6;
/// Damper release: −60 dB in ~0.25 s.
const RELEASE_T60: f32 = 0.25;

// ── The waveguide engine (mirrors research/piano-model/src/waveguide.rs) ────

#[derive(Clone, Copy, Default)]
struct Allpass {
    a: f32,
    x1: f32,
    y1: f32,
}
impl Allpass {
    fn new(a: f32) -> Self {
        Self { a, x1: 0.0, y1: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.x1 - self.a * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

fn ap_phase_delay(a: f64, w: f64) -> f64 {
    let (s, c) = w.sin_cos();
    let phi = (-s).atan2(a + c) - (-a * s).atan2(1.0 + a * c);
    -phi / w
}

fn lp_phase_delay(d: f64, w: f64) -> f64 {
    let phi = -(d * w.sin()).atan2(1.0 - d * w.cos());
    -phi / w
}

/// Numerically solve the loop (dispersion coeff, integer delay, tuning coeff)
/// so partial 1 lands on f0 and partial k on the stiff-string target.
fn design_loop(f0: f64, b: f64, m: usize, d: f64, sr: f64) -> (usize, f32, f32) {
    let w0 = std::f64::consts::TAU * f0 / sr;
    let k = ((0.4 * sr / f0).floor() as usize).clamp(2, 12) as f64;
    let solve = |a: f64| -> (f64, f64, f64) {
        let period = sr / f0;
        let fixed = m as f64 * ap_phase_delay(a, w0) + lp_phase_delay(d, w0);
        let dline = period - fixed;
        let n = (dline - 1.0).floor().max(2.0);
        let mut frac = (dline - n).clamp(0.05, 1.95);
        let mut ta = (1.0 - frac) / (1.0 + frac);
        for _ in 0..4 {
            let err = (dline - n) - ap_phase_delay(ta, w0);
            frac = (frac + err).clamp(0.02, 1.98);
            ta = (1.0 - frac) / (1.0 + frac);
        }
        let mut fk = k * f0;
        for _ in 0..24 {
            let w = std::f64::consts::TAU * fk / sr;
            let tau =
                n + ap_phase_delay(ta, w) + m as f64 * ap_phase_delay(a, w) + lp_phase_delay(d, w);
            fk = k * sr / tau;
        }
        (n, ta, fk)
    };
    let fk_target = k * f0 * (1.0 + b * k * k).sqrt();
    let (mut lo, mut hi) = (-0.6f64, 0.0f64);
    let mut a = 0.0;
    if b > 1e-9 {
        for _ in 0..40 {
            a = 0.5 * (lo + hi);
            let (_, _, fk) = solve(a);
            if fk < fk_target {
                hi = a;
            } else {
                lo = a;
            }
        }
    }
    let (n, ta, _) = solve(a);
    (n as usize, ta as f32, a as f32)
}

struct StringWaveguide {
    buf: Vec<f32>,
    n: usize,
    idx: usize,
    loop_gain: f32,
    d: f32,
    lp: f32,
    tune: Allpass,
    disp: Vec<Allpass>,
    sr: f32,
    exc: Vec<f32>,
    exc_pos: usize,
}

impl StringWaveguide {
    fn new(f0: f32, t60: f32, brightness: f32, b: f32, n_disp: usize, sr: u32) -> Self {
        let sr = sr as f32;
        let d = (1.0 - brightness).clamp(0.0, 0.98) * 0.5;
        let m = n_disp.max(1);
        let (n, tune_a, disp_a) = design_loop(f0 as f64, b as f64, m, d as f64, sr as f64);
        // T60 loop gain COMPENSATED for the loss LP's own attenuation at f0
        // (uncompensated it adds −96 dB/s in the treble — the "plucks" bug).
        let w0 = TAU * f0 / sr;
        let hlp = (1.0 - d) / (1.0 - 2.0 * d * w0.cos() + d * d).sqrt();
        let loops = (f0 * t60).max(1.0);
        let loop_gain = (10f32.powf(-3.0 / loops) / hlp.max(1e-3)).min(0.99995);
        Self {
            buf: vec![0.0; n.max(2)],
            n: n.max(2),
            idx: 0,
            loop_gain,
            d,
            lp: 0.0,
            tune: Allpass::new(tune_a),
            disp: (0..m).map(|_| Allpass::new(disp_a)).collect(),
            sr,
            exc: Vec::new(),
            exc_pos: 0,
        }
    }

    /// Nonlinear felt hammer (Chaigne–Askenfelt), integrated at note-on.
    fn strike(&mut self, vel01: f32, strike_pos: f32) {
        let f0_est = self.sr / self.n as f32;
        let g = (f0_est / 220.0).clamp(0.1, 20.0);
        let m = (0.009 * g.powf(-0.3)).clamp(0.004, 0.014);
        let k = (1.5e9 * g.powf(1.5)).clamp(1e7, 1e11);
        let p_exp = 2.8f32;
        let two_r = 10.0f32;
        let dt = 1.0 / self.sr;
        let v0 = 1.2 + 4.3 * vel01;
        let (mut xh, mut vh, mut ys) = (0.0f32, v0, 0.0f32);
        let mut pulse: Vec<f32> = Vec::with_capacity(256);
        let g_exc = 0.02;
        for _ in 0..(0.02 * self.sr) as usize {
            let u = xh - ys;
            if u <= 0.0 && !pulse.is_empty() {
                break;
            }
            let f = if u > 0.0 { k * u.powf(p_exp) } else { 0.0 };
            vh -= f / m * dt;
            xh += vh * dt;
            ys += f / two_r * dt;
            pulse.push(g_exc * f / two_r);
        }
        // strike-point comb: subtract a copy delayed by the node distance.
        // On short treble delay lines the node distance degenerates to 1–2
        // samples and the comb becomes a differencer that guts the excitation
        // — skip it there (physically: the hammer contact patch spans the
        // whole node spacing anyway).
        let dcomb = (strike_pos * self.n as f32) as usize;
        if dcomb >= 3 && dcomb < self.n {
            let orig = pulse.clone();
            for i in dcomb..pulse.len() {
                pulse[i] -= orig[i - dcomb];
            }
        }
        self.exc = pulse;
        self.exc_pos = 0;
    }

    fn scale_exc(&mut self, k: f32) {
        for x in &mut self.exc {
            *x *= k;
        }
    }
    fn delay_exc(&mut self, samples: usize) {
        if samples > 0 {
            self.exc.splice(0..0, std::iter::repeat(0.0).take(samples));
        }
    }

    #[inline]
    fn reflect(&mut self) -> f32 {
        let out = self.buf[self.idx];
        self.lp = (1.0 - self.d) * out + self.d * self.lp;
        let mut s = self.lp;
        for ap in &mut self.disp {
            s = ap.process(s);
        }
        self.loop_gain * self.tune.process(s)
    }

    #[inline]
    fn commit(&mut self, mut refl: f32) {
        if self.exc_pos < self.exc.len() {
            refl += self.exc[self.exc_pos];
            self.exc_pos += 1;
        }
        self.buf[self.idx] = refl;
        self.idx += 1;
        if self.idx >= self.n {
            self.idx = 0;
        }
    }
}

/// Unison strings through a passive bridge junction (see the research crate
/// for the derivation; symmetric mode = prompt, antisymmetric = aftersound).
struct CoupledStrings {
    strings: Vec<StringWaveguide>,
    g: f32,
    outs: Vec<f32>,
    skew: f32,
}

impl CoupledStrings {
    #[allow(clippy::too_many_arguments)]
    fn new(
        f0: f32,
        t60: f32,
        brightness: f32,
        b: f32,
        n_disp: usize,
        sr: u32,
        n_strings: usize,
        detune_cents: f32,
        zb: f32,
    ) -> Self {
        let n = n_strings.max(1);
        let strings: Vec<StringWaveguide> = (0..n)
            .map(|i| {
                let frac = if n > 1 { i as f32 / (n as f32 - 1.0) - 0.5 } else { 0.0 };
                let f = f0 * 2f32.powf(frac * detune_cents / 1200.0);
                StringWaveguide::new(f, t60, brightness, b, n_disp, sr)
            })
            .collect();
        let g = 2.0 / (n as f32 + zb.max(0.0));
        Self { strings, g, outs: vec![0.0; n], skew: 0.15 }
    }

    fn strike(&mut self, vel01: f32, strike_pos: f32) {
        let n = self.strings.len();
        for (i, s) in self.strings.iter_mut().enumerate() {
            s.strike(vel01, strike_pos);
            let frac = if n > 1 { i as f32 / (n as f32 - 1.0) - 0.5 } else { 0.0 };
            s.scale_exc(1.0 + self.skew * frac);
            let skew = (0.0003 * i as f32 * s.sr) as usize;
            s.delay_exc(skew);
        }
    }

    #[inline]
    fn process(&mut self) -> f32 {
        let mut sum = 0.0;
        for (o, s) in self.outs.iter_mut().zip(self.strings.iter_mut()) {
            *o = s.reflect();
            sum += *o;
        }
        let vj = self.g * sum;
        for (o, s) in self.outs.iter().zip(self.strings.iter_mut()) {
            s.commit(*o - vj);
        }
        vj
    }
}

// ── Per-note parameter table (pm wg-table output) ───────────────────────────

#[derive(Deserialize, Clone)]
struct WgNoteRow {
    note: u8,
    f0: f32,
    b: f32,
    n_disp: usize,
    t60: f32,
    zb: f32,
    brightness: f32,
    strike: f32,
    detune: f32,
    #[serde(default = "default_skew")]
    skew: f32,
    #[serde(default)]
    #[allow(dead_code)]
    body: Option<Vec<(f32, f32)>>,
}

fn default_skew() -> f32 {
    0.15
}

#[derive(Deserialize)]
struct WgTableFile {
    notes: Vec<WgNoteRow>,
}

fn load_table() -> Option<Vec<WgNoteRow>> {
    // resolution order: explicit table path → named preset (the pm preset
    // pipeline installs to ~/.config/signal/pianos/<name>.json) → the default
    // City Grand table.
    let path = std::env::var_os("CITY_GRAND_WG_TABLE")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let name = std::env::var("CITY_GRAND_PRESET").ok()?;
            let h = std::env::var_os("HOME")?;
            Some(std::path::PathBuf::from(h).join(format!(".config/signal/pianos/{name}.json")))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                std::path::PathBuf::from(h).join(".config/signal/city-grand/wg-table.json")
            })
        })?;
    let text = std::fs::read_to_string(path).ok()?;
    let t: WgTableFile = serde_json::from_str(&text).ok()?;
    (!t.notes.is_empty()).then_some(t.notes)
}

/// Analytic fallback curves when no trained table is present (register trends
/// measured from the C7: B valley, t60 64→6 s, zb ~600, dark bass hammers).
fn fallback_row(note: u8) -> WgNoteRow {
    let f0 = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
    let x = note as f32;
    WgNoteRow {
        note,
        f0,
        b: if x < 40.0 { 3.5e-4 - (x - 21.0) * 1.2e-5 } else { 1.1e-4 * 2f32.powf((x - 40.0) / 44.0 * 1.5) },
        n_disp: if x < 40.0 { 48 } else if x < 60.0 { 24 } else if x < 76.0 { 8 } else { 4 },
        t60: (64.0 * 2f32.powf(-(x - 21.0) / 26.0)).clamp(4.0, 64.0),
        zb: 600.0,
        brightness: 0.4 + 0.4 * ((x - 21.0) / 87.0),
        strike: 0.08,
        detune: 0.3,
        skew: 0.15,
        body: None,
    }
}

// ── The voice ────────────────────────────────────────────────────────────────

struct WgVoice {
    note: u8,
    cs: CoupledStrings,
    out_gain: f32,
    rel_env: f32,
    rel_mult: f32,
    releasing: bool,
    sustained: bool, // held only by CC64
    sample: u64,
    dead: bool,
}

pub struct NativeWaveguide {
    sample_rate: f32,
    prepared: bool,
    table: Option<Vec<WgNoteRow>>,
    voices: Vec<WgVoice>,
    pedal: bool,
}

impl NativeWaveguide {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            prepared: false,
            table: load_table(),
            voices: Vec::new(),
            pedal: false,
        }
    }

    #[cfg(test)]
    fn active_voices(&self) -> usize {
        self.voices.len()
    }

    fn row(&self, note: u8) -> WgNoteRow {
        self.table
            .as_ref()
            .and_then(|t| {
                t.iter()
                    .min_by_key(|r| (r.note as i32 - note as i32).abs())
                    .filter(|r| (r.note as i32 - note as i32).abs() <= 3)
                    .cloned()
            })
            .map(|mut r| {
                if r.note != note {
                    // shift a neighbor's params to this pitch
                    r.f0 *= 2f32.powf((note as f32 - r.note as f32) / 12.0);
                    r.note = note;
                }
                r
            })
            .unwrap_or_else(|| fallback_row(note))
    }

    fn note_on(&mut self, note: u8, vel: u8) {
        if vel == 0 {
            self.note_off(note);
            return;
        }
        // retrigger: release the old voice fast (real dampers act on restrike)
        for v in &mut self.voices {
            if v.note == note && !v.releasing {
                v.releasing = true;
                v.sustained = false;
            }
        }
        let r = self.row(note);
        let n_strings = if note < 28 { 1 } else if note < 40 { 2 } else { 3 };
        let sr = self.sample_rate as u32;
        let mut cs = CoupledStrings::new(
            r.f0, r.t60, r.brightness, r.b, r.n_disp, sr, n_strings, r.detune, r.zb,
        );
        cs.skew = r.skew;
        cs.strike((vel as f32 / 127.0).clamp(0.0, 1.0), r.strike);
        let rel_mult = (-6.908 / (RELEASE_T60 * self.sample_rate)).exp();
        let out_gain = MASTER_GAIN / cs.g.max(1e-6);
        self.voices.push(WgVoice {
            note,
            cs,
            out_gain,
            rel_env: 1.0,
            rel_mult,
            releasing: false,
            sustained: false,
            sample: 0,
            dead: false,
        });
    }

    fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.note == note && !v.releasing {
                if self.pedal {
                    v.sustained = true;
                } else {
                    v.releasing = true;
                }
            }
        }
    }

    fn pedal_change(&mut self, down: bool) {
        self.pedal = down;
        if !down {
            for v in &mut self.voices {
                if v.sustained {
                    v.sustained = false;
                    v.releasing = true;
                }
            }
        }
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => self.note_on(key.get(), velocity.get()),
            MidiEvent::NoteOff { key, .. } => self.note_off(key.get()),
            MidiEvent::ControlChange { controller, value, .. } => {
                if controller.get() == 64 {
                    self.pedal_change(value.get() >= 64);
                }
            }
            _ => {}
        }
    }
}

impl PluginInstance for NativeWaveguide {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.city_grand_wg".into(),
            name: "City Grand (waveguide)".into(),
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
            self.voices.clear(); // delay lengths are sr-dependent; drop tails
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
        for v in &mut self.voices {
            let mut alive = false;
            for out_s in out_l[..frames].iter_mut() {
                let mut y = v.cs.process();
                if v.releasing {
                    v.rel_env *= v.rel_mult;
                }
                y *= v.out_gain * v.rel_env;
                *out_s += y;
                if y.abs() > 1e-5 {
                    alive = true;
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
        for s in out_l[..frames].iter_mut() {
            *s = s.tanh();
        }
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
    fn waveguide_note_is_audible_and_tuned() {
        // No table in the test env → analytic fallback.
        let mut m = NativeWaveguide::new(48_000);
        m.prepare(48_000.0, 512).unwrap();
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(57, 100)];
        let ev = PluginEvents { params: &[], midi: &midi, note_expressions: &[] };
        // run a few blocks so the hammer pulse circulates
        m.process_block(&inl, &inr, &mut outl, &mut outr, &ev).unwrap();
        let ev2 = PluginEvents { params: &[], midi: &[], note_expressions: &[] };
        let mut energy = 0.0f32;
        for _ in 0..20 {
            m.process_block(&inl, &inr, &mut outl, &mut outr, &ev2).unwrap();
            energy += outl.iter().map(|s| s * s).sum::<f32>();
        }
        assert_eq!(m.active_voices(), 1);
        assert!(energy > 0.5, "waveguide voice should be audible, energy={energy}");
    }

    #[test]
    fn note_off_releases() {
        let mut m = NativeWaveguide::new(48_000);
        m.prepare(48_000.0, 512).unwrap();
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let on = [note_on(60, 100)];
        let ev = PluginEvents { params: &[], midi: &on, note_expressions: &[] };
        m.process_block(&inl, &inr, &mut outl, &mut outr, &ev).unwrap();
        use midicore::{Channel, KeyNumber, MidiEvent};
        let off = [PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOff {
                channel: Channel::new(0),
                key: KeyNumber::new(60),
                velocity: midicore::Velocity::new(0),
            },
        }];
        let ev_off = PluginEvents { params: &[], midi: &off, note_expressions: &[] };
        m.process_block(&inl, &inr, &mut outl, &mut outr, &ev_off).unwrap();
        let ev2 = PluginEvents { params: &[], midi: &[], note_expressions: &[] };
        // after ~0.5 s of release the voice should be gone
        for _ in 0..50 {
            m.process_block(&inl, &inr, &mut outl, &mut outr, &ev2).unwrap();
        }
        assert_eq!(m.active_voices(), 0, "released voice should die");
    }
}
