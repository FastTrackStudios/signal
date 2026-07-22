//! Note-trigger + CC-handler helpers for `SampleEngine`. Split out of
//! `engine/mod.rs`; same impl, separate file.

use super::*;

/// Fade time for the previous same-note body when a key is re-struck (see
/// [`SampleEngine::trigger_short`]). Smooth enough to subsume the old ring, not
/// so long that fast repeats overlap into a pileup.
const RETRIGGER_FADE_MS: u32 = 90;

impl SampleEngine {
    pub(crate) fn trigger_sustain(&mut self, note: u8) {
        let vib_blend = self.cc2_blend();
        let nv_scale = 1.0 - vib_blend;
        let vb_scale = vib_blend;

        // `make_voice` and the layer/vibrato helpers are `&self`, so the
        // current selection (`articulation`/`section`/`mic`) is passed by
        // reference directly — no per-note `String` clones.
        let vib_artic = self.find_vibrato_pair_id(&self.articulation);
        let release_frames = self.release_frames;

        // Compute CC1 layers separately for each articulation so that
        // Vibsus (4 dyns: ppp/p/mf/ff) and Nonvib (3 dyns: p/mf/ff) each
        // use their own crossfade map. Without this, Nonvib gets asked for
        // "ppp" samples that don't exist at very low CC1 values.
        let (nv_lo, nv_hi, nv_cc1_blend) = self.layers_for_artic(&self.articulation);
        let nv_lo_gain = nv_scale * (1.0 - nv_cc1_blend);
        let nv_hi_gain = nv_scale * nv_cc1_blend;

        if let Some(v) = self.make_voice(
            &self.articulation,
            &self.section,
            &self.mic,
            &nv_lo,
            note,
            "",
            VoiceKind::SustainNVLo,
            nv_lo_gain,
            release_frames,
        ) {
            self.voices.spawn(v);
        }
        if nv_hi != nv_lo {
            if let Some(v) = self.make_voice(
                &self.articulation,
                &self.section,
                &self.mic,
                &nv_hi,
                note,
                "",
                VoiceKind::SustainNVHi,
                nv_hi_gain,
                release_frames,
            ) {
                self.voices.spawn(v);
            }
        }

        // Vibrato voices — only if a vibrato-pair articulation exists.
        if let Some(vib_id) = vib_artic {
            let (vb_lo, vb_hi, vb_cc1_blend) = self.layers_for_artic(&vib_id);
            let vb_lo_gain = vb_scale * (1.0 - vb_cc1_blend);
            let vb_hi_gain = vb_scale * vb_cc1_blend;

            if let Some(v) = self.make_voice(
                &vib_id,
                &self.section,
                &self.mic,
                &vb_lo,
                note,
                "",
                VoiceKind::SustainVibLo,
                vb_lo_gain,
                release_frames,
            ) {
                self.voices.spawn(v);
            }
            if vb_hi != vb_lo {
                if let Some(v) = self.make_voice(
                    &vib_id,
                    &self.section,
                    &self.mic,
                    &vb_hi,
                    note,
                    "",
                    VoiceKind::SustainVibHi,
                    vb_hi_gain,
                    release_frames,
                ) {
                    self.voices.spawn(v);
                }
            }
        }
    }

    pub(crate) fn trigger_short(&mut self, note: u8, velocity: u8) {
        // Re-striking the same key re-excites the same string: fade the prior
        // still-ringing body instead of stacking a second full voice. Under a
        // held sustain pedal (note-offs deferred) this is what keeps repeated
        // notes from piling up 30 s voices until the pool steals still-ringing
        // notes ("plays a click, the note doesn't ring"). ~90 ms is a smooth
        // subsume, not an audible cut.
        let retrigger_fade = ms_to_frames(RETRIGGER_FADE_MS, self.sample_rate);
        self.voices.retrigger_fade_note(note, retrigger_fade);

        // Pick dynamic layer based on velocity and spec short_note_cc1_map.
        let dynamic = self.short_note_dynamic(velocity);
        let artic_kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());
        let has_release_artic = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.as_ref())
            .is_some();
        // Voice kind + release length:
        // - has_release_artic (Keyscape pianos, orchestral shorts):
        //   body releases on note-off in KEY_UP_DECLICK_MS (effectively
        //   immediate). The dedicated release_artic supplies the tail.
        // - no release_artic: long natural decay; sample's tail IS the
        //   release.
        let is_oneshot = matches!(artic_kind, Some(ArticulationKind::OneShot));
        let voice_kind = if is_oneshot || has_release_artic {
            VoiceKind::SustainLo
        } else {
            VoiceKind::Short
        };
        let release_frames = if has_release_artic {
            ms_to_frames(KEY_UP_DECLICK_MS, self.sample_rate)
        } else {
            self.release_frames
        };
        // For multi-dynamic articulations (lacrm has 21 dyn-layers), the
        // velocity-selected sample IS already recorded at that loudness,
        // so applying velocity² gain on top double-attenuates the body.
        // Use unity gain when the artic has multiple dynamic layers and
        // fall back to a velocity curve when there's just one layer.
        let n_dyn = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        let gain = if n_dyn > 1 {
            1.0
        } else {
            velocity_gain(velocity)
        };
        // Blend every body LAYER present for this note+dynamic (the base plus
        // any `_2` hard-hit layer) as one coherent round-robin take.
        let primary = self.articulation.clone();
        let mut body_spawned =
            self.spawn_layers(&primary, note, &dynamic, voice_kind.clone(), gain, release_frames);
        if body_spawned {
            self.body_voiced.insert(note);
        } else {
            // Primary articulation has no sample for this note (e.g.
            // Keyscape LA Custom: `lacrm` covers notes 21,22,28-108 but
            // notes 23-27 live in `lacrmsp`). Try every other non-pedal,
            // non-Release OneShot articulation in the spec and use the
            // first that resolves a sample for this note. Without this
            // the user hears only the release-tail click on those notes.
            let alt_ids: Vec<String> = self
                .patch
                .spec
                .articulations
                .iter()
                .filter(|a| matches!(a.kind, ArticulationKind::OneShot | ArticulationKind::Short))
                .filter(|a| {
                    let lower = a.id.to_ascii_lowercase();
                    !lower.contains("ped") && !lower.contains("mech")
                })
                .filter(|a| a.id != primary)
                .map(|a| a.id.clone())
                .collect();
            for alt_id in alt_ids {
                let alt_dyn = self.dynamic_for_artic(&alt_id, velocity);
                if self.spawn_layers(&alt_id, note, &alt_dyn, voice_kind.clone(), gain, release_frames)
                {
                    self.body_voiced.insert(note);
                    body_spawned = true;
                    break;
                }
            }
        }
        // The main body of the note MUST sound. If nothing spawned (sample or
        // cache miss on every candidate), the player hears only the release /
        // pedal click — "a click and no note". Flag it loudly.
        if !body_spawned {
            tracing::warn!(
                target: "signal_sampler::trigger",
                note, velocity, artic = %self.articulation, dynamic = %dynamic,
                "note-on triggered NO body voice (only release/click will sound)"
            );
        }
    }

    /// Spawn every blend LAYER present for `(artic, note, dynamic)` as one
    /// coherent round-robin take — the way the multi-layer Keyscape samples are
    /// meant to sound: a body's base + `_2` hard-hit layer together, or a
    /// release's `rel` + `relm` + `relsl` (+ `rel_2`) together, NOT one picked
    /// at random. Every layer shares the same RR index so the take is coherent.
    /// Falls back to a single nearest-match voice when the exact note+dynamic
    /// has no layers indexed. Returns true if any voice sounded.
    pub(crate) fn spawn_layers(
        &mut self,
        artic: &str,
        note: u8,
        dynamic: &str,
        kind: VoiceKind,
        gain: f32,
        release_frames: usize,
    ) -> bool {
        let section = self.section.clone();
        let mic = self.mic.clone();
        let layers = self
            .patch
            .map
            .layer_directions(&section, artic, &mic, note, dynamic);
        if layers.is_empty() {
            // No exact (note, dynamic) layers — single nearest-match voice.
            if let Some(v) =
                self.make_voice(artic, &section, &mic, dynamic, note, "", kind, gain, release_frames)
            {
                self.voices.spawn(v);
                return true;
            }
            return false;
        }
        let rr_idx = self.next_rr(&section, artic, dynamic);
        let mut any = false;
        for dir in &layers {
            if let Some(v) = self.make_voice_at_rr(
                artic,
                &section,
                &mic,
                dynamic,
                note,
                dir,
                kind.clone(),
                gain,
                release_frames,
                rr_idx,
            ) {
                self.voices.spawn(v);
                any = true;
            }
        }
        any
    }

    pub(crate) fn trigger_legzero(&mut self, note: u8, _velocity: u8) {
        // Find a Legato-kind articulation for same-note retrigger.
        let Some(rz_id) = self.find_legato_artic_id(true) else {
            return;
        };
        let (lo_dyn, _, _) = self.current_layers_owned();
        let release_frames = self.release_frames;

        if let Some(v) = self.make_voice(
            &rz_id,
            &self.section,
            &self.mic,
            &lo_dyn,
            note,
            "",
            VoiceKind::Legato,
            1.0,
            release_frames,
        ) {
            self.voices.spawn(v);
        }
    }

    pub(crate) fn initiate_legato(&mut self, to_note: u8, velocity: u8) {
        let from_note = *self
            .held_notes
            .keys()
            .find(|&&n| n != to_note)
            .unwrap_or(&to_note);

        // Check portamento threshold (default 20, velocity ≤ threshold triggers glide).
        let port_thresh = self
            .patch
            .spec
            .legato_engine
            .as_ref()
            .and_then(|le| le.portamento.as_ref())
            .map(|p| p.trigger_vel_max)
            .unwrap_or(0); // 0 disables portamento
        let cc5 = self.cc_values[5];
        let portamento = port_thresh > 0 && velocity <= port_thresh && cc5 > 10;

        // IOI-driven Overlap-Delay (spec §2.1), same as the zoned path.
        let now = self.frames_rendered;
        let ioi_frames = now.saturating_sub(self.line().last_onset_frame);
        self.line_mut().last_onset_frame = now;
        let ioi_ms = frames_to_ms(ioi_frames, self.sample_rate);
        let delay_ms = if portamento {
            0 // portamento fires immediately — the glide pitch ramp is the "delay"
        } else {
            self.patch
                .spec
                .legato_cfg()
                .overlap_delay_ms(ioi_ms, velocity, self.legato_expressive)
        };

        let frames_remaining = ms_to_frames(delay_ms, self.sample_rate);

        // Reactive path (see start_legato_transition) — count it.
        self.reactive_legato_fires = self.reactive_legato_fires.saturating_add(1);
        self.line_mut().state = LegatoState::Pending {
            frames_remaining,
            from_note,
            to_note,
            to_note_velocity: velocity,
            portamento,
            ioi_ms,
        };
    }

    pub(crate) fn fire_legato(
        &mut self,
        from_note: u8,
        to_note: u8,
        velocity: u8,
        portamento: bool,
        ioi_ms: f32,
    ) {
        self.fire_legato_with_lead(from_note, to_note, velocity, portamento, None, ioi_ms);
    }

    /// [`fire_legato`](Self::fire_legato) with the document scheduler's
    /// prefire lead (frames until the destination tick) — the transition
    /// sample's surplus lead-in is skipped so the pitch change lands exactly
    /// on the tick, and the old note crossfades out across the audible
    /// lead-in.
    pub(crate) fn fire_legato_with_lead(
        &mut self,
        from_note: u8,
        to_note: u8,
        velocity: u8,
        portamento: bool,
        sched_lead: Option<u64>,
        ioi_ms: f32,
    ) {
        let log_idx = if self.legato_fire_log_enabled
            && self.legato_fire_log.len() < LEGATO_FIRE_LOG_CAP
        {
            self.legato_fire_log.push(LegatoFireEvent {
                frame: self.frames_rendered,
                line: self.cur_line as u8,
                from_note,
                to_note,
                velocity,
                portamento,
                // Patched below from `last_arrival_prediction` once the
                // transition voice has spawned and the actually-chosen zone's
                // arrival marker (lead_in) and start offset are known.
                arrival: self.frames_rendered,
            });
            Some(self.legato_fire_log.len() - 1)
        } else {
            None
        };
        // Default: arrival == fire frame (re-bow / legacy / non-zoned);
        // `spawn_transition_voice` overwrites for measured transition zones.
        self.last_arrival_prediction = self.frames_rendered;
        self.trace_push(TraceKind::Transition {
            from: from_note,
            to: to_note,
            portamento,
        });
        let direction = if to_note > from_note { "up" } else { "down" };

        // Retire the PREVIOUS pair on THIS line (spec §2.1 step 4) — the old
        // transition and old sustain fade out on SEPARATE, long crossfades
        // (real persistent values, indexed by attack-velocity range), so the
        // new pair emerges under them with no inter-note tick. Line-scoped so a
        // unison note held by another divisi line keeps sounding.
        let (retire_trans_ms, retire_sus_ms) = self.patch.spec.legato_cfg().retire_fades_ms(velocity);
        let trans_fade = ms_to_frames(retire_trans_ms, self.sample_rate);
        let sus_fade = ms_to_frames(retire_sus_ms, self.sample_rate);
        self.voices
            .retire_note_line(self.cur_line as u8, from_note, trans_fade, sus_fade);

        // Zoned libraries (CSS): the decoded KSP model spawns THREE voices per
        // legato note. We model the two that carry it:
        //   1. `%ftriy` — the ONE-SHOT bow-change TRANSITION (attack ornament),
        //      one-shot (no tail loop), retired (faded) when the NEXT legato
        //      note arrives. Plays at recorded level × CC1 (no makeup).
        //   2. `%grhcg` — the MAIN held SUSTAIN: `play_note(note,$jabns,0,-1)`
        //      plays IMMEDIATELY at full level, then `change_vol(…,$3tsb0)` = −6 dB.
        //      It loops on the real loop points and carries the note. Spawned
        //      immediately with a declick fade only (NOT muted/faded ~1 s), and
        //      gained −6 dB via `legato_sustain` so it sits ~6 dB below a fresh
        //      first note.
        //   (`%1wcdh` — the slow secondary bloom overlay, `$foyeb/$g4dbu` 1 s/1 s —
        //    is left unmodeled; the transition + immediate sustain already cover it.)
        // The previous sustain + transition at `from_note` were already
        // crossfaded out above (`retire_note_line`).
        if self.patch.is_zoned() {
            self.play_direction = direction.to_string();
            // 1. One-shot bow-change transition (`%ftriy`).
            self.spawn_legato_transition(
                from_note, to_note, velocity, portamento, sched_lead, ioi_ms,
            );
            // 2. Main held sustain (`%grhcg`) — full level, declick only,
            //    carrying the −6 dB `$3tsb0` legato makeup. In REACTIVE play
            //    the fire happens AFTER the musical tick (the CSS latency),
            //    so "immediate at fire" enters at/after the beat — but a
            //    document PREFIRE happens `sched_lead` BEFORE the tick, and
            //    an immediate destination sustain would put the arrived
            //    note's pitch in the air up to a whole velocity-zone delay
            //    early (the "underlay masking" that reads as wide intervals
            //    arriving before the click). Hold it back to the tick: the
            //    transition sample owns the pre-arrival air, exactly as it
            //    does in Kontakt.
            let declick = ms_to_frames(SUSTAIN_DECLICK_MS, self.sample_rate);
            let hold = sched_lead.unwrap_or(0) as usize;
            self.sustain_fade_in = Some((hold, declick));
            self.legato_sustain = true;
            self.trigger_zoned_sustain(to_note);
            self.legato_sustain = false;
            self.sustain_fade_in = None;
            self.line_mut().note = Some(to_note);
            if let Some(i) = log_idx {
                self.legato_fire_log[i].arrival = self.last_arrival_prediction;
            }
            return;
        }

        // For portamento, look for Port-type articulation; otherwise Leg/NVLeg.
        let leg_id = if portamento {
            self.find_port_artic_id()
                .or_else(|| self.find_legato_artic_id(false))
        } else {
            self.find_legato_artic_id(false)
        };

        let Some(leg_id) = leg_id else {
            self.trigger_sustain(to_note);
            return;
        };

        let (lo_dyn, _, _) = self.current_layers_owned();
        let release_frames = self.release_frames;

        // Try directional first; fall back to directionless if not found.
        let v = self
            .make_voice(
                &leg_id,
                &self.section,
                &self.mic,
                &lo_dyn,
                to_note,
                direction,
                VoiceKind::Legato,
                1.0,
                release_frames,
            )
            .or_else(|| {
                self.make_voice(
                    &leg_id,
                    &self.section,
                    &self.mic,
                    &lo_dyn,
                    to_note,
                    "",
                    VoiceKind::Legato,
                    1.0,
                    release_frames,
                )
            });

        if let Some(v) = v {
            self.voices.spawn(v);
        }
        // Always trigger a background sustain so the note doesn't go silent
        // when the legato transition sample finishes. The Leg sample provides
        // the attack character; the Vibsus/Nonvib body takes over after it ends.
        self.trigger_sustain(to_note);
        if let Some(i) = log_idx {
            self.legato_fire_log[i].arrival = self.last_arrival_prediction;
        }
    }

    /// Find the Port articulation matching the current sordino state.
    pub(crate) fn find_port_artic_id(&self) -> Option<String> {
        let want_sord = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.is_sordino())
            .unwrap_or_else(|| self.articulation.starts_with("Sord"));
        self.patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.kind == ArticulationKind::Legato)
            .filter(|a| a.is_sordino() == want_sord)
            .find(|a| a.resolve_legato_role() == crate::spec::LegatoRole::Portamento)
            .map(|a| a.id.clone())
    }

    /// Number of distinct-note span an articulation must cover to count as a
    /// real playable BODY (rather than a fixed pedal-noise layer). Keyscape
    /// pedal-noise packs (`lacrped`, `wingpedal…`) index the pedal STATE as
    /// the sample "note" and so span only 1–2 notes; a genuine pedal-down
    /// body spans most of the keyboard.
    const PEDAL_BODY_MIN_SPAN: u8 = 12;

    /// Keyboard span (highest − lowest sampled note + 1) of an articulation in
    /// the sample map, or 0 if it has no samples. Cheap max/min scan — no
    /// allocation — invoked only on pedal transitions, never per-sample.
    pub(crate) fn artic_note_span(&self, artic_id: &str) -> u8 {
        let (mut lo, mut hi, mut any) = (u8::MAX, 0u8, false);
        for (k, _) in self.patch.map.iter() {
            if k.articulation == artic_id {
                lo = lo.min(k.note);
                hi = hi.max(k.note);
                any = true;
            }
        }
        if any {
            hi - lo + 1
        } else {
            0
        }
    }

    /// Find a sustain-pedal-down BODY sibling of `base` in the spec — a full
    /// alternate keymap that replaces the played articulation while the pedal
    /// is held (some libraries ship a distinct pedal-down resonance body).
    ///
    /// Candidates must span a real keyboard range ([`Self::PEDAL_BODY_MIN_SPAN`]):
    /// pedal-NOISE articulations (`lacrped`, `wingpedal…`) map only 1–2 fixed
    /// samples and must NOT replace the body — otherwise every note held under
    /// the pedal plays the pedal clunk instead of the instrument. Pedal noise
    /// is handled separately by [`Self::trigger_pedal_noise`].
    ///
    /// Naming conventions vary; we try in priority:
    ///   1. `<base>ped`                              — `lacrm` → `lacrmped`
    ///   2. `<base>` with trailing `m` removed + `ped` — `lacrm` → `lacrped`
    ///   3. any non-Release / non-mechanical articulation containing `ped`
    ///
    /// Mechanical-pedal articulations (`mech`) are excluded.
    pub(crate) fn find_pedal_pair(&self, base: &str) -> Option<String> {
        let is_body = |id: &str| self.artic_note_span(id) >= Self::PEDAL_BODY_MIN_SPAN;
        let id = |s: &str| -> Option<String> {
            self.patch
                .spec
                .articulation(s)
                .filter(|a| !matches!(a.kind, ArticulationKind::Release))
                .map(|a| a.id.clone())
                .filter(|id| is_body(id))
        };
        if let Some(v) = id(&format!("{base}ped")) {
            return Some(v);
        }
        let trimmed = base.strip_suffix('m').unwrap_or(base);
        if trimmed != base {
            if let Some(v) = id(&format!("{trimmed}ped")) {
                return Some(v);
            }
        }
        self.patch
            .spec
            .articulations
            .iter()
            .filter(|a| {
                !matches!(a.kind, ArticulationKind::Release)
                    && a.id.to_ascii_lowercase().contains("ped")
                    && !a.id.to_ascii_lowercase().contains("mech")
            })
            .map(|a| a.id.clone())
            .find(|id| is_body(id))
    }

    /// Fire every pedal-NOISE articulation (felt `lacrped` + mechanical
    /// `lacrmechped`, `wingpedal…`) once as a one-shot ambience layer when the
    /// sustain pedal crosses. These packs index the pedal STATE as the sample
    /// "note" (0 = up/release, 1 = down/press), so `down` selects which. A
    /// noise artic is one that fails the body-span test. Silently no-ops when
    /// the pack ships no pedal noise.
    ///
    /// The noise gain tracks the recent playing dynamic: these packs ship a
    /// single fixed-level sample, so without scaling the mechanical clunk sits
    /// at full volume even under a pianissimo passage. Scale by the smoothed
    /// recent velocity (with a small floor so it never vanishes entirely).
    pub(crate) fn trigger_pedal_noise(&mut self, down: bool) {
        let state_note = u8::from(down); // 1 = pedal down/press, 0 = up/release
        let ids: Vec<String> = self
            .patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.id.to_ascii_lowercase().contains("ped"))
            .filter(|a| self.artic_note_span(&a.id) < Self::PEDAL_BODY_MIN_SPAN)
            .map(|a| a.id.clone())
            .collect();
        let release_frames = self.release_frames;
        // Velocity-scaled: soft playing → soft pedal noise. Floor at 0.2 of
        // full so the mechanism stays faintly present. Mechanical (`mechped`)
        // and felt (`ped`) noise have independent base levels (both -20 dB by
        // default, matching Keyscape).
        let v_scale = (self.recent_velocity as f32 / 110.0)
            .clamp(0.0, 1.0)
            .powf(1.2)
            .max(0.2);
        for id in ids {
            let is_mech = id.to_ascii_lowercase().contains("mech");
            let base = if is_mech {
                self.mech_noise_gain
            } else {
                self.pedal_noise_gain
            };
            let gain = base * v_scale;
            let dyn_id = self.dynamic_for_artic(&id, 100);
            if let Some(v) = self.make_voice(
                &id,
                &self.section,
                &self.mic,
                &dyn_id,
                state_note,
                "",
                VoiceKind::Release,
                gain,
                release_frames,
            ) {
                self.voices.spawn(v);
            }
        }
    }

    pub(crate) fn do_note_off_with_release_frames(&mut self, note: u8, velocity: u8, release_frames: usize) {
        // Trigger release trail if the current articulation specifies one.
        let release_artic = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.clone());

        if let Some(rel_id) = release_artic
            .as_ref()
            .filter(|_| velocity >= RELEASE_SAMPLE_VELOCITY_MIN)
        {
            // The release-tail DYNAMIC follows the note-on strike (controllers
            // send 0/64 "no info" on note-off), and its GAIN sits a fixed dB
            // UNDER the note body that actually sounded. The body samples encode
            // velocity by loudness (a soft note is a genuinely quiet recording),
            // so a fixed-level release drowns a soft note — "plays a mechanical
            // click, no note". Scaling to the body's measured peak keeps the
            // key-up/damper noise subordinate at every dynamic (Keyscape's
            // release default is -10 dB below the note).
            let strike = {
                let s = self.note_strike_vel[note as usize];
                if s > 0 { s } else { velocity.max(1) }
            };
            let rel_dyn = self.dynamic_for_artic(rel_id, strike);
            // Scale the release STRICTLY to the note body that is still
            // sounding. A note held until it decays (or the voice auto-retired)
            // has a body_peak near zero — firing a release then is a "thump"
            // out of nowhere on a note that already faded. So gate on the body
            // still being audible; no guessing from strike velocity.
            let body_peak = self.voices.note_body_peak(note);
            let gain = self.release_gain * body_peak;
            let had_body = self.body_voiced.remove(&note);
            let fire = had_body && body_peak >= RELEASE_BODY_FLOOR;
            tracing::debug!(
                target: "signal_sampler::trigger",
                note, strike, gain, body_peak, fire, dyn = %rel_dyn, "release tail"
            );

            if fire {
                // Blend ALL release layers (`rel` + `relm` + `relsl` + `rel_2`)
                // as one coherent take — the composite Keyscape release, rather
                // than picking one variant.
                let rel_id = rel_id.clone();
                self.spawn_layers(
                    &rel_id,
                    note,
                    &rel_dyn,
                    VoiceKind::Release,
                    gain,
                    release_frames,
                );
            }
        } else {
            // No release_artic at all — still need to clear the tracker
            // so a future re-trigger doesn't carry stale state.
            self.body_voiced.remove(&note);
        }

        // Body voices were spawned with a short release window when the
        // articulation has a release_artic (the release sample supplies
        // the natural tail). Use the short cutoff here regardless of the
        // caller's `release_frames` — that parameter is meant for the
        // release voice's own decay, not for the body's fade.
        let body_release = if release_artic.is_some() {
            ms_to_frames(KEY_UP_DECLICK_MS, self.sample_rate)
        } else {
            release_frames
        };
        self.voices
            .note_off_with_release_frames(note, Some(body_release));
    }

    // ── Private — CC handlers ─────────────────────────────────────────────────

    pub(crate) fn pedal_release_frames(&self) -> usize {
        half_pedal_release_frames(
            self.release_frames,
            self.cc64_value,
            &self.patch.spec.dynamics.half_pedal_curve,
            self.patch.spec.dynamics.half_pedal_max_release_multiplier,
        )
    }

    /// Recompute and ramp sustain voice gains when the ACTIVE line's CC1 or
    /// CC2 changes. Only voices belonging to the active line re-level — each
    /// divisi line's dynamics ride its own controller lane. (Live play keeps
    /// everything on line 0, so behavior is unchanged.)
    pub(crate) fn update_sustain_gains(&mut self) {
        let cur_line = self.cur_line as u8;
        // CC2 → non-vib/vib balance (equal-power).
        let (nv, vb) = Self::equal_power(self.cc2_blend());
        let ramp = self.cc1_ramp_frames;
        let nv_artic = self.articulation.clone();
        let vib_artic = self.find_vibrato_pair_id(&nv_artic);

        // Per-side CC1 crossfade across ALL dynamic layers: the active pair
        // (lo,hi) get equal-power gains, every other layer is silent. Held
        // SustainLayer voices are re-levelled by index → full-range swell.
        let (nv_lo, nv_hi, nv_b) = Self::cc1_blend_idx(self.cc1_layers_for(&nv_artic), self.cc1);
        let (nv_lo_g, nv_hi_g) = Self::equal_power(nv_b);
        let (vb_lo, vb_hi, vb_lo_g, vb_hi_g) = match vib_artic.as_deref() {
            Some(vib) => {
                let (l, h, b) = Self::cc1_blend_idx(self.cc1_layers_for(vib), self.cc1);
                let (lg, hg) = Self::equal_power(b);
                (l, h, lg, hg)
            }
            None => (0, 0, 0.0, 0.0),
        };

        // Continuous loudness sweep on top of the (short) timbre crossfade.
        let expr = self.cc1_expression(self.cc1);
        for v in self.voices.voices_mut() {
            if v.line != cur_line {
                continue;
            }
            if let Some(layer) = v.dyn_layer {
                let i = layer.index as usize;
                let g = if layer.vib {
                    vb * layer_gain(i, vb_lo, vb_hi, vb_lo_g, vb_hi_g)
                } else {
                    nv * layer_gain(i, nv_lo, nv_hi, nv_lo_g, nv_hi_g)
                };
                v.ramp_gain(g * expr, ramp);
            }
        }

        // Legacy 2-layer kinds (non-zoned trigger_sustain path) — unchanged.
        self.voices.update_sustain_blend(
            cur_line,
            nv * nv_lo_g,
            nv * nv_hi_g,
            vb * vb_lo_g,
            vb * vb_hi_g,
            ramp,
        );
    }

    /// Equal-power crossfade gains for a blend in `[0,1]`: returns `(lo, hi)` =
    /// `(cos, sin)` of the quarter-turn. Keeps perceived loudness constant
    /// across the fade — the smooth curve CSS uses for dynamics/vibrato, vs a
    /// linear fade which dips ~3 dB through the middle.
    pub(crate) fn equal_power(blend: f32) -> (f32, f32) {
        let b = blend.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
        (b.cos(), b.sin())
    }

    /// Compute the vibrato blend factor [0.0, 1.0] from the current CC2 value.
    ///
    /// `"on_off"` mode (CSSS): snaps at 64. All other libraries: linear.
    pub(crate) fn cc2_blend(&self) -> f32 {
        match self.patch.spec.dynamics.vibrato_mode.as_deref() {
            Some("on_off") => {
                if self.cc2 >= 64 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => self.cc2 as f32 / 127.0,
        }
    }

    /// Find the vibrato counterpart of `artic_id`.
    ///
    /// CSS/CSSS convention: if the current artic has no "NV"/"Nonvib" in its
    /// name we look for one that does (and vice-versa), staying within the
    /// same family (Con Sordino vs regular).
    pub(crate) fn find_vibrato_pair_id(&self, artic_id: &str) -> Option<String> {
        self.patch.spec.vibrato_counterpart(artic_id)
    }

    /// Map CC58 → action and execute it.
    ///
    /// Several CC58 values are **mode switches** rather than articulation selectors:
    /// - `"Con Sordino On"` / `"Con Sordino Off"` → toggle sordino sample set
    /// - `"Legato On"` / `"Legato Off"` → enable/disable legato processing
    /// - `"Sustain: Low Latency Legato"` / `"Sustain: Expressive Legato"` → select
    ///   legato pre-delay mode (latency/expressive) without changing the articulation
    ///
    /// All other labels are treated as articulation IDs or display labels. If Con
    /// Sordino mode is active, the matched articulation is remapped to its sordino
    /// counterpart.
    /// If `note` is a configured keyswitch, apply its velocity-mapped value and
    /// return `true` (the note is consumed — keyswitches don't sound).
    pub(crate) fn try_keyswitch(&mut self, note: u8, velocity: u8) -> bool {
        let Some(&idx) = self.keyswitch_notes.get(&note) else {
            return false;
        };
        let value = self
            .patch
            .spec
            .keyswitch
            .as_ref()
            .and_then(|ks| ks.notes.get(idx))
            .and_then(|kn| kn.value_for(velocity))
            .map(|s| s.to_string());
        if let Some(v) = value {
            self.apply_keyswitch_value(&v);
        }
        true
    }

    /// Apply a keyswitch value: `+`-joined tokens, each either an `@mode` token
    /// (`@legato-on`, `@legato-expressive`, `@sordino-off`, …) or a zone
    /// articulation tag (`Spiccato`, `Leg`, …).
    pub(crate) fn apply_keyswitch_value(&mut self, value: &str) {
        for tok in value.split('+').map(str::trim).filter(|t| !t.is_empty()) {
            match tok {
                "@ignore" => {}
                "@legato-on" => self.legato_enabled = true,
                "@legato-off" => self.legato_enabled = false,
                "@legato-low" => {
                    self.legato_enabled = true;
                    self.legato_expressive = false;
                }
                "@legato-expressive" => {
                    self.legato_enabled = true;
                    self.legato_expressive = true;
                }
                "@sordino-on" => self.set_con_sordino(true),
                "@sordino-off" => self.set_con_sordino(false),
                // Force full non-vibrato (the CSS Non-Vib keyswitch): CC2 → 0.
                "@novib" => {
                    self.cc2 = 0;
                    self.line_mut().cc2 = 0;
                    self.update_sustain_gains();
                }
                t if t.starts_with('@') => tracing::debug!("unknown keyswitch token {t:?}"),
                tag => self.select_articulation_tag(tag),
            }
        }
    }

    /// Select an articulation by tag/id/label: prefer a matching declared
    /// articulation, else use the tag verbatim (zone tags like `"Leg"` aren't
    /// always declared as articulations). Honours Con Sordino remapping.
    pub(crate) fn select_articulation_tag(&mut self, tag: &str) {
        let id = self
            .patch
            .spec
            .articulations
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(tag) || a.label.eq_ignore_ascii_case(tag))
            .map(|a| a.id.clone())
            .unwrap_or_else(|| tag.to_string());
        self.articulation = self.remap_sordino(&id, self.con_sordino);
    }

    /// Apply a latched-CC selector value (spec `selector uacc`): the CC's
    /// value is a code in the resolved code → articulation map. A matching
    /// code latches that articulation for all subsequent notes — identical
    /// semantics to a keyswitch latch (StrictLive: a CC arriving before a
    /// note-on selects the articulation the notes after it play). Unknown
    /// codes keep the previous latch (published code tables leave gaps by
    /// design). Honours Con Sordino remapping via `select_articulation_tag`.
    // r[impl signal.sampling.articulation.select]
    pub(crate) fn apply_latched_cc_selector(&mut self, value: u8) {
        let Some(id) = self
            .latched_cc_selector
            .as_ref()
            .and_then(|sel| sel.artic_for(value))
            .map(str::to_string)
        else {
            return;
        };
        self.select_articulation_tag(&id);
        // A direct selection cancels any pending CC58 velocity-split group.
        self.pending_cc58_group = None;
    }

    pub(crate) fn apply_cc58(&mut self) {
        let Some(ks) = self.patch.spec.keyswitch.as_ref() else {
            return;
        };
        let Some(label) = ks.cc58_function(self.cc58) else {
            return;
        };
        let label = label.to_string();

        // ── Mode switches ────────────────────────────────────────────────────
        match label.as_str() {
            "Con Sordino On" => {
                self.set_con_sordino(true);
                return;
            }
            "Con Sordino Off" => {
                self.set_con_sordino(false);
                return;
            }
            "Legato On" => {
                self.legato_enabled = true;
                return;
            }
            "Legato Off" => {
                self.legato_enabled = false;
                return;
            }
            "Sustain: Low Latency Legato" => {
                self.legato_enabled = true;
                self.legato_expressive = false;
                self.select_articulation_tag("Nonvib");
                return;
            }
            "Sustain: Expressive Legato" => {
                self.legato_enabled = true;
                self.legato_expressive = true;
                self.select_articulation_tag("Nonvib");
                return;
            }
            "Measured Tremolo" => {
                // Scripted mode — no dedicated samples. Cannot replicate without a
                // built-in scripted repeating trigger. Ignore for now.
                return;
            }
            _ => {}
        }

        // ── Articulation selection ───────────────────────────────────────────
        let matched = self
            .patch
            .spec
            .articulations
            .iter()
            .find(|a| a.id == label || a.label == label)
            .map(|a| a.id.clone());

        if let Some(id) = matched {
            self.articulation = self.remap_sordino(&id, self.con_sordino);
            self.pending_cc58_group = None;
            return;
        }

        // No single articulation carries this label — it names a velocity-split
        // keyswitch GROUP (e.g. "Trills" → HTrills/WTrills, "Marcato (…)" →
        // Marcato). Match it to a keyswitch NOTE by group label (ignoring any
        // " (…)" qualifier) and defer the concrete pick to note-on velocity,
        // exactly like striking that KS note (`try_keyswitch` / `value_for`).
        let stem = label.split(" (").next().unwrap_or(&label).trim();
        self.pending_cc58_group = ks.notes.iter().position(|kn| {
            kn.label.eq_ignore_ascii_case(&label) || kn.label.eq_ignore_ascii_case(stem)
        });
    }

}
