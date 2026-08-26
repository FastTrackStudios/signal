//! Self-verifying pitch checks for the CSS legato mapping.
//!
//! Renders three legato documents through the Cinematic Studio Strings
//! 1st Violins patch (C major scale, melody set, chromatic walkup), then
//! pitch-tracks each note segment of the OUTPUT audio (autocorrelation,
//! median over several windows inside the note's slot) and asserts the
//! detected fundamental matches the document note within ±30 cents.
//!
//! This regression-tests the whole chain: zone generation (interval-suffix
//! naming, sounding-pitch roots), transition selection (direction / named
//! lower note / interval), destination re-tuning, and measured-lead-in
//! prefire alignment (the destination must SPEAK on its tick, so windows
//! inside the slot read the note, not the neighbour).
//!
//! Skips (with a note on stderr) when the CSS sample library is not
//! present on this machine.

use std::path::PathBuf;

use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const SR: u32 = 48_000;
const BPM: f64 = 90.0;
const OVERLAP_QN: f64 = 1.0 / 32.0;
/// Max allowed |detected − expected| in cents, per note (median detector).
const TOLERANCE_CENTS: f64 = 30.0;

// ── pitch detection ──────────────────────────────────────────────────────────

/// Autocorrelation fundamental (MIDI float) of `win` mono frames at `start`.
fn detect_midi(mono: &[f32], sr: u32, start: usize, win: usize) -> Option<f64> {
    let seg = mono.get(start..start + win)?;
    let mean = seg.iter().sum::<f32>() / win as f32;
    const STRIDE: usize = 2;
    let r0: f32 = seg
        .iter()
        .step_by(STRIDE)
        .map(|v| (v - mean) * (v - mean))
        .sum();
    if r0 < 1e-9 {
        return None;
    }
    let lag_min = (sr as f32 / 2200.0) as usize;
    let lag_max = ((sr as f32 / 55.0) as usize).min(win - 2);
    let mut best_lag = 0usize;
    let mut best = 0.0f32;
    for lag in lag_min..=lag_max {
        let mut acc = 0.0f32;
        let mut i = 0;
        while i + lag < win {
            acc += (seg[i] - mean) * (seg[i + lag] - mean);
            i += STRIDE;
        }
        if acc / r0 > best {
            best = acc / r0;
            best_lag = lag;
        }
    }
    if best < 0.4 || best_lag == 0 {
        return None;
    }
    let hz = sr as f64 / best_lag as f64;
    Some(69.0 + 12.0 * (hz / 440.0).log2())
}

/// Median detected pitch across windows spread over [10%, 70%] of the slot —
/// robust against the crossfade tails at the slot edges and the next
/// transition's glide at its end.
fn note_pitch(mono: &[f32], sr: u32, t0_sec: f64, dur_sec: f64) -> Option<f64> {
    let mut ests: Vec<f64> = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]
        .iter()
        .filter_map(|f| {
            let start = ((t0_sec + f * dur_sec) * sr as f64) as usize;
            detect_midi(mono, sr, start, 3072)
        })
        .collect();
    if ests.is_empty() {
        return None;
    }
    ests.sort_by(|a, b| a.total_cmp(b));
    Some(ests[ests.len() / 2])
}

// ── document builders (mirrors examples/render_pitch_checks.rs) ─────────────

/// `(pitch, start_qn, dur_qn)` triples → a mono legato line document.
fn doc_from_steps(steps: &[(u8, f64, f64)]) -> TrackDocument {
    let notes = steps
        .iter()
        .enumerate()
        .map(|(i, &(pitch, start, dur))| DocNote {
            start_qn: start,
            end_qn: start
                + dur
                + if i + 1 == steps.len() {
                    0.0
                } else {
                    OVERLAP_QN
                },
            chan: 0,
            pitch,
            vel: 80,
        })
        .collect();
    TrackDocument {
        version: 1,
        seed: 0x0C0F_FEE0_5EED_0001,
        auto_divisi: false,
        ccs: vec![DocCc {
            qn: 0.0,
            chan: 0,
            cc: 1,
            val: 84,
        }],
        notes,
        tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
    }
}

fn run_case(rig: &SamplerRig, name: &str, steps: &[(u8, f64, f64)]) -> Vec<String> {
    let doc = doc_from_steps(steps);
    let res = rig
        .render_offline_document("strings_1v", &doc, &DocumentRenderOptions::default())
        .expect("document render");
    assert_eq!(
        res.reactive_fallbacks, 0,
        "{name}: document render must never fall back to the reactive path"
    );
    // Fold to mono for the tracker.
    let mono: Vec<f32> = res
        .audio
        .chunks_exact(2)
        .map(|f| (f[0] + f[1]) * 0.5)
        .collect();
    let qn_sec = 60.0 / BPM;
    let mut failures = Vec::new();
    for &(pitch, start, dur) in steps {
        match note_pitch(&mono, SR, start * qn_sec, dur * qn_sec) {
            Some(m) => {
                let cents = (m - pitch as f64) * 100.0;
                if cents.abs() > TOLERANCE_CENTS {
                    failures.push(format!(
                        "{name}: note {pitch} @ {start} QN detected {m:.2} ({cents:+.0} cents)"
                    ));
                }
            }
            None => failures.push(format!(
                "{name}: note {pitch} @ {start} QN — no confident pitch detected"
            )),
        }
    }
    failures
}

fn steps_seq(pitches: &[u8], start: f64, dur: f64, last_dur: f64) -> Vec<(u8, f64, f64)> {
    let mut qn = start;
    let n = pitches.len();
    pitches
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let d = if i + 1 == n { last_dur } else { dur };
            let s = (p, qn, d);
            qn += dur;
            s
        })
        .collect()
}

#[test]
fn css_legato_renders_are_in_tune() {
    let css_root = PathBuf::from(CSS_ROOT);
    let zones = css_root.join("_patches/1st Violins/library.styx");
    if !zones.exists() || !PathBuf::from(CSS_CONFIG).exists() {
        eprintln!("SKIP css_legato_renders_are_in_tune: CSS samples not present");
        return;
    }
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig.load_instrument_with_config(
        "strings_1v",
        &PathBuf::from(CSS_CONFIG),
        &zones,
        &css_root,
        "1st Violins",
        "Mix",
    )
    .expect("load CSS 1st Violins");
    rig.set_solo_mic("strings_1v", Some("Mix".into()));
    rig.set_attack_ms("strings_1v", 20);
    rig.set_release_ms("strings_1v", 400);

    let mut failures = Vec::new();

    // 1. C major scale up + down (whole/half-step transitions, both directions).
    let mut scale = steps_seq(&[60, 62, 64, 65, 67, 69, 71, 72], 1.0, 1.0, 1.0);
    scale.extend(steps_seq(&[72, 71, 69, 67, 65, 64, 62, 60], 10.0, 1.0, 4.0));
    failures.extend(run_case(&rig, "cmajor_scale", &scale));

    // 2. Melody set: repeated notes (re-bow), wide leaps, accidentals.
    let mut mel = steps_seq(
        &[64, 64, 65, 67, 67, 65, 64, 62, 60, 60, 62, 64],
        1.0,
        1.0,
        1.0,
    );
    mel.push((64, 13.0, 1.5));
    mel.push((62, 14.5, 0.5));
    mel.push((62, 15.0, 3.0));
    let arp = steps_seq(&[60, 64, 67, 72, 76, 72, 67, 64, 60], 20.0, 1.0, 4.0);
    let dmin = steps_seq(&[62, 64, 65, 67, 69, 70, 73, 74], 34.0, 1.0, 4.0);
    mel.extend(arp);
    mel.extend(dmin);
    failures.extend(run_case(&rig, "melodies", &mel));

    // 3. Chromatic walkup C4→C5 — every pitch class, semitone transitions.
    let chroma: Vec<(u8, f64, f64)> = (0..13)
        .map(|i| {
            (
                60 + i as u8,
                1.0 + 0.5 * i as f64,
                if i == 12 { 4.0 } else { 0.5 },
            )
        })
        .collect();
    failures.extend(run_case(&rig, "chromatic", &chroma));

    assert!(
        failures.is_empty(),
        "pitch verification failed for {} note(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
