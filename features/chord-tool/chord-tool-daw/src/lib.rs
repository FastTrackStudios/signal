//! Writes chord-tool's chords into a DAW project.
//!
//! The other half of [`chord_tool::ChordSink`]: the panel decides *what*,
//! this decides *where*. Split so the panel can run in a desktop window
//! with no DAW behind it — the standalone example provides
//! `chord_tool::LogSink` instead of this.
//!
//! Placement follows `ChordGun`: a chord lands at the edit cursor on the
//! selected track, and the cursor advances past it so repeated inserts
//! lay out a progression left to right.

use std::sync::Mutex;

use daw::service::{
    Items, Midi, MidiNoteCreate, PositionConversion, PositionInSeconds, ProjectContext, Tracks,
    transport::service::Transport,
};

/// Ticks per quarter note. REAPER's default MIDI resolution, and the unit
/// `Midi::add_notes` takes for note *length* (position is quarter notes —
/// see `daw_reaper::midi`).
const PPQ: f64 = 960.0;

/// Default note velocity. Middle-of-the-road so an inserted chord is
/// audible without dominating whatever it lands next to.
const VELOCITY: u8 = 96;

/// A [`chord_tool::ChordSink`] backed by a live DAW.
pub struct DawChordSink<D> {
    daw: D,
    /// Preview state, so a second preview can silence the first. Behind a
    /// Mutex because the sink is shared immutably across the UI.
    playing: Mutex<Vec<u8>>,
}

impl<D> DawChordSink<D> {
    pub const fn new(daw: D) -> Self {
        Self {
            daw,
            playing: Mutex::new(Vec::new()),
        }
    }
}

/// What writing a chord needs from a backend.
pub trait ChordDaw:
    Tracks + Items + Midi + Transport + PositionConversion + Send + Sync + 'static
{
}

impl<T> ChordDaw for T where
    T: Tracks + Items + Midi + Transport + PositionConversion + Send + Sync + 'static
{
}

impl<D: ChordDaw> DawChordSink<D> {
    /// Seconds one beat lasts at the project's current tempo.
    fn beat_seconds(&self, project: ProjectContext) -> f64 {
        let bpm = self.daw.get_tempo(project);
        if bpm > 0.0 { 60.0 / bpm } else { 0.5 }
    }

    fn write(&self, notes: &[u8], beats: u32) -> Result<(), String> {
        if notes.is_empty() {
            return Err("nothing to insert".to_string());
        }
        let project = ProjectContext::Current;

        // The selected track is the target — same convention as REAPER's
        // own insert actions, and it keeps the panel from having to carry
        // a track picker.
        let track = self
            .daw
            .selected(project.clone())
            .into_iter()
            .next()
            .ok_or_else(|| "select a track first".to_string())?;

        let start = self.daw.get_position(project.clone());
        let length = self.beat_seconds(project.clone()) * f64::from(beats.max(1));
        let end = start + length;

        let location = self
            .daw
            .create_midi_item(
                project.clone(),
                daw::service::TrackRef::Guid(track.guid),
                start,
                end,
            )
            .ok_or_else(|| "could not create a MIDI item".to_string())?;

        // `start_ppq` is re-read as a project quarter-note position by the
        // REAPER backend; length stays a raw PPQ delta.
        let qn = |seconds: f64| {
            self.daw
                .time_to_quarter_notes(project.clone(), PositionInSeconds::from_seconds(seconds))
                .quarter_notes
                .as_quarter_notes()
        };
        let start_qn = qn(start);
        let length_ppq = ((qn(end) - start_qn) * PPQ).max(1.0);

        let creates: Vec<MidiNoteCreate> = notes
            .iter()
            .map(|pitch| MidiNoteCreate {
                channel: 0,
                pitch: *pitch,
                velocity: VELOCITY,
                start_ppq: start_qn,
                length_ppq,
            })
            .collect();
        self.daw.add_notes(location, creates);

        // Advance so the next insert continues the progression rather
        // than stacking on top of this one.
        // UFCS: `set_position` exists on both Items and Transport, so a
        // method call is ambiguous (E0034). See issue #92.
        let _ = Transport::set_position(&self.daw, project, end);
        Ok(())
    }
}

impl<D: ChordDaw> chord_tool::ChordSink for DawChordSink<D> {
    fn preview(&self, notes: &[u8]) {
        // Auditioning without writing needs a note-send path the daw
        // facade doesn't expose yet (REAPER's StuffMIDIMessage, or a
        // routed sampler). Recording what would sound keeps the UI
        // honest until that exists.
        if let Ok(mut playing) = self.playing.lock() {
            *playing = notes.to_vec();
        }
        tracing::debug!(?notes, "preview requested (no note-send path yet)");
    }

    fn insert(&self, notes: &[u8], beats: u32) -> Result<(), String> {
        self.write(notes, beats)
    }
}
