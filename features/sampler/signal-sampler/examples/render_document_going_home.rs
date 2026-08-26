//! Document-mode end-to-end proof (phase 1, see docs/plan/document-mode.md):
//! MusicXML score → TrackDocument → annotated Schedule →
//! `render_offline_document` with the Cinematic Studio Strings Violin 1
//! patch → WAV — legato transition ARRIVALS land on the grid with no
//! negative track delay, byte-identical across runs (seeded).
//!
//! ```text
//! cargo run --release -p signal-sampler --example render_document_going_home \
//!     [-- <score.mxl> <out.wav>]
//! ```
//!
//! keyflow-orchestra is used here (dev-dependency, example-only) purely to
//! parse the .mxl into score-time notes + CC curves; the timing inversion
//! itself happens inside the sampler's document annotation.

use std::path::PathBuf;

use keyflow_orchestra::{process_part, score, Config, ProfileKind};
use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
use signal_sampler::SamplerRig;

const DEFAULT_MXL: &str = "/run/media/Development/FastTrackStudio/keyflow/examples/mxl/GOING HOME - Dvorak New World Symphony 1.6 mxml.mxl";
const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str = "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
/// Determinism seed — persisted with a project in real use; fixed here so
/// every run of the example is byte-identical.
const SEED: u64 = 0x60D0_1234_D0C5_EED0;

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

/// FNV-1a over the sample bit patterns — a cheap byte-identity fingerprint.
fn audio_hash(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x1_0000_01b3);
        }
    }
    h
}

fn main() -> eyre::Result<()> {
    let mxl = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_MXL.to_string());
    let out = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "target/going_home_violin1_document.wav".to_string());

    // ── Score → part ──────────────────────────────────────────────────────────
    let sc = score::load(&mxl).map_err(|e| eyre::eyre!("score load: {e}"))?;
    println!("score: {:?}", sc.work_title.as_deref().unwrap_or("?"));
    println!("parts:");
    for p in &sc.parts {
        println!("  - {:?} ({} notes)", p.name, p.notes.len());
    }
    let part = sc
        .parts
        .iter()
        .find(|p| {
            let n = p.name.to_lowercase();
            n.contains("violin") && (n.contains('1') || n.split_whitespace().any(|w| w == "i"))
        })
        .or_else(|| {
            sc.parts
                .iter()
                .find(|p| p.name.to_lowercase().contains("violin"))
        })
        .ok_or_else(|| eyre::eyre!("no Violin 1 part found"))?;
    println!("using part: {:?}", part.name);

    // Score-time output: notes stay at notated positions (timing_comp OFF —
    // the document annotation does the anticipation, that's the whole point),
    // full CC picture (CC1/CC2 expression, CC58 keyswitches, CC64 re-bow).
    let cfg = Config {
        profile: ProfileKind::Strings,
        timing_comp: false,
        tempo_map: Some(sc.meta.tempos.clone()),
        ..Config::default()
    };
    let po = process_part(part, &cfg);
    println!(
        "engine output: {} notes, {} cc events, channels {:?}",
        po.notes.len(),
        po.ccs.len(),
        po.channels
    );
    let mut notes_per_chan: std::collections::BTreeMap<u8, usize> = Default::default();
    for n in &po.notes {
        *notes_per_chan.entry(n.chan).or_default() += 1;
    }
    for (ch, n) in &notes_per_chan {
        println!("  chan {ch}: {n} notes");
    }

    // ── PartOutput → TrackDocument (QN domain; keyflow chans are 1-based) ────
    let doc = TrackDocument {
        version: 1,
        seed: SEED,
        // keyflow already assigned divisi channels — respect them as-is.
        auto_divisi: false,
        notes: po
            .notes
            .iter()
            .map(|n| DocNote {
                start_qn: n.start_qn,
                end_qn: n.end_qn,
                chan: n.chan.saturating_sub(1),
                pitch: n.pitch.clamp(0, 127) as u8,
                vel: n.vel,
            })
            .collect(),
        ccs: po
            .ccs
            .iter()
            .map(|e| DocCc {
                qn: e.qn,
                chan: e.chan.saturating_sub(1),
                cc: e.cc,
                val: e.val,
            })
            .collect(),
        tempo: sc
            .meta
            .tempos
            .iter()
            .map(|t| TempoPoint {
                qn: t.qn,
                bpm: t.bpm,
            })
            .collect(),
    };

    // ── CSS Violin 1 through the sampler ──────────────────────────────────────
    let css_root = PathBuf::from(CSS_ROOT);
    let zones = css_root.join("_patches/1st Violins/library.styx");
    if !zones.exists() {
        eyre::bail!(
            "CSS Violin 1 patch not found at {} — samples not on this machine",
            zones.display()
        );
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
    // CSS-default envelope (same as examples/render_css_test.rs).
    rig.set_attack_ms(ID, 20);
    rig.set_release_ms(ID, 400);

    let opts = DocumentRenderOptions::default();
    let t0 = std::time::Instant::now();
    let res = rig.render_offline_document(ID, &doc, &opts)?;
    let dur = res.audio.len() as f64 / 2.0 / SR as f64;
    println!(
        "rendered {:.1}s in {:.1}s: {} notes, {} legato transitions fired, seed {:#x}",
        dur,
        t0.elapsed().as_secs_f64(),
        res.note_count,
        res.transitions.len(),
        res.seed
    );
    // Per-line breakdown (divisi channels map to engine mono lines) + the
    // reactive-fallback count: per-line prefire scheduling must leave the
    // reactive path completely unused during document playback.
    let mut per_line: std::collections::BTreeMap<u8, usize> = Default::default();
    for t in &res.transitions {
        *per_line.entry(t.line).or_default() += 1;
    }
    for (line, n) in &per_line {
        println!("  line {line}: {n} prefired transitions");
    }
    println!("  reactive fallbacks: {}", res.reactive_fallbacks);
    if res.reactive_fallbacks != 0 {
        eyre::bail!(
            "document playback hit the reactive legato path {} times — annotator missed edges",
            res.reactive_fallbacks
        );
    }
    let h1 = audio_hash(&res.audio);
    println!("audio hash: {h1:#018x}");

    // Determinism proof: render again on a fresh rig — must hash identically.
    let rig2 = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig2.load_instrument_with_config(
        ID,
        &PathBuf::from(CSS_CONFIG),
        &zones,
        &css_root,
        "1st Violins",
        "Mix",
    )?;
    rig2.set_solo_mic(ID, Some("Mix".into()));
    rig2.set_attack_ms(ID, 20);
    rig2.set_release_ms(ID, 400);
    let res2 = rig2.render_offline_document(ID, &doc, &opts)?;
    let h2 = audio_hash(&res2.audio);
    println!(
        "second render hash: {h2:#018x} — {}",
        if h1 == h2 {
            "BYTE-IDENTICAL ✓"
        } else {
            "MISMATCH ✗"
        }
    );
    if h1 != h2 {
        eyre::bail!("determinism violation: two seeded renders differ");
    }

    let out = PathBuf::from(out);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_wav(&out, &res.audio)?;
    println!("wrote {}", out.display());
    Ok(())
}
