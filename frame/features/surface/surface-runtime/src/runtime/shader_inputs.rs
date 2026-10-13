//! Shader nodes' live inputs: each [`ShaderInput`] resolved against the
//! runtime's params, feeds and curve providers, for the effects stage
//! (`render::fx::Frame::inputs`).

use std::collections::{BTreeMap, HashMap};

use document_proto::{NodeId, ShaderInput, ShaderInputSource};
use surface_proto::Axis;

use super::{Runtime, pick};

/// Per shader node, each input's values by name.
pub type ShaderData = HashMap<NodeId, BTreeMap<String, Vec<f32>>>;

impl Runtime {
    /// Set the host's clock (seconds): animated curve providers (a
    /// visualizer's playhead, an LFO drawn in vectors) read it as `time`.
    pub fn set_clock(&mut self, seconds: f64) {
        self.clock_s = seconds;
    }

    /// The clock [`Self::set_clock`] last set.
    #[must_use]
    pub fn clock(&self) -> f64 {
        self.clock_s
    }

    /// Whether any curve on the surface — a `Curve` control's or a shader
    /// input's — reads the clock.
    pub(super) fn reads_clock(&self) -> bool {
        let controls = self.surface.controls.values().any(|c| match &c.kind {
            surface_proto::ControlKind::Curve { provider, .. } => self.curves.is_animated(provider),
            _ => false,
        });
        controls
            || self.surface.scene.nodes.values().filter_map(|n| n.shader.as_ref()).flat_map(|s| &s.inputs).any(|i| match &i.source {
                ShaderInputSource::Curve { provider, .. } => self.curves.is_animated(provider),
                _ => false,
            })
    }

    /// Every shader node's inputs this frame: each input's values from its
    /// source, mapped and resampled as it asks. Inputs with no data yet (a
    /// feed that hasn't sent a frame, an unknown param) are left out — the
    /// shader reads them as empty.
    #[must_use]
    pub fn shader_inputs(&self) -> ShaderData {
        let mut out = ShaderData::new();
        for node in self.surface.scene.nodes.values() {
            let Some(fx) = node.shader.as_ref().filter(|s| !s.inputs.is_empty()) else { continue };
            let values: BTreeMap<String, Vec<f32>> =
                fx.inputs.iter().filter_map(|i| self.input_raw(i).map(|raw| (i.name.clone(), i.shape(&raw)))).collect();
            out.insert(node.id, values);
        }
        out
    }

    /// One input's values before shaping.
    fn input_raw(&self, input: &ShaderInput) -> Option<Vec<f64>> {
        match &input.source {
            ShaderInputSource::Param { addr } => self.real(addr).map(|v| vec![v]),
            ShaderInputSource::Stream { id, channel: Some(c) } => self.stream_value(id, Some(*c)).map(|v| vec![v]),
            ShaderInputSource::Stream { id, channel: None } => self.streams.get(id).cloned(),
            ShaderInputSource::Const { values } => Some(values.clone()),
            ShaderInputSource::Curve { provider, inputs, series } => {
                let all = self.curve_series(provider, inputs, &Axis::Linear { min: 0.0, max: 1.0 })?;
                let s = pick(&all, series.as_deref())?;
                Some(s.points.iter().flat_map(|(x, y)| [*x, *y]).collect())
            }
        }
    }
}
