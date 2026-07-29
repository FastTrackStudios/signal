//! NAM model space (#77 M5) — measure what a model actually *does* and put
//! it on the same similarity space the samples use, so a library of
//! inconsistently-named `.nam` files becomes navigable: "amps that sound
//! like this one", grouped by archetype, instead of a wall of filenames.
//!
//! A model is not audio, so it is **probed**: run known signals through it
//! and measure the response.
//!
//! - **EQ curve** — broadband noise in, per-band output magnitude out. The
//!   model's voicing, independent of level.
//! - **IO curve** — a sine at stepped input levels; output RMS per step
//!   traces the compression/distortion knee. A clean amp's curve is a
//!   straight line, a high-gain amp's flattens early.
//! - **gain / output level** — where the model sits, for level matching.
//!
//! The measurements become an ordinary [`signal_space::Space`], so KNN
//! similarity, the 2D map and the map UI all work unchanged.

use std::path::{Path, PathBuf};

use neural_amp_modeler::NamModel;
use signal_space::{Space, SpaceItem, SPACE_VERSION, knn};

/// The space built under `<nam-root>/Space/nam.space`.
pub const NAM_SPACE: &str = "nam";
/// Probe rate. NAM models are typically 48 k; mismatches still probe fine.
const PROBE_SR: f64 = 48_000.0;
/// Log-spaced bands of the measured EQ curve.
const EQ_BANDS: usize = 24;
/// Input levels (dBFS) the IO curve is sampled at.
///
/// REAL guitar-DI territory, not full scale: a pickup peaks around −20 to
/// −12 dBFS, and NAM models are trained at those levels. Probing up to
/// 0 dBFS drives every model into full saturation, which made a "cleanish"
/// AC30 measure as hard-clipped.
const IO_STEPS: &[f64] = &[-54.0, -48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0];
/// EQ bands + IO steps + (output level, knee sharpness).
pub const DIM: usize = EQ_BANDS + 8 + 2;

/// What a probe measures about one model.
#[derive(Debug, Clone)]
pub struct NamProbe {
    /// Per-band response in dB, mean-centered (voicing, level-independent).
    pub eq: [f32; EQ_BANDS],
    /// Output RMS in dBFS at each [`IO_STEPS`] input level.
    pub io: [f32; 8],
    /// Output level at −18 dBFS in (the reference operating point).
    pub output_db: f32,
    /// 0 = linear (clean), 1 = hard compression (high gain) — how much the
    /// IO curve bends across its range.
    pub knee: f32,
}

fn rms_db(x: &[f64]) -> f32 {
    let m = (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt();
    (20.0 * m.max(1e-12).log10()) as f32
}

/// Deterministic pseudo-noise (no RNG — every probe is reproducible).
fn noise(n: usize) -> Vec<f64> {
    let mut s = 0x9E3779B9u32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f64 / 8_388_608.0) - 1.0
        })
        .collect()
}

/// Probe one model.
pub fn probe(path: &Path) -> Result<NamProbe, String> {
    let mut model = NamModel::load(path)?;
    let block = 4096usize;
    model.reset(PROBE_SR, block);

    // ── EQ curve: broadband noise at a moderate level ──
    let drive = 10f64.powf(-30.0 / 20.0);
    let input: Vec<f64> = noise(block * 4).iter().map(|s| s * drive).collect();
    let mut out = vec![0.0f64; input.len()];
    for (i, o) in input.chunks(block).zip(out.chunks_mut(block)) {
        model.process(i, o);
    }
    // Skip the first block (model settling), analyze the rest.
    let tail = &out[block..];
    let eq = band_response(tail, &input[block..]);

    // ── IO curve: sine at stepped levels ──
    let mut io = [0.0f32; 8];
    for (k, &level_db) in IO_STEPS.iter().enumerate() {
        let amp = 10f64.powf(level_db / 20.0);
        let sine: Vec<f64> = (0..block * 2)
            .map(|i| (2.0 * std::f64::consts::PI * 220.0 * i as f64 / PROBE_SR).sin() * amp)
            .collect();
        let mut o = vec![0.0f64; sine.len()];
        model.reset(PROBE_SR, block);
        for (i, ob) in sine.chunks(block).zip(o.chunks_mut(block)) {
            model.process(i, ob);
        }
        io[k] = rms_db(&o[block..]);
    }
    // Knee: how far the curve departs from a straight line. A linear model
    // gains the same dB per step; a saturating one compresses the top.
    let low_slope = (io[3] - io[0]) as f64 / (IO_STEPS[3] - IO_STEPS[0]);
    let high_slope = (io[7] - io[4]) as f64 / (IO_STEPS[7] - IO_STEPS[4]);
    let knee = ((low_slope - high_slope).max(0.0) as f32).clamp(0.0, 1.0);
    tracing::debug!(?io, low_slope, high_slope, knee, "nam probe");

    Ok(NamProbe { eq, io, output_db: io[6], knee })
}

/// Output/input magnitude ratio per log-spaced band, in dB, mean-centered.
fn band_response(out: &[f64], inp: &[f64]) -> [f32; EQ_BANDS] {
    let n = out.len().min(inp.len()).next_power_of_two() / 2;
    let mut bands = [0.0f32; EQ_BANDS];
    // Goertzel-style band energy: cheap, no FFT dep, plenty for a voicing
    // curve at 24 bands.
    for (b, band) in bands.iter_mut().enumerate() {
        let lo = 40.0 * (10_000.0f64 / 40.0).powf(b as f64 / EQ_BANDS as f64);
        let hi = 40.0 * (10_000.0f64 / 40.0).powf((b + 1) as f64 / EQ_BANDS as f64);
        let f = (lo * hi).sqrt(); // band centre
        let (mut or, mut oi, mut ir, mut ii) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let w = 2.0 * std::f64::consts::PI * f / PROBE_SR;
        for k in 0..n {
            let (c, s) = ((w * k as f64).cos(), (w * k as f64).sin());
            or += out[k] * c;
            oi += out[k] * s;
            ir += inp[k] * c;
            ii += inp[k] * s;
        }
        let om = (or * or + oi * oi).sqrt().max(1e-12);
        let im = (ir * ir + ii * ii).sqrt().max(1e-12);
        *band = (20.0 * (om / im).log10()) as f32;
    }
    let mean = bands.iter().sum::<f32>() / EQ_BANDS as f32;
    for b in bands.iter_mut() {
        *b = ((*b - mean) / 12.0).clamp(-2.0, 2.0); // ±24 dB → ±2
    }
    bands
}

impl NamProbe {
    /// The similarity vector. Voicing dominates (it is what "sounds like"
    /// means); the IO curve and knee carry gain character.
    pub fn features(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(DIM);
        v.extend_from_slice(&self.eq);
        // IO curve normalized to its own reference so two amps with the same
        // shape but different output levels still match.
        for s in self.io {
            v.push(((s - self.output_db) / 24.0).clamp(-2.0, 2.0));
        }
        v.push(self.knee * 2.0);
        v.push((self.output_db / 24.0).clamp(-2.0, 2.0));
        v
    }
}

/// Build (or rebuild) the NAM space over every `.nam` under `root`.
/// Returns `(space dir, probed, skipped)`.
pub fn build(root: &Path) -> Result<(PathBuf, usize, usize), String> {
    let mut models: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("nam"))
        .collect();
    models.sort();
    if models.is_empty() {
        return Err(format!("no .nam models under {}", root.display()));
    }
    let mut items = Vec::new();
    let mut features = Vec::new();
    let mut skipped = 0usize;
    for path in &models {
        let p = match probe(path) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(model = %path.display(), "nam space: probe failed: {e}");
                skipped += 1;
                continue;
            }
        };
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
        items.push(SpaceItem {
            path: rel,
            class: archetype(&p).to_string(),
            x: 0.0,
            y: 0.0,
            duration_s: 0.0,
            // Voicing centroid, so the map's "main freq" filter still reads.
            centroid_hz: voicing_centroid(&p),
            rms_db: p.output_db,
            percussiveness: p.knee,
            size_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            mtime_s: 0,
            favorite: false,
        });
        features.extend_from_slice(&p.features());
    }
    if items.is_empty() {
        return Err("no model probed successfully".into());
    }
    let coords = signal_space::project::project_2d(&features, items.len(), DIM);
    for (item, (x, y)) in items.iter_mut().zip(coords) {
        item.x = x;
        item.y = y;
    }
    let probed = items.len();
    let space = Space {
        version: SPACE_VERSION,
        name: NAM_SPACE.to_string(),
        root: root.to_string_lossy().into_owned(),
        dim: DIM,
        items,
    };
    let dir = Space::space_dir(root, NAM_SPACE);
    space.save(&dir, &features)?;
    Ok((dir, probed, skipped))
}

/// Coarse archetype from the measured behaviour — the Rig-Scope idea:
/// derived from analysis, never asserted by the filename.
pub fn archetype(p: &NamProbe) -> &'static str {
    let bright = p.eq[EQ_BANDS - 6..].iter().sum::<f32>() / 6.0;
    let dark = p.eq[..6].iter().sum::<f32>() / 6.0;
    match p.knee {
        k if k < 0.10 => {
            if bright > dark { "clean-bright" } else { "clean-warm" }
        }
        k if k < 0.30 => "crunch",
        k if k < 0.60 => "hi-gain",
        _ => "saturated",
    }
}

/// Centre of the measured voicing, in Hz.
fn voicing_centroid(p: &NamProbe) -> f32 {
    let (mut num, mut den) = (0.0f32, 0.0f32);
    for (b, &v) in p.eq.iter().enumerate() {
        let f = 40.0 * (10_000.0f32 / 40.0).powf(b as f32 / EQ_BANDS as f32);
        let w = 10f32.powf(v * 12.0 / 20.0); // back to linear magnitude
        num += f * w;
        den += w;
    }
    if den > 0.0 { num / den } else { 0.0 }
}

/// Models most similar to `model_path`, best-first.
pub fn similar_to(root: &Path, model_path: &Path, limit: usize) -> Result<Vec<(String, f32)>, String> {
    let dir = Space::space_dir(root, NAM_SPACE);
    let (space, features) = Space::load(&dir)?;
    let rel = model_path
        .strip_prefix(root)
        .unwrap_or(model_path)
        .to_string_lossy()
        .into_owned();
    let idx = space
        .items
        .iter()
        .position(|i| i.path == rel)
        .ok_or_else(|| format!("{rel} not in the NAM space"))?;
    Ok(knn::similar(&features, space.dim, idx, limit, |_| true)
        .into_iter()
        .map(|(i, score)| (space.items[i].path.clone(), score))
        .collect())
}

/// **Partner**: a model that is similar in gain behaviour but deliberately
/// offset in voicing — the stereo-pair pick, not the closest match.
pub fn partner_for(root: &Path, model_path: &Path, limit: usize) -> Result<Vec<(String, f32)>, String> {
    let dir = Space::space_dir(root, NAM_SPACE);
    let (space, features) = Space::load(&dir)?;
    let rel = model_path
        .strip_prefix(root)
        .unwrap_or(model_path)
        .to_string_lossy()
        .into_owned();
    let idx = space
        .items
        .iter()
        .position(|i| i.path == rel)
        .ok_or_else(|| format!("{rel} not in the NAM space"))?;
    let dim = space.dim;
    let q = &features[idx * dim..(idx + 1) * dim];
    let mut scored: Vec<(String, f32)> = (0..space.items.len())
        .filter(|&i| i != idx)
        .map(|i| {
            let r = &features[i * dim..(i + 1) * dim];
            // Gain behaviour (IO curve + knee) should MATCH…
            let gain_d: f32 = q[EQ_BANDS..].iter().zip(&r[EQ_BANDS..]).map(|(a, b)| (a - b).abs()).sum();
            // …while the voicing should differ, but not wildly (a partner is
            // a complement, not a stranger).
            let voice_d: f32 =
                q[..EQ_BANDS].iter().zip(&r[..EQ_BANDS]).map(|(a, b)| (a - b).abs()).sum::<f32>()
                    / EQ_BANDS as f32;
            let sweet = 1.0 - (voice_d - 0.35).abs() / 0.35; // peak at ~0.35
            (space.items[i].path.clone(), sweet.clamp(0.0, 1.0) - gain_d * 0.1)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(limit);
    Ok(scored)
}
