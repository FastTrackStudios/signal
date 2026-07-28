//! Multiband splitter core — make ANY effect multiband (ShaperBox
//! model: N frequency bands, an independent processor per band,
//! recombine to a flat sum).
//!
//! Topology: sequential Linkwitz-Riley 4th-order splits from the
//! lowest crossover up. After each split, every already-separated band
//! runs through the crossover's matching 2nd-order allpass (an LR4
//! LP+HP pair sums to exactly that allpass), so ALL bands arrive at
//! the recombine with matched phase and the sum is magnitude-flat.
//! Standard published crossover math (Linkwitz 1976, RBJ cookbook) —
//! no third-party code.
//!
//! Processing-core rules apply: no allocation in `process` (buffers
//! preallocated by `prepare`), no threads, no I/O.
//!
//! `Multiband` is the generic host: `process(l, r, |band, bl, br| …)`
//! hands each band's buffers to the caller's closure — wrap any inner
//! effect (a compressor per band, a pattern modulator per band à la
//! ShaperBox, a saturator per band…). One band = bit-exact
//! passthrough of the closure over the raw buffers (zero split cost).

/// Maximum supported bands (5 crossovers).
pub const MAX_BANDS: usize = 6;

/// One TDF2 biquad section, stereo state.
#[derive(Debug, Clone, Copy, Default)]
struct Sos {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

impl Sos {
    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

const Q_BUTTERWORTH: f64 = core::f64::consts::FRAC_1_SQRT_2;

fn rbj(freq: f64, q: f64, sample_rate: f64) -> (f64, f64, f64) {
    let w0 = core::f64::consts::TAU * (freq / sample_rate).clamp(1.0e-5, 0.49);
    let alpha = w0.sin() / (2.0 * q);
    (w0.cos(), alpha, 1.0 + alpha)
}

fn lowpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: (1.0 - cw) / 2.0 / a0,
        b1: (1.0 - cw) / a0,
        b2: (1.0 - cw) / 2.0 / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

fn highpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: (1.0 + cw) / 2.0 / a0,
        b1: -(1.0 + cw) / a0,
        b2: (1.0 + cw) / 2.0 / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

fn allpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: (1.0 - alpha) / a0,
        b1: -2.0 * cw / a0,
        b2: (1.0 + alpha) / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

/// One LR4 crossover: LP/HP each = two cascaded Butterworth-2 sections;
/// plus the matching 2nd-order allpass for phase-correcting bands that
/// were split off earlier.
#[derive(Debug, Clone)]
struct Crossover {
    freq: f64,
    lp: [Sos; 2],
    hp: [Sos; 2],
    /// One allpass per earlier band (each needs its own state).
    correction: [Sos; MAX_BANDS],
}

impl Crossover {
    fn new(freq: f64, sample_rate: f64) -> Self {
        Self {
            freq,
            lp: [lowpass(freq, sample_rate); 2],
            hp: [highpass(freq, sample_rate); 2],
            correction: [allpass(freq, sample_rate); MAX_BANDS],
        }
    }

    fn reset(&mut self) {
        for s in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            s.reset();
        }
        for s in &mut self.correction {
            s.reset();
        }
    }
}

/// The splitter: crossover frequencies (sorted ascending) → N = k+1
/// bands. Band buffers are caller-provided (`Multiband` owns a set).
#[derive(Debug, Clone)]
pub struct BandSplitter {
    crossovers: Vec<Crossover>,
    sample_rate: f64,
}

impl BandSplitter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            crossovers: Vec::with_capacity(MAX_BANDS - 1),
            sample_rate: sample_rate.max(1.0),
        }
    }

    /// Configure crossover frequencies (unsorted OK; deduped, clamped,
    /// capped at MAX_BANDS−1). Allocation-free after the first call at
    /// full size. Resets filter state.
    pub fn set_crossovers(&mut self, freqs: &[f64]) {
        let mut sorted: [f64; MAX_BANDS - 1] = [0.0; MAX_BANDS - 1];
        let mut n = 0;
        for &f in freqs.iter().take(MAX_BANDS - 1) {
            sorted[n] = f.clamp(20.0, self.sample_rate * 0.45);
            n += 1;
        }
        sorted[..n].sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.crossovers.clear();
        for &f in sorted[..n].iter() {
            // Skip near-duplicates (< 1/12 octave apart).
            if self
                .crossovers
                .last()
                .is_some_and(|c| f / c.freq < 1.06)
            {
                continue;
            }
            self.crossovers.push(Crossover::new(f, self.sample_rate));
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        let freqs: Vec<f64> = self.crossovers.iter().map(|c| c.freq).collect();
        self.set_crossovers(&freqs);
    }

    pub fn num_bands(&self) -> usize {
        self.crossovers.len() + 1
    }

    /// Split one stereo sample into `bands` (caller slices sized ≥
    /// num_bands). Band 0 = lowest.
    #[inline]
    pub fn tick(&mut self, l: f64, r: f64, bands_l: &mut [f64], bands_r: &mut [f64]) {
        let (mut rem_l, mut rem_r) = (l, r);
        let n = self.crossovers.len();
        for k in 0..n {
            let c = &mut self.crossovers[k];
            // Split the lowest remaining range off.
            let low_l = {
                let s = c.lp[0].tick(0, rem_l);
                c.lp[1].tick(0, s)
            };
            let low_r = {
                let s = c.lp[0].tick(1, rem_r);
                c.lp[1].tick(1, s)
            };
            let hi_l = {
                let s = c.hp[0].tick(0, rem_l);
                c.hp[1].tick(0, s)
            };
            let hi_r = {
                let s = c.hp[0].tick(1, rem_r);
                c.hp[1].tick(1, s)
            };
            bands_l[k] = low_l;
            bands_r[k] = low_r;
            rem_l = hi_l;
            rem_r = hi_r;
            // Phase-match every earlier band through this crossover's
            // allpass so the final sum stays flat.
            for j in 0..k {
                bands_l[j] = c.correction[j].tick(0, bands_l[j]);
                bands_r[j] = c.correction[j].tick(1, bands_r[j]);
            }
        }
        bands_l[n] = rem_l;
        bands_r[n] = rem_r;
    }

    pub fn reset(&mut self) {
        for c in &mut self.crossovers {
            c.reset();
        }
    }
}

/// Per-band output controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandControls {
    pub gain_db: f64,
    pub mute: bool,
    pub solo: bool,
}

impl Default for BandControls {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// Generic multiband host: split → per-band closure → recombine.
///
/// The closure receives `(band_index, left, right)` block slices; run
/// any effect on them. With no crossovers configured the closure gets
/// the raw buffers directly (band 0) — zero split cost.
pub struct Multiband {
    pub splitter: BandSplitter,
    pub controls: [BandControls; MAX_BANDS],
    band_l: Vec<Vec<f64>>,
    band_r: Vec<Vec<f64>>,
    block: usize,
}

impl Multiband {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            splitter: BandSplitter::new(sample_rate),
            controls: [BandControls::default(); MAX_BANDS],
            band_l: (0..MAX_BANDS).map(|_| Vec::new()).collect(),
            band_r: (0..MAX_BANDS).map(|_| Vec::new()).collect(),
            block: 0,
        }
    }

    /// Preallocate band buffers. Call from setup, never the audio path.
    pub fn prepare(&mut self, max_block: usize) {
        self.block = max_block.max(1);
        for b in self.band_l.iter_mut().chain(self.band_r.iter_mut()) {
            b.resize(self.block, 0.0);
        }
    }

    pub fn num_bands(&self) -> usize {
        self.splitter.num_bands()
    }

    /// Process a stereo block: split into bands, run `f` per band,
    /// apply per-band gain/mute/solo, sum back into `l`/`r`.
    pub fn process<F>(&mut self, l: &mut [f64], r: &mut [f64], mut f: F)
    where
        F: FnMut(usize, &mut [f64], &mut [f64]),
    {
        let n_bands = self.splitter.num_bands();
        if n_bands == 1 {
            // Single band: the closure runs on the raw buffers.
            f(0, l, r);
            let c = &self.controls[0];
            let g = if c.mute {
                0.0
            } else {
                10.0f64.powf(c.gain_db / 20.0)
            };
            if (g - 1.0).abs() > 1.0e-12 {
                for i in 0..l.len() {
                    l[i] *= g;
                    r[i] *= g;
                }
            }
            return;
        }
        let len = l.len().min(r.len()).min(self.block);
        // Split.
        let mut sl = [0.0; MAX_BANDS];
        let mut sr = [0.0; MAX_BANDS];
        for i in 0..len {
            self.splitter.tick(l[i], r[i], &mut sl, &mut sr);
            for b in 0..n_bands {
                self.band_l[b][i] = sl[b];
                self.band_r[b][i] = sr[b];
            }
        }
        // Per-band processing.
        for b in 0..n_bands {
            f(b, &mut self.band_l[b][..len], &mut self.band_r[b][..len]);
        }
        // Recombine with gain/mute/solo.
        let any_solo = self.controls[..n_bands].iter().any(|c| c.solo);
        for i in 0..len {
            let mut ol = 0.0;
            let mut or = 0.0;
            for b in 0..n_bands {
                let c = &self.controls[b];
                let audible = !c.mute && (!any_solo || c.solo);
                if audible {
                    let g = 10.0f64.powf(c.gain_db / 20.0);
                    ol += self.band_l[b][i] * g;
                    or += self.band_r[b][i] * g;
                }
            }
            l[i] = ol;
            r[i] = or;
        }
    }

    pub fn reset(&mut self) {
        self.splitter.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn rms(buf: &[f64]) -> f64 {
        (buf.iter().map(|x| x * x).sum::<f64>() / buf.len() as f64).sqrt()
    }

    fn sine(freq: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| (core::f64::consts::TAU * freq * i as f64 / SR).sin() * 0.5)
            .collect()
    }

    #[test]
    fn identity_processing_sums_flat() {
        // 3-band split, no processing: magnitude must survive within
        // 0.1 dB at frequencies across the spectrum (incl. right at
        // the crossovers).
        for &freq in &[50.0, 200.0, 800.0, 2000.0, 5000.0, 12000.0] {
            let mut mb = Multiband::new(SR);
            mb.splitter.set_crossovers(&[200.0, 2000.0]);
            mb.prepare(48_000);
            let mut l = sine(freq, 48_000);
            let mut r = l.clone();
            let dry = l.clone();
            mb.process(&mut l, &mut r, |_, _, _| {});
            let g = 20.0 * (rms(&l[24_000..]) / rms(&dry[24_000..])).log10();
            assert!(
                g.abs() < 0.1,
                "flat sum must be magnitude-transparent at {freq} Hz: {g:+.3} dB"
            );
        }
    }

    #[test]
    fn bands_isolate_their_ranges() {
        let mut mb = Multiband::new(SR);
        mb.splitter.set_crossovers(&[200.0, 2000.0]);
        mb.prepare(48_000);
        // 100 Hz content lands in band 0; 8 kHz in band 2.
        let mut captured = vec![0.0f64; 3];
        let mut l = sine(100.0, 48_000);
        let mut r = l.clone();
        mb.process(&mut l, &mut r, |b, bl, _| {
            captured[b] += bl[24_000..].iter().map(|x| x * x).sum::<f64>();
        });
        assert!(
            captured[0] > 20.0 * captured[1] && captured[0] > 100.0 * captured[2],
            "100 Hz belongs to band 0: {captured:?}"
        );
    }

    #[test]
    fn single_band_is_directly_processed() {
        let mut mb = Multiband::new(SR);
        mb.prepare(512);
        let mut l = sine(440.0, 512);
        let mut r = l.clone();
        let dry = l.clone();
        // The closure must see the raw buffer (bit-exact).
        mb.process(&mut l, &mut r, |b, bl, _| {
            assert_eq!(b, 0);
            assert_eq!(bl, &dry[..]);
        });
        assert_eq!(l, dry, "single band with no processing is bit-exact");
    }

    #[test]
    fn solo_and_gain_apply_per_band() {
        let mut mb = Multiband::new(SR);
        mb.splitter.set_crossovers(&[1000.0]);
        mb.prepare(48_000);
        mb.controls[0].solo = true; // low band only
        let mut l = sine(100.0, 48_000);
        let mut r = l.clone();
        let dry_low = l.clone();
        mb.process(&mut l, &mut r, |_, _, _| {});
        let low_g = 20.0 * (rms(&l[24_000..]) / rms(&dry_low[24_000..])).log10();
        assert!(low_g.abs() < 0.2, "low content passes low-band solo: {low_g:+.2}");

        let mut l2 = sine(8000.0, 48_000);
        let mut r2 = l2.clone();
        let dry_hi = l2.clone();
        mb.process(&mut l2, &mut r2, |_, _, _| {});
        let hi_g = 20.0 * (rms(&l2[24_000..]) / rms(&dry_hi[24_000..])).log10();
        assert!(hi_g < -40.0, "high content dies under low-band solo: {hi_g:+.1}");
    }

    #[test]
    fn per_band_processing_stays_in_its_lane() {
        // Gain +6 dB on the high band only: low sine unchanged, high
        // sine boosted.
        let run = |freq: f64| -> f64 {
            let mut mb = Multiband::new(SR);
            mb.splitter.set_crossovers(&[1000.0]);
            mb.prepare(48_000);
            let mut l = sine(freq, 48_000);
            let mut r = l.clone();
            let dry = l.clone();
            mb.process(&mut l, &mut r, |b, bl, br| {
                if b == 1 {
                    for x in bl.iter_mut().chain(br.iter_mut()) {
                        *x *= 2.0; // +6 dB
                    }
                }
            });
            20.0 * (rms(&l[24_000..]) / rms(&dry[24_000..])).log10()
        };
        let low = run(100.0);
        let high = run(8000.0);
        assert!(low.abs() < 0.3, "low band untouched: {low:+.2} dB");
        assert!((high - 6.0).abs() < 0.3, "high band boosted: {high:+.2} dB");
    }
}
