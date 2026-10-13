//! Engineering ranges and units, shared by parameters and feeds: how a
//! normalized 0..1 position maps to a value, and how a value prints.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::curve::Curve;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Unit {
    #[default]
    None,
    Hz,
    Db,
    Seconds,
    Milliseconds,
    Percent,
    Ratio,
    Semitones,
    Cents,
    Degrees,
    Beats,
    Bpm,
    Custom { suffix: String },
}

impl Unit {
    #[must_use]
    pub fn suffix(&self) -> &str {
        match self {
            Self::None => "",
            Self::Hz => "Hz",
            Self::Db => "dB",
            Self::Seconds => "s",
            Self::Milliseconds => "ms",
            Self::Percent => "%",
            Self::Ratio => ":1",
            Self::Semitones => "st",
            Self::Cents => "ct",
            Self::Degrees => "°",
            Self::Beats => "beats",
            Self::Bpm => "BPM",
            Self::Custom { suffix } => suffix,
        }
    }
}

/// How the normalized 0..1 control position spreads over `min..max`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Scale {
    /// Even across the range (dB gain, mix, pan).
    #[default]
    Linear,
    /// Even in ratio: equal travel is equal multiplication (frequency,
    /// time). Needs `min > 0`; otherwise behaves linearly.
    Logarithmic,
    /// Any other shape.
    Curve { curve: Curve },
}

/// A parameter's engineering range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub scale: Scale,
    pub unit: Unit,
    /// Whole steps only (modes, algorithm choices): `Some(n)` values.
    pub steps: Option<u32>,
}

impl Default for Range {
    /// The unit interval: normalized in, normalized out.
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, scale: Scale::Linear, unit: Unit::None, steps: None }
    }
}

impl Range {
    #[must_use]
    pub fn linear(min: f64, max: f64, unit: Unit) -> Self {
        Self { min, max, scale: Scale::Linear, unit, steps: None }
    }

    #[must_use]
    pub fn log(min: f64, max: f64, unit: Unit) -> Self {
        Self { min, max, scale: Scale::Logarithmic, unit, steps: None }
    }

    /// Snap a normalized value (0..1) to this range's steps, if it has any.
    #[must_use]
    pub fn quantize(&self, n: f64) -> f64 {
        match self.steps {
            Some(s) if s >= 2 => {
                let k = f64::from(s - 1);
                (n * k).round() / k
            }
            _ => n,
        }
    }

    /// Control position (0..1) → engineering value.
    #[must_use]
    pub fn to_real(&self, normalized: f64) -> f64 {
        let n = self.quantize(if normalized.is_nan() { 0.0 } else { normalized.clamp(0.0, 1.0) });
        match &self.scale {
            Scale::Logarithmic if self.min > 0.0 && self.max > 0.0 => self.min * (self.max / self.min).powf(n),
            Scale::Curve { curve } => self.min + (self.max - self.min) * curve.apply(n),
            _ => self.min + (self.max - self.min) * n,
        }
    }

    /// Engineering value → control position (0..1).
    #[must_use]
    pub fn to_normalized(&self, real: f64) -> f64 {
        let span = self.max - self.min;
        let n = match &self.scale {
            Scale::Logarithmic if self.min > 0.0 && self.max > 0.0 && real > 0.0 => {
                (real / self.min).ln() / (self.max / self.min).ln()
            }
            Scale::Curve { curve } if span != 0.0 => curve.invert((real - self.min) / span),
            _ if span != 0.0 => (real - self.min) / span,
            _ => 0.0,
        };
        self.quantize(n.clamp(0.0, 1.0))
    }

    /// Human readout: `"440 Hz"`, `"-6.0 dB"`, `"2.50 s"`, `"35 %"`. A
    /// stepped linear range prints as many decimals as its step needs
    /// (`0.5`-dB clicks read `-1.5 dB`; whole steps read whole).
    #[must_use]
    pub fn format(&self, real: f64) -> String {
        self.format_as(real, &ValueFormat::default())
    }

    /// [`Self::format`] under an explicit [`ValueFormat`]: fixed decimals,
    /// another unit (or none), the note a pitch is, or one label per step.
    #[must_use]
    pub fn format_as(&self, real: f64, f: &ValueFormat) -> String {
        if !f.marks.is_empty() {
            let plain = ValueFormat { marks: Vec::new(), ..f.clone() };
            let text = self.format_as(real, &plain);
            return f.marks.iter().find(|m| self.format_as(m.at, &plain) == text).map_or(text, |m| m.label.clone());
        }
        match &f.style {
            FormatStyle::Labels { labels } if !labels.is_empty() => {
                let last = labels.len() - 1;
                let i = (self.to_normalized(real) * last as f64).round() as usize;
                return labels[i.min(last)].clone();
            }
            FormatStyle::Note => {
                let midi = match self.unit {
                    Unit::Hz if real > 0.0 => 69.0 + 12.0 * (real / 440.0).log2(),
                    _ => real,
                };
                return note_name(midi);
            }
            _ => {}
        }
        let (value, unit, converted) = match (&f.unit, &self.unit) {
            (Some(u), _) => (real, u.clone(), false),
            (None, Unit::Hz) if real.abs() >= 1000.0 => (real / 1000.0, "kHz".to_string(), true),
            (None, Unit::Seconds) if real.abs() < 1.0 => (real * 1000.0, "ms".to_string(), true),
            (None, u) => (real, u.suffix().to_string(), false),
        };
        let decimals = f.decimals.map(usize::from).or_else(|| if converted { None } else { self.step_decimals() });
        let text = match decimals {
            Some(d) => format!("{value:.d$}"),
            None if self.steps.is_some() => format!("{value:.0}"),
            None if value.abs() >= 100.0 => format!("{value:.0}"),
            None if value.abs() >= 10.0 => format!("{value:.1}"),
            None => format!("{value:.2}"),
        };
        // `-0.0` reads as a glitch on a panel.
        let text = match text.strip_prefix('-') {
            Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
            _ => text,
        };
        let text = if f.sign && !text.starts_with('-') && text.chars().any(|c| c.is_ascii_digit() && c != '0') { format!("+{text}") } else { text };
        if unit.is_empty() { text } else { format!("{text} {unit}") }
    }

    /// Decimals a stepped range's step needs (`None` = not stepped). Only a
    /// linear range has one step size; a stepped curve prints whole values.
    fn step_decimals(&self) -> Option<usize> {
        let n = self.steps.filter(|s| *s >= 2)?;
        if !matches!(self.scale, Scale::Linear) {
            return Some(0);
        }
        let step = ((self.max - self.min) / f64::from(n - 1)).abs();
        let whole_at = |d: i32| {
            let k = step * 10f64.powi(d);
            (k - k.round()).abs() < 1e-6 * k.max(1.0)
        };
        Some((0..=3).find(|d| whole_at(*d)).unwrap_or(3) as usize)
    }
}

/// `"A4"`, `"C#3"` for a MIDI note number (rounded to the nearest note).
#[must_use]
pub fn note_name(midi: f64) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let n = midi.round() as i64;
    format!("{}{}", NAMES[n.rem_euclid(12) as usize], n.div_euclid(12) - 1)
}

/// How a readout prints a value. Carried on a surface control
/// (`Control::format`), so one param can read `-6 dB` on a knob and
/// `-6.0` in a window.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct ValueFormat {
    /// Fixed decimals (`None` = the range's own: its step, else by size).
    pub decimals: Option<u8>,
    /// A unit suffix instead of the range's (`Some("")` prints none).
    pub unit: Option<String>,
    pub style: FormatStyle,
    /// A number reads `+` before a positive value (a gain, a pitch offset).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[facet(default)]
    pub sign: bool,
    /// Words that replace particular values: `OFF` at a filter's stop,
    /// `C` at a pan's centre. A mark prints when the value reads the same
    /// as the mark's value would.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub marks: Vec<FormatMark>,
}

/// A word a readout prints in place of one value (see [`ValueFormat::mark`]).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct FormatMark {
    /// The value, in the range's units.
    pub at: f64,
    pub label: String,
}

/// What a readout prints.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum FormatStyle {
    /// The number (and unit).
    #[default]
    Number,
    /// The note it is: a `Hz` value as its nearest note, anything else as
    /// a MIDI note number (`69` → `A4`).
    Note,
    /// One label per position, evenly over the range: note divisions, a
    /// stepped knob whose positions have names but isn't a choice param.
    Labels { labels: Vec<String> },
}

impl ValueFormat {
    #[must_use]
    pub fn decimals(d: u8) -> Self {
        Self { decimals: Some(d), ..Self::default() }
    }

    #[must_use]
    pub fn note() -> Self {
        Self { style: FormatStyle::Note, ..Self::default() }
    }

    #[must_use]
    pub fn labels(labels: &[&str]) -> Self {
        Self { style: FormatStyle::Labels { labels: labels.iter().map(ToString::to_string).collect() }, ..Self::default() }
    }

    /// Print `unit` instead of the range's own (`""` = none).
    #[must_use]
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    #[must_use]
    pub fn with_decimals(mut self, d: u8) -> Self {
        self.decimals = Some(d);
        self
    }

    /// Print `+` before a positive number.
    #[must_use]
    pub fn signed(mut self) -> Self {
        self.sign = true;
        self
    }

    /// Print `label` where the value reads as `at` does (`OFF` at a
    /// filter's stop, `C` at a pan's centre).
    #[must_use]
    pub fn mark(mut self, at: f64, label: impl Into<String>) -> Self {
        self.marks.push(FormatMark { at, label: label.into() });
        self
    }

    /// A level in dB (a meter's readout): `decimals` (default 1) and this
    /// format's unit (default `dB`).
    #[must_use]
    pub fn db(&self, db: f64) -> String {
        let f = Self { decimals: Some(self.decimals.unwrap_or(1)), style: FormatStyle::Number, unit: self.unit.clone(), sign: self.sign, marks: Vec::new() };
        Range::linear(-1.0e9, 1.0e9, Unit::Db).format_as(db, &f)
    }
}
