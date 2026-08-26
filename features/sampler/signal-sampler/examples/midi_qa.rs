//! MIDI QA harness — extensive note-transition testing for the CSS engine.
//!
//! Renders a battery of MIDI patterns through the CSS Violin 1 patch, then
//! MARKS every boundary where the engine changes voices (note-on/attack, each
//! legato transition arrival, note-off/release) and CHECKS the rendered audio
//! at each mark for artifacts:
//!   * click / pop  — an abrupt inter-sample step far above the waveform slope
//!   * gap          — an unexpected dropout to near-silence mid-phrase
//!   * jump         — a sudden level change across a legato move (dynamics
//!     are continuous, so an arrival should not step in level)
//!   * doubling     — level ~2× after a move = two voices of the note stacking
//!
//! For each pattern it writes `target/qa_<name>.wav` and an Audacity label
//! track `target/qa_<name>.labels.txt` (import over the WAV: File ▸ Import ▸
//! Labels) so every checked boundary — and any failure — is visible on the
//! waveform. Exits non-zero if any boundary fails.
//!
//! ```text
//! cargo run --release -p signal-sampler --example midi_qa
//! ```

use std::path::PathBuf;

use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str = "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const SEED: u64 = 0x000D_A11A_5EED_0001;
const OVERLAP_QN: f64 = 1.0 / 32.0;

// ── Boundary-check thresholds ───────────────────────────────────────────────
/// Inter-sample step above this = a click/pop (the CSS tone's own max slope is
/// ~0.02; a real click steps 0.1+).
const CLICK: f32 = 0.06;
/// Mid-move RMS below this fraction of the surrounding level = a gap/dropout.
const GAP_FRAC: f32 = 0.15;
/// |post-pre|/max level above this across a legato arrival = a sudden jump.
const JUMP_FRAC: f32 = 0.7;
/// post/pre above this across a legato arrival = voice doubling.
const DOUBLE_RATIO: f32 = 1.9;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Onset,      // phrase-start attack
    Transition, // legato arrival (interior note change)
    Release,    // phrase-end note-off
}

struct Mark {
    frame: usize,
    kind: Kind,
    label: String,
}

fn qn_to_frame(bpm: f64, qn: f64) -> usize {
    (qn * 60.0 / bpm * SR as f64).round() as usize
}

/// Build a mono legato line from (pitch, dur-QN); pitch 0 = rest.
fn line(steps: &[(u8, f64)], vel: u8) -> Vec<DocNote> {
    let mut out = Vec::new();
    let mut qn = 1.0;
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
    if let Some(last) = out.last_mut() {
        last.end_qn -= OVERLAP_QN;
    }
    out
}

const NN: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn nm(p: u8) -> String {
    format!("{}{}", NN[(p % 12) as usize], (p / 12) as i32 - 1)
}

fn rms(x: &[f32], a: usize, b: usize) -> f32 {
    let a = a.min(x.len());
    let b = b.min(x.len());
    if b <= a {
        return 0.0;
    }
    (x[a..b].iter().map(|&v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
}

fn max_step(x: &[f32], a: usize, b: usize) -> f32 {
    let a = a.min(x.len());
    let b = b.min(x.len());
    let mut mx = 0.0f32;
    for i in a.max(1)..b {
        mx = mx.max((x[i] - x[i - 1]).abs());
    }
    mx
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

    let q = 1.0;
    // ── Battery ─────────────────────────────────────────────────────────────
    // Each entry: (pattern name, total-QN length, [(pitch, duration-QN), ...]).
    type QaPattern = (&'static str, f64, Vec<(u8, f64)>);
    let mut battery: Vec<QaPattern> = Vec::new();

    // Chromatic up then down (the reported "doubles at the top").
    let mut chroma: Vec<(u8, f64)> = (60u8..=72).map(|p| (p, 0.5)).collect();
    chroma.extend((60u8..=71).rev().map(|p| (p, 0.5)));
    chroma.last_mut().unwrap().1 = 3.0;
    battery.push(("chromatic_updown", 96.0, chroma));

    // C major scale up + down.
    let mut scale: Vec<(u8, f64)> = [60u8, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .map(|&p| (p, q))
        .collect();
    scale.extend([71u8, 69, 67, 65, 64, 62, 60].iter().map(|&p| (p, q)));
    scale.last_mut().unwrap().1 = 3.0;
    battery.push(("cmajor_updown", 90.0, scale));

    // Wide leaps — octaves and larger, both directions.
    battery.push((
        "leaps",
        80.0,
        vec![
            (60, q),
            (72, q),
            (60, q),
            (67, q),
            (79, q),
            (67, q),
            (55, q),
            (72, q),
            (60, 3.0),
        ],
    ));

    // Repeated notes — same-pitch re-bows.
    battery.push((
        "repeats",
        100.0,
        vec![
            (67, q),
            (67, q),
            (67, q),
            (67, q),
            (65, q),
            (65, q),
            (64, q),
            (64, 3.0),
        ],
    ));

    // Fast run — 16th notes stress transition timing.
    let mut run: Vec<(u8, f64)> = Vec::new();
    for &p in &[60u8, 62, 64, 65, 67, 69, 71, 72, 71, 69, 67, 65, 64, 62] {
        run.push((p, 0.25));
    }
    run.push((60, 3.0));
    battery.push(("fast_run", 120.0, run));

    let mut total_fail = 0usize;
    let opts = DocumentRenderOptions::default();

    for (name, bpm, steps) in &battery {
        let notes = line(steps, 80);
        // Fresh trace per pattern (enable clears).
        rig.set_trace_enabled(ID, true);
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
            notes: notes.clone(),
            tempo: vec![TempoPoint { qn: 0.0, bpm: *bpm }],
        };
        let res = rig.render_offline_document(ID, &doc, &opts)?;
        // Interleaved → mono.
        let mono: Vec<f32> = res.audio.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect();

        // Per-note solo stems (deterministic re-render with only one note
        // audible; legato timing is bit-identical). One WAV per distinct pitch.
        let mut distinct: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
        distinct.sort_unstable();
        distinct.dedup();
        let mut stems: Vec<(u8, String, String)> = Vec::new();
        for &pitch in &distinct {
            rig.set_solo_notes(ID, Some(std::iter::once(pitch).collect()));
            let sres = rig.render_offline_document(ID, &doc, &opts)?;
            let fname = format!("qa_{name}.n{pitch}.wav");
            write_wav(&PathBuf::from(format!("target/{fname}")), &sres.audio)?;
            stems.push((pitch, nm(pitch), fname));
        }
        rig.set_solo_notes(ID, None);

        // Metronome click over the full render length (real click sample).
        let click_sample = std::path::Path::new(signal_sampler::report::DEFAULT_CLICK_SAMPLE);
        let click = signal_sampler::report::click_track(
            res.audio.len() / 2,
            SR,
            *bpm,
            4,
            click_sample.exists().then_some(click_sample),
        );
        write_wav(
            &PathBuf::from(format!("target/qa_{name}.click.wav")),
            &click,
        )?;

        // Boundary marks: note onsets (first = attack, rest = transitions) and
        // phrase-end note-offs. Frames come straight from the notated grid.
        let mut marks: Vec<Mark> = Vec::new();
        for (i, n) in notes.iter().enumerate() {
            let kind = if i == 0 {
                Kind::Onset
            } else {
                Kind::Transition
            };
            marks.push(Mark {
                frame: qn_to_frame(*bpm, n.start_qn),
                kind,
                label: format!(
                    "{}→{}",
                    if i == 0 {
                        "on".into()
                    } else {
                        nm(notes[i - 1].pitch)
                    },
                    nm(n.pitch)
                ),
            });
        }
        // Phrase-end release: the final note's end.
        if let Some(last) = notes.last() {
            marks.push(Mark {
                frame: qn_to_frame(*bpm, last.end_qn),
                kind: Kind::Release,
                label: format!("rel {}", nm(last.pitch)),
            });
        }

        // Window sizes (frames).
        let ms = |m: f64| (m / 1000.0 * SR as f64) as usize;
        let mut fails: Vec<String> = Vec::new();
        let mut labels: Vec<(f64, f64, String)> = Vec::new();

        for m in &marks {
            let f = m.frame;
            if f < ms(40.0) || f + ms(50.0) >= mono.len() {
                continue;
            }
            let pre = rms(&mono, f - ms(30.0), f - ms(8.0));
            let post = rms(&mono, f + ms(8.0), f + ms(45.0));
            let mid = rms(&mono, f - ms(3.0), f + ms(3.0));
            let step = max_step(&mono, f - ms(4.0), f + ms(12.0));

            let mut tags: Vec<String> = Vec::new();
            if step > CLICK {
                tags.push(format!("CLICK({step:.3})"));
            }
            match m.kind {
                Kind::Transition => {
                    let lo = pre.min(post).max(1e-6);
                    if mid < GAP_FRAC * lo {
                        tags.push(format!("GAP(mid {mid:.4} vs {lo:.4})"));
                    }
                    if post / pre.max(1e-6) > DOUBLE_RATIO {
                        tags.push(format!("DOUBLING(post/pre {:.2})", post / pre.max(1e-6)));
                    } else {
                        let rel = (post - pre).abs() / pre.max(post).max(1e-6);
                        if rel > JUMP_FRAC {
                            tags.push(format!("JUMP({rel:.2})"));
                        }
                    }
                }
                Kind::Onset => {}
                Kind::Release => {}
            }

            let t = f as f64 / SR as f64;
            let lbl = if tags.is_empty() {
                m.label.clone()
            } else {
                format!("{} !! {}", m.label, tags.join(","))
            };
            labels.push((t, t, lbl));
            if !tags.is_empty() {
                fails.push(format!("  {t:7.3}s  {}  [{}]", m.label, tags.join(", ")));
            }
        }

        // Write WAV + Audacity label sidecar.
        let wav = PathBuf::from(format!("target/qa_{name}.wav"));
        write_wav(&wav, &res.audio)?;
        let lbl_path = PathBuf::from(format!("target/qa_{name}.labels.txt"));
        let lbl_body: String = labels
            .iter()
            .map(|(a, b, s)| format!("{a:.6}\t{b:.6}\t{s}\n"))
            .collect();
        std::fs::write(&lbl_path, lbl_body)?;

        // Waveform + full-event-log HTML report next to the WAV.
        //
        // Frame bases differ: `res.transitions`/`res.markers` are already in
        // the AUDIO window, but `render_trace()` frames are engine-lifetime
        // (this rig renders the whole battery, so pattern N's trace starts
        // where pattern N-1 ended). Anchor-match the first Transition trace
        // event against the first fired transition to find the offset.
        let mut trace = rig.render_trace(ID);
        let trace_anchor = trace
            .events
            .iter()
            .find(|e| matches!(e.kind, signal_sampler::TraceKind::Transition { .. }))
            .map(|e| e.frame);
        let audio_anchor = res.transitions.first().map(|f| f.frame);
        let offset = match (trace_anchor, audio_anchor) {
            (Some(te), Some(ae)) => te.saturating_sub(ae),
            _ => trace.events.first().map(|e| e.frame).unwrap_or(0),
        };
        trace.events.retain(|e| e.frame >= offset);
        for e in &mut trace.events {
            e.frame -= offset;
        }
        let fires = res.transitions.clone();
        let mut rep_markers: Vec<(u64, String, u8, u8)> = res
            .markers
            .iter()
            .map(|m| (m.frame, format!("{:?}", m.kind), m.note, m.line))
            .collect();
        // QA boundary labels (incl. failure tags) as their own markers.
        for (t, _, s) in &labels {
            rep_markers.push(((t * SR as f64) as u64, format!("QA {s}"), 0, 0));
        }
        let emitted = res.emitted_markers.clone();
        let sources = signal_sampler::report::ReportSources {
            trace,
            fires,
            markers: rep_markers,
            emitted,
            audio_href: Some(format!("qa_{name}.wav")),
            stems,
            // The QA grid is anchored at qn 0 = frame 0 (same basis as the
            // boundary labels); 4/4 throughout.
            tempo: Some((*bpm, 4)),
            click_href: Some(format!("qa_{name}.click.wav")),
            mode_label: "DOCUMENT (prefire)".into(),
            reactive_fallbacks: res.reactive_fallbacks,
        };
        let data = signal_sampler::report::render_report_json(
            &format!("qa_{name}"),
            &res.audio,
            2,
            SR,
            &sources,
        );
        signal_sampler::report::write_report_html(
            &PathBuf::from(format!("target/qa_{name}.html")),
            &data,
        )
        .map_err(|e| eyre::eyre!("report: {e}"))?;

        println!(
            "── {name}: {} notes, {} transitions fired, {} reactive fallbacks, {}s",
            notes.len(),
            res.transitions.len(),
            res.reactive_fallbacks,
            res.audio.len() / 2 / SR as usize,
        );
        if res.reactive_fallbacks != 0 {
            println!(
                "   ! {} reactive fallback(s) — annotator missed an edge",
                res.reactive_fallbacks
            );
        }
        if fails.is_empty() {
            println!("   ✓ {} boundaries clean", marks.len());
        } else {
            println!("   ✗ {} / {} boundaries FAILED:", fails.len(), marks.len());
            for f in &fails {
                println!("{f}");
            }
        }
        total_fail += fails.len();
    }

    println!(
        "\n{} total boundary failures across the battery.",
        total_fail
    );
    if total_fail > 0 {
        std::process::exit(1);
    }
    Ok(())
}
