//! Alignment against pairs whose true answer is known.
//!
//! Every case is a reference and a dub built from one phrase description
//! with a single property deliberately changed — a delay, a stretch, a
//! different pitch, a different level. That is the only way to assert an
//! alignment is *correct* rather than merely plausible: a matcher that
//! has learned to return "no change" scores beautifully on real material
//! and fails every test here.

use align_dsp::{AlignConfig, Features, Material, align, align_audio, extract, features};

const SR: f64 = 44_100.0;
const HOP: usize = 256;
const WINDOW: usize = 1024;
/// Frames per second the fixtures are analysed at, ~172 Hz.
const FPS: f64 = SR / HOP as f64;

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// One syllable: a pitched tone with an attack, a body and a release.
///
/// The harmonics matter — a bare sine has no energy above the
/// fundamental, so three of the four bands would be empty and the test
/// would not exercise the thing it is testing.
fn syllable(midi: f64, secs: f64, gain: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            phase += core::f64::consts::TAU * midi_to_hz(midi) / SR;
            let s = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
            // A fast attack so there is a real onset to find.
            s * (t / 0.008).min(1.0) * ((secs - t) / 0.04).clamp(0.0, 1.0) * 0.3 * gain
        })
        .collect()
}

fn silence(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs).max(0.0) as usize]
}

/// A phrase: `(midi, length, gap-after)`, optionally transposed and
/// time-scaled.
fn phrase(parts: &[(f64, f64, f64)], transpose: f64, rate: f64, gain: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for &(midi, len, after) in parts {
        out.extend(syllable(midi + transpose, len * rate, gain));
        out.extend(silence(after * rate));
    }
    out
}

/// The pitch a phrase is carrying at each sample, `None` in the gaps.
///
/// Stands in for what a detector would report. Tests run *with* pitch
/// because the shipped engine has it — this repo's callers hand it
/// `tune_dsp`'s YIN track — and a matcher tested blind is a matcher
/// tested in a configuration nobody uses. It matters more than it looks:
/// a sung phrase is highly self-similar in level alone, and level alone
/// cannot tell syllable four from syllable five.
fn pitch_track(parts: &[(f64, f64, f64)], transpose: f64, rate: f64, lead_in: f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; (SR * lead_in) as usize];
    for &(midi, len, after) in parts {
        out.extend(vec![midi + transpose; (SR * len * rate) as usize]);
        out.extend(vec![f64::NAN; (SR * after * rate) as usize]);
    }
    out
}

/// Sample-rate pitch reduced to one value per analysis frame.
fn per_frame(pitch: &[f64], frames: usize) -> Vec<Option<f64>> {
    (0..frames)
        .map(|i| {
            let at = i * HOP + WINDOW / 2;
            pitch.get(at).copied().filter(|v| !v.is_nan())
        })
        .collect()
}

const LINE: &[(f64, f64, f64)] = &[
    (60.0, 0.30, 0.12),
    (64.0, 0.25, 0.10),
    (67.0, 0.35, 0.14),
    (65.0, 0.30, 0.12),
    (62.0, 0.28, 0.10),
    (60.0, 0.40, 0.15),
];

fn analyse(audio: &[f64]) -> Features {
    extract(audio, SR, HOP, WINDOW, features::FeatureConfig::default())
}

/// Seconds the map moves dub time `t` by.
fn shift_at(a: &align_dsp::Alignment, secs: f64) -> f64 {
    let i = ((secs * a.frame_rate).round() as usize).min(a.map.len().saturating_sub(1));
    (a.map[i] - i as f64) / a.frame_rate
}

/// Analyse a phrase and attach the pitch track it was built from.
fn analyse_sung(
    parts: &[(f64, f64, f64)],
    transpose: f64,
    rate: f64,
    gain: f64,
    lead_in: f64,
) -> (Vec<f64>, Features) {
    let mut audio = silence(lead_in);
    audio.extend(phrase(parts, transpose, rate, gain));
    let f = analyse(&audio);
    let frames = f.len();
    let track = pitch_track(parts, transpose, rate, lead_in);
    let f = f.with_pitch(&per_frame(&track, frames));
    (audio, f)
}

fn vocal() -> AlignConfig {
    let mut cfg = AlignConfig::for_material(Material::DoubleTrack);
    cfg.anchors.strength = 1.0;
    cfg
}

// ── the offset stage ──────────────────────────────────────────────────

#[test]
fn a_late_dub_reports_how_late_it_is() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    let mut dub = silence(0.120);
    dub.extend(phrase(LINE, 0.0, 1.0, 1.0));

    let offset = align_dsp::offset::macro_offset(
        &analyse(&reference).envelope(),
        &analyse(&dub).envelope(),
        FPS,
        &reference,
        &dub,
        SR,
        align_dsp::OffsetConfig::default(),
    );
    // Negative: the dub must move *earlier* to land on the reference.
    assert!(
        (offset.seconds + 0.120).abs() < 0.010,
        "expected about -0.120 s, got {:.4} s (score {:.2})",
        offset.seconds,
        offset.score
    );
    assert!(
        offset.score > 0.5,
        "a real match should score: {}",
        offset.score
    );
}

#[test]
fn an_early_dub_reports_a_positive_offset() {
    let mut reference = silence(0.250);
    reference.extend(phrase(LINE, 0.0, 1.0, 1.0));
    let dub = phrase(LINE, 0.0, 1.0, 1.0);

    let offset = align_dsp::offset::macro_offset(
        &analyse(&reference).envelope(),
        &analyse(&dub).envelope(),
        FPS,
        &reference,
        &dub,
        SR,
        align_dsp::OffsetConfig::default(),
    );
    assert!(
        (offset.seconds - 0.250).abs() < 0.010,
        "expected about +0.250 s, got {:.4} s",
        offset.seconds
    );
}

#[test]
fn the_offset_survives_a_level_difference() {
    // Correlation normalized by overlapping energy should not care that
    // one take is 12 dB down; an unnormalized one would.
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    let mut dub = silence(0.180);
    dub.extend(phrase(LINE, 0.0, 1.0, 0.25));

    let offset = align_dsp::offset::macro_offset(
        &analyse(&reference).envelope(),
        &analyse(&dub).envelope(),
        FPS,
        &[],
        &[],
        0.0,
        align_dsp::OffsetConfig::default(),
    );
    assert!(
        (offset.seconds + 0.180).abs() < 0.020,
        "expected about -0.180 s, got {:.4} s",
        offset.seconds
    );
}

// ── the whole engine ──────────────────────────────────────────────────

#[test]
fn an_identical_take_needs_no_correction() {
    let audio = phrase(LINE, 0.0, 1.0, 1.0);
    let f = analyse(&audio);
    let a = align(&f, &f, &vocal()).expect("aligned");

    assert_eq!(a.map.len(), f.len());
    assert!(
        a.max_shift_secs() < 0.015,
        "a take against itself should barely move: {:.4} s",
        a.max_shift_secs()
    );
    assert!(
        a.max_stretch_ratio() < 1.05,
        "and should barely stretch: {:.3}",
        a.max_stretch_ratio()
    );
}

/// The capability the engine did not have before: a dub further out than
/// the warp band is wide. Without a macro stage the true path is not in
/// the search at all, and no amount of tuning recovers it.
#[test]
fn a_dub_further_out_than_the_warp_band_still_aligns() {
    let (reference, r) = analyse_sung(LINE, 0.0, 1.0, 1.0, 0.0);
    let (dub, d) = analyse_sung(LINE, 0.0, 1.0, 1.0, 1.200);

    let cfg = vocal();
    assert!(
        cfg.warp.band_secs < 1.2,
        "the point of this test is that 1.2 s is outside the band"
    );
    let a = align_audio(&r, &reference, &d, &dub, SR, &cfg).expect("aligned");

    assert!(
        (a.offset.seconds + 1.200).abs() < 0.020,
        "offset should find the 1.2 s: {:.4} s",
        a.offset.seconds
    );
    // Every syllable should now land on its reference: sample the middle
    // of the phrase, well past the opening silence.
    for t in [1.4, 1.8, 2.2] {
        assert!(
            (shift_at(&a, t) + 1.200).abs() < 0.030,
            "at {t} s the correction should be about -1.2 s, got {:.4}",
            shift_at(&a, t)
        );
    }
}

#[test]
fn a_dub_that_drifts_is_pulled_back_progressively() {
    let (_, r) = analyse_sung(LINE, 0.0, 1.0, 1.0, 0.0);
    // The same phrase, six percent slower: no offset at the start and a
    // growing error by the end. A constant shift cannot fix this, so it
    // is the case that proves the warp is doing something.
    let (_, d) = analyse_sung(LINE, 0.0, 1.06, 1.0, 0.0);
    let a = align(&r, &d, &vocal()).expect("aligned");

    let early = shift_at(&a, 0.2);
    let late = shift_at(&a, 2.0);
    assert!(
        early.abs() < 0.040,
        "the start is already in place: {early:.4} s"
    );
    assert!(
        late < -0.060,
        "by 2 s the dub is behind and should be pulled back: {late:.4} s"
    );
    assert!(
        late < early,
        "the correction must grow through the take: {early:.4} → {late:.4}"
    );
}

#[test]
fn the_map_never_goes_backwards() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    let dub = phrase(LINE, 2.0, 1.04, 0.6);
    let a = align(&analyse(&reference), &analyse(&dub), &vocal()).expect("aligned");

    for pair in a.map.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "a map that goes backwards renders as a stutter: {pair:?}"
        );
    }
}

// ── anchors ───────────────────────────────────────────────────────────

#[test]
fn the_result_is_a_handful_of_anchors_not_one_per_frame() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    let dub = phrase(LINE, 0.0, 1.04, 1.0);
    let a = align(&analyse(&reference), &analyse(&dub), &vocal()).expect("aligned");

    assert!(
        a.anchors.len() >= 3,
        "six syllables should yield more than the two ends: {}",
        a.anchors.len()
    );
    assert!(
        a.anchors.len() < a.map.len() / 10,
        "{} anchors for {} frames is not a reduction",
        a.anchors.len(),
        a.map.len()
    );
}

#[test]
fn anchors_are_spaced_and_ordered() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    let dub = phrase(LINE, 0.0, 1.05, 1.0);
    let cfg = vocal();
    let a = align(&analyse(&reference), &analyse(&dub), &cfg).expect("aligned");

    for pair in a.anchors.windows(2) {
        assert!(pair[1].dub > pair[0].dub, "anchors must advance: {pair:?}");
        assert!(
            pair[1].reference > pair[0].reference,
            "and must advance on the reference too: {pair:?}"
        );
        let gap = (pair[1].dub - pair[0].dub) as f64 / FPS;
        assert!(
            gap >= cfg.anchors.min_gap_secs - 1e-9,
            "two anchors {gap:.4} s apart describe the same event twice"
        );
    }
}

#[test]
fn no_segment_stretches_further_than_allowed() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    // A wildly wrong pairing — a different phrase entirely — is where an
    // unconstrained matcher produces the 3× compressions that sound like
    // a fault rather than an edit.
    let other: &[(f64, f64, f64)] = &[
        (55.0, 0.10, 0.40),
        (72.0, 0.60, 0.05),
        (58.0, 0.15, 0.30),
        (69.0, 0.50, 0.20),
    ];
    let dub = phrase(other, 0.0, 1.0, 1.0);

    let mut cfg = vocal();
    cfg.anchors.max_stretch_ratio = 1.5;
    let a = align(&analyse(&reference), &analyse(&dub), &cfg).expect("aligned");

    assert!(
        a.max_stretch_ratio() <= 1.5 + 1e-6,
        "ratio {:.3} exceeds the limit that was asked for",
        a.max_stretch_ratio()
    );
}

#[test]
fn strength_scales_the_correction_but_not_the_offset() {
    let (_, r) = analyse_sung(LINE, 0.0, 1.0, 1.0, 0.0);
    let (_, d) = analyse_sung(LINE, 0.0, 1.06, 1.0, 0.0);

    let mut full = vocal();
    full.anchors.strength = 1.0;
    let mut half = vocal();
    half.anchors.strength = 0.5;

    let a = align(&r, &d, &full).expect("aligned");
    let b = align(&r, &d, &half).expect("aligned");

    // Measured against the offset, not against zero. Strength is a
    // judgement about the *performance* — how much of the dub's own
    // phrasing to keep — and where the take sits on the timeline is not
    // a matter of phrasing. So the global offset is applied in full at
    // any strength, and only the warp on top of it is scaled.
    let warp_of = |x: &align_dsp::Alignment| shift_at(x, 2.0) - x.offset.seconds;
    let (full_warp, half_warp) = (warp_of(&a), warp_of(&b));
    assert!(
        full_warp.abs() > 0.02,
        "the fixture must actually need warping, or this proves nothing: {full_warp:.4} s"
    );
    assert!(
        (half_warp - full_warp * 0.5).abs() < 0.1 * full_warp.abs() + 0.003,
        "half strength should warp half as far: {full_warp:.4} → {half_warp:.4}"
    );
}

#[test]
fn a_take_with_no_onsets_still_gets_a_map() {
    // A single sustained tone: no attacks to anchor to after the first.
    // The fallback keeps the shape of the match rather than collapsing
    // the take to one straight line.
    let reference = syllable(60.0, 3.0, 1.0);
    let mut dub = silence(0.080);
    dub.extend(syllable(60.0, 3.0, 1.0));

    let a = align(&analyse(&reference), &analyse(&dub), &vocal()).expect("aligned");
    assert!(a.anchors.len() > 2, "expected fallback anchors");
    assert_eq!(a.map.len(), analyse(&dub).len());
}

// ── presets ───────────────────────────────────────────────────────────

#[test]
fn the_same_source_preset_shifts_without_stretching() {
    let reference = phrase(LINE, 0.0, 1.0, 1.0);
    // The same audio, delayed — two mics on one source, or a copy.
    let mut dub = silence(0.0042);
    dub.extend(phrase(LINE, 0.0, 1.0, 1.0));

    let cfg = AlignConfig::for_material(Material::SameSource);
    let a = align_audio(
        &analyse(&reference),
        &reference,
        &analyse(&dub),
        &dub,
        SR,
        &cfg,
    )
    .expect("aligned");

    assert!(
        (a.max_stretch_ratio() - 1.0).abs() < 1e-6,
        "a rigid shift must not stretch anything: {:.6}",
        a.max_stretch_ratio()
    );
    // Phase-coherent takes: the waveform pass should get this to well
    // inside a millisecond.
    assert!(
        (a.offset.seconds + 0.0042).abs() < 0.0005,
        "expected about -4.2 ms, got {:.5} s",
        a.offset.seconds
    );
}

#[test]
fn a_percussive_pair_aligns_on_its_transients() {
    // Deterministic noise bursts on a pattern: no pitch, no sustain,
    // nothing but onsets — the material the vocal cues say nothing about.
    fn hits(pattern: &[f64], delay: f64) -> Vec<f64> {
        let mut out = vec![0.0; (SR * (delay + 3.0)) as usize];
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for &at in pattern {
            let start = (SR * (delay + at)) as usize;
            for k in 0..(SR * 0.12) as usize {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let noise = (seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
                let decay = (-(k as f64) / (SR * 0.02)).exp();
                if start + k < out.len() {
                    out[start + k] += noise * decay * 0.5;
                }
            }
        }
        out
    }
    let pattern = [0.10, 0.55, 1.02, 1.48, 1.95, 2.40];
    let reference = hits(&pattern, 0.0);
    let dub = hits(&pattern, 0.070);

    let cfg = AlignConfig::for_material(Material::Percussive);
    let a = align_audio(
        &analyse(&reference),
        &reference,
        &analyse(&dub),
        &dub,
        SR,
        &cfg,
    )
    .expect("aligned");

    assert!(
        (a.offset.seconds + 0.070).abs() < 0.010,
        "expected about -70 ms, got {:.4} s",
        a.offset.seconds
    );
    for t in [0.7, 1.6, 2.3] {
        assert!(
            (shift_at(&a, t) + 0.070).abs() < 0.020,
            "at {t} s the hits should be pulled back 70 ms, got {:.4}",
            shift_at(&a, t)
        );
    }
}

// ── features ──────────────────────────────────────────────────────────

#[test]
fn the_bands_separate_low_from_high() {
    let low = syllable(36.0, 1.0, 1.0);
    let high = syllable(84.0, 1.0, 1.0);
    let (l, h) = (analyse(&low), analyse(&high));

    let middle = |f: &Features| f.frames[f.len() / 2];
    let (l, h) = (middle(&l), middle(&h));
    assert!(
        l.bands[0] > l.bands[3],
        "a low note should have more sub than air: {:?}",
        l.bands
    );
    assert!(
        h.bands[2] > h.bands[0],
        "a high note should have more presence than sub: {:?}",
        h.bands
    );
}

#[test]
fn silence_reads_as_silence_and_a_note_does_not() {
    let mut audio = silence(0.5);
    audio.extend(syllable(60.0, 0.5, 1.0));
    let f = analyse(&audio);

    assert!(f.frames[10].silent, "the opening gap is silence");
    assert!(!f.frames[f.len() - 10].silent, "and the note is not");
}

// ── the feature stage ─────────────────────────────────────────────────

/// Deterministic low-level noise, standing in for a room, an amp, or a
/// preamp — anything tracked outside a computer.
fn noise(secs: f64, amplitude: f64) -> Vec<f64> {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    (0..(SR * secs) as usize)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

#[test]
fn a_short_measurement_window_sharpens_the_onset() {
    // The fix this asserts: the window features are measured over is not
    // the caller's analysis window. A pitch detector needs ~46 ms and an
    // onset is over in ten, so measuring at the detector's length smears
    // every attack across four frames and blunts every anchor.
    // The framing the shipping path actually uses: `tune_dsp`'s YIN
    // window is 2048 samples, hop a quarter of that. Measuring at the
    // framing length is what this used to do.
    const YIN_WINDOW: usize = 2048;
    const YIN_HOP: usize = YIN_WINDOW / 4;

    let mut audio = silence(0.5);
    audio.extend(syllable(60.0, 0.5, 1.0));

    // Where the attack really is, in frames: the audio starts at 0.5 s,
    // and a frame's measurement is centred on its framing window.
    let true_frame = (0.5 * SR - (YIN_WINDOW / 2) as f64) / YIN_HOP as f64;

    let flux_peak = |measure_ms: f64| {
        let f = extract(
            &audio,
            SR,
            YIN_HOP,
            YIN_WINDOW,
            features::FeatureConfig {
                measure_ms,
                ..Default::default()
            },
        );
        let peak = f.frames.iter().map(|x| x.flux).fold(0.0_f64, f64::max);
        f.frames.iter().position(|x| x.flux >= peak).unwrap() as f64
    };

    let framing_ms = YIN_WINDOW as f64 * 1000.0 / SR;
    let sharp = (flux_peak(10.0) - true_frame).abs();
    let smeared = (flux_peak(framing_ms) - true_frame).abs();

    assert!(
        sharp < 0.5,
        "a 10 ms window should put the flux peak on the attack: {sharp:.2} frames out"
    );
    // The wide window sees the attack enter one side of itself long
    // before the frame it belongs to, so it fires early — and an anchor
    // is only ever as accurate as the onset that placed it.
    assert!(
        smeared > sharp + 0.5,
        "the {framing_ms:.0} ms framing window should be measurably worse: \
         {smeared:.2} frames out vs {sharp:.2}"
    );
}

#[test]
fn a_take_with_a_real_noise_floor_still_has_silence_in_it() {
    // The gate is measured, not assumed. Anything recorded with a room
    // has a floor well above 60 dB below its own peak, so a fixed
    // threshold never fires and every rule built on silence — pairing
    // gaps at zero cost, discounting the step penalty through them —
    // stops applying on exactly the material it exists for.
    let mut audio = noise(0.5, 0.003);
    audio.extend(
        syllable(60.0, 0.5, 1.0)
            .iter()
            .zip(noise(0.5, 0.003))
            .map(|(s, n)| s + n)
            .collect::<Vec<_>>(),
    );
    let f = analyse(&audio);

    let quiet = f.frames[20];
    let sung = f.frames[f.len() - 20];
    assert!(
        quiet.silent,
        "a gap that is only room noise is still a gap (level {:.3})",
        quiet.level
    );
    assert!(
        !sung.silent,
        "and the note is not (level {:.3})",
        sung.level
    );
}

#[test]
fn the_spectral_shape_ignores_how_loud_the_frame_is() {
    // The same note, the second half 12 dB down. `bands` describes where
    // the energy sits and must not move; `level` describes how much
    // there is and must. Measured against the take's peak instead, all
    // five would move together and the band terms would be four noisy
    // copies of the level term.
    let mut audio = syllable(60.0, 0.6, 1.0);
    audio.extend(syllable(60.0, 0.6, 0.25));
    let f = analyse(&audio);

    let loud = f.frames[(0.3 * FPS) as usize];
    let quiet = f.frames[(0.9 * FPS) as usize];

    assert!(
        loud.level - quiet.level > 0.1,
        "the level should register the 12 dB: {:.3} vs {:.3}",
        loud.level,
        quiet.level
    );
    for k in 0..4 {
        assert!(
            (loud.bands[k] - quiet.bands[k]).abs() < 0.06,
            "band {k} moved with the gain: {:.3} vs {:.3}",
            loud.bands[k],
            quiet.bands[k]
        );
    }
}

#[test]
fn a_take_trimmed_to_its_own_attack_has_an_onset_at_the_first_frame() {
    // How anyone crops an item. Measured against a previous frame that
    // does not exist, the opening attack shows no rise at all, its
    // partner take finds no agreement there, and the whole first phrase
    // interpolates from the take's start instead of being pinned to its
    // downbeat.
    let f = analyse(&syllable(60.0, 1.0, 1.0));
    assert!(
        f.frames[0].delta > 0.5,
        "the take begins with everything arriving at once: {:.3}",
        f.frames[0].delta
    );

    // And a take that opens in silence still reads as quiet, or every
    // alignment would acquire a spurious anchor at frame zero.
    let mut opens_quiet = silence(0.3);
    opens_quiet.extend(syllable(60.0, 1.0, 1.0));
    let g = analyse(&opens_quiet);
    assert!(
        g.frames[0].delta < 0.05,
        "nothing arrives at the start of a silent lead-in: {:.3}",
        g.frames[0].delta
    );
}
