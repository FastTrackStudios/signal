//! A trigger's hits, and whether the trigger can be believed.
//!
//! A trigger is a contact mic on the drum: one spike a stroke, almost no
//! bleed, so its hits are found by level alone — a spike within
//! `under_db` of the trigger's loudest, `retrig` after the last. That is
//! `session-daw`'s `drum_edit::levels`, on one signal.
//!
//! A trigger can also be dead or noisy — Unbreakable's floor-tom trigger
//! fires about eighteen times a second for the whole song — and a dead
//! trigger used as the truth makes every hit on its piece wrong. So a
//! trigger is checked before it is believed ([`health`]).

/// Where `samples` spike: a sample at most `under_db` under the loudest
/// starts a hit, `retrig_secs` after the last.
#[must_use]
pub fn spikes(samples: &[f32], rate: f64, under_db: f64, retrig_secs: f64) -> Vec<usize> {
    let loudest = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if loudest <= 0.0 {
        return Vec::new();
    }
    let threshold = f64::from(loudest) * super::gain(-under_db.abs());
    let skip = super::samples(retrig_secs, rate).max(1);
    let mut out = Vec::new();
    let mut i = 0_usize;
    while let Some(s) = samples.get(i) {
        if f64::from(s.abs()) >= threshold {
            out.push(i);
            i = i.saturating_add(skip);
        } else {
            i = i.saturating_add(1);
        }
    }
    out
}

/// Whether a trigger is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Health {
    /// It agrees with its close mic: its hits are the piece's.
    Working,
    /// It has hits and its mic has almost none: the mic was off, or
    /// silent, and the trigger alone is believed.
    MicSilent(String),
    /// It is not believed, and why.
    Dead(String),
}

/// The least agreement (F1) between a trigger and its calibrated mic
/// for the trigger to be believed. A working trigger reached 0.92–1.0
/// against its mic on every Unbreakable piece; the dead one, 0.26.
pub const AGREES: f64 = 0.6;

/// A mic finding under this share of its trigger's hits is not hearing
/// the drum at all.
pub const SILENT: f64 = 0.2;

/// Judge a trigger by how well the best-calibrated close mic agrees with
/// it. `None` (no mic, or nothing calibrated) believes the trigger.
///
/// Disagreement is the trigger's fault unless the mic found almost
/// nothing: a mic that was off says nothing about the trigger.
#[must_use]
pub fn health(best: Option<&super::Calibration>) -> Health {
    match best {
        Some(c)
            if c.agreement.f1() < AGREES
                && super::seconds(c.agreement.found, 1.0)
                    < SILENT * super::seconds(c.agreement.truth, 1.0) =>
        {
            Health::MicSilent(format!(
                "its mic found {} of its {} hits: the trigger alone is used",
                c.agreement.found, c.agreement.truth
            ))
        }
        Some(c) if c.agreement.f1() < AGREES => Health::Dead(format!(
            "agrees with its mic on {:.0}% (needs {:.0}%): its {} hits against the mic's {}",
            c.agreement.f1() * 100.0,
            AGREES * 100.0,
            c.agreement.truth,
            c.agreement.found,
        )),
        _ => Health::Working,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_hit_per_spike() {
        let rate = 48_000.0;
        let mut sig = vec![0.0_f32; 48_000];
        for at in [1_000, 10_000, 30_000] {
            for i in 0..50 {
                sig[at + i] = num_traits::ToPrimitive::to_f32(&i)
                    .unwrap()
                    .mul_add(-0.01, 0.9);
            }
        }
        assert_eq!(spikes(&sig, rate, 24.0, 0.04), vec![1_000, 10_000, 30_000]);
    }
}
