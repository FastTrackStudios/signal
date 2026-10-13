//! MIDI editing service — notes, CCs, pitch bends, program changes,
//! SysEx in MIDI takes.
//!
//! For realtime MIDI I/O see `LiveMidi`.

use super::{
    MidiCC, MidiChannelPressure, MidiNote, MidiNoteCreate, MidiNoteExpression, MidiPitchBend,
    MidiPolyPressure, MidiProgramChange, MidiSysEx, NoteExpressionDim,
};
use crate::TrackRef;
use crate::item::{ItemRef, TakeRef};
use crate::project::ProjectContext;
use facet::Facet;

/// Location of a MIDI take (project + item + take).
#[derive(Clone, Debug, Facet)]
pub struct MidiTakeLocation {
    pub project: ProjectContext,
    pub item: ItemRef,
    pub take: TakeRef,
}

impl MidiTakeLocation {
    pub fn new(project: ProjectContext, item: ItemRef, take: TakeRef) -> Self {
        Self {
            project,
            item,
            take,
        }
    }

    /// For the active take of an item.
    pub fn active(project: ProjectContext, item: ItemRef) -> Self {
        Self::new(project, item, TakeRef::Active)
    }
}

/// PPQ range for queries.
#[derive(Clone, Debug, Facet)]
pub struct PpqRange {
    pub start: f64,
    pub end: f64,
}

impl PpqRange {
    pub fn new(start: f64, end: f64) -> Self {
        Self { start, end }
    }
}

/// Quantize parameters.
#[derive(Clone, Debug, Facet)]
pub struct QuantizeParams {
    /// Empty = selected notes.
    pub indices: Vec<u32>,
    /// Grid size in PPQ (1.0 = quarter note).
    pub grid_ppq: f64,
    /// Strength (0.0 = no change, 1.0 = full snap).
    pub strength: f64,
}

/// Humanize parameters.
#[derive(Clone, Debug, Facet)]
pub struct HumanizeParams {
    /// Empty = selected notes.
    pub indices: Vec<u32>,
    /// Random timing variation range in PPQ.
    pub timing_range_ppq: f64,
    /// Random velocity variation range.
    pub velocity_range: u8,
}

/// CC event create parameters.
#[derive(Clone, Debug, Facet)]
pub struct MidiCCCreate {
    pub channel: u8,
    pub controller: u8,
    pub value: u8,
    pub position_ppq: f64,
}

impl MidiCCCreate {
    pub fn new(channel: u8, controller: u8, value: u8, position_ppq: f64) -> Self {
        Self {
            channel: channel & 0x0F,
            controller: controller & 0x7F,
            value: value & 0x7F,
            position_ppq,
        }
    }
}

/// Pitch bend event create parameters.
#[derive(Clone, Debug, Facet)]
pub struct MidiPitchBendCreate {
    pub channel: u8,
    /// Pitch bend value (-8192 to 8191).
    pub value: i16,
    pub position_ppq: f64,
}

/// Parameters for creating a new Program Change event.
#[derive(Clone, Debug, Facet)]
pub struct MidiProgramChangeCreate {
    pub channel: u8,
    pub program: u8,
    pub position_ppq: f64,
}

impl MidiProgramChangeCreate {
    pub fn new(channel: u8, program: u8, position_ppq: f64) -> Self {
        Self {
            channel: channel & 0x0F,
            program: program & 0x7F,
            position_ppq,
        }
    }
}

/// Parameters for creating a new System Exclusive event. The `data`
/// vector should include the leading `0xF0` and trailing `0xF7`
/// framing bytes — the standalone impl stores them verbatim and the
/// renderer hands them straight to the plugin's MIDI input port.
#[derive(Clone, Debug, Facet)]
pub struct MidiSysExCreate {
    pub data: Vec<u8>,
    pub position_ppq: f64,
}

impl MidiSysExCreate {
    pub fn new(data: Vec<u8>, position_ppq: f64) -> Self {
        Self { data, position_ppq }
    }
}

/// Parameters for creating a channel-pressure (mono aftertouch) event.
#[derive(Clone, Debug, Facet)]
pub struct MidiChannelPressureCreate {
    pub channel: u8,
    pub pressure: u8,
    pub position_ppq: f64,
}

impl MidiChannelPressureCreate {
    pub fn new(channel: u8, pressure: u8, position_ppq: f64) -> Self {
        Self {
            channel: channel & 0x0F,
            pressure: pressure & 0x7F,
            position_ppq,
        }
    }
}

/// Parameters for creating a poly-pressure (per-note aftertouch) event.
#[derive(Clone, Debug, Facet)]
pub struct MidiPolyPressureCreate {
    pub channel: u8,
    pub note: u8,
    pub pressure: u8,
    pub position_ppq: f64,
}

impl MidiPolyPressureCreate {
    pub fn new(channel: u8, note: u8, pressure: u8, position_ppq: f64) -> Self {
        Self {
            channel: channel & 0x0F,
            note: note & 0x7F,
            pressure: pressure & 0x7F,
            position_ppq,
        }
    }
}

/// Parameters for creating a per-note expression event.
#[derive(Clone, Debug, Facet)]
pub struct MidiNoteExpressionCreate {
    pub channel: u8,
    /// Target note (pitch). `0xFF` = "any note on this channel".
    pub note: u8,
    pub dimension: NoteExpressionDim,
    pub value: f64,
    pub position_ppq: f64,
}

impl MidiNoteExpressionCreate {
    pub fn new(
        channel: u8,
        note: u8,
        dimension: NoteExpressionDim,
        value: f64,
        position_ppq: f64,
    ) -> Self {
        Self {
            channel: channel & 0x0F,
            note: if note == 0xFF { 0xFF } else { note & 0x7F },
            dimension,
            value,
            position_ppq,
        }
    }
}

impl MidiPitchBendCreate {
    pub fn new(channel: u8, value: i16, position_ppq: f64) -> Self {
        Self {
            channel: channel & 0x0F,
            value: value.clamp(-8192, 8191),
            position_ppq,
        }
    }
}

/// Everything in a take, in one round trip.
///
/// Per-index accessors are fine for a tweak but unusable for an editor:
/// loading a 500-note take through `notes()` plus a `ccs()` call per
/// controller plus `pitch_bends()` is a request storm, and writing an
/// edit back one setter at a time is worse. A surface that wants to
/// *own* a take reads one of these and writes one back.
#[derive(Clone, Debug, Facet, Default)]
pub struct MidiTakeSnapshot {
    pub notes: Vec<MidiNote>,
    pub ccs: Vec<MidiCC>,
    pub pitch_bends: Vec<MidiPitchBend>,
    pub channel_pressures: Vec<MidiChannelPressure>,
    pub poly_pressures: Vec<MidiPolyPressure>,
    pub note_expressions: Vec<MidiNoteExpression>,
    /// Ticks per quarter note the positions are expressed in.
    pub ppq: f64,
    /// Take length in PPQ.
    pub length_ppq: f64,
}

/// A take's contents to write, without indices.
///
/// Separate from [`MidiTakeSnapshot`] because a write has no meaningful
/// indices to supply — they are assigned by the backend — and reusing
/// the read type would invite round-tripping stale ones.
#[derive(Clone, Debug, Facet, Default)]
pub struct MidiTakeContent {
    pub notes: Vec<MidiNoteCreate>,
    pub ccs: Vec<MidiCCCreate>,
    pub pitch_bends: Vec<MidiPitchBendCreate>,
    pub note_expressions: Vec<MidiNoteExpressionCreate>,
    /// Channel pressure (mono aftertouch).
    ///
    /// Added for #167: without it, an MPE editor had to write pressure
    /// out as CC11 while reading it back from real channel pressure, so
    /// the dimension silently did not survive a round trip. A field per
    /// event kind the snapshot already reads is the shape that makes
    /// read and write symmetrical.
    #[facet(default)]
    pub channel_pressures: Vec<MidiChannelPressureCreate>,
}

/// How a write treats what is already in the take.
#[derive(Clone, Copy, Debug, Facet, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum WriteMode {
    /// Replace the take's contents entirely.
    #[default]
    Replace,
    /// Add to what is there.
    Merge,
    /// Replace only within the written span, leaving the rest — the
    /// mode an editor working on a selection wants.
    ReplaceRange,
}

#[architect::rpc]
pub trait Midi {
    // ── Bulk take access ───────────────────────────────────────────

    /// Read everything in a take in one call.
    fn read_take(&self, location: MidiTakeLocation) -> MidiTakeSnapshot;

    /// Write a take's contents in one call, as a single undo point.
    ///
    /// Returns the notes' new indices, in the order supplied.
    fn write_take(
        &self,
        location: MidiTakeLocation,
        content: MidiTakeContent,
        mode: WriteMode,
    ) -> Vec<u32>;

    /// Replace a PPQ span, leaving everything outside it untouched.
    fn replace_range(
        &self,
        location: MidiTakeLocation,
        range: PpqRange,
        content: MidiTakeContent,
    ) -> Vec<u32>;

    /// Read a standard MIDI file into a take snapshot.
    ///
    /// Goes through the DAW API rather than a separate file path so a
    /// caller reads a `.mid` and a live take through one surface, and
    /// gets the same types back either way.
    fn read_midi_file(&self, path: String, track_index: u32) -> Option<MidiTakeSnapshot>;

    /// Write a snapshot out as a standard MIDI file.
    fn write_midi_file(&self, path: String, content: MidiTakeContent, ppq: f64) -> bool;

    // ── Notes ──────────────────────────────────────────────────────

    fn notes(&self, location: MidiTakeLocation) -> Vec<MidiNote>;

    fn notes_in_range(&self, location: MidiTakeLocation, range: PpqRange) -> Vec<MidiNote>;

    fn selected_notes(&self, location: MidiTakeLocation) -> Vec<MidiNote>;
    fn note_count(&self, location: MidiTakeLocation) -> u32;

    /// Create a new empty MIDI item on a track. Returns the take
    /// location of the new item's active take.
    fn create_midi_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Option<MidiTakeLocation>;

    /// Add a note. Returns the note index.
    fn add_note(&self, location: MidiTakeLocation, note: MidiNoteCreate) -> u32;

    fn add_notes(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32>;

    /// Add notes positioned in **raw take PPQ** — the same units
    /// [`Midi::notes`] hands back.
    ///
    /// [`Midi::add_notes`] does *not* round-trip with [`Midi::notes`]:
    /// it reads `start_ppq` as a project *quarter-note* position (the
    /// REAPER backend runs it through `MIDI_GetPPQPosFromProjQN`), which
    /// suits callers placing notes at musical positions they computed
    /// themselves, and silently misplaces notes for anyone echoing back
    /// what they just read. Use this when your positions came out of
    /// `notes()`; use `add_notes` when they came out of a tempo map.
    fn add_notes_ppq(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32>;
    fn delete_note(&self, location: MidiTakeLocation, index: u32);
    fn delete_notes(&self, location: MidiTakeLocation, indices: Vec<u32>);
    fn delete_selected_notes(&self, location: MidiTakeLocation);

    fn set_note_pitch(&self, location: MidiTakeLocation, index: u32, pitch: u8);
    fn set_note_velocity(&self, location: MidiTakeLocation, index: u32, velocity: u8);
    fn set_note_position(&self, location: MidiTakeLocation, index: u32, start_ppq: f64);
    fn set_note_length(&self, location: MidiTakeLocation, index: u32, length_ppq: f64);
    fn set_note_channel(&self, location: MidiTakeLocation, index: u32, channel: u8);
    fn set_note_selected(&self, location: MidiTakeLocation, index: u32, selected: bool);
    fn set_note_muted(&self, location: MidiTakeLocation, index: u32, muted: bool);

    // ── Batch ops ──────────────────────────────────────────────────

    fn select_all_notes(&self, location: MidiTakeLocation, selected: bool);
    fn transpose_notes(&self, location: MidiTakeLocation, indices: Vec<u32>, semitones: i8);
    fn quantize_notes(&self, location: MidiTakeLocation, params: QuantizeParams);
    fn humanize_notes(&self, location: MidiTakeLocation, params: HumanizeParams);

    // ── CCs ────────────────────────────────────────────────────────

    fn ccs(&self, location: MidiTakeLocation, controller: Option<u8>) -> Vec<MidiCC>;
    fn add_cc(&self, location: MidiTakeLocation, cc: MidiCCCreate) -> u32;
    fn delete_cc(&self, location: MidiTakeLocation, index: u32);
    fn set_cc_value(&self, location: MidiTakeLocation, index: u32, value: u8);

    // ── Other event types ─────────────────────────────────────────

    fn pitch_bends(&self, location: MidiTakeLocation) -> Vec<MidiPitchBend>;
    fn add_pitch_bend(&self, location: MidiTakeLocation, pb: MidiPitchBendCreate) -> u32;
    fn delete_pitch_bend(&self, location: MidiTakeLocation, index: u32);
    fn set_pitch_bend_value(&self, location: MidiTakeLocation, index: u32, value: i16);

    fn program_changes(&self, location: MidiTakeLocation) -> Vec<MidiProgramChange>;
    fn add_program_change(&self, location: MidiTakeLocation, pc: MidiProgramChangeCreate) -> u32;
    fn delete_program_change(&self, location: MidiTakeLocation, index: u32);
    fn set_program(&self, location: MidiTakeLocation, index: u32, program: u8);

    fn sysex(&self, location: MidiTakeLocation) -> Vec<MidiSysEx>;
    fn add_sysex(&self, location: MidiTakeLocation, sysex: MidiSysExCreate) -> u32;
    fn delete_sysex(&self, location: MidiTakeLocation, index: u32);

    // ── Channel + poly aftertouch ─────────────────────────────────

    fn channel_pressures(&self, location: MidiTakeLocation) -> Vec<MidiChannelPressure>;
    fn add_channel_pressure(
        &self,
        location: MidiTakeLocation,
        cp: MidiChannelPressureCreate,
    ) -> u32;
    fn delete_channel_pressure(&self, location: MidiTakeLocation, index: u32);
    fn set_channel_pressure_value(&self, location: MidiTakeLocation, index: u32, pressure: u8);

    fn poly_pressures(&self, location: MidiTakeLocation) -> Vec<MidiPolyPressure>;
    fn add_poly_pressure(&self, location: MidiTakeLocation, pp: MidiPolyPressureCreate) -> u32;
    fn delete_poly_pressure(&self, location: MidiTakeLocation, index: u32);
    fn set_poly_pressure_value(&self, location: MidiTakeLocation, index: u32, pressure: u8);

    // ── Per-note expression (MPE / CLAP / VST3) ────────────────────

    fn note_expressions(&self, location: MidiTakeLocation) -> Vec<MidiNoteExpression>;
    fn add_note_expression(&self, location: MidiTakeLocation, ne: MidiNoteExpressionCreate) -> u32;
    fn delete_note_expression(&self, location: MidiTakeLocation, index: u32);
    fn set_note_expression_value(&self, location: MidiTakeLocation, index: u32, value: f64);
}
