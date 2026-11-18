//! A script's ensemble of players (`PlayersSpec`; MOJO 2's Ensemble, decoded
//! in `_notes/decode-players-divisi.md` beside the library, script_1.ksp line
//! numbers below).
//!
//! One key plays one voice per player, stacked, or — Auto Divisi — the
//! notes arriving within a short window are spread over the players, one
//! voice each. Each player has its own humanize (detune, volume, start
//! offset), pan and key-noise memory; every voice of the instrument loses
//! the ensemble attenuation. At one player none of this exists: the paths
//! below run only when the count is above one.

use super::*;
use crate::spec::PlayersSpec;

/// Kontakt's player slots (`%ukzvh`, `%zjrub`, … are arrays of ten).
pub(crate) const MAX_PLAYERS: usize = 10;
/// Note callbacks that can wait in a divisi gather at once.
const MAX_WAITS: usize = 32;

/// What one player's voice adds (set around a spawn, read by
/// `spawn_zone_voice_at` and `with_script_start`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlayerMods {
    /// `change_vol` (dB).
    pub(crate) gain_db: f32,
    /// `change_pan` (−1..1).
    pub(crate) pan: f32,
    /// `change_tune` (cents).
    pub(crate) cents: f64,
    /// Added to the note's `play_note` offset (ms).
    pub(crate) start_ms: i32,
    /// The key the zone was picked at (`set_event_par(NOTE)`), what a key
    /// modulation reads.
    pub(crate) key: Option<u8>,
    /// The key of the event that plays the voice, whose note-off releases
    /// it (`Voice::env_key`).
    pub(crate) parent: Option<u8>,
}

/// One note callback's choices, kept for a divisi gather that plays later.
#[derive(Debug, Clone)]
pub(crate) struct ShortPlan {
    pub(crate) artic: String,
    pub(crate) dynamic: String,
    pub(crate) gain: f32,
    pub(crate) stretch: f64,
    pub(crate) tune_cents: i32,
    /// The callback's round robin (0-based), `None` = the patch counter's.
    pub(crate) rr: Option<usize>,
    pub(crate) counter_rr: usize,
    pub(crate) velocity: u8,
    pub(crate) line: u8,
    /// The key of the callback that plays (a divisi gather's newest note).
    pub(crate) parent: u8,
}

/// The ensemble's state.
#[derive(Debug, Clone)]
pub(crate) struct PlayersState {
    /// Player count (`$gl1sy`) and Auto Divisi (`$jlrfr`).
    pub(crate) count: u8,
    pub(crate) divisi: bool,
    /// Humanize tables per player: detune m-cents, volume mdB, offset ms
    /// (`%qs4ls`, `%xs5ow`, `%hrr4o`; player 0 is 0).
    pub(crate) tables: [[i32; MAX_PLAYERS]; 3],
    /// Divisi: each player's voice (its strike) and note (`%ukzvh`, `%zjrub`).
    slot_strike: [Option<u32>; MAX_PLAYERS],
    slot_note: [Option<u8>; MAX_PLAYERS],
    /// Each (note, player)'s body (`%gbku1[10·note + p]`), as its strike.
    bodies: Vec<[Option<u32>; MAX_PLAYERS]>,
    /// The note each player last tried a key noise on (`%zjtfu`).
    pub(crate) keynoise_last: [Option<u8>; MAX_PLAYERS],
    /// The gather: notes (`%cnuka`), its owner (`$4vepx`), the shared tick
    /// counter (`$jbard`), the newest note (`$35s1d`) and its callback's plan.
    gathered: [bool; 128],
    owner: Option<u32>,
    ticks: u32,
    last: u8,
    plan: Option<ShortPlan>,
    /// Waiting callbacks: `(id, due frame)`.
    waits: [Option<(u32, u64)>; MAX_WAITS],
    next_id: u32,
}

impl Default for PlayersState {
    fn default() -> Self {
        Self {
            count: 1,
            divisi: false,
            tables: [[0; MAX_PLAYERS]; 3],
            slot_strike: [None; MAX_PLAYERS],
            slot_note: [None; MAX_PLAYERS],
            bodies: vec![[None; MAX_PLAYERS]; 128],
            keynoise_last: [None; MAX_PLAYERS],
            gathered: [false; 128],
            owner: None,
            ticks: 0,
            last: 0,
            plan: None,
            waits: [None; MAX_WAITS],
            next_id: 0,
        }
    }
}

impl PlayersState {
    /// The shipped state of `spec`, its humanize tables drawn from `seed`
    /// (`SeedHumanizationValues` 21457: for p in 1..=9 detune, volume,
    /// offset, each `random(−max, max)`; Kontakt draws them at load, so a
    /// render's script seed never reaches them).
    pub(crate) fn new(spec: &PlayersSpec, seed: u64) -> Self {
        let mut s = Self {
            count: spec.count.clamp(1, MAX_PLAYERS as u8),
            divisi: spec.divisi,
            ..Self::default()
        };
        let mut x = seed | 1;
        let mut draw = |m: i32| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let span = 2 * i64::from(m.max(0)) + 1;
            (x % span as u64) as i32 - m.max(0)
        };
        for p in 1..MAX_PLAYERS {
            s.tables[0][p] = draw(spec.detune_max_mcents);
            s.tables[1][p] = draw(spec.volume_max_mdb);
            s.tables[2][p] = draw(spec.offset_max_ms);
        }
        s
    }
}

/// The player count `value` of the count controller sets (38214-38246):
/// bands of `band`, the first `0..=band`, the last running to 127.
pub(crate) fn count_from_cc(value: u8, band: u8) -> u8 {
    let band = band.max(1);
    let n = if value == 0 { 0 } else { (value - 1) / band };
    (n + 1).min(MAX_PLAYERS as u8)
}

/// The key the `k`-th extra voice of note `e` plays from (25457-25473): the
/// duplicates go down first and then up, as many down as fit below in the
/// range `[lo, hi]` up to half the players.
pub(crate) fn neighbour_key(e: u8, k: u8, lo: u8, hi: u8, players: u8) -> u8 {
    let p = i32::from(players);
    let h = p / 2;
    let below = (i32::from(e) - i32::from(lo)).clamp(0, h);
    let above = (i32::from(hi) - i32::from(e)).clamp(0, h.max(p - below - 1));
    let down = p - 1 - above;
    let k = i32::from(k);
    let key = if k - 1 < down {
        i32::from(e) - k
    } else {
        i32::from(e) + (k - down)
    };
    key.clamp(0, 127) as u8
}

/// Player `p`'s pan in ‰ (25475-25476): even players left, odd right, by
/// pairs, `((p/2 + 1)·spread)/(count/2)`; 0 at one player (KONTRA's `x/0`).
pub(crate) fn player_pan(p: usize, count: u8, spread: i32) -> i32 {
    let half = i32::from(count) / 2;
    if half == 0 {
        return 0;
    }
    let sign = if p % 2 == 0 { -1 } else { 1 };
    (sign * ((p as i32 / 2 + 1) * spread) / half).clamp(-1000, 1000)
}

impl SampleEngine {
    fn players_spec(&self) -> Option<&PlayersSpec> {
        self.patch.spec.performance.players.as_ref()
    }

    /// More than one player: the ensemble paths run.
    pub(crate) fn players_active(&self) -> bool {
        self.players_spec().is_some() && self.players.count > 1
    }

    /// The ensemble attenuation beyond one player's (dB), on every voice.
    pub(crate) fn ensemble_db(&self) -> f32 {
        match self.players_spec() {
            Some(s) if self.players.count > 1 => {
                -(s.attenuation_mdb as f32) * f32::from(self.players.count - 1) / 1000.0
            }
            _ => 0.0,
        }
    }

    /// Divisi plays this articulation's notes (25255): on, more than one
    /// player, and the articulation not in its legato mode.
    pub(crate) fn divisi_applies(&self, artic: &str) -> bool {
        self.players.divisi && self.players_active() && !self.script_legato_on_for(artic)
    }

    /// Test/render override: the humanize tables (players 1..=9 each:
    /// detune m-cents, then volume mdB, then offset ms — 27 values).
    pub fn force_humanize(&mut self, v: &[i32]) {
        for (i, &x) in v.iter().take(27).enumerate() {
            self.players.tables[i / 9][1 + i % 9] = x;
        }
    }

    /// The count and divisi controllers (38214-38258); each message ends
    /// the divisi voices.
    pub(crate) fn players_cc(&mut self, controller: u8, value: u8) {
        let Some(s) = self.players_spec() else {
            return;
        };
        let (count_cc, divisi_cc, band) = (s.count_cc, s.divisi_cc, s.count_band);
        if controller == count_cc {
            self.players.count = count_from_cc(value, band);
        } else if controller == divisi_cc {
            self.players.divisi = value >= 64;
        } else {
            return;
        }
        self.reset_divisi();
    }

    /// `ResetDivisi` (22801): every divisi voice fades, every player is free.
    pub(crate) fn reset_divisi(&mut self) {
        let fade = self
            .players_spec()
            .map_or(0, |s| ms_to_frames_f(s.reset_fade_ms, self.sample_rate));
        for p in 0..MAX_PLAYERS {
            if let Some(strike) = self.players.slot_strike[p].take() {
                self.voices.fade_out_strike_bodies(strike, fade);
            }
            self.players.slot_note[p] = None;
        }
    }

    /// `DoArticulationCutoff` (20235): the gather and the players are
    /// cleared (the voices fade with everything else).
    pub(crate) fn clear_divisi(&mut self) {
        self.players.gathered = [false; 128];
        self.players.slot_strike = [None; MAX_PLAYERS];
        self.players.slot_note = [None; MAX_PLAYERS];
    }

    /// This callback's humanize per player: `(cents, dB, ms)`, the tables
    /// scaled by the knob and truncated (25234-25240).
    fn humanize_now(&self) -> [(f64, f32, i32); MAX_PLAYERS] {
        let h = self.players_spec().map_or(0, |s| s.humanize);
        let scale = |v: i32| (f64::from(h) / 10000.0 * f64::from(v)) as i32;
        let t = &self.players.tables;
        let mut out = [(0.0, 0.0, 0); MAX_PLAYERS];
        for (p, o) in out.iter_mut().enumerate() {
            *o = (
                f64::from(scale(t[0][p])) / 1000.0,
                scale(t[1][p]) as f32 / 1000.0,
                scale(t[2][p]),
            );
        }
        out
    }

    /// The voice keys and round robins of a callback's players (the player
    /// loop's first step, 25457-25474): the first voice of a note keeps its
    /// key and `rr`; each further one in this callback moves to a
    /// neighbouring key and takes the next round robin.
    fn player_keys(
        &mut self,
        assign: &[Option<u8>; MAX_PLAYERS],
        range: Option<(u8, u8)>,
        n_rr: usize,
        rr: usize,
    ) -> [Option<(u8, u8, usize)>; MAX_PLAYERS] {
        let count = self.players.count;
        let mut seen = [0u8; 128];
        let mut out = [None; MAX_PLAYERS];
        for p in 0..usize::from(count) {
            let Some(e) = assign[p] else { continue };
            let k = &mut seen[usize::from(e & 127)];
            out[p] = Some(if *k == 0 {
                (e, e, rr)
            } else {
                let (lo, hi) = range.unwrap_or((0, 127));
                let key = neighbour_key(e, *k, lo, hi, count);
                let next = (usize::from(self.rr_global_last) + 1) % n_rr.max(1);
                self.rr_global_last = next as u8;
                (e, key, next)
            });
            *k += 1;
        }
        out
    }

    /// A note-on with more than one player (from `trigger_zoned_short`):
    /// stacked, every player plays it now; divisi, it joins the gather.
    pub(crate) fn players_note_on(&mut self, note: u8, plan: ShortPlan) {
        if self.divisi_applies(&plan.artic) {
            self.divisi_arrive(note, plan);
        } else {
            let mut assign = [None; MAX_PLAYERS];
            for a in assign.iter_mut().take(usize::from(self.players.count)) {
                *a = Some(note);
            }
            self.push_release_record(note, &plan);
            self.play_players(&plan, &assign, false);
        }
    }

    fn push_release_record(&mut self, note: u8, plan: &ShortPlan) {
        let wants = self
            .patch
            .spec
            .articulation(&plan.artic)
            .is_some_and(|a| a.release_sample.is_some() || a.release_types.is_some());
        if wants {
            self.release_sample_notes[usize::from(note & 127)].push((
                plan.artic.clone(),
                plan.dynamic.clone(),
                plan.velocity,
            ));
        }
    }

    /// Spawn the voices of `assign` (player → note) as one callback: each
    /// player's key and round robin, humanize, pan; then each player's key
    /// noise. `divisi`: the players take their voices (a busy one's old
    /// voice fades: a steal).
    fn play_players(&mut self, plan: &ShortPlan, assign: &[Option<u8>; MAX_PLAYERS], divisi: bool) {
        let artic = plan.artic.clone();
        let (range, n_rr) = self.patch.spec.articulation(&artic).map_or((None, 1), |a| {
            let r = match a.key_range[..] {
                [lo, hi] => Some((lo, hi)),
                _ => None,
            };
            (r, a.rr)
        });
        let rr0 = plan.rr.unwrap_or(plan.counter_rr);
        let keys = self.player_keys(assign, range, n_rr, rr0);
        let hum = self.humanize_now();
        let (count, spread, steal) = self.players_spec().map_or((1, 0, 0), |s| {
            (
                self.players.count,
                s.spread,
                ms_to_frames_f(s.steal_fade_ms, self.sample_rate),
            )
        });
        let saved_stretch = std::mem::replace(&mut self.spawn_stretch, plan.stretch);
        let saved_line = self.cur_line;
        self.set_active_line(LineId::from(plan.line));
        let mut played = [false; MAX_PLAYERS];
        for p in 0..MAX_PLAYERS {
            let Some((e, key, rr)) = keys[p] else {
                continue;
            };
            if divisi {
                if let Some(old) = self.players.slot_strike[p].take() {
                    self.voices.fade_out_strike_bodies(old, steal);
                }
            }
            self.voices.begin_strike();
            let strike = self.voices.current_strike();
            let (cents, db, ms) = hum[p];
            self.spawn_player = Some(PlayerMods {
                gain_db: db,
                pan: player_pan(p, count, spread) as f32 / 1000.0,
                cents,
                start_ms: ms,
                key: Some(key),
                parent: Some(plan.parent),
            });
            self.spawn_short_at(
                &artic,
                &plan.dynamic,
                e,
                key,
                rr,
                plan.gain,
                plan.tune_cents,
            );
            self.spawn_player = None;
            self.players.bodies[usize::from(e & 127)][p] = Some(strike);
            if divisi {
                self.players.slot_strike[p] = Some(strike);
                self.players.slot_note[p] = Some(e);
            }
            played[p] = true;
        }
        // key noise per player (25520-25557): volume and pan, no tune
        for p in 0..MAX_PLAYERS {
            let Some((e, ..)) = keys[p] else { continue };
            if !played[p] {
                continue;
            }
            self.spawn_player = Some(PlayerMods {
                gain_db: hum[p].1,
                pan: player_pan(p, count, spread) as f32 / 1000.0,
                ..PlayerMods::default()
            });
            self.keynoise(&artic, e, plan.velocity, p);
            self.spawn_player = None;
        }
        self.set_active_line(saved_line);
        self.spawn_stretch = saved_stretch;
    }

    /// Spawn one short voice of `artic` sounding `note`, its zone picked at
    /// `key` and round robin `rr` (with the zones layered on it), as
    /// `trigger_zoned_short` does for one player.
    fn spawn_short_at(
        &mut self,
        artic: &str,
        dynamic: &str,
        note: u8,
        key: u8,
        rr: usize,
        gain: f32,
        tune_cents: i32,
    ) {
        let Some(idx) = self
            .find_layer_zone_rr_index(artic, dynamic, key, rr)
            .or_else(|| self.find_layer_zone(artic, "", dynamic, key, rr))
        else {
            return;
        };
        let (dynamic_of, rr_of, mic_of) = {
            let z = &self.patch.spec.zones[idx];
            (z.dynamic.clone(), z.rr_index, z.mic.clone())
        };
        let n = self.patch.spec.zones.len();
        let saved = self.with_script_start(artic);
        let cents = self.spawn_player.map_or(0.0, |m| m.cents);
        // the zones Kontakt layers at this key (as `trigger_zoned_short`);
        // a fixed buffer, none allocated
        let mut layers = [idx; 8];
        let mut n_layers = 1;
        for i in 0..n {
            let o = &self.patch.spec.zones[i];
            if n_layers < layers.len()
                && i != idx
                && !o.disabled
                && o.rr_index == rr_of
                && (o.key_min..=o.key_max).contains(&key)
                && o.articulation.eq_ignore_ascii_case(artic)
                && o.dynamic.eq_ignore_ascii_case(&dynamic_of)
                && o.mic.eq_ignore_ascii_case(&mic_of)
            {
                layers[n_layers] = i;
                n_layers += 1;
            }
        }
        for &i in &layers[..n_layers] {
            self.spawn_zone_voice(
                i,
                note,
                VoiceKind::Short,
                gain,
                None,
                (f64::from(tune_cents) + cents) / 100.0,
            );
        }
        self.spawn_extra_start = saved;
    }

    /// A divisi note arrives (25255-25300): it joins the gather, resets the
    /// shared counter and owns the gather; it plays now if the gathered
    /// notes fill the free players, else it waits in 1 ms ticks.
    fn divisi_arrive(&mut self, note: u8, plan: ShortPlan) {
        self.push_release_record(note, &plan);
        let s = &mut self.players;
        if s.owner.is_none() {
            s.gathered = [false; 128];
        }
        s.last = note;
        s.gathered[usize::from(note & 127)] = true;
        s.next_id = s.next_id.wrapping_add(1);
        let id = s.next_id;
        s.owner = Some(id);
        s.ticks = 0;
        s.plan = Some(plan);
        if self.divisi_waiting() {
            let tick = ms_to_frames(1, self.sample_rate) as u64;
            let due = self.frames_rendered + tick.max(1);
            if let Some(w) = self.players.waits.iter_mut().find(|w| w.is_none()) {
                *w = Some((id, due));
                return;
            }
        }
        self.divisi_play();
    }

    /// The gather's loop condition: ticks left and fewer notes than free
    /// players.
    fn divisi_waiting(&self) -> bool {
        let buffer = self.players_spec().map_or(0, |s| s.buffer_ms);
        let s = &self.players;
        let free = (0..usize::from(s.count))
            .filter(|&p| s.slot_strike[p].is_none())
            .count();
        let gathered = s.gathered.iter().filter(|&&g| g).count();
        s.ticks < buffer && gathered < free
    }

    /// Frames until the next waiting callback resumes.
    pub(crate) fn divisi_due_in(&self) -> Option<usize> {
        self.players
            .waits
            .iter()
            .flatten()
            .map(|&(_, due)| due.saturating_sub(self.frames_rendered) as usize)
            .min()
    }

    /// Resume every waiting callback due now, in order: each advances the
    /// shared counter; one whose wait ends plays if it still owns the
    /// gather, else ends.
    pub(crate) fn fire_divisi(&mut self) {
        let now = self.frames_rendered;
        let tick = (ms_to_frames(1, self.sample_rate) as u64).max(1);
        while let Some(i) = self
            .players
            .waits
            .iter()
            .enumerate()
            .filter_map(|(i, w)| {
                w.filter(|&(_, due)| due <= now)
                    .map(|(id, due)| (due, id, i))
            })
            .min()
            .map(|(.., i)| i)
        {
            let Some((id, due)) = self.players.waits[i] else {
                break;
            };
            self.players.ticks += 1;
            if self.divisi_waiting() {
                self.players.waits[i] = Some((id, due + tick));
                continue;
            }
            self.players.waits[i] = None;
            if self.players.owner == Some(id) {
                self.divisi_play();
            }
        }
    }

    /// The owner plays the gather (25301-25355): with no free player the
    /// newest note steals the closest note's player(s), else the gathered
    /// notes fill the free players, lowest first, cycling.
    fn divisi_play(&mut self) {
        let Some(plan) = self.players.plan.take() else {
            self.players.owner = None;
            return;
        };
        let s = &self.players;
        let count = usize::from(s.count);
        let free = (0..count).filter(|&p| s.slot_strike[p].is_none()).count();
        let mut assign = [None; MAX_PLAYERS];
        if free == 0 {
            // DivisiNoteClosestToNote: first player with a strictly smaller
            // distance; then half its players (all of one)
            let last = i32::from(s.last);
            let mut best: Option<(i32, u8)> = None;
            for p in 0..count {
                if let Some(n) = s.slot_note[p] {
                    let d = (i32::from(n) - last).abs();
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, n));
                    }
                }
            }
            if let Some((_, target)) = best {
                let holding = (0..count)
                    .filter(|&p| s.slot_note[p] == Some(target))
                    .count();
                let mut take = if holding > 1 { holding / 2 } else { holding };
                for (p, a) in assign.iter_mut().enumerate().take(count) {
                    if s.slot_note[p] == Some(target) && take > 0 {
                        *a = Some(s.last);
                        take -= 1;
                    }
                }
            }
        } else {
            let (mut ni, mut slot) = (0usize, 0usize);
            if s.gathered.iter().any(|&g| g) {
                let mut slots: [Option<u8>; MAX_PLAYERS] = [None; MAX_PLAYERS];
                for _ in 0..count {
                    while !s.gathered[ni] {
                        ni = (ni + 1) % 128;
                    }
                    while s.slot_strike[slot].is_some() {
                        slot = (slot + 1) % MAX_PLAYERS;
                    }
                    slots[slot] = Some(ni as u8);
                    ni = (ni + 1) % 128;
                    slot = (slot + 1) % MAX_PLAYERS;
                }
                assign = slots;
            }
        }
        // only players below the count play (25356: `while p < $gl1sy`)
        for a in assign.iter_mut().skip(count) {
            *a = None;
        }
        let plan = ShortPlan {
            parent: self.players.last,
            ..plan
        };
        let keep = std::mem::replace(&mut self.articulation, plan.artic.clone());
        self.play_players(&plan, &assign, true);
        self.articulation = keep;
        self.players.owner = None;
    }

    /// A release with more than one player (36575-36583): every player's
    /// body of `note` fades.
    pub(crate) fn players_fade_bodies(&mut self, note: u8, fade: usize) {
        for p in 0..MAX_PLAYERS {
            if let Some(strike) = self.players.bodies[usize::from(note & 127)][p].take() {
                self.voices.fade_out_strike_bodies(strike, fade);
            }
        }
    }

    /// The release voices of `note` with more than one player (36616ff):
    /// stacked, each player plays the release at its own offset; divisi,
    /// each player holding `note` plays it (no offset humanize) and is
    /// freed. `artic`'s zone of `dynamic` at `rr`, extra players' keys inside
    /// `range`, `gain`, `start_ms` in.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn players_release(
        &mut self,
        parent: &str,
        artic: &str,
        dynamic: &str,
        note: u8,
        rr: usize,
        range: Option<(u8, u8)>,
        gain: f32,
        start_ms: f32,
    ) {
        let divisi = self.divisi_applies(parent);
        let count = usize::from(self.players.count);
        let mut assign = [None; MAX_PLAYERS];
        if divisi {
            for (p, a) in assign.iter_mut().enumerate() {
                if self.players.slot_note[p] == Some(note) {
                    *a = Some(note);
                    self.players.slot_note[p] = None;
                    self.players.slot_strike[p] = None;
                }
            }
        } else {
            for a in assign.iter_mut().take(count) {
                *a = Some(note);
            }
        }
        let n_rr = self.patch.spec.articulation(artic).map_or(1, |a| a.rr);
        let keys = self.player_keys(&assign, range, n_rr, rr);
        let hum = self.humanize_now();
        let spread = self.players_spec().map_or(0, |s| s.spread);
        let count = self.players.count;
        for p in 0..MAX_PLAYERS {
            let Some((e, key, rr)) = keys[p] else {
                continue;
            };
            let Some(idx) = self
                .find_layer_zone_rr_index(artic, dynamic, key, rr)
                .or_else(|| self.find_layer_zone(artic, "", dynamic, key, rr))
            else {
                continue;
            };
            let (cents, db, ms) = hum[p];
            let ms = if divisi {
                start_ms
            } else {
                start_ms + ms as f32
            };
            let offset = self.script_offset(idx, kontakt_offset_frames(ms, self.sample_rate));
            self.spawn_player = Some(PlayerMods {
                gain_db: db,
                pan: player_pan(p, count, spread) as f32 / 1000.0,
                cents,
                start_ms: 0,
                key: Some(key),
                parent: None,
            });
            self.spawn_zone_voice_at(
                idx,
                e,
                VoiceKind::Release,
                gain,
                None,
                cents / 100.0,
                offset,
            );
            self.spawn_player = None;
        }
    }

    /// A key-up frees the divisi players still holding its note (shorts:
    /// no release sample, 37578-37588).
    pub(crate) fn divisi_note_off(&mut self, note: u8) {
        for p in 0..MAX_PLAYERS {
            if self.players.slot_note[p] == Some(note) {
                self.players.slot_note[p] = None;
                self.players.slot_strike[p] = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{count_from_cc, neighbour_key, player_pan};

    #[test]
    fn count_bands_of_twelve() {
        let got: Vec<u8> = [0, 12, 13, 24, 25, 30, 50, 108, 109, 127]
            .iter()
            .map(|&v| count_from_cc(v, 12))
            .collect();
        assert_eq!(got, [1, 1, 2, 2, 3, 3, 5, 9, 10, 10]);
    }

    /// decode §3: mid-range shifts, and the range edges (KONTRA traces A, C, D).
    #[test]
    fn neighbour_keys() {
        let keys =
            |e, p, lo, hi| -> Vec<u8> { (1..p).map(|k| neighbour_key(e, k, lo, hi, p)).collect() };
        assert_eq!(keys(60, 2, 50, 87), [61]);
        assert_eq!(keys(60, 3, 50, 87), [59, 61]);
        assert_eq!(keys(60, 4, 50, 87), [59, 61, 62]);
        assert_eq!(keys(60, 5, 48, 81), [59, 58, 61, 62]);
        assert_eq!(keys(60, 10, 0, 127), [59, 58, 57, 56, 61, 62, 63, 64, 65]);
        assert_eq!(keys(81, 2, 48, 81), [80]);
        assert_eq!(keys(87, 3, 50, 87), [86, 85]);
        assert_eq!(keys(50, 3, 50, 87), [51, 52]);
    }

    #[test]
    fn pans() {
        let pans =
            |n: u8| -> Vec<i32> { (0..usize::from(n)).map(|p| player_pan(p, n, 500)).collect() };
        assert_eq!(pans(1), [0]);
        assert_eq!(pans(2), [-500, 500]);
        assert_eq!(pans(3), [-500, 500, -1000]);
        assert_eq!(pans(5), [-250, 250, -500, 500, -750]);
    }
}
