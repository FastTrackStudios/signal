//! Native **Amp** block — the voice-chain gain stage (`BlockType::Amp`,
//! `Native` impl). Unity by default; the ModMatrix will drive its gain from the
//! amp envelope once control-rate modulation lands (roadmap §2).

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

pub struct NativeAmp {
    gain: f32,
    prepared: bool,
}

impl NativeAmp {
    pub fn new(_sample_rate: u32) -> Self {
        Self {
            gain: 1.0,
            prepared: false,
        }
    }

    #[must_use]
    pub fn with_gain_db(mut self, db: f32) -> Self {
        self.gain = 10f32.powf(db / 20.0);
        self
    }

    /// Normalized 0..1 → amplitude 0..2 (unity at 0.5) — matches param 0.
    #[must_use]
    pub fn with_gain_norm(mut self, v: f32) -> Self {
        self.gain = v.clamp(0.0, 1.0) * 2.0;
        self
    }
}

impl PluginInstance for NativeAmp {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.amp".into(),
            name: "Amp".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![PluginParamInfo {
            id: 0,
            name: "gain".into(),
            min: 0.0,
            max: 1.0,
            default: 0.5, // normalized: amplitude = 2v, unity at 0.5
        }]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        (id == 0).then_some((self.gain / 2.0) as f64)
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, _sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            if id == 0 {
                // Normalized 0..1 → amplitude 0..2 (unity at 0.5).
                self.gain = (value as f32).clamp(0.0, 1.0) * 2.0;
            }
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        for f in 0..frames {
            out_l[f] = in_l[f] * self.gain;
            out_r[f] = in_r[f] * self.gain;
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_scales_input() {
        let mut amp = NativeAmp::new(48_000).with_gain_db(-6.0);
        amp.prepare(48_000.0, 4).unwrap();
        let input = vec![1.0f32; 4];
        let (mut l, mut r) = (vec![0.0; 4], vec![0.0; 4]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        amp.process_block(&input, &input, &mut l, &mut r, &ev)
            .unwrap();
        assert!((l[0] - 0.501).abs() < 0.01, "-6 dB ≈ ×0.5, got {}", l[0]);
    }
}
