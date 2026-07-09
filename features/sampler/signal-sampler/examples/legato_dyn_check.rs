//! Legato dynamic-match check (document mode).
//!
//! Renders the same C-major legato scale at a SOFT (CC1=25) and a LOUD
//! (CC1=110) dynamic, non-vibrato (CC2=0), and reports — for every legato
//! transition — the level of the transition arrival relative to the sustain
//! just before it. The fix makes the transition track the current dynamic
//! (`cc1_expression`), so:
//!   * the ratio arrival/sustain should be ≈1 at BOTH dynamics (no "ff bump"),
//!   * absolute levels should scale with CC1 (soft render quieter than loud).
//!
//! ```text
//! cargo run --release -p signal-sampler --example legato_dyn_check
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
const SEED: u64 = 0x0C0F_FEE0_5EED_0002;
const BPM: f64 = 80.0;
const OVERLAP_QN: f64 = 1.0 / 32.0;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, &v| m.max(v.abs()))
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

    // C major scale up, quarter notes, all legato.
    let pitches = [60u8, 62, 64, 65, 67, 69, 71, 72];
    let mut notes = Vec::new();
    let mut qn = 1.0;
    for (i, &p) in pitches.iter().enumerate() {
        let last = i == pitches.len() - 1;
        notes.push(DocNote {
            start_qn: qn,
            end_qn: qn + 1.0 + if last { 0.0 } else { OVERLAP_QN },
            chan: 0,
            pitch: p,
            vel: 80,
        });
        qn += 1.0;
    }

    let opts = DocumentRenderOptions::default();
    for (label, cc1) in [("soft(CC1=25)", 25u8), ("loud(CC1=110)", 110u8)] {
        let doc = TrackDocument {
            version: 1,
            seed: SEED,
            auto_divisi: false,
            ccs: vec![
                DocCc {
                    qn: 0.0,
                    chan: 0,
                    cc: 1,
                    val: cc1,
                },
                DocCc {
                    qn: 0.0,
                    chan: 0,
                    cc: 2,
                    val: 0,
                },
            ],
            notes: notes.clone(),
            tempo: vec![TempoPoint { qn: 0.0, bpm: BPM }],
        };
        let res = rig.render_offline_document(ID, &doc, &opts)?;
        let a = &res.audio; // interleaved stereo
        let frame = |i: usize| -> f32 { 0.5 * (a[i * 2].abs() + a[i * 2 + 1].abs()) };
        let nframes = a.len() / 2;
        let mono: Vec<f32> = (0..nframes).map(frame).collect();

        println!("── {label}: {} transitions", res.transitions.len());
        let mut ratios = Vec::new();
        let mut arr_levels = Vec::new();
        for t in &res.transitions {
            let f = t.frame as usize;
            // sustain just before the transition arrival (200 ms ending 30 ms
            // before) vs the arrival peak (0..80 ms after).
            let sus_a = f.saturating_sub((0.23 * SR as f64) as usize);
            let sus_b = f.saturating_sub((0.03 * SR as f64) as usize);
            let arr_a = f;
            let arr_b = (f + (0.08 * SR as f64) as usize).min(nframes);
            if sus_b <= sus_a || arr_b <= arr_a {
                continue;
            }
            let sus = rms(&mono[sus_a..sus_b]);
            let arr = peak(&mono[arr_a..arr_b]);
            if sus > 1e-5 {
                ratios.push(arr / sus);
            }
            arr_levels.push(arr);
        }
        ratios.sort_by(|a, b| a.total_cmp(b));
        let median = ratios.get(ratios.len() / 2).copied().unwrap_or(0.0);
        let maxr = ratios.last().copied().unwrap_or(0.0);
        let mean_arr = arr_levels.iter().sum::<f32>() / arr_levels.len().max(1) as f32;
        println!(
            "   arrival/sustain ratio: median={:.2}×  max={:.2}×   mean arrival level={:.4}",
            median, maxr, mean_arr
        );
    }
    println!(
        "\nInterpret: ratio ≈1 at BOTH dynamics = transition matches the note (no ff bump).\n\
         mean arrival level should be much lower for soft than loud (dynamic tracked)."
    );
    Ok(())
}
