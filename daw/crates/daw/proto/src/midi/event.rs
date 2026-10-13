//! MIDI editing events for subscriptions

use super::MidiNote;
use facet::Facet;

/// Events related to MIDI editing changes
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum MidiEditEvent {
    /// A note was added
    NoteAdded {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        note: MidiNote,
    },
    /// A note was deleted
    NoteDeleted {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        note_index: u32,
    },
    /// A note was modified
    NoteChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        note: MidiNote,
    },
    /// Notes were transposed
    NotesTransposed {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        semitones: i8,
        note_count: u32,
    },
    /// Notes were quantized
    NotesQuantized {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        note_count: u32,
    },
    /// A CC event was added
    CcAdded {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        controller: u8,
        index: u32,
    },
    /// A CC event was deleted
    CcDeleted {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        index: u32,
    },
}
