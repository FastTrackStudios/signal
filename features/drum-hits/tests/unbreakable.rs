//! The detector scored against Unbreakable's recorded triggers.
//!
//! Spec: `r[hits.bench]`. Real media, so not part of the ordinary run:
//!
//! ```text
//! cargo test --release -p drum-hits --test unbreakable -- --ignored --nocapture
//! ```
//!
//! `FTS_UNBREAKABLE_MEDIA` points at the song's `Media` folder (default:
//! the fts-real working copy) and `FTS_UNBREAKABLE_TAKE` picks the take
//! (default: the comp's, `260907_1321-01`). Missing media skips.
//!
//! A trigger is the reference here: a contact mic hears its own drum and
//! almost nothing else. What is scored is everything that has to stand
//! in for one — a close mic calibrated against it, and a close mic with
//! no trigger at all — and how long a whole kit takes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use drum_hits::calibrate::TOLERANCE;
use drum_hits::{Health, Method, Piece, Settings, Trust, agree, detect_kit, samples, spikes};
use rayon::prelude::*;

/// Each piece's close mic and trigger, by file prefix.
const PIECES: [(&str, &str, &str); 6] = [
    ("Kick", "65-In", "67-Trig"),
    ("Snare", "69-Top", "71-Trig"),
    ("T1", "73-T1", "78-T1 Trig"),
    ("T2", "74-T2", "79-T2 Trig"),
    ("T3", "75-T3", "80-T3 Trig"),
    ("T4", "76-T4", "81-T4 Trig"),
];
/// Close mics with no piece of their own: bleed to refuse.
const EXTRA: [&str; 2] = ["84-Hi Hat", "85-Ride"];

fn media() -> PathBuf {
    std::env::var_os("FTS_UNBREAKABLE_MEDIA").map_or_else(
        || PathBuf::from("/run/media/Development/fts-real/crescendum-2026-10-08/unbreakable/Media"),
        PathBuf::from,
    )
}

fn take() -> String {
    std::env::var("FTS_UNBREAKABLE_TAKE").unwrap_or_else(|_| "260907_1321-01".to_owned())
}

/// A file's samples, its channels summed, and its rate.
fn load(dir: &Path, prefix: &str, take: &str) -> Option<(Vec<f32>, f64)> {
    let path = dir.join(format!("{prefix}-{take}.wav"));
    let mut reader = hound::WavReader::open(&path).ok()?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = f32::from(2_u16).powi(i32::from(spec.bits_per_sample.saturating_sub(1)));
            reader
                .samples::<i32>()
                .map(|s| num_traits::ToPrimitive::to_f32(&s.ok()?).map(|v| v / scale))
                .collect::<Option<_>>()?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(Result::ok)
            .collect::<Option<_>>()?,
    };
    let mono = raw
        .chunks(channels)
        .map(|c| c.iter().sum::<f32>())
        .collect();
    Some((mono, f64::from(spec.sample_rate)))
}

/// Each piece's line: what it found, and how that compares with its
/// trigger. For a fused piece, also where its attacks sit against the
/// trigger — asserted to be within 3 ms.
fn report(piece: &str, f: &drum_hits::Found, truth: &[usize], rate: f64) {
    let hits: Vec<usize> = f.hits.iter().map(|h| h.at).collect();
    let score = agree(&hits, truth, samples(TOLERANCE, rate));
    let calibration = f.calibration.map_or_else(String::new, |c| {
        format!(
            "mic at {:.0} dBFS / {:.0} dB agrees F {:.3}",
            c.gate.threshold_db,
            c.gate.crest_db,
            c.agreement.f1()
        )
    });
    let count = |keep: fn(&drum_hits::Hit) -> bool| f.hits.iter().filter(|h| keep(h)).count();
    println!(
        "{piece:<6} {:?} {:>5} hits (both {:>4}, trigger only {:>3}, mic only {:>3}, ghosts {:>3}, flams {:>3})  vs trigger F {:.3}  {calibration}  {:?}",
        f.method,
        f.hits.len(),
        count(|h| h.from_trigger && h.from_mic),
        count(|h| h.from_trigger && !h.from_mic),
        count(|h| !h.from_trigger && h.from_mic),
        count(|h| h.ghost),
        count(|h| h.flam_with.is_some()),
        score.f1(),
        f.trigger,
    );
    if f.method != Method::Fused {
        return;
    }
    // The fused hit is the mic's attack start: a little ahead of the
    // trigger's threshold crossing, never far behind it.
    let mut offsets: Vec<f64> = f
        .hits
        .iter()
        .filter(|h| h.from_trigger && h.from_mic)
        .filter_map(|h| {
            truth
                .iter()
                .min_by_key(|t| t.abs_diff(h.at))
                .map(|&t| (h.at, t))
        })
        .map(|(a, t)| (drum_hits::seconds(a, rate) - drum_hits::seconds(t, rate)) * 1e3)
        .collect();
    offsets.sort_by(f64::total_cmp);
    let at = |q: usize| {
        offsets
            .get(offsets.len().saturating_mul(q) / 20)
            .copied()
            .unwrap_or(f64::NAN)
    };
    println!(
        "         attack vs trigger {:+.2} ms (5–95%: {:+.2} … {:+.2})",
        at(10),
        at(1),
        at(19)
    );
    assert!(at(10).abs() < 3.0, "{piece}: attacks {:+.2} ms off", at(10));
}

#[test]
#[ignore = "reads Unbreakable's media; run with --ignored"]
fn against_the_triggers() {
    let dir = media();
    if !dir.is_dir() {
        eprintln!("no media at {}: skipped", dir.display());
        return;
    }
    let take = take();

    let started = Instant::now();
    let files: Vec<&str> = PIECES
        .iter()
        .flat_map(|p| [p.1, p.2])
        .chain(EXTRA)
        .collect();
    let loaded: Vec<(Vec<f32>, f64)> = files
        .par_iter()
        .map(|f| load(&dir, f, &take).unwrap())
        .collect();
    let rate = loaded[0].1;
    println!(
        "take {take}, {rate} Hz, {:.0} s: {} files read in {:.2} s",
        drum_hits::seconds(loaded[0].0.len(), rate),
        files.len(),
        started.elapsed().as_secs_f64()
    );
    let signal = |i: usize| loaded[i].0.as_slice();
    let pieces: Vec<Piece<'_>> = (0..PIECES.len())
        .map(|i| Piece {
            mic: signal(2 * i),
            trigger: Some(signal(2 * i + 1)),
        })
        .collect();
    let extra: Vec<&[f32]> = (0..EXTRA.len())
        .map(|i| signal(2 * PIECES.len() + i))
        .collect();
    let truths: Vec<Vec<usize>> = (0..PIECES.len())
        .map(|i| spikes(signal(2 * i + 1), rate, 24.0, 0.04))
        .collect();
    // The kit as it will be detected: triggers where they work.
    let started = Instant::now();
    let found = detect_kit(&pieces, &extra, rate, &Settings::default());
    let took = started.elapsed().as_secs_f64();
    println!("detected the kit in {took:.3} s");

    for ((piece, _, _), (f, truth)) in PIECES.iter().zip(found.iter().zip(&truths)) {
        report(piece, f, truth, rate);
    }

    one_trigger_lost(&pieces, &extra, &truths, rate);

    // Every piece from its mic alone: what a kit with no triggers gets.
    let started = Instant::now();
    let alone = detect_kit(
        &pieces,
        &extra,
        rate,
        &Settings {
            trust: Trust::Mic,
            ..Settings::default()
        },
    );
    println!(
        "\nwithout triggers ({:.3} s):",
        started.elapsed().as_secs_f64()
    );
    for ((piece, _, _), (f, truth)) in PIECES.iter().zip(alone.iter().zip(&truths)) {
        let hits: Vec<usize> = f.hits.iter().map(|h| h.at).collect();
        let s = agree(&hits, truth, samples(TOLERANCE, rate));
        println!(
            "{piece:<6} {:>5} hits  P {:.3} R {:.3} F {:.3}",
            hits.len(),
            s.precision(),
            s.recall(),
            s.f1()
        );
    }

    for ((piece, _, _), f) in PIECES.iter().zip(&found) {
        if *piece == "T4" {
            // Unbreakable's floor-tom trigger is dead: it must be caught.
            assert!(
                matches!(f.trigger, Some(Health::Dead(_))),
                "T4's dead trigger was believed"
            );
            assert!(!f.hits.is_empty(), "T4 got no hits from its mic");
        } else {
            assert_eq!(
                f.trigger,
                Some(Health::Working),
                "{piece}'s trigger was refused"
            );
            let f1 = f.calibration.map_or(0.0, |c| c.agreement.f1());
            assert!(f1 >= 0.9, "{piece}: calibrated F1 {f1:.3}");
        }
    }
    if !cfg!(debug_assertions) {
        assert!(took < 1.0, "the kit took {took:.3} s");
    }
}

/// One piece's trigger lost, the rest working — Unbreakable's floor tom —
/// played on each tom whose trigger can score it.
fn one_trigger_lost(pieces: &[Piece<'_>], extra: &[&[f32]], truths: &[Vec<usize>], rate: f64) {
    println!("\none trigger lost at a time:");
    for (lost, (piece, _, _)) in PIECES
        .iter()
        .enumerate()
        .filter(|(_, p)| p.0.starts_with('T') && p.0 != "T4")
    {
        let mut kit = pieces.to_vec();
        if let Some(p) = kit.get_mut(lost) {
            p.trigger = None;
        }
        let found = detect_kit(&kit, extra, rate, &Settings::default());
        let hits: Vec<usize> = found
            .get(lost)
            .map(|f| f.hits.iter().map(|h| h.at).collect())
            .unwrap_or_default();
        let s = agree(
            &hits,
            truths.get(lost).map_or(&[][..], Vec::as_slice),
            samples(TOLERANCE, rate),
        );
        println!(
            "{piece:<6} {:>5} hits  P {:.3} R {:.3} F {:.3}",
            hits.len(),
            s.precision(),
            s.recall(),
            s.f1()
        );
    }
}
