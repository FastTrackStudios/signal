//! MIDI input for `SampleEngine` — note-on/off, CC dispatch, articulation
//! selection. Split out of `engine/mod.rs`; same impl, separate file.

use super::*;

impl SampleEngine {
    /// Process a MIDI note-on event.
    /// Note-on with a per-trigger articulation override (percussion routing).
    /// `articulation` fires only the matching articulation's zones for this
    /// hit, ignoring key — letting one shared drum pack serve many routed
    /// notes. `None` behaves like [`note_on`](Self::note_on).
    pub fn note_on_articulated(&mut self, note: u8, velocity: u8, articulation: Option<&str>) {
        let prev = self.trigger_articulation.take();
        self.trigger_articulation = articulation
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        self.note_on(note, velocity);
        self.trigger_articulation = prev;
    }

    /// Resolve a pending CC58 velocity-GROUP (Trills → HTrills/WTrills, Marcato
    /// overlay variants → Marcato) to its concrete articulation using THIS
    /// note's velocity — identical to striking the equivalent keyswitch note.
    /// No-op unless the last CC58 selected a velocity-split group. Skipped for
    /// keyswitch notes (they select, not sound).
    pub(crate) fn resolve_pending_cc58(&mut self, velocity: u8) {
        let Some(gi) = self.pending_cc58_group else {
            return;
        };
        if let Some(v) = self
            .patch
            .spec
            .keyswitch
            .as_ref()
            .and_then(|ks| ks.notes.get(gi))
            .and_then(|kn| kn.value_for(velocity))
            .map(|s| s.to_string())
        {
            self.apply_keyswitch_value(&v);
        }
    }

    /// Whether `artic` uses the monophonic legato-transition machinery. TRUE for
    /// a Legato-kind articulation, or a main sustain that has a *reciprocal* CC2
    /// vibrato pair (Nonvib↔Vibsus). FALSE for Tremolo/Harmonics/Col Legno/
    /// Trills/Marcato — `Sustain`/`Trill` kind with no reciprocal pair and no
    /// legato transitions, so in CSS every note is a fresh attack even while the
    /// global legato toggle is on. (Reciprocity is what rejects the false
    /// `find_vibrato_pair_id` matches: Tremolo→Nonvib, but Nonvib→Vibsus.)
    pub(crate) fn is_legato_capable_artic(&self, artic: &str) -> bool {
        let kind = self.patch.spec.articulation(artic).map(|a| a.kind.clone());
        if matches!(kind, Some(ArticulationKind::Legato)) {
            return true;
        }
        if !matches!(
            kind,
            Some(ArticulationKind::Sustain | ArticulationKind::Looped)
        ) {
            return false;
        }
        // Pacific-style libraries have no CC2 vibrato pairs at all — any main
        // sustain is legato-capable when transition zones exist.
        if self.patch.spec.legato_cfg().style == crate::spec::LegatoStyle::Pacific {
            return true;
        }
        self.find_vibrato_pair_id(artic)
            .and_then(|p| self.find_vibrato_pair_id(&p))
            .as_deref()
            == Some(artic)
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        // A pending CC58 velocity-group resolves to its concrete articulation
        // from THIS note's velocity before any routing decision — so the live
        // auto-divisi gate below and every downstream path see the real artic
        // (a playable note; keyswitch notes select via `try_keyswitch` instead).
        if velocity > 0 && !self.keyswitch_notes.contains_key(&note) {
            self.resolve_pending_cc58(velocity);
        }

        // Live auto-divisi (StrictLive + mono-legato patch): the greedy line
        // allocator decides which mono line the note belongs to and whether
        // it may legato — see `live_divisi_note_on`. Lookahead mode has no
        // such gates (the document knows the actual voice-leading), and
        // non-legato material (shorts, percussion, pianos) is line-agnostic.
        if velocity > 0
            && self.play_mode == PlayMode::StrictLive
            && self.live_divisi_applicable()
            && !self.keyswitch_notes.contains_key(&note)
        {
            self.live_divisi_note_on(note, velocity);
            return;
        }
        self.note_on_line(0, note, velocity);
    }

    /// Whether the live auto-divisi allocator governs note-ons right now:
    /// zoned patch, legato enabled, and a sustain-family articulation
    /// selected (the mono-legato branch of `note_on_line`).
    pub(crate) fn live_divisi_applicable(&self) -> bool {
        self.patch.is_zoned()
            && self.legato_enabled
            && self.is_legato_capable_artic(&self.articulation)
    }

    /// Live auto-divisi with legato gating (see `docs/plan/document-mode.md`,
    /// "Auto-divisi" → live gating). Greedy, reactive, deterministic given
    /// the same input event stream:
    ///
    /// - **Simultaneity gate**: notes within `live_chord_window_ms` of the
    ///   previous onset are a chord — fresh sustain attacks on separate
    ///   lines (allocated in arrival order; a zero-latency engine cannot
    ///   buffer to rank a chord it hasn't finished hearing), never legato.
    /// - **Interval gate**: a note continues an existing line as LEGATO only
    ///   if within `live_legato_interval_max` semitones of that line's
    ///   sounding note (nearest line wins; ties → lowest line id), or of a
    ///   line's just-released note (abutment within the chord window).
    ///   Transitions run through the reactive countdown, which StrictLive
    ///   times from the `low_latency` tables.
    /// - Otherwise: fresh attack on a free line (LRU-silent first; if every
    ///   line is sounding, the least-recently-active line is retagged).
    pub(crate) fn live_divisi_note_on(&mut self, note: u8, velocity: u8) {
        let now = self.frames_rendered;
        let window = ms_to_frames(
            self.patch.spec.live_chord_window_ms.max(0.0).round() as u32,
            self.sample_rate,
        ) as u64;
        let interval_max = self.patch.spec.live_legato_interval_max as i16;
        let chord = self
            .live_last_onset
            .is_some_and(|f| now.saturating_sub(f) <= window);
        self.live_last_onset = Some(now);

        // Legato continuation target: nearest sounding line within the
        // interval gate, else a just-released line whose note abuts.
        // `(interval, line, released-from)`; ascending line order breaks ties.
        let mut target: Option<(i16, LineId, Option<u8>)> = None;
        if !chord {
            for (li, l) in self.lines.iter().enumerate() {
                if let Some(cur) = l.note {
                    let iv = (cur as i16 - note as i16).abs();
                    if iv <= interval_max && target.is_none_or(|(best, _, _)| iv < best) {
                        target = Some((iv, li, None));
                    }
                }
            }
            if target.is_none() {
                for (li, l) in self.lines.iter().enumerate() {
                    if l.note.is_some() || now.saturating_sub(l.last_release_frame) > window {
                        continue;
                    }
                    if let Some(rel) = l.released_note {
                        let iv = (rel as i16 - note as i16).abs();
                        if iv <= interval_max && target.is_none_or(|(best, _, _)| iv < best) {
                            target = Some((iv, li, Some(rel)));
                        }
                    }
                }
            }
        }

        let li = match target {
            Some((_, li, _)) => li,
            None => self.alloc_free_line(),
        };
        self.set_active_line(li);
        self.held_notes.insert(note, velocity);
        let l = self.line_mut();
        l.order.retain(|&n| n != note);
        l.order.push(note);
        l.last_activity = now;
        l.released_note = None;

        match target {
            // Continue the line: reactive transition (low_latency-timed).
            Some((_, _, None)) => {
                let cur = self.line().note.expect("sounding target line");
                self.start_legato_transition(cur, note, velocity);
            }
            // Abutting re-entry on a just-released line: transition from the
            // released note (the recorded slur carries the connection).
            Some((_, _, Some(rel))) => {
                self.start_legato_transition(rel, note, velocity);
            }
            // Fresh sustain attack on its own line.
            None => {
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
                self.line_mut().note = Some(note);
            }
        }
    }

    /// Pick a line for a fresh live attack: the least-recently-active SILENT
    /// line (fresh lines have activity 0, so from silence a chord fans out
    /// as line 0, 1, 2, …). If every line is sounding, retag the
    /// least-recently-active one (its old voices ring on until their own
    /// note-offs; the global fallback in `note_off` still finds them).
    pub(crate) fn alloc_free_line(&mut self) -> LineId {
        let mut best_free: Option<(u64, LineId)> = None;
        let mut best_any: Option<(u64, LineId)> = None;
        for (li, l) in self.lines.iter().enumerate() {
            let free = l.note.is_none() && matches!(l.state, LegatoState::Idle);
            if free && best_free.is_none_or(|(k, _)| l.last_activity < k) {
                best_free = Some((l.last_activity, li));
            }
            if best_any.is_none_or(|(k, _)| l.last_activity < k) {
                best_any = Some((l.last_activity, li));
            }
        }
        let li = best_free.or(best_any).map(|(_, li)| li).unwrap_or(0);
        // Stealing a sounding line: clear its bookkeeping so the new note
        // owns it (the old note's voices release via the global fallback).
        let l = &mut self.lines[li];
        l.note = None;
        l.order.clear();
        l.state = LegatoState::Idle;
        li
    }

    /// The line that currently owns `note` (sounding on it, or held in its
    /// press order — which includes pending transition targets).
    pub(crate) fn line_owning(&self, note: u8) -> Option<LineId> {
        self.lines
            .iter()
            .position(|l| l.note == Some(note))
            .or_else(|| self.lines.iter().position(|l| l.order.contains(&note)))
    }

    /// [`note_on_line`](Self::note_on_line) carrying the document
    /// scheduler's pre-roll lead (wall frames from this call to the note's
    /// grid tick). Every attack voice spawned by this dispatch is held back
    /// by `lead − its zone's measured heard-arrival` (`ZoneSpec::arrival_ms`)
    /// so the note is HEARD exactly on the tick — per round-robin (the
    /// per-RR replacement for the single global
    /// `short_note_timing.pre_delay_ms`), per mic, per dynamic layer.
    /// Zones without a measurement fall back to the historical claim
    /// (shorts: `pre_delay_ms`; sustains: heard-at-trigger).
    pub fn note_on_line_lead(&mut self, line: LineId, note: u8, velocity: u8, lead: u64) {
        self.spawn_align_lead = (lead > 0).then_some(lead);
        self.note_on_line(line, note, velocity);
        self.spawn_align_lead = None;
    }

    /// Line-addressed note-on: mono-line legato bookkeeping happens on
    /// `line`, and spawned voices are tagged with it. The channel-less
    /// [`note_on`](Self::note_on) uses line 0 (live single-line play).
    pub fn note_on_line(&mut self, line: LineId, note: u8, velocity: u8) {
        self.set_active_line(line);
        self.last_velocity = velocity;
        if velocity == 0 {
            self.note_off_line(line, note);
            return;
        }

        // Track a lightly-smoothed recent velocity (75% history, 25% new) as a
        // proxy for current playing dynamic — used to scale note-independent
        // ambience (pedal/mechanical noise). Also remember this note's strike
        // velocity so its release tail can match it.
        self.recent_velocity =
            (((self.recent_velocity as u16 * 3 + velocity as u16) / 4) as u8).max(1);
        self.note_strike_vel[note as usize] = velocity;

        // Velocity-sensitive keyswitches (CSS-style): a keyswitch note selects
        // an articulation / mode and does NOT sound.
        if self.try_keyswitch(note, velocity) {
            return;
        }

        self.resolve_pending_cc58(velocity);

        if self.patch.is_zoned() {
            // CSS-style legato: a legato articulation, with another note already
            // held, plays a delayed directional transition (the famous latency).
            // The first note of a phrase — or any non-legato articulation —
            // sounds immediately.
            let kind = self
                .patch
                .spec
                .articulation(&self.articulation)
                .map(|a| a.kind.clone());
            let is_legato = matches!(kind, Some(ArticulationKind::Legato));
            // Held + CC1-crossfaded: sustains, tremolo, harmonics, and trills.
            let is_sustain = matches!(
                kind,
                Some(
                    ArticulationKind::Sustain | ArticulationKind::Looped | ArticulationKind::Trill
                )
            );
            // One-shot, velocity-picked dynamic: spiccato/staccato/sfz/pizz/etc.
            let is_short = matches!(
                kind,
                Some(ArticulationKind::Short | ArticulationKind::OneShot)
            );
            self.held_notes.insert(note, velocity);

            // Only TRUE legato articulations take the monophonic transition
            // path: a Legato-kind artic, or a main sustain that has a CC2
            // vibrato pair (Nonvib↔Vibsus). Tremolo/Harmonics/Col Legno/Trills/
            // Marcato are `Sustain`/`Trill` kind but have NO vibrato pair and NO
            // legato transitions — in CSS every note is a fresh attack, so they
            // must take the polyphonic sustain path even while legato is toggled
            // on globally (otherwise a repeated/held note fires a *Nonvib*
            // transition under a muted CSS_W sustain → silence / wrong tone).
            let legato_capable = self.is_legato_capable_artic(&self.articulation.clone());

            if legato_capable && self.legato_enabled {
                // Monophonic legato line (CSS default). CSS plays its *sustain*
                // articulation (e.g. "Nonvib") legato — the Leg/NVLeg samples are
                // the sustain body's transitions — so any long articulation, not
                // just a Legato-kind one, takes this path when legato is enabled.
                // Track press order for last-note-priority fall-back on release.
                let l = self.line_mut();
                l.order.retain(|&n| n != note);
                l.order.push(note);
                match self.line().note {
                    // First note of the phrase: sounds immediately, no transition.
                    None => {
                        self.play_direction = "up".to_string();
                        self.trigger_zoned_sustain(note);
                        // Pacific atk+sus: layer the one-shot attack on a
                        // fresh phrase start (no-op without `attack_artic`).
                        self.spawn_attack_layer(note);
                        let now = self.frames_rendered;
                        let l = self.line_mut();
                        l.note = Some(note);
                        l.last_onset_frame = now;
                    }
                    // Transition from the currently-sounding note to this one,
                    // delayed by the velocity-mapped legato latency. A note that
                    // arrives mid-transition just re-targets it (fast runs
                    // collapse to the latest note — never dropped, never stacked).
                    Some(cur) => self.start_legato_transition(cur, note, velocity),
                }
            } else if is_legato || is_sustain {
                // Held: polyphonic sustain / trill (legato OFF for legato artics),
                // full CC1 dynamic + CC2 vibrato blend, loops to hold.
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
            } else if is_short {
                // One-shot short: velocity picks the dynamic layer, nearest-key
                // pitch-shift, plays to completion.
                self.trigger_zoned_short(note, velocity);
            } else {
                // Anything unusual: fall back to the generic zoned trigger.
                self.trigger_zoned(note, velocity, ZoneTrigger::Attack, true);
            }
            return;
        }

        // Legzero: same note re-trigger while sustain pedal is held.
        let legzero = self.cc64_held && self.held_notes.contains_key(&note);

        // Whether any other note is currently held (legato condition).
        let other_held = self.held_notes.keys().any(|&n| n != note);

        self.held_notes.insert(note, velocity);
        self.deferred_note_off_velocities.remove(&note);

        let artic_kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());

        match artic_kind {
            Some(ArticulationKind::Sustain | ArticulationKind::Looped) => {
                if legzero {
                    self.trigger_legzero(note, velocity);
                } else if other_held && self.legato_enabled {
                    self.initiate_legato(note, velocity);
                } else {
                    self.trigger_sustain(note);
                }
            }
            Some(ArticulationKind::Short | ArticulationKind::OneShot) => {
                self.trigger_short(note, velocity);
            }
            Some(ArticulationKind::Legato | ArticulationKind::Release) => {
                // These are not triggered directly by note-on.
            }
            Some(ArticulationKind::Trill | ArticulationKind::Special) => {
                // Treat special/trill as sustain for basic playback.
                self.trigger_sustain(note);
            }
            None => {
                tracing::warn!(
                    artic = %self.articulation,
                    "note_on: unknown articulation — skipping"
                );
            }
        }
    }

    /// Process a MIDI note-off event. Channel-less (live) note-offs route to
    /// the line that owns the note — the live allocator may have placed it
    /// anywhere; single-line play always resolves to line 0.
    pub fn note_off(&mut self, note: u8) {
        let line = self.line_owning(note).unwrap_or(0);
        self.note_off_line(line, note);
    }

    /// Line-addressed note-off (see [`note_on_line`](Self::note_on_line)).
    pub fn note_off_line(&mut self, line: LineId, note: u8) {
        self.set_active_line(line);
        let release_velocity = self.cc1;
        self.note_off_with_velocity_on_line(note, release_velocity);
    }

    pub fn note_off_with_velocity(&mut self, note: u8, release_velocity: u8) {
        let line = self.line_owning(note).unwrap_or(0);
        self.set_active_line(line);
        self.note_off_with_velocity_on_line(note, release_velocity);
    }

    pub(crate) fn note_off_with_velocity_on_line(&mut self, note: u8, release_velocity: u8) {
        let release_frames = self.pedal_release_frames();
        if self.cc64_held {
            // Sustain pedal held — defer release.
            self.deferred_note_off_velocities
                .insert(note, release_velocity);
            tracing::debug!(note, release_velocity, "pedal defer");
            return;
        }
        self.held_notes.remove(&note);
        self.deferred_note_off_velocities.remove(&note);
        self.trace_push(TraceKind::NoteOff { note });
        if self.patch.is_zoned() {
            let cur_line = self.cur_line as u8;
            self.line_mut().order.retain(|&n| n != note);
            // Monophonic legato: releasing the SOUNDING note falls back to the
            // most-recent still-held note via a legato transition; releasing the
            // line's last note ends it. Lifting a held-but-silent key is a no-op.
            if self.legato_enabled && self.line().note.is_some() {
                if self.line().note != Some(note) {
                    return;
                }
                // Live mono legato: releasing the SOUNDING note falls back to
                // the most-recent still-held note via a transition. In LOOKAHEAD
                // (document) mode the annotator schedules every transition, so a
                // note-off must NOT synthesize one — that races the scheduled
                // prefire and DOUBLES the arrived voice (seen when a fast
                // passage's legato lead-in outruns the note length). Document
                // note-offs only end phrases.
                if self.play_mode != PlayMode::Lookahead {
                    if let Some(&prev) = self.line().order.last() {
                        // Fall back at a medium transition speed.
                        let fallback_vel = self.patch.spec.legato_cfg().fallback_velocity;
                        self.start_legato_transition(note, prev, fallback_vel);
                        return;
                    }
                }
                // Line ends — remember what/when for the live allocator's
                // "just released" abutment gate.
                let now = self.frames_rendered;
                let l = self.line_mut();
                l.note = None;
                l.released_note = Some(note);
                l.last_release_frame = now;
            }
            // Recorded release tail (CSS Vsusrel/NVrel) — default OFF
            // (`$4p5kj=0`, spec §6). Only fires when "Releases" is enabled.
            if self.releases_enabled {
                self.spawn_release(note);
            }
            // Held-sustain note-off (spec §6): immediate note_off + overlapping
            // fade (~`$tukcw=200` ms). This line's voices only, so a unison
            // note held by another divisi line keeps sounding.
            let sus_off = ms_to_frames(
                self.patch.spec.performance.sustain_noteoff_ms,
                self.sample_rate,
            )
            .max(release_frames);
            self.voices.note_off_line(cur_line, note, Some(sus_off));
            return;
        }
        self.do_note_off_with_release_frames(note, release_velocity, release_frames);
    }

    /// Release every held/sounding note and reset pedal-deferred state.
    pub fn all_notes_off(&mut self) {
        self.held_notes.clear();
        self.deferred_note_off_velocities.clear();
        self.cc64_held = false;
        self.cc64_value = 0;
        if let Some(orig) = self.no_pedal_articulation.take() {
            self.articulation = orig;
        }
        self.reset_lines();
        self.voices.all_notes_off();
    }

    /// End every mono line: sounding note, press order, pending countdowns.
    /// (CC1/CC2 values persist — controllers outlive notes.)
    pub(crate) fn reset_lines(&mut self) {
        for l in &mut self.lines {
            l.state = LegatoState::Idle;
            l.note = None;
            l.order.clear();
            l.released_note = None;
            l.last_release_frame = 0;
            l.last_activity = 0;
        }
        self.live_last_onset = None;
    }

    pub fn panic(&mut self) {
        self.held_notes.clear();
        self.deferred_note_off_velocities.clear();
        self.cc64_held = false;
        self.cc64_value = 0;
        if let Some(orig) = self.no_pedal_articulation.take() {
            self.articulation = orig;
        }
        self.reset_lines();
        self.voices.panic();
    }

    /// Zone-mode trigger. Looks up every zone matching `(note, velocity)`
    /// and spawns one `Zoned` voice per **mic group** within the match
    /// set. RR-cycling happens within each mic group so a kick with 7
    /// mics × 8 RR slots fires 7 simultaneous voices that share the
    /// same RR index.
    pub(crate) fn trigger_zoned(
        &mut self,
        note: u8,
        velocity: u8,
        trigger: ZoneTrigger,
        record_empty_miss: bool,
    ) {
        // Bucket matching zones by mic id so each mic gets its own
        // round-robin within the candidate set.
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if self.zone_selected(z, note, velocity, trigger) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, Some(note), velocity, trigger, record_empty_miss);
    }

    pub(crate) fn trigger_cc_zones(&mut self, controller: u8, old_value: u8, value: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_cc_trigger_crossed(z, controller, old_value, value) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, None, value, ZoneTrigger::Cc, false);
    }

    pub(crate) fn trigger_aftertouch_zones(&mut self, note: Option<u8>, old_value: u8, value: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_aftertouch_trigger_crossed(z, note, old_value, value) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, note, value, ZoneTrigger::Aftertouch, false);
    }

    pub(crate) fn trigger_event_zones(&mut self, trigger: ZoneTrigger, velocity: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_trigger_matches(z, trigger) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, None, velocity, trigger, false);
    }

    /// Trigger a zoned sustain/legato note with the full CSS expressive blend:
    /// CC1 crossfades across ALL dynamic layers, CC2 crossfades the non-vibrato
    /// and vibrato sample sets. Spawns one `SustainLayer` voice per dynamic
    /// layer per vib side; `render()` re-levels them live as CC1/CC2 move (see
    /// [`update_sustain_gains`](Self::update_sustain_gains)), so a held note
    /// swells the full dynamic range. `self.articulation` is the non-vibrato
    /// base; the vibrato pair is found and blended in by CC2.
    /// Library master tune (`PerformanceSpec::master_tune_cents`), applied
    /// globally on top of the per-note transpose. 0 for libraries that don't
    /// set it; CSS ships `tune=1.00521` ≈ +9.0 cents in its spec.
    pub(crate) fn master_tune_cents(&self) -> f64 {
        f64::from(self.patch.spec.performance.master_tune_cents)
    }

    pub(crate) fn trigger_zoned_sustain(&mut self, note: u8) {
        let direction = self.play_direction.clone();
        // Honour the forced-RR pin like every other RR-bearing trigger —
        // document mode pins a stable per-note slot here so playback never
        // consults the mutable counter (position-independent determinism).
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);

        let nv_artic = self.articulation.clone();

        // CC2 picks the non-vib vs vib balance (equal-power) — in pure playback
        // too: the CC2 vibrato crossfade IS CSS (pure = exact CSS; the old
        // nonvib-only shortcut predated that definition and silenced the vib
        // sampleset — param-test S07 measured the reference 1.84× louder
        // because our vib side was missing).
        let (nv_scale, vb_scale) = Self::equal_power(self.cc2_blend());
        // ORIENT the pair: the default articulation may be the VIBRATO member
        // (CSS default = Vibsus), so resolve which side is which — CC2=0 must
        // play the NON-vibrato sampleset regardless of which id is default.
        let base_is_vib = self
            .patch
            .spec
            .articulation(&nv_artic)
            .map(|a| a.is_vibrato())
            .unwrap_or(false);
        let (nv_id, vib_id) = if base_is_vib {
            (
                self.find_vibrato_pair_id(&nv_artic)
                    .unwrap_or_else(|| nv_artic.clone()),
                Some(nv_artic.clone()),
            )
        } else {
            (nv_artic.clone(), self.find_vibrato_pair_id(&nv_artic))
        };
        self.spawn_sustain_layers(&nv_id, false, nv_scale, &direction, note, rr);
        if let Some(vib_id) = vib_id {
            self.spawn_sustain_layers(&vib_id, true, vb_scale, &direction, note, rr);
        }

        // CSS FIRST-NOTE attack ornament (KSP §2 first-note branch): a fresh
        // note plays a TRANSITION-GROUP voice at offset 0 (`%jcxqm =
        // play_note(note,…,0,0)`, `$1fvjk = 0` — the full recorded bow attack)
        // STACKED on the sustain layers. Without it every isolated/first note
        // sits ~3.5 dB under Kontakt (param-test S13/S08/S09 level ratios
        // 1.4-1.55). The retrigger (Legzero) sampleset is the bow-attack
        // recording at the played pitch; recorded level × CC1 expr, no makeup
        // (same rule as transitions). Connected notes skip this — their onset
        // is the legato transition.
        if !self.legato_sustain {
            let (nv_ret, vib_ret) = self.legato_pair_ids(true);
            let expr = self.cc1_expression(self.cc1);
            for (id, scale) in [(nv_ret, nv_scale), (vib_ret, vb_scale)] {
                let Some(id) = id else { continue };
                if scale < 0.01 {
                    continue;
                }
                let (lo, hi, blend) = self.layers_for_artic(&id);
                let dynamic = if blend >= 0.5 { hi } else { lo };
                if let Some(idx) = self.find_layer_zone(&id, &direction, &dynamic, note, rr) {
                    self.spawn_zone_voice(idx, note, VoiceKind::Legato, scale * expr, None, 0.0);
                }
            }
        }
    }

    /// Trigger a one-shot short note (spiccato / staccato / sfz / pizz / …). The
    /// nearest recorded key is pitch-shifted and it plays to completion (no loop).
    ///
    /// KSP-confirmed CSS model (`script_1.ksp` ~10775–11135, verified against the
    /// reference render): the short TYPE is selected upstream by CC1
    /// (`short_note_cc1_map`) / CC58 keyswitch; **VELOCITY** then does BOTH —
    /// it selects the recorded DYNAMIC layer via that type's `%g1qri` thresholds
    /// AND scales volume continuously within the band via `$arhiq`
    /// ([`short_layer_and_velvol`]). CC1 is NOT in the short-dynamic path. When
    /// the flag is off or the artic carries no thresholds this falls back to the
    /// even-split dynamic with the velocity² loudness curve (non-CSS libraries).
    pub(crate) fn trigger_zoned_short(&mut self, note: u8, velocity: u8) {
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);
        let artic = self.articulation.clone();

        // VELOCITY → dynamic layer (`%g1qri` band) + intra-band volume (`$arhiq`).
        let (dynamic, velvol_db) = self.short_layer_and_velvol(&artic, velocity);
        let n_dyn = self
            .patch
            .spec
            .articulation(&artic)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        // With ≥2 recorded dynamics the loudness IS the sampled layer plus the
        // `$arhiq` intra-band trim; only single-layer artics use velocity².
        let gain = if n_dyn > 1 {
            1.0
        } else {
            velocity_gain(velocity)
        } * db_to_gain(css_short_makeup_db(&artic) + velvol_db);
        if let Some(idx) = self.find_layer_zone(&artic, "", &dynamic, note, rr) {
            self.spawn_zone_voice(idx, note, VoiceKind::Short, gain, None, 0.0);
        }
    }

    /// VELOCITY → dynamic band + `$arhiq` volume for a SHORT note, per the
    /// KSP `%g1qri` model. Returns `(band_index, n_bands, band_top_velocity,
    /// band_span)` or `None` when the articulation carries no usable thresholds.
    ///
    /// The interior boundaries are the `vel_thresholds` (the `%g1qri` values after
    /// the implicit floor of 1) with a **trailing terminal `127` removed** — a
    /// `127` threshold is the open-top edge of the highest band, not a new band.
    /// So `n_bands = interior_boundaries + 1`:
    /// - Spiccato `(25 55 108)` → 4 bands `[1,24][25,54][55,107][108,127]`.
    /// - Staccato `(51 83 127)` → 3 bands `[1,50][51,82][83,127]` (the `127` is
    ///   the top edge, not a degenerate `[127,127]` band).
    ///
    /// The last band's top is 127 with span `128 − lo` (KSP open top edge).
    ///
    /// Band → recorded-dynamic mapping is done by the caller: 1:1 when the counts
    /// match, else TOP-aligned (the extra softest recorded dynamics sit below the
    /// short's `%g1qri` floor — verified against the reference render, where the
    /// CC1=90-collapsed Staccato's vel 40/80/120 land on mp/f/fff, not pp/mp/f).
    pub(crate) fn short_band(
        &self,
        artic: &crate::spec::ArticulationSpec,
        velocity: u8,
    ) -> Option<(usize, usize, i32, i32)> {
        if artic.dynamics.is_empty() {
            return None;
        }
        // Interior boundaries = thresholds, minus a trailing terminal 127.
        let mut bounds: Vec<i32> = artic.vel_thresholds.iter().map(|&t| t as i32).collect();
        if bounds.last() == Some(&127) {
            bounds.pop();
        }
        if bounds.is_empty() {
            return None;
        }
        let n_bands = bounds.len() + 1;
        let v = velocity as i32;
        let band = bounds.iter().position(|&t| v < t).unwrap_or(n_bands - 1);
        let lo = if band == 0 { 1 } else { bounds[band - 1] };
        let (num_top, span) = if band == n_bands - 1 {
            (127, 128 - lo)
        } else {
            let t = bounds[band];
            (t, t - lo)
        };
        Some((band, n_bands, num_top, span))
    }

    /// Velocity → VOLUME (dB) for a SHORT note: the decoded CSS `$arhiq` intra-band
    /// law applied ON TOP of the `%g1qri` layer selection. Ramps 0 dB at the
    /// band's top velocity down to ~`%bcez1` dB at its bottom, making velocity a
    /// continuous loudness ramp within each recorded layer. Returns 0 when the
    /// flag is off or the artic carries no thresholds. KSP law:
    /// `dB = (band_top − vel)·%bcez1 / (span − 1)`.
    pub(crate) fn short_velocity_volume_db(&self, artic_id: &str, velocity: u8) -> f32 {
        // `$arhiq` is gated separately from the layer selection: the decoded
        // `%bcez1` (`vel_layer_db`) currently over-attenuates vs the reference,
        // so it is staged but not applied for CSS. See `apply_short_velvol`.
        if !self.patch.spec.dynamics.apply_short_velvol {
            return 0.0;
        }
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return 0.0;
        };
        let Some((band, _n_bands, num_top, span)) = self.short_band(artic, velocity) else {
            return 0.0;
        };
        let delta = artic.vel_layer_db.get(band).copied().unwrap_or(0.0);
        if span > 1 {
            (num_top - velocity as i32) as f32 * delta / (span as f32 - 1.0)
        } else {
            0.0
        }
    }

    /// Spawn EVERY dynamic layer of `artic` (one `SustainLayer` voice each),
    /// gained by the current CC1 crossfade. Holding all layers — even the silent
    /// ones — is what lets a held note swell the full dynamic range as CC1 moves
    /// (`update_sustain_gains` re-levels them live), the way CSS does.
    pub(crate) fn spawn_sustain_layers(
        &mut self,
        artic: &str,
        vib: bool,
        side_scale: f32,
        direction: &str,
        note: u8,
        rr: usize,
    ) {
        // Layer LABELS come from the articulation's OWN recorded dynamics — they
        // must match the zone `dynamic` tags. The generic `cc1_layers_N` labels
        // only line up with the main Nonvib/Vibsus sustains (`[ppp,p,mf,ff]`),
        // so using them silences every other multi-dynamic sustain
        // (Tremolo `[pp,mp,f,fff]`, Clegno `[pp,mp,f]`, Harm `[pp,mp]`, …). The
        // `cc1_layers_N` table only supplies the CC1 crossfade RANGES, indexed
        // by layer count; a count with no table falls back to an even split.
        let dyn_labels: Vec<String> = self
            .patch
            .spec
            .articulation(artic)
            .map(|a| a.dynamics.clone())
            .unwrap_or_default();

        // CC1 two-sampleset dynamics (CSS `%grhcg`/`%u1bjb` crossfade) apply in
        // pure playback too — pure = exact CSS, and this IS CSS (the thing pure
        // drops is OUR added dynamic-lane naturalism, not the recorded-dynamics
        // crossfade). Both paths spawn every declared dynamic layer and blend
        // the active pair equal-power by CC1; `update_sustain_gains` re-levels
        // them live as CC1 sweeps.
        let expr = self.cc1_expression(self.cc1);
        if dyn_labels.len() <= 1 {
            // Single (or no) declared dynamic — one zone, loudness from CC1.
            let label = dyn_labels.first().map(String::as_str).unwrap_or("");
            let gain = side_scale * expr;
            if let Some(idx) = self.find_layer_zone(artic, direction, label, note, rr) {
                self.spawn_zone_voice(
                    idx,
                    note,
                    VoiceKind::SustainLayer,
                    gain,
                    Some(DynLayer { vib, index: 0 }),
                    0.0,
                );
            }
            return;
        }
        let ranges = self.cc1_layers_for(artic);
        let (lo_idx, hi_idx, blend) = if ranges.len() == dyn_labels.len() {
            Self::cc1_blend_idx(ranges, self.cc1)
        } else {
            // No CC1 range table for this layer count — even split across N.
            let n = dyn_labels.len();
            let pos = (self.cc1 as f32 / 127.0) * (n as f32 - 1.0);
            let lo = pos.floor() as usize;
            (lo, (lo + 1).min(n - 1), pos - lo as f32)
        };
        let (lo_g, hi_g) = Self::equal_power(blend);
        for (i, label) in dyn_labels.iter().enumerate() {
            let gain = side_scale * expr * layer_gain(i, lo_idx, hi_idx, lo_g, hi_g);
            if let Some(idx) = self.find_layer_zone(artic, direction, label, note, rr) {
                self.spawn_zone_voice(
                    idx,
                    note,
                    VoiceKind::SustainLayer,
                    gain,
                    Some(DynLayer {
                        vib,
                        index: i as u8,
                    }),
                    0.0,
                );
            }
        }
    }

    /// **LIVE / reactive entry** (the live keyboard path — see the two-paths
    /// doc block on [`PlayMode`](crate::engine::PlayMode)). Start a monophonic
    /// legato transition `from` → `to` at `velocity`. The note is delayed by
    /// the velocity-mapped legato latency (CSS's three transition speeds);
    /// when it elapses, `render()` calls `fire_legato`, which fades the old
    /// note and plays the directional transition zone. A zero delay
    /// (portamento) fires immediately. The DOCUMENT path never calls this —
    /// it prefires via `legato_prefire_line_lead` → `fire_legato_with_lead`.
    pub(crate) fn start_legato_transition(&mut self, from: u8, to: u8, velocity: u8) {
        // Every entry here is the REACTIVE path (live note-on countdown or
        // note-off fallback) — document playback must never reach it (it
        // schedules `legato_prefire_line` instead), which tests assert via
        // this counter.
        self.reactive_legato_fires = self.reactive_legato_fires.saturating_add(1);
        // Inter-onset interval (IOI) = time since the previous note-on on this
        // line — the KSP's `$ftvnh`, which drives the Overlap-Delay (spec §2.1).
        let now = self.frames_rendered;
        let ioi_frames = now.saturating_sub(self.line().last_onset_frame);
        self.line_mut().last_onset_frame = now;
        let ioi_ms = frames_to_ms(ioi_frames, self.sample_rate);
        let (delay_ms, portamento) = self.legato_timing(velocity, ioi_frames);
        let frames = ms_to_frames(delay_ms, self.sample_rate);
        if frames == 0 {
            self.play_direction = if to >= from { "up" } else { "down" }.to_string();
            self.fire_legato(from, to, velocity, portamento, ioi_ms);
        } else {
            self.line_mut().state = LegatoState::Pending {
                frames_remaining: frames,
                from_note: from,
                to_note: to,
                to_note_velocity: velocity,
                portamento,
                ioi_ms,
            };
        }
    }

    /// Stem class of an articulation id: Short/OneShot kinds are Shorts,
    /// everything long (Sustain/Legato/Looped/Trill/Release/Special) is
    /// Longs. Percussion engines class every unmatched tag as Shorts (a drum
    /// hit is a short by nature).
    pub(crate) fn artic_class_for(&self, artic_id: &str) -> ArticClass {
        match self.patch.spec.articulation(artic_id).map(|a| &a.kind) {
            Some(ArticulationKind::Short | ArticulationKind::OneShot) => ArticClass::Shorts,
            Some(_) => ArticClass::Longs,
            None => {
                if self.percussion {
                    ArticClass::Shorts
                } else {
                    ArticClass::Longs
                }
            }
        }
    }

    /// The legato transition delay (ms) + portamento flag for a target
    /// velocity: portamento below the threshold, else the expressive or
    /// low-latency velocity→delay curve from the spec — chosen by the
    /// [`PlayMode`] policy: Lookahead → expressive (full authenticity),
    /// StrictLive → low_latency, NO exceptions (a CC58 "expressive" request
    /// only takes effect once the mode is Lookahead).
    pub(crate) fn legato_timing(&self, velocity: u8, ioi_frames: u64) -> (u32, bool) {
        // Pacific: transitions fire IMMEDIATELY — no velocity-zone delays, no
        // Overlap-Delay curves, no portamento model.
        if self.patch.spec.legato_cfg().style == crate::spec::LegatoStyle::Pacific {
            return (0, false);
        }
        let port_thresh = self
            .patch
            .spec
            .legato_engine
            .as_ref()
            .and_then(|le| le.portamento.as_ref())
            .map(|p| p.trigger_vel_max)
            .unwrap_or(0);
        // CSS portamento (spec §2.4): CC5 > 10 AND attack velocity ≤ threshold
        // (real `$fkyb2 = 10`; falls back to the spec-configured value).
        let cc5 = self.cc_values[5];
        let portamento = port_thresh > 0 && velocity <= port_thresh && cc5 > 10;
        // Overlap-Delay (spec §2.1) — driven by IOI, attack velocity range, and
        // legato mode, using the real persistent anchors (near-zero except
        // soft+fast).
        let delay_ms = if portamento {
            0
        } else {
            self.patch.spec.legato_cfg().overlap_delay_ms(
                frames_to_ms(ioi_frames, self.sample_rate),
                velocity,
                self.legato_expressive,
            )
        };
        (delay_ms, portamento)
    }

    /// Spawn the legato TRANSITION sample(s) for the move `from → to` (the
    /// one-shot `Leg`/`NVLeg`/`Port` recordings carrying the bow change; the
    /// long-form sample also holds the arrived note by looping its tail).
    ///
    /// CSS transition naming (verified empirically — see the generator in
    /// sample-collector's `sc-import css-legato-fix`): the sample's
    /// `root_key` is the LOWER pitch of the pair, `direction` says which end
    /// is the source (`up` = root→root+interval, `down` = the reverse), and
    /// `interval` is the semitone distance (1..=12, whole-tone root grid).
    /// So the zone for `from → to` is: direction `sign(to-from)`, named note
    /// `min(from,to)`, interval `|to-from|` clamped to an octave. The chosen
    /// zone is pitch-shifted so the DESTINATION lands exactly on `to` —
    /// guaranteed by construction even when the interval or root had to be
    /// approximated (only the lead-in's source end is then off, and the real
    /// source voice is still sounding over it).
    ///
    /// Same-pitch re-bows (`from == to`) use the `Legzero` re-trigger
    /// samples (destination note only, no lead-in, 3 RRs).
    ///
    /// Vibrato: like the sustains, the Leg/NVLeg transition sets are a CC2
    /// crossfade pair — both sides spawn at equal-power gains.
    ///
    /// `sched_lead` is the document scheduler's prefire lead in frames (the
    /// transition fires that early so the pitch change lands on the
    /// destination tick). When the chosen zone's measured lead-in is LONGER
    /// than the scheduled lead, the surplus is skipped off the front of the
    /// sample so the arrival still lands on the tick.
    pub(crate) fn spawn_legato_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        portamento: bool,
        sched_lead: Option<u64>,
        ioi_ms: f32,
    ) {
        if portamento {
            // Portamento glides: single `Port` articulation (one `f` dynamic,
            // no vibrato pair), volume from CC5 "Portamento Volume".
            let id = self
                .find_port_artic_id()
                .or_else(|| self.find_legato_artic_id(false));
            if let Some(id) = id {
                self.spawn_transition_voice(
                    &id,
                    from,
                    to,
                    velocity,
                    self.cc5_porta_volume,
                    sched_lead,
                    ioi_ms,
                );
            }
            return;
        }
        let (nv_id, vib_id) = self.legato_pair_ids(from == to);
        // CC2 vibrato blend applies to the transition pair in pure playback too
        // (pure = exact CSS; the nonvib-only shortcut is gone).
        let (nv_scale, vb_scale) = Self::equal_power(self.cc2_blend());
        // The transition must track the CURRENT dynamic, exactly as the sustain
        // layers do (`spawn_sustain_layers`). Without this the transition plays
        // at the recorded sample's full level — so a soft (low-CC1) passage's
        // legato hand-offs jump up to the loud recorded dynamic ("ff" bump) and
        // clash with the quiet sustain around them. `cc1_expression` is the same
        // continuous CC1→loudness curve the held note is gained by.
        //
        // The transition (`%ftriy`) is the attack ornament, NOT the held body,
        // so it takes NO makeup here (neither OUTPUT_MAKEUP nor the −6 dB
        // `$3tsb0`). It plays at recorded level × CC1 — the same net level as the
        // legato SUSTAIN it overlays (which nets 0 dB: +6 OUTPUT_MAKEUP − 6
        // $3tsb0). `$3tsb0` lands on the sustain voice via `legato_sustain`.
        // `cc1_expression` applies in pure playback too — it is the CSS
        // bottom-rolloff (calibrated on the reference; the pure expr=1.0 gate
        // was starving the S05 recalibration on legato lines).
        let expr = self.cc1_expression(self.cc1);
        let mut spawned = false;
        for (id, scale) in [(nv_id, nv_scale), (vib_id, vb_scale)] {
            let Some(id) = id else { continue };
            // A fully-crossfaded-out side is skipped — unless it is the only
            // side present (mono-set libraries keep sounding regardless of CC2).
            if scale <= 0.001 && spawned {
                continue;
            }
            self.spawn_transition_voice(
                &id,
                from,
                to,
                velocity,
                scale.max(0.001) * expr,
                sched_lead,
                ioi_ms,
            );
            spawned = true;
        }
    }

    /// The non-vibrato / vibrato legato articulation pair for the CC2
    /// crossfade — (`NVLeg`, `Leg`), or the `*zero` re-trigger pair when
    /// `retrigger`. Either side may be absent.
    pub(crate) fn legato_pair_ids(&self, retrigger: bool) -> (Option<String>, Option<String>) {
        let want_sord = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.is_sordino())
            .unwrap_or_else(|| self.articulation.starts_with("Sord"));
        let want_role = if retrigger {
            crate::spec::LegatoRole::Retrigger
        } else {
            crate::spec::LegatoRole::Transition
        };
        let mut nv = None;
        let mut vib = None;
        for a in &self.patch.spec.articulations {
            if a.kind != ArticulationKind::Legato {
                continue;
            }
            if a.is_sordino() != want_sord {
                continue;
            }
            if a.resolve_legato_role() != want_role {
                continue;
            }
            let slot = if a.is_vibrato() { &mut vib } else { &mut nv };
            if slot.is_none() {
                *slot = Some(a.id.clone());
            }
        }
        (nv, vib)
    }

    /// Spawn one transition voice from articulation `leg_id` for `from → to`
    /// (see [`spawn_legato_transition`](Self::spawn_legato_transition) for
    /// the selection rules).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_transition_voice(
        &mut self,
        leg_id: &str,
        from: u8,
        to: u8,
        velocity: u8,
        gain: f32,
        sched_lead: Option<u64>,
        ioi_ms: f32,
    ) {
        // Dynamic layer for the transition from CC1 (single dominant layer).
        let (lo, hi, blend) = self.layers_for_artic(leg_id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        if from == to {
            // Re-bow: `Legzero` destination-note re-trigger (3 RRs). The
            // re-attack is NOT instantaneous — the zone's measured onset
            // (`arrival_ms`) says when the bow change speaks. Under a
            // document prefire (`sched_lead`) the spawn is arrival-aligned
            // (held back by `lead − onset`) so the re-bow speaks ON the
            // tick; live re-bows keep firing immediately.
            let rr = self
                .forced_rr
                .map(|f| f as usize)
                .unwrap_or(self.zone_rr_counter);
            if let Some(idx) = self.find_layer_zone(leg_id, "", &dynamic, to, rr) {
                let restore = self.spawn_align_lead;
                self.spawn_align_lead = sched_lead.filter(|&l| l > 0);
                self.spawn_zone_voice(idx, to, VoiceKind::Legato, gain, None, 0.0);
                self.spawn_align_lead = restore;
            }
            return;
        }
        let direction = if to > from { "up" } else { "down" };
        let named = from.min(to);
        // CSS samples nothing beyond an octave: clamp and pitch-shift so the
        // destination stays exact (the lead-in's source end lands an octave
        // off, under the still-sounding real source voice).
        let interval = u32::from(from.abs_diff(to)).min(12);
        let Some(idx) = self.find_transition_zone(leg_id, direction, &dynamic, named, interval)
        else {
            return;
        };
        let z = &self.patch.spec.zones[idx];
        // The sample's destination pitch is root (down) or root+interval
        // (up); the voice plays at `to - root + pitch_offset` semitones, so
        // this offset makes the destination land exactly on `to` no matter
        // which zone was chosen.
        let pitch_offset = if z.direction.eq_ignore_ascii_case("up") {
            -f64::from(z.interval)
        } else {
            0.0
        };
        // The heard-arrival marker is in SAMPLE time (measured `arrival_ms`
        // when present, metadata `lead_in_ms` otherwise). Playback now runs at
        // the TRUE speed (tuning × SR-conversion only — the whole-tone
        // transposition is a time-preserving pitch shift that does NOT change
        // duration), so lead↔sample conversions and the arrival prediction use
        // that rate, not the transposition. This is what puts pitch-shifted
        // (off-grid) transitions back on the musical grid.
        let sr_scale = self
            .cache
            .get_loaded(&self.patch.zone_paths[idx])
            .map(|d| d.sample_rate as f64 / self.sample_rate as f64)
            .unwrap_or(1.0);
        let rate = 2.0f64.powf((z.tune_cents as f64 + self.master_tune_cents()) / 1200.0) * sr_scale;
        // Diagnostic sweep semantics (see `document::arrival_semantics_env`):
        // the effective arrival is re-interpreted between the LT offset and
        // the measured settle. Unset env = the marker as measured. The
        // schedule applied the same re-interpretation to the prefire lead,
        // so `offset = arrival_eff − lead·rate` stays consistent.
        let arrival_eff_ms = {
            let (frac, bias) = crate::document::arrival_semantics_env();
            if (frac - 1.0).abs() < 1e-6 && bias.abs() < 1e-6 {
                z.transition_arrival_ms()
            } else {
                let off = self
                    .patch
                    .spec
                    .legato_cfg()
                    .lt_offset_ms(ioi_ms.max(400.0), velocity, self.legato_expressive);
                (off + (z.transition_arrival_ms() - off).max(0.0) * frac + bias).max(0.0)
            }
        };
        let lead_sample_frames =
            ms_to_frames(arrival_eff_ms.max(0.0).round() as u32, self.sample_rate) as u64;
        // Sample-start offset = the CSS `$1fvjk` curve, IOI-driven (177 → 117 ms):
        //
        //  * Document (lookahead) path — `sched_lead` is set: the scheduler
        //    prefired the transition by the audible pre-bow (measured arrival −
        //    $1fvjk; see `document::annotate` / `LibrarySpec::legato_lead_ms`),
        //    so `arrival − sched_lead·rate` resolves to exactly `$1fvjk` while
        //    keeping the arrival ON the destination tick (the audible pre-bow in
        //    wall frames equals `sched_lead`, independent of `rate`).
        //  * Reactive (live) path — `sched_lead` is None: no lookahead, so start
        //    directly at `$1fvjk` (from the armed IOI), clamped to the measured
        //    arrival so we never begin past the destination pitch. Legacy
        //    libraries have no measured lead-in (`lead_sample_frames == 0`) →
        //    offset 0, the pre-`$1fvjk` behaviour.
        let start_offset = match sched_lead {
            // NO-SHIFT mode fires at the tick (lead 0); play from the reactive
            // `$1fvjk` offset (the `None` arm) so the arrival lands late, not
            // arrival-aligned. Any lead>0 uses the document skip.
            Some(lead) if lead > 0 && !crate::document::no_prefire() => {
                let lead_in_sample = (lead as f64 * rate) as u64;
                let raw = lead_sample_frames.saturating_sub(lead_in_sample);
                // Enforce the CSS `$1fvjk` MINIMUM skip (≈60 ms) so we never play
                // the transition sample's sharp bow-attack HEAD. When `raw` would
                // start before that (slow/medium legato, where the measured
                // arrival < lead), skipping deeper shortens the audible pre-bow —
                // and the `start_hold` formula below auto-grows the SILENT gap to
                // keep the arrival on the tick. That silent-gap + head-skip IS
                // the CSS Overlap-Delay + `$1fvjk` structure, and it masks the
                // audible NVLeg onset. Never skip past the arrival marker itself.
                let kbqnb_hard =
                    self.patch.spec.legato_cfg().velocity_range(velocity) == 3 && ioi_ms > 50.0;
                let min_skip_ms = crate::engine::css_lt_min_skip_ms(
                    kbqnb_hard,
                    self.patch.spec.zones[idx].interval,
                );
                let min_skip = ms_to_frames(min_skip_ms, self.sample_rate) as u64;
                raw.max(min_skip.min(lead_sample_frames.saturating_sub(1))) as usize
            }
            _ => {
                // Reactive / NO-SHIFT path: the real CSS `$1fvjk` — velocity-range
                // base + soft/fast IOI boost (falls back to the legacy IOI
                // curve when the spec authors no bases).
                let off_ms = self.patch.spec.legato_cfg().lt_offset_ms(
                    ioi_ms,
                    velocity,
                    self.legato_expressive,
                );
                let off = ms_to_frames(off_ms.round() as u32, self.sample_rate) as u64;
                off.min(lead_sample_frames) as usize
            }
        };
        // CSS `%jcxqm` two-stage transition fade-in (document path): the
        // transition EMERGES via the swell over the window from its audible
        // start to the arrival, reaching full ON the tick — instead of a 25 ms
        // declick that reads as an artificial onset (the NVLeg sample has no
        // silent head). Same shape as the destination swell (igmiu / IOI-scaled
        // $x444h), capped at the audible window so it lands full at the arrival.
        self.transition_fade = sched_lead.map(|_| {
            let arrival_wall = ((lead_sample_frames.saturating_sub(start_offset as u64)) as f64
                / rate.max(1e-9))
            .round() as usize;
            let total = ms_to_frames(crate::engine::CSS_XTIME_MS, self.sample_rate)
                .min(arrival_wall)
                .max(ms_to_frames(crate::engine::SUSTAIN_DECLICK_MS, self.sample_rate));
            let igmiu = crate::engine::CSS_ATK_FADE_PCT as usize;
            let x444h = crate::engine::css_node_vol_div(ioi_ms) as usize;
            let s1 = total * igmiu / 100;
            let s1d = (total * igmiu / x444h).max(1);
            let s2 = total * (100 - igmiu) / 100;
            (s1, s1d, s2)
        });
        // Deterministic heard-arrival prediction (see
        // [`LegatoFireEvent::arrival`]): the in-sample arrival marker
        // (`lead_in_ms`, measured per zone) minus the offset we skip off the
        // front, in wall frames at this voice's playback rate. When both CC2
        // sides spawn, the last (dominant-selection order) wins — the pair is
        // recorded from the same performance, so the markers agree.
        self.last_arrival_prediction = self.frames_rendered
            + ((lead_sample_frames.saturating_sub(start_offset as u64)) as f64 / rate).round()
                as u64;
        // Arrival alignment (document path): when the zone's remaining
        // in-sample arrival is SHORTER than the scheduled lead (a corrected
        // per-zone marker under a median-derived schedule lead), the voice is
        // held back by the difference so the pitch change still lands ON the
        // tick — sample-exact, per zone (`spawn_zone_voice_at` computes the
        // hold from this lead and the zone's own marker).
        let restore = self.spawn_align_lead;
        self.spawn_align_lead = sched_lead.filter(|&l| l > 0);
        self.spawn_arrival_override_ms = Some(arrival_eff_ms);
        let ok = self.spawn_zone_voice_at(
            idx,
            to,
            VoiceKind::Legato,
            gain,
            None,
            pitch_offset,
            start_offset,
        );
        self.spawn_arrival_override_ms = None;
        self.spawn_align_lead = restore;
        self.transition_fade = None;
        if std::env::var_os("SIGNAL_LEGATO_DEBUG").is_some() {
            eprintln!(
                "LEGATO {}→{} zone={} root={} int={} dir={} lead_ms={} sched={:?} offset={} pitch_off={} spawned={}",
                from,
                to,
                self.patch.spec.zones[idx].file,
                self.patch.spec.zones[idx].root_key,
                self.patch.spec.zones[idx].interval,
                self.patch.spec.zones[idx].direction,
                self.patch.spec.zones[idx].transition_arrival_ms(),
                sched_lead,
                start_offset,
                pitch_offset,
                ok,
            );
        }
    }

    /// Pick the transition zone for (articulation, direction, dynamic,
    /// named lower note, interval): exact interval at the nearest recorded
    /// root wins; otherwise nearest interval, nearest root. Honours the solo
    /// mic. Destination-pitch correctness never depends on this choice (the
    /// caller re-tunes), only lead-in realism does.
    pub(crate) fn find_transition_zone(
        &self,
        artic: &str,
        direction: &str,
        dynamic: &str,
        named: u8,
        interval: u32,
    ) -> Option<usize> {
        let mut best: Option<(u32, usize)> = None;
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            if z.interval == 0 || !z.articulation.eq_ignore_ascii_case(artic) {
                continue;
            }
            if !z.direction.eq_ignore_ascii_case(direction) {
                continue;
            }
            if !dynamic.is_empty()
                && !z.dynamic.is_empty()
                && !z.dynamic.eq_ignore_ascii_case(dynamic)
            {
                continue;
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            let d_int = z.interval.abs_diff(interval);
            let d_root = u32::from(z.root_key.abs_diff(named));
            let score = d_int * 100 + d_root;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// Spawn the recorded RELEASE sample (e.g. `NVrel`/`Vsusrel`) for the
    /// current sustain articulation when a note is released — CSS's release
    /// tail. No-op if the articulation declares no `release_artic`.
    pub(crate) fn spawn_release(&mut self, note: u8) {
        let rel_id = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.clone());
        let Some(rel_id) = rel_id else {
            return;
        };
        let (lo, hi, blend) = self.layers_for_artic(&rel_id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        // Release samples are non-directional → pass "" (no direction filter).
        // CSS's recorded releases are a subtle bow-off tail UNDER the note's
        // decay — but these samples are normalised loud, so at unity (×makeup)
        // they spike louder than the note itself ("note-off noise"). Trim them.
        if let Some(idx) = self.find_layer_zone(&rel_id, "", &dynamic, note, rr) {
            let release_gain = self.patch.spec.performance.release_gain;
            self.spawn_zone_voice(idx, note, VoiceKind::Release, release_gain, None, 0.0);
        }
    }

    /// Pacific release-overlap: spawn the current articulation's release for
    /// `note` and immediately ramp it to silence over `fade_frames` — the
    /// KSP's `legrel` layer (`$wbgz2` faded over `$amble`), the departed
    /// note's bow-lift sounding UNDER the incoming transition.
    pub(crate) fn spawn_release_overlap(&mut self, note: u8, fade_frames: usize) {
        self.spawn_release(note);
        if let Some(v) = self.voices.last_spawned_mut() {
            if v.note == note {
                v.ramp_gain(0.0, fade_frames.max(1));
            }
        }
    }

    /// One-shot ATTACK layer spawned with a fresh sustain note-on (Pacific
    /// atk+sus pairing, `ArticulationSpec.attack_artic`). Plays at the same
    /// CC1 dynamics blend; no-op when the articulation declares none.
    pub(crate) fn spawn_attack_layer(&mut self, note: u8) {
        let atk_id = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.attack_artic.clone());
        let Some(atk_id) = atk_id else {
            return;
        };
        let (lo, hi, blend) = self.layers_for_artic(&atk_id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        if let Some(idx) = self.find_layer_zone(&atk_id, "", &dynamic, note, rr) {
            self.spawn_zone_voice(idx, note, VoiceKind::Short, 1.0, None, 0.0);
        }
    }

    /// Pick the zone index for (articulation, direction, dynamic layer, note),
    /// honouring the solo mic, with simple round-robin over matches by `rr`.
    pub(crate) fn find_layer_zone(
        &self,
        artic: &str,
        direction: &str,
        dynamic: &str,
        note: u8,
        rr: usize,
    ) -> Option<usize> {
        let mut matches: Vec<usize> = Vec::new();
        // Nearest single-key zone when no zone spans the note: CSS records a
        // whole-tone grid (every other semitone) and pitch-shifts ±1 to fill,
        // and the extracted zones are single-key (key_min == key_max), so even
        // notes have no exact zone — fall back to the closest recorded pitch.
        let mut nearest: Option<(u8, usize)> = None;
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            if !z.articulation.eq_ignore_ascii_case(artic) {
                continue;
            }
            if !z.direction.is_empty() && !z.direction.eq_ignore_ascii_case(direction) {
                continue;
            }
            if !dynamic.is_empty()
                && !z.dynamic.is_empty()
                && !z.dynamic.eq_ignore_ascii_case(dynamic)
            {
                continue;
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            if note >= z.key_min && note <= z.key_max {
                matches.push(i);
            } else {
                let dist = (z.root_key as i32 - note as i32).unsigned_abs() as u8;
                if nearest.is_none_or(|(d, _)| dist < d) {
                    nearest = Some((dist, i));
                }
            }
        }
        if !matches.is_empty() {
            return Some(matches[rr % matches.len()]);
        }
        // No exact zone — use the nearest recorded pitch within range and let
        // spawn_zone_voice pitch-shift it (note - root_key semitones).
        nearest
            .filter(|(d, _)| *d <= self.patch.spec.performance.zone_pitch_tolerance)
            .map(|(_, i)| i)
    }

    /// Spawn a voice from zone `idx` for `note` with a layer `kind` and
    /// crossfade `gain_scale` (multiplied into the zone's own gain). Returns
    /// false if the sample isn't loaded yet.
    pub(crate) fn spawn_zone_voice(
        &mut self,
        idx: usize,
        note: u8,
        kind: VoiceKind,
        gain_scale: f32,
        dyn_layer: Option<DynLayer>,
        pitch_offset: f64,
    ) -> bool {
        self.spawn_zone_voice_at(idx, note, kind, gain_scale, dyn_layer, pitch_offset, 0)
    }

    /// [`spawn_zone_voice`](Self::spawn_zone_voice) starting `start_offset`
    /// frames INTO the zone's window — the legato prefire path skips the
    /// surplus of a transition sample's measured lead-in so the pitch change
    /// lands exactly on the destination tick.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_zone_voice_at(
        &mut self,
        idx: usize,
        note: u8,
        kind: VoiceKind,
        gain_scale: f32,
        dyn_layer: Option<DynLayer>,
        pitch_offset: f64,
        start_offset: usize,
    ) -> bool {
        // Copy out the zone fields up front so no borrow of `self.patch`
        // outlives the `&mut self` cache-miss bookkeeping below.
        let z = &self.patch.spec.zones[idx];
        let (root_key, tune_cents, gain_db, mic, pan) =
            (z.root_key, z.tune_cents, z.gain_db, z.mic.clone(), z.pan);
        // Per-articulation fixed transpose (CSS Harmonics -12: shipped zones are
        // mapped an octave above CSS's sounded pitch). Applied to the playback
        // rate only; zone selection already keyed off the played note.
        let artic_transpose = self
            .patch
            .spec
            .articulation(&z.articulation)
            .map(|a| a.transpose as f64)
            .unwrap_or(0.0);
        let (sample_start, sample_end) = (z.sample_start, z.sample_end);
        let playback_mode = z.playback_mode.clone();
        let (loop_start, loop_end) = (z.loop_start, z.loop_end);
        let alternating = zone_is_alternating_loop(z);
        // Zone descriptors for the render trace — cloned only while tracing.
        let trace_zone = self.trace_enabled.then(|| {
            (
                z.file.clone(),
                z.articulation.clone(),
                z.dynamic.clone(),
                z.direction.clone(),
                z.interval,
            )
        });
        // Stem class: a Release voice follows its PARENT articulation (a
        // short's release lands in the Shorts stem); everything else is
        // classed by the zone's own articulation.
        let artic_class = if matches!(kind, VoiceKind::Release) {
            self.artic_class_for(&self.articulation)
        } else {
            self.artic_class_for(&z.articulation)
        };
        let line = self.cur_line as u8;
        let path = self.patch.zone_paths[idx].clone();

        let Some(data) = self.cache.get_loaded(&path) else {
            self.cache_misses
                .set(self.cache_misses.get().saturating_add(1));
            self.record_cache_miss(&path);
            return false;
        };
        let num_frames = data.num_frames;

        // `pitch_offset` lets legato keep the correct note-tag (`to`, for
        // note-off/silence) while shifting the recorded transition so it lands
        // on the target (CSS legato samples are source-labelled a grid step away).
        let semitones = note as f64 - root_key as f64 + pitch_offset + artic_transpose;
        // Split pitch from playback speed (whole-tone-grid fill): the integer
        // TRANSPOSITION (note - root_key) becomes a time-preserving pitch
        // shift on the voice so an off-grid note keeps its recorded arrival
        // timing; only TUNING (per-zone + master, tiny) rides `rate`, which
        // now carries just tuning × SR-conversion. `transpose_cents == 0` on
        // the recorded grid → no shifter, byte-identical to before.
        let transpose_cents = semitones * 100.0;
        let tune_cents_total = tune_cents as f64 + self.master_tune_cents();
        let rate = 2.0f64.powf(tune_cents_total / 1200.0);

        // Marker position for playback emission (FILE frames): the zone's
        // heard-arrival CLAIM, exactly the ladder the alignment uses —
        // measured `arrival_ms`, else `lead_in_ms` for pitch transitions,
        // else the global short pre-delay, else "heard at playback start"
        // (0 → the marker emits on the voice's first sounding frame).
        // Attack kinds only: release tails and CC-trigger voices have no
        // grid arrival.
        let marker_arrival_file = if matches!(
            kind,
            VoiceKind::Legato | VoiceKind::Short | VoiceKind::SustainLayer
        ) {
            let z = &self.patch.spec.zones[idx];
            let ms = if z.arrival_ms > 0.0 {
                z.arrival_ms
            } else if matches!(kind, VoiceKind::Legato) && z.interval > 0 {
                z.lead_in_ms
            } else if matches!(kind, VoiceKind::Short) {
                self.patch
                    .spec
                    .short_note_timing
                    .as_ref()
                    .map(|t| t.pre_delay_ms as f32)
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            Some(f64::from(ms) / 1000.0 * data.sample_rate as f64 + f64::from(sample_start))
        } else {
            None
        };

        // ── Per-zone arrival alignment (document scheduling) ─────────────
        // The scheduler pre-rolled this trigger by `spawn_align_lead` wall
        // frames (an upper bound over the candidate zones); this voice is
        // held back by `lead − its own heard-arrival` so the note is HEARD
        // exactly on the grid tick — per round-robin / mic / dynamic layer.
        // The zone's claim is the MEASURED `arrival_ms` when present, else
        // the historical fallback (shorts: the global `pre_delay_ms`;
        // transitions: metadata `lead_in_ms`; sustains: heard-at-trigger),
        // which reproduces the pre-marker timing for unmeasured zones.
        let start_hold = match self.spawn_align_lead {
            Some(lead) => {
                let z = &self.patch.spec.zones[idx];
                // The zone's arrival claim (ms of sample time from playback
                // start). `None` = no claim at all — an UNMEASURED pitch
                // transition, whose schedule lead was built on the premise
                // that the sample's intrinsic latency IS the lead (legacy
                // behaviour): fire immediately, no hold.
                let eff_ms = if matches!(kind, VoiceKind::Legato)
                    && z.interval > 0
                    && self.spawn_arrival_override_ms.is_some()
                {
                    // Diagnostic sweep: the transition caller already
                    // computed the re-interpreted arrival — use it verbatim.
                    self.spawn_arrival_override_ms
                } else if z.arrival_ms > 0.0 {
                    Some(z.arrival_ms)
                } else if matches!(kind, VoiceKind::Legato) && z.interval > 0 {
                    (z.lead_in_ms > 0.0).then_some(z.lead_in_ms)
                } else if matches!(kind, VoiceKind::Short) {
                    // Unmeasured short: the historical claim is the global
                    // recorded pre-delay.
                    Some(
                        self.patch
                            .spec
                            .short_note_timing
                            .as_ref()
                            .map(|t| t.pre_delay_ms as f32)
                            .unwrap_or(0.0),
                    )
                } else {
                    // Unmeasured sustain / re-trigger: heard at trigger.
                    Some(0.0)
                };
                match eff_ms {
                    Some(eff_ms) => {
                        // Arrival in FILE frames from playback start, minus
                        // whatever the caller already skipped off the front
                        // (`start_offset`), converted to wall frames at this
                        // voice's true playback advance (pitch rate ×
                        // source/output sample-rate ratio).
                        let sr_scale = data.sample_rate as f64 / self.sample_rate as f64;
                        let arrival_file = (f64::from(eff_ms) / 1000.0
                            * data.sample_rate as f64)
                            - start_offset as f64;
                        let arrival_wall =
                            (arrival_file.max(0.0) / (rate * sr_scale)).round() as u64;
                        let hold = lead.saturating_sub(arrival_wall) as usize;
                        // The deterministic heard-arrival this spawn actually
                        // produces (fire-log truth for transitions/re-bows).
                        self.last_arrival_prediction =
                            self.frames_rendered + hold as u64 + arrival_wall;
                        hold
                    }
                    None => 0,
                }
            }
            None => 0,
        };
        let is_sustain_layer = matches!(
            kind,
            VoiceKind::SustainNVLo
                | VoiceKind::SustainNVHi
                | VoiceKind::SustainVibLo
                | VoiceKind::SustainVibHi
                | VoiceKind::SustainLo
                | VoiceKind::SustainHi
                | VoiceKind::SustainLayer
        );
        // OUTPUT_MAKEUP is a synth-loop-plateau compensation (see its doc): it
        // applies to looping sustain-layer voices (which replay the sample body's
        // steady plateau ~6 dB below the recorded peak). The MAIN held tone
        // (`%grhcg`) is a sustain layer either way — for the first note it gets
        // the full +6 dB makeup; for a legato-connected note it additionally
        // carries the −6 dB `$3tsb0` legato makeup (`legato_sustain`), netting
        // 0 dB → the note sits ~6 dB below the fresh first note (the real CSS
        // handoff). The one-shot bow-change TRANSITION (`VoiceKind::Legato`, the
        // `%ftriy` attack ornament) is NOT the held body, so it takes NO makeup —
        // it plays at recorded level scaled by CC1, same net level as the −6 dB
        // sustain it overlays. Shorts and release tails play the recording as-is.
        // The −6 dB `$3tsb0` connected-sustain TRIM is STRUCTURAL legato level
        // (a connected note sits 6 dB below the fresh first note), NOT CC1
        // amplitude dynamics — so it stays ON even in pure playback (pure only
        // disables the CC1 dynamics crossfade). Every legato-connected sustain
        // enters 6 dB down and blooms back (below); the first note of a phrase
        // (not `legato_sustain`) keeps the full +6 dB makeup.
        let makeup = if is_sustain_layer {
            let base = db_to_gain(self.patch.spec.performance.sustain_makeup_db);
            if self.legato_trim {
                // −6 dB `$3tsb0` structural trim + the attack-transient
                // anti-machine-gun dip (0 unless the note is <250 ms after the
                // previous onset).
                base * db_to_gain(
                    self.patch.spec.legato_cfg().sustain_trim_db + self.legato_attack_dip_db,
                )
            } else {
                base
            }
        } else {
            1.0
        };
        // CSS `%1wcdh` slow secondary bloom: the −6 dB (`$3tsb0`) connected
        // sustain swells back to FULL body over ~1 s ($foyeb/$g4dbu), so a
        // long held note doesn't sit 6 dB down until its next join — without
        // it the note's carrier has decayed by the time the next
        // transition's pre-bow plays (slow tempi), and the incoming
        // destination pitch reads early against a vanishing source.
        // Multiplicative on the voice, so CC1/CC2 re-levelling keeps working.
        // Structural (the counterpart of the trim above), so it stays in pure.
        let bloom = if is_sustain_layer && self.legato_trim {
            let cfg = self.patch.spec.legato_cfg();
            let frames = ms_to_frames(cfg.sustain_bloom_ms, self.sample_rate);
            (frames > 0).then(|| (frames, db_to_gain(-cfg.sustain_trim_db)))
        } else {
            None
        };
        let gain = 10.0f32.powf(gain_db / 20.0) * gain_scale * makeup;
        let mic_index = self.mic_index_for(&mic);

        // Decoded ENV_FLEX amp envelope for this voice's articulation family
        // (`CSS_GROUP_MOD.md` §2). Built once, cloned per unison copy. Sustains
        // freeze at their hold level while held (indefinite sustain); shorts and
        // releases play their decoded decay one-shot.
        let flex_artic = self.patch.spec.zones[idx].articulation.clone();
        let flex_env = amp_env_for(
            &self.patch.spec,
            &flex_artic,
            &kind,
            is_sustain_layer,
            self.sample_rate,
        );

        // CSS-style sustains ship no loop points but have a slow ~0.8s natural
        // attack pre-roll that CSS skips (Kontakt sample-start) for a fast attack.
        // Start such bodies at the loud steady region so onsets aren't sluggish.
        let synth_loop = is_sustain_layer
            && loop_end <= loop_start
            && playback_mode.is_empty()
            && !alternating
            && num_frames > 0;
        let (sus_lo, sus_hi) = if synth_loop {
            // Loop the sample's steady plateau — NOT a fixed fraction. The old
            // `num_frames/6 .. 0.55` window started mid-bloom (the slow attack
            // swell), so each wrap dropped back to a still-rising level and the
            // held note pulsed. The plateau finder skips the bloom and the
            // end-decay; fall back to the fractional window if it can't resolve.
            let min_len = ms_to_frames(300, self.sample_rate);
            // Search the body only — keep clear of the recorded end-swell /
            // bow-off in the final ~15%, which would otherwise skew the peak.
            let search_hi = (num_frames as f32 * 0.85) as usize;
            steady_loop_region(data.as_ref(), num_frames / 12, search_hi, min_len).unwrap_or_else(
                || {
                    let lo = num_frames / 6;
                    (lo, ((num_frames as f32 * 0.55) as usize).max(lo + 2))
                },
            )
        } else {
            (0, 0)
        };
        // A CSS legato TRANSITION is a ONE-SHOT bow change (spec §2.2): it does
        // NOT loop its tail. The looping SUSTAIN spawned alongside it (faded in
        // underneath) carries the held note. The old model looped the
        // transition's tail; that produced the tick/loop artifacts the KSP
        // rewrite removes.
        let legato_hold = false;
        let (leg_lo, leg_hi) = (0usize, 0usize);
        // Play from the sample's actual start so the recorded attack (the bow
        // onset) is heard — we want the start of the sample, not a mid-sample
        // jump. The loop below still holds the body for indefinite sustain.
        // (`start_offset` shifts into the window for prefired legato
        // transitions whose measured lead-in exceeds the scheduled lead.)
        let start_frame = sample_start as usize + start_offset;

        // Attack envelope. Sustain layers get the full musical attack. Legato
        // transitions and release tails — and ANY voice that starts mid-sample
        // via `start_offset` (prefired transitions skip into the recording) —
        // get a short declick ramp: without it the onset steps from silence to
        // a non-zero sample value and clicks on every note change. Shorts keep
        // their sharp natural attack (they start at true sample-start silence).
        let _ = SUSTAIN_DECLICK_MS;
        let attack = if matches!(kind, VoiceKind::Legato) && self.transition_fade.is_some() {
            // The two-stage transition swell (below) IS the fade-in — no
            // separate declick attack, which would double the onset.
            0
        } else if is_sustain_layer {
            // FRESH sustain attack: the KSP first-note branch plays with NO
            // scripted fade — only the group FLEX declick (4-20 ms) — and the
            // recording's own bow swell delivers the bloom (the measured
            // velocity-dependent rise times ARE the samples). A long engine
            // attack on top double-models the swell and starves the early
            // energy (param-test S13: ref 1.5× our level). Velocity/CC1 shape
            // the onset via layer selection + the attack-transient dip, not an
            // envelope here. Connected sustains keep the authored attack.
            if self.legato_sustain {
                self.attack_frames
            } else {
                ms_to_frames(
                    crate::engine::SUSTAIN_DECLICK_MS.max(20),
                    self.sample_rate,
                )
            }
        } else if start_offset > 0 {
            // Deep mid-sample entry (skipped-swell Low-Latency prefire): fade
            // in over a longer window, scaled to how far we skipped (capped),
            // so the steep bow-change we begin partway through eases in.
            ms_to_frames(self.patch.spec.legato_cfg().skip_declick_ms, self.sample_rate)
                .min(start_offset)
        } else if matches!(kind, VoiceKind::Legato | VoiceKind::Release) {
            ms_to_frames(ONSET_DECLICK_MS, self.sample_rate)
        } else {
            0
        };
        // CSS legato handoff: the main held sustain (`%grhcg`) is spawned during
        // a legato move with only a short declick fade ((0, ~12 ms) — it plays
        // IMMEDIATELY at full level, it is NOT muted/held silent. Overrides the
        // normal attack for this voice only.
        let fade_in_under = if is_sustain_layer {
            self.sustain_fade_in
        } else if matches!(kind, VoiceKind::Legato) {
            // Transition voice: emerge via the two-stage swell (delay 0 — the
            // `start_hold` above already provides the silent pre-roll).
            self.transition_fade.map(|(s1, s1d, s2)| (0, s1, s1d, s2))
        } else {
            None
        };
        // Unison: spawn N copies spread across ±detune/2 cents + a pan spread,
        // level-compensated. copies == 1 is the normal single-voice path.
        let (copies, det_cents, width) = self.unison;
        let copies = copies.max(1);
        let comp = 1.0 / (copies as f32).sqrt();
        for k in 0..copies {
            let off = if copies == 1 {
                0.0
            } else {
                (k as f32 / (copies - 1) as f32) * 2.0 - 1.0
            };
            // Source-vs-output sample-rate compensation (see Voice::with_rate_scale).
            let sr_scale = data.sample_rate as f64 / self.sample_rate as f64;
            let u_rate = rate * 2f64.powf((off * det_cents * 0.5) as f64 / 1200.0) * sr_scale;
            let u_pan = (pan + off * width).clamp(-1.0, 1.0);
            let mut voice = Voice::with_rate(
                data.clone(),
                note,
                kind.clone(),
                u_rate,
                gain * comp,
                self.release_frames,
            )
            .with_mic_index(mic_index)
            .with_line(line)
            .with_artic_class(artic_class)
            .with_pan(u_pan)
            .with_attack(attack)
            .with_start_hold(start_hold)
            .with_pitch_cents(transpose_cents)
            .with_sample_window(start_frame, (sample_end > 0).then_some(sample_end as usize));
            // Playback-emitted arrival marker: attach the zone's heard-
            // arrival position (FILE frames at the SOURCE rate) so the voice
            // emits the marker when its real playhead crosses it. Underlay
            // sustains spawned during a legato handoff are excluded — their
            // muted `attack_delay` advances the playhead while inaudible, so
            // a crossing there would not be a heard arrival.
            if let Some(m) = marker_arrival_file {
                if !(is_sustain_layer && self.legato_sustain) {
                    voice = voice.with_arrival_marker(m, self.frames_rendered);
                }
            }
            if let Some((delay, stage1_run, stage1_denom, stage2)) = fade_in_under {
                voice = voice.with_two_stage_fade_in(delay, stage1_run, stage1_denom, stage2);
            }
            // Portamento micro-glide on the incoming legato voices (CSS `$ma0b1`),
            // scooping from -jyttf up to true pitch (0).
            if let Some((start_cents, frames)) = self.legato_glide {
                voice = voice.with_pitch_glide(start_cents, 0.0, frames);
            }
            if let Some((frames, target)) = bloom {
                voice = voice.with_slow_bloom(frames, target);
            }
            if let Some(layer) = dyn_layer {
                voice = voice.with_dyn_layer(layer);
            }
            // ENV_FLEX stays in pure playback: its attack segment is a utility
            // declick (and, with amp_env_hold, it just holds a flat plateau —
            // it was never the per-note accent; that was the CC1 crossfade +
            // legato trim/bloom). Pure only disables the CC1 AMPLITUDE
            // DYNAMICS, not the utility fades / arrival timing.
            if let Some(flex) = flex_env.clone() {
                voice = voice.with_flex_env(flex);
            }
            let loop_xfade =
                ms_to_frames(self.patch.spec.performance.loop_xfade_ms, self.sample_rate);
            // Effective loop window for this voice: (start, end, xfade). An
            // `end == 0` means the voice plays once to the end (no loop). This
            // is the single source of truth for both the voice and the trace.
            let (eff_ls, eff_le, eff_xf) = if playback_mode.eq_ignore_ascii_case("reverse") {
                voice = voice.reversed();
                (0usize, 0usize, 0usize)
            } else if alternating {
                voice = voice.with_alternating_loop(loop_start as usize, loop_end as usize);
                (loop_start as usize, loop_end as usize, 0)
            } else if loop_end > loop_start {
                voice = voice
                    .with_forward_loop(loop_start as usize, loop_end as usize)
                    .with_loop_xfade(loop_xfade);
                (loop_start as usize, loop_end as usize, loop_xfade)
            } else if synth_loop {
                // CSS sustain samples ship no loop points but must hold
                // indefinitely. Loop the sample's steady plateau (found above),
                // crossfaded at the wrap so the held note neither pulses nor
                // clicks.
                if k == 0 && std::env::var_os("SIGNAL_LOOP_DEBUG").is_some() {
                    eprintln!(
                        "SYNTH_LOOP note={note} len={num_frames} loop={sus_lo}..{sus_hi} \
                         ({:.0}%..{:.0}%) xf={loop_xfade}",
                        100.0 * sus_lo as f32 / num_frames as f32,
                        100.0 * sus_hi as f32 / num_frames as f32,
                    );
                }
                voice = voice
                    .with_forward_loop(sus_lo, sus_hi)
                    .with_loop_xfade(loop_xfade);
                (sus_lo, sus_hi, loop_xfade)
            } else if legato_hold {
                voice = voice
                    .with_forward_loop(leg_lo, leg_hi)
                    .with_loop_xfade(loop_xfade);
                (leg_lo, leg_hi, loop_xfade)
            } else {
                (0, 0, 0)
            };
            // Mint the trace id BEFORE the pool consumes the voice so the
            // VoiceEnd sweep can correlate this voice's lifetime.
            let trace_voice_id = if k == 0 && self.trace_enabled {
                let id = self.next_trace_voice_id();
                voice.trace_id = Some(id);
                Some(id)
            } else {
                None
            };
            voice.prime_pitch_shifters();
            self.voices.spawn(voice);

            // Render trace: record the FIRST copy (unison detune copies are
            // acoustically the same spawn) with everything needed to reason
            // about it after the fact — file, pitch, gain, loop window.
            if let Some(id) = trace_voice_id {
                if let Some((file, artic, dynamic, direction, interval)) = trace_zone.clone() {
                    self.trace_push(TraceKind::VoiceSpawn(TraceVoiceSpawn {
                        voice_id: id,
                        voice_kind: kind.trace_name(),
                        file,
                        note,
                        root_key,
                        rate,
                        gain,
                        dynamic,
                        articulation: artic,
                        mic: mic.clone(),
                        direction,
                        interval,
                        rr: 0,
                        start_frame,
                        loop_start: eff_ls,
                        loop_end: eff_le,
                        loop_xfade: eff_xf,
                    }));
                }
            }
        }
        true
    }

    pub(crate) fn trigger_zoned_groups(
        &mut self,
        mut by_mic: std::collections::BTreeMap<String, Vec<usize>>,
        event_note: Option<u8>,
        velocity: u8,
        trigger: ZoneTrigger,
        record_empty_miss: bool,
    ) {
        // Single-mic solo: keep only the requested mic's bucket so multi-mic
        // zone sets (CSS ships Main + Mix in one set, no `mics` block) don't
        // fold every mic to bus 0 and double. Centralised here so it applies to
        // every trigger path (attack / release / CC / aftertouch / event).
        if let Some(solo) = &self.solo_mic {
            by_mic.retain(|mic, _| mic.eq_ignore_ascii_case(solo));
        }
        if by_mic.is_empty() {
            if record_empty_miss {
                self.sample_misses
                    .set(self.sample_misses.get().saturating_add(1));
                self.record_sample_miss(format!(
                    "zone note={} velocity={velocity}",
                    event_note
                        .map(|note| note.to_string())
                        .unwrap_or_else(|| "event".to_string())
                ));
                tracing::debug!(note = ?event_note, velocity, trigger = ?trigger, "zone miss");
            }
            return;
        }

        let rr_idx = self.zone_rr_counter;
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);
        // Reuse scratch buffers across note-ons: `mem::take` swaps in an empty
        // Vec (no allocation) and we restore the grown buffer afterwards, so
        // these allocate only on the first few note-ons, never steady-state.
        let mut all_indices = std::mem::take(&mut self.zone_indices_scratch);
        all_indices.clear();
        all_indices.extend(by_mic.values().flatten().copied());
        // Packed key: trigger discriminant | note (+1, 0 = None) | velocity.
        let rr_key = ((trigger as u8 as u64) << 16)
            | ((event_note.map(|n| n as u64 + 1).unwrap_or(0)) << 8)
            | velocity as u64;
        let last_slot = self.zone_rr_last_slots.get(&rr_key).copied();
        let selected_rr_slot = select_zone_rr_slot(
            &self.patch.spec.zones,
            &all_indices,
            rr_idx,
            last_slot,
            &mut self.zone_rr_random_state,
            self.forced_rr,
        );
        self.zone_rr_last_slots.insert(rr_key, selected_rr_slot);
        self.zone_indices_scratch = all_indices;

        let mut choked_groups = std::mem::take(&mut self.zone_choked_scratch);
        choked_groups.clear();
        for indices in by_mic.values() {
            let z = &self.patch.spec.zones
                [select_zone_rr_index_by_slot(&self.patch.spec.zones, indices, selected_rr_slot)];
            for group in z.off_by.iter().filter(|group| !group.is_empty()) {
                push_unique_u64(&mut choked_groups, stable_group_hash(group));
            }
            if !z.choke_group.is_empty() {
                push_unique_u64(&mut choked_groups, stable_group_hash(&z.choke_group));
            }
        }
        // Engine-wide choke: silence the group when this hit is a choking one
        // (mono for hi-hats; only the "Choke" articulation for cymbals).
        if let Some(group) = self.engine_choke_group {
            if self.should_engine_choke() {
                push_unique_u64(&mut choked_groups, group);
            }
        }
        for &group in &choked_groups {
            self.voices
                .silence_choke_group(group, self.legato_fade_frames);
        }
        self.zone_choked_scratch = choked_groups;

        let mut capped_groups = std::mem::take(&mut self.zone_capped_scratch);
        capped_groups.clear();
        for indices in by_mic.values() {
            let z = &self.patch.spec.zones
                [select_zone_rr_index_by_slot(&self.patch.spec.zones, indices, selected_rr_slot)];
            if z.group_polyphony > 0 {
                if let Some(group) = zone_choke_group(z) {
                    push_unique_group_limit(&mut capped_groups, group, z.group_polyphony as usize);
                }
            }
        }
        for &(group, max_voices) in &capped_groups {
            if self.voices.active_choke_group_count(group) >= max_voices {
                self.voices
                    .silence_choke_group(group, self.legato_fade_frames);
            }
        }
        self.zone_capped_scratch = capped_groups;

        for (mic_id, indices) in by_mic {
            let pick =
                select_zone_rr_index_by_slot(&self.patch.spec.zones, &indices, selected_rr_slot);
            let z = &self.patch.spec.zones[pick];
            // Tag the new voice into the engine-wide choke group (if any) so
            // the next hit can silence it; an explicit zone choke wins.
            let choke_group = zone_choke_group(z).or(self.engine_choke_group);
            let path = self.patch.zone_paths[pick].clone();
            let Some(data) = self.cache.get_loaded(&path) else {
                self.cache_misses
                    .set(self.cache_misses.get().saturating_add(1));
                self.record_cache_miss(&path);
                tracing::trace!("zone sample not yet loaded: {}", path.display());
                continue;
            };

            let note = event_note.unwrap_or(z.root_key);
            // Percussion (and articulation-pinned) engines play at natural
            // pitch — the routed note is a trigger selector, not a transpose.
            // Without this a drum routed to any note other than its zone key
            // would detune (e.g. a tom on note 45 vs root 50 = -5 semitones).
            let semitones = if self.percussion || self.pinned_articulation.is_some() {
                0.0
            } else {
                note as f64 - z.root_key as f64
            };
            // Same pitch/speed split as spawn_zone_voice_at: transposition →
            // time-preserving shifter; tuning → rate.
            let transpose_cents = semitones * 100.0;
            let rate = 2.0f64.powf((z.tune_cents as f64 + self.master_tune_cents()) / 1200.0);
            let gain = 10.0f32.powf(z.gain_db / 20.0);
            let mic_index = self.mic_index_for(&mic_id);

            // Percussion plays one-shot: the sample rings to its natural end
            // and note-off never cuts it (a drum is struck, not held). Pitched
            // samplers keep held/zoned semantics unless the zone says one-shot.
            let voice_kind = if trigger == ZoneTrigger::Release {
                VoiceKind::Release
            } else if zone_is_one_shot(z) || self.percussion {
                VoiceKind::Short
            } else {
                VoiceKind::Zoned
            };
            // Stem class: releases follow the parent articulation; direct
            // triggers are classed by their zone's articulation.
            let artic_class = if matches!(voice_kind, VoiceKind::Release) {
                self.artic_class_for(&self.articulation)
            } else {
                self.artic_class_for(&z.articulation)
            };
            let line = self.cur_line as u8;
            // Unison: spawn N detuned/panned copies (copies == 1 = normal).
            let (copies, det_cents, width) = self.unison;
            let copies = copies.max(1);
            let comp = 1.0 / (copies as f32).sqrt();
            for k in 0..copies {
                let off = if copies == 1 {
                    0.0
                } else {
                    (k as f32 / (copies - 1) as f32) * 2.0 - 1.0
                };
                // Source-vs-output sample-rate compensation (see Voice::with_rate_scale).
                let sr_scale = data.sample_rate as f64 / self.sample_rate as f64;
                let u_rate = rate * 2f64.powf((off * det_cents * 0.5) as f64 / 1200.0) * sr_scale;
                let u_pan = (z.pan + off * width).clamp(-1.0, 1.0);
                let mut voice = Voice::with_rate(
                    data.clone(),
                    note,
                    voice_kind.clone(),
                    u_rate,
                    gain * comp,
                    self.release_frames,
                )
                .with_mic_index(mic_index)
                .with_line(line)
                .with_artic_class(artic_class)
                .with_choke_group(choke_group)
                .with_pan(u_pan)
                .with_attack(self.attack_frames)
                .with_pitch_cents(transpose_cents)
                .with_sample_window(
                    z.sample_start as usize,
                    (z.sample_end > 0).then_some(z.sample_end as usize),
                );
                if z.playback_mode.eq_ignore_ascii_case("reverse") {
                    voice = voice.reversed();
                } else if zone_is_alternating_loop(z) {
                    voice = voice.with_alternating_loop(z.loop_start as usize, z.loop_end as usize);
                } else {
                    voice = voice.with_forward_loop(z.loop_start as usize, z.loop_end as usize);
                }
                voice.prime_pitch_shifters();
                self.voices.spawn(voice);
            }
        }
    }

    /// Process a MIDI CC event.
    pub fn cc(&mut self, controller: u8, value: u8) {
        self.cc_line(0, controller, value);
        // Live single-channel dispatch: CC1 (dynamics) and CC2 (vibrato) are the
        // one player's mod / expression wheel and govern EVERY mono-legato /
        // auto-divisi line the allocator may place a note on — not just line 0.
        // Without this, a note the live divisi allocator routes to a line ≠ 0
        // renders at that line's STALE CC1 (init default 64), which flattens the
        // entire CC1 dynamic sweep (every SUS-DYN note came out at the cc1=64
        // level regardless of the mod wheel). Document mode addresses CCs by the
        // event's real channel via `cc_line()` and is unaffected.
        if matches!(controller, 1 | 2) {
            for l in 1..MAX_LINES {
                match controller {
                    1 => self.lines[l].cc1 = value,
                    2 => self.lines[l].cc2 = value,
                    _ => {}
                }
                self.set_active_line(l as LineId);
                if controller == 2 {
                    self.next_cc_ramp =
                        Some(ms_to_frames(crate::engine::CC2_RAMP_MS, self.sample_rate));
                }
                self.update_sustain_gains();
            }
            self.set_active_line(0);
        }
    }

    /// Line-addressed CC. CC1 (dynamics) and CC2 (vibrato) are per-line
    /// state — a divisi line's expression rides its own controller lane and
    /// re-levels only that line's held voices. Every other controller
    /// (CC58 articulation/mode, CC64 pedal, CC11 volume, …) is engine-global:
    /// divisi desks of one section share articulation, pedal, and output
    /// level. Document CCs carry their channel; the scheduler resolves it to
    /// a line before calling this.
    pub fn cc_line(&mut self, line: LineId, controller: u8, value: u8) {
        self.set_active_line(line);
        let old_value = self
            .cc_values
            .get(controller as usize)
            .copied()
            .unwrap_or(0);
        if let Some(slot) = self.cc_values.get_mut(controller as usize) {
            *slot = value;
        }
        if self.patch.is_zoned() {
            self.trigger_cc_zones(controller, old_value, value);
        }
        // Latched-CC articulation selector (spec `selector uacc`): the
        // selector's CC number is data-configured, so it's matched here
        // rather than in the fixed-CC arms below. Engine-global, like CC58.
        // r[impl signal.sampling.articulation.select]
        if self
            .latched_cc_selector
            .as_ref()
            .is_some_and(|sel| sel.cc == controller)
        {
            self.apply_latched_cc_selector(value);
        }
        match controller {
            1 => {
                self.cc1 = value;
                self.line_mut().cc1 = value;
                // Short-note articulations use CC1 to select sub-type (spiccato/
                // staccato/pizzicato/etc.); sustain articulations use it for dynamics.
                let is_short = self
                    .patch
                    .spec
                    .articulation(&self.articulation)
                    .map(|a| a.kind == ArticulationKind::Short)
                    .unwrap_or(false);
                if is_short {
                    // KSP-confirmed model: CC1 selects the short TYPE via
                    // `short_note_cc1_map` (the reference collapses every short to
                    // the CC1=90 type); VELOCITY selects the dynamic layer at
                    // trigger time. CC1 is never the short DYNAMIC axis.
                    self.apply_cc1_short_select();
                } else {
                    self.update_sustain_gains();
                }
            }
            2 => {
                self.cc2 = value;
                self.line_mut().cc2 = value;
                // CC2 (vibrato crossfade) re-levels over its decoded 1000 ms lag.
                self.next_cc_ramp =
                    Some(ms_to_frames(crate::engine::CC2_RAMP_MS, self.sample_rate));
                self.update_sustain_gains();
            }
            58 => {
                self.cc58 = value;
                self.apply_cc58();
            }
            59 => {
                // CC59: round-robin reset (v1.7). Value is the 0-based starting
                // index. Resets all RR counters so the next short-note passage
                // plays back the same RR sequence every time.
                self.rr.borrow_mut().reset_to(value as usize);
                self.zone_rr_counter = value as usize;
                self.zone_rr_last_slots.clear();
            }
            64 => {
                let was_held = self.cc64_held;
                self.cc64_value = value;
                self.cc64_held = value >= 64;

                // Sustain-pedal articulation swap: some libraries ship a full
                // distinct pedal-down BODY keymap (a different string
                // resonance) that replaces the played articulation while the
                // pedal is held. `find_pedal_pair` returns only genuine
                // full-span bodies — pedal NOISE (`lacrped`) is handled
                // separately below so it never masquerades as the body.
                if !was_held && self.cc64_held {
                    let restored = self.voices.repedal_releasing();
                    if restored > 0 {
                        tracing::debug!(restored, "pedal down repedal");
                    }
                    if self.patch.is_zoned() {
                        self.trigger_event_zones(ZoneTrigger::PedalDown, value);
                    }
                    if let Some(pedal_id) = self.find_pedal_pair(&self.articulation) {
                        self.no_pedal_articulation = Some(self.articulation.clone());
                        self.articulation = pedal_id;
                    }
                    // Felt + mechanical pedal-down noise — one-shot ambience
                    // layer, independent of any body swap. No-ops silently
                    // when the pack ships no pedal-noise samples.
                    self.trigger_pedal_noise(true);
                } else if was_held && !self.cc64_held {
                    if let Some(orig) = self.no_pedal_articulation.take() {
                        self.articulation = orig;
                    }
                    if self.patch.is_zoned() {
                        self.trigger_event_zones(ZoneTrigger::PedalUp, value);
                    }
                    // Pedal-up release noise (damper drop + mechanical return).
                    self.trigger_pedal_noise(false);
                }

                if was_held && !self.cc64_held {
                    let release_frames = self.pedal_release_frames();
                    // Pedal released — release only notes that received note-off
                    // while the pedal was down. Notes still physically held have
                    // no deferred note-off entry and must keep sounding.
                    let notes: Vec<(u8, u8)> = self
                        .deferred_note_off_velocities
                        .iter()
                        .map(|(&note, &release_velocity)| (note, release_velocity))
                        .collect();
                    tracing::debug!(
                        deferred = notes.len(),
                        still_held = self.held_notes.len().saturating_sub(notes.len()),
                        "pedal up release"
                    );
                    self.deferred_note_off_velocities.clear();
                    for (note, velocity) in notes {
                        self.held_notes.remove(&note);
                        if self.patch.is_zoned() {
                            self.trigger_zoned(note, velocity, ZoneTrigger::Release, false);
                            self.voices
                                .note_off_with_release_frames(note, Some(release_frames));
                        } else {
                            self.do_note_off_with_release_frames(note, velocity, release_frames);
                        }
                    }
                }
            }
            // CSS "Volume" — master output level (separate from CC1 dynamics).
            11 => self.cc11_volume = value as f32 / 127.0,
            // CSS "Portamento Volume" — level of the portamento glide.
            5 => self.cc5_porta_volume = value as f32 / 127.0,
            // All Sound Off (immediate) / All Notes Off (release held) — standard
            // MIDI panic CCs, so they work through the one MIDI dispatch path.
            120 => self.panic(),
            123 => self.all_notes_off(),
            _ => {}
        }
    }

    pub fn channel_aftertouch(&mut self, value: u8) {
        let old_value = self.channel_aftertouch;
        self.channel_aftertouch = value;
        if self.patch.is_zoned() {
            self.trigger_aftertouch_zones(None, old_value, value);
        }
    }

    pub fn poly_aftertouch(&mut self, note: u8, value: u8) {
        let old_value = self
            .poly_aftertouch
            .get(note as usize)
            .copied()
            .unwrap_or(0);
        if let Some(slot) = self.poly_aftertouch.get_mut(note as usize) {
            *slot = value;
        }
        if self.patch.is_zoned() {
            self.trigger_aftertouch_zones(Some(note), old_value, value);
        }
    }
}
