//! Demo generators for every feed kind, as pure functions of time: the
//! mock engine's telemetry, previews, tests and offline renders
//! (`render_surface --demo-feeds`) all draw the same signals.
//!
//! [`frame`] makes a frame of any [`FeedInfo`] at `t` seconds, shaped as
//! the info says (bins, len, voices, rate), so a feed resolved against a
//! request ([`FeedInfo::resolve`]) generates the shape that was asked for.
//! A history is the scalar generator sampled backwards from `t` at the
//! feed's rate, so a GR trace and a GR meter agree.

use std::f64::consts::{PI, TAU};

use feed_proto::{FeedFrame, FeedInfo, FeedKind, Range, ScalarKind, Unit, VoiceState};

/// A frame of `info` at `t` seconds.
#[must_use]
pub fn frame(info: &FeedInfo, t: f64) -> FeedFrame {
    FeedFrame {
        id: info.id.clone(),
        values: values(&info.kind, info.rate_hz, t),
    }
}

/// One frame per feed at `t` seconds.
#[must_use]
pub fn frames<'a>(feeds: impl IntoIterator<Item = &'a FeedInfo>, t: f64) -> Vec<FeedFrame> {
    feeds.into_iter().map(|f| frame(f, t)).collect()
}

/// A frame's values for `kind` (published at `rate_hz`) at `t` seconds;
/// always [`FeedKind::frame_len`] long.
#[must_use]
pub fn values(kind: &FeedKind, rate_hz: f64, t: f64) -> Vec<f64> {
    match kind {
        FeedKind::Level { channels } => (0..*channels).map(|c| level_db(c, t)).collect(),
        FeedKind::GainReduction => vec![gr_db(t)],
        FeedKind::Spectrum {
            bins,
            f_min,
            f_max,
            floor_db,
        } => {
            let n = f64::from(*bins);
            let ratio = f_max / f_min.max(1e-3);
            (0..*bins)
                .map(|j| {
                    let hz = f_min * ratio.powf((f64::from(j) + 0.5) / n);
                    (spectrum_db(hz, t) + 1.5 * (t * 5.0 + f64::from(j) * 0.9).sin()).max(*floor_db)
                })
                .collect()
        }
        FeedKind::Tuner => vec![64.0, 8.0 * (t * 0.9).sin()],
        FeedKind::Scalar { range } => vec![scalar(range, t)],
        FeedKind::History { of, len } => {
            let dt = 1.0 / if rate_hz > 0.0 { rate_hz } else { 30.0 };
            let n = *len as usize;
            (0..n)
                .map(|i| history_point(of, t - (n - 1 - i) as f64 * dt))
                .collect()
        }
        FeedKind::Waveform { len } => waveform(*len as usize, t),
        FeedKind::Envelope { stages, voices } => {
            let last = stages.len().max(1) - 1;
            (0..*voices)
                .flat_map(|v| {
                    let mut s = Adsr::DEMO.voice(t, v);
                    s.stage = s.stage.map(|i| i.min(last));
                    s.encode()
                })
                .collect()
        }
    }
}

/// The demo level of `channel`, dBFS.
#[must_use]
pub fn level_db(channel: u8, t: f64) -> f64 {
    -18.0 + 12.0 * (t * 2.3 + f64::from(channel) * 0.7).sin() - 6.0 * (t * 0.37).cos().abs()
}

/// The demo gain reduction, dB (−3…−6).
#[must_use]
pub fn gr_db(t: f64) -> f64 {
    -3.0 - 3.0 * (t * 1.7).sin().abs()
}

/// A demo scalar sweeping `range`.
#[must_use]
pub fn scalar(range: &Range, t: f64) -> f64 {
    range.to_real(0.5 + 0.5 * (t * 0.5).sin())
}

fn history_point(of: &ScalarKind, t: f64) -> f64 {
    match of {
        ScalarKind::Level => level_db(0, t).max(level_db(1, t)),
        ScalarKind::GainReduction => gr_db(t),
        ScalarKind::Scalar { range } => scalar(range, t),
    }
}

/// A mix's spectrum at `hz`, dBFS: a pink-ish slope with a pulsing kick
/// near 55 Hz, a tone gliding around 440 Hz and a presence bump at 3 kHz.
#[must_use]
pub fn spectrum_db(hz: f64, t: f64) -> f64 {
    let oct = |a: f64, b: f64| (a / b).log2();
    let bump = |center: f64, width_oct: f64, gain_db: f64| {
        gain_db * (-(oct(hz, center) / width_oct).powi(2)).exp()
    };
    let slope = -30.0 - 3.0 * oct(hz, 1000.0) - 12.0 * (-(oct(hz, 30.0) * 1.5)).exp().min(4.0);
    let kick = bump(55.0, 0.4, 20.0 * (t * PI * 2.0).sin().abs().powi(4));
    let tone = bump(440.0 * (0.5 * (t * 0.3).sin()).exp2(), 1.0 / 12.0, 22.0);
    let presence = bump(3000.0, 1.0, 6.0);
    slope + kick + tone + presence
}

/// `len` samples of a guitar-ish wave, three periods, starting at its
/// upward crossing (as a triggered scope shows it), −1..1.
#[must_use]
pub fn waveform(len: usize, t: f64) -> Vec<f64> {
    let amp = 0.6 + 0.35 * (t * 0.8).sin().abs();
    let n = len.max(1) as f64;
    (0..len)
        .map(|i| {
            let ph = TAU * 3.0 * i as f64 / n;
            let x =
                0.7 * ph.sin() + 0.25 * (2.0 * ph).sin() * (t * 1.3).cos() + 0.1 * (3.0 * ph).sin();
            (amp * x).clamp(-1.0, 1.0)
        })
        .collect()
}

/// An ADSR, seconds and a sustain level, played by voices that each
/// retrigger every [`Adsr::cycle`] seconds, staggered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
    /// How long the key is held (attack and decay included).
    pub gate: f64,
    /// Each voice's note-on is this much later than the previous voice's.
    pub stagger: f64,
}

impl Adsr {
    pub const DEMO: Self = Self {
        attack: 0.25,
        decay: 0.35,
        sustain: 0.6,
        release: 0.6,
        gate: 1.2,
        stagger: 0.7,
    };

    /// Note-on to note-on.
    #[must_use]
    pub fn cycle(&self) -> f64 {
        self.gate + self.release + 1.0
    }

    /// The playhead `since` seconds after note-on (stages A=0, D=1, S=2,
    /// R=3; idle after the release).
    #[must_use]
    pub fn at(&self, since: f64) -> VoiceState {
        let (a, d, s, r) = (
            self.attack.max(1e-6),
            self.decay.max(1e-6),
            self.sustain,
            self.release.max(1e-6),
        );
        let hold = (self.gate - a - d).max(1e-6);
        let state = |stage, position: f64, level| VoiceState {
            stage: Some(stage),
            position: position.clamp(0.0, 1.0),
            level,
        };
        if since < 0.0 {
            VoiceState {
                stage: None,
                position: 0.0,
                level: 0.0,
            }
        } else if since < a {
            state(0, since / a, since / a)
        } else if since < a + d {
            let p = (since - a) / d;
            state(1, p, 1.0 - (1.0 - s) * p)
        } else if since < a + d + hold {
            state(2, (since - a - d) / hold, s)
        } else if since < a + d + hold + r {
            let p = (since - a - d - hold) / r;
            state(3, p, s * (1.0 - p))
        } else {
            VoiceState {
                stage: None,
                position: 0.0,
                level: 0.0,
            }
        }
    }

    /// Voice `voice`'s playhead at `t`.
    #[must_use]
    pub fn voice(&self, t: f64, voice: u8) -> VoiceState {
        self.at((t - f64::from(voice) * self.stagger).rem_euclid(self.cycle()))
    }
}

/// One feed of every kind (ids under `demo/`), for previews and tests.
#[must_use]
pub fn demo_feeds() -> Vec<FeedInfo> {
    vec![
        FeedInfo::new("demo/in", "Input", FeedKind::Level { channels: 2 }),
        FeedInfo::new("demo/gr", "Gain Reduction", FeedKind::GainReduction),
        FeedInfo::new("demo/gr_trace", "GR History", FeedKind::gr_history(256)),
        FeedInfo::new(
            "demo/in_trace",
            "Input History",
            FeedKind::History {
                of: ScalarKind::Level,
                len: 256,
            },
        ),
        FeedInfo::new("demo/spectrum", "Spectrum", FeedKind::spectrum(96)),
        FeedInfo::new("demo/scope", "Scope", FeedKind::Waveform { len: 512 }),
        FeedInfo::new("demo/env", "Envelope", FeedKind::adsr(4)).rate(60.0),
        FeedInfo::new("demo/tuner", "Tuner", FeedKind::Tuner),
        FeedInfo::new(
            "demo/lfo",
            "LFO",
            FeedKind::Scalar {
                range: Range::linear(0.0, 1.0, Unit::None),
            },
        )
        .rate(60.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use feed_proto::FeedRequest;

    #[test]
    fn every_kind_generates_its_frame_len() {
        for info in demo_feeds() {
            for t in [0.0, 0.37, 12.5] {
                let f = frame(&info, t);
                assert_eq!(f.values.len(), info.kind.frame_len(), "{}", info.id);
                assert!(f.values.iter().all(|v| v.is_finite()), "{}", info.id);
            }
        }
    }

    #[test]
    fn requests_shape_the_generated_frames() {
        let spec = &demo_feeds()[4];
        let asked = spec.resolve(&FeedRequest::new("demo/spectrum").bins(300).options);
        assert_eq!(frame(&asked, 1.0).values.len(), 300);
        let trace = &demo_feeds()[2];
        let asked = trace.resolve(&FeedRequest::new("demo/gr_trace").len(10).options);
        assert_eq!(frame(&asked, 1.0).values.len(), 10);
    }

    #[test]
    fn a_history_ends_at_the_live_value() {
        let trace = FeedInfo::new("gr", "GR", FeedKind::gr_history(64)).rate(20.0);
        let v = frame(&trace, 3.0).values;
        assert_eq!(*v.last().unwrap(), gr_db(3.0));
        assert_eq!(v[62], gr_db(3.0 - 0.05), "one point per 1/rate s");
        assert!(v.iter().all(|g| (-6.0..=-3.0).contains(g)));
    }

    #[test]
    fn the_adsr_walks_its_stages() {
        let e = Adsr::DEMO;
        let stages: Vec<_> = [0.1, 0.4, 1.0, 1.5, 2.0, 2.9]
            .iter()
            .map(|t| e.at(*t).stage)
            .collect();
        assert_eq!(stages, [Some(0), Some(1), Some(2), Some(3), None, None]);
        assert!(
            (e.at(0.6).level - e.sustain).abs() < 1e-9,
            "decay lands on sustain"
        );
        assert!(e.at(1.2 + 0.6 - 1e-9).level < 1e-6, "release lands on zero");
        // Voices are staggered.
        assert_ne!(e.voice(1.0, 0), e.voice(1.0, 1));
    }

    #[test]
    fn the_spectrum_has_its_tone() {
        let t = 0.0; // tone at 440 Hz
        assert!(spectrum_db(440.0, t) > spectrum_db(600.0, t) + 10.0);
        assert!(waveform(256, 1.0).iter().all(|x| (-1.0..=1.0).contains(x)));
    }
}
