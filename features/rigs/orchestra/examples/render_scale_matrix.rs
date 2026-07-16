//! Scale-start diagnostic matrix (owner round-8): the SAME up-down legato
//! line transposed to start on G4, C4, D4, E4 — rendered in the chosen
//! `kontakt` arrival-semantics lane with click. If the same LINE POSITIONS
//! go early regardless of start pitch → systematic (engine/logic). If the
//! same ABSOLUTE PITCHES / zones go early across starts → sample-specific
//! (pack markers for those zones).
//!
//! Prints a per-join table: position, from→to, direction, fired zone info
//! (from the LegatoFireEvent), predicted arrival err vs grid, and — the
//! ground truth — the EMITTED arrival marker err vs grid.
//!
//!   cargo run --release -p signal-orchestra --example render_scale_matrix -- listen/scale-matrix

use std::io::Write as _;
use std::path::PathBuf;

use signal_orchestra::timing::{mix_click, render_click, CountIn, COUNT_IN_QN};
use signal_orchestra::{load_strings, CSS_CONFIG, CSS_ROOT};
use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::SamplerRig;

const SR: u32 = 48_000;
const ID: &str = "strings_1v";
const SEED: u64 = 0x71D1_1E6A_70C0_2B05;
const BPM: f64 = 90.0;
const KS_EXPR: u8 = 8;
const VEL: u8 = 85; // expressive medium lane — where the owner heard it

/// Up-down major-shape line, same intervals as the original G scale:
/// +0 +2 +4 +5 +7 +5 +4 +2 +0 (do re mi fa sol fa mi re do — ends on the root).
const PATTERN: [i16; 9] = [0, 2, 4, 5, 7, 5, 4, 2, 0];

fn scale_doc(start: u8) -> TrackDocument {
    let mut notes = Vec::new();
    for (i, off) in PATTERN.iter().enumerate() {
        let s = COUNT_IN_QN + i as f64;
        let e = s + if i + 1 < PATTERN.len() { 1.02 } else { 1.0 };
        notes.push(DocNote {
            start_qn: s,
            end_qn: e,
            chan: 0,
            pitch: (i16::from(start) + off) as u8,
            vel: VEL,
        });
    }
    TrackDocument {
        version: 1,
        seed: SEED,
        auto_divisi: false,
        notes,
        ccs: vec![
            DocCc { qn: 0.0, chan: 0, cc: 58, val: KS_EXPR },
            DocCc { qn: 0.0, chan: 0, cc: 1, val: 90 },
        ],
        tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
    }
}

fn qn_frame(qn: f64) -> i64 {
    (qn * 60.0 / BPM * f64::from(SR)).round() as i64
}

fn write_wav(path: &std::path::Path, samples: &[f32]) -> eyre::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> eyre::Result<()> {
    // The owner's chosen lane.
    std::env::set_var("SIGNAL_ARRIVAL_SEMANTICS", "kontakt");

    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "listen/scale-matrix".into()),
    );
    std::fs::create_dir_all(&out)?;
    let mut table = std::fs::File::create(out.join("join-table.txt"))?;

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG).map_err(|e| eyre::eyre!(e))?;

    let names = ["C4","Db4","D4","Eb4","E4","F4","Gb4","G4","Ab4","A4","Bb4","B4"];
    let mut jobs: Vec<(String, TrackDocument)> = (0u8..12)
        .map(|i| (format!("scale_{}", names[i as usize]), scale_doc(60 + i)))
        .collect();
    // Chromatic runs: 13 quarter-note semitones up, from G4 and from C4.
    for (start, nm) in [(67u8, "G4"), (60, "C4")] {
        let mut notes = Vec::new();
        for k in 0..13u8 {
            let s = COUNT_IN_QN + f64::from(k);
            let e = s + if k < 12 { 1.02 } else { 1.0 };
            notes.push(DocNote { start_qn: s, end_qn: e, chan: 0, pitch: start + k, vel: VEL });
        }
        jobs.push((format!("chromatic_{nm}"), TrackDocument {
            version: 1, seed: SEED, auto_divisi: false, notes,
            ccs: vec![
                DocCc { qn: 0.0, chan: 0, cc: 58, val: KS_EXPR },
                DocCc { qn: 0.0, chan: 0, cc: 1, val: 90 },
            ],
            tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
        }));
    }
    for (name, doc) in jobs {
        let res = rig
            .render_offline_document(ID, &doc, &DocumentRenderOptions::default())
            .map_err(|e| eyre::eyre!("{name}: {e}"))?;

        let hdr = format!(
            "── {name} (vel {VEL}, {BPM} bpm, kontakt lane): {} transitions, {} fallbacks",
            res.transitions.len(),
            res.reactive_fallbacks
        );
        println!("{hdr}");
        writeln!(table, "{hdr}")?;

        // Per-join: position in line, from→to, predicted vs grid, EMITTED vs grid.
        for (i, t) in res.transitions.iter().enumerate() {
            let join_qn = COUNT_IN_QN + (i + 1) as f64;
            let grid = qn_frame(join_qn);
            let dir = if t.to_note > t.from_note { "up  " } else { "down" };
            // Emitted arrival for this destination note (first crossing on the line).
            let emitted = res
                .emitted_markers
                .iter()
                .find(|m| m.note == t.to_note && (m.frame as i64 - grid).abs() < (SR as i64 / 2))
                .map(|m| format!("{:+7.1}ms", (m.frame as i64 - grid) as f64 * 1000.0 / f64::from(SR)))
                .unwrap_or_else(|| "  (none)".into());
            let line = format!(
                "  pos {} {:>3}→{:<3} {dir} predicted {:+7.1}ms emitted {emitted}",
                i + 2,
                t.from_note,
                t.to_note,
                (t.arrival as i64 - grid) as f64 * 1000.0 / f64::from(SR),
            );
            println!("{line}");
            writeln!(table, "{line}")?;
        }

        let frames = res.audio.len() / 2;
        let click = render_click(
            &doc.tempo,
            frames,
            SR,
            Some(CountIn { start_qn: 0.0, beats: 4 }),
        );
        let mixed = mix_click(&res.audio, &click, 0.5, 0.7);
        write_wav(&out.join(format!("{name}_kontakt_click.wav")), &mixed)?;
        write_wav(&out.join(format!("{name}_kontakt_dry.wav")), &res.audio)?;
    }
    println!("wrote {} + join-table.txt", out.display());
    Ok(())
}
