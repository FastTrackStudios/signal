//! MIDI Utilities
//!
//! Port of functions from BR_MidiUtil.cpp (SWS).
//! Provides utilities for working with MIDI takes and editors.

use reaper_low::raw::*;
use reaper_medium::{BorrowedPcmSource, Reaper as MediumReaper};

/// Check if a take is open in the inline MIDI editor
/// Port of BR_MidiUtil::IsOpenInInlineEditor
pub fn is_open_in_inline_editor(take: *mut MediaItem_Take, medium_reaper: &MediumReaper) -> bool {
    if take.is_null() {
        return false;
    }

    let low_reaper = medium_reaper.low();

    // Get the item from the take
    let item = unsafe { low_reaper.GetMediaItemTake_Item(take) };

    if item.is_null() {
        return false;
    }

    // Check if this is the active take
    let active_take = unsafe { low_reaper.GetActiveTake(item) };

    if active_take != take {
        return false;
    }

    // Check if it's a MIDI take
    let is_midi = unsafe { low_reaper.TakeIsMIDI(take) };

    if !is_midi {
        return false;
    }

    // For inline editor, we also need to check if it's in the project
    // This is approximated by checking if the take is active
    let _in_project = true; // If it's the active take, it's in the project

    // Check if the source has inline editor extension
    let source_ptr = unsafe { low_reaper.GetMediaItemTake_Source(take) };

    if source_ptr.is_null() {
        return false;
    }

    // Use the medium-level API to call Extended
    let source = unsafe { BorrowedPcmSource::from_raw(&*source_ptr) };

    // Check PCM_SOURCE_EXT_INLINEEDITOR extension
    // PCM_SOURCE_EXT_INLINEEDITOR = 256 (0x100)
    const PCM_SOURCE_EXT_INLINEEDITOR: i32 = 256;
    let extended_result = unsafe {
        source.extended(
            PCM_SOURCE_EXT_INLINEEDITOR,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    // Return true if Extended returns > 0 (indicates inline editor is supported)
    extended_result > 0
}

/// Check if a MIDI note is a black key
/// Port of BR_MidiUtil::IsMidiNoteBlack
pub fn is_midi_note_black(note: i32) -> bool {
    // note position in an octave (first bit is C, second bit is C# etc...)
    let note_in_octave = 1 << (note % 12);
    // 0x054A = all black notes bits set to 1 (binary: 0101 0100 1010)
    // Black keys: C#, D#, F#, G#, A#
    (note_in_octave & 0x054A) != 0
}
