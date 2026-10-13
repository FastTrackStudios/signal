//! Per-block MIDI / note-expression event collection for the FX stage.

use super::snapshot::TrackSnapshot;

/// Walk every MIDI note on the track's items, emit NoteOn / NoteOff
/// events whose timestamps fall inside `[start_seconds, end_seconds)`.
/// Returns events sorted by sample offset — VST3's `IEventList`
/// (and most CLAP hosts) want note events monotonically ordered.
/// `frames_per_second` is output frames per timeline second — the
/// sample rate over the transport's varispeed rate.
pub(crate) fn collect_midi_events(
    track: &TrackSnapshot,
    start_seconds: f64,
    end_seconds: f64,
    frames_per_second: f64,
    frames: usize,
) -> Vec<crate::plugin::PluginMidiEvent> {
    let mut out = Vec::new();
    collect_midi_events_into(&mut out, track, start_seconds, end_seconds, frames_per_second, frames);
    out
}

/// [`collect_midi_events`] into `out` (cleared first) — the render keeps one
/// buffer and reuses it, so the audio thread does not allocate per track per
/// block.
pub(crate) fn collect_midi_events_into(
    out: &mut Vec<crate::plugin::PluginMidiEvent>,
    track: &TrackSnapshot,
    start_seconds: f64,
    end_seconds: f64,
    frames_per_second: f64,
    frames: usize,
) {
    use daw_proto::live_midi::{Channel, KeyNumber, MidiEvent, Velocity};

    out.clear();
    let to_sample = |t_seconds: f64| -> u32 {
        let frame = ((t_seconds - start_seconds) * frames_per_second).floor() as i64;
        frame.clamp(0, (frames as i64).saturating_sub(1)) as u32
    };
    for item in &track.items {
        if item.muted {
            continue;
        }
        // Quick bail-out: skip items that can't possibly overlap.
        // MIDI events can fire from notes already started before
        // start_seconds (only their NoteOff matters), so we use the
        // full item span [position, position+length) plus the
        // outstanding-tail length already encoded in start_seconds.
        let item_end = item.position_seconds + item.length_seconds;
        if item_end < start_seconds || item.position_seconds >= end_seconds + 0.0 {
            // Note: items entirely before the block window may still
            // have notes that *end* during the block — but if the
            // item itself ends before start_seconds, no on-going note
            // can survive past it.
            if item_end < start_seconds {
                continue;
            }
        }
        for n in &item.midi_notes {
            // NoteOn falls in this block?
            if n.start_seconds >= start_seconds && n.start_seconds < end_seconds {
                out.push(crate::plugin::PluginMidiEvent {
                    offset: to_sample(n.start_seconds),
                    message: MidiEvent::NoteOn {
                        channel: Channel::new(n.channel),
                        key: KeyNumber::new(n.pitch),
                        velocity: Velocity::new(n.velocity),
                    },
                });
            }
            // NoteOff falls in this block?
            let end = n.start_seconds + n.length_seconds;
            if end >= start_seconds && end < end_seconds {
                out.push(crate::plugin::PluginMidiEvent {
                    offset: to_sample(end),
                    message: MidiEvent::NoteOff {
                        channel: Channel::new(n.channel),
                        key: KeyNumber::new(n.pitch),
                        velocity: Velocity::new(0),
                    },
                });
            }
        }
        // CC / pitch bend / program change / sysex — all delivered at
        // their `time_seconds`, clamped into the block window.
        for ev in &item.midi_other {
            if ev.time_seconds >= start_seconds && ev.time_seconds < end_seconds {
                out.push(crate::plugin::PluginMidiEvent {
                    offset: to_sample(ev.time_seconds),
                    message: ev.message.clone(),
                });
            }
        }
    }
    out.sort_by_key(|e| e.offset);
}

/// Walk every per-note expression point on the track's items and
/// emit those that fall inside `[start_seconds, end_seconds)` as
/// `PluginNoteExpression`s with sample offsets clamped to the block.
pub(crate) fn collect_note_expressions(
    track: &TrackSnapshot,
    start_seconds: f64,
    end_seconds: f64,
    frames_per_second: f64,
    frames: usize,
) -> Vec<crate::plugin::PluginNoteExpression> {
    let mut out: Vec<crate::plugin::PluginNoteExpression> = Vec::new();
    let to_sample = |t_seconds: f64| -> u32 {
        let frame = ((t_seconds - start_seconds) * frames_per_second).floor() as i64;
        frame.clamp(0, (frames as i64).saturating_sub(1)) as u32
    };
    for item in &track.items {
        if item.muted {
            continue;
        }
        for ev in &item.note_expressions {
            if ev.time_seconds >= start_seconds && ev.time_seconds < end_seconds {
                out.push(crate::plugin::PluginNoteExpression {
                    offset: to_sample(ev.time_seconds),
                    channel: ev.channel,
                    note: ev.note,
                    dimension: ev.dimension,
                    value: ev.value,
                });
            }
        }
    }
    out.sort_by_key(|e| e.offset);
    out
}
