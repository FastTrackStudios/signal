//! What the stretcher does to real signals, measured.

#![allow(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::missing_panics_doc,
    clippy::suboptimal_flops,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::missing_const_for_fn
)]

use stretch_dsp::{Config, Source, Stretcher};

const SR: f64 = 48_000.0;

/// A source defined by a function of the frame index, per channel.
struct Fn<F: FnMut(i64, usize) -> f32>(F);

impl<F: FnMut(i64, usize) -> f32> Source for Fn<F> {
    fn read(&mut self, start: i64, left: &mut [f32], right: &mut [f32]) {
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let t = start + i as i64;
            *l = (self.0)(t, 0);
            *r = (self.0)(t, 1);
        }
    }
}

fn sine(freq: f64, phase_right: f64) -> Fn<impl FnMut(i64, usize) -> f32> {
    Fn(move |t, ch| {
        let x = core::f64::consts::TAU * freq * t as f64 / SR + if ch == 1 { phase_right } else { 0.0 };
        (0.5 * x.sin()) as f32
    })
}

fn render(stretcher: &mut Stretcher, source: &mut impl Source, frames: usize, ratio: f64, transpose: f32, block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; frames], vec![0.0; frames]);
    let mut at = 0;
    while at < frames {
        let n = block.min(frames - at);
        stretcher.render(&mut l[at..at + n], &mut r[at..at + n], ratio, transpose, source);
        at += n;
    }
    (l, r)
}

/// The frequency of a clean tone: its rising zero crossings, interpolated.
fn frequency(x: &[f32]) -> f64 {
    let mut crossings = Vec::new();
    for i in 1..x.len() {
        if x[i - 1] < 0.0 && x[i] >= 0.0 {
            let f = f64::from(-x[i - 1]) / f64::from(x[i] - x[i - 1]);
            crossings.push((i - 1) as f64 + f);
        }
    }
    let (first, last) = (crossings[0], *crossings.last().unwrap());
    (crossings.len() - 1) as f64 / ((last - first) / SR)
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
}

/// The phase of `freq` in `x` (a single-bin DFT).
fn phase_at(x: &[f32], freq: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let a = core::f64::consts::TAU * freq * i as f64 / SR;
        re += f64::from(v) * a.cos();
        im -= f64::from(v) * a.sin();
    }
    im.atan2(re)
}

#[test]
fn unstretched_it_is_the_input_from_the_first_sample() {
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    let mut src = sine(440.0, 0.3);
    let start = 12_345.0;
    s.seek(start, 1.0, 1.0, &mut src);
    let (l, _) = render(&mut s, &mut src, 24_000, 1.0, 1.0, 256);
    let mut expect = sine(440.0, 0.3);
    let (mut el, mut er) = (vec![0.0; 24_000], vec![0.0; 24_000]);
    expect.read(start as i64, &mut el, &mut er);
    let noise: f64 = l.iter().zip(&el).map(|(&a, &b)| f64::from(a - b).powi(2)).sum::<f64>();
    let signal: f64 = el.iter().map(|&b| f64::from(b).powi(2)).sum::<f64>();
    let snr = 10.0 * (signal / noise).log10();
    assert!(snr > 30.0, "identity SNR {snr:.1} dB — output 0 is input `position`, no latency");
}

#[test]
fn slower_keeps_the_pitch_and_the_level() {
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    let mut src = sine(440.0, 0.0);
    s.seek(0.0, 0.8, 1.0, &mut src);
    let (l, _) = render(&mut s, &mut src, 96_000, 0.8, 1.0, 512);
    let body = &l[8_000..];
    let f = frequency(body);
    assert!((f - 440.0).abs() < 440.0 * 0.003, "{f:.2} Hz at 80 % tempo");
    let (a, b) = body.split_at(body.len() / 2);
    let db = 20.0 * (rms(a) / rms(b)).log10();
    assert!(db.abs() < 1.0, "level steady: {db:.2} dB between halves");
    assert!((rms(body) - 0.5 / 2f64.sqrt()).abs() < 0.06, "level kept: rms {}", rms(body));
}

#[test]
fn a_fifth_up_keeps_the_length() {
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    let mut src = sine(440.0, 0.0);
    let up = 2f32.powf(7.0 / 12.0);
    s.seek(0.0, 1.0, up, &mut src);
    let (l, _) = render(&mut s, &mut src, 96_000, 1.0, up, 480);
    let f = frequency(&l[8_000..]);
    let want = 440.0 * f64::from(up);
    assert!((f - want).abs() < want * 0.003, "{f:.2} Hz, want {want:.2}");
}

#[test]
fn faster_and_lower_at_once() {
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    let mut src = sine(330.0, 0.0);
    let down = 2f32.powf(-5.0 / 12.0);
    s.seek(0.0, 1.25, down, &mut src);
    let (l, _) = render(&mut s, &mut src, 96_000, 1.25, down, 333);
    let f = frequency(&l[8_000..]);
    let want = 330.0 * f64::from(down);
    assert!((f - want).abs() < want * 0.003, "{f:.2} Hz, want {want:.2}");
}

#[test]
fn the_stereo_image_survives_a_stretch() {
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    let mut src = sine(440.0, core::f64::consts::FRAC_PI_2);
    s.seek(0.0, 0.8, 1.0, &mut src);
    let (l, r) = render(&mut s, &mut src, 48_000, 0.8, 1.0, 256);
    let seg = 20_000..28_000;
    let diff = phase_at(&r[seg.clone()], 440.0) - phase_at(&l[seg], 440.0);
    let diff = (diff + core::f64::consts::PI).rem_euclid(core::f64::consts::TAU) - core::f64::consts::PI;
    assert!((diff - core::f64::consts::FRAC_PI_2).abs() < 0.1, "L→R phase {diff:.3} rad, want π/2");
}

#[test]
fn the_output_does_not_depend_on_block_size() {
    let run = |block| {
        let mut s = Stretcher::new(Config::for_sample_rate(SR));
        let mut src = sine(523.25, 0.0);
        s.seek(1_000.0, 0.9, 1.1, &mut src);
        render(&mut s, &mut src, 20_000, 0.9, 1.1, block).0
    };
    assert_eq!(run(64), run(20_000));
}

#[test]
fn clicks_slowed_down_land_on_their_new_beats() {
    // A click every 0.25 s (a 240 bpm sixteenth), played at 80 %: every
    // 0.3125 s.
    let every = (0.25 * SR) as i64;
    let mut src = Fn(move |t, _| {
        let at = t.rem_euclid(every);
        if at < 48 { (1.0 - at as f32 / 48.0) * if at % 2 == 0 { 0.8 } else { -0.8 } } else { 0.0 }
    });
    let mut s = Stretcher::new(Config::for_sample_rate(SR));
    s.seek(0.0, 0.8, 1.0, &mut src);
    let (l, _) = render(&mut s, &mut src, 5 * 48_000, 0.8, 1.0, 256);
    // Envelope peaks, one per expected beat window.
    let spacing = 0.25 / 0.8 * SR;
    for beat in 1..12 {
        let centre = f64::from(beat) * spacing;
        let lo = (centre - spacing / 2.0) as usize;
        let hi = (centre + spacing / 2.0) as usize;
        let (peak_at, peak) = l[lo..hi]
            .iter()
            .enumerate()
            .map(|(i, &v)| (i + lo, v.abs()))
            .fold((0, 0.0f32), |a, b| if b.1 > a.1 { b } else { a });
        let off_ms = (peak_at as f64 - centre) / SR * 1000.0;
        assert!(off_ms.abs() < 6.0, "beat {beat}: click {off_ms:.1} ms from its place");
        assert!(peak > 0.25, "beat {beat}: click kept its punch ({peak:.2})");
    }
}
