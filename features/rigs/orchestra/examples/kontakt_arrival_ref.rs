//! Measure where KONTAKT ITSELF places legato arrivals relative to MIDI
//! note-ons — the reference-behaviour verdict for the arrival-placement
//! policy question.
//!
//! Inputs: the per-note-alignable A/B pair (see `gen_css_ab.rs`):
//! * `css_ab_css.wav` — the real CSS-in-Kontakt export of `css_ab.mid`;
//! * `css_ab_ours.wav` — our StrictLive render of the same MIDI (note-ons
//!   placed at exact MIDI times by construction);
//! * `css_ab_manifest.tsv` — unit index (t_start, pitch, category).
//!
//! Method:
//! 1. The Kontakt export's global offset `g` is estimated from the SHORT
//!    units: both engines fire shorts AT note-on with the same recordings,
//!    so per unit `flux-peak(css) − flux-peak(ours)` differs only by `g`;
//!    the median over all shorts is robust to per-RR spread.
//! 2. For each LEG-VEL / LEG-INT unit, the destination note-on is
//!    `t_start + 0.5` (see `Gen::leg`). The destination-pitch arrival in the
//!    Kontakt render is measured with the same detector the corpus uses
//!    (`pitch_arrival`), searched over [note-on − 150 ms, note-on + 600 ms].
//!    `arrival − note-on` is Kontakt's own trigger→arrival latency.
//!
//! If Kontakt's arrivals land AFTER note-on by ≈ the velocity-zone delay,
//! then Kontakt's real-world placement is TRANSITION-at-note-on (arrive
//! late), and a player's Kontakt-trained ear will hear our
//! arrive-at-tick renders as "the note arrives early".
//!
//! ```text
//! cargo run --release -p signal-orchestra --example kontakt_arrival_ref -- \
//!     <css.wav> <ours.wav> <manifest.tsv>
//! ```

use signal_orchestra::timing::{dest_energy_curve, pitch_arrival, spectral_flux};
use signal_orchestra::{CSS_CONFIG, CSS_ROOT};
use signal_sampler::PlayerPatch;

fn load_wav(path: &str) -> eyre::Result<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let norm = |v: i32, bits: u16| v as f32 / (1i64 << (bits - 1)) as f32;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => r
            .samples::<i32>()
            .map(|s| norm(s.unwrap_or(0), spec.bits_per_sample))
            .collect(),
    };
    let frames = samples.len() / ch;
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let l = samples[f * ch];
        let rr = if ch >= 2 { samples[f * ch + 1] } else { l };
        out.push(l);
        out.push(rr);
    }
    Ok((out, spec.sample_rate))
}

struct Unit {
    t_start: f64,
    pitch: u8,
    category: String,
    desc: String,
}

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let css_path = args.next().unwrap_or_else(|| {
        "/run/media/Development/FastTrackStudio-legacy/signal/css_ab_css.wav".into()
    });
    let ours_path = args.next().unwrap_or_else(|| {
        "/run/media/Development/FastTrackStudio-legacy/signal/css_ab_ours.wav".into()
    });
    let manifest_path = args.next().unwrap_or_else(|| {
        "/run/media/Development/FastTrackStudio-wt-020bc328-orchestral-violin-1-perfect-render-css/css_ab_manifest.tsv".into()
    });

    let (css, sr_css) = load_wav(&css_path)?;
    let (ours, sr_ours) = load_wav(&ours_path)?;
    eyre::ensure!(sr_css == sr_ours, "sample-rate mismatch");
    let sr = sr_css;

    let units: Vec<Unit> = std::fs::read_to_string(&manifest_path)?
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some(Unit {
                t_start: f.get(1)?.parse().ok()?,
                pitch: f.get(3)?.parse().ok()?,
                category: f.get(4)?.to_string(),
                desc: f.get(5)?.to_string(),
            })
        })
        .collect();

    // 1. Global offset from the SHORT units' flux peaks.
    let flux_css = spectral_flux(&css, sr);
    let flux_ours = spectral_flux(&ours, sr);
    let mut deltas: Vec<f64> = Vec::new();
    for u in units.iter().filter(|u| u.category == "SHORT") {
        // Shorts fire at note-on = t_start; search generously.
        let (a, b) = (
            flux_css.onset_near(u.t_start + 0.15, 0.40),
            flux_ours.onset_near(u.t_start + 0.15, 0.40),
        );
        if let (Some(a), Some(b)) = (a, b) {
            deltas.push(a - b);
        }
    }
    eyre::ensure!(!deltas.is_empty(), "no short units matched");
    deltas.sort_by(|a, b| a.total_cmp(b));
    let g = deltas[deltas.len() / 2];
    eprintln!(
        "global Kontakt-export offset g = {:+.1} ms (median over {} shorts, spread {:+.1}..{:+.1})",
        g * 1000.0,
        deltas.len(),
        deltas[0] * 1000.0,
        deltas[deltas.len() - 1] * 1000.0
    );

    // 2. CALIBRATION: Kontakt is causal — it starts each transition at
    //    note-on with the velocity-range start offset and plays linearly, so
    //    every render moment maps to a sample position:
    //        sample_pos(t) = lt_offset + (t − note_on)
    //    The perceived arrival in the KONTAKT render therefore gives the
    //    TRUE perceptual in-sample arrival position for that zone:
    //        true_marker = lt_offset + (perceived_arrival − note_on)
    //    measured from the exact rendering the owner's ear treats as
    //    reference. Compared against our pack's 50%-settle marker per zone.
    let spec = PlayerPatch::load_merged(
        std::path::Path::new(CSS_CONFIG),
        &std::path::Path::new(CSS_ROOT).join("_patches/1st Violins/library.styx"),
        std::path::Path::new(CSS_ROOT),
    )
    .map_err(|e| eyre::eyre!("load spec: {e}"))?
    .spec;
    let cfg = spec.legato_cfg();

    // Our pack's marker for the zone Kontakt plays for this unit: NVLeg
    // (CC2=0), ff layer (CC1=90 → hi), direction/interval/nearest-root —
    // the same selection as the engine.
    let marker_of = |from: u8, to: u8| -> Option<(String, f32)> {
        let direction = if to > from { "up" } else { "down" };
        let named = from.min(to);
        let interval = u32::from(from.abs_diff(to)).min(12);
        spec.zones
            .iter()
            .filter(|z| {
                z.interval == interval
                    && z.direction.eq_ignore_ascii_case(direction)
                    && z.articulation.eq_ignore_ascii_case("NVLeg")
                    && z.mic.eq_ignore_ascii_case("Mix")
                    && z.dynamic.eq_ignore_ascii_case("ff")
            })
            .min_by_key(|z| z.root_key.abs_diff(named))
            .map(|z| (z.file.clone(), z.transition_arrival_ms()))
    };

    println!(
        "{:<26} {:>7} {:>9} {:>9} {:>9} {:>10} {:>10} {:>10} {:>9}",
        "unit",
        "lt_off",
        "K-pitch",
        "K-e25",
        "K-e50",
        "K-true50",
        "ourMarker",
        "delta",
        "ours-live"
    );
    let mut deltas_by_class: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    for u in units
        .iter()
        .filter(|u| u.category == "LEG-VEL" || u.category == "LEG-INT")
    {
        let noteon = u.t_start + 0.5;
        let from = 67u8;
        let to = u.pitch;
        if from == to {
            continue;
        }
        // Mode + velocity → lt_offset (IOI here is 500 ms — OD ≈ 0).
        let (expressive, vel) = if u.desc.starts_with("EX") {
            (
                true,
                u.desc
                    .rsplit("vel")
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(85),
            )
        } else if u.desc.starts_with("LL") {
            (
                false,
                u.desc
                    .rsplit("vel")
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(85),
            )
        } else {
            (false, 85) // LEG-INT units: vel85, LL mode (CC58=2)
        };
        let lt_off = f64::from(cfg.lt_offset_ms(500.0, vel, expressive));

        // Ensemble on the ISOLATED Kontakt unit.
        let expected = noteon + g + 0.225;
        let k_pitch =
            pitch_arrival(&css, sr, expected, from, to, 0.375).map(|t| (t - g - noteon) * 1000.0);
        let (w0, w1) = (noteon + g - 0.15, noteon + g + 0.65);
        let curve = dest_energy_curve(&css, sr, from, to, w0, w1);
        let n = curve.v.len();
        let med = |sl: &mut Vec<f32>| -> f64 {
            sl.sort_by(|a, b| a.total_cmp(b));
            f64::from(sl[sl.len() / 2])
        };
        let head_n = ((0.10 / curve.hop_sec) as usize).min(n);
        let tail_n = ((0.15 / curve.hop_sec) as usize).min(n);
        let floor = med(&mut curve.v[..head_n].to_vec());
        let plateau = med(&mut curve.v[n - tail_n..].to_vec());
        let rise = |frac: f64| -> Option<f64> {
            if plateau <= floor {
                return None;
            }
            let th = floor + (plateau - floor) * frac;
            curve
                .v
                .iter()
                .position(|&e| f64::from(e) >= th)
                .map(|i| (curve.t0 + i as f64 * curve.hop_sec - g - noteon) * 1000.0)
        };
        let (k_e25, k_e50) = (rise(0.25), rise(0.50));

        // True in-sample arrival per the causal mapping (using e50 as the
        // primary perceptual estimate; the table shows all three).
        let k_true50 = k_e50.map(|a| lt_off + a);
        let (zone, our_marker) = marker_of(from, to)
            .map(|(f, m)| (f, f64::from(m)))
            .unwrap_or(("?".into(), 0.0));
        let delta = k_true50.map(|t| our_marker - t);
        let arr_ours = pitch_arrival(&ours, sr, noteon + 0.225, from, to, 0.375)
            .map(|t| (t - noteon) * 1000.0);
        if let Some(d) = delta {
            let class = if u.category == "LEG-VEL" {
                format!("vel({})", if expressive { "EX" } else { "LL" })
            } else {
                format!("int{}", from.abs_diff(to))
            };
            deltas_by_class.entry(class).or_default().push(d);
        }
        let fmt = |v: Option<f64>| match v {
            Some(v) => format!("{v:+8.1}"),
            None => "       —".into(),
        };
        println!(
            "{:<26} {:>7.0} {:>9} {:>9} {:>9} {:>10} {:>10.1} {:>10} {:>9}   {}",
            u.desc,
            lt_off,
            fmt(k_pitch),
            fmt(k_e25),
            fmt(k_e50),
            fmt(k_true50),
            our_marker,
            fmt(delta),
            fmt(arr_ours),
            zone.rsplit('/').next().unwrap_or("")
        );
    }
    println!(
        "
delta = ourMarker − Kontakt-true (positive = our marker sits DEEPER than perception):"
    );
    for (class, mut v) in deltas_by_class {
        v.sort_by(|a, b| a.total_cmp(b));
        let median = v[v.len() / 2];
        println!(
            "  {:<10} n={:<2} median {:+7.1} ms  range {:+7.1}..{:+7.1}",
            class,
            v.len(),
            median,
            v[0],
            v[v.len() - 1]
        );
    }
    Ok(())
}
