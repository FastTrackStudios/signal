//! `ChorusChain` — multi-engine stereo chorus / flanger / vibrato.
//!
//! Every engine is built once, up front (and resized in `update`), so
//! switching engines on the audio thread allocates nothing; the switch is an
//! equal-power crossfade between the old wet and the new. Every control is
//! smoothed per sample, so a knob turned while playing never steps the delay
//! (a stepped delay is a click on anything with top end).

use audiocore_dsp::{AudioConfig, Processor};

use crate::classic::Classic;
use crate::dsp::Smooth;
use crate::engine::{EffectType, EngineType, Frame, MAX_VOICES, StereoEngine, VoiceBank};

/// Build one engine, as the chain does.
#[must_use]
pub fn make_engine(engine: EngineType) -> Box<dyn StereoEngine> {
    match engine {
        EngineType::Cubic
        | EngineType::Bbd
        | EngineType::Tape
        | EngineType::Orbit
        | EngineType::Juno => Box::new(VoiceBank::new(engine)),
        _ => Box::new(Classic::new(engine)),
    }
}

/// Each engine's wet level made up to unity (linear gain) — measured on pink
/// noise, fully wet at depth 0.35 / 0.8 Hz (`chorus-dsp/examples/
/// chorus_measure.rs level`, then `fx-blocks/examples/chorus_gain.rs` for
/// the residual per mix). Set so the channel level and the mono sum at full
/// width straddle 0 dB: a wide engine's mono sum sits 1–1.5 dB under its
/// channels, and neither a stereo nor a mono rig should hear a jump.
///
/// A vibrato is one voice through the line (see `VoiceBank::tick`), so the
/// original engines' multi-voice makeup does not apply to it.
const fn engine_makeup(engine: EngineType, effect: EffectType) -> f64 {
    if matches!(effect, EffectType::Vibrato) {
        // Pink noise and plucked chords disagree by up to 2 dB on the
        // filtered and saturating ones; these split the difference.
        match engine {
            EngineType::Cubic | EngineType::Bbd => return 1.0,
            EngineType::Tape => return 1.15,
            EngineType::Orbit => return 1.16,
            EngineType::Juno => return 1.19,
            // The newer engines' vibrato is their chorus's first tap:
            // same makeup (their own trim is in `Classic::control`).
            _ => {}
        }
    }
    match engine {
        EngineType::Cubic => 0.809,
        EngineType::Bbd => 0.827,
        EngineType::Tape => 1.113,
        EngineType::Orbit => 0.837,
        EngineType::Juno => 1.062,
        EngineType::Ce2 => 1.274,
        EngineType::Dimension => 1.155,
        EngineType::Clone => 1.326,
        EngineType::TriChorus => 1.27,
        EngineType::Scf | EngineType::Julia => 1.288,
    }
}

/// `(dry, wet)` gains for `mix`.
///
/// A chorus's wet is decorrelated from the dry, so the two add in power and
/// the equal-power law is the level-neutral one (a linear crossfade dipped
/// ~3 dB at 50/50). A flanger's wet is the dry a millisecond or two late —
/// half correlated with it (ρ ≈ 0.35–0.55 on pink noise, measured per engine
/// with `chorus_gain flanger`): linear left the filtered engines 1.7 dB down
/// at 50/50 and equal power the wide ones 1.9 dB up, so it takes the mean of
/// the two laws, which holds every engine within ±0.5 dB. Vibrato has no dry.
fn mix_gains(effect: EffectType, mix: f64) -> (f64, f64) {
    let mix = mix.clamp(0.0, 1.0);
    let theta = mix * std::f64::consts::FRAC_PI_2;
    let (c, s) = (theta.cos(), theta.sin());
    match effect {
        EffectType::Vibrato => (0.0, 1.0),
        EffectType::Flanger => (0.5 * (1.0 - mix + c), 0.5 * (mix + s)),
        EffectType::Chorus => (c, s),
    }
}

/// An engine change in progress.
#[derive(Clone, Copy, Debug)]
enum Switch {
    None,
    /// Equal-power crossfade from the old engine (run with its own effect).
    Cross {
        from: EngineType,
        effect: EffectType,
    },
    /// Same engine, new effect: the wet dips out, the line is cleared and
    /// re-voiced, the wet comes back.
    Dip {
        effect: EffectType,
        switched: bool,
    },
}

/// Complete stereo chorus/flanger/vibrato processor.
pub struct ChorusChain {
    /// By `EngineType::index`: built when first picked ([`Self::set_engine`])
    /// and kept, so switching back and forth stays allocation-free. Every
    /// engine built up front was ~0.5 MB a chain, in every mod block of
    /// every prepared patch, for the one that plays.
    engines: Vec<Option<Box<dyn StereoEngine>>>,
    /// The rate `update` last prepared for: an engine built after it is
    /// prepared too.
    prepared_sr: Option<f64>,

    /// Number of active voices per channel (1–4; the original five engines).
    pub num_voices: usize,
    /// LFO rate in Hz.
    pub rate_hz: f64,
    /// Modulation depth (0..1).
    pub depth: f64,
    /// Feedback amount (0..1). Mainly for flanger.
    pub feedback: f64,
    /// Color/tone parameter (0..1). Engine-specific meaning.
    pub color: f64,
    /// Engine type (the target — the chain crossfades to it).
    pub engine: EngineType,
    /// Effect type (chorus/flanger/vibrato).
    pub effect_type: EffectType,
    /// Dry/wet mix (0..1). Ignored for vibrato (wet only).
    pub mix: f64,
    /// Stereo width (0..1). 0 = the engine's mono output, 1 = full spread.
    pub width: f64,

    playing: EngineType,
    playing_effect: EffectType,
    switch: Switch,
    fade: f64,
    fade_step: f64,
    s_rate: Smooth,
    s_depth: Smooth,
    s_feedback: Smooth,
    s_color: Smooth,
    s_width: Smooth,
    s_dry: Smooth,
    s_wet: Smooth,
    primed: bool,
}

impl ChorusChain {
    #[must_use]
    pub fn new() -> Self {
        let engine = EngineType::Cubic;
        let mut c = Self {
            engines: EngineType::ALL
                .iter()
                .map(|e| (*e == engine).then(|| make_engine(*e)))
                .collect(),
            prepared_sr: None,
            num_voices: 2,
            rate_hz: 1.0,
            depth: 0.5,
            feedback: 0.0,
            color: 0.5,
            engine,
            effect_type: EffectType::Chorus,
            mix: 0.5,
            width: 1.0,
            playing: engine,
            playing_effect: EffectType::Chorus,
            switch: Switch::None,
            fade: 0.0,
            fade_step: 0.0,
            s_rate: Smooth::new(1.0),
            s_depth: Smooth::new(0.5),
            s_feedback: Smooth::new(0.0),
            s_color: Smooth::new(0.5),
            s_width: Smooth::new(1.0),
            s_dry: Smooth::new(0.0),
            s_wet: Smooth::new(0.0),
            primed: false,
        };
        c.set_times(48_000.0);
        c
    }

    /// Switch the chorus engine; the chain crossfades to it over ~25 ms.
    /// Builds the engine the first time it is picked — an allocation, as a
    /// param write's (never the audio callback's); after that, free.
    pub fn set_engine(&mut self, engine: EngineType) {
        self.engine = engine;
        if let Some(slot) = self.engines.get_mut(engine.index())
            && slot.is_none()
        {
            let mut e = make_engine(engine);
            if let Some(sr) = self.prepared_sr {
                e.update(sr);
            }
            e.reset();
            *slot = Some(e);
        }
    }

    /// Whether `engine` has been built (it plays only once it has).
    fn has(&self, engine: EngineType) -> bool {
        self.engines.get(engine.index()).is_some_and(Option::is_some)
    }

    /// One frame from `engine`, or silence if it was never built.
    fn tick_engine(&mut self, engine: EngineType, l: f64, r: f64, f: &Frame) -> (f64, f64) {
        match self.engines.get_mut(engine.index()).and_then(Option::as_deref_mut) {
            Some(e) => e.tick(l, r, f),
            None => (0.0, 0.0),
        }
    }

    /// The engine actually playing (differs from [`Self::engine`] only
    /// while a switch is fading).
    #[must_use]
    pub const fn playing_engine(&self) -> EngineType {
        self.playing
    }

    /// The delay the playing engine's first voice is reading at, in ms.
    #[must_use]
    pub fn delay_ms(&self) -> f64 {
        self.engines
            .get(self.playing.index())
            .and_then(Option::as_deref)
            .map_or(0.0, |e| e.delay_ms())
    }

    fn set_times(&mut self, sr: f64) {
        self.s_rate.set_time(60.0, sr);
        self.s_depth.set_time(40.0, sr);
        self.s_feedback.set_time(40.0, sr);
        self.s_color.set_time(40.0, sr);
        self.s_width.set_time(40.0, sr);
        self.s_dry.set_time(15.0, sr);
        self.s_wet.set_time(15.0, sr);
        self.fade_step = 1.0 / (0.025 * sr).max(1.0);
    }

    fn frame(&self, effect: EffectType) -> Frame {
        Frame {
            rate_hz: self.s_rate.value,
            depth: self.s_depth.value,
            feedback: self.s_feedback.value,
            color: self.s_color.value,
            width: self.s_width.value,
            effect,
            voices: self.num_voices.clamp(1, MAX_VOICES),
        }
    }
}

impl Default for ChorusChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for ChorusChain {
    fn reset(&mut self) {
        for e in self.engines.iter_mut().flatten() {
            e.reset();
        }
        self.switch = Switch::None;
        self.primed = false;
    }

    fn update(&mut self, config: AudioConfig) {
        self.prepared_sr = Some(config.sample_rate);
        for e in self.engines.iter_mut().flatten() {
            e.update(config.sample_rate);
        }
        self.set_times(config.sample_rate);
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let rate = self.rate_hz.clamp(0.0, 40.0);
        let depth = self.depth.clamp(0.0, 1.0);
        let feedback = self.feedback.clamp(0.0, 1.0);
        let color = self.color.clamp(0.0, 1.0);
        let width = self.width.clamp(0.0, 1.0);

        if !self.primed {
            // A fresh chain starts where its controls are, not ramping to
            // them from the constructor's defaults.
            self.playing = self.engine;
            self.playing_effect = self.effect_type;
            self.s_rate.value = rate;
            self.s_depth.value = depth;
            self.s_feedback.value = feedback;
            self.s_color.value = color;
            self.s_width.value = width;
            let (d, w) = mix_gains(self.playing_effect, self.mix);
            self.s_dry.value = d;
            self.s_wet.value = w;
            self.primed = true;
        }

        // Start a switch (one at a time; a second waits for the first).
        // An engine set without `set_engine` (never built) is not switched
        // to: building it is not the audio callback's to do.
        if matches!(self.switch, Switch::None)
            && (self.engine != self.playing || self.effect_type != self.playing_effect)
            && self.has(self.engine)
        {
            if self.engine == self.playing {
                self.switch = Switch::Dip {
                    effect: self.playing_effect,
                    switched: false,
                };
            } else {
                self.switch = Switch::Cross {
                    from: self.playing,
                    effect: self.playing_effect,
                };
                if let Some(e) = self.engines.get_mut(self.engine.index()).and_then(Option::as_deref_mut) {
                    e.reset();
                }
                self.playing = self.engine;
                self.playing_effect = self.effect_type;
            }
            self.fade = 0.0;
        }

        let (dry_t, wet_t) = mix_gains(self.effect_type, self.mix);
        let makeup = engine_makeup(self.playing, self.effect_type);

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.s_rate.tick(rate);
            self.s_depth.tick(depth);
            self.s_feedback.tick(feedback);
            self.s_color.tick(color);
            self.s_width.tick(width);
            let dry_g = self.s_dry.tick(dry_t);
            let wet_g = self.s_wet.tick(wet_t);
            let (dl, dr) = (*l, *r);

            let (mut wl, mut wr);
            match self.switch {
                Switch::None => {
                    let f = self.frame(self.playing_effect);
                    (wl, wr) = self.tick_engine(self.playing, dl, dr, &f);
                    wl *= makeup;
                    wr *= makeup;
                }
                Switch::Cross { from, effect } => {
                    let f_new = self.frame(self.playing_effect);
                    let f_old = self.frame(effect);
                    let (nl, nr) = self.tick_engine(self.playing, dl, dr, &f_new);
                    let (ol, or) = self.tick_engine(from, dl, dr, &f_old);
                    let theta = self.fade * std::f64::consts::FRAC_PI_2;
                    let (gn, go) = (
                        theta.sin() * makeup,
                        theta.cos() * engine_makeup(from, effect),
                    );
                    wl = nl.mul_add(gn, ol * go);
                    wr = nr.mul_add(gn, or * go);
                    self.fade += self.fade_step;
                    if self.fade >= 1.0 {
                        self.switch = Switch::None;
                    }
                }
                Switch::Dip { effect, switched } => {
                    let f = self.frame(if switched {
                        self.playing_effect
                    } else {
                        effect
                    });
                    let (el, er) = self.tick_engine(self.playing, dl, dr, &f);
                    // Out over the first half, back over the second.
                    let g = (self.fade * std::f64::consts::PI).cos().abs() * makeup;
                    wl = el * g;
                    wr = er * g;
                    self.fade += self.fade_step;
                    if !switched && self.fade >= 0.5 {
                        if let Some(e) = self.engines.get_mut(self.playing.index()).and_then(Option::as_deref_mut) {
                            e.reset();
                        }
                        self.playing_effect = self.effect_type;
                        self.switch = Switch::Dip {
                            effect,
                            switched: true,
                        };
                    } else if self.fade >= 1.0 {
                        self.switch = Switch::None;
                    }
                }
            }

            *l = dl.mul_add(dry_g, wl * wet_g);
            *r = dr.mul_add(dry_g, wr * wet_g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    fn pink(seconds: f64, seed: u32) -> Vec<f64> {
        let mut seed = seed;
        let (mut b0, mut b1) = (0.0f64, 0.0f64);
        (0..(SR * seconds) as usize)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let w = f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5;
                b0 = 0.997f64.mul_add(b0, 0.029 * w);
                b1 = 0.95f64.mul_add(b1, 0.048 * w);
                0.02f64.mul_add(w, b0 + b1) * 2.0
            })
            .collect()
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|s| s * s).sum::<f64>() / x.len() as f64).sqrt()
    }

    fn chain(engine: EngineType) -> ChorusChain {
        let mut c = ChorusChain::new();
        c.set_engine(engine);
        c.update(config());
        c
    }

    fn run(c: &mut ChorusChain, input: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let (mut l, mut r) = (input.to_vec(), input.to_vec());
        for (bl, br) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
            c.process(bl, br);
        }
        (l, r)
    }

    /// Peak pitch deviation (cents) of the playing engine's first voice.
    fn peak_cents(chain: &mut ChorusChain, seconds: f64) -> f64 {
        let len = (SR * seconds) as usize;
        let mut trace = Vec::with_capacity(len);
        let (mut left, mut right) = ([0.0; 1], [0.0; 1]);
        for i in 0..len {
            left[0] = (i as f64 * 0.05).sin() * 0.1;
            right[0] = left[0];
            chain.process(&mut left, &mut right);
            trace.push(chain.delay_ms());
        }
        let trace = &trace[len / 4..];
        let step = 48;
        (step..trace.len())
            .step_by(step)
            .map(|i| {
                let slope = (trace[i] - trace[i - step]) * 0.001 * SR / step as f64;
                (1200.0 * (1.0 - slope).max(1e-9).log2()).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Engaged at any mix, every engine plays at the level it is fed —
    /// switching a chorus on must not drop (or jump) the guitar.
    #[test]
    fn every_engine_sits_at_unity_at_any_mix() {
        let input = pink(3.0, 11);
        for engine in EngineType::ALL {
            for mix in [0.25, 0.5, 0.75, 1.0] {
                let mut c = chain(engine);
                c.mix = mix;
                c.depth = 0.35;
                c.rate_hz = 0.8;
                let (l, r) = run(&mut c, &input);
                let half = input.len() / 2;
                let out: Vec<f64> = l[half..]
                    .iter()
                    .zip(&r[half..])
                    .map(|(a, b)| (a + b) * 0.5)
                    .collect();
                let db = 20.0 * (rms(&out) / rms(&input[half..])).log10();
                assert!(db.abs() < 1.5, "{engine:?} at mix {mix}: {db:+.1} dB");
            }
        }
    }

    /// Width moves the image, not the level: each channel within ±1.5 dB of
    /// the input from mono to full spread.
    #[test]
    fn width_does_not_move_the_level() {
        let input = pink(2.0, 5);
        for engine in EngineType::ALL {
            for width in [0.0, 0.5, 1.0] {
                let mut c = chain(engine);
                c.mix = 1.0;
                c.depth = 0.4;
                c.width = width;
                let (l, r) = run(&mut c, &input);
                let half = input.len() / 2;
                for (side, x) in [("L", &l), ("R", &r)] {
                    let db = 20.0 * (rms(&x[half..]) / rms(&input[half..])).log10();
                    assert!(
                        db.abs() < 1.5,
                        "{engine:?} width {width} {side}: {db:+.1} dB"
                    );
                }
            }
        }
    }

    /// A guitar panned hard left keeps its chorus on the left: every engine,
    /// chorus and vibrato, at full width and none. (One line fed the sum of
    /// both sides put the chorus of a panned guitar in both.)
    #[test]
    fn a_panned_input_stays_panned() {
        let input = pink(1.0, 7);
        for engine in EngineType::ALL {
            for effect in [EffectType::Chorus, EffectType::Vibrato] {
                for width in [0.0, 1.0] {
                    let mut c = chain(engine);
                    c.effect_type = effect;
                    c.mix = 1.0;
                    c.width = width;
                    let (mut l, mut r) = (input.clone(), vec![0.0; input.len()]);
                    for (bl, br) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
                        c.process(bl, br);
                    }
                    let (lo, ro) = (rms(&l[4800..]), rms(&r[4800..]));
                    assert!(lo > 0.01, "{engine:?} {effect:?} w{width}: the left plays ({lo})");
                    assert!(
                        ro < lo * 0.01,
                        "{engine:?} {effect:?} w{width}: the right should stay silent — {ro:.5} vs {lo:.5}"
                    );
                }
            }
        }
    }

    /// A centred input plays both sides alike as before: the per-side lines
    /// of a classic engine hold the same signal, and the width's centre is
    /// the plain mid.
    #[test]
    fn a_centred_input_fills_both_sides() {
        let input = pink(1.0, 9);
        for engine in EngineType::ALL {
            let mut c = chain(engine);
            c.mix = 1.0;
            let (l, r) = run(&mut c, &input);
            let (lo, ro) = (rms(&l[4800..]), rms(&r[4800..]));
            assert!(lo > 0.01 && ro > 0.01, "{engine:?}: {lo} {ro}");
            assert!((20.0 * (lo / ro).log10()).abs() < 3.0, "{engine:?}: sides {lo} vs {ro}");
        }
    }

    #[test]
    fn silence_in_silence_out() {
        for engine in EngineType::ALL {
            let mut c = chain(engine);
            let (l, r) = run(&mut c, &vec![0.0; 4800]);
            assert!(l.iter().chain(&r).all(|s| s.abs() < 1e-10), "{engine:?}");
        }
    }

    #[test]
    fn zero_mix_passes_through() {
        let mut c = ChorusChain::new();
        c.mix = 0.0;
        c.update(config());
        let input: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5)
            .collect();
        let (l, _) = run(&mut c, &input);
        for (i, (&out, &inp)) in l.iter().zip(input.iter()).enumerate() {
            assert!((out - inp).abs() < 1e-10, "at {i}: {out} vs {inp}");
        }
    }

    /// Every engine, every effect, every knob at both ends: finite and
    /// bounded (a 0.5 sine in never comes out past 2).
    #[test]
    fn every_engine_is_bounded_at_the_extremes() {
        let input: Vec<f64> = (0..24_000)
            .map(|i| (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5)
            .collect();
        for engine in EngineType::ALL {
            for effect in [EffectType::Chorus, EffectType::Flanger, EffectType::Vibrato] {
                for (rate, depth, fb, color, width) in [
                    (10.0, 1.0, 1.0, 1.0, 1.0),
                    (0.05, 1.0, 1.0, 0.0, 0.0),
                    (20.0, 1.0, 0.0, 0.5, 0.5),
                    (1.0, 0.0, 1.0, 0.0, 1.0),
                ] {
                    let mut c = chain(engine);
                    c.effect_type = effect;
                    c.num_voices = 4;
                    c.rate_hz = rate;
                    c.depth = depth;
                    c.feedback = fb;
                    c.color = color;
                    c.width = width;
                    c.mix = 0.5;
                    let (l, r) = run(&mut c, &input);
                    let peak = l.iter().chain(&r).fold(0.0f64, |m, s| {
                        assert!(s.is_finite(), "{engine:?}/{effect:?} NaN");
                        m.max(s.abs())
                    });
                    assert!(
                        peak < 2.0,
                        "{engine:?}/{effect:?} rate {rate} depth {depth} fb {fb}: peak {peak}"
                    );
                }
            }
        }
    }

    /// The depth knob reaches a real chorus by noon and never passes the
    /// mode's pitch ceiling, whatever the rate.
    #[test]
    fn modulation_depth_is_musical_across_the_knob() {
        for engine in EngineType::ALL {
            // Noon at 1 Hz: audible, not seasick.
            let mut c = chain(engine);
            c.depth = 0.5;
            c.rate_hz = 1.0;
            let mid = peak_cents(&mut c, 3.0);
            assert!(
                (1.5..=40.0).contains(&mid),
                "{engine:?} noon @1 Hz: ±{mid:.1} c"
            );
            // Full depth at 10 Hz: capped.
            for effect in [EffectType::Chorus, EffectType::Vibrato] {
                let mut c = chain(engine);
                c.effect_type = effect;
                c.depth = 1.0;
                c.rate_hz = 10.0;
                let top = peak_cents(&mut c, 1.5);
                // (Tape's wow and flutter ride on top of the tamed LFO.)
                let cap = effect.ceiling_cents() * 1.1;
                assert!(
                    top <= cap,
                    "{engine:?}/{effect:?} full @10 Hz: ±{top:.0} c > {cap:.0}"
                );
            }
        }
    }

    /// High-frequency energy (the second difference) right after a big knob
    /// move stays within a small multiple of the steady state — no zipper,
    /// no click.
    #[test]
    fn knob_moves_do_not_click() {
        for engine in EngineType::ALL {
            for knob in [
                "depth", "rate", "color", "width", "feedback", "voices", "engine",
            ] {
                let mut c = chain(engine);
                c.mix = 0.5;
                c.depth = 0.2;
                c.rate_hz = 0.5;
                c.color = 0.2;
                c.width = 0.0;
                let n = (SR * 1.5) as usize;
                let mut out = Vec::with_capacity(n);
                let mut i0 = 0;
                while i0 < n {
                    if i0 == n / 2 {
                        match knob {
                            "depth" => c.depth = 0.9,
                            "rate" => c.rate_hz = 6.0,
                            "color" => c.color = 1.0,
                            "width" => c.width = 1.0,
                            "feedback" => c.feedback = 0.8,
                            "voices" => c.num_voices = 4,
                            _ => c.set_engine(if engine == EngineType::Cubic {
                                EngineType::Ce2
                            } else {
                                EngineType::Cubic
                            }),
                        }
                    }
                    let mut l: Vec<f64> = (i0..i0 + 64)
                        .map(|i| (2.0 * PI * 330.0 * i as f64 / SR).sin() * 0.5)
                        .collect();
                    let mut r = l.clone();
                    c.process(&mut l, &mut r);
                    out.extend(l);
                    i0 += 64;
                }
                let hf: Vec<f64> = (2..out.len())
                    .map(|i| 2.0f64.mul_add(-out[i - 1], out[i] + out[i - 2]).abs())
                    .collect();
                let steady = hf[n / 4..n / 2 - 10].iter().copied().fold(0.0, f64::max);
                let after = hf[n / 2 - 2..n / 2 + 4_800]
                    .iter()
                    .copied()
                    .fold(0.0, f64::max);
                assert!(
                    after < steady * 2.5,
                    "{engine:?} {knob}: HF {after:.4} after vs {steady:.4} steady"
                );
            }
        }
    }

    #[test]
    fn different_engines_produce_different_output() {
        let input: Vec<f64> = (0..9600)
            .map(|i| (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5)
            .collect();
        let outputs: Vec<Vec<f64>> = EngineType::ALL
            .iter()
            .map(|e| {
                let mut c = chain(*e);
                c.depth = 0.5;
                c.mix = 1.0;
                c.num_voices = 1;
                run(&mut c, &input).0
            })
            .collect();
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                let diff: f64 = outputs[i]
                    .iter()
                    .zip(&outputs[j])
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / 9600.0;
                assert!(
                    diff > 0.001,
                    "{:?} and {:?} should differ: {diff}",
                    EngineType::ALL[i],
                    EngineType::ALL[j]
                );
            }
        }
    }

    /// The persisted index round-trips, and the order is the old one
    /// followed by the new engines.
    #[test]
    fn engine_indices_are_stable() {
        for (i, e) in EngineType::ALL.iter().enumerate() {
            assert_eq!(e.index(), i);
            assert_eq!(EngineType::from_index(i), *e);
        }
        assert_eq!(EngineType::from_index(4), EngineType::Juno);
        assert_eq!(EngineType::from_index(99), EngineType::Cubic);
    }
}
