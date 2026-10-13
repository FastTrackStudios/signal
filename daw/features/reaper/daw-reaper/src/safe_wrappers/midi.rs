//! Safe wrappers for REAPER MIDI APIs.

use super::ReaperLow;
use reaper_medium::{MediaItem, MediaItemTake, MediaTrack};

/// Counts of MIDI events in a take.
pub struct MidiEventCounts {
    pub notes: i32,
    pub ccs: i32,
    pub text_sysex: i32,
}

/// Count MIDI events (notes, CCs, text/sysex) in a take.
pub fn count_events(low: &ReaperLow, take: MediaItemTake) -> MidiEventCounts {
    let mut notes: i32 = 0;
    let mut ccs: i32 = 0;
    let mut text_sysex: i32 = 0;
    unsafe {
        low.MIDI_CountEvts(take.as_ptr(), &mut notes, &mut ccs, &mut text_sysex);
    }
    MidiEventCounts {
        notes,
        ccs,
        text_sysex,
    }
}

/// Raw MIDI note data.
pub struct MidiNoteRaw {
    pub selected: bool,
    pub muted: bool,
    pub start_ppq: f64,
    pub end_ppq: f64,
    pub channel: i32,
    pub pitch: i32,
    pub velocity: i32,
}

/// Get a MIDI note by index.
pub fn get_note(low: &ReaperLow, take: MediaItemTake, index: i32) -> Option<MidiNoteRaw> {
    let mut n = MidiNoteRaw {
        selected: false,
        muted: false,
        start_ppq: 0.0,
        end_ppq: 0.0,
        channel: 0,
        pitch: 0,
        velocity: 0,
    };
    let ok = unsafe {
        low.MIDI_GetNote(
            take.as_ptr(),
            index,
            &mut n.selected,
            &mut n.muted,
            &mut n.start_ppq,
            &mut n.end_ppq,
            &mut n.channel,
            &mut n.pitch,
            &mut n.velocity,
        )
    };
    ok.then_some(n)
}

/// Insert a MIDI note.
#[allow(clippy::too_many_arguments)]
pub fn insert_note(
    low: &ReaperLow,
    take: MediaItemTake,
    selected: bool,
    muted: bool,
    start_ppq: f64,
    end_ppq: f64,
    channel: i32,
    pitch: i32,
    velocity: i32,
) {
    unsafe {
        low.MIDI_InsertNote(
            take.as_ptr(),
            selected,
            muted,
            start_ppq,
            end_ppq,
            channel,
            pitch,
            velocity,
            std::ptr::null_mut(),
        );
    }
}

/// Fields of an existing note to change. `None` leaves a field alone.
///
/// `MIDI_SetNote` takes a null pointer per field to mean "don't touch",
/// which is what makes it usable for a velocity edit that must not
/// disturb timing or pitch. Modelling that as `Option`s keeps the unsafe
/// pointer juggling in one place instead of at every call site.
#[derive(Clone, Copy, Debug, Default)]
pub struct MidiNoteEdit {
    pub selected: Option<bool>,
    pub muted: Option<bool>,
    pub start_ppq: Option<f64>,
    pub end_ppq: Option<f64>,
    pub channel: Option<i32>,
    pub pitch: Option<i32>,
    pub velocity: Option<i32>,
}

/// Modify an existing note in place. Returns whether REAPER accepted it.
///
/// `no_sort` is passed true: callers editing a run of notes should sort
/// once at the end via [`sort`], not once per note. Sorting per note is
/// O(n²) over a take and — worse — can renumber the very indices the
/// caller is still iterating over.
pub fn set_note(low: &ReaperLow, take: MediaItemTake, index: i32, edit: MidiNoteEdit) -> bool {
    // These must outlive the call: the pointers handed to REAPER point
    // into these locals.
    let mut selected = edit.selected.unwrap_or_default();
    let mut muted = edit.muted.unwrap_or_default();
    let mut start_ppq = edit.start_ppq.unwrap_or_default();
    let mut end_ppq = edit.end_ppq.unwrap_or_default();
    let mut channel = edit.channel.unwrap_or_default();
    let mut pitch = edit.pitch.unwrap_or_default();
    let mut velocity = edit.velocity.unwrap_or_default();
    let mut no_sort = true;

    macro_rules! opt_ptr {
        ($field:expr, $local:expr) => {
            if $field.is_some() {
                &mut $local
            } else {
                std::ptr::null_mut()
            }
        };
    }

    unsafe {
        low.MIDI_SetNote(
            take.as_ptr(),
            index,
            opt_ptr!(edit.selected, selected),
            opt_ptr!(edit.muted, muted),
            opt_ptr!(edit.start_ppq, start_ppq),
            opt_ptr!(edit.end_ppq, end_ppq),
            opt_ptr!(edit.channel, channel),
            opt_ptr!(edit.pitch, pitch),
            opt_ptr!(edit.velocity, velocity),
            &no_sort,
        )
    }
}

/// Delete a note by index. Returns whether REAPER accepted it.
///
/// Deleting renumbers every note above `index`, so callers removing more
/// than one must work from the highest index down — see
/// `Midi::delete_notes`.
pub fn delete_note(low: &ReaperLow, take: MediaItemTake, index: i32) -> bool {
    unsafe { low.MIDI_DeleteNote(take.as_ptr(), index) }
}

/// Sort MIDI events in a take.
pub fn sort(low: &ReaperLow, take: MediaItemTake) {
    unsafe { low.MIDI_Sort(take.as_ptr()) };
}

/// Convert a project quarter-note position to PPQ for a take.
pub fn get_ppq_pos_from_proj_qn(low: &ReaperLow, take: MediaItemTake, qn: f64) -> f64 {
    unsafe { low.MIDI_GetPPQPosFromProjQN(take.as_ptr(), qn) }
}

/// Check if a take contains MIDI data.
pub fn take_is_midi(low: &ReaperLow, take: MediaItemTake) -> bool {
    unsafe { low.TakeIsMIDI(take.as_ptr()) }
}

/// Create a new MIDI item in a project.
pub fn create_new_midi_item(
    low: &ReaperLow,
    track: MediaTrack,
    start: f64,
    end: f64,
) -> Option<MediaItem> {
    let ptr =
        unsafe { low.CreateNewMIDIItemInProj(track.as_ptr(), start, end, std::ptr::null_mut()) };
    MediaItem::new(ptr)
}

/// Get the active take of a media item (low-level).
pub fn get_active_take(low: &ReaperLow, item: MediaItem) -> Option<MediaItemTake> {
    let ptr = unsafe { low.GetActiveTake(item.as_ptr()) };
    MediaItemTake::new(ptr)
}
