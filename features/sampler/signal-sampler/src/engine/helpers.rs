//! Private helper methods for `SampleEngine` (voice building, resolution,
//! articulation lookup). Split out of `engine/mod.rs`; same impl.

use super::*;

impl SampleEngine {
    /// Build a `Voice` for a resolved sample, or `None` if the sample can't
    /// be found or loaded.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_voice(
        &self,
        artic_id: &str,
        section: &str,
        mic: &str,
        dynamic: &str,
        note: u8,
        direction: &str,
        kind: VoiceKind,
        gain: f32,
        release_frames: usize,
    ) -> Option<Voice> {
        let max_rr = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.rr)
            .unwrap_or(1);

        let rr_idx = self
            .rr
            .borrow_mut()
            .next(section, artic_id, dynamic, max_rr);

        let (path, sampled_note) = match self
            .patch
            .resolve(section, artic_id, mic, dynamic, note, direction, rr_idx)
        {
            Some(resolved) => resolved,
            None => {
                self.sample_misses
                    .set(self.sample_misses.get().saturating_add(1));
                self.record_sample_miss(format!(
                    "section={section} artic={artic_id} mic={mic} dynamic={dynamic} note={note} direction={direction:?} rr={rr_idx}"
                ));
                tracing::debug!(
                    target: "signal_sampler::trigger",
                    section, artic = artic_id, mic, dynamic, note,
                    direction = ?direction, rr = rr_idx,
                    "sample miss: no matching sample"
                );
                self.trace_push(TraceKind::SampleMiss {
                    note,
                    articulation: artic_id.to_string(),
                    dynamic: dynamic.to_string(),
                    rr: rr_idx,
                    reason: MissReason::NoSample,
                });
                return None;
            }
        };

        // Audio-thread fast path: skip silently when not yet preloaded.
        let Some(data) = self.cache.get_loaded(&path) else {
            self.cache_misses
                .set(self.cache_misses.get().saturating_add(1));
            self.record_cache_miss(&path);
            tracing::debug!(
                target: "signal_sampler::trigger",
                artic = artic_id, note, dynamic, rr = rr_idx,
                path = %path.display(),
                "sample miss: not yet loaded"
            );
            self.trace_push(TraceKind::SampleMiss {
                note,
                articulation: artic_id.to_string(),
                dynamic: dynamic.to_string(),
                rr: rr_idx,
                reason: MissReason::NotLoaded,
            });
            return None;
        };

        // Per-articulation fixed transpose (e.g. CSS Harmonics -12: the shipped
        // natural-harmonic zones sound an octave above CSS's rendered pitch).
        let artic_transpose = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.transpose as i16)
            .unwrap_or(0);
        let semitone_offset = note as i16 - sampled_note as i16 + artic_transpose;
        let mic_index = self.mic_index_for(mic);
        // Cap Release-voice lifetime. lacr release-tail FLACs are 30 s of
        // mostly silence; without this they live in the pool until the
        // sample's natural end and chew through the 64-voice budget, which
        // forces voice-stealing on every subsequent key press and produces
        // intermittent silence/clicks. 2 s is plenty for any real damper
        // click + decay tail.
        let release_lifetime_frames =
            (RELEASE_MAX_LIFETIME_MS as usize) * (self.sample_rate as usize) / 1000;
        // Stem class: releases follow the parent articulation (see
        // `artic_class_for`); direct triggers are classed by their own.
        let artic_class = if matches!(kind, VoiceKind::Release) {
            self.artic_class_for(&self.articulation)
        } else {
            self.artic_class_for(artic_id)
        };
        // Compensate for a sample recorded at a different rate than the engine
        // renders (e.g. Keyscape ships 44.1 kHz packs; the engine runs 48 kHz).
        // Without this the note sounds `output/native` sharp — ~147 cents for
        // 44.1→48 kHz. `src_sr == output` is a no-op (e.g. 48 kHz CSS packs).
        let src_sr = data.sample_rate;
        let voice = Voice::new(
            data,
            note,
            kind.clone(),
            semitone_offset.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            gain,
            release_frames,
        )
        .with_rate_scale(src_sr as f64 / self.sample_rate as f64)
        .with_mic_index(mic_index)
        .with_line(self.cur_line as u8)
        .with_artic_class(artic_class);
        let voice = if matches!(kind, VoiceKind::Release) {
            let end = release_lifetime_frames.min(voice.data_num_frames());
            voice.with_sample_window(0, Some(end))
        } else {
            voice
        };

        // Structured trace + live tracing of the actual spawn — the ground
        // truth for "what sounded on this note". Covers the convention-mode
        // path (Keyscape, drums); the zoned path records its own spawns.
        let rate = 2.0f64.powf(semitone_offset as f64 / 12.0) * (src_sr as f64 / self.sample_rate as f64);
        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        // Loud, filterable flag whenever a `rel_2` / `relm` release variant
        // actually sounds — these are the sharp mechanical-click layers the
        // engine is NOT supposed to select (it asks for `rel`/`relsl`). If this
        // ever fires during play, a resolution path is picking the click layer.
        if fname.contains("rel_2") || fname.contains("relm") {
            tracing::warn!(
                target: "signal_sampler::trigger",
                note, dynamic, rr = rr_idx, gain, file = %fname,
                "REL_2/RELM click-layer release played (should be rel/relsl)"
            );
        }
        tracing::debug!(
            target: "signal_sampler::trigger",
            artic = artic_id, dynamic, note, sampled_note, rr = rr_idx,
            kind = kind.trace_name(), rate, gain,
            file = %fname,
            "voice spawn"
        );
        if self.trace_enabled {
            let voice_id = self.next_trace_voice_id();
            self.trace_push(TraceKind::VoiceSpawn(crate::engine::TraceVoiceSpawn {
                voice_id,
                voice_kind: kind.trace_name(),
                file: path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string(),
                note,
                root_key: sampled_note,
                rate,
                gain,
                dynamic: dynamic.to_string(),
                articulation: artic_id.to_string(),
                mic: mic.to_string(),
                direction: direction.to_string(),
                interval: 0,
                rr: rr_idx,
                start_frame: 0,
                loop_start: 0,
                loop_end: 0,
                loop_xfade: 0,
            }));
        }
        Some(voice)
    }

    /// Returns `(lo_label, hi_label, hi_blend)` for a specific articulation ID
    /// at the current CC1 value, using that articulation's own dynamics count.
    pub(crate) fn layers_for_artic(&self, artic_id: &str) -> (String, String, f32) {
        let n = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        let d = &self.patch.spec.dynamics;
        let layers: &[Cc1Layer] = match n {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => return ("p".into(), "p".into(), 0.0),
        };
        Self::cc1_blend(layers, self.cc1)
    }

    pub(crate) fn record_cache_miss(&self, path: &Path) {
        push_recent(
            &mut self.recent_cache_misses.borrow_mut(),
            path.display().to_string(),
            RECENT_MISS_LIMIT,
        );
    }

    pub(crate) fn record_sample_miss(&self, label: String) {
        push_recent(
            &mut self.recent_sample_misses.borrow_mut(),
            label,
            RECENT_MISS_LIMIT,
        );
    }

    /// Returns `(lo_label, hi_label, hi_blend)` for the current CC1 value,
    /// using the active articulation's own layer set.
    pub(crate) fn current_layers_owned(&self) -> (String, String, f32) {
        Self::cc1_blend(self.active_cc1_layers(), self.cc1)
    }

    /// Core crossfade algorithm: walk adjacent layer pairs and return
    /// `(lo_label, hi_label, hi_blend)` for a given CC value.
    pub(crate) fn cc1_blend(layers: &[Cc1Layer], cc1: u8) -> (String, String, f32) {
        if layers.is_empty() {
            return ("p".into(), "p".into(), 0.0);
        }
        // Walk through adjacent pairs. The crossfade region between layer i
        // and layer i+1 is [layers[i+1].cc_range[0], layers[i].cc_range[1]].
        for i in 0..layers.len().saturating_sub(1) {
            let lo = &layers[i];
            let hi = &layers[i + 1];
            let xfade_start = hi.cc_range[0];
            let xfade_end = lo.cc_range[1];

            if cc1 <= xfade_end {
                if cc1 < xfade_start {
                    return (lo.label.clone(), lo.label.clone(), 0.0);
                } else {
                    let span = (xfade_end - xfade_start + 1).max(1) as f32;
                    let blend = (cc1 - xfade_start) as f32 / span;
                    return (lo.label.clone(), hi.label.clone(), blend);
                }
            }
        }
        let top = &layers[layers.len() - 1];
        (top.label.clone(), top.label.clone(), 0.0)
    }

    /// Active layer pair `(lo, hi, blend)` for the N-layer zoned crossfade.
    /// Crossfade only within each layer pair's overlap (`cc_range`) — kept SHORT
    /// because CSS crossfades differently-recorded dynamic samples briefly to
    /// avoid phasing/doubling. The continuous loudness sweep is handled
    /// separately by [`cc1_expression`](Self::cc1_expression), so short timbre
    /// crossfades don't make CC1 feel stepped.
    pub(crate) fn cc1_blend_idx(layers: &[Cc1Layer], cc1: u8) -> (usize, usize, f32) {
        if layers.is_empty() {
            return (0, 0, 0.0);
        }
        for i in 0..layers.len() - 1 {
            let xfade_start = layers[i + 1].cc_range[0];
            let xfade_end = layers[i].cc_range[1];
            if cc1 <= xfade_end {
                if cc1 < xfade_start {
                    return (i, i, 0.0);
                }
                let span = (xfade_end - xfade_start + 1).max(1) as f32;
                return (i, i + 1, (cc1 - xfade_start) as f32 / span);
            }
        }
        let top = layers.len() - 1;
        (top, top, 0.0)
    }

    /// Continuous CC1 → loudness curve applied on top of the (short) dynamic
    /// crossfade — this is what makes the dynamics feel smooth across the whole
    /// 0–127 range rather than stepping between the recorded layers. A dB ramp
    /// from `CC1_DYN_FLOOR_DB` at CC1=0 up to 0 dB at CC1=127.
    pub(crate) fn cc1_expression(cc1: u8) -> f32 {
        // DATA-DERIVED flat-with-bottom-rolloff (see `CC1_KNEE`/`CC1_FLOOR_DB`).
        // The per-layer CC1 tables handle the TIMBRE crossfade (equal-power, so
        // total level ≈ flat); this only supplies the gentle bottom rolloff the
        // reference render shows (−3 dB at CC1=20, 0 dB from CC1≈45 up).
        let db = if cc1 >= CC1_KNEE {
            0.0
        } else {
            CC1_FLOOR_DB * (CC1_KNEE - cc1) as f32 / CC1_KNEE as f32
        };
        10f32.powf(db / 20.0)
    }

    /// The CC1 layer slice for a given articulation (by its dynamics count).
    pub(crate) fn cc1_layers_for(&self, artic_id: &str) -> &[Cc1Layer] {
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return &[];
        };
        let d = &self.patch.spec.dynamics;
        match artic.dynamics.len() {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => &[],
        }
    }

    /// Return the correct CC1 layer slice for the current articulation.
    pub(crate) fn active_cc1_layers(&self) -> &[Cc1Layer] {
        let Some(artic) = self.patch.spec.articulation(&self.articulation) else {
            return &[];
        };
        let n = artic.dynamics.len();
        let d = &self.patch.spec.dynamics;
        match n {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => &[],
        }
    }

    /// Determine the dynamic layer label for a short note from velocity.
    ///
    /// Named orchestral dynamics (`pp`, `mf`, `fff`, etc.) divide the MIDI
    /// velocity range evenly across the articulation's `dynamics` array.
    /// Numeric Keyscape-style dynamics are different: the labels are the
    /// recorded source velocities, so pick the nearest recorded value.
    pub(crate) fn short_note_dynamic(&self, velocity: u8) -> String {
        self.dynamic_for_artic(&self.articulation, velocity)
    }

    /// Decoded CSS short-note velocity model (KSP `%g1qri` + `$arhiq`).
    ///
    /// Returns `(dynamic_label, velvol_db)`:
    /// - the recorded dynamic is picked by the `%g1qri` velocity bands
    ///   ([`short_band`], mapped 1:1 onto the recorded dynamics) rather than an
    ///   even split, and
    /// - `velvol_db` is the intra-layer velocity→volume `$arhiq`
    ///   ([`short_velocity_volume_db`]), which ramps level continuously across
    ///   each band by the decoded `%bcez1` adjacent-layer dB delta, so a note
    ///   tracks velocity within its layer.
    ///
    /// When the flag is off or the articulation carries no thresholds, falls back
    /// to the even-split dynamic with 0 dB (non-CSS libraries are unchanged).
    pub(crate) fn short_layer_and_velvol(&self, artic_id: &str, velocity: u8) -> (String, f32) {
        if !self.patch.spec.dynamics.enable_velocity_layers {
            return (self.dynamic_for_artic(artic_id, velocity), 0.0);
        }
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return (self.dynamic_for_artic(artic_id, velocity), 0.0);
        };
        let Some((band, n_bands, _num_top, _span)) = self.short_band(artic, velocity) else {
            return (self.dynamic_for_artic(artic_id, velocity), 0.0);
        };
        // Band → recorded dynamic: 1:1 when counts match, else TOP-align (the
        // extra softest recorded dynamics sit below the short's %g1qri floor).
        let n_dyn = artic.dynamics.len();
        let offset = n_dyn.saturating_sub(n_bands);
        let dyn_idx = (band + offset).min(n_dyn - 1);
        let velvol = self.short_velocity_volume_db(artic_id, velocity);
        (artic.dynamics[dyn_idx].clone(), velvol)
    }

    pub(crate) fn dynamic_for_artic(&self, artic_id: &str, velocity: u8) -> String {
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return "p".into();
        };
        if artic.dynamics.is_empty() {
            return "p".into();
        }
        dynamic_label_for_velocity(&artic.dynamics, velocity).unwrap_or_else(|| "p".into())
    }

    /// When CC1 moves and the active articulation is a short-note type, switch
    /// to the sub-type that corresponds to the new CC1 value.
    ///
    /// CSS maps:
    ///   `short_note_cc1_map`  — Spiccato / Staccatissimo / Staccato / Sfz
    ///   `pizzicato_cc1_map`   — Pizzicato / Bartokpizz / Clegno
    pub(crate) fn apply_cc1_short_select(&mut self) {
        let d = &self.patch.spec.dynamics;

        // Determine which map to consult based on current articulation family.
        let in_pizz_family = d
            .pizzicato_cc1_map
            .values()
            .any(|id| id == &self.articulation);
        let in_short_family = d
            .short_note_cc1_map
            .values()
            .any(|id| id == &self.articulation);

        if !in_short_family && !in_pizz_family {
            return; // not in a switchable short-note family
        }

        let map = if in_pizz_family {
            &d.pizzicato_cc1_map
        } else {
            &d.short_note_cc1_map
        };
        let cc1 = self.cc1;

        for (range_str, artic_id) in map {
            if let Some((lo, hi)) = crate::spec::parse_range(range_str) {
                if cc1 >= lo && cc1 <= hi {
                    if self.patch.spec.articulation(artic_id).is_some() {
                        self.articulation = artic_id.clone();
                    }
                    return;
                }
            }
        }
    }

    /// Find the ID of a Legato-type articulation appropriate for the current
    /// section. `retrigger` selects the same-note (Legzero) variant.
    ///
    /// Automatically matches:
    /// - Sordino state: `"Sord"` prefix ↔ current articulation prefix.
    /// - Vibrato state: if the current articulation is NV-type (Nonvib/NVLeg),
    ///   prefer `NVLeg`; otherwise prefer `Leg`. Falls back to any match if
    ///   the preferred variant is absent.
    pub(crate) fn find_legato_artic_id(&self, retrigger: bool) -> Option<String> {
        let want_sord = self.articulation.starts_with("Sord");
        let artic_lower = self.articulation.to_lowercase();
        let prefer_nv = artic_lower.contains("nv") || artic_lower.contains("nonvib");

        let candidates: Vec<&crate::spec::ArticulationSpec> = self
            .patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.kind == ArticulationKind::Legato)
            .filter(|a| {
                a.instrument_filter.is_empty() || a.instrument_filter.contains(&self.section)
            })
            .filter(|a| a.id.starts_with("Sord") == want_sord)
            .filter(|a| {
                let id_lower = a.id.to_lowercase();
                if retrigger {
                    id_lower.contains("zero")
                } else {
                    !id_lower.contains("zero")
                }
            })
            .collect();

        // Prefer NVLeg when in non-vibrato mode, Leg otherwise.
        let preferred = if prefer_nv {
            candidates
                .iter()
                .find(|a| a.id.to_lowercase().contains("nv"))
        } else {
            candidates
                .iter()
                .find(|a| !a.id.to_lowercase().contains("nv"))
        };

        preferred
            .or_else(|| candidates.first())
            .map(|a| a.id.clone())
    }

    /// Map an articulation ID to/from its Con Sordino counterpart.
    ///
    /// `"Vibsus"` + `active=true`  → `"SordVibsus"` (if it exists in the spec)
    /// `"SordVibsus"` + `active=false` → `"Vibsus"` (if it exists in the spec)
    /// Returns the original ID unchanged if no counterpart is found.
    pub(crate) fn remap_sordino(&self, artic_id: &str, active: bool) -> String {
        if active {
            if !artic_id.starts_with("Sord") {
                let sord_id = format!("Sord{artic_id}");
                if self.patch.spec.articulation(&sord_id).is_some() {
                    return sord_id;
                }
            }
        } else if let Some(base) = artic_id.strip_prefix("Sord") {
            if self.patch.spec.articulation(base).is_some() {
                return base.to_string();
            }
        }
        artic_id.to_string()
    }
}
