//! Timed phrases on held keys (`ArticulationSpec::phrase`, CSB Taka "Hold").
//!
//! A phrase key does not play one short. `delay_ms` after its note-on it
//! plays a first hit; every `tick_ms` while any phrase key is down each key
//! plays the next hit, fading its previous one; at the first tick after
//! every key is up each key plays an end hit. The hits are one-shots that
//! ring to their ends — the phrase's note-offs only end the phrase.
//!
//! One phrase runs at a time (the script's one loop, `$0dkf5`): a key
//! struck while it runs joins it at the next tick. Every random pick takes
//! the next forced draw first (`set_forced_rrs`), as an index into its
//! candidate list, so a null test can replay Kontakt's draws.

use super::{SampleEngine, ms_to_frames_f, next_zone_random};

/// Keys one phrase can carry (a chord's worth).
const MAX_KEYS: usize = 8;

/// The running phrase.
#[derive(Debug, Clone)]
pub(crate) struct PhraseRun {
    artic: String,
    /// Frame of the first hit, and the ticks after it so far.
    start: u64,
    ticks: u32,
    /// The RR the next loop hit plays (1-based; 0 = the second hit is
    /// still to be drawn).
    rr: u8,
    /// `(note, velocity, down, strike of its last hit)` per key.
    keys: [Option<(u8, u8, bool, Option<u32>)>; MAX_KEYS],
    first_done: bool,
}

impl PhraseRun {
    fn due(&self, tick_frames: f64) -> u64 {
        self.start + (f64::from(self.ticks) * tick_frames).round() as u64
    }
}

impl SampleEngine {
    /// The current articulation's phrase, if it has one.
    fn phrase_spec(&self, artic: &str) -> Option<crate::spec::PhraseSpec> {
        self.patch
            .spec
            .articulation(artic)
            .and_then(|a| a.phrase.clone())
    }

    /// Start (or join) a phrase for `note`. `false` when the articulation
    /// plays no phrase (the caller triggers the plain short).
    pub(crate) fn phrase_note_on(&mut self, note: u8, velocity: u8) -> bool {
        let artic = self.articulation.clone();
        let Some(spec) = self.phrase_spec(&artic) else {
            return false;
        };
        if let Some(run) = self.phrase.as_mut() {
            if run.artic == artic {
                if let Some(slot) = run
                    .keys
                    .iter_mut()
                    .find(|k| k.is_none_or(|(n, ..)| n == note))
                {
                    let last = slot.and_then(|k| k.3);
                    *slot = Some((note, velocity, true, last));
                }
                return true;
            }
        }
        let now = self.frames_rendered;
        let mut keys = [None; MAX_KEYS];
        keys[0] = Some((note, velocity, true, None));
        self.phrase = Some(PhraseRun {
            artic,
            start: now + ms_to_frames_f(spec.delay_ms, self.sample_rate) as u64,
            ticks: 0,
            rr: 0,
            keys,
            first_done: false,
        });
        true
    }

    /// Key-up of a phrase key: `true` when `note` belongs to the running
    /// phrase (its hits are not released).
    pub(crate) fn phrase_note_off(&mut self, note: u8) -> bool {
        let Some(run) = self.phrase.as_mut() else {
            return false;
        };
        let Some(k) = run.keys.iter_mut().flatten().find(|(n, ..)| *n == note) else {
            return false;
        };
        k.2 = false;
        true
    }

    /// Frames until the running phrase's next hit (the block loop splits
    /// there), or `None`.
    pub(crate) fn phrase_due_in(&self) -> Option<usize> {
        let run = self.phrase.as_ref()?;
        let spec = self.phrase_spec(&run.artic)?;
        let tick = f64::from(spec.tick_ms) * f64::from(self.sample_rate) / 1000.0;
        Some(run.due(tick).saturating_sub(self.frames_rendered) as usize)
    }

    /// Play every hit that is due.
    pub(crate) fn fire_phrase(&mut self) {
        let Some(mut run) = self.phrase.take() else {
            return;
        };
        let Some(spec) = self.phrase_spec(&run.artic) else {
            return;
        };
        let tick = f64::from(spec.tick_ms) * f64::from(self.sample_rate) / 1000.0;
        if run.due(tick) > self.frames_rendered {
            self.phrase = Some(run);
            return;
        }
        let fade = ms_to_frames_f(spec.fade_ms, self.sample_rate);
        if !run.first_done {
            // The first hit: one draw for every key, not the last phrase's
            // first; no fade (the previous phrase's end hit rings on).
            let first = self.phrase_pick(&spec.first_rr, Some(self.phrase_last_first));
            self.phrase_last_first = first;
            for k in run.keys.iter_mut().flatten() {
                k.3 = Some(self.phrase_hit(&run.artic, k.0, k.1, first, None));
            }
            run.first_done = true;
            run.ticks = 1;
            self.phrase = Some(run);
            return;
        }
        if run.rr == 0 {
            run.rr = self.phrase_pick(&spec.second_rr, None);
        }
        if run.keys.iter().flatten().any(|k| k.2) {
            // The loop: a held key plays the next hit, a released one its
            // end hit (and leaves the phrase).
            for slot in &mut run.keys {
                let Some((note, vel, down, last)) = *slot else {
                    continue;
                };
                let prev = last.map(|s| (s, fade));
                if down {
                    let s = self.phrase_hit(&run.artic, note, vel, run.rr, prev);
                    *slot = Some((note, vel, down, Some(s)));
                } else {
                    let end = self.phrase_pick(&spec.end_rr, None);
                    self.phrase_hit(&run.artic, note, vel, end, prev);
                    *slot = None;
                }
            }
            run.rr = if run.rr >= spec.step_wrap[1] {
                spec.step_wrap[0]
            } else {
                run.rr + 1
            };
            run.ticks += 1;
            self.phrase = Some(run);
            return;
        }
        // Every key is up: one end draw, every key plays it; the phrase ends.
        let end = self.phrase_pick(&spec.end_rr, None);
        for (note, vel, _, last) in run.keys.into_iter().flatten() {
            self.phrase_hit(&run.artic, note, vel, end, last.map(|s| (s, fade)));
        }
    }

    /// One of `list` (1-based RRs): the next forced draw as an index into
    /// it, else at random, never `avoid`.
    fn phrase_pick(&mut self, list: &[u8], avoid: Option<u8>) -> u8 {
        if list.is_empty() {
            return 1;
        }
        if let Some(f) = self.forced_rrs.pop_front() {
            return list[usize::from(f) % list.len()];
        }
        loop {
            let r = list[next_zone_random(&mut self.tune_rng) % list.len()];
            if list.len() == 1 || Some(r) != avoid {
                return r;
            }
        }
    }

    /// Play RR `rr` (1-based) of `artic` at `note`, through the short's own
    /// layer and level law, after fading `fade`'s `(strike, frames)` — the
    /// key's previous hit. Returns the new hit's strike.
    fn phrase_hit(
        &mut self,
        artic: &str,
        note: u8,
        velocity: u8,
        rr: u8,
        fade: Option<(u32, usize)>,
    ) -> u32 {
        if let Some((strike, frames)) = fade {
            self.voices.fade_strike(strike, frames);
        }
        self.voices.begin_strike();
        let strike = self.voices.current_strike();
        let keep_artic = std::mem::replace(&mut self.articulation, artic.to_string());
        self.phrase_rr = Some(usize::from(rr.saturating_sub(1)));
        self.trigger_zoned_short(note, velocity);
        self.phrase_rr = None;
        self.articulation = keep_artic;
        strike
    }
}
