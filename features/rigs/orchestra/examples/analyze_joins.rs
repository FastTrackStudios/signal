//! Three-way join analysis — the detector-independent cross-measurement.
//!
//! For every legato join in the three-speed scale + interval corpus cases,
//! measures the destination note's entry THREE ways and prints them side by
//! side against the grid:
//!
//! 1. **pitch-share detector** (`timing::pitch_arrival`) — what the acoustic
//!    cross-check gates on (destination-vs-source harmonic balance
//!    crossing);
//! 2. **destination-band energy rise** (`timing::dest_energy_curve`) — the
//!    INDEPENDENT method: raw Goertzel energy on the destination's
//!    collision-pruned harmonics, normalized between its pre-join floor and
//!    post-join plateau, reported at the 25% and 50% rise crossings. No
//!    source normalization — none of the share detector's collision
//!    blindness — this tracks "when does the new pitch appear and grow",
//!    which is what the ear follows;
//! 3. **grid** — the tick the schedule promises.
//!
//! Also prints the WORKED EXAMPLE for each case's first join: zone file,
//! measured arrival marker, LT offset, mode cap, prefire lead, and the
//! engine's start offset — so the compensation arithmetic can be audited by
//! eye (double-compensation would show up here as
//! `offset != arrival − lead`).
//!
//! ```text
//! cargo run --release -p signal-orchestra --example analyze_joins
//! ```

use signal_orchestra::timing::{dest_energy_curve, pitch_arrival, timing_corpus};
use signal_orchestra::{load_strings, CSS_CONFIG, CSS_ROOT};
use signal_sampler::document::DocumentRenderOptions;
use signal_sampler::SamplerRig;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

fn main() -> eyre::Result<()> {
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG)
        .map_err(|e| eyre::eyre!(e))?;

    let wanted = [
        "scale_expr_90bpm_slow",
        "scale_expr_90bpm_med",
        "scale_expr_90bpm_fast",
        "intervals_up_90bpm_med",
    ];
    for case in timing_corpus()
        .into_iter()
        .filter(|c| wanted.contains(&c.name.as_str()))
    {
        let res = rig
            .render_offline_document(ID, &case.doc, &DocumentRenderOptions::default())
            .map_err(|e| eyre::eyre!("{}: {e}", case.name))?;
        println!("── {} — {}", case.name, case.desc);
        println!(
            "   {:>6} {:>4}→{:<3} {:>10} {:>12} {:>12} {:>12} {:>12}",
            "qn", "from", "to", "EMITTED", "detector", "energy25%", "energy50%", "drift(sum)"
        );
        let mut prev = None::<u8>;
        let mut drift = 0.0f64;
        let emitted_ms = |pitch: u8, tick: f64| -> Option<f64> {
            // Earliest playback-emitted arrival for this pitch within half a
            // beat of the tick — the ground truth of when the note was heard.
            res.emitted_markers
                .iter()
                .filter(|m| m.note == pitch)
                .map(|m| (m.frame as f64 / f64::from(SR) - tick) * 1000.0)
                .filter(|d| d.abs() < 370.0)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        };
        for exp in &case.expected {
            let Some(from) = prev else {
                prev = Some(exp.pitch);
                continue;
            };
            prev = Some(exp.pitch);
            if from == exp.pitch {
                continue;
            }
            let tick = exp.sec;
            // Detector (share crossing).
            let det = pitch_arrival(&res.audio, SR, tick, from, exp.pitch, 0.25)
                .map(|t| (t - tick) * 1000.0);
            // Independent: dest-band energy rise, normalized floor→plateau.
            // Window [tick − 400 ms, tick + 400 ms]; floor = median of the
            // first 100 ms (before ANY transition content for these leads),
            // plateau = median of the last 150 ms (note fully arrived).
            let (w0, w1) = (tick - 0.40, tick + 0.40);
            let curve = dest_energy_curve(&res.audio, SR, from, exp.pitch, w0, w1);
            let n = curve.v.len();
            let med = |sl: &mut [f32]| -> f64 {
                sl.sort_by(|a, b| a.total_cmp(b));
                f64::from(sl[sl.len() / 2])
            };
            let head_n = ((0.100 / curve.hop_sec) as usize).min(n);
            let tail_n = ((0.150 / curve.hop_sec) as usize).min(n);
            let floor = med(&mut curve.v[..head_n].to_vec());
            let plateau = med(&mut curve.v[n - tail_n..].to_vec());
            let rise = |frac: f64| -> Option<f64> {
                if plateau <= floor {
                    return None;
                }
                let thresh = floor + (plateau - floor) * frac;
                for (i, &e) in curve.v.iter().enumerate() {
                    if f64::from(e) >= thresh {
                        return Some((curve.t0 + i as f64 * curve.hop_sec - tick) * 1000.0);
                    }
                }
                None
            };
            let (e25, e50) = (rise(0.25), rise(0.50));
            let emit = emitted_ms(exp.pitch, tick);
            if let Some(e) = emit {
                drift += e;
            }
            let fmt = |v: Option<f64>| match v {
                Some(v) => format!("{v:+9.1} ms"),
                None => "        —".to_string(),
            };
            println!(
                "   {:6.2} {:>4}→{:<3} {:>10} {:>12} {:>12} {:>12} {:>+9.1} ms",
                exp.qn,
                from,
                exp.pitch,
                fmt(emit),
                fmt(det),
                fmt(e25),
                fmt(e50),
                drift
            );
        }
        println!();
    }
    println!(
        "note: run with SIGNAL_ANNOTATE_DEBUG=1 SIGNAL_LEGATO_DEBUG=1 for the worked\n\
         example (marker / lt_offset / lead / engine start offset per join)."
    );
    Ok(())
}
