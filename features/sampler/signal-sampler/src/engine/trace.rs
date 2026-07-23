//! Render trace — a structured, queryable log of what the engine actually did
//! during a render: which sample files it played, at what frame, at what gain
//! and pitch, where their loop points are, and every note-off / legato
//! transition. Off by default (zero cost); enable with
//! [`SampleEngine::set_trace_enabled`](super::SampleEngine::set_trace_enabled).
//!
//! This is the ground truth for debugging "what is actually going on" — the QA
//! harness and the `trace_dump` example consume it, and it answers questions a
//! waveform can't: *which* take (RR/dynamic/mic) sounded, whether a voice
//! looped and where, and how voices overlap at a transition.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// One recorded render event, timestamped in engine frames.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceEvent {
    /// Engine frame (since construction) at which this happened.
    pub frame: u64,
    /// Mono legato line the event belongs to.
    pub line: u8,
    pub kind: TraceKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TraceKind {
    /// A sample voice started playing.
    VoiceSpawn(VoiceSpawn),
    /// A note-off arrived for `note` (sustain fades; a release tail may follow).
    NoteOff { note: u8 },
    /// A legato transition fired: `from → to` (`from == to` = re-bow).
    Transition { from: u8, to: u8, portamento: bool },
    /// A requested voice produced NO sound: either no sample matched the
    /// (articulation, dynamic, note, rr) lookup, or the matched sample wasn't
    /// loaded into the cache yet. This is the ground truth behind "I only hear
    /// the release / a click / nothing" — a body that never sounded.
    SampleMiss {
        note: u8,
        articulation: String,
        dynamic: String,
        rr: usize,
        reason: MissReason,
    },
    /// A traced voice finished (played out, faded to done, or was stolen).
    /// Frame-stamped by the per-block sweep, so the timestamp is accurate to
    /// one render block. Correlate with [`VoiceSpawn::voice_id`] for spans.
    VoiceEnd { voice_id: u64 },
}

/// Why a [`TraceKind::SampleMiss`] fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissReason {
    /// No sample matched the (section, articulation, mic, note, dynamic, rr) key.
    NoSample,
    /// A sample matched but wasn't decoded into the cache yet (streaming preload
    /// hasn't reached it) — the audio thread skips it silently.
    NotLoaded,
}

/// Everything about a spawned voice needed to reason about it after the fact.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSpawn {
    /// Monotonic id, unique within a render (correlate with future end events).
    pub voice_id: u64,
    /// The `VoiceKind` name (`"Legato"`, `"SustainLayer"`, `"Release"`, …).
    pub voice_kind: &'static str,
    /// Sample file that plays (relative path within the library).
    pub file: String,
    /// MIDI note this voice sounds.
    pub note: u8,
    /// Zone root key; `note` is pitched from here by `rate`.
    pub root_key: u8,
    /// Playback rate (1.0 = original pitch).
    pub rate: f64,
    /// Spawn gain (post makeup / dynamic / crossfade scaling).
    pub gain: f32,
    /// Dynamic layer label (`"p"`/`"mf"`/`"ff"`/…), empty if none.
    pub dynamic: String,
    /// Articulation id (`"NVsus"`, `"NVLeg"`, `"NVLegzero"`, `"NVrel"`, …).
    pub articulation: String,
    /// Mic/section label.
    pub mic: String,
    /// Legato direction (`"up"`/`"down"`) / empty for non-directional.
    pub direction: String,
    /// Legato interval in semitones (0 = not a transition zone).
    pub interval: u32,
    /// Round-robin take index chosen.
    pub rr: usize,
    /// First frame of the sample this voice plays from (after `start_offset`).
    pub start_frame: usize,
    /// Loop window `[loop_start, loop_end)` in sample frames — `loop_end == 0`
    /// means no loop (plays once to the end).
    pub loop_start: usize,
    pub loop_end: usize,
    /// Loop crossfade length in frames (0 = hard wrap / no loop).
    pub loop_xfade: usize,
}

impl VoiceSpawn {
    /// True when this voice loops (holds indefinitely).
    pub fn loops(&self) -> bool {
        self.loop_end > self.loop_start
    }
}

/// Collected render trace with simple query helpers. Frames are engine-relative
/// as recorded; document renders re-base them to the audible window.
#[derive(Clone, Debug, Default)]
pub struct RenderTrace {
    pub events: Vec<TraceEvent>,
}

impl RenderTrace {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// All voice spawns in `[from, to)` frames, in order.
    pub fn spawns_in(&self, from: u64, to: u64) -> Vec<(&TraceEvent, &VoiceSpawn)> {
        self.events
            .iter()
            .filter(|e| e.frame >= from && e.frame < to)
            .filter_map(|e| match &e.kind {
                TraceKind::VoiceSpawn(v) => Some((e, v)),
                _ => None,
            })
            .collect()
    }

    /// Every spawn of a sample whose file path contains `needle`.
    pub fn spawns_of_file(&self, needle: &str) -> Vec<(&TraceEvent, &VoiceSpawn)> {
        self.events
            .iter()
            .filter_map(|e| match &e.kind {
                TraceKind::VoiceSpawn(v) if v.file.contains(needle) => Some((e, v)),
                _ => None,
            })
            .collect()
    }

    /// Every voice spawn of `note`, in order.
    pub fn spawns_of_note(&self, note: u8) -> Vec<&VoiceSpawn> {
        self.events
            .iter()
            .filter_map(|e| match &e.kind {
                TraceKind::VoiceSpawn(v) if v.note == note => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Every sample-miss recorded, in order.
    pub fn misses(&self) -> Vec<(&TraceEvent, u8, MissReason)> {
        self.events
            .iter()
            .filter_map(|e| match &e.kind {
                TraceKind::SampleMiss { note, reason, .. } => Some((e, *note, *reason)),
                _ => None,
            })
            .collect()
    }

    /// How many voices are simultaneously sounding `note` at `frame`, counting
    /// only spawns before `frame` (a coarse overlap probe — no end tracking
    /// yet, so it over-counts once release tails are added; useful for spotting
    /// doubling right after a transition).
    pub fn voices_on_note_at(&self, note: u8, frame: u64, window: u64) -> usize {
        self.events
            .iter()
            .filter(|e| e.frame + window >= frame && e.frame <= frame)
            .filter(|e| matches!(&e.kind, TraceKind::VoiceSpawn(v) if v.note == note))
            .count()
    }
}
