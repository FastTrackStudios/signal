//! Live data a shader reads: [`ShaderInput`]s on a [`ShaderFx`](crate::document::proto::ShaderFx).
//!
//! A shader node names the data it needs — a feed's latest frame (spectrum
//! bins, a gain-reduction history, a scope), a param's value, a curve
//! provider's series (delay taps worked out from time and feedback), or
//! fixed numbers — and the host resolves each to an array of floats every
//! frame. The effects stage uploads them and the WGSL reads them by name
//! (`IN_<NAME>`, see `render/src/fx/CONTRACT.md`). Renderers without the
//! stage ignore them: the node's subtree (its vector fallback) draws as is.

use crate::surface::proto::{CurveInput, EntryId, ParamAddr};
use facet::Facet;
use serde::{Deserialize, Serialize};

/// The most values one input carries (after resampling).
pub const MAX_INPUT_LEN: usize = 1024;
/// The most inputs one shader reads.
pub const MAX_INPUTS: usize = 16;

/// One named array of live data for a shader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ShaderInput {
    /// The WGSL reads it as `IN_<NAME>` (upper-cased): `gr_trace` → `IN_GR_TRACE`.
    pub name: String,
    pub source: ShaderInputSource,
    /// Resample to exactly `len` values (linear, both ends kept), like a
    /// fixed-length uniform array; `None` = as the source gives them (up to
    /// [`MAX_INPUT_LEN`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub len: Option<u32>,
    /// Map every value `lo..hi → 0..1` (clamped unless `unclamped`), before
    /// resampling. `hi < lo` runs the other way (gain reduction in dB: `0 →
    /// 0`, `−60 → 1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub map: Option<InputMap>,
}

/// Where a [`ShaderInput`]'s values come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ShaderInputSource {
    /// A param's value in engineering units (a choice's option index, a
    /// toggle's 0/1): one value.
    Param { addr: ParamAddr },
    /// A feed's latest frame — all of it (spectrum bins, a history, a
    /// scope's samples, envelope voices), or one `channel`'s value.
    Stream { id: EntryId, channel: Option<u8> },
    /// A curve provider's series (the runtime's curve registry), flattened
    /// `x0, y0, x1, y1, …` in engineering units; `series` picks one (else
    /// the first). Evaluated like a `Curve` control's, so the vector
    /// fallback and the shader agree.
    Curve {
        provider: String,
        inputs: Vec<CurveInput>,
        series: Option<String>,
    },
    /// Fixed values.
    Const { values: Vec<f64> },
}

/// A linear map onto `0..1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct InputMap {
    pub lo: f64,
    pub hi: f64,
    /// Keep values outside `0..1` instead of clamping.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[facet(default)]
    pub unclamped: bool,
}

impl InputMap {
    #[must_use]
    pub fn apply(&self, v: f64) -> f64 {
        let span = self.hi - self.lo;
        let u = if span.abs() < 1e-12 {
            0.0
        } else {
            (v - self.lo) / span
        };
        if self.unclamped { u } else { u.clamp(0.0, 1.0) }
    }
}

impl ShaderInput {
    fn new(name: impl Into<String>, source: ShaderInputSource) -> Self {
        Self {
            name: name.into(),
            source,
            len: None,
            map: None,
        }
    }

    /// A param's value.
    #[must_use]
    pub fn param(name: impl Into<String>, addr: impl Into<String>) -> Self {
        Self::new(
            name,
            ShaderInputSource::Param {
                addr: ParamAddr::new(addr),
            },
        )
    }

    /// A feed's latest frame.
    #[must_use]
    pub fn stream(name: impl Into<String>, id: impl Into<String>) -> Self {
        Self::new(
            name,
            ShaderInputSource::Stream {
                id: EntryId::new(id),
                channel: None,
            },
        )
    }

    /// A curve provider's series, flattened `x, y` pairs.
    #[must_use]
    pub fn curve(
        name: impl Into<String>,
        provider: impl Into<String>,
        inputs: Vec<CurveInput>,
        series: Option<&str>,
    ) -> Self {
        Self::new(
            name,
            ShaderInputSource::Curve {
                provider: provider.into(),
                inputs,
                series: series.map(str::to_string),
            },
        )
    }

    /// Fixed values.
    #[must_use]
    pub fn constant(name: impl Into<String>, values: Vec<f64>) -> Self {
        Self::new(name, ShaderInputSource::Const { values })
    }

    /// Resampled to `len` values.
    #[must_use]
    pub fn len(mut self, len: u32) -> Self {
        self.len = Some(len);
        self
    }

    /// Mapped `lo..hi → 0..1`, clamped.
    #[must_use]
    pub fn map(mut self, lo: f64, hi: f64) -> Self {
        self.map = Some(InputMap {
            lo,
            hi,
            unclamped: false,
        });
        self
    }

    /// The WGSL constant naming this input's slot.
    #[must_use]
    pub fn constant_name(&self) -> String {
        constant_name(&self.name)
    }

    /// `raw` (the source's values) as this input delivers them: mapped,
    /// resampled, non-finite values as 0, at most [`MAX_INPUT_LEN`].
    #[must_use]
    pub fn shape(&self, raw: &[f64]) -> Vec<f32> {
        let mapped: Vec<f64> = raw
            .iter()
            .map(|v| {
                let v = self.map.map_or(*v, |m| m.apply(*v));
                if v.is_finite() { v } else { 0.0 }
            })
            .collect();
        let out = match self.len {
            Some(n) => resample(&mapped, (n as usize).min(MAX_INPUT_LEN)),
            None => mapped.into_iter().take(MAX_INPUT_LEN).collect(),
        };
        out.into_iter().map(|v| v as f32).collect()
    }
}

/// `IN_<NAME>`: upper-cased, anything not `[A-Z0-9_]` as `_`.
#[must_use]
pub fn constant_name(name: &str) -> String {
    let body: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("IN_{body}")
}

/// `src` linearly resampled to `n` values, first and last kept (an empty
/// source stays empty — "no data", not zeros).
#[must_use]
pub fn resample(src: &[f64], n: usize) -> Vec<f64> {
    if src.is_empty() || n == 0 {
        return Vec::new();
    }
    if src.len() == 1 || n == 1 {
        return vec![src[0]; n];
    }
    let last = (src.len() - 1) as f64;
    (0..n)
        .map(|i| {
            let pos = i as f64 / (n - 1) as f64 * last;
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(src.len() - 1);
            let f = pos - lo as f64;
            src[lo] * (1.0 - f) + src[hi] * f
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_both_ends() {
        let src: Vec<f64> = (0..500).map(|i| f64::from(i) / 499.0).collect();
        let out = resample(&src, 128);
        assert_eq!(out.len(), 128);
        assert!((out[0] - 0.0).abs() < 1e-9);
        assert!((out[127] - 1.0).abs() < 1e-9);
        assert!(resample(&[], 8).is_empty());
        assert_eq!(resample(&[3.0], 4), vec![3.0; 4]);
    }

    #[test]
    fn a_map_runs_either_way_and_clamps() {
        let gr = InputMap {
            lo: 0.0,
            hi: -60.0,
            unclamped: false,
        };
        assert!((gr.apply(-6.0) - 0.1).abs() < 1e-9);
        assert_eq!(gr.apply(3.0), 0.0);
        let free = InputMap {
            unclamped: true,
            ..gr
        };
        assert!((free.apply(3.0) + 0.05).abs() < 1e-9);
    }

    #[test]
    fn shaping_maps_then_resamples_and_zeroes_gaps() {
        let i = ShaderInput::stream("in", "comp/in_history")
            .map(-60.0, 0.0)
            .len(3);
        assert_eq!(i.shape(&[-60.0, f64::NAN, 0.0]), vec![0.0, 0.0, 1.0]);
        assert_eq!(i.constant_name(), "IN_IN");
        assert_eq!(constant_name("gr-trace"), "IN_GR_TRACE");
    }

    #[test]
    fn inputs_round_trip_through_json() {
        let fx = crate::document::proto::ShaderFx {
            shader: "viz.comp".into(),
            params: Default::default(),
            inputs: vec![
                ShaderInput::param("threshold", "comp/threshold"),
                ShaderInput::stream("trace", "comp/in_history")
                    .map(-60.0, 0.0)
                    .len(128),
                ShaderInput::curve(
                    "taps",
                    "delay.taps",
                    vec![CurveInput::param("time", "d/time")],
                    Some("taps"),
                ),
                ShaderInput::constant("color", vec![0.2, 0.5, 1.0]),
            ],
        };
        let json = serde_json::to_string(&fx).unwrap();
        assert_eq!(
            serde_json::from_str::<crate::document::proto::ShaderFx>(&json).unwrap(),
            fx
        );
        // Without inputs, the old shape exactly.
        let plain = crate::document::proto::ShaderFx {
            shader: "glass".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"shader":"glass","params":{}}"#
        );
    }
}
