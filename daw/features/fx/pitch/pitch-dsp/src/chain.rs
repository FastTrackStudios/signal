//! PitchChain — top-level processor with algorithm selection.
//!
//! Wraps all four pitch shifting algorithms behind the `Processor` trait
//! with runtime algorithm switching. Pitch is controlled via a semitone
//! slider (−24 to +24).

use audiocore_dsp::{AudioConfig, Processor};
use serde::{Deserialize, Serialize};

use crate::allpass_shift::AllpassShifter;
use crate::divider::{DivideRatio, FreqDivider};
use crate::granular::GranularShifter;
use crate::pll::{PllOctave, PllTracker, SubWaveform};
use crate::pog::{OctaveShift, PolyOctave};
use crate::psola::PsolaShifter;
#[cfg(feature = "rubberband")]
use crate::rubberband::RubberbandShifter;
use crate::spectral::SpectralShifter;
use crate::wsola::WsolaShifter;

/// Which pitch shifting algorithm to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Algorithm {
    /// Analog-style frequency divider. 0 latency, synthy character.
    /// Only supports exact octave shifts (−12 or −24 semitones).
    /// Other values are clamped to the nearest octave.
    FreqDivider,
    /// PLL tracking oscillator. 0 latency, warmer sub.
    /// Only supports exact octave shifts (−12 or −24 semitones).
    /// Other values are clamped to the nearest octave.
    Pll,
    /// Granular fixed-ratio. ~1024 sample latency, natural tone.
    /// Supports arbitrary semitone values.
    Granular,
    /// PSOLA pitch-synchronous. ~2048 sample latency, highest quality.
    /// Supports arbitrary semitone values.
    Psola,
    /// WSOLA waveform-similarity overlap-add. ~1024 sample latency.
    /// Works on any signal (monophonic or polyphonic) without pitch detection.
    Wsola,
    /// Rubber Band Library — professional pitch shifter with formant preservation.
    /// High quality, ~512+ sample latency.
    Rubberband,
    /// Allpass interpolation — Dattorro/Schroeder barberpole style.
    /// ~1024 sample latency (standard), ~256 (live). Classic hardware character.
    Allpass,
    /// Polyphonic Octave Generator — ERB filter bank phase scaling (EHX POG style).
    /// 0 sample latency, polyphonic, octave shifts only (±12, ±24 semitones).
    PolyOctave,
    /// Phase-locked phase vocoder (Laroche–Dolson peak shifting).
    /// Polyphonic, no grain warble, arbitrary ratio. Latency 2048 samples
    /// (42.7 ms @ 48 kHz); `live` = 1024 (21.3 ms).
    Spectral,
}

/// Convert semitones to pitch ratio: `2^(semitones / 12)`.
pub fn semitones_to_ratio(semitones: f64) -> f64 {
    (2.0f64).powf(semitones / 12.0)
}

/// Convert pitch ratio to semitones: `12 * log2(ratio)`.
pub fn ratio_to_semitones(ratio: f64) -> f64 {
    12.0 * ratio.log2()
}

/// Top-level pitch shifter with algorithm selection.
pub struct PitchChain {
    /// Active algorithm.
    pub algorithm: Algorithm,
    /// Pitch shift in semitones. Range: −24.0 to +24.0.
    /// 0 = no shift, −12 = octave down, +12 = octave up.
    pub semitones: f64,
    /// Dry/wet mix (0.0–1.0).
    pub mix: f64,
    /// Live mode: minimize latency at the cost of quality.
    pub live: bool,
    /// Formant shift in semitones. Range: −24.0 to +24.0.
    /// 0 = no shift. Only affects algorithms that support formant control
    /// (Rubberband).
    pub formant_semitones: f64,
    /// When true, formant shift is linked to pitch (formants stay in place).
    /// When false, formant_semitones is applied independently.
    pub formant_linked: bool,

    // -- PLL-specific settings --
    /// PLL sub-oscillator waveform.
    pub pll_waveform: SubWaveform,

    // -- Granular-specific settings --
    /// Grain size in samples.
    pub grain_size: usize,

    divider: FreqDivider,
    pll: PllTracker,
    // The engines with buffers (seconds of audio, FFT frames), built when
    // their algorithm is first selected — [`PitchChain::only`] holds one.
    // A guitar rig carries a pitch block, four chains of it, in every
    // patch it has prepared: all nine engines each was ~2.6 MB a chain.
    granular: Option<Box<GranularShifter>>,
    psola: Option<Box<PsolaShifter>>,
    wsola: Option<Box<WsolaShifter>>,
    #[cfg(feature = "rubberband")]
    rubberband: Option<Box<RubberbandShifter>>,
    allpass: Option<Box<AllpassShifter>>,
    pog: PolyOctave,
    spectral: Option<Box<SpectralShifter>>,

    /// Track previous live state to detect changes and reinitialise.
    prev_live: bool,
    sample_rate: f64,
    /// `update` has run: an engine built after it is updated as it is built.
    prepared: bool,
}

impl PitchChain {
    pub fn new() -> Self {
        Self {
            algorithm: Algorithm::FreqDivider,
            semitones: -12.0,
            mix: 1.0,
            live: false,
            formant_semitones: 0.0,
            formant_linked: true,
            pll_waveform: SubWaveform::Saw,
            grain_size: 1024,
            divider: FreqDivider::new(),
            pll: PllTracker::new(),
            granular: Some(Box::new(GranularShifter::new())),
            psola: Some(Box::new(PsolaShifter::new())),
            wsola: Some(Box::new(WsolaShifter::new())),
            #[cfg(feature = "rubberband")]
            rubberband: Some(Box::new(RubberbandShifter::new())),
            allpass: Some(Box::new(AllpassShifter::new())),
            pog: PolyOctave::new(),
            spectral: Some(Box::new(SpectralShifter::new())),
            prev_live: false,
            sample_rate: 48000.0,
            prepared: false,
        }
    }

    /// A chain holding only `algorithm`'s engine: another is built when it
    /// is [`select`](Self::select)ed. The memory of one engine, not nine.
    pub fn only(algorithm: Algorithm) -> Self {
        let mut c = Self::new();
        c.granular = None;
        c.psola = None;
        c.wsola = None;
        #[cfg(feature = "rubberband")]
        {
            c.rubberband = None;
        }
        c.allpass = None;
        c.spectral = None;
        c.select(algorithm);
        c
    }

    /// Play `algorithm`, building its engine if this chain has none —
    /// an allocation: call it where a reverb's new algorithm is built (a
    /// param write on the control thread, or before preparing), never from
    /// `process`.
    pub fn select(&mut self, algorithm: Algorithm) {
        self.algorithm = algorithm;
        let live = self.live;
        let (sr, prepared) = (self.sample_rate, self.prepared);
        match algorithm {
            Algorithm::Granular if self.granular.is_none() => {
                let mut e = Box::new(GranularShifter::new());
                if prepared {
                    e.update(sr);
                }
                self.granular = Some(e);
            }
            Algorithm::Psola if self.psola.is_none() => {
                let mut e = Box::new(PsolaShifter::new());
                e.base_window_size = if live { 512 } else { 2048 };
                if prepared {
                    e.update(sr);
                }
                self.psola = Some(e);
            }
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband | Algorithm::Wsola if self.wsola.is_none() => self.build_wsola(),
            #[cfg(feature = "rubberband")]
            Algorithm::Wsola if self.wsola.is_none() => self.build_wsola(),
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband if self.rubberband.is_none() => {
                let mut e = Box::new(RubberbandShifter::new());
                e.live = live;
                if prepared {
                    e.update(sr);
                }
                self.rubberband = Some(e);
            }
            Algorithm::Allpass if self.allpass.is_none() => {
                let mut e = Box::new(AllpassShifter::new());
                if prepared {
                    e.update(sr);
                }
                self.allpass = Some(e);
            }
            Algorithm::Spectral if self.spectral.is_none() => {
                let mut e = Box::new(Self::spectral_for(sr));
                e.fft_size = if live { 1024 } else { 2048 };
                if prepared {
                    e.update(sr);
                }
                self.spectral = Some(e);
            }
            _ => {}
        }
    }

    /// A spectral engine holding the frames this chain plays at
    /// `sample_rate` (its non-live frame, the larger), not every frame.
    fn spectral_for(sample_rate: f64) -> SpectralShifter {
        SpectralShifter::with_max_frame(SpectralShifter::frame_for(2048, sample_rate))
    }

    fn build_wsola(&mut self) {
        let mut e = Box::new(WsolaShifter::new());
        e.base_grain_size = if self.live { 256 } else { 1024 };
        if self.prepared {
            e.update(self.sample_rate);
        }
        self.wsola = Some(e);
    }

    /// Latency introduced by the currently selected algorithm.
    pub fn latency(&self) -> usize {
        match self.algorithm {
            Algorithm::FreqDivider => self.divider.latency(),
            Algorithm::Pll => self.pll.latency(),
            Algorithm::Granular => self.granular.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Psola => self.psola.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Wsola => self.wsola.as_ref().map_or(0, |e| e.latency()),
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband => self.rubberband.as_ref().map_or(0, |e| e.latency()),
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband => self.wsola.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Allpass => self.allpass.as_ref().map_or(0, |e| e.latency()),
            Algorithm::PolyOctave => self.pog.latency(),
            Algorithm::Spectral => self.spectral.as_ref().map_or(0, |e| e.latency()),
        }
    }

    /// Map semitones to the nearest octave division for divider/PLL.
    /// Returns (DivideRatio/PllOctave, direction).
    /// Divider/PLL only support down-shifting by exact octaves.
    fn nearest_octave(&self) -> (bool, bool) {
        // is_oct2: true if closer to −24 than −12.
        // is_down: true if semitones < −6 (clearly wants a sub).
        let is_down = self.semitones < -6.0;
        let is_oct2 = self.semitones < -18.0;
        (is_down, is_oct2)
    }

    /// Sync algorithm-specific parameters before processing.
    fn sync_params(&mut self) {
        // Detect live mode changes and reconfigure engines.
        if self.live != self.prev_live {
            self.prev_live = self.live;

            let (live, sr) = (self.live, self.sample_rate);
            // PSOLA: smaller analysis window in live mode.
            if let Some(e) = self.psola.as_deref_mut() {
                e.base_window_size = if live { 512 } else { 2048 };
                e.update(sr);
            }
            // WSOLA: smaller grains in live mode.
            if let Some(e) = self.wsola.as_deref_mut() {
                e.base_grain_size = if live { 256 } else { 1024 };
                e.update(sr);
            }
            // Rubberband: R2 engine + smaller blocks.
            #[cfg(feature = "rubberband")]
            if let Some(e) = self.rubberband.as_deref_mut() {
                e.live = live;
                e.update(sr);
            }
            // Allpass: no live mode toggle (not supported).
            if let Some(e) = self.allpass.as_deref_mut() {
                e.update(sr);
            }
            // Spectral: 1024-sample frames live, 2048 otherwise.
            if let Some(e) = self.spectral.as_deref_mut() {
                e.fft_size = if live { 1024 } else { 2048 };
                e.update(sr);
            }
        }

        let ratio = semitones_to_ratio(self.semitones.clamp(-24.0, 24.0));
        let (is_down, is_oct2) = self.nearest_octave();

        // Divider: only supports octave down. Bypass if shifting up.
        self.divider.ratio = if is_oct2 {
            DivideRatio::Oct2
        } else {
            DivideRatio::Oct1
        };
        self.divider.mix = if is_down { self.mix } else { 0.0 };

        // PLL: only supports octave down. Bypass if shifting up.
        self.pll.waveform = self.pll_waveform;
        self.pll.octave = if is_oct2 {
            PllOctave::Oct2
        } else {
            PllOctave::Oct1
        };
        self.pll.mix = if is_down { self.mix } else { 0.0 };

        // Granular: arbitrary ratio. In live mode, cap grain size at 256.
        let (mix, live, grain) = (self.mix, self.live, self.grain_size);
        if let Some(e) = self.granular.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
            e.grain_size = if live { grain.min(256) } else { grain };
        }

        // PSOLA: arbitrary ratio.
        if let Some(e) = self.psola.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // WSOLA: arbitrary ratio.
        if let Some(e) = self.wsola.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // Compute effective formant shift.
        // When linked: formant_semitones cancels out the pitch shift so formants stay in place.
        // When unlinked: formant_semitones is applied as-is.
        let effective_formant_st = if self.formant_linked {
            -self.semitones.clamp(-24.0, 24.0)
        } else {
            self.formant_semitones.clamp(-24.0, 24.0)
        };
        let _formant_ratio = semitones_to_ratio(effective_formant_st);

        // Rubberband: arbitrary ratio + formant control.
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
            e.preserve_formants = true;
        }
        // formant_scale not yet exposed on RubberbandShifter.

        // Allpass: arbitrary ratio.
        if let Some(e) = self.allpass.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // Spectral: arbitrary ratio.
        if let Some(e) = self.spectral.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // PolyOctave: snap semitones to nearest octave.
        self.pog.shift = OctaveShift::from_semitones(self.semitones);
        self.pog.mix = self.mix;
    }
}

impl Default for PitchChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for PitchChain {
    fn reset(&mut self) {
        self.divider.reset();
        self.pll.reset();
        if let Some(e) = self.granular.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.psola.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.wsola.as_deref_mut() {
            e.reset();
        }
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.allpass.as_deref_mut() {
            e.reset();
        }
        self.pog.reset();
        if let Some(e) = self.spectral.as_deref_mut() {
            e.reset();
        }
    }

    fn update(&mut self, config: AudioConfig) {
        let sr = config.sample_rate;
        self.sample_rate = sr;
        self.prepared = true;
        self.divider.update(sr);
        self.pll.update(sr);
        if let Some(e) = self.granular.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.psola.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.wsola.as_deref_mut() {
            e.update(sr);
        }
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.allpass.as_deref_mut() {
            e.update(sr);
        }
        self.pog.update(sr);
        // A rate whose frames outgrow the engine's (44.1 → 96 kHz): a
        // larger one, here in `update` — preparing, not processing.
        if let Some(e) = self.spectral.as_deref_mut() {
            if e.max_frame() < SpectralShifter::frame_for(2048, sr) {
                let mut bigger = Self::spectral_for(sr);
                bigger.speed = e.speed;
                bigger.mix = e.mix;
                *e = bigger;
            }
        }
        if let Some(e) = self.spectral.as_deref_mut() {
            e.fft_size = if self.live { 1024 } else { 2048 };
            e.update(sr);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        self.sync_params();

        match self.algorithm {
            Algorithm::FreqDivider => {
                for s in left.iter_mut() {
                    *s = self.divider.tick(*s);
                }
            }
            Algorithm::Pll => {
                for s in left.iter_mut() {
                    *s = self.pll.tick(*s);
                }
            }
            Algorithm::Granular => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.granular.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Psola => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.psola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Wsola => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.wsola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            // Without the `rubberband` system library, Rubberband falls
            // back to WSOLA (closest arbitrary-ratio pure-Rust quality).
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband => {
                if let Some(e) = self.rubberband.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.wsola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Allpass => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.allpass.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::PolyOctave => {
                for s in left.iter_mut() {
                    *s = self.pog.tick(*s);
                }
            }
            Algorithm::Spectral => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.spectral.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
        }

        // Copy mono result to right channel.
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    fn all_algorithms() -> Vec<Algorithm> {
        let mut v = vec![
            Algorithm::FreqDivider,
            Algorithm::Pll,
            Algorithm::Granular,
            Algorithm::Psola,
            Algorithm::Wsola,
            Algorithm::Allpass,
            Algorithm::PolyOctave,
            Algorithm::Spectral,
        ];
        if cfg!(feature = "rubberband") {
            v.push(Algorithm::Rubberband);
        }
        v
    }

    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    /// A chain holding only one engine — built with it, or switched to it
    /// later — plays exactly as a chain holding all of them.
    #[test]
    fn a_chain_with_one_engine_plays_as_one_with_all() {
        for algo in all_algorithms() {
            let render = |c: &mut PitchChain| {
                c.semitones = 7.0;
                let mut out = Vec::new();
                for k in 0..40 {
                    let mut l = sine_block(220.0, k * 256, 256);
                    let mut r = l.clone();
                    c.process(&mut l, &mut r);
                    out.extend(l);
                }
                out
            };
            let mut all = PitchChain::new();
            all.algorithm = algo;
            all.update(config());
            all.reset();
            let mut one = PitchChain::only(algo);
            one.update(config());
            one.reset();
            // Switched to after it was prepared, as a param write does.
            let mut later = PitchChain::only(Algorithm::PolyOctave);
            later.update(config());
            later.select(algo);
            later.reset();
            let (a, b, c) = (render(&mut all), render(&mut one), render(&mut later));
            assert_eq!(a, b, "{algo:?}: built with only it");
            assert_eq!(a, c, "{algo:?}: selected after preparing");
        }
    }

    /// Prepared at a rate whose frames outgrow the engine it was built with
    /// (96 kHz: 4096-sample frames), the spectral engine grows — and plays
    /// as one that held every frame from the start.
    #[test]
    fn a_spectral_engine_grows_with_the_rate_it_is_prepared_at() {
        let hi = AudioConfig {
            sample_rate: 96_000.0,
            max_buffer_size: 512,
        };
        let render = |c: &mut PitchChain| {
            c.semitones = -5.0;
            let mut out = Vec::new();
            for k in 0..60 {
                let mut l: Vec<f64> = (k * 256..k * 256 + 256)
                    .map(|i| (2.0 * PI * 220.0 * i as f64 / 96_000.0).sin() * 0.5)
                    .collect();
                let mut r = l.clone();
                c.process(&mut l, &mut r);
                out.extend(l);
            }
            out
        };
        let mut all = PitchChain::new();
        all.algorithm = Algorithm::Spectral;
        all.update(hi);
        all.reset();
        let mut one = PitchChain::only(Algorithm::Spectral);
        one.update(hi);
        one.reset();
        assert_eq!(render(&mut all), render(&mut one));
    }

    fn sine_block(freq: f64, offset: usize, len: usize) -> Vec<f64> {
        (offset..offset + len)
            .map(|i| (2.0 * PI * freq * i as f64 / SR).sin() * 0.5)
            .collect()
    }

    #[test]
    fn semitone_to_ratio_conversions() {
        assert!((semitones_to_ratio(0.0) - 1.0).abs() < 1e-10);
        assert!((semitones_to_ratio(-12.0) - 0.5).abs() < 1e-10);
        assert!((semitones_to_ratio(12.0) - 2.0).abs() < 1e-10);
        assert!((semitones_to_ratio(-24.0) - 0.25).abs() < 1e-10);
        assert!((semitones_to_ratio(24.0) - 4.0).abs() < 1e-10);
        // -7 semitones (perfect fifth down)
        assert!((semitones_to_ratio(-7.0) - 0.6674199270850172).abs() < 1e-10);
    }

    #[test]
    fn ratio_to_semitone_roundtrip() {
        for st in [
            -24.0, -12.0, -7.0, -5.0, -1.0, 0.0, 1.0, 5.0, 7.0, 12.0, 24.0,
        ] {
            let rt = ratio_to_semitones(semitones_to_ratio(st));
            assert!(
                (rt - st).abs() < 1e-10,
                "Roundtrip failed for {st}: got {rt}"
            );
        }
    }

    #[test]
    fn all_algorithms_produce_output() {
        for algo in all_algorithms() {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.mix = 1.0;
            chain.update(config());
            chain.reset();

            let mut energy = 0.0;
            let blocks = 100;
            let block_size = 512;

            for b in 0..blocks {
                let mut left = sine_block(220.0, b * block_size, block_size);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);

                if b > 10 {
                    energy += left.iter().map(|s| s * s).sum::<f64>();
                }
            }

            assert!(
                energy > 0.1,
                "{algo:?} should produce output: energy={energy}"
            );
        }
    }

    #[test]
    fn granular_various_semitones() {
        // Test that different semitone values produce different output.
        let semitone_values = [-12.0, -7.0, -5.0, -1.0, 0.0, 1.0, 5.0, 7.0, 12.0];
        let mut outputs = Vec::new();

        for &st in &semitone_values {
            let mut chain = PitchChain::new();
            chain.algorithm = Algorithm::Granular;
            chain.semitones = st;
            chain.mix = 1.0;
            chain.update(config());

            let mut all = Vec::new();
            for b in 0..50 {
                let mut left = sine_block(220.0, b * 512, 512);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);
                all.extend_from_slice(&left);
            }
            outputs.push(all);
        }

        // Each distinct semitone setting should produce distinct output.
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                let diff: f64 = outputs[i]
                    .iter()
                    .zip(outputs[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / outputs[i].len() as f64;
                assert!(
                    diff > 0.0001,
                    "Semitones {} and {} should differ: avg_diff={diff}",
                    semitone_values[i],
                    semitone_values[j]
                );
            }
        }
    }

    #[test]
    fn psola_various_semitones() {
        for &st in &[-12.0, -7.0, -3.0, 0.0, 3.0, 7.0, 12.0] {
            let mut chain = PitchChain::new();
            chain.algorithm = Algorithm::Psola;
            chain.semitones = st;
            chain.mix = 1.0;
            chain.update(config());

            for b in 0..100 {
                let mut left = sine_block(220.0, b * 512, 512);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);

                for (i, s) in left.iter().enumerate() {
                    assert!(
                        s.is_finite(),
                        "PSOLA NaN at semitones={st}, block {b} sample {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn divider_bypasses_on_positive_semitones() {
        let mut chain = PitchChain::new();
        chain.algorithm = Algorithm::FreqDivider;
        chain.semitones = 5.0; // Upshift — divider can't do this.
        chain.mix = 1.0;
        chain.update(config());

        // With mix set to 1.0 but divider forced to mix=0 (bypass),
        // output should equal input (dry passthrough).
        let input = sine_block(440.0, 0, 512);
        let mut left = input.clone();
        let mut right = input.clone();
        chain.process(&mut left, &mut right);

        for (i, (out, inp)) in left.iter().zip(input.iter()).enumerate() {
            assert!(
                (out - inp).abs() < 1e-10,
                "Divider should bypass on upshift at sample {i}"
            );
        }
    }

    #[test]
    fn algorithm_switching() {
        let mut chain = PitchChain::new();
        chain.semitones = -12.0;
        chain.update(config());
        chain.reset();

        for (idx, algo) in [
            Algorithm::FreqDivider,
            Algorithm::Pll,
            Algorithm::Granular,
            Algorithm::Psola,
            Algorithm::Wsola,
            Algorithm::PolyOctave,
        ]
        .iter()
        .enumerate()
        {
            chain.algorithm = *algo;
            let mut left = sine_block(220.0, idx * 512, 512);
            let mut right = left.clone();
            chain.process(&mut left, &mut right);

            for (i, s) in left.iter().enumerate() {
                assert!(s.is_finite(), "{algo:?} produced NaN at sample {i}");
            }
        }
    }

    #[test]
    fn no_nan_all_algorithms() {
        for algo in all_algorithms() {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.update(config());

            for b in 0..200 {
                let mut left = sine_block(82.0, b * 512, 512);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);

                for (i, s) in left.iter().enumerate() {
                    assert!(s.is_finite(), "{algo:?} NaN at block {b} sample {i}");
                }
            }
        }
    }

    #[test]
    fn different_algorithms_differ() {
        let mut outputs: Vec<Vec<f64>> = Vec::new();

        for algo in all_algorithms() {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.update(config());

            let mut all = Vec::new();
            for b in 0..50 {
                let mut left = sine_block(220.0, b * 512, 512);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);
                all.extend_from_slice(&left);
            }
            outputs.push(all);
        }

        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                let diff: f64 = outputs[i]
                    .iter()
                    .zip(outputs[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / outputs[i].len() as f64;
                // PSOLA (3) and WSOLA (4) are OLA-family variants that share
                // the same dual-head crossfade architecture — they differ only
                // in pitch-adaptive xcorr parameters, so expect smaller diffs.
                let threshold = if (i == 3 && j == 4) || (i == 4 && j == 3) {
                    0.0001
                } else {
                    0.001
                };
                assert!(
                    diff > threshold,
                    "Algorithms {i} and {j} should differ: avg_diff={diff}"
                );
            }
        }
    }

    #[test]
    fn mix_zero_passes_dry() {
        // PolyOctave excluded: SSB filter bank has 1-sample latency
        // on dry path; dry-wet correctness tested in pog::tests::dry_wet_mix.
        for algo in all_algorithms()
            .into_iter()
            .filter(|a| *a != Algorithm::PolyOctave)
        {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.mix = 0.0;
            chain.update(config());

            let input = sine_block(440.0, 0, 512);
            let mut left = input.clone();
            let mut right = input.clone();
            chain.process(&mut left, &mut right);

            for (i, (out, inp)) in left.iter().zip(input.iter()).enumerate() {
                assert!(
                    (out - inp).abs() < 1e-10,
                    "{algo:?} mix=0 should pass dry at sample {i}: diff={}",
                    (out - inp).abs()
                );
            }
        }
    }

    #[test]
    fn latency_standard_mode() {
        let mut chain = PitchChain::new();
        chain.live = false;
        chain.update(config());
        // Process one block so sync_params runs.
        let mut l = sine_block(220.0, 0, 512);
        let mut r = l.clone();

        for algo in all_algorithms() {
            chain.algorithm = algo;
            chain.process(&mut l, &mut r);
            let lat = chain.latency();
            let ms = lat as f64 / SR * 1000.0;
            eprintln!("  {algo:?} standard: {lat} samples ({ms:.1} ms)");
        }

        // Zero-latency algorithms.
        chain.algorithm = Algorithm::FreqDivider;
        assert_eq!(chain.latency(), 0);
        chain.algorithm = Algorithm::Pll;
        assert_eq!(chain.latency(), 0);

        // Time-domain algorithms: moderate latency.
        chain.algorithm = Algorithm::Granular;
        assert!(
            chain.latency() <= 4096,
            "Granular too high: {}",
            chain.latency()
        );
        chain.algorithm = Algorithm::Psola;
        assert!(chain.latency() > 0 && chain.latency() <= 4096);
        chain.algorithm = Algorithm::Wsola;
        assert!(
            chain.latency() <= 2048,
            "WSOLA too high: {}",
            chain.latency()
        );
    }

    #[test]
    fn latency_live_mode() {
        let mut chain = PitchChain::new();
        chain.live = true;
        chain.update(config());
        // Process one block so sync_params runs and live mode propagates.
        let mut l = sine_block(220.0, 0, 512);
        let mut r = l.clone();

        for algo in all_algorithms() {
            chain.algorithm = algo;
            chain.process(&mut l, &mut r);
            let lat = chain.latency();
            let ms = lat as f64 / SR * 1000.0;
            eprintln!("  {algo:?} live: {lat} samples ({ms:.1} ms)");
        }

        // In live mode, all algorithms should be under 25ms (~1200 samples at 48k).
        // Rubberband R2 real-time has ~1024 sample minimum internal latency.
        let live_limit_samples = (SR * 0.025) as usize; // 1200 samples
        for algo in all_algorithms() {
            chain.algorithm = algo;
            chain.process(&mut l, &mut r);
            let lat = chain.latency();
            assert!(
                lat <= live_limit_samples,
                "{algo:?} live latency too high: {lat} samples ({:.1} ms), limit {live_limit_samples}",
                lat as f64 / SR * 1000.0,
            );
        }
    }

    #[test]
    fn live_mode_reduces_latency() {
        for algo in all_algorithms() {
            let mut standard = PitchChain::new();
            standard.algorithm = algo;
            standard.live = false;
            standard.semitones = -12.0;
            standard.update(config());
            let mut l = sine_block(220.0, 0, 512);
            let mut r = l.clone();
            standard.process(&mut l, &mut r);
            let std_lat = standard.latency();

            let mut live = PitchChain::new();
            live.algorithm = algo;
            live.live = true;
            live.semitones = -12.0;
            live.update(config());
            let mut l = sine_block(220.0, 0, 512);
            let mut r = l.clone();
            live.process(&mut l, &mut r);
            let live_lat = live.latency();

            eprintln!(
                "  {algo:?}: standard={std_lat} live={live_lat} ({})",
                if live_lat < std_lat {
                    "reduced"
                } else if live_lat == std_lat {
                    "same"
                } else {
                    "INCREASED!"
                }
            );

            // Live should never increase latency.
            assert!(
                live_lat <= std_lat,
                "{algo:?} live latency ({live_lat}) > standard ({std_lat})!"
            );
        }
    }

    #[test]
    fn throughput_all_algorithms() {
        let block_size = 512;
        let num_blocks = 200;
        let total_samples = block_size * num_blocks;

        for algo in all_algorithms() {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.mix = 1.0;
            chain.update(config());

            let start = std::time::Instant::now();
            for b in 0..num_blocks {
                let mut left = sine_block(220.0, b * block_size, block_size);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);
            }
            let elapsed = start.elapsed();

            let audio_duration_s = total_samples as f64 / SR;
            let process_s = elapsed.as_secs_f64();
            let realtime_ratio = audio_duration_s / process_s;

            eprintln!(
                "  {algo:?}: {total_samples} samples in {:.1}ms ({realtime_ratio:.0}x realtime)",
                process_s * 1000.0,
            );

            // In release mode all algorithms must be faster than realtime.
            // In debug mode (unoptimized), some heavy algorithms (PSOLA, Rubberband)
            // may be slower — just check they complete without hanging.
            #[cfg(not(debug_assertions))]
            assert!(
                realtime_ratio > 1.0,
                "{algo:?} is SLOWER than realtime: {realtime_ratio:.1}x"
            );
        }
    }

    #[test]
    fn live_mode_produces_output() {
        // Verify live mode still produces actual audio, not silence.
        for algo in all_algorithms() {
            let mut chain = PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = -12.0;
            chain.mix = 1.0;
            chain.live = true;
            chain.update(config());

            let mut energy = 0.0;
            for b in 0..100 {
                let mut left = sine_block(220.0, b * 512, 512);
                let mut right = left.clone();
                chain.process(&mut left, &mut right);
                if b > 20 {
                    energy += left.iter().map(|s| s * s).sum::<f64>();
                }
            }

            assert!(
                energy > 0.1,
                "{algo:?} live mode produced no output: energy={energy}"
            );
        }
    }
}
