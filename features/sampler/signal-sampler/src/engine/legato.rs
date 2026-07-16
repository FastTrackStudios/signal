//! Mono legato-line handling for `SampleEngine` — the 3-voice CSS legato
//! handoff. Split out of `engine/mod.rs`; same impl (strings articulation).

use super::*;

impl SampleEngine {

    /// Make `line` the active line for the current dispatch: line-scoped
    /// state (mono note, press order, pending countdown) and the CC1/CC2
    /// mirrors now refer to it. Out-of-range ids clamp into the pool.
    pub(crate) fn set_active_line(&mut self, line: LineId) {
        let l = line.min(MAX_LINES - 1);
        self.cur_line = l;
        self.cc1 = self.lines[l].cc1;
        self.cc2 = self.lines[l].cc2;
    }

    pub(crate) fn line(&self) -> &LegatoLine {
        &self.lines[self.cur_line]
    }

    pub(crate) fn line_mut(&mut self) -> &mut LegatoLine {
        &mut self.lines[self.cur_line]
    }

    /// Running render position in frames since construction.
    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }

    /// Document-mode note-on for a legato-followed note: fire the transition
    /// NOW instead of arming the reactive countdown. The document scheduler
    /// calls this `delay_ms` (the spec's expressive velocity→delay) *before*
    /// the destination note's tick, so the transition's audible arrival lands
    /// exactly on the grid — the inversion of the reactive path, where the
    /// countdown makes the note speak `delay_ms` *after* its tick
    /// (see `docs/plan/document-mode.md`).
    ///
    /// The scheduler emits this INSTEAD of a note-on for the note; all
    /// held-note / mono-line bookkeeping happens here. Degrades gracefully:
    /// non-zoned patches, legato-off, or an empty legato line fall back to
    /// the plain note-on behaviour.
    pub fn legato_prefire(&mut self, note: u8, velocity: u8) {
        self.legato_prefire_line(0, note, velocity);
    }

    /// Line-addressed [`legato_prefire`](Self::legato_prefire) — the document
    /// scheduler resolves each scheduled event to a [`LineId`] (currently the
    /// channel→line allocator: chan N → line N) so every divisi line runs its
    /// own prefired mono legato.
    pub fn legato_prefire_line(&mut self, line: LineId, note: u8, velocity: u8) {
        self.legato_prefire_line_lead(line, note, velocity, None);
    }

    /// [`legato_prefire_line`](Self::legato_prefire_line) carrying the
    /// schedule's prefire lead (frames from this call to the destination
    /// tick). The engine aligns the transition sample's MEASURED lead-in to
    /// it: a sample with a longer lead-in starts partway in, a shorter one
    /// is held back on a countdown — either way the pitch change lands
    /// exactly on the tick. `None` = fire now, arrival lands wherever the
    /// sample's own lead-in puts it (live/back-compat behaviour).
    pub fn legato_prefire_line_lead(
        &mut self,
        line: LineId,
        note: u8,
        velocity: u8,
        sched_lead: Option<u64>,
    ) {
        self.set_active_line(line);
        if velocity == 0 {
            self.note_off_line(line, note);
            return;
        }
        if self.try_keyswitch(note, velocity) {
            return;
        }
        if !self.patch.is_zoned() || !self.legato_enabled {
            self.note_on_line(line, note, velocity);
            return;
        }
        self.held_notes.insert(note, velocity);
        let l = self.line_mut();
        l.order.retain(|&n| n != note);
        l.order.push(note);
        match self.line().note {
            // No sounding line (document started mid-phrase, or annotation
            // was optimistic): start the note plainly.
            None => {
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
                self.line_mut().note = Some(note);
            }
            // Fire the transition — no reactive countdown. The scheduler
            // subtracted the measured lead-in from the trigger time; if the
            // zone that will actually play has a SHORTER lead-in (e.g. a
            // different CC1 layer than the schedule's estimate), hold the
            // fire back by the difference so the arrival still lands on the
            // destination tick. (NOT counted as a reactive fire.)
            Some(cur) => {
                // Document prefire: the scheduler already computed the lead
                // (IOI-driven); here we only need the portamento flag.
                let (_delay_ms, portamento) = self.legato_timing(velocity, 0);
                self.play_direction = if note >= cur { "up" } else { "down" }.to_string();
                if let Some(lead) = sched_lead {
                    if let Some(zone_lead) = self.transition_lead_frames(cur, note, portamento) {
                        if zone_lead < lead {
                            // Hold the fire back so the arrival lands ON the
                            // tick (fires via the render countdown; NOT a
                            // reactive fire — the counter is untouched). With
                            // the `$1fvjk` pre-bow lead this branch is
                            // essentially unreachable for CSS (the scheduled
                            // lead ≤ ~60 ms is always < the measured arrival),
                            // so it resolves through the reactive path with the
                            // armed IOI = 0 → deepest-offset fallback.
                            self.line_mut().state = LegatoState::Pending {
                                frames_remaining: (lead - zone_lead) as usize,
                                from_note: cur,
                                to_note: note,
                                to_note_velocity: velocity,
                                portamento,
                                ioi_ms: 0.0,
                            };
                            return;
                        }
                        // Longer lead-in than scheduled: fire now, skipping the
                        // surplus off the sample's front. `arrival − lead·rate`
                        // resolves the offset to the CSS `$1fvjk` value the
                        // scheduler encoded in `lead`; the reactive `ioi_ms` is
                        // unused on this (document) path.
                        self.fire_legato_with_lead(
                            cur,
                            note,
                            velocity,
                            portamento,
                            Some(lead),
                            0.0,
                        );
                        return;
                    }
                }
                // No measurement (legacy library) or live prefire: fire now,
                // the sample's own lead-in lands wherever it lands.
                self.fire_legato_with_lead(cur, note, velocity, portamento, None, 0.0);
            }
        }
    }

    /// Sample rate (frames/s) — for ms↔frame conversion by callers.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Warm (decode into cache, on the caller's thread) every attack zone a
    /// note-on at `note` could trigger under the CURRENT articulation pin +
    /// solo mic — across all dynamic layers and round-robins — so the first
    /// hit at any velocity is never silent. Selection mirrors the live trigger
    /// path (unlike [`warm_note_samples`](Self::warm_note_samples), which uses
    /// the pin/mic-agnostic `resolve_zone` and can warm the wrong articulation).
    /// Returns how many samples were decoded; a no-op for non-zoned patches.
    pub fn warm_note(&self, note: u8) -> crate::engine::cache::PreloadStats {
        use crate::engine::cache::PreloadStats;
        if !self.patch.is_zoned() {
            return PreloadStats::default();
        }
        let artic = self.effective_articulation().map(|s| s.to_string());
        // Also warm the vibrato pair — CC2 can blend it in at any time, and the
        // directional legato samples come in up/down pairs (warmed by ignoring
        // direction below), so any combination is ready without a cold first hit.
        let vib_pair = artic.as_deref().and_then(|a| self.find_vibrato_pair_id(a));
        // And the recorded RELEASE samples (e.g. NVrel/Vsusrel) for both the main
        // articulation and its vib pair — otherwise note-off cache-misses and the
        // release tail is silent (CSS rings out ~0.7s). These are separate
        // articulations triggered on key-up by `spawn_release`.
        let rel = |a: &str| {
            self.patch
                .spec
                .articulation(a)
                .and_then(|art| art.release_artic.clone())
        };
        // And the legato + portamento TRANSITION articulations (Leg/NVLeg/
        // Legzero/NVLegzero/Port), which `spawn_legato_transition` fires on
        // overlapping notes — same cache-miss-→-silent trap as releases.
        // warm_note ignores direction, so both up/down transitions warm.
        // BOTH sides of each CC2 pair warm (the vibrato crossfade can call
        // either at any time — a CC2-low prefire plays NVLeg even when the
        // pinned articulation is the vibrato sustain).
        let (leg_nv, leg_vib) = self.legato_pair_ids(false);
        let (zero_nv, zero_vib) = self.legato_pair_ids(true);
        let leg_rel = leg_nv
            .as_deref()
            .and_then(rel)
            .or_else(|| leg_vib.as_deref().and_then(rel));
        let mut warm_ids: Vec<String> = [
            artic.clone(),
            vib_pair.clone(),
            artic.as_deref().and_then(rel),
            vib_pair.as_deref().and_then(rel),
            leg_nv,
            leg_vib,
            zero_nv,
            zero_vib,
            leg_rel,
            self.find_port_artic_id(),
        ]
        .into_iter()
        .flatten()
        .collect();
        // A pending CC58 velocity-group resolves to its concrete articulation
        // only at note-on (from the note's velocity), so `effective_articulation`
        // above can't see it yet — warm EVERY possible resolution (Trills →
        // HTrills+WTrills, etc.) so the group's first note isn't cold-silent.
        if let Some(kn) = self.pending_cc58_group.and_then(|gi| {
            self.patch
                .spec
                .keyswitch
                .as_ref()
                .and_then(|ks| ks.notes.get(gi))
        }) {
            for val in kn.vel_map.values() {
                for tok in val.split('+').map(str::trim) {
                    if tok.is_empty() || tok.starts_with('@') {
                        continue;
                    }
                    let id = self
                        .patch
                        .spec
                        .articulations
                        .iter()
                        .find(|a| {
                            a.id.eq_ignore_ascii_case(tok) || a.label.eq_ignore_ascii_case(tok)
                        })
                        .map(|a| a.id.clone())
                        .unwrap_or_else(|| tok.to_string());
                    warm_ids.push(id.clone());
                    if let Some(r) = rel(&id) {
                        warm_ids.push(r);
                    }
                }
            }
        }
        let mut stats = PreloadStats::default();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            // Within the zone's range, or within pitch-shift tolerance of its
            // recorded key (whole-tone grid → even notes warm their neighbour).
            let in_range = note >= z.key_min && note <= z.key_max;
            let near =
                (z.root_key as i32 - note as i32).unsigned_abs() as u8
                    <= self.patch.spec.performance.zone_pitch_tolerance;
            if !in_range && !near {
                continue;
            }
            if artic.is_some() {
                let matches_artic = z.articulation.is_empty()
                    || warm_ids
                        .iter()
                        .any(|id| z.articulation.eq_ignore_ascii_case(id));
                if !matches_artic {
                    continue;
                }
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            let path = &self.patch.zone_paths[i];
            if self.cache.get_loaded(path).is_some() {
                continue;
            }
            match self.cache.get(path) {
                Ok(data) => {
                    stats.loaded += 1;
                    stats.bytes += data.decoded_bytes();
                }
                Err(_) => stats.failed += 1,
            }
        }
        stats
    }

    /// Returns the currently active articulation ID.
    pub fn articulation(&self) -> &str {
        &self.articulation
    }

    /// Directly set the active articulation. Used in tests; production code
    /// should generally go through CC58 / keyswitches.
    pub fn set_articulation(&mut self, artic_id: impl Into<String>) {
        self.articulation = artic_id.into();
    }

    /// Pin this engine to a single articulation (percussion kits). When set,
    /// only zones whose `articulation` matches fire, regardless of the
    /// incoming key — so one drum pack can be split across several
    /// performance notes (hats Closed vs Open, snare Hit vs Cross Stick).
    /// `None` (or an empty string) clears the pin.
    pub fn pin_articulation(&mut self, artic: Option<String>) {
        self.pinned_articulation = artic.filter(|s| !s.is_empty());
    }

    /// Set an engine-wide choke group. Voices join it so they can be silenced.
    /// `choke_on` lists the articulations that actually trigger the choke:
    /// empty = monophonic (every hit chokes — hi-hats); non-empty = only those
    /// articulations choke (cymbals: `["Choke"]` so crashes ring but the choke
    /// stops them). `None`/empty group clears it (engine stays polyphonic).
    pub fn set_choke_group(&mut self, group: Option<&str>, choke_on: &[String]) {
        self.engine_choke_group = group.filter(|s| !s.is_empty()).map(stable_group_hash);
        self.engine_choke_on = choke_on
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
    }

    /// Whether this note-on should silence the engine choke group: true when a
    /// group is set and either it's monophonic (`choke_on` empty) or the active
    /// articulation is one of the configured choke triggers.
    pub(crate) fn should_engine_choke(&self) -> bool {
        if self.engine_choke_group.is_none() {
            return false;
        }
        if self.engine_choke_on.is_empty() {
            return true;
        }
        match self
            .trigger_articulation
            .as_deref()
            .or(self.pinned_articulation.as_deref())
        {
            Some(a) => self
                .engine_choke_on
                .iter()
                .any(|c| c.eq_ignore_ascii_case(a)),
            None => false,
        }
    }

    /// Whether a zone should fire for this `(note, velocity, trigger)`,
    /// honoring percussion mode and any pinned articulation.
    ///
    /// - Pinned articulation: match by articulation only, key ignored.
    /// - Single-key percussion (kick, tom): match any note.
    /// - Otherwise: the zone's key range gates as usual (pitched samplers,
    ///   multi-articulation drum packs addressed by their native keys).
    pub(crate) fn zone_selected(
        &self,
        zone: &crate::spec::ZoneSpec,
        note: u8,
        velocity: u8,
        trigger: ZoneTrigger,
    ) -> bool {
        if !zone_trigger_matches(zone, trigger) {
            return false;
        }
        if velocity < zone.vel_min || velocity > zone.vel_max {
            return false;
        }
        // An explicit pin (percussion kits / routed triggers) matches by
        // articulation ONLY, ignoring the key — any routed note plays the pinned
        // articulation's zone.
        if let Some(pin) = self
            .trigger_articulation
            .as_ref()
            .or(self.pinned_articulation.as_ref())
        {
            return zone.articulation.eq_ignore_ascii_case(pin);
        }
        if self.percussion && self.single_attack_key {
            return true;
        }
        // Melodic libraries: only the currently-selected articulation fires
        // (set by keyswitch / CC58 / `set_articulation`). Without this filter,
        // every articulation in a multi-articulation zone set (e.g. CSS ships
        // ~20 in one set) would sound at once. An empty selection = no filter
        // (legacy behaviour for libraries that don't switch articulations).
        if !self.articulation.is_empty()
            && !zone.articulation.is_empty()
            && !zone.articulation.eq_ignore_ascii_case(&self.articulation)
        {
            return false;
        }
        // Directional zones (CSS legato records up- vs down-transitions
        // separately): only the current play direction fires. Non-directional
        // zones (shorts, sustains) carry an empty `direction` and are unaffected.
        if !zone.direction.is_empty() && !zone.direction.eq_ignore_ascii_case(&self.play_direction)
        {
            return false;
        }
        // CC1 dynamic layer: CSS sustains/legato record p/mf/ff at full velocity
        // range and crossfade them by CC1 — without picking one layer they all
        // stack. Velocity-driven (short) articulations keep their vel_min/max
        // gating and are left alone here.
        if !zone.dynamic.is_empty() {
            if let Some(dyn_label) = self.current_zone_dynamic_cc1() {
                if !zone.dynamic.eq_ignore_ascii_case(&dyn_label) {
                    return false;
                }
            }
        }
        note >= zone.key_min && note <= zone.key_max
    }

    /// The dominant CC1 dynamic-layer label for the current articulation, or
    /// `None` for articulations that aren't CC1-controlled (shorts) or have no
    /// declared dynamic layers. Used to pick a single zoned dynamic layer.
    pub(crate) fn current_zone_dynamic_cc1(&self) -> Option<String> {
        let kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());
        if matches!(
            kind,
            Some(ArticulationKind::Short | ArticulationKind::OneShot)
        ) {
            return None;
        }
        let layers = self.active_cc1_layers();
        if layers.is_empty() {
            return None;
        }
        let (lo, hi, blend) = Self::cc1_blend(layers, self.cc1);
        Some(if blend >= 0.5 { hi } else { lo })
    }

    /// The articulation that drives zone selection / warming: an explicit pin
    /// wins (percussion / routed triggers), otherwise the live selection set by
    /// keyswitch / CC58 / [`set_articulation`](Self::set_articulation). `None`
    /// when nothing is selected (legacy "all articulations" behaviour).
    pub(crate) fn effective_articulation(&self) -> Option<&str> {
        self.trigger_articulation
            .as_deref()
            .or(self.pinned_articulation.as_deref())
            .or(if self.articulation.is_empty() {
                None
            } else {
                Some(self.articulation.as_str())
            })
    }

    /// Toggle Con Sordino mode.
    ///
    /// When enabled the engine remaps the current articulation to its sordino
    /// counterpart (`"Vibsus"` → `"SordVibsus"`, etc.) using the `"Sord"`
    /// prefix convention. When disabled it strips the prefix. If no
    /// counterpart exists in the spec the articulation is left unchanged.
    pub fn set_con_sordino(&mut self, active: bool) {
        if self.con_sordino == active {
            return;
        }
        self.con_sordino = active;
        self.articulation = self.remap_sordino(&self.articulation, active);
        if !active {
            // Clear filter state so stale tail doesn't bleed into dry output.
            self.sord_filter.reset();
        }
    }

    /// Returns whether Con Sordino mode is currently active.
    pub fn con_sordino(&self) -> bool {
        self.con_sordino
    }

    /// Number of voices currently active.
    pub fn active_voices(&self) -> usize {
        self.voices.active_count()
    }

    pub fn set_voice_config(&mut self, config: &VoiceConfig) {
        if config.polyphony > 0 {
            self.voices.set_max_voices(config.polyphony as usize);
        }
        if !config.voice_steal.trim().is_empty() {
            self.voices
                .set_steal_policy(VoiceStealPolicy::from_str(&config.voice_steal));
        }
    }

    pub fn max_voices(&self) -> usize {
        self.voices.max_voices()
    }

    pub fn voice_steal_policy(&self) -> &'static str {
        match self.voices.steal_policy() {
            VoiceStealPolicy::ReleaseFirstQuietest => "release_first_quietest",
            VoiceStealPolicy::Oldest => "oldest",
            VoiceStealPolicy::Quietest => "quietest",
            VoiceStealPolicy::SameNoteFirst => "same_note_first",
            VoiceStealPolicy::DropNew => "drop_new",
        }
    }

    pub fn stolen_voices(&self) -> usize {
        self.voices.stolen_count()
    }

    pub fn cache_misses(&self) -> usize {
        self.cache_misses.get()
    }

    pub fn sample_misses(&self) -> usize {
        self.sample_misses.get()
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.recent_cache_misses.borrow().iter().cloned().collect()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.recent_sample_misses.borrow().iter().cloned().collect()
    }
}
