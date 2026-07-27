//! saturate — the public facade over the `saturate-dsp` crate.
//!
//! Apps and plugin shells depend on this crate, never on `saturate-dsp`
//! directly (mirrors the level/comp facade pattern).

pub use saturate_dsp::{SaturationCurve, Saturator};

/// Stereo (or N-channel) saturator: one memoryless [`Saturator`] per channel
/// sharing one settings set.
#[derive(Debug, Clone, Default)]
pub struct StereoSaturator {
    stage: Saturator,
}

impl StereoSaturator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_drive(&mut self, drive: f32) {
        self.stage.set_drive(drive);
    }

    pub fn set_curve(&mut self, curve: SaturationCurve) {
        self.stage.set_curve(curve);
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.stage.set_mix(mix);
    }

    pub fn set_output_db(&mut self, db: f32) {
        self.stage.set_output_db(db);
    }

    pub fn reset(&mut self) {
        self.stage.reset();
    }

    /// Process one frame in place. The stage is memoryless, so a single
    /// instance serves every channel without state coupling.
    #[inline]
    pub fn process_frame(&self, frame: &mut [f32]) {
        for sample in frame {
            *sample = self.stage.process(*sample);
        }
    }

    #[inline]
    pub fn process_sample(&self, input: f32) -> f32 {
        self.stage.process(input)
    }
}
