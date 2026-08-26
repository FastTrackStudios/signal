//! Long-hold loop check (document mode).
//!
//! Holds single notes for ~20 s each through the CSS Violin 1 patch,
//! non-vibrato (CC2=0), so the synthesized sustain loop repeats many times —
//! isolating loop-boundary behaviour (pulsing / clicks) from the sample's
//! natural attack arc.
//!
//! ```text
//! cargo run --release -p signal-sampler --example loop_hold_check
//! ```

use std::path::PathBuf;

use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str = "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const SEED: u64 = 0x1009_ABCD_EF01_2345;
const BPM: f64 = 60.0; // 1 QN = 1 s

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

    // Three separate held notes, 20 s each, spaced by a 1 s gap.
    let hold = 20.0;
    let gap = 1.0;
    let pitches = [60u8, 67, 72];
    let mut notes = Vec::new();
    let mut qn = 1.0;
    for &p in &pitches {
        notes.push(DocNote {
            start_qn: qn,
            end_qn: qn + hold,
            chan: 0,
            pitch: p,
            vel: 80,
        });
        qn += hold + gap;
    }

    let doc = TrackDocument {
        version: 1,
        seed: SEED,
        auto_divisi: false,
        // CC1=84 medium dynamic; CC2=0 non-vibrato.
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
        tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
    };

    let opts = DocumentRenderOptions::default();
    let res = rig.render_offline_document(ID, &doc, &opts)?;
    let out = PathBuf::from("target/loop_hold_check.wav");
    write_wav(&out, &res.audio)?;
    println!(
        "wrote {} ({:.1}s, {} notes)",
        out.display(),
        res.audio.len() as f64 / 2.0 / SR as f64,
        res.note_count
    );
    Ok(())
}
