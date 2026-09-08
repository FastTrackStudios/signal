//! Band routing and parameter installation.
use super::{DynSettings, FtsEq, SideChain, eq_shape_to_filter};

impl FtsEq {
    /// Route + configure one band after any of its params changed.
    pub(super) fn sync_band(&mut self, band: usize) {
        self.sync_band_prepared(band, None);
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one flat parameter sync per band across thirteen shapes, the dynamics routing and the side-chain; longer as a table than as thirteen helpers"
    )]
    pub(super) fn sync_band_prepared(
        &mut self,
        band: usize,
        prepared: Option<&crate::PreparedFilter>,
    ) {
        let Some(mut slot) = self.bands.get(band).copied() else {
            return;
        };
        let (used, on) = (slot.used, slot.enabled);
        let enabled = used && on;
        let shape = slot.shape;
        let DynSettings {
            range_db: range,
            threshold_db: thr,
            attack_pct: atk,
            release_pct: rel,
            auto,
            relative,
        } = slot.dynamics;
        // A band goes dynamic when it has a range and a dynamics-capable
        // shape (Bell/shelves — same rule as Pro-Q).
        let dyn_shape = match shape {
            crate::design::slope::FilterShape::Bell => Some(crate::dynamics::DynShape::Bell),
            crate::design::slope::FilterShape::LowShelf => {
                Some(crate::dynamics::DynShape::LowShelf)
            }
            crate::design::slope::FilterShape::HighShelf => {
                Some(crate::dynamics::DynShape::HighShelf)
            }
            _ => None,
        };
        let spectral = slot.spectral.on && range.abs() > 1.0e-3;
        let go_dynamic = enabled && !spectral && range.abs() > 1.0e-3 && dyn_shape.is_some();
        slot.dyn_active = go_dynamic;
        // A dynamic band whose shape has no SVF equivalent — Flat Tilt is the
        // one the factory library uses — keeps its exact static design and has
        // that design's gain ridden by the detector instead. Measured against
        // the plugin, a dynamic Flat Tilt is simply the static Flat Tilt curve
        // scaled by the drive, and the static one already matches to 0.00 dB.
        let modulated = enabled && !spectral && range.abs() > 1.0e-3 && dyn_shape.is_none();
        slot.dyn_modulated = modulated;
        if !modulated {
            slot.dyn_modulated_gain = f64::NAN;
        }

        let freq = slot.freq_hz.clamp(10.0, 30000.0);
        let gain = slot.gain_db.clamp(-30.0, 30.0) * self.gain_scale;
        let q = slot.q.clamp(0.025, 40.0);

        // Stream routing (transient mode): 0 Both, 1 Transient (chain
        // A), 2 Steady (chain B). Outside transient mode chain A takes
        // everything and chain B idles.
        let stream = slot.stream;
        let in_a = !self.transient_mode || stream != crate::Stream::Steady;
        let in_b = self.transient_mode && stream != crate::Stream::Transient;
        if let Some(stored) = self.bands.get_mut(band) {
            *stored = slot;
        }
        for (chain, present) in [(&mut self.eq, in_a), (&mut self.eq_b, in_b)] {
            if let Some(b) = chain.band_mut(band) {
                b.enabled = enabled && !go_dynamic && present;
                b.freq_hz = freq;
                b.gain_db = gain;
                b.q = q;
                b.filter_type = eq_shape_to_filter(slot.shape);
                let order = slot.slope.order;
                let fraction = slot.slope.fraction;
                b.order = order;
                b.fractional_order = fraction;
                b.enabled = b.enabled && (order > 0 || fraction > 1.0e-6);
                b.placement = slot.placement;
            }
            if let Some(filter) = prepared {
                if let Some(b) = chain.band_mut(band) {
                    let enabled = b.enabled;
                    b.install(filter);
                    b.enabled = enabled;
                }
            } else {
                chain.update_band(band);
            }
        }
        if prepared.is_none() {
            self.finish_update();
        }

        let Some(d) = self.dyn_bands.get_mut(band) else {
            return;
        };
        d.params.enabled = go_dynamic || modulated;
        d.params.modulate_only = modulated;
        if go_dynamic || modulated {
            const BASE_RELEASE_MS: f64 = 300.0;
            d.params.shape = dyn_shape.unwrap_or(crate::dynamics::DynShape::Bell);
            d.params.freq_hz = freq;
            d.params.q = q;
            d.params.base_gain_db = gain;
            d.params.range_db = range * self.gain_scale;
            d.params.placement = slot.placement;
            // Side-chain range: a filtered band listens to what it is told to,
            // an unfiltered one listens to itself.
            let SideChain {
                filtered,
                lo_hz: lo,
                hi_hz: hi,
            } = slot.side;
            d.params.side_mode = if filtered {
                crate::dynamics::SideMode::Free
            } else {
                // A tilt is band-linked too, even though it reshapes the whole
                // spectrum. Driven to a FIXED threshold the plugin applies its
                // full static curve at every frequency — a trigger band around
                // the tilt's own frequency left ours 4.6 dB short at 62 Hz and
                // 5.6 at 16 kHz, and a wide one matched to 0.00. On AUTO,
                // which is what all 15 of the factory library's tilt bands
                // use, the wide trigger measures worse on every preset that
                // has one: four presets moved, none improved, up to 1 dB. The
                // library is the arbiter.
                crate::dynamics::SideMode::BandLinked
            };
            d.params.side_lo_hz = lo;
            d.params.side_hi_hz = hi;
            d.detector.params.threshold_db = if auto { 0.0 } else { thr };
            d.detector.params.auto = auto;
            // Auto FOLLOWS the programme. Measured by feeding the plugin
            // unchanging noise and reading one band every second: its gain
            // walked for about seven seconds and only then held, and it held
            // *partway* to the band's target rather than at it — -7.84 dB
            // where full range is -10.75. A fixed threshold cannot do either.
            //
            // An earlier reading said the opposite, because it was taken
            // before the plugin had settled: every measurement in a sweep of
            // levels was of a threshold still moving, and the same
            // configuration measured twice in one run disagreed by 7 dB.
            d.detector.params.adaptive = auto;
            d.detector.params.relative = relative;
            // Percent knobs around the ballistics measured from the plugin.
            //
            // Attack scales with the band's frequency — a low band cannot ride
            // faster than its own period, and Pro-Q's steps a decade of
            // frequency into roughly a halving of attack time (measured at
            // 1 ms after a step: -6.3 dB at 200 Hz, -8.7 at 1 kHz, -10.0 at
            // 8 kHz on a -12 dB band).
            //
            // Release does NOT. It measures the same at 200 Hz, 1 kHz and
            // 8 kHz — about a 300 ms time constant, an order of magnitude
            // slower than the frequency-scaled figure that used to be derived
            // from the attack. That mattered far more than it looks: on a
            // steady tone the ballistics settle and the difference vanishes,
            // but programme material never settles, so a release 12x too fast
            // let every dynamic band recover between transients and apply far
            // less average reduction than the plugin.
            // Attack time constants read off the plugin's step response on a
            // -12 dB band: 1.34 ms at 200 Hz, 0.77 at 1 kHz, 0.57 at 8 kHz.
            let base_atk = (0.5 + 170.0 / freq.max(1.0)).clamp(0.3, 20.0);
            let base_rel = BASE_RELEASE_MS;
            d.detector.params.attack_ms =
                base_atk * 8.0f64.powf((atk.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.detector.params.release_ms =
                base_rel * 8.0f64.powf((rel.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.update(self.sample_rate);
        }
    }
}
