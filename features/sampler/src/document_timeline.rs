//! A host's track timeline (`com.fasttrackstudio.host-timeline/1`,
//! daw-clap-ext `docs/host-timeline.md`) as the sampler's
//! [`TrackDocument`]: what document mode plays ahead of the playhead.
//!
//! - Notes and CCs keep their channel: channel *n* is engine line *n*, so
//!   each line pre-fires its own notes by its own leads.
//! - Release velocity, pitch bend, pressure and program changes are
//!   carried by the timeline but have no place in a document yet (the
//!   sampler's document plays notes and CCs); they are dropped here.
//! - The tempo map: a step stays a step. A ramp (linear in QN) becomes
//!   [`RAMP_STEP_QN`] steps whose each duration is the ramp's exact
//!   integral over it, so every step boundary lands where the host's
//!   ramp puts it; inside a step a note is off by a fraction of a frame.

use daw_clap_ext::timeline::{Control, TempoShape, Timeline};

use crate::document::{DocCc, DocNote, TempoPoint, TrackDocument};

/// The step a tempo ramp is cut into, QN.
pub const RAMP_STEP_QN: f64 = 1.0 / 64.0;

/// `timeline` as a document, with `seed` for its round-robin choices.
#[must_use]
pub fn document_from_timeline(timeline: &Timeline, seed: u64) -> TrackDocument {
    TrackDocument {
        version: timeline.revision,
        seed,
        notes: timeline
            .notes
            .iter()
            .filter(|n| n.end_qn > n.start_qn && n.start_qn >= 0.0)
            .map(|n| DocNote {
                start_qn: n.start_qn,
                end_qn: n.end_qn,
                chan: n.channel,
                pitch: n.key,
                vel: n.velocity,
            })
            .collect(),
        ccs: timeline
            .ctrls
            .iter()
            .filter_map(|c| match c.control {
                Control::Cc { controller, value } => Some(DocCc {
                    qn: c.qn.max(0.0),
                    chan: c.channel,
                    cc: controller,
                    val: value,
                }),
                _ => None,
            })
            .collect(),
        tempo: tempo_steps(timeline),
        auto_divisi: false,
    }
}

/// The timeline's tempo map as the document's piecewise-constant one.
fn tempo_steps(timeline: &Timeline) -> Vec<TempoPoint> {
    let mut out = Vec::with_capacity(timeline.tempo.len());
    for (i, t) in timeline.tempo.iter().enumerate() {
        let next = timeline.tempo.get(i + 1);
        match (t.shape, next) {
            (TempoShape::Ramp, Some(n)) if n.qn > t.qn && (n.bpm - t.bpm).abs() > 1e-12 => {
                out.extend(ramp_steps(t.qn, t.bpm, n.qn, n.bpm));
            }
            _ => out.push(TempoPoint {
                qn: t.qn,
                bpm: t.bpm,
            }),
        }
    }
    out
}

/// A linear ramp from `b0` bpm at `q0` to `b1` at `q1` as constant steps:
/// each step's bpm is its length over its exact duration.
fn ramp_steps(q0: f64, b0: f64, q1: f64, b1: f64) -> impl Iterator<Item = TempoPoint> {
    let steps = ((q1 - q0) / RAMP_STEP_QN).ceil().max(1.0) as u32;
    let bpm_at = move |q: f64| (b1 - b0).mul_add((q - q0) / (q1 - q0), b0);
    // ∫ 60 / bpm(q) dq over [a, b] = 60 (q1 − q0) / (b1 − b0) · ln(bpm(b) / bpm(a)).
    let seconds = move |a: f64, b: f64| 60.0 * (q1 - q0) / (b1 - b0) * (bpm_at(b) / bpm_at(a)).ln();
    (0..steps).map(move |i| {
        let a = q0 + f64::from(i) * RAMP_STEP_QN;
        let b = (a + RAMP_STEP_QN).min(q1);
        TempoPoint {
            qn: a,
            bpm: 60.0 * (b - a) / seconds(a, b),
        }
    })
}

#[cfg(test)]
mod tests {
    use daw_clap_ext::timeline::{Ctrl, Note, Tempo};

    use super::*;
    use crate::document::qn_to_sec;

    fn tempo(qn: f64, seconds: f64, bpm: f64, shape: TempoShape) -> Tempo {
        Tempo {
            qn,
            seconds,
            bpm,
            numerator: 4,
            denominator: 4,
            shape,
        }
    }

    #[test]
    fn notes_and_ccs_keep_their_channels() {
        let t = Timeline {
            revision: 5,
            end_qn: 8.0,
            notes: vec![
                Note {
                    start_qn: 0.0,
                    end_qn: 2.1,
                    channel: 0,
                    key: 60,
                    velocity: 90,
                    release_velocity: 64,
                },
                Note {
                    start_qn: 2.0,
                    end_qn: 4.0,
                    channel: 2,
                    key: 62,
                    velocity: 30,
                    release_velocity: 64,
                },
            ],
            ctrls: vec![
                Ctrl {
                    qn: 0.0,
                    channel: 2,
                    control: Control::Cc {
                        controller: 58,
                        value: 2,
                    },
                },
                Ctrl {
                    qn: 1.0,
                    channel: 2,
                    control: Control::PitchBend(9000),
                },
            ],
            tempo: vec![tempo(0.0, 0.0, 120.0, TempoShape::Step)],
        };
        let doc = document_from_timeline(&t, 42);
        assert_eq!((doc.version, doc.seed), (5, 42));
        assert_eq!(
            doc.notes
                .iter()
                .map(|n| (n.chan, n.pitch))
                .collect::<Vec<_>>(),
            vec![(0, 60), (2, 62)]
        );
        assert_eq!(doc.ccs.len(), 1, "the bend has no place in a document");
        assert_eq!((doc.ccs[0].chan, doc.ccs[0].cc), (2, 58));
    }

    #[test]
    fn a_ramp_lands_every_step_where_the_host_puts_it() {
        // 120 → 60 bpm over 8 QN, then steady: the ramp's exact duration
        // is 60·8/(60−120)·ln(60/120) = 5.545 s.
        let exact = 60.0 * 8.0 / (60.0 - 120.0) * (60.0f64 / 120.0).ln();
        let t = Timeline {
            tempo: vec![
                tempo(0.0, 0.0, 120.0, TempoShape::Ramp),
                tempo(8.0, exact, 60.0, TempoShape::Step),
            ],
            ..Timeline::default()
        };
        let doc = document_from_timeline(&t, 0);
        assert_eq!(doc.tempo.len(), 8 * 64 + 1);
        let at = |qn| qn_to_sec(&doc.tempo, qn);
        assert!((at(8.0) - exact).abs() < 1e-9, "{} vs {exact}", at(8.0));
        // Mid-ramp, on a step boundary: the exact integral.
        let half = 60.0 * 8.0 / -60.0 * (90.0f64 / 120.0).ln();
        assert!((at(4.0) - half).abs() < 1e-9);
        // Past the ramp: steady 60 bpm.
        assert!((at(10.0) - (exact + 2.0)).abs() < 1e-9);
    }
}
