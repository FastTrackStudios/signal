//! Chord identification for the chord box.
//!
//! Recognition is [`keyflow_proto::chord`]'s, not a second
//! implementation. Keyflow already knows about extensions, alterations,
//! inversions, slash bases and key-aware spelling, and two detectors in
//! one tree would eventually disagree about what a chord is called.
//!
//! This module is only the adapter: the editor knows a set of sounding
//! pitches, keyflow wants timed `MidiNote`s, so we hand it a synthetic
//! simultaneity and take the reading back.

use keyflow_proto::chord::midi::{MidiNote, detect_chords_from_midi_notes};

pub use keyflow_proto::chord::Chord;

/// Identify the chord formed by a set of sounding pitches.
///
/// `pitches` are absolute MIDI numbers. The lowest is the bass, which
/// is what lets keyflow read an inversion as a slash chord rather than
/// as a different chord entirely.
pub fn identify(pitches: &[i32]) -> Option<Chord> {
    if pitches.len() < 2 {
        return None;
    }
    // A synthetic simultaneity: same start, same end, so keyflow's
    // arpeggio filtering cannot split it into several chords.
    const LEN: i64 = 960;
    let notes: Vec<MidiNote> = pitches
        .iter()
        .filter(|p| (0..=127).contains(*p))
        .map(|&p| MidiNote {
            pitch: p as u8,
            start_ppq: 0,
            end_ppq: LEN,
            channel: 0,
            velocity: 100,
        })
        .collect();
    if notes.len() < 2 {
        return None;
    }
    // A zero minimum duration: the caller already decided these notes
    // sound together, so nothing should be filtered as a fragment.
    //
    // Take the LAST reading, not the first. The detector emits
    // progressive interpretations as it accumulates pitches — for a
    // Cmaj7 it reports "C", "C", "Cmaj7" — so the final entry is the
    // one that has seen every note. Taking the first is how you end up
    // with a chord box that calls Cmaj7 "C" and C/E "Em".
    detect_chords_from_midi_notes(&notes, 0)
        .into_iter()
        .next_back()
        .map(|d| d.chord)
}

/// Chord symbols across a whole document region, for a chord track.
///
/// `min_duration` filters arpeggio fragments — a run of passing notes
/// should not each become a chord.
pub fn identify_region(
    notes: &[(i32, f64, f64, f64)],
    min_duration: f64,
) -> Vec<(f64, f64, Chord)> {
    let midi: Vec<MidiNote> = notes
        .iter()
        .filter(|(p, ..)| (0..=127).contains(p))
        .map(|&(pitch, start, end, vel)| MidiNote {
            pitch: pitch as u8,
            start_ppq: start as i64,
            end_ppq: end as i64,
            channel: 0,
            velocity: (vel.clamp(0.0, 1.0) * 127.0) as u8,
        })
        .collect();
    detect_chords_from_midi_notes(&midi, min_duration as i64)
        .into_iter()
        .map(|d| (d.start_ppq as f64, d.end_ppq as f64, d.chord))
        .collect()
}

/// The chord's display name.
///
/// Kept as a function rather than assuming a `Display` impl, so the
/// call site stays one place if keyflow's naming surface changes.
pub fn name(chord: &Chord) -> String {
    chord.to_string()
}
