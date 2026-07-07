//! Render trace dump — the ground-truth timeline of what the engine played.
//!
//! Renders a MIDI pattern through CSS Violin 1 with the structured render trace
//! enabled, then prints, in time order: every voice spawn (which sample file,
//! note, gain, pitch, loop window), every legato transition, and every
//! note-off. Finally it flags DOUBLING — two voices of the same note spawned
//! within a few ms (what "doubles at the top" looks like in the trace).
//!
//! ```text
//! cargo run --release -p signal-sampler --example trace_dump           # chromatic up/down
//! cargo run --release -p signal-sampler --example trace_dump -- scale  # C major up/down
//! ```

use std::path::PathBuf;

use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::{SamplerRig, TraceKind};

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const SEED: u64 = 0x77ACE_D00D;
const OVERLAP_QN: f64 = 1.0 / 32.0;

const NN: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn nm(p: u8) -> String {
    format!("{}{}", NN[(p % 12) as usize], (p / 12) as i32 - 1)
}

fn line_notes(steps: &[(u8, f64)]) -> Vec<DocNote> {
    let mut out = Vec::new();
    let mut qn = 1.0;
    for &(pitch, dur) in steps {
        out.push(DocNote {
            start_qn: qn,
            end_qn: qn + dur + OVERLAP_QN,
            chan: 0,
            pitch,
            vel: 80,
        });
        qn += dur;
    }
    if let Some(last) = out.last_mut() {
        last.end_qn -= OVERLAP_QN;
    }
    out
}

fn main() -> eyre::Result<()> {
    let which = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "chromatic".into());
    let (bpm, steps): (f64, Vec<(u8, f64)>) = if which == "scale" {
        let mut s: Vec<(u8, f64)> = [60u8, 62, 64, 65, 67, 69, 71, 72]
            .iter()
            .map(|&p| (p, 1.0))
            .collect();
        s.extend([71u8, 69, 67, 65, 64, 62, 60].iter().map(|&p| (p, 1.0)));
        (90.0, s)
    } else {
        let mut c: Vec<(u8, f64)> = (60u8..=72).map(|p| (p, 0.5)).collect();
        c.extend((60u8..=71).rev().map(|p| (p, 0.5)));
        (96.0, c)
    };

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

    let notes = line_notes(&steps);
    let doc = TrackDocument {
        version: 1,
        seed: SEED,
        auto_divisi: false,
        ccs: vec![
            DocCc {
                qn: 0.0,
                chan: 0,
                cc: 1,
                val: 84,
            },
            DocCc {
                qn: 0.0,
                chan: 0,
                cc: 2,
                val: 0,
            },
        ],
        notes,
        tempo: vec![TempoPoint { qn: 0.0, bpm }],
    };

    // Enable the trace, render, read it back.
    rig.set_trace_enabled(ID, true);
    let _res = rig.render_offline_document(ID, &doc, &DocumentRenderOptions::default())?;
    let trace = rig.render_trace(ID);

    let t = |f: u64| f as f64 / SR as f64;
    println!("── {which} trace: {} events ──\n", trace.events.len());
    for e in &trace.events {
        match &e.kind {
            TraceKind::VoiceSpawn(v) => {
                let file = v.file.rsplit('/').next().unwrap_or(&v.file);
                let looped = if v.loops() {
                    format!("loop {}..{} xf{}", v.loop_start, v.loop_end, v.loop_xfade)
                } else {
                    "no-loop".into()
                };
                println!(
                    "{:8.3}s  L{}  SPAWN  {:<12} {:<4} gain={:.3} rate={:.3} {:<22} {}",
                    t(e.frame),
                    e.line,
                    v.voice_kind,
                    nm(v.note),
                    v.gain,
                    v.rate,
                    file,
                    looped,
                );
            }
            TraceKind::Transition {
                from,
                to,
                portamento,
            } => {
                println!(
                    "{:8.3}s  L{}  TRANS  {} → {}{}",
                    t(e.frame),
                    e.line,
                    nm(*from),
                    nm(*to),
                    if *portamento { " (porta)" } else { "" }
                );
            }
            TraceKind::NoteOff { note } => {
                println!("{:8.3}s  L{}  OFF    {}", t(e.frame), e.line, nm(*note));
            }
        }
    }

    // Doubling probe: two AUDIBLE spawns of the same note within 60 ms. The
    // multi-layer sustain spawns all dynamic layers at once (most at gain 0 —
    // held silently so CC1 can swell them live); those are not doubling, so
    // ignore near-silent voices.
    println!("\n── doubling probe (audible same-note spawns within 60 ms) ──");
    let spawns: Vec<(u64, u8, &str)> = trace
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            TraceKind::VoiceSpawn(v) if v.gain > 0.02 => Some((e.frame, v.note, v.voice_kind)),
            _ => None,
        })
        .collect();
    let win = (0.060 * SR as f64) as u64;
    let mut flagged = 0;
    for i in 0..spawns.len() {
        for j in (i + 1)..spawns.len() {
            let (fi, ni, ki) = spawns[i];
            let (fj, nj, kj) = spawns[j];
            if nj != ni {
                continue;
            }
            if fj.saturating_sub(fi) > win {
                break;
            }
            // Two sounding voices of the same note = doubling (ignore a Release
            // tail stacking on its own note — that is expected on note-off).
            if ki != "Release" && kj != "Release" {
                println!(
                    "  DOUBLING {} : {:.3}s [{}] + {:.3}s [{}]  (Δ{:.0}ms)",
                    nm(ni),
                    t(fi),
                    ki,
                    t(fj),
                    kj,
                    (fj - fi) as f64 / SR as f64 * 1000.0,
                );
                flagged += 1;
            }
        }
    }
    if flagged == 0 {
        println!("  none.");
    }
    Ok(())
}
