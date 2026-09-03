//! What a hit vocal is actually doing, in the terms the leveller tunes.
//!
//! `level-dsp` rides, gates, de-esses and de-breathes. Each has knobs
//! that currently start from judgement. This runs its own analyser over
//! separated vocal stems and summarises the result, so those knobs can
//! start from what several thousand released vocals do instead.
//!
//! # How far each number can be trusted
//!
//! Measured against ground truth on a real multitrack, a separated
//! vocal reproduces crest within 0.17 dB, its EQ curve within 0.88 dB,
//! and its sibilance band within 0.7 dB. That is the basis for taking
//! these seriously — but it does not apply evenly:
//!
//! - **Riding** is the solid one. Target level and retained range are
//!   level statistics over voiced material, exactly what survives best.
//! - **De-essing** is solid. The sibilance region holds to under a
//!   decibel, so where consonants sit relative to voiced material is
//!   real.
//! - **Gating** is partly real. Phrase and gap *timing* is a property of
//!   the performance and survives. The noise floor does not: a separated
//!   stem's floor is the separator's, not the recording's, so
//!   [`VocalLevelAnalysis::silence_db`] describes the model as much as
//!   the record.
//! - **De-breathing** is the weakest. Breaths are quiet and broadband,
//!   which is what separation smears most. Still worth collecting — a
//!   distribution across thousands of vocals is a better starting point
//!   than a guess, provided nobody mistakes it for ground truth.
//!
//! Those caveats are why this stores distributions rather than a single
//! recommended value: a number with a visible spread invites judgement,
//! where a lone figure invites belief.

use level_dsp::{
    BlockClass, ClassifyConfig, SegmentConfig, analyze,
};
use serde::{Deserialize, Serialize};

/// Percentile summary of a set of decibel readings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
}

impl Spread {
    fn of(mut v: Vec<f64>) -> Option<Spread> {
        if v.is_empty() {
            return None;
        }
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        Some(Spread {
            p10: at(0.10),
            p50: at(0.50),
            p90: at(0.90),
        })
    }
}

/// One vocal stem, described in the leveller's own terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VocalLevelAnalysis {
    // ── riding ───────────────────────────────────────────────────────
    /// Mean level of voiced material — what a rider would aim at.
    pub auto_target_db: Option<f64>,
    /// Spread of voiced block levels. `p90 - p10` is how much range the
    /// vocal still has after whatever riding and compression it got, and
    /// is the number a `max_gain_db` / `max_cut_db` pair has to cover.
    pub voiced_level: Option<Spread>,

    // ── gating ───────────────────────────────────────────────────────
    /// Adaptive silence threshold the analyser settled on, dBFS.
    ///
    /// Describes the separator's noise floor as much as the record's.
    /// Useful as a distribution, misleading as a target.
    pub silence_db: f64,
    /// Share of blocks in each class, summing to 1.
    pub voiced_share: f64,
    pub consonant_share: f64,
    pub silent_share: f64,
    /// Phrase and gap durations in milliseconds — a property of the
    /// performance, so these survive separation well and are the honest
    /// basis for hold and release timing.
    pub phrase_ms: Option<Spread>,
    pub gap_ms: Option<Spread>,

    // ── de-essing ────────────────────────────────────────────────────
    /// Spectral centroid of consonant blocks. Informs `crossover_hz`.
    pub consonant_centroid_hz: Option<Spread>,
    /// How far consonants sit above voiced material, in dB. The gap a
    /// de-esser threshold has to sit inside.
    pub consonant_over_voiced_db: Option<f64>,

    // ── de-breathing ─────────────────────────────────────────────────
    /// Blocks below voiced material but above silence — where breaths
    /// live. Level and centroid map onto `min_level_db` / `max_level_db`
    /// and `max_centroid_hz`.
    ///
    /// The least reliable figures here; see the module docs.
    pub quiet_level_db: Option<Spread>,
    pub quiet_centroid_hz: Option<Spread>,
}

/// Analyse one mono vocal stem.
///
/// Returns `None` when the analyser found no blocks at all, which is
/// what an empty or unreadably short stem looks like — not something to
/// report as a vocal with zero range.
pub fn analyse_vocal(samples: &[f32], sample_rate: f64) -> Option<VocalLevelAnalysis> {
    let mono: Vec<f64> = samples.iter().map(|&s| f64::from(s)).collect();
    let a = analyze(
        &mono,
        sample_rate,
        ClassifyConfig::default(),
        SegmentConfig::default(),
    );
    if a.blocks.is_empty() {
        return None;
    }

    let total = a.blocks.len() as f64;
    let voiced: Vec<&level_dsp::AnalyzedBlock> = a
        .blocks
        .iter()
        .filter(|b| b.class == BlockClass::Tonal)
        .collect();
    let consonant: Vec<&level_dsp::AnalyzedBlock> = a
        .blocks
        .iter()
        .filter(|b| b.class == BlockClass::Consonant)
        .collect();
    let silent = a
        .blocks
        .iter()
        .filter(|b| b.class == BlockClass::Silence)
        .count();

    let voiced_level = Spread::of(voiced.iter().map(|b| b.features.rms_db).collect());

    // Breaths: quieter than voiced material but above the floor. Taken
    // relative to the voiced median rather than an absolute level, so
    // the window means the same thing on a quiet master and a loud one.
    let quiet: Vec<&level_dsp::AnalyzedBlock> = match &voiced_level {
        Some(v) => a
            .blocks
            .iter()
            .filter(|b| {
                b.class != BlockClass::Tonal
                    && b.features.rms_db < v.p50 - 6.0
                    && b.features.rms_db > a.silence_db
            })
            .collect(),
        None => Vec::new(),
    };

    let block_ms = a.block_samples as f64 / sample_rate * 1000.0;
    let (phrases, gaps) = phrase_and_gap_ms(&a.blocks, block_ms);

    let consonant_over_voiced_db = match (
        Spread::of(consonant.iter().map(|b| b.features.rms_db).collect()),
        &voiced_level,
    ) {
        (Some(c), Some(v)) => Some(c.p50 - v.p50),
        _ => None,
    };

    Some(VocalLevelAnalysis {
        auto_target_db: a.auto_target_db,
        voiced_level,
        silence_db: a.silence_db,
        voiced_share: voiced.len() as f64 / total,
        consonant_share: consonant.len() as f64 / total,
        silent_share: silent as f64 / total,
        phrase_ms: Spread::of(phrases),
        gap_ms: Spread::of(gaps),
        consonant_centroid_hz: Spread::of(
            consonant.iter().map(|b| b.features.centroid_hz).collect(),
        ),
        consonant_over_voiced_db,
        quiet_level_db: Spread::of(quiet.iter().map(|b| b.features.rms_db).collect()),
        quiet_centroid_hz: Spread::of(quiet.iter().map(|b| b.features.centroid_hz).collect()),
    })
}

/// Runs of sounding and of silent blocks, in milliseconds.
///
/// Leading and trailing runs are dropped: a stem starting mid-silence
/// would otherwise report a "gap" that is really where the file was cut.
fn phrase_and_gap_ms(blocks: &[level_dsp::AnalyzedBlock], block_ms: f64) -> (Vec<f64>, Vec<f64>) {
    let sounding: Vec<bool> = blocks
        .iter()
        .map(|b| b.class != BlockClass::Silence)
        .collect();

    let mut runs: Vec<(bool, usize)> = Vec::new();
    for &s in &sounding {
        match runs.last_mut() {
            Some((prev, n)) if *prev == s => *n += 1,
            _ => runs.push((s, 1)),
        }
    }
    if runs.len() <= 2 {
        return (Vec::new(), Vec::new());
    }
    let interior = &runs[1..runs.len() - 1];

    let ms = |n: usize| n as f64 * block_ms;
    (
        interior.iter().filter(|(s, _)| *s).map(|(_, n)| ms(*n)).collect(),
        interior.iter().filter(|(s, _)| !*s).map(|(_, n)| ms(*n)).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Alternating tone and silence — a crude stand-in for phrases.
    fn phrased(phrase_s: f64, gap_s: f64, repeats: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..repeats {
            let n = (SR * phrase_s) as usize;
            out.extend((0..n).map(|i| {
                0.3 * (2.0 * std::f64::consts::PI * 220.0 * i as f64 / SR).sin() as f32
            }));
            out.extend(std::iter::repeat_n(0.0, (SR * gap_s) as usize));
        }
        out
    }

    #[test]
    fn an_empty_stem_reports_nothing() {
        assert!(analyse_vocal(&[], SR).is_none());
    }

    #[test]
    fn a_phrased_take_yields_voiced_material_and_gaps() {
        let v = analyse_vocal(&phrased(1.0, 0.5, 4), SR).expect("should analyse");
        assert!(v.voiced_share > 0.2, "voiced share {}", v.voiced_share);
        assert!(v.silent_share > 0.1, "silent share {}", v.silent_share);
        assert!(v.voiced_level.is_some());
        assert!(v.auto_target_db.is_some());
    }

    #[test]
    fn class_shares_account_for_every_block() {
        let v = analyse_vocal(&phrased(1.0, 0.5, 4), SR).unwrap();
        let total = v.voiced_share + v.consonant_share + v.silent_share;
        assert!((total - 1.0).abs() < 1e-9, "shares sum to {total}");
    }

    #[test]
    fn a_spread_is_ordered() {
        let s = Spread::of(vec![1.0, 5.0, 2.0, 9.0, 3.0]).unwrap();
        assert!(s.p10 <= s.p50 && s.p50 <= s.p90, "{s:?}");
    }

    #[test]
    fn a_spread_of_nothing_is_none() {
        // An absent feature must not be reported as a spread of zeros.
        assert!(Spread::of(Vec::new()).is_none());
    }

    /// The edges of a stem are where it was cut, not where the singer
    /// stopped — counting them would inflate every gap statistic.
    #[test]
    fn leading_and_trailing_runs_are_not_counted_as_gaps() {
        let block_ms = 10.0;
        let mk = |sounding: bool| level_dsp::AnalyzedBlock {
            t_sec: 0.0,
            features: level_dsp::BlockFeatures {
                rms: 0.0,
                rms_db: -60.0,
                zcr: 0.0,
                centroid_hz: 0.0,
                flux: 0.0,
            },
            class: if sounding {
                BlockClass::Tonal
            } else {
                BlockClass::Silence
            },
            is_tonal: sounding,
        };
        // silence, phrase, silence, phrase, silence
        let blocks: Vec<_> = [false, true, false, true, false]
            .iter()
            .flat_map(|&s| std::iter::repeat_n(mk(s), 3))
            .collect();

        let (phrases, gaps) = phrase_and_gap_ms(&blocks, block_ms);
        // Only the interior runs survive: two phrases, one gap.
        assert_eq!(phrases.len(), 2, "{phrases:?}");
        assert_eq!(gaps.len(), 1, "{gaps:?}");
    }

    #[test]
    fn a_record_round_trips() {
        let v = analyse_vocal(&phrased(1.0, 0.5, 3), SR).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let back: VocalLevelAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(v.voiced_share, back.voiced_share);
        assert_eq!(v.silence_db, back.silence_db);
    }
}
