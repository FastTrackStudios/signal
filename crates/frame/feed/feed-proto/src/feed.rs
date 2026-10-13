//! Feed kinds, catalog entries, requests and frames.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::EntryId;
use crate::range::Range;

/// Largest spectrum a request may ask for.
pub const MAX_BINS: u16 = 4096;
/// Longest history or waveform a request may ask for.
pub const MAX_LEN: u32 = 1 << 16;
/// Fastest publish rate a request may ask for.
pub const MAX_RATE_HZ: f64 = 240.0;

const fn default_f_min() -> f64 {
    20.0
}
const fn default_f_max() -> f64 {
    20_000.0
}
const fn default_floor_db() -> f64 {
    -90.0
}

/// What a feed carries — and so how a frame's `values` are laid out
/// ([`Self::frame_len`]):
///
/// | kind | `values` |
/// |---|---|
/// | `Level { channels }` | dBFS per channel |
/// | `GainReduction` | `[dB]`, ≤ 0 |
/// | `Spectrum { bins, .. }` | dB per log-spaced bin, `f_min` → `f_max` |
/// | `Tuner` | `[midi_note, cents]` |
/// | `Scalar { range }` | `[v]` in `range`'s units |
/// | `History { of, len }` | the last `len` values of `of`, oldest first |
/// | `Waveform { len }` | `len` samples, −1..1 |
/// | `Envelope { stages, voices }` | `voices` × `[stage, position, level]` |
///
/// Variant order is wire order: new kinds go at the end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum FeedKind {
    /// Peak level in dBFS, per channel.
    Level { channels: u8 },
    /// Gain reduction in dB (≤ 0).
    GainReduction,
    /// Magnitude spectrum in dB over `bins` log-spaced bins from `f_min` to
    /// `f_max` Hz; values at or below `floor_db` read as silence. (Catalogs
    /// written before the band fields default to 20 Hz–20 kHz, −90 dB.)
    Spectrum {
        bins: u16,
        #[serde(default = "default_f_min")]
        #[facet(default = 20.0)]
        f_min: f64,
        #[serde(default = "default_f_max")]
        #[facet(default = 20_000.0)]
        f_max: f64,
        #[serde(default = "default_floor_db")]
        #[facet(default = -90.0)]
        floor_db: f64,
    },
    /// Detected pitch: note, cents offset.
    Tuner,
    /// Any scalar readout (tempo, CPU %, an LFO).
    Scalar { range: Range },
    /// A scrolling trace: the last `len` values of a scalar feed (gain
    /// reduction over time, input level over time).
    History { of: ScalarKind, len: u32 },
    /// A scope: `len` samples of the signal, −1..1, triggered.
    Waveform { len: u32 },
    /// Envelope playheads: per voice, which stage (an index into `stages`,
    /// −1 when the voice is idle), how far through it (0..1) and the
    /// envelope's level (0..1).
    Envelope { stages: Vec<String>, voices: u8 },
}

/// What a [`FeedKind::History`] records: one value per point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ScalarKind {
    /// A level in dBFS (the loudest channel).
    Level,
    /// Gain reduction in dB (≤ 0).
    GainReduction,
    /// A value in `range`'s units.
    Scalar { range: Range },
}

impl FeedKind {
    /// A spectrum over the audible band (20 Hz–20 kHz, floor −90 dB).
    #[must_use]
    pub fn spectrum(bins: u16) -> Self {
        Self::Spectrum {
            bins,
            f_min: default_f_min(),
            f_max: default_f_max(),
            floor_db: default_floor_db(),
        }
    }

    /// A gain-reduction trace of `len` points.
    #[must_use]
    pub fn gr_history(len: u32) -> Self {
        Self::History {
            of: ScalarKind::GainReduction,
            len,
        }
    }

    /// ADSR playheads for `voices` voices (stages `A`, `D`, `S`, `R`).
    #[must_use]
    pub fn adsr(voices: u8) -> Self {
        Self::Envelope {
            stages: ["A", "D", "S", "R"].map(String::from).to_vec(),
            voices,
        }
    }

    /// How many `values` a frame of this kind carries.
    #[must_use]
    pub fn frame_len(&self) -> usize {
        match self {
            Self::Level { channels } => usize::from(*channels),
            Self::GainReduction | Self::Scalar { .. } => 1,
            Self::Spectrum { bins, .. } => usize::from(*bins),
            Self::Tuner => 2,
            Self::History { len, .. } | Self::Waveform { len } => *len as usize,
            Self::Envelope { voices, .. } => usize::from(*voices) * 3,
        }
    }

    /// Whether a frame is a level in dB a meter can show (a level or a gain
    /// reduction, not a curve of many values).
    #[must_use]
    pub fn is_meter(&self) -> bool {
        matches!(self, Self::Level { .. } | Self::GainReduction)
    }

    /// Whether a frame is a series a plot draws (spectrum, history,
    /// waveform) rather than a handful of readouts.
    #[must_use]
    pub fn is_series(&self) -> bool {
        matches!(
            self,
            Self::Spectrum { .. } | Self::History { .. } | Self::Waveform { .. }
        )
    }

    /// This kind as a request's `options` reshape it: `bins` resizes a
    /// spectrum, `len` a history or waveform, `voices` an envelope. Other
    /// options (tap, smoothing, trigger) don't change the frame's shape.
    #[must_use]
    pub fn with_options(&self, o: &FeedOptions) -> Self {
        let mut k = self.clone();
        match &mut k {
            Self::Spectrum { bins, .. } => {
                if let Some(b) = o.bins {
                    *bins = b.clamp(1, MAX_BINS);
                }
            }
            Self::History { len, .. } | Self::Waveform { len } => {
                if let Some(l) = o.len {
                    *len = l.clamp(1, MAX_LEN);
                }
            }
            Self::Envelope { voices, .. } => {
                if let Some(v) = o.voices {
                    *voices = v;
                }
            }
            _ => {}
        }
        k
    }
}

/// One feed an engine offers — an entry of the catalog's `streams`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FeedInfo {
    pub id: EntryId,
    pub name: String,
    pub kind: FeedKind,
    /// Nominal publish rate.
    pub rate_hz: f64,
}

impl FeedInfo {
    /// A feed at 30 Hz.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: FeedKind) -> Self {
        Self {
            id: EntryId::new(id),
            name: name.into(),
            kind,
            rate_hz: 30.0,
        }
    }

    #[must_use]
    pub fn rate(mut self, rate_hz: f64) -> Self {
        self.rate_hz = rate_hz;
        self
    }

    /// What this feed becomes under `options`: its kind reshaped
    /// ([`FeedKind::with_options`]) and its rate, if asked, within
    /// `(0, MAX_RATE_HZ]`. An engine answers a request with this.
    #[must_use]
    pub fn resolve(&self, options: &FeedOptions) -> Self {
        let rate_hz = match options.rate_hz {
            Some(r) if r.is_finite() && r > 0.0 => r.min(MAX_RATE_HZ),
            _ => self.rate_hz,
        };
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind.with_options(options),
            rate_hz,
        }
    }
}

/// Where in the signal path a feed listens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Tap {
    /// Before the block's processing (the input).
    Pre,
    /// After it (the output).
    #[default]
    Post,
    /// The detector's key input.
    Sidechain,
}

/// When a scope ([`FeedKind::Waveform`]) starts its window.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Trigger {
    /// The latest `len` samples, no alignment.
    Free,
    /// Start at an upward zero crossing, so a periodic wave stands still.
    #[default]
    Rising,
    /// Start at a downward zero crossing.
    Falling,
}

/// How a surface wants a feed. Every field is optional: `None` keeps the
/// engine's default (the catalog's shape and rate).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct FeedOptions {
    /// Spectrum bins.
    #[serde(default)]
    #[facet(default)]
    pub bins: Option<u16>,
    /// History points or waveform samples.
    #[serde(default)]
    #[facet(default)]
    pub len: Option<u32>,
    /// Publish rate.
    #[serde(default)]
    #[facet(default)]
    pub rate_hz: Option<f64>,
    /// Where to listen (pre, post, sidechain).
    #[serde(default)]
    #[facet(default)]
    pub tap: Option<Tap>,
    /// Frequency smoothing of a spectrum, in octaves (`1/3` = third-octave).
    #[serde(default)]
    #[facet(default)]
    pub smoothing: Option<f64>,
    /// A scope's trigger.
    #[serde(default)]
    #[facet(default)]
    pub trigger: Option<Trigger>,
    /// Envelope voices to report.
    #[serde(default)]
    #[facet(default)]
    pub voices: Option<u8>,
}

/// A subscription with options: "feed `id`, shaped like this".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FeedRequest {
    pub id: EntryId,
    #[serde(default)]
    #[facet(default)]
    pub options: FeedOptions,
}

impl FeedRequest {
    /// Feed `id` as the engine offers it.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: EntryId::new(id),
            options: FeedOptions::default(),
        }
    }

    #[must_use]
    pub fn bins(mut self, bins: u16) -> Self {
        self.options.bins = Some(bins);
        self
    }

    #[must_use]
    pub fn len(mut self, len: u32) -> Self {
        self.options.len = Some(len);
        self
    }

    #[must_use]
    pub fn rate(mut self, rate_hz: f64) -> Self {
        self.options.rate_hz = Some(rate_hz);
        self
    }

    #[must_use]
    pub fn tap(mut self, tap: Tap) -> Self {
        self.options.tap = Some(tap);
        self
    }

    #[must_use]
    pub fn smoothing(mut self, octaves: f64) -> Self {
        self.options.smoothing = Some(octaves);
        self
    }

    #[must_use]
    pub fn trigger(mut self, trigger: Trigger) -> Self {
        self.options.trigger = Some(trigger);
        self
    }

    #[must_use]
    pub fn voices(mut self, voices: u8) -> Self {
        self.options.voices = Some(voices);
        self
    }
}

/// One sample of a feed; `values` are laid out by the feed's kind
/// ([`FeedKind`]'s table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FeedFrame {
    pub id: EntryId,
    pub values: Vec<f64>,
}

/// One voice of an [`FeedKind::Envelope`] frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceState {
    /// Index into the feed's `stages`; `None` when idle.
    pub stage: Option<usize>,
    /// How far through the stage, 0..1.
    pub position: f64,
    /// The envelope's output, 0..1.
    pub level: f64,
}

impl VoiceState {
    /// The voices of an envelope frame's `values`.
    pub fn decode(values: &[f64]) -> impl Iterator<Item = Self> + '_ {
        values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[stage, position, level]| Self {
                stage: (stage >= 0.0).then(|| stage.round() as usize),
                position,
                level,
            })
    }

    /// This voice as a frame's triple.
    #[must_use]
    pub fn encode(&self) -> [f64; 3] {
        [
            self.stage.map_or(-1.0, |s| s as f64),
            self.position,
            self.level,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::range::Unit;

    #[test]
    fn existing_catalog_json_still_reads() {
        let old = r#"[
            {"id":"fet/gr","name":"GR","kind":"GainReduction","rate_hz":30.0},
            {"id":"out","name":"Out","kind":{"Level":{"channels":2}},"rate_hz":30.0},
            {"id":"lfo","name":"LFO","kind":{"Scalar":{"range":{"min":0.0,"max":1.0,"scale":"Linear","unit":"None","steps":null}}},"rate_hz":60.0},
            {"id":"spec","name":"Spectrum","kind":{"Spectrum":{"bins":64}},"rate_hz":30.0},
            {"id":"tuner","name":"Tuner","kind":"Tuner","rate_hz":30.0}
        ]"#;
        let feeds: Vec<FeedInfo> = serde_json::from_str(old).unwrap();
        assert_eq!(feeds[0].kind, FeedKind::GainReduction);
        assert_eq!(feeds[1].kind, FeedKind::Level { channels: 2 });
        assert_eq!(feeds[3].kind, FeedKind::spectrum(64), "band fields default");
        assert_eq!(feeds[4].kind, FeedKind::Tuner);
        // Old kinds write exactly what they used to.
        assert_eq!(
            serde_json::to_string(&feeds[1].kind).unwrap(),
            r#"{"Level":{"channels":2}}"#
        );
        assert_eq!(
            serde_json::to_string(&feeds[0].kind).unwrap(),
            r#""GainReduction""#
        );
    }

    #[test]
    fn new_kinds_round_trip() {
        let kinds = [
            FeedKind::gr_history(256),
            FeedKind::History {
                of: ScalarKind::Scalar {
                    range: Range::linear(0.0, 1.0, Unit::None),
                },
                len: 8,
            },
            FeedKind::Waveform { len: 512 },
            FeedKind::adsr(4),
            FeedKind::Spectrum {
                bins: 128,
                f_min: 30.0,
                f_max: 16_000.0,
                floor_db: -72.0,
            },
        ];
        for k in kinds {
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(
                serde_json::from_str::<FeedKind>(&json).unwrap(),
                k,
                "{json}"
            );
        }
    }

    #[test]
    fn requests_reshape_and_clamp() {
        let spec = FeedInfo::new("eq/spectrum", "Spectrum", FeedKind::spectrum(64));
        let r = FeedRequest::new("eq/spectrum")
            .bins(256)
            .rate(1000.0)
            .smoothing(1.0 / 3.0);
        let got = spec.resolve(&r.options);
        assert_eq!(got.kind.frame_len(), 256);
        assert_eq!(got.rate_hz, MAX_RATE_HZ);
        let trace = FeedInfo::new("comp/gr_trace", "GR", FeedKind::gr_history(128));
        assert_eq!(
            trace
                .resolve(&FeedRequest::new("x").len(0).options)
                .kind
                .frame_len(),
            1
        );
        assert_eq!(trace.resolve(&FeedOptions::default()), trace);
        // A request with no options on the wire.
        let bare: FeedRequest = serde_json::from_str(r#"{"id":"x"}"#).unwrap();
        assert_eq!(bare, FeedRequest::new("x"));
    }

    #[test]
    fn envelope_frames_decode() {
        let voices = [
            VoiceState {
                stage: Some(1),
                position: 0.5,
                level: 0.8,
            },
            VoiceState {
                stage: None,
                position: 0.0,
                level: 0.0,
            },
        ];
        let values: Vec<f64> = voices.iter().flat_map(VoiceState::encode).collect();
        assert_eq!(values.len(), FeedKind::adsr(2).frame_len());
        assert_eq!(VoiceState::decode(&values).collect::<Vec<_>>(), voices);
    }
}
