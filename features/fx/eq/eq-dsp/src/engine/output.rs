//! Output controls and response-based gain compensation.
use super::{FtsEq, band_envelope};
use dsp_core::num;

impl FtsEq {
    /// Recompute the Auto Gain compensation from the curve the chain draws.
    ///
    /// Pro-Q's Auto Gain holds the broadband level steady, and what it holds
    /// steady is measurable: on "Fast Food Notch" — a wide notch between a low
    /// cut and a high cut — the plugin's whole response sits 4.71 dB above an
    /// uncompensated render of the same preset, flat across every band. The
    /// energy-weighted mean of that preset's own curve is -4.97 dB, so the
    /// compensation is the negative of what the curve does to noise, within a
    /// quarter of a decibel.
    ///
    /// **Pink, not white.** Weighting each grid cell by its bandwidth puts
    /// most of the sum in the top octave, where a high shelf then dominates
    /// the answer. Measured against the plugin on two presets, as the error in
    /// the compensation each weighting predicts:
    ///
    /// ```text
    ///                          pink   white
    ///   Fast Food Notch        0.80    0.26
    ///   Supreme Transparency   0.42    1.79
    /// ```
    ///
    /// **From the drawn curve, not from the signal.** A live level matcher —
    /// K-weighted mean square of the output against the input, which is what
    /// "auto gain" means in most plugins — was built and measured. It fixes
    /// the preset the curve model gets most wrong (`Production Ready Vocals`,
    /// 4.66 dB to 1.07, because that preset's EQ is mostly dynamic and the
    /// static curve cannot see it) and loses everywhere else, for 4.8 dB more
    /// total error across the ten presets that set Auto Gain. It also fails
    /// outright where the two disagree in sign: on `Overheads 2` every
    /// energy-weighted measure says the curve makes the signal *louder* and
    /// the plugin compensates **upward** by 4.5 dB anyway. And a matcher
    /// launders our own broadband errors into the compensation, which makes
    /// the harness less able to see them. The exact law is still unknown.
    ///
    /// Runs only for the presets that switch Auto Gain on — ten of the 171 in
    /// the factory library.
    pub(super) fn refresh_auto_gain(&mut self) {
        if !self.auto_gain {
            self.auto_gain_db = 0.0;
            self.auto_grid_hz.clear();
            return;
        }
        // 1/12-octave grid from 20 Hz up. Fine enough that a surgical notch is
        // not stepped over, cheap enough to rebuild on a parameter change.
        if self.auto_grid_hz.is_empty() {
            let step = (1.0_f64 / 12.0).exp2();
            let ceiling = self.sample_rate * 0.45;
            let mut hz = 20.0f64;
            let max_iterations =
                num::f64_to_index((ceiling / 20.0).log(step).ceil()).saturating_add(1);
            for _ in 0..max_iterations {
                if hz >= ceiling {
                    break;
                }
                self.auto_grid_hz.push(hz);
                hz *= step;
            }
        }
        let n = self.auto_grid_hz.len();
        self.auto_grid_static_db.clear();
        for &hz in &self.auto_grid_hz {
            self.auto_grid_static_db
                .push(self.eq.magnitude_db(hz, self.sample_rate));
        }
        // And the shape of every band the static chain cannot see.
        self.auto_grid_env
            .resize_with(self.bands.len(), || vec![0.0; n]);
        for (slot, env) in self.bands.iter().zip(&mut self.auto_grid_env) {
            env.resize(n, 0.0);
            env.fill(0.0);
            let (used, on) = (slot.used, slot.enabled);
            let dynamic = used && on && (slot.dyn_active || slot.spectral.on);
            if dynamic {
                let shape = slot.shape;
                let f0 = slot.freq_hz.clamp(10.0, 30000.0);
                let q = slot.q.clamp(0.025, 40.0);
                // A band that touches one side of the image only moves half
                // the signal, so it is worth half as much to a broadband
                // compensation. "Hammond Levelling" is four bands that are
                // really two, duplicated for left and right; counting both at
                // full weight doubled the compensation and cost 1.6 dB.
                let w = match slot.placement {
                    crate::runtime::band::Placement::Stereo => 1.0,
                    _ => 0.5,
                };
                for (e, &hz) in env.iter_mut().zip(&self.auto_grid_hz) {
                    *e = w * band_envelope(shape, f0, q, hz);
                }
            }
        }
        self.update_auto_gain();
    }

    /// Sum the grid with whatever the dynamic and spectral bands are applying
    /// right now, and set the compensation.
    pub(super) fn update_auto_gain(&mut self) {
        if !self.auto_gain || self.auto_grid_static_db.is_empty() {
            return;
        }
        let n = self.auto_grid_static_db.len();
        let live = &mut self.auto_live;
        live.fill(0.0);
        let mut region = 0usize;
        for (band, (live_slot, slot)) in live.iter_mut().zip(&self.bands).enumerate() {
            let (used, on) = (slot.used, slot.enabled);
            if !(used && on) {
                continue;
            }
            if slot.spectral.on && slot.dynamics.range_db.abs() > 1.0e-3 {
                *live_slot = -self.spectral.region_reduction_db(region);
                region = region.saturating_add(1);
            } else if slot.dyn_active {
                // The band's live gain relative to the base the static chain
                // is NOT carrying — a dynamic band is out of that chain
                // entirely, so its whole applied gain counts here.
                *live_slot = self
                    .dyn_bands
                    .get(band)
                    .map_or(0.0, crate::dynamics::DynBand::live_gain_db);
            }
        }
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for (i, &static_db) in self.auto_grid_static_db.iter().enumerate().take(n) {
            let mut db = static_db;
            for (&live_val, env) in live.iter().zip(&self.auto_grid_env) {
                if live_val != 0.0 {
                    db += live_val * env.get(i).copied().unwrap_or(0.0);
                }
            }
            // Equal weight per octave — pink, not white.
            num += 10.0f64.powf(db / 10.0);
            den += 1.0;
        }
        self.auto_gain_db = if den > 0.0 && num > 0.0 {
            (-10.0 * (num / den).log10()).clamp(-30.0, 30.0)
        } else {
            0.0
        };
    }

    /// Turn Pro-Q's Auto Gain on or off.
    pub fn set_auto_gain(&mut self, on: bool) {
        if self.auto_gain == on {
            return;
        }
        self.auto_gain = on;
        self.refresh_auto_gain();
    }

    /// The compensation Auto Gain is currently applying, in dB (0 when off).
    #[must_use]
    pub const fn auto_gain_db(&self) -> f64 {
        self.auto_gain_db
    }

    /// The Output Pan currently set, and its mode.
    #[must_use]
    pub const fn output_pan(&self) -> f64 {
        self.output_pan
    }

    /// Whether Output Pan balances mid against side rather than left/right.
    #[must_use]
    pub const fn output_pan_mid_side(&self) -> bool {
        self.output_pan_mid_side
    }

    /// Output Pan: -1..1, turning one side down and never boosting the other.
    ///
    /// Measured by writing the global straight into the plugin and reading the
    /// mid and side transfer functions back. In **Mid/Side** mode a negative
    /// value scales the side by `1 + pan` and leaves the mid alone, and a
    /// positive one scales the mid by `1 - pan` and leaves the side alone:
    ///
    /// ```text
    ///   pan     -0.80   -0.44   -0.20   +0.11   +0.50   +0.90
    ///   side   -13.96   -5.06   -1.92    0.00    0.00    0.00
    ///   mid      0.00    0.00    0.00   -0.98   -6.00  -19.98
    /// ```
    ///
    /// In Stereo mode the same law applies to left and right. Nine of the 171
    /// factory presets set the mode and five of those set a non-zero pan; all
    /// five are Mid/Side, and on "Room 01" this global alone was 2.54 dB of a
    /// 3.23 dB error.
    pub const fn set_output_pan(&mut self, pan: f64, mid_side: bool) {
        self.output_pan = pan.clamp(-1.0, 1.0);
        self.output_pan_mid_side = mid_side;
    }

    /// Pro-Q's Character mode: 0 Clean, 1 Subtle, 2 Warm.
    ///
    /// Only its **gain** is modelled — see [`character_gain_db`].
    pub fn set_character(&mut self, mode: u32) {
        self.character = mode.min(2);
        for sh in &mut self.character_shaper {
            sh.update(self.sample_rate);
        }
    }
}
