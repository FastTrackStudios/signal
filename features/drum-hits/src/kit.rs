//! A whole kit at once: every piece's hits, in parallel.
//!
//! This is what a caller runs — on a worker, never a window's thread —
//! with every piece's close mic and trigger read whole at their own
//! rate. It goes in two rounds, because a piece without a working
//! trigger needs to know when the others were struck:
//!
//! 1. Each piece with a trigger, in parallel: the trigger's spikes, the
//!    mic scanned and calibrated against them, the trigger judged
//!    ([`crate::health`]), and — when it is believed — the two fused.
//! 2. Each piece left over (no trigger, a dead one, or told to trust its
//!    mic), in parallel: its mic alone, refusing the bleed of every other
//!    mic, learned at the other pieces' strokes from round one.
//!
//! Spec: `r[hits.detect.calibrate]`, `r[hits.detect.fusion]`,
//! `r[hits.detect.no-trigger]`, `r[hits.detect.trigger-health]`.

use rayon::prelude::*;

use crate::bleed::{Other, confident, without_trigger};
use crate::calibrate::{Calibration, TOLERANCE, agree, calibrate};
use crate::fuse::{Hit, corroborate, fuse};
use crate::gate::{Gate, Scan, gate};
use crate::trigger::{Health, health, spikes};

/// One piece's signals, on one clock and at one rate.
#[derive(Clone, Copy, Debug)]
pub struct Piece<'a> {
    pub mic: &'a [f32],
    pub trigger: Option<&'a [f32]>,
}

/// Which source a piece's hits come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trust {
    /// The trigger when it is working, else the mic alone.
    #[default]
    Auto,
    /// The trigger, even when it disagrees with its mic.
    Trigger,
    /// The mic alone, whatever the trigger says.
    Mic,
}

/// How a kit is detected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// After a hit, how long until the next can start, seconds.
    pub retrig_secs: f64,
    /// How far under its loudest a trigger spike still counts, dB.
    pub trigger_under_db: f64,
    /// A fixed gate for every mic, instead of calibrating each against
    /// its trigger.
    pub gate: Option<Gate>,
    /// The crest a mic with no trigger is read at, dB.
    pub crest_db: f64,
    pub trust: Trust,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retrig_secs: 0.04,
            trigger_under_db: 24.0,
            gate: None,
            crest_db: 5.0,
            trust: Trust::Auto,
        }
    }
}

/// How a piece's hits were found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// Its trigger and its calibrated mic, fused.
    Fused,
    /// Its mic alone, the others' bleed refused.
    MicAlone,
}

/// One piece's result.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub hits: Vec<Hit>,
    pub method: Method,
    /// The trigger's verdict; `None` without one (or told to trust the
    /// mic).
    pub trigger: Option<Health>,
    /// The mic's gate and how well it agreed with the trigger; `None`
    /// without a trigger.
    pub calibration: Option<Calibration>,
}

/// Round one's answer for a piece.
struct RoundOne {
    /// Its mic, walked once — for round two as well.
    scan: Scan,
    /// Its trigger's verdict.
    trigger: Option<Health>,
    /// Its mic's gate against the trigger.
    calibration: Option<Calibration>,
    /// Its hits, when the trigger settled them.
    hits: Option<Vec<Hit>>,
}

/// The hits of `g` on a scanned mic: from the scan when the setting is
/// on its grid, else one more walk.
fn gate_on(scan: &Scan, mic: &[f32], g: &Gate) -> Vec<usize> {
    scan.gate_at(g).unwrap_or_else(|| gate(mic, scan.rate(), g))
}

/// Round one for one piece.
fn with_trigger(piece: &Piece<'_>, rate: f64, settings: &Settings) -> RoundOne {
    let scan = Scan::of(piece.mic, rate);
    let Some(trigger) = piece.trigger.filter(|_| settings.trust != Trust::Mic) else {
        return RoundOne {
            scan,
            trigger: None,
            calibration: None,
            hits: None,
        };
    };
    let truth = spikes(
        trigger,
        rate,
        settings.trigger_under_db,
        settings.retrig_secs,
    );
    let calibration = settings.gate.map_or_else(
        || calibrate(&scan, &truth, settings.retrig_secs),
        |g| {
            Some(Calibration {
                gate: g,
                agreement: agree(
                    &gate_on(&scan, piece.mic, &g),
                    &truth,
                    crate::samples(TOLERANCE, rate),
                ),
            })
        },
    );
    let verdict = if settings.trust == Trust::Trigger {
        Health::Working
    } else {
        health(calibration.as_ref())
    };
    let hits = match verdict {
        Health::Working => {
            let mic_hits =
                calibration.map_or_else(Vec::new, |c| gate_on(&scan, piece.mic, &c.gate));
            let mut hits = fuse(piece.mic, rate, &truth, &mic_hits);
            corroborate(&mut hits, trigger, rate);
            Some(hits)
        }
        Health::MicSilent(_) => Some(fuse(piece.mic, rate, &truth, &[])),
        Health::Dead(_) => None,
    };
    RoundOne {
        scan,
        trigger: Some(verdict),
        calibration,
        hits,
    }
}

/// Every piece's hits. `extra` are close mics with no piece of their
/// own (hi-hat, ride): bleed to refuse, nothing to detect.
#[must_use]
pub fn detect_kit(
    pieces: &[Piece<'_>],
    extra: &[&[f32]],
    rate: f64,
    settings: &Settings,
) -> Vec<Found> {
    let first: Vec<RoundOne> = pieces
        .par_iter()
        .map(|p| with_trigger(p, rate, settings))
        .collect();

    // Round two: only when some piece needs its mic alone.
    let alone: Vec<usize> = first
        .iter()
        .enumerate()
        .filter(|(_, r)| r.hits.is_none())
        .map(|(i, _)| i)
        .collect();
    let found_alone: Vec<(usize, Vec<Hit>)> = if alone.is_empty() {
        Vec::new()
    } else {
        // Every mic's strokes: its fused hits, else its confident moments.
        let mics: Vec<&[f32]> = pieces
            .iter()
            .map(|p| p.mic)
            .chain(extra.iter().copied())
            .collect();
        let extra_scans: Vec<Scan> = extra.par_iter().map(|m| Scan::of(m, rate)).collect();
        let scans: Vec<&Scan> = first.iter().map(|r| &r.scan).chain(&extra_scans).collect();
        let strokes: Vec<Vec<usize>> = mics
            .par_iter()
            .zip(&scans)
            .enumerate()
            .map(|(i, (mic, scan))| {
                first.get(i).and_then(|r| r.hits.as_ref()).map_or_else(
                    || confident(mic, scan, settings.crest_db).0,
                    |hits| hits.iter().map(|h| h.at).collect(),
                )
            })
            .collect();
        alone
            .par_iter()
            .filter_map(|&i| {
                let own = pieces.get(i)?;
                let scan = scans.get(i)?;
                let others: Vec<Other<'_>> = mics
                    .iter()
                    .zip(&strokes)
                    .enumerate()
                    .filter(|(n, _)| *n != i)
                    .map(|(_, (samples, strokes))| Other { samples, strokes })
                    .collect();
                Some((
                    i,
                    without_trigger(own.mic, scan, &others, settings.crest_db),
                ))
            })
            .collect()
    };

    first
        .into_iter()
        .enumerate()
        .map(|(i, r)| match r.hits {
            Some(hits) => Found {
                hits,
                method: Method::Fused,
                trigger: r.trigger,
                calibration: r.calibration,
            },
            None => Found {
                hits: found_alone
                    .iter()
                    .find(|(n, _)| *n == i)
                    .map(|(_, h)| h.clone())
                    .unwrap_or_default(),
                method: Method::MicAlone,
                trigger: r.trigger,
                calibration: r.calibration,
            },
        })
        .collect()
}
