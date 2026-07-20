//! ARRIVAL-SEMANTICS SWEEP — the ear-calibration renders.
//!
//! The engine places each legato transition so that a chosen in-sample point
//! lands exactly on the grid tick (playback-emitted markers prove the
//! placement is sample-exact). WHICH point — where in the recording "the
//! note is heard" — is a perceptual definition, and this sweep renders the
//! candidates side by side so the owner's ears pick it:
//!
//! | lane      | `SIGNAL_ARRIVAL_SEMANTICS` | meaning                                            |
//! |-----------|---------------------------|-----------------------------------------------------|
//! | glide     | `0`                       | transition content starts AT the beat (zero prefire beyond the LT offset — exactly what causal Kontakt does at note-on) |
//! | cross     | `0.5`                     | halfway between offset and measured settle — approximates the first pitch departure toward the destination (sweep-only approximation, labelled honestly) |
//! | settle50  | unset                     | the measured 50%-settle marker (current pack semantics) |
//! | settle80  | `1.15`                    | ~15% beyond the 50% settle — approximates an 80% settle (sweep-only approximation) |
//! | kontakt   | `1,20`                    | markers nudged +20 ms — the median delta measured against the REAL Kontakt render of the A/B corpus (`kontakt_arrival_ref`), i.e. arrival as Kontakt's own audio places it |
//!
//! Writes `listen/sweep/<case>_arrival-<lane>_click.wav` + `README.md`.
//!
//! ```text
//! cargo run --release -p signal-orchestra --example sweep_arrival_semantics
//! ```

use std::path::PathBuf;

use signal_orchestra::timing::{CountIn, mix_click, render_click, timing_corpus};
use signal_orchestra::{CSS_CONFIG, CSS_ROOT, load_strings};
use signal_sampler::SamplerRig;
use signal_sampler::document::DocumentRenderOptions;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

fn write_wav(path: &std::path::Path, samples: &[f32]) -> eyre::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
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
            .unwrap_or_else(|| "listen/sweep".to_string()),
    );
    std::fs::create_dir_all(&out_dir)?;

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG)
        .map_err(|e| eyre::eyre!(e))?;

    let wanted = [
        "scale_expr_60bpm",
        "scale_expr_90bpm_med",
        "intervals_up_90bpm_med",
    ];
    let lanes: &[(&str, Option<&str>)] = &[
        ("glide", Some("0")),
        ("cross", Some("0.5")),
        ("settle50", None),
        ("settle80", Some("1.15")),
        ("kontakt", Some("1,20")),
    ];

    let mut readme = String::from(
        "# Arrival-semantics sweep\n\n\
         Same MIDI, same zones, same click — only the DEFINITION of \"where in the\n\
         transition sample the note is heard\" changes per lane. The engine places\n\
         that point exactly on the click (playback-emitted markers verify each\n\
         render); your ears pick the definition.\n\n\
         | lane | placement of the beat |\n|---|---|\n\
         | glide | transition content STARTS at the beat (zero prefire beyond the LT offset — causal-Kontakt behaviour at note-on) |\n\
         | cross | halfway offset→settle (approximates the first pitch departure; sweep-only approximation) |\n\
         | settle50 | the measured 50%-settle marker — current pack semantics |\n\
         | settle80 | ~15% past the 50%-settle (approximates an 80% settle; sweep-only approximation) |\n\
         | kontakt | settle50 nudged +20 ms — the median delta measured against the real Kontakt render of the A/B corpus (`kontakt_arrival_ref`) |\n\n\
         Files: `<case>_arrival-<lane>_click.wav` (click panned right, one-bar count-in).\n",
    );

    for case in timing_corpus()
        .into_iter()
        .filter(|c| wanted.contains(&c.name.as_str()))
    {
        for (lane, env) in lanes {
            match env {
                Some(v) => std::env::set_var("SIGNAL_ARRIVAL_SEMANTICS", v),
                None => std::env::remove_var("SIGNAL_ARRIVAL_SEMANTICS"),
            }
            let res = rig
                .render_offline_document(ID, &case.doc, &DocumentRenderOptions::default())
                .map_err(|e| eyre::eyre!("{} {lane}: {e}", case.name))?;
            let frames = res.audio.len() / 2;
            let click = render_click(
                &case.doc.tempo,
                frames,
                SR,
                Some(CountIn {
                    start_qn: 0.0,
                    beats: 4,
                }),
            );
            let mix = mix_click(&res.audio, &click, 0.9, 0.85);
            let path = out_dir.join(format!("{}_arrival-{lane}_click.wav", case.name));
            write_wav(&path, &mix)?;
            eprintln!("wrote {}", path.display());
        }
        readme.push_str(&format!("\n- case `{}`: {}\n", case.name, case.desc));
    }
    std::env::remove_var("SIGNAL_ARRIVAL_SEMANTICS");
    std::fs::write(out_dir.join("README.md"), readme)?;
    Ok(())
}
