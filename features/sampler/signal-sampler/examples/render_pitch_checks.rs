//! Pitch-class verification renders (document mode).
//!
//! Three hand-built documents through the CSS Violin 1 patch, all legato:
//! 1. C major scale, up and down — diatonic pitch classes
//! 2. Melody lines — repeated notes (re-bow), leaps, accidentals
//! 3. Chromatic walkup C4→C5 — every pitch class + semitone transitions
//!
//! ```text
//! cargo run --release -p signal-sampler --example render_pitch_checks
//! ```

use std::path::PathBuf;

use signal_sampler::SamplerRig;
use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const SEED: u64 = 0x0C0F_FEE0_5EED_0001;
const BPM: f64 = 90.0;
/// Legato overlap between consecutive notes (QN) — gives the annotator its
/// different-pitch-overlap edge, same convention keyflow's stage-1 emits.
const OVERLAP_QN: f64 = 1.0 / 32.0;

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn name(p: u8) -> String {
    format!("{}{}", NOTE_NAMES[(p % 12) as usize], (p / 12) as i32 - 1)
}

/// Build a mono legato line from (pitch, duration-QN) pairs; a pitch of 0
/// inserts a rest (phrase break) of that duration.
fn legato_line(steps: &[(u8, f64)], vel: u8) -> Vec<DocNote> {
    let mut out = Vec::new();
    let mut qn = 1.0; // one-beat lead-in
    for &(pitch, dur) in steps {
        if pitch == 0 {
            qn += dur;
            continue;
        }
        out.push(DocNote {
            start_qn: qn,
            end_qn: qn + dur + OVERLAP_QN,
            chan: 0,
            pitch,
            vel,
        });
        qn += dur;
    }
    // No overlap past the final note-off.
    if let Some(last) = out.last_mut() {
        last.end_qn -= OVERLAP_QN;
    }
    out
}

fn write_wav(path: &std::path::Path, samples: &[f32]) -> eyre::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> eyre::Result<()> {
    let css_root = PathBuf::from(CSS_ROOT);
    let zones = css_root.join("_patches/1st Violins/library.styx");
    if !zones.exists() {
        eyre::bail!("CSS Violin 1 patch not found at {}", zones.display());
    }
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig.load_instrument_with_config(
        ID,
        &PathBuf::from(CSS_CONFIG),
        &zones,
        &css_root,
        "1st Violins",
        "Mix",
    )?;
    rig.set_solo_mic(ID, Some("Mix".into()));
    rig.set_attack_ms(ID, 20);
    rig.set_release_ms(ID, 400);

    // ── 1. C major scale, quarter notes, up + down ────────────────────────
    let q = 1.0;
    let up: Vec<(u8, f64)> = [60u8, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .map(|&p| (p, q))
        .collect();
    let mut scale = up.clone();
    scale.push((0, q)); // breath
    scale.extend([72u8, 71, 69, 67, 65, 64, 62, 60].iter().map(|&p| (p, q)));
    // hold the final tonic
    scale.last_mut().unwrap().1 = 4.0;

    // ── 2. Melody lines ───────────────────────────────────────────────────
    // a) Ode to Joy opening — stepwise + repeated notes (re-bow test)
    let ode: Vec<(u8, f64)> = [
        (64u8, q),
        (64, q),
        (65, q),
        (67, q),
        (67, q),
        (65, q),
        (64, q),
        (62, q),
        (60, q),
        (60, q),
        (62, q),
        (64, q),
        (64, 1.5),
        (62, 0.5),
        (62, 3.0),
    ]
    .to_vec();
    // b) arpeggio with wide leaps — exercises directional transitions + leaps
    let arp: Vec<(u8, f64)> = [
        (60u8, q),
        (64, q),
        (67, q),
        (72, q),
        (76, q),
        (72, q),
        (67, q),
        (64, q),
        (60, 4.0),
    ]
    .to_vec();
    // c) D harmonic minor phrase — accidentals (Bb, C#)
    let dmin: Vec<(u8, f64)> = [
        (62u8, q),
        (64, q),
        (65, q),
        (67, q),
        (69, q),
        (70, q),
        (73, q),
        (74, 4.0),
    ]
    .to_vec();
    let mut melodies = ode;
    melodies.push((0, 2.0));
    melodies.extend(arp);
    melodies.push((0, 2.0));
    melodies.extend(dmin);

    // ── 3. Chromatic walkup, eighth notes, C4→C5, hold the octave ────────
    let mut chroma: Vec<(u8, f64)> = (60u8..=72).map(|p| (p, 0.5)).collect();
    chroma.last_mut().unwrap().1 = 4.0;

    let renders: [(&str, Vec<(u8, f64)>); 3] = [
        ("pitch_check_cmajor_scale", scale),
        ("pitch_check_melodies", melodies),
        ("pitch_check_chromatic", chroma),
    ];

    let opts = DocumentRenderOptions::default();
    for (stem, steps) in renders {
        let notes = legato_line(&steps, 80);
        println!("── {stem}: {} notes", notes.len());
        for n in &notes {
            println!(
                "    {:>4}  {:6.2} → {:6.2} QN",
                name(n.pitch),
                n.start_qn,
                n.end_qn
            );
        }
        let document = TrackDocument {
            version: 1,
            seed: SEED,
            auto_divisi: false,
            // Medium dynamics so the CC1 layer blend is audible.
            ccs: vec![DocCc {
                qn: 0.0,
                chan: 0,
                cc: 1,
                val: 84,
            }],
            notes,
            tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
        };
        let res = rig.render_offline_document(ID, &document, &opts)?;
        if res.reactive_fallbacks != 0 {
            eyre::bail!("{stem}: {} reactive fallbacks", res.reactive_fallbacks);
        }
        let out = PathBuf::from(format!("target/{stem}.wav"));
        write_wav(&out, &res.audio)?;
        println!(
            "    → {} ({} transitions, {:.1}s)",
            out.display(),
            res.transitions.len(),
            res.audio.len() as f64 / 2.0 / SR as f64
        );
    }
    Ok(())
}
