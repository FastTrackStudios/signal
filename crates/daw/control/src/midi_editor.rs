//! MIDI editing handles

use std::sync::Arc;

use crate::DawClients;
use crate::Result;
use daw_proto::{
    ProjectContext,
    item::{ItemRef, TakeRef},
    midi::{
        HumanizeParams, MidiCC, MidiCCCreate, MidiNote, MidiNoteCreate, MidiPitchBend,
        MidiPitchBendCreate, MidiProgramChange, MidiSysEx, MidiTakeLocation, PpqRange,
        QuantizeParams,
    },
};

/// MIDI editor for a take
///
/// Provides methods for editing MIDI notes, CC events, and other MIDI data
/// within a take.
#[derive(Clone)]
pub struct MidiEditor {
    item_guid: String,
    take_ref: TakeRef,
    project_id: String,
    clients: Arc<DawClients>,
}

impl MidiEditor {
    /// Create a new MIDI editor
    pub(crate) fn new(
        item_guid: String,
        take_ref: TakeRef,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            item_guid,
            take_ref,
            project_id,
            clients,
        }
    }

    /// Helper to create MIDI take location
    fn location(&self) -> MidiTakeLocation {
        MidiTakeLocation::new(
            ProjectContext::Project(self.project_id.clone()),
            ItemRef::Guid(self.item_guid.clone()),
            self.take_ref.clone(),
        )
    }

    // =========================================================================
    // Note Queries
    // =========================================================================

    /// Get all notes
    pub async fn notes(&self) -> Result<Vec<MidiNote>> {
        let notes = self.clients.midi.notes(self.location()).await?;
        Ok(notes)
    }

    /// Write the take's contents in one call, as one undo point — what an
    /// editor that holds the whole take (the expression editor) saves
    /// with. Returns the notes' new indices, in the order given.
    pub async fn write_take(
        &self,
        content: daw_proto::midi::MidiTakeContent,
        mode: daw_proto::midi::WriteMode,
    ) -> Result<Vec<u32>> {
        let indices = self
            .clients
            .midi
            .write_take(self.location(), content, mode)
            .await?;
        Ok(indices)
    }

    /// Get notes in a PPQ range
    pub async fn notes_in_range(&self, start_ppq: f64, end_ppq: f64) -> Result<Vec<MidiNote>> {
        let notes = self
            .clients
            .midi
            .notes_in_range(self.location(), PpqRange::new(start_ppq, end_ppq))
            .await?;
        Ok(notes)
    }

    /// Get selected notes
    pub async fn selected_notes(&self) -> Result<Vec<MidiNote>> {
        let notes = self.clients.midi.selected_notes(self.location()).await?;
        Ok(notes)
    }

    /// Get note count
    pub async fn note_count(&self) -> Result<u32> {
        let count = self.clients.midi.note_count(self.location()).await?;
        Ok(count)
    }

    // =========================================================================
    // Note CRUD
    // =========================================================================

    /// Add a note
    pub async fn add_note(
        &self,
        pitch: u8,
        velocity: u8,
        start_ppq: f64,
        length_ppq: f64,
    ) -> Result<u32> {
        let note = MidiNoteCreate::new(pitch, velocity, start_ppq, length_ppq);
        let index = self.clients.midi.add_note(self.location(), note).await?;
        Ok(index)
    }

    /// Add a note with channel
    pub async fn add_note_with_channel(
        &self,
        channel: u8,
        pitch: u8,
        velocity: u8,
        start_ppq: f64,
        length_ppq: f64,
    ) -> Result<u32> {
        let note = MidiNoteCreate::with_channel(channel, pitch, velocity, start_ppq, length_ppq);
        let index = self.clients.midi.add_note(self.location(), note).await?;
        Ok(index)
    }

    /// Add multiple notes
    pub async fn add_notes(&self, notes: Vec<MidiNoteCreate>) -> Result<Vec<u32>> {
        let indices = self.clients.midi.add_notes(self.location(), notes).await?;
        Ok(indices)
    }

    /// Add notes positioned in raw take PPQ — the units [`Self::notes`]
    /// returns.
    ///
    /// [`Self::add_notes`] does not round-trip with [`Self::notes`]: it
    /// reads `start_ppq` as a project *quarter-note* position. Use this
    /// one when the positions came from reading the take.
    pub async fn add_notes_ppq(&self, notes: Vec<MidiNoteCreate>) -> Result<Vec<u32>> {
        let indices = self
            .clients
            .midi
            .add_notes_ppq(self.location(), notes)
            .await?;
        Ok(indices)
    }

    /// Delete a note
    pub async fn delete_note(&self, index: u32) -> Result<()> {
        self.clients
            .midi
            .delete_note(self.location(), index)
            .await?;
        Ok(())
    }

    /// Delete multiple notes
    pub async fn delete_notes(&self, indices: Vec<u32>) -> Result<()> {
        self.clients
            .midi
            .delete_notes(self.location(), indices)
            .await?;
        Ok(())
    }

    /// Delete all selected notes
    pub async fn delete_selected(&self) -> Result<()> {
        self.clients
            .midi
            .delete_selected_notes(self.location())
            .await?;
        Ok(())
    }

    // =========================================================================
    // Note Modification
    // =========================================================================

    /// Set note pitch
    pub async fn set_pitch(&self, index: u32, pitch: u8) -> Result<()> {
        self.clients
            .midi
            .set_note_pitch(self.location(), index, pitch)
            .await?;
        Ok(())
    }

    /// Set note velocity
    pub async fn set_velocity(&self, index: u32, velocity: u8) -> Result<()> {
        self.clients
            .midi
            .set_note_velocity(self.location(), index, velocity)
            .await?;
        Ok(())
    }

    /// Set note position
    pub async fn set_position(&self, index: u32, start_ppq: f64) -> Result<()> {
        self.clients
            .midi
            .set_note_position(self.location(), index, start_ppq)
            .await?;
        Ok(())
    }

    /// Set note length
    pub async fn set_length(&self, index: u32, length_ppq: f64) -> Result<()> {
        self.clients
            .midi
            .set_note_length(self.location(), index, length_ppq)
            .await?;
        Ok(())
    }

    /// Select or deselect a note
    pub async fn set_selected(&self, index: u32, selected: bool) -> Result<()> {
        self.clients
            .midi
            .set_note_selected(self.location(), index, selected)
            .await?;
        Ok(())
    }

    /// Mute or unmute a note
    pub async fn set_muted(&self, index: u32, muted: bool) -> Result<()> {
        self.clients
            .midi
            .set_note_muted(self.location(), index, muted)
            .await?;
        Ok(())
    }

    // =========================================================================
    // Batch Operations
    // =========================================================================

    /// Select all notes
    pub async fn select_all(&self) -> Result<()> {
        self.clients
            .midi
            .select_all_notes(self.location(), true)
            .await?;
        Ok(())
    }

    /// Deselect all notes
    pub async fn deselect_all(&self) -> Result<()> {
        self.clients
            .midi
            .select_all_notes(self.location(), false)
            .await?;
        Ok(())
    }

    /// Transpose notes by semitones
    pub async fn transpose(&self, indices: Vec<u32>, semitones: i8) -> Result<()> {
        self.clients
            .midi
            .transpose_notes(self.location(), indices, semitones)
            .await?;
        Ok(())
    }

    /// Transpose selected notes by semitones
    pub async fn transpose_selected(&self, semitones: i8) -> Result<()> {
        self.transpose(vec![], semitones).await
    }

    /// Quantize notes to a grid
    pub async fn quantize(&self, grid_ppq: f64, strength: f64) -> Result<()> {
        self.clients
            .midi
            .quantize_notes(
                self.location(),
                QuantizeParams {
                    indices: vec![],
                    grid_ppq,
                    strength,
                },
            )
            .await?;
        Ok(())
    }

    /// Quantize specific notes to a grid
    pub async fn quantize_notes(
        &self,
        indices: Vec<u32>,
        grid_ppq: f64,
        strength: f64,
    ) -> Result<()> {
        self.clients
            .midi
            .quantize_notes(
                self.location(),
                QuantizeParams {
                    indices,
                    grid_ppq,
                    strength,
                },
            )
            .await?;
        Ok(())
    }

    /// Humanize notes (add random variation)
    pub async fn humanize(&self, timing_range_ppq: f64, velocity_range: u8) -> Result<()> {
        self.clients
            .midi
            .humanize_notes(
                self.location(),
                HumanizeParams {
                    indices: vec![],
                    timing_range_ppq,
                    velocity_range,
                },
            )
            .await?;
        Ok(())
    }

    // =========================================================================
    // CC Events
    // =========================================================================

    /// Get all CC events (optionally filtered by controller)
    pub async fn ccs(&self, controller: Option<u8>) -> Result<Vec<MidiCC>> {
        let ccs = self.clients.midi.ccs(self.location(), controller).await?;
        Ok(ccs)
    }

    /// Add a CC event
    pub async fn add_cc(
        &self,
        channel: u8,
        controller: u8,
        value: u8,
        position_ppq: f64,
    ) -> Result<u32> {
        let cc = MidiCCCreate::new(channel, controller, value, position_ppq);
        let index = self.clients.midi.add_cc(self.location(), cc).await?;
        Ok(index)
    }

    /// Delete a CC event
    pub async fn delete_cc(&self, index: u32) -> Result<()> {
        self.clients.midi.delete_cc(self.location(), index).await?;
        Ok(())
    }

    /// Set CC value
    pub async fn set_cc_value(&self, index: u32, value: u8) -> Result<()> {
        self.clients
            .midi
            .set_cc_value(self.location(), index, value)
            .await?;
        Ok(())
    }

    // =========================================================================
    // Other Events
    // =========================================================================

    /// Get pitch bend events
    pub async fn pitch_bends(&self) -> Result<Vec<MidiPitchBend>> {
        let bends = self.clients.midi.pitch_bends(self.location()).await?;
        Ok(bends)
    }

    /// Add a pitch bend event
    pub async fn add_pitch_bend(&self, channel: u8, value: i16, position_ppq: f64) -> Result<u32> {
        let pb = MidiPitchBendCreate::new(channel, value, position_ppq);
        let index = self
            .clients
            .midi
            .add_pitch_bend(self.location(), pb)
            .await?;
        Ok(index)
    }

    /// Get program change events
    pub async fn program_changes(&self) -> Result<Vec<MidiProgramChange>> {
        let changes = self.clients.midi.program_changes(self.location()).await?;
        Ok(changes)
    }

    /// Get SysEx events
    pub async fn sysex(&self) -> Result<Vec<MidiSysEx>> {
        let events = self.clients.midi.sysex(self.location()).await?;
        Ok(events)
    }
}

impl std::fmt::Debug for MidiEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiEditor")
            .field("item_guid", &self.item_guid)
            .field("take_ref", &self.take_ref)
            .field("project_id", &self.project_id)
            .finish()
    }
}
