//! Render the legato timing-showcase corpus (see
//! `signal_orchestra::timing::timing_corpus`) through BOTH engine paths with
//! a click generated from THE SAME tempo map, for the owner's ears:
//!
//! * `<case>_doc_click.wav` — offline document (Lookahead) render + click
//! * `<case>_live_click.wav` — StrictLive reactive replay + click
//! * `<case>_doc.wav` / `<case>_live.wav` — dry solo renders
//! * `click_only_<bpm>bpm.wav` — the click by itself
//! * `manifest.tsv` — per-note map: case, path, note index, qn,
//!   expected-arrival seconds, pitch, kind
//!
//! Every case starts after a one-bar count-in; the click pans right so the
//! music reads against it. Prints per-note deterministic arrival errors
//! (engine metadata) and acoustic flux errors as it goes.
//!
//! ```text
//! cargo run --release -p signal-orchestra --example render_timing_corpus [-- <out_dir>]
//! ```

use std::io::Write as _;
use std::path::PathBuf;

use signal_orchestra::timing::{
    mix_click, pitch_arrival, render_click, render_live_replay, spectral_flux, timing_corpus,
    CountIn, OnsetKind,
};
use signal_orchestra::{load_strings, CSS_CONFIG, CSS_ROOT};
use signal_sampler::document::{qn_to_frame, qn_to_sec, DocumentRenderOptions};
use signal_sampler::SamplerRig;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

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
    let out_dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "listen/corpus".to_string()),
    );
    std::fs::create_dir_all(&out_dir)?;

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG)
        .map_err(|e| eyre::eyre!(e))?;
    // A second rig for the live path so reactive state never leaks into the
    // document renders.
    let live_rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&live_rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG)
        .map_err(|e| eyre::eyre!(e))?;

    let mut manifest = std::fs::File::create(out_dir.join("manifest.tsv"))?;
    writeln!(
        manifest,
        "case\tfile\tnote_idx\tqn\texpected_sec\tpitch\tkind\tbpm"
    )?;

    let mut click_bpms: Vec<u32> = Vec::new();
    for case in timing_corpus() {
        println!("── {} — {}", case.name, case.desc);

        // Document (Lookahead) path.
        let res = rig
            .render_offline_document(ID, &case.doc, &DocumentRenderOptions::default())
            .map_err(|e| eyre::eyre!("{}: {e}", case.name))?;
        let peak = res.audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        println!(
            "   doc: {:.1}s, peak {:.3}, {} transitions, {} reactive fallbacks",
            res.audio.len() as f64 / 2.0 / SR as f64,
            peak,
            res.transitions.len(),
            res.reactive_fallbacks
        );
        // Deterministic engine arrivals vs the grid.
        let legatos: Vec<_> = case
            .expected
            .iter()
            .filter(|e| matches!(e.kind, OnsetKind::Legato | OnsetKind::Rebow))
            .collect();
        for (t, exp) in res.transitions.iter().zip(&legatos) {
            let grid = qn_to_frame(&case.doc.tempo, exp.qn, SR);
            println!(
                "   engine arrival {}→{} qn {:4.1}: err {:+} frames",
                t.from_note,
                t.to_note,
                exp.qn,
                t.arrival as i64 - grid
            );
        }

        // Live (StrictLive reactive) path — same document, no lookahead.
        let live = render_live_replay(&live_rig, ID, &case.doc, 3.0);

        // Click from THE SAME tempo map, with count-in on bar 1.
        let frames = (res.audio.len() / 2).max(live.len() / 2);
        let click = render_click(
            &case.doc.tempo,
            frames,
            SR,
            Some(CountIn {
                start_qn: 0.0,
                beats: 4,
            }),
        );

        let doc_click = mix_click(&res.audio, &click, 0.5, 0.7);
        let live_click = mix_click(&live, &click, 0.5, 0.7);
        write_wav(&out_dir.join(format!("{}_doc.wav", case.name)), &res.audio)?;
        write_wav(&out_dir.join(format!("{}_live.wav", case.name)), &live)?;
        write_wav(
            &out_dir.join(format!("{}_doc_click.wav", case.name)),
            &doc_click,
        )?;
        write_wav(
            &out_dir.join(format!("{}_live_click.wav", case.name)),
            &live_click,
        )?;

        // Acoustic flux errors, doc path (informational — the deterministic
        // numbers above are the proof).
        let flux = spectral_flux(&res.audio, SR);
        let min_ioi = case
            .expected
            .windows(2)
            .map(|w| w[1].sec - w[0].sec)
            .fold(f64::INFINITY, f64::min);
        let search = (min_ioi / 2.0).min(0.18);
        let mut prev_pitch: Option<u8> = None;
        for (i, exp) in case.expected.iter().enumerate() {
            // Same per-kind acoustic measures as tests/legato_arrival.rs.
            let (measured, label) = match exp.kind {
                OnsetKind::Legato => (
                    pitch_arrival(
                        &res.audio,
                        SR,
                        exp.sec,
                        prev_pitch.unwrap_or(exp.pitch),
                        exp.pitch,
                        search.max(0.15),
                    ),
                    "pitch",
                ),
                OnsetKind::Short | OnsetKind::Rebow => {
                    (flux.onset_near(exp.sec, search.min(0.25)), "flux-peak")
                }
                OnsetKind::PhraseStart => (flux.leading_edge(exp.sec, 0.10, 0.25), "flux-edge"),
            };
            let err = measured
                .map(|t| format!("{:+.1} ms", (t - exp.sec) * 1000.0))
                .unwrap_or_else(|| "n/a".into());
            prev_pitch = Some(exp.pitch);
            println!(
                "   note {i:2} qn {:5.2} pitch {:3} {:12} {label:9} err {err}",
                exp.qn,
                exp.pitch,
                format!("{:?}", exp.kind)
            );
            for path in ["doc", "live"] {
                writeln!(
                    manifest,
                    "{}\t{}_{}_click.wav\t{}\t{}\t{:.4}\t{}\t{:?}\t{}",
                    case.name, case.name, path, i, exp.qn, exp.sec, exp.pitch, exp.kind, case.bpm
                )?;
            }
        }

        let bpm = case.bpm as u32;
        if !click_bpms.contains(&bpm) {
            click_bpms.push(bpm);
            let solo = render_click(
                &case.doc.tempo,
                (qn_to_sec(&case.doc.tempo, 16.0) * SR as f64) as usize,
                SR,
                Some(CountIn {
                    start_qn: 0.0,
                    beats: 4,
                }),
            );
            write_wav(&out_dir.join(format!("click_only_{bpm}bpm.wav")), &solo)?;
        }
    }
    println!(
        "wrote corpus renders + manifest.tsv to {}",
        out_dir.display()
    );
    Ok(())
}
