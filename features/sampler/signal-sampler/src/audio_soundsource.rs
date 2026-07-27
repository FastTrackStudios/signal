//! The **Audio** [`Soundsource`] — emits the
//! layer's *input* as the source, rather than synthesizing or sampling.
//!
//! This is the **guitar-rig** case: the live DI feeds straight into the
//! layer's filter / amp / FX chain, with the generator slot simply passing
//! the input through at a `level` gain. It is also the seed of the cinematic /
//! file-player case — a file streamer can slot in behind the same trait later;
//! for now the source is whatever audio the render tree hands in as `in_l` /
//! `in_r`.
//!
//! It is not note-triggered: [`note_on`](Soundsource::note_on) /
//! [`note_off`](Soundsource::note_off) are no-ops (the signal is continuous).

use signal_plugin_host::{PluginEvents, PluginParamInfo};

use crate::soundsource::{Soundsource, SoundsourceKind};

/// Param id for the passthrough level (linear gain).
const PARAM_LEVEL: u32 = 0;

/// A [`Soundsource`] that emits its audio input, scaled by a linear `level`.
pub struct AudioSoundsource {
    /// Linear output gain applied to the input (1.0 = unity).
    level: f32,
}

impl Default for AudioSoundsource {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioSoundsource {
    /// A unity-gain input passthrough.
    pub fn new() -> Self {
        Self { level: 1.0 }
    }

    /// Set the initial passthrough gain (linear).
    #[must_use]
    pub fn with_level(mut self, level: f32) -> Self {
        self.level = level.max(0.0);
        self
    }

    /// Current passthrough gain (linear).
    pub fn level(&self) -> f32 {
        self.level
    }
}

// r[impl signal.soundsource.audio]
impl Soundsource for AudioSoundsource {
    fn kind(&self) -> SoundsourceKind {
        SoundsourceKind::Audio
    }

    /// Nothing to allocate — the passthrough owns no buffers.
    fn prepare(&mut self, _sample_rate: f32, _block_size: usize) {}

    /// Continuous source: not note-triggered.
    fn note_on(&mut self, _note: u8, _velocity: u8) {}
    fn note_off(&mut self, _note: u8) {}

    fn render(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) {
        // Honor mod-engine / automation writes to `level` at block start.
        for &(id, value) in events.params {
            if id == PARAM_LEVEL {
                self.level = (value as f32).max(0.0);
            }
        }
        let g = self.level;
        let frames = out_l.len().min(out_r.len());
        for f in 0..frames {
            out_l[f] = in_l.get(f).copied().unwrap_or(0.0) * g;
            out_r[f] = in_r.get(f).copied().unwrap_or(0.0) * g;
        }
    }

    fn params(&self) -> Vec<PluginParamInfo> {
        vec![PluginParamInfo {
            id: PARAM_LEVEL,
            name: "level".into(),
            min: 0.0,
            max: 4.0,
            default: self.level as f64,
        }]
    }

    fn set_param(&mut self, id: u32, value: f64) {
        if id == PARAM_LEVEL {
            self.level = (value as f32).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_input_passes_through() {
        let mut src = AudioSoundsource::new();
        src.prepare(48_000.0, 256);
        let in_l = vec![1.0f32; 256];
        let in_r = vec![1.0f32; 256];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        src.render(&in_l, &in_r, &mut out_l, &mut out_r, &PluginEvents::default());
        assert!(out_l.iter().all(|&s| (s - 1.0).abs() < 1e-6));
        assert!(out_r.iter().all(|&s| (s - 1.0).abs() < 1e-6));
    }

    #[test]
    fn level_zero_is_silent() {
        let mut src = AudioSoundsource::new().with_level(0.0);
        let in_l = vec![1.0f32; 128];
        let in_r = vec![-1.0f32; 128];
        let mut out_l = vec![9.0f32; 128];
        let mut out_r = vec![9.0f32; 128];
        src.render(&in_l, &in_r, &mut out_l, &mut out_r, &PluginEvents::default());
        assert!(out_l.iter().all(|&s| s == 0.0));
        assert!(out_r.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn set_param_scales_the_passthrough() {
        let mut src = AudioSoundsource::new();
        src.set_param(0, 0.5);
        assert_eq!(src.level(), 0.5);
        let in_l = vec![1.0f32; 64];
        let in_r = vec![1.0f32; 64];
        let mut out_l = vec![0.0f32; 64];
        let mut out_r = vec![0.0f32; 64];
        src.render(&in_l, &in_r, &mut out_l, &mut out_r, &PluginEvents::default());
        assert!(out_l.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
