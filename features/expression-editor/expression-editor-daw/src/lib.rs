//! Adapter between the expression editor's document and the DAW MIDI
//! API.
//!
//! Depends on the `daw` **facade**, never on a backend, which is what
//! lets one editor drive a live REAPER take and a standalone project
//! without knowing which it has. A `.mid` file arrives through the same
//! service, so all three paths produce the same [`ExpressionDoc`].
//!
//! The conversion is the interesting part, and it is deliberately pure
//! and testable without a DAW: [`to_doc`] and [`to_content`] are plain
//! functions over snapshots.
//!
//! ## Where the editor's model and MIDI disagree
//!
//! - MIDI channels are 0-based on the wire and 1-based to musicians.
//!   The editor stores the musician's number (2..=16 for MPE members),
//!   so every crossing converts.
//! - The editor's pitch curve is **semitones relative to the note's
//!   row**; MIDI carries a 14-bit bend word whose meaning depends on
//!   the instrument's bend range. The document's `bend_range` is the
//!   only thing that makes those two comparable, and it has to travel
//!   with the conversion or every curve reads wrong by a factor.
//! - Pitch bend is per *channel*, not per note. Reconstructing which
//!   note owns a bend event is exactly the MPE ownership problem the
//!   core already models, so bends are attributed to the note sounding
//!   on that channel and flagged ambiguous when two are.

pub mod fixture;
pub mod midi_tools_sink;
pub mod session;
pub mod shapes;
pub mod state;

pub use session::Session;

/// MPE's per-note bend range convention, in semitones.
///
/// Lives here rather than in each caller because it has to be the same
/// number on both sides of a conversion: a document read at one range
/// and written at another has every pitch curve rescaled silently. The
/// REAPER module loads takes with it and [`fixture`] declares it on the
/// wire, so the two cannot drift apart.
pub const DEFAULT_BEND_RANGE: f64 = 48.0;

use daw::service::midi::{
    MidiCCCreate, MidiChannelPressureCreate, MidiNoteCreate, MidiPitchBendCreate, MidiTakeContent,
    MidiTakeSnapshot,
};
use expression_editor_core::cc::CcLane;
use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::tuning;

/// Convert a take snapshot into an editable document.
pub fn to_doc(snapshot: &MidiTakeSnapshot, bend_range: f64) -> ExpressionDoc {
    let ppq = if snapshot.ppq > 0.0 {
        snapshot.ppq
    } else {
        960.0
    };
    let end = snapshot.length_ppq.max(ppq * 4.0);
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq }, 0.0, end);
    doc.bend_range = bend_range;

    for (i, n) in snapshot.notes.iter().enumerate() {
        let mut note = Note::new(
            NoteId(i as u64 + 1),
            n.start_ppq,
            n.start_ppq + n.length_ppq,
            n.pitch as i32,
        );
        // Wire channels are 0-based; the editor stores what a musician
        // would call it.
        note.channel = Some(n.channel + 1);
        note.velocity = n.velocity as f64 / 127.0;
        note.muted = n.muted;
        doc.push(note);
    }

    // Attribute bends to whichever note is sounding on that channel.
    for pb in &snapshot.pitch_bends {
        let semitones = tuning::bend14_to_semitones((pb.value as i32 + 8192) as u16, bend_range);
        let owners: Vec<NoteId> = doc
            .notes
            .iter()
            .filter(|n| {
                n.channel == Some(pb.channel + 1)
                    && n.start <= pb.position_ppq
                    && n.end >= pb.position_ppq
            })
            .map(|n| n.id)
            .collect();
        // More than one owner is the ambiguity the core already models;
        // writing to both would be a guess, so leave it to be flagged.
        if owners.len() == 1
            && let Some(n) = doc.note_mut(owners[0])
        {
            n.pitch.set(pb.position_ppq, semitones);
        }
    }

    // Channel pressure and CC74 are the MPE expression dimensions.
    for cp in &snapshot.channel_pressures {
        set_lane(
            &mut doc,
            cp.channel + 1,
            cp.position_ppq,
            Dimension::Pressure,
            cp.pressure,
        );
    }
    for cc in &snapshot.ccs {
        if cc.controller == 74 {
            set_lane(
                &mut doc,
                cc.channel + 1,
                cc.position_ppq,
                Dimension::Timbre,
                cc.value,
            );
        }
    }

    // Everything else is a document-level controller lane.
    for cc in snapshot.ccs.iter().filter(|c| c.controller != 74) {
        let i = doc.cc.ensure(cc.controller);
        doc.cc.lanes[i]
            .curve
            .set(cc.position_ppq, cc.value as f64 / 127.0);
    }
    // CC1 and CC11 are what an orchestral part rides, so show them
    // without being asked; the rest stay available but unpinned.
    for number in [1u8, 11] {
        if let Some(l) = doc.cc.get_mut(number) {
            l.pinned = true;
        }
    }

    doc.mark_ambiguity();
    doc
}

fn set_lane(doc: &mut ExpressionDoc, channel: u8, t: f64, dimension: Dimension, value: u8) {
    let owners: Vec<NoteId> = doc
        .notes
        .iter()
        .filter(|n| n.channel == Some(channel) && n.start <= t && n.end >= t)
        .map(|n| n.id)
        .collect();
    if owners.len() != 1 {
        return;
    }
    if let Some(n) = doc.note_mut(owners[0]) {
        n.curve_mut(dimension).set(t, value as f64 / 127.0);
    }
}

/// How densely per-note expression is written back.
///
/// Instruments vary in how they interpolate, and several ignore sparse
/// data entirely — the density here is the same "modest constant
/// stream" MPE writers use so endpoint-only curves are not silently
/// dropped.
pub const EXPRESSION_SAMPLES_PER_NOTE: usize = 24;

/// Convert a document back into take content.
pub fn to_content(doc: &ExpressionDoc) -> MidiTakeContent {
    let mut notes = Vec::with_capacity(doc.notes.len());
    let mut bends = Vec::new();
    let mut ccs = Vec::new();
    let mut pressures = Vec::new();

    for n in &doc.notes {
        // Ambiguous ownership means the writer cannot know which note a
        // bend belongs to. Writing anyway would corrupt the take, so the
        // note goes out but its expression does not.
        let safe = !n.ambiguous;
        let channel = n.channel.unwrap_or(1).saturating_sub(1).min(15);
        notes.push(MidiNoteCreate {
            channel,
            pitch: n.row.clamp(0, 127) as u8,
            velocity: (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8,
            start_ppq: n.start,
            length_ppq: n.len().max(1.0),
        });
        if !safe {
            continue;
        }

        let steps = EXPRESSION_SAMPLES_PER_NOTE;
        for dimension in Dimension::ALL {
            if n.curve(dimension).is_empty() {
                continue;
            }
            for k in 0..=steps {
                let f = k as f64 / steps as f64;
                let t = n.start + n.len() * f;
                let v = n.curve(dimension).sample(t, dimension.default_value());
                match dimension {
                    Dimension::Pitch => bends.push(MidiPitchBendCreate {
                        channel,
                        value: tuning::semitones_to_bend14(v, doc.bend_range) as i16 - 8192,
                        position_ppq: t,
                    }),
                    Dimension::Timbre => ccs.push(MidiCCCreate {
                        channel,
                        controller: 74,
                        value: (v * 127.0).round().clamp(0.0, 127.0) as u8,
                        position_ppq: t,
                    }),
                    // Real channel pressure, not CC11. It used to go
                    // out as CC11 because the bulk content type had no
                    // field for it — but the snapshot reads pressure
                    // from `channel_pressures`, so the dimension did not
                    // survive a round trip (#167).
                    Dimension::Pressure => pressures.push(MidiChannelPressureCreate {
                        channel,
                        pressure: (v * 127.0).round().clamp(0.0, 127.0) as u8,
                        position_ppq: t,
                    }),
                }
            }
        }
    }

    // Document-level controllers, written as authored rather than
    // resampled — these are already the shape the user drew.
    for dimension in &doc.cc.lanes {
        for p in dimension.curve.points() {
            ccs.push(MidiCCCreate {
                channel: 0,
                controller: dimension.number,
                value: (p.value * 127.0).round().clamp(0.0, 127.0) as u8,
                position_ppq: p.t,
            });
        }
    }

    MidiTakeContent {
        notes,
        ccs,
        pitch_bends: bends,
        note_expressions: Vec::new(),
        channel_pressures: pressures,
    }
}

/// Whether a document is safe to write.
///
/// Ambiguous notes lose their expression on the way out (see
/// [`to_content`]), so a caller should say so before overwriting a take
/// rather than after.
pub fn write_warnings(doc: &ExpressionDoc) -> Vec<String> {
    let ambiguous = doc.notes.iter().filter(|n| n.ambiguous).count();
    let mut out = Vec::new();
    if ambiguous > 0 {
        out.push(format!(
            "{ambiguous} notes share a channel while sounding — their \
             expression cannot be attributed and will not be written"
        ));
    }
    let unchannelled = doc.notes.iter().filter(|n| n.channel.is_none()).count();
    if unchannelled > 0 {
        out.push(format!(
            "{unchannelled} notes have no channel and will be written to channel 1"
        ));
    }
    out
}

/// Controller lanes a document ended up with, for a UI listing.
pub fn controller_summary(doc: &ExpressionDoc) -> Vec<(u8, String, usize)> {
    doc.cc
        .lanes
        .iter()
        .map(|l: &CcLane| (l.number, l.name.clone(), l.curve.len()))
        .collect()
}
