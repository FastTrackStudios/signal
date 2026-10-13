//! Bounded audio callback path.
use super::{FtsEq, character_gain_db};

impl FtsEq {
    #[expect(
        clippy::too_many_lines,
        reason = "a decoded routine: one contiguous function in the binary, whose commentary cites the captured rows each branch was verified against. Splitting it would separate the arithmetic from its evidence"
    )]
    /// Process one block in place.
    pub fn process(&mut self, buf_l: &mut [f64], buf_r: &mut [f64]) {
        if !self.prepared || buf_l.len() != buf_r.len() || buf_l.len() > self.side_ref.len() {
            return;
        }

        // Fully-idle block (no active bands, no dynamics, no spectral,
        // no transient split, unity output): straight copy, zero DSP.
        let any_dyn = self
            .bands
            .iter()
            .any(|slot| slot.dyn_active || slot.dyn_modulated);
        if self.listen.is_none()
            && !self.transient_mode
            && !any_dyn
            && !self.spectral.has_regions()
            && !self.eq.has_active_bands()
            && (self.output_gain_db + self.auto_gain_db + character_gain_db(self.character)).abs()
                < 1.0e-9
            && self.output_pan.abs() < 1.0e-9
        {
            return;
        }
        // The compensation follows the curve the chain is applying right now,
        // so it is recomputed once a block — the grid itself is cached.
        if self.auto_gain {
            self.update_auto_gain();
        }
        let eq = &mut self.eq;
        let eq_b = &mut self.eq_b;
        let splitter = &mut self.splitter;
        let spectral = &mut self.spectral;
        let dyn_bands = &mut self.dyn_bands;
        let slots = &mut self.bands;
        let character = self.character;
        let character_shaper = &mut self.character_shaper;
        let pan = self.output_pan;
        let pan_mid_side = self.output_pan_mid_side;
        let transient_mode = self.transient_mode;
        let split_solo = self.split_solo;
        let tg = audiocore_dsp::db::db_to_linear(self.transient_gain_db);
        let sg = audiocore_dsp::db::db_to_linear(self.steady_gain_db);
        let out_gain = audiocore_dsp::db::db_to_linear(
            self.output_gain_db + self.auto_gain_db + character_gain_db(self.character),
        );
        let scratch_left = &mut self.scratch_sl;
        let scratch_right = &mut self.scratch_sr;
        let listen = self.listen;
        let solo_filter = &mut self.solo_filter;
        let dry_ring = &mut self.dry_ring;
        let dry_pos = &mut self.dry_pos;
        // Delta listening compares against the dry signal delayed by
        // the current path latency (spectral engaged → block-1).
        let dry_delay = if spectral.has_regions() {
            spectral.latency()
        } else {
            0
        };
        // Snapshot the input for the detectors before anything touches it.
        let n_in = buf_l.len().min(buf_r.len());
        for (slot, (l, r)) in self
            .side_ref
            .iter_mut()
            .zip(buf_l.iter().zip(buf_r.iter()))
            .take(n_in)
        {
            *slot = 0.5 * (l + r);
        }
        let side_ref = &self.side_ref;

        {
            let left: &mut [f64] = buf_l;
            let right: &mut [f64] = buf_r;
            {
                // Record dry for delta listening (cheap ring write,
                // only while a delta listen is active).
                if matches!(listen, Some((_, 2))) {
                    let ring = dry_ring[0].len();
                    let mut p = *dry_pos;
                    let [ring_l, ring_r] = dry_ring;
                    for (l, r) in left.iter().zip(right.iter()) {
                        if let (Some(dl), Some(dr)) = (ring_l.get_mut(p), ring_r.get_mut(p)) {
                            *dl = *l;
                            *dr = *r;
                        }
                        p = p.saturating_add(1).checked_rem(ring).unwrap_or(0);
                    }
                }
                if transient_mode {
                    // Split the whole block, run each stream's chain
                    // block-wise (left/right become the transient stream, the
                    // steady stream rides the dedicated scratch), then
                    // recombine. Complementary split keeps flat
                    // settings a null.
                    let n = left.len();
                    for (((l, r), sl), sr) in left
                        .iter_mut()
                        .zip(right.iter_mut())
                        .zip(scratch_left.iter_mut())
                        .zip(scratch_right.iter_mut())
                        .take(n)
                    {
                        let mask = splitter.tick_mask(0.5 * (*l + *r));
                        let tl = *l * mask;
                        let tr = *r * mask;
                        *sl = *l - tl;
                        *sr = *r - tr;
                        *l = tl;
                        *r = tr;
                    }
                    eq.process(left, right);
                    if let (Some(sl), Some(sr)) =
                        (scratch_left.get_mut(..n), scratch_right.get_mut(..n))
                    {
                        eq_b.process(sl, sr);
                    }
                    for (((l, r), &sl), &sr) in left
                        .iter_mut()
                        .zip(right.iter_mut())
                        .zip(scratch_left.iter())
                        .zip(scratch_right.iter())
                        .take(n)
                    {
                        (*l, *r) = match split_solo {
                            1 => (*l * tg, *r * tg),
                            2 => (sl * sg, sr * sg),
                            _ => (l.mul_add(tg, sl * sg), r.mul_add(tg, sr * sg)),
                        };
                    }
                } else {
                    // Bands whose dynamics ride the static design run their
                    // detectors over the block first, then the design is
                    // rebuilt at the gain they arrived at. Once per block, and
                    // only when the gain has actually moved — a redesign is
                    // not free, and a tenth of a decibel is inaudible.
                    for (bi, (d, slot)) in dyn_bands.iter_mut().zip(slots.iter_mut()).enumerate() {
                        if !slot.dyn_modulated {
                            continue;
                        }
                        for ((l, r), s) in left.iter().zip(right.iter()).zip(side_ref) {
                            d.observe(*l, *r, *s);
                        }
                        let g = d.live_gain_db();
                        if !(g - slot.dyn_modulated_gain).abs().lt(&0.1) {
                            slot.dyn_modulated_gain = g;
                            if let Some(band) = eq.band_mut(bi) {
                                band.gain_db = g;
                            }
                            eq.update_band(bi);
                        }
                    }
                    eq.process(left, right);
                }
                for (d, slot) in dyn_bands.iter_mut().zip(slots.iter()) {
                    if !slot.dyn_active {
                        continue;
                    }
                    for ((l, r), s) in left.iter_mut().zip(right.iter_mut()).zip(side_ref) {
                        d.tick(l, r, *s);
                    }
                }
                // Per-band spectral dynamics (engaged only while at
                // least one band has its spectral toggle on).
                if spectral.has_regions() {
                    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                        (*l, *r) = spectral.tick(*l, *r);
                    }
                }
                // Character's waveshaper sits at the OUTPUT — its own makeup
                // is already in `out_gain`. Placed at the input instead, where
                // the EQ would then shape its harmonics, it measures worse on
                // programme material: "Production Ready Vocals" 1.49 dB to
                // 1.81, "Kick - IN 01" 1.65 to 1.82.
                if character == 2 {
                    let [shaper_l, shaper_r] = character_shaper;
                    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                        *l = shaper_l.tick(*l);
                        *r = shaper_r.tick(*r);
                    }
                }
                if (out_gain - 1.0).abs() > 1.0e-9 {
                    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                        *l *= out_gain;
                        *r *= out_gain;
                    }
                }
                // Output Pan: turn one side down, never boost the other.
                if pan.abs() > 1.0e-9 {
                    let (pan_neg, pan_pos) = (1.0 + pan.min(0.0), 1.0 - pan.max(0.0));
                    if pan_mid_side {
                        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                            let (mid, side_diff) = (0.5 * (*l + *r), 0.5 * (*l - *r));
                            let (mid, side_diff) = (mid * pan_pos, side_diff * pan_neg);
                            *l = mid + side_diff;
                            *r = mid - side_diff;
                        }
                    } else {
                        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                            *l *= pan_neg;
                            *r *= pan_pos;
                        }
                    }
                }
                // ── Listen: solo the band's region, or hear only the
                // delta this EQ creates. Composes with split_solo (the
                // stream solo already happened upstream), so
                // "transients of the soloed band" is stream solo +
                // band solo together.
                if let Some((_, mode)) = listen {
                    if mode == 1 {
                        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                            *l = solo_filter.tick(0, *l);
                            *r = solo_filter.tick(1, *r);
                        }
                    } else {
                        let [ring_l, ring_r] = dry_ring;
                        let ring = ring_l.len();
                        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                            // `+ ring` before the subtraction keeps the index
                            // positive when the delay reaches back past the
                            // ring's origin.
                            let read = dry_pos
                                .saturating_add(ring)
                                .saturating_sub(dry_delay)
                                .checked_rem(ring)
                                .unwrap_or(0);
                            *l -= ring_l.get(read).copied().unwrap_or(0.0);
                            *r -= ring_r.get(read).copied().unwrap_or(0.0);
                            *dry_pos = dry_pos.saturating_add(1).checked_rem(ring).unwrap_or(0);
                        }
                    }
                }
            }
        }
    }
}
