//! FTS-Saturate: the full distortion chain, measured.

use signal_fx::{NativeSaturate, SATURATE_HARMONIC_BASE};
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48000.0;
const N: usize = 96_000;

fn no_events() -> PluginEvents<'static> {
    PluginEvents { params: &[], midi: &[], note_expressions: &[] }
}

fn process(sat: &mut NativeSaturate, input_l: &[f32], input_r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    sat.prepare(SR, 512).unwrap();
    let n = input_l.len();
    let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (i, chunk) in input_l.chunks(512).enumerate() {
        let s = i * 512;
        sat.process_block(chunk, &input_r[s..s + chunk.len()],
            &mut ol[s..s + chunk.len()], &mut or[s..s + chunk.len()], &no_events()).unwrap();
    }
    (ol, or)
}

fn rms(b: &[f32]) -> f64 {
    (b.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / b.len() as f64).sqrt()
}

fn tone(buf: &[f32], freq: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &x) in buf.iter().enumerate() {
        let ph = core::f64::consts::TAU * freq * i as f64 / SR;
        re += f64::from(x) * ph.cos();
        im += f64::from(x) * ph.sin();
    }
    ((re * re + im * im).sqrt() / buf.len() as f64).max(1e-12)
}

fn sine(freq: f64, amp: f64) -> Vec<f32> {
    (0..N).map(|i| (amp * (core::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32).collect()
}

#[test]
fn oversampling_kills_aliasing() {
    // Hard clip on a high tone: at 1x an alias of H3 folds into the
    // audible band; at 8x it must drop sharply. 15 kHz H3 = 45 kHz →
    // aliases to 3 kHz at 48 kHz.
    let run = |os: f64| -> f64 {
        let mut s = NativeSaturate::new(SR);
        s.set_named("drive", 18.0);
        s.set_named("pos_shaper", 5.0); // Hard
        s.set_named("neg_shaper", 5.0);
        s.set_named("auto_gain", 0.0);
        s.set_named("oversample", os);
        let input = sine(15_000.0, 0.8);
        let (l, _) = process(&mut s, &input, &input);
        tone(&l[N / 2..], 3000.0)
    };
    let alias_1x = run(0.0);
    let alias_8x = run(3.0);
    let drop_db = 20.0 * (alias_8x / alias_1x).log10();
    assert!(
        drop_db < -20.0,
        "8x oversampling must crush the 3 kHz alias: {drop_db:+.1} dB"
    );
}

#[test]
fn auto_gain_holds_loudness_under_drive() {
    let mut s = NativeSaturate::new(SR);
    s.set_named("drive", 24.0);
    s.set_named("pos_shaper", 1.0);
    s.set_named("neg_shaper", 1.0);
    s.set_named("auto_gain", 1.0);
    let input = sine(500.0, 0.4);
    let (l, _) = process(&mut s, &input, &input);
    let g = 20.0 * (rms(&l[N / 2..]) / rms(&input[N / 2..])).log10();
    assert!(g.abs() < 3.0, "auto gain holds loudness within 3 dB: {g:+.1}");
}

#[test]
fn lf_protect_keeps_bass_clean() {
    // 60 Hz + 3 kHz together, heavy drive. Without protection the bass
    // intermodulates; with lf_protect at 120 Hz the 60 Hz fundamental
    // passes clean (unshapen) while the 3 kHz still distorts.
    let mk_input = || -> Vec<f32> {
        (0..N).map(|i| {
            let a = 0.6 * (core::f64::consts::TAU * 60.0 * i as f64 / SR).sin();
            let b = 0.2 * (core::f64::consts::TAU * 3000.0 * i as f64 / SR).sin();
            (a + b) as f32
        }).collect()
    };
    let run = |lf: f64| -> (f64, f64) {
        let mut s = NativeSaturate::new(SR);
        s.set_named("drive", 20.0);
        s.set_named("pos_shaper", 5.0);
        s.set_named("neg_shaper", 5.0);
        s.set_named("auto_gain", 0.0);
        s.set_named("lf_protect", lf);
        let input = mk_input();
        let (l, _) = process(&mut s, &input, &input);
        let late = &l[N / 2..];
        // 2nd harmonic of the bass (120 Hz) = bass distortion product.
        (tone(late, 60.0), tone(late, 120.0))
    };
    let (_, h2_unprot) = run(0.0);
    let (fund_prot, h2_prot) = run(300.0);
    assert!(
        h2_prot < h2_unprot * 0.2,
        "protected bass distorts far less: {h2_prot:e} vs {h2_unprot:e}"
    );
    assert!(fund_prot > 0.1, "bass fundamental survives: {fund_prot:e}");
}

#[test]
fn delta_listen_and_harmonics_readback() {
    let mut s = NativeSaturate::new(SR);
    s.set_named("drive", 12.0);
    s.set_named("pos_shaper", 1.0);
    s.set_named("neg_shaper", 1.0);
    s.set_named("q_point", 0.5);
    s.set_named("auto_gain", 0.0);
    s.set_named("listen", 1.0);
    let input = sine(500.0, 0.4);
    let (l, _) = process(&mut s, &input, &input);
    // Delta carries mostly harmonics: H2 (1 kHz) energy must rival the
    // residual fundamental leakage.
    let late = &l[N / 2..];
    assert!(tone(late, 1000.0) > 1.0e-4, "delta carries the 2nd harmonic");
    // Readback contract mirrors the preamp.
    let h2 = s.param_value(SATURATE_HARMONIC_BASE + 1).unwrap();
    assert!(h2 > 0.01, "harmonic readback alive: {h2:.4}");
}

#[test]
fn emphasis_eq_mirror_is_tonally_transparent_when_clean() {
    // +12 dB bell at 3 kHz emphasis, CLEAN shapers: the mirror must
    // null the curve — output magnitude ≈ input at every probe.
    for &freq in &[200.0, 3000.0, 8000.0] {
        let mut s = NativeSaturate::new(SR);
        s.set_named("drive", 0.0);
        s.set_named("pos_shaper", 0.0); // Clean
        s.set_named("neg_shaper", 0.0);
        s.set_named("auto_gain", 0.0);
        s.set_named("oversample", 0.0);
        s.set_named("eq_b1_used", 1.0);
        s.set_named("eq_b1_on", 1.0);
        s.set_named("eq_b1_freq", 3000.0);
        s.set_named("eq_b1_q", 1.0);
        s.set_named("eq_b1_gain", 12.0);
        let input = sine(freq, 0.3);
        let (l, _) = process(&mut s, &input, &input);
        let g = 20.0 * (rms(&l[N / 2..]) / rms(&input[N / 2..])).log10();
        assert!(
            g.abs() < 1.0,
            "mirror EQ must null at {freq} Hz with clean shapers: {g:+.2} dB"
        );
    }
}

#[test]
fn emphasis_eq_targets_the_drive() {
    // Same +12 dB bell at 3 kHz emphasis, HARD shapers: the 3 kHz tone
    // distorts far more than an equal-level 500 Hz tone — the curve
    // chose what saturates, while auto-gain and the mirror keep the
    // overall balance.
    let run = |freq: f64| -> f64 {
        let mut s = NativeSaturate::new(SR);
        s.set_named("drive", 12.0);
        s.set_named("pos_shaper", 5.0);
        s.set_named("neg_shaper", 5.0);
        s.set_named("auto_gain", 0.0);
        s.set_named("eq_b1_used", 1.0);
        s.set_named("eq_b1_on", 1.0);
        s.set_named("eq_b1_freq", 3000.0);
        s.set_named("eq_b1_q", 1.0);
        s.set_named("eq_b1_gain", 12.0);
        let input = sine(freq, 0.3);
        let (l, _) = process(&mut s, &input, &input);
        // THD proxy: 3rd harmonic relative to fundamental.
        let late = &l[N / 2..];
        tone(late, freq * 3.0) / tone(late, freq)
    };
    let thd_emphasized = run(3000.0);
    let thd_plain = run(500.0);
    assert!(
        thd_emphasized > thd_plain * 3.0,
        "the emphasized region must distort much harder: {thd_emphasized:.4} vs {thd_plain:.4}"
    );
}
