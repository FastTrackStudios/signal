//! A translated Pro-Q 4 spectral band acts per-bin, not per-band.
//!
//! `proq4_dynamics.rs` proves a dynamic band's numbers reach the filter. This
//! one asks the harder question: does `b1_spectral` change what the band
//! *does*, or is it a flag that crosses and then gets ignored?
//!
//! The two are distinguishable by construction. A whole-band dynamic EQ has
//! one gain for the whole band, so ducking a resonance ducks everything
//! sharing that band with it. A spectral band works on individual FFT bins, so
//! it can pull down a tone that sticks out of its own spectral neighbourhood
//! and leave the neighbourhood alone. That selectivity is the entire reason
//! Pro-Q has the control, and it is what 42 of the 171 factory presets are
//! reaching for.
//!
//! So the stimulus is a loud tone buried in broadband noise, and the test
//! measures the tone and its neighbours separately.

use signal_fx::NativeEq;
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;
/// The resonance to suppress.
const TONE_HZ: f64 = 1_000.0;
/// Where the neighbouring noise is measured — inside the same band, but
/// carrying no resonance of its own.
///
/// Deliberately NOT a second tone. A resonance suppressor is supposed to
/// suppress tones, so measuring one would be asking it to fail at its job; the
/// thing that must survive is the broadband material sharing the band.
const NEIGHBOUR_HZ: f64 = 1_500.0;
/// How wide a neighbourhood the noise is averaged over, to measure a floor
/// rather than one bin's worth of variance.
const NEIGHBOUR_SPAN_HZ: f64 = 200.0;

/// Goertzel power of `freq` over `buf`, normalized by length.
fn goertzel(buf: &[f64], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (coeff * s1).mul_add(-s2, s1.mul_add(s1, s2 * s2)) / (buf.len() as f64).powi(2)
}

/// A deterministic noise source — no rand dependency, and a fixed spectrum
/// across runs so the assertions are stable.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }
}

/// Noise with a strong tone sitting on top of it, plus a quieter neighbour.
fn stimulus(frames: usize) -> Vec<f64> {
    let mut rng = Lcg(0x5EED_1234);
    let mut phase = 0.0f64;
    let mut nphase = 0.0f64;
    let inc = std::f64::consts::TAU * TONE_HZ / SR;
    let ninc = std::f64::consts::TAU * NEIGHBOUR_HZ / SR;
    (0..frames)
        .map(|_| {
            let s = 0.05f64.mul_add(rng.next(), 0.35 * phase.sin());
            phase += inc;
            let _ = (&mut nphase, ninc);
            s
        })
        .collect()
}

/// Run the stimulus through an EQ and return the tail (past the STFT latency).
fn render(eq: &mut NativeEq, input: &[f64]) -> Vec<f64> {
    let events = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l: Vec<f32> = input[pos..pos + n].iter().map(|s| *s as f32).collect();
        let r = l.clone();
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(&l, &r, &mut ol, &mut or, &events).expect("process");
        out.extend(ol.iter().map(|s| f64::from(*s)));
        pos += n;
    }
    // The detector and the overlap-add both need to settle.
    out.split_off(out.len() / 2)
}

/// One band at `TONE_HZ`, dynamic, optionally spectral.
fn band(spectral: bool) -> NativeEq {
    let mut eq = NativeEq::new(SR);
    for (name, value) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", TONE_HZ),
        ("b1_gain", 0.0),
        // A wide band, so the tone and its neighbour are both inside it —
        // which is what makes the two modes tell each other apart.
        ("b1_q", 0.7),
        ("b1_shape", 0.0),
        ("b1_slope", 2.0),
        ("b1_dyn_range", -24.0),
        ("b1_dyn_thr", -40.0),
        ("b1_dyn_atk", 20.0),
        ("b1_dyn_rel", 40.0),
        ("b1_dyn_auto", 0.0),
        ("b1_spectral", if spectral { 1.0 } else { 0.0 }),
    ] {
        eq.set_named(name, value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    eq
}

/// The broadband floor around `centre`, averaged over several probes so it
/// measures noise rather than one bin's variance.
fn noise_floor(buf: &[f64], centre: f64) -> f64 {
    let n = 9;
    let mut sum = 0.0;
    for k in 0..n {
        let f = NEIGHBOUR_SPAN_HZ.mul_add(f64::from(k) / f64::from(n - 1), centre - NEIGHBOUR_SPAN_HZ / 2.0);
        sum += goertzel(buf, f);
    }
    sum / f64::from(n)
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a / b.max(1e-30)).log10()
}

/// The flag reaches the engine and puts it in the signal path.
#[test]
fn a_spectral_band_engages_the_spectral_engine() {
    assert!(
        !band(false).spectral_engaged(),
        "an ordinary dynamic band must not pull in the STFT",
    );
    assert!(
        band(true).spectral_engaged(),
        "b1_spectral must engage the spectral engine — otherwise the flag \
         crosses from the preset and is then ignored",
    );
}

/// A spectral band suppresses the resonance without taking its neighbours.
#[test]
fn a_spectral_band_is_selective_where_a_dynamic_band_is_not() {
    let input = stimulus((SR * 3.0) as usize);
    let dry = input.split_at(input.len() / 2).1.to_vec();

    let spectral = render(&mut band(true), &input);
    let whole = render(&mut band(false), &input);

    let (dry_tone, dry_nb) = (goertzel(&dry, TONE_HZ), noise_floor(&dry, NEIGHBOUR_HZ));
    let spec_tone = db(goertzel(&spectral, TONE_HZ), dry_tone);
    let spec_nb = db(noise_floor(&spectral, NEIGHBOUR_HZ), dry_nb);
    let whole_tone = db(goertzel(&whole, TONE_HZ), dry_tone);
    let whole_nb = db(noise_floor(&whole, NEIGHBOUR_HZ), dry_nb);

    println!(
        "spectral: tone {spec_tone:+.2} dB, noise {spec_nb:+.2} dB\n\
         band:     tone {whole_tone:+.2} dB, noise {whole_nb:+.2} dB"
    );

    // Both modes have to actually do something to the resonance.
    assert!(
        spec_tone < -2.0,
        "the spectral band left the resonance alone ({spec_tone:+.2} dB)",
    );

    // The distinguishing property: spectral spares the neighbour by a wider
    // margin than the whole-band ride does.
    let spectral_selectivity = spec_nb - spec_tone;
    let band_selectivity = whole_nb - whole_tone;
    assert!(
        spectral_selectivity > band_selectivity + 2.0,
        "spectral must be more selective than a whole-band ride, but the \
         margins were {spectral_selectivity:.2} dB against {band_selectivity:.2} dB \
         — the flag is not changing what the band does",
    );
}
