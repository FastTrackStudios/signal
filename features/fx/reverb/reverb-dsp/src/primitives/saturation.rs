//! Soft-clip saturation with drive + tanh shaping.
//!
//! Reference: Zölzer (ed.), "DAFX: Digital Audio Effects" 2nd ed., Ch. 4
//! — nonlinear processing. tanh is the classic smooth soft-clipper.

/// Single-channel soft saturator. `drive` 0..1 → 1x..8x pre-gain.
pub struct Saturator {
    drive: f64,
    drive_gain: f64,
    makeup: f64,
}

impl Saturator {
    pub fn new() -> Self {
        Self {
            drive: 0.0,
            drive_gain: 1.0,
            makeup: 1.0,
        }
    }

    /// 0.0 = clean, 1.0 = heavy. Internal: pre-gain 1x..8x.
    pub fn set_drive(&mut self, drive: f64) {
        self.drive = drive.clamp(0.0, 1.0);
        self.drive_gain = 1.0 + self.drive * 7.0;
        // Compensate by 1/sqrt(drive_gain) so loudness stays roughly even.
        self.makeup = 1.0 / self.drive_gain.sqrt();
    }

    pub fn drive(&self) -> f64 {
        self.drive
    }

    #[inline]
    pub fn tick(&self, input: f64) -> f64 {
        if self.drive == 0.0 {
            return input;
        }
        (input * self.drive_gain).tanh() * self.makeup
    }
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}
