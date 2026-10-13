//! Offline whole-item analysis → volume envelope.
//!
//! This is the Melodyne/macro-style path: hand it the entire mono buffer of a
//! take, and it returns a per-block analysis plus a smoothed gain envelope that
//! rides tonal material toward a target level. Callers turn the envelope into a
//! DAW volume automation lane or clip-gain (the `daw` layer owns that
//! mapping). Because it sees the whole signal it can compute an adaptive silence
//! floor and an auto-target — things the realtime [`crate::VocalRider`] must
//! approximate.
//!
//! Clean-room reimplementation of the reference macro's analysis + gain stages.

use audiocore_dsp::db::db_to_linear;

use crate::classify::{
    adaptive_silence_db, smooth_tonal_mask, BlockClass, BlockFeatures, Classifier, ClassifyConfig,
};
use crate::segment::{auto_target_db, build_segments, Segment, SegmentConfig};

/// One analysed block with its timeline position.
#[derive(Clone, Copy, Debug)]
pub struct AnalyzedBlock {
    /// Block start time, seconds from the buffer start.
    pub t_sec: f64,
    /// Extracted features.
    pub features: BlockFeatures,
    /// Class after temporal smoothing.
    pub class: BlockClass,
    /// Whether this block ended up tonal (post gap-bridging).
    pub is_tonal: bool,
}

/// Full offline analysis of a take.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// Per-block results.
    pub blocks: Vec<AnalyzedBlock>,
    /// Retained tonal segments.
    pub segments: Vec<Segment>,
    /// Adaptive silence threshold used, dBFS.
    pub silence_db: f64,
    /// Auto-computed target (mean tonal RMS), dBFS, if any tonal material.
    pub auto_target_db: Option<f64>,
    /// Samples per analysis block.
    pub block_samples: usize,
    /// Sample rate the analysis ran at.
    pub sample_rate: f64,
}

/// A point on the rendered gain envelope.
#[derive(Clone, Copy, Debug)]
pub struct GainPoint {
    /// Time in seconds from the buffer start.
    pub t_sec: f64,
    /// Gain in dB to apply at this time.
    pub gain_db: f64,
}

/// Gain-rendering options for the offline path.
#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    /// Target level for tonal blocks, dBFS. `None` ⇒ use the auto-target.
    pub target_db: Option<f64>,
    /// Correction amount, 0..1.
    pub amount: f64,
    /// Maximum boost, dB.
    pub max_gain_db: f64,
    /// Maximum cut, dB (positive magnitude).
    pub max_cut_db: f64,
    /// Blocks quieter than this ride nothing (0 dB), dBFS.
    pub gate_db: f64,
    /// Per-block (true) vs per-segment (false) gain resolution.
    pub per_block: bool,
    /// Exponential smoothing factor for the double pass, 0..1.
    pub smooth_alpha: f64,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            target_db: None,
            amount: 1.0,
            max_gain_db: 12.0,
            max_cut_db: 18.0,
            gate_db: -40.0,
            per_block: true,
            smooth_alpha: 0.25,
        }
    }
}

/// Analyse a whole mono buffer.
pub fn analyze(
    samples: &[f64],
    sample_rate: f64,
    class_cfg: ClassifyConfig,
    seg_cfg: SegmentConfig,
) -> Analysis {
    let mut classifier = Classifier::new(sample_rate, class_cfg);
    let bs = classifier.block_samples();
    let n_blocks = samples.len() / bs;

    let mut features: Vec<BlockFeatures> = Vec::with_capacity(n_blocks);
    for b in 0..n_blocks {
        let block = &samples[b * bs..b * bs + bs];
        features.push(classifier.analyze_block(block));
    }

    let rms: Vec<f64> = features.iter().map(|f| f.rms).collect();
    let silence_db = adaptive_silence_db(&rms);

    // Raw tonal mask, then temporal smoothing.
    let raw: Vec<bool> = features
        .iter()
        .map(|f| classifier.classify(f, silence_db) == BlockClass::Tonal)
        .collect();
    let mut is_tonal = smooth_tonal_mask(&raw, class_cfg.smooth_frames);

    let segments = build_segments(&mut is_tonal, &rms, class_cfg.block_ms, seg_cfg, &class_cfg);
    let auto_target = auto_target_db(&segments);

    let blocks = features
        .iter()
        .enumerate()
        .map(|(i, &features)| AnalyzedBlock {
            t_sec: (i * bs) as f64 / sample_rate,
            features,
            class: classifier.classify(&features, silence_db),
            is_tonal: is_tonal[i],
        })
        .collect();

    Analysis {
        blocks,
        segments,
        silence_db,
        auto_target_db: auto_target,
        block_samples: bs,
        sample_rate,
    }
}

/// Render a smoothed gain envelope from an analysis.
///
/// One [`GainPoint`] per block; tonal blocks are ridden toward the target
/// (log-domain, like a compressor's makeup), non-tonal blocks stay at 0 dB.
/// A forward+backward exponential pass removes intra-segment stepping.
pub fn render_gain_envelope(analysis: &Analysis, cfg: RenderConfig) -> Vec<GainPoint> {
    let n = analysis.blocks.len();
    let mut gain_db = vec![0.0f64; n];

    let target_db = cfg.target_db.or(analysis.auto_target_db).unwrap_or(-18.0);

    for seg in &analysis.segments {
        if cfg.per_block {
            #[allow(clippy::needless_range_loop)]
            for i in seg.from..=seg.to {
                let bl_db = analysis.blocks[i].features.rms_db;
                if bl_db > cfg.gate_db {
                    let delta = (target_db - bl_db) * cfg.amount;
                    gain_db[i] = delta.clamp(-cfg.max_cut_db, cfg.max_gain_db);
                }
            }
        } else if seg.avg_rms_db > cfg.gate_db {
            let delta = (target_db - seg.avg_rms_db) * cfg.amount;
            let g = delta.clamp(-cfg.max_cut_db, cfg.max_gain_db);
            #[allow(clippy::needless_range_loop)]
            for i in seg.from..=seg.to {
                gain_db[i] = g;
            }
        }
    }

    // Double-pass exponential smoothing within each segment.
    if cfg.per_block {
        let a = cfg.smooth_alpha;
        for seg in &analysis.segments {
            if seg.to > seg.from {
                let mut cur = gain_db[seg.from];
                #[allow(clippy::needless_range_loop)]
                for i in (seg.from + 1)..=seg.to {
                    cur += a * (gain_db[i] - cur);
                    gain_db[i] = cur;
                }
                let mut cur = gain_db[seg.to];
                for i in (seg.from..seg.to).rev() {
                    cur += a * (gain_db[i] - cur);
                    gain_db[i] = cur;
                }
            }
        }
    }

    analysis
        .blocks
        .iter()
        .zip(gain_db)
        .map(|(b, g)| GainPoint {
            t_sec: b.t_sec,
            gain_db: g,
        })
        .collect()
}

/// Convenience: apply a rendered envelope destructively to a mono buffer,
/// linearly interpolating gain between block points. Useful for offline render
/// and for verifying the envelope end-to-end in tests.
pub fn apply_envelope(samples: &mut [f64], env: &[GainPoint], sample_rate: f64) {
    if env.is_empty() {
        return;
    }
    for (i, s) in samples.iter_mut().enumerate() {
        let t = i as f64 / sample_rate;
        // Find bracketing envelope points (env is time-sorted, small; linear scan
        // is fine for tests / offline render).
        let mut g_db = env[0].gain_db;
        for w in env.windows(2) {
            if t >= w[0].t_sec && t < w[1].t_sec {
                let span = (w[1].t_sec - w[0].t_sec).max(1e-9);
                let frac = (t - w[0].t_sec) / span;
                g_db = w[0].gain_db + frac * (w[1].gain_db - w[0].gain_db);
                break;
            } else if t >= env[env.len() - 1].t_sec {
                g_db = env[env.len() - 1].gain_db;
            }
        }
        *s *= db_to_linear(g_db);
    }
}
