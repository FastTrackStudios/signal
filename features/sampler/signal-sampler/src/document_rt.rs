//! Document mode — the REALTIME transport-driven schedule walker (phase 2,
//! see `docs/plan/document-mode.md`).
//!
//! [`RealtimeScheduler`] is the same walk as the offline
//! [`render_schedule`](crate::document::render_schedule) walker, driven by
//! the HOST transport instead of a loop: each audio block covers the window
//! `[playhead, playhead + block_frames)` in **absolute frames from the
//! document epoch** (= project time zero), and every scheduled event whose
//! frame falls inside the window is dispatched at its exact in-block offset —
//! including `LegatoPrefire`s ahead of their destination ticks.
//!
//! ## Playback-start invariance (the determinism contract)
//!
//! Starting playback at ANY position `P` — first play, seek, loop wrap —
//! produces the offline full render sliced at `P`, **bit-exactly** (asserted
//! by `signal-sampler-clap/tests/host_sim.rs` for adversarial `P`s: inside a
//! legato transition window, mid-sustain, inside a release tail, exactly on
//! prefire/arrival frames). WHERE you press play never changes WHAT you
//! hear.
//!
//! This is achieved by **voice reconstruction via bounded deterministic
//! replay** ([`Schedule::reconstruction_start`]): on any discontinuity the
//! scheduler kills all voices, pre-rolls controller state (CC events from
//! the provably-quiescent past), then replays the schedule from the start of
//! the continuous activity span containing `P` — through the real render
//! path, audio discarded — and resumes audibly at `P`. Replay is the only
//! mechanism that can be bit-exact: voice state (fractional playback
//! position, recursive gain ramps, loop wraps) is accumulated per rendered
//! frame; a closed-form "spawn at offset n with envelope advanced" would
//! round differently. Replayed trigger events are counted in
//! [`RealtimeScheduler::reconstructed_voices`].
//!
//! **Cost**: the replay renders (and discards) up to one activity span on
//! the audio thread at the seek block — bounded by the longest continuous
//! (non-quiescent) stretch of the piece, worst case the piece itself for
//! unbroken material. This is the deliberate v2 trade: exactness first;
//! moving reconstruction off-thread behind a short crossfade is a future
//! optimization if seek stalls matter in practice.
//!
//! ## Transport / tempo mapping policy
//!
//! The schedule's frames were baked from the DOCUMENT tempo map by
//! [`annotate`](crate::document::annotate). The host playhead
//! ([`BlockTransport::pos_frame`], the song position in samples) is trusted
//! as-is — **REAPER is the tempo authority**. If the host-reported tempo at
//! the current position disagrees with the document tempo map, the two
//! timelines have diverged (the document is stale); we log a warning ONCE
//! and keep following the host playhead. The fix is upstream: rebuild the
//! document from the host's tempo map (the phase-3 self-sourced document
//! does this automatically).
//!
//! ## Mode arbitration (block boundaries only)
//!
//! `transport playing && schedule present` ⇒ document mode owns the engine
//! (Lookahead + expressive legato). Anything else ⇒ StrictLive: the caller
//! (the CLAP plugin) dispatches incoming live MIDI through the normal bank
//! path. Transitions happen exclusively at block boundaries: entering kills
//! live voices and reconstructs at the playhead; leaving releases scheduled
//! notes (`all_notes_off`) and restores [`PlayMode::StrictLive`]. While a
//! document is playing, incoming live MIDI is IGNORED by the plugin for
//! phase 2 (overdub arbitration is phase 3+).
//!
//! ## Threading
//!
//! Everything here runs on the audio thread and only WALKS: schedule
//! building (`annotate`) happens off-thread and arrives as a pre-built
//! `Arc<Schedule>` via [`RealtimeScheduler::set_schedule`] (the plugin swaps
//! it in at a block boundary). The steady walk path performs no allocation
//! (the reconstruction scratch is pre-allocated in `new`; known engine-side
//! exception, pre-existing: some trigger paths build short strings).

use std::sync::Arc;

use crate::bank::SamplerBank;
use crate::document::{DocEvent, Schedule, TempoPoint, dispatch_event, is_trigger};
use crate::engine::PlayMode;

/// One block's host-transport snapshot (from the CLAP process context, or a
/// fake host in tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTransport {
    /// Transport rolling?
    pub playing: bool,
    /// Song position of the FIRST frame of this block, in samples from
    /// project time zero (= the document epoch). May be negative (count-in).
    pub pos_frame: i64,
    /// Host-reported tempo at this block, if available (BPM). Only used for
    /// the stale-document diagnostic — the playhead is the authority.
    pub tempo_bpm: Option<f64>,
}

/// Reconstruction replay chunk size (frames). Chunking is deterministic
/// (engine output is chunk-size invariant), so this is purely a scratch
/// sizing choice.
const WARM_CHUNK: usize = 512;

/// Realtime schedule walker for ONE bank instrument. See the module docs.
pub struct RealtimeScheduler {
    /// Bank instrument this scheduler drives.
    id: String,
    schedule: Option<Arc<Schedule>>,
    /// Index of the next un-dispatched schedule event.
    cursor: usize,
    /// Playhead frame the next block must start at to be contiguous.
    /// `None` forces a relocate.
    expect_frame: Option<i64>,
    /// Document mode currently owns the engine.
    active: bool,
    /// Trigger events replayed (audio discarded) by reconstruction.
    reconstructed_voices: u64,
    /// Pre-allocated discard buffer for reconstruction replay.
    warm: Vec<f32>,
    tempo_warned: bool,
}

impl RealtimeScheduler {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            schedule: None,
            cursor: 0,
            expect_frame: None,
            active: false,
            reconstructed_voices: 0,
            warm: vec![0.0; WARM_CHUNK * 2],
            tempo_warned: false,
        }
    }

    /// Swap the schedule (block boundary; `None` clears document mode).
    /// A changed schedule mid-playback is treated as a discontinuity: the
    /// next block reconstructs at the playhead inside the new schedule.
    pub fn set_schedule(&mut self, schedule: Option<Arc<Schedule>>) {
        let same = match (&self.schedule, &schedule) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.schedule = schedule;
        self.expect_frame = None; // force relocate (or clean exit) next block
        self.tempo_warned = false;
    }

    /// Whether document mode owned the engine after the last block.
    pub fn document_active(&self) -> bool {
        self.active
    }

    /// Trigger events replayed (audio discarded) so far to reconstruct the
    /// voices alive across playback starts/seeks.
    pub fn reconstructed_voices(&self) -> u64 {
        self.reconstructed_voices
    }

    /// Process one audio block. `out` is interleaved stereo (len =
    /// `frames * 2`) and is cleared first.
    ///
    /// Returns `true` when document mode consumed the block (the schedule
    /// was walked and `out` rendered). Returns `false` when the engine is in
    /// StrictLive for this block — `out` is left CLEARED and untouched; the
    /// caller dispatches live MIDI and renders. Mode transitions (including
    /// the release/reconstruction bookkeeping) happen inside this call, at
    /// the block boundary only.
    pub fn process_block(
        &mut self,
        bank: &mut SamplerBank,
        t: &BlockTransport,
        out: &mut [f32],
    ) -> bool {
        out.fill(0.0);
        let frames = out.len() / 2;

        let want = t.playing && self.schedule.is_some();
        if want != self.active {
            if want {
                // Enter document mode: relocate() below reconstructs.
                self.active = true;
                self.expect_frame = None;
            } else {
                self.exit(bank);
            }
        }
        if !self.active {
            return false;
        }

        let sched = self.schedule.clone().expect("active implies schedule");
        if self.expect_frame != Some(t.pos_frame) {
            self.relocate(bank, &sched, t.pos_frame);
        }
        self.check_tempo(&sched, t);

        let start = t.pos_frame;
        let end = start + frames as i64;
        let mut off = 0usize; // frames of this block already rendered

        while let Some(ev) = sched.events.get(self.cursor) {
            let nf = ev.frame as i64;
            if nf >= end {
                break;
            }
            // Render up to the event's exact in-block offset. (Events whose
            // frame precedes the block — count-in — fire at offset 0.)
            let target = (nf.max(start) - start) as usize;
            if target > off {
                bank.render(&mut out[off * 2..target * 2]);
                off = target;
            }
            let ev = *ev;
            self.cursor += 1;
            dispatch_event(bank, &self.id, &ev);
        }
        if frames > off {
            bank.render(&mut out[off * 2..frames * 2]);
        }
        self.expect_frame = Some(end);
        true
    }

    /// Discontinuity handling — see the module docs' "Playback-start
    /// invariance": kill everything, pre-roll controller state, then
    /// deterministically replay the activity span containing `pos_frame`
    /// (audio discarded) so every voice the full render would have alive
    /// here exists with bit-exact state.
    fn relocate(&mut self, bank: &mut SamplerBank, sched: &Schedule, pos_frame: i64) {
        bank.panic(&self.id);
        // Document playback always runs the full expressive legato — that is
        // the whole point of lookahead. This also flips the engine into
        // PlayMode::Lookahead.
        bank.set_legato_mode(&self.id, true, true);

        let pos = pos_frame.max(0) as u64;
        let recon_from = sched.reconstruction_start(pos);
        let mut warm_cursor = recon_from;
        self.cursor = sched.events.len();
        for (i, ev) in sched.events.iter().enumerate() {
            if ev.frame >= pos {
                self.cursor = i;
                break;
            }
            if ev.frame < recon_from {
                // Provably-quiescent past: controller state only (voices
                // from before the span are dead by construction).
                if matches!(ev.kind, DocEvent::Cc { .. }) {
                    dispatch_event(bank, &self.id, ev);
                }
                continue;
            }
            // Reconstruction replay — identical walk to the offline warm-up.
            while warm_cursor < ev.frame {
                let n = ((ev.frame - warm_cursor) as usize).min(WARM_CHUNK);
                self.warm[..n * 2].fill(0.0);
                bank.render(&mut self.warm[..n * 2]);
                warm_cursor += n as u64;
            }
            if is_trigger(ev) {
                self.reconstructed_voices += 1;
            }
            dispatch_event(bank, &self.id, ev);
        }
        // Advance the remainder of the span up to the playhead itself.
        while warm_cursor < pos {
            let n = ((pos - warm_cursor) as usize).min(WARM_CHUNK);
            self.warm[..n * 2].fill(0.0);
            bank.render(&mut self.warm[..n * 2]);
            warm_cursor += n as u64;
        }
    }

    /// Leave document mode: release scheduled notes and hand the engine back
    /// to the strict zero-latency live policy.
    fn exit(&mut self, bank: &mut SamplerBank) {
        bank.set_forced_rr(&self.id, None);
        bank.all_notes_off(&self.id);
        bank.set_play_mode(&self.id, PlayMode::StrictLive);
        self.active = false;
        self.expect_frame = None;
    }

    /// Stale-document diagnostic: compare the host tempo against the
    /// document tempo map at the current position; warn ONCE on divergence
    /// and keep trusting the host playhead (see module docs).
    fn check_tempo(&mut self, sched: &Schedule, t: &BlockTransport) {
        if self.tempo_warned {
            return;
        }
        let Some(host_bpm) = t.tempo_bpm else { return };
        let sec = t.pos_frame.max(0) as f64 / sched.sample_rate as f64;
        let doc_bpm = bpm_at_sec(&sched.tempo, sec);
        if (host_bpm - doc_bpm).abs() > 0.01 {
            self.tempo_warned = true;
            tracing::warn!(
                host_bpm,
                doc_bpm,
                pos_sec = sec,
                "document tempo map diverges from host tempo — following the \
                 HOST playhead (REAPER is the tempo authority); rebuild the \
                 document from the host tempo map to realign scheduled frames"
            );
        }
    }
}

/// BPM of the piecewise-constant document tempo map at `sec` seconds from
/// the document epoch (inverse-integration counterpart of
/// [`qn_to_sec`](crate::document::qn_to_sec)).
fn bpm_at_sec(tempo: &[TempoPoint], sec: f64) -> f64 {
    let mut bpm = tempo.first().map(|t| t.bpm).unwrap_or(120.0);
    let mut cur_sec = 0.0;
    let mut cur_qn = 0.0;
    for t in tempo {
        let seg_sec = (t.qn - cur_qn).max(0.0) * 60.0 / bpm;
        if cur_sec + seg_sec > sec {
            return bpm;
        }
        cur_sec += seg_sec;
        cur_qn = t.qn.max(cur_qn);
        bpm = t.bpm;
    }
    bpm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_at_sec_follows_the_map() {
        let map = vec![
            TempoPoint {
                qn: 0.0,
                bpm: 120.0,
            },
            TempoPoint { qn: 4.0, bpm: 60.0 }, // switch at 2.0 s
        ];
        assert_eq!(bpm_at_sec(&map, 0.0), 120.0);
        assert_eq!(bpm_at_sec(&map, 1.9), 120.0);
        assert_eq!(bpm_at_sec(&map, 2.1), 60.0);
        assert_eq!(bpm_at_sec(&[], 5.0), 120.0);
    }
}
