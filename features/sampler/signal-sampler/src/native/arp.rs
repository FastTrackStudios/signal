//! **Arpeggiator** — the MIDI-domain step engine (Omnisphere per-Part arp,
//! Nord Master-Clock arp). Sits in front of the render tree: held notes are
//! consumed and re-emitted as tempo-clocked steps with per-step velocity and
//! gate length.
//!
//! v1: up-pattern over the held notes, one note per step, 32-step velocity/
//! gate sequence (imported from `ARPSEQ2`), latch off. Step modifiers
//! (transpose/chord/dividers/strums), other patterns and Groove Lock are
//! pending.

use signal_plugin_host::PluginMidiEvent;

/// Build a channel-0 note-on `MidiEvent` from raw note/velocity.
fn ev_note_on(note: u8, vel: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOn {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(vel),
    }
}

/// Build a channel-0 note-off `MidiEvent` from a raw note.
fn ev_note_off(note: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOff {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(0),
    }
}

/// One programmed step.
#[derive(Clone, Copy, Debug)]
pub struct ArpStep {
    /// Step on/off (a rest when off).
    pub on: bool,
    /// Velocity 1..127 (scales the played note's velocity is NOT done — the
    /// step velocity IS the emitted velocity, like the pattern programmer).
    pub velocity: u8,
    /// Gate length as a fraction of the step (0..1].
    pub gate: f32,
}

impl Default for ArpStep {
    fn default() -> Self {
        Self {
            on: true,
            velocity: 100,
            gate: 0.8,
        }
    }
}

/// The step engine. Feed it the incoming MIDI each block; it returns the
/// MIDI the tree should hear instead.
pub struct ArpEngine {
    pub steps: Vec<ArpStep>,
    /// Step length in beats (0.25 = 1/16).
    pub step_beats: f32,
    /// Held notes, sorted ascending (up pattern).
    held: Vec<u8>,
    /// Position within the pattern.
    step_idx: usize,
    /// Which held note the next step plays (up pattern cycles this).
    note_idx: usize,
    /// Frames left until the next step boundary.
    frames_to_step: f32,
    /// Currently sounding note + frames until its gate closes.
    sounding: Option<(u8, f32)>,
}

impl ArpEngine {
    pub fn new(steps: Vec<ArpStep>, step_beats: f32) -> Self {
        Self {
            steps: if steps.is_empty() {
                vec![ArpStep::default(); 16]
            } else {
                steps
            },
            step_beats: step_beats.clamp(0.03125, 4.0),
            held: Vec::new(),
            step_idx: 0,
            note_idx: 0,
            frames_to_step: 0.0,
            sounding: None,
        }
    }

    /// Transform one block of MIDI. Note on/off is consumed into the held
    /// set; everything else passes through. Returns the stepped stream.
    pub fn process(
        &mut self,
        events: &[PluginMidiEvent],
        frames: usize,
        sample_rate: f32,
        tempo_bpm: f32,
    ) -> Vec<PluginMidiEvent> {
        use midicore::MidiEvent;
        let mut out = Vec::new();
        for ev in events {
            match &ev.message {
                MidiEvent::NoteOn { key, velocity, .. } if velocity.get() > 0 => {
                    let note = key.get();
                    if !self.held.contains(&note) {
                        let was_empty = self.held.is_empty();
                        self.held.push(note);
                        self.held.sort_unstable();
                        if was_empty {
                            // Phrase start: fire the first step immediately.
                            self.step_idx = 0;
                            self.note_idx = 0;
                            self.frames_to_step = 0.0;
                        }
                    }
                }
                MidiEvent::NoteOn { key, .. } | MidiEvent::NoteOff { key, .. } => {
                    let note = key.get();
                    self.held.retain(|&n| n != note);
                    if let Some(idx) = self.held.len().checked_sub(1) {
                        self.note_idx = self.note_idx.min(idx);
                    }
                }
                _ => out.push(ev.clone()),
            }
        }

        let step_frames = (self.step_beats * 60.0 / tempo_bpm.max(1.0)) * sample_rate.max(1.0);
        let mut t = 0.0f32;
        while t < frames as f32 {
            // Close the sounding note when its gate ends inside this block.
            if let Some((note, off_at)) = self.sounding {
                if (off_at <= t || off_at < frames as f32)
                    && off_at >= t {
                        out.push(PluginMidiEvent {
                            offset: off_at as u32,
                            message: ev_note_off(note),
                        });
                        self.sounding = None;
                    }
            }
            if self.held.is_empty() {
                break;
            }
            if self.frames_to_step <= t {
                // Step boundary: cut anything still sounding, fire the step.
                let at = self.frames_to_step.max(0.0);
                if let Some((note, _)) = self.sounding.take() {
                    out.push(PluginMidiEvent {
                        offset: at as u32,
                        message: ev_note_off(note),
                    });
                }
                let step = self.steps[self.step_idx % self.steps.len()];
                if step.on {
                    let note = self.held[self.note_idx % self.held.len()];
                    out.push(PluginMidiEvent {
                        offset: at as u32,
                        message: ev_note_on(note, step.velocity.max(1)),
                    });
                    self.sounding = Some((note, at + step_frames * step.gate.clamp(0.05, 1.0)));
                    self.note_idx = (self.note_idx + 1) % self.held.len().max(1);
                }
                self.step_idx = (self.step_idx + 1) % self.steps.len();
                self.frames_to_step += step_frames;
            }
            // Advance to the next event inside the block, or leave.
            let next = self
                .frames_to_step
                .min(self.sounding.map(|(_, off)| off).unwrap_or(f32::INFINITY));
            if next >= frames as f32 {
                break;
            }
            t = next;
        }
        // Roll counters into the next block's frame of reference.
        self.frames_to_step -= frames as f32;
        if self.frames_to_step < -step_frames {
            self.frames_to_step = 0.0;
        }
        if let Some((_, off_at)) = &mut self.sounding {
            *off_at -= frames as f32;
        }
        // All notes released → stop the sounding note.
        if self.held.is_empty() {
            if let Some((note, _)) = self.sounding.take() {
                out.push(PluginMidiEvent {
                    offset: frames.saturating_sub(1) as u32,
                    message: ev_note_off(note),
                });
            }
        }
        out.sort_by_key(|e| e.offset);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midicore::MidiEvent;

    fn note_on(note: u8) -> PluginMidiEvent {
        PluginMidiEvent {
            offset: 0,
            message: ev_note_on(note, 100),
        }
    }

    #[test]
    fn held_chord_steps_upward() {
        // 1/16 at 120 BPM = 0.125 s = 6000 frames at 48k.
        let mut arp = ArpEngine::new(vec![ArpStep::default(); 4], 0.25);
        let first = arp.process(
            &[note_on(60), note_on(64), note_on(67)],
            6_000,
            48_000.0,
            120.0,
        );
        let ons: Vec<u8> = first
            .iter()
            .filter_map(|e| match e.message {
                MidiEvent::NoteOn { key, .. } => Some(key.get()),
                _ => None,
            })
            .collect();
        assert_eq!(ons, vec![60], "first step fires the lowest note");
        // Next blocks cycle 64, 67, 60…
        let mut seen = Vec::new();
        for _ in 0..3 {
            let evs = arp.process(&[], 6_000, 48_000.0, 120.0);
            seen.extend(evs.iter().filter_map(|e| match e.message {
                MidiEvent::NoteOn { key, .. } => Some(key.get()),
                _ => None,
            }));
        }
        assert_eq!(seen, vec![64, 67, 60], "up pattern cycles the held chord");
    }

    #[test]
    fn gates_close_and_release_stops_everything() {
        let mut arp = ArpEngine::new(
            vec![
                ArpStep {
                    gate: 0.5,
                    ..Default::default()
                };
                4
            ],
            0.25,
        );
        let evs = arp.process(&[note_on(60)], 6_000, 48_000.0, 120.0);
        // Gate 0.5 of a 6000-frame step → note-off near frame 3000.
        let off = evs
            .iter()
            .find(|e| matches!(e.message, MidiEvent::NoteOff { .. }))
            .expect("gated note-off");
        assert!(
            (2_900..3_100).contains(&off.offset),
            "gate at ~3000, got {}",
            off.offset
        );

        // Releasing the key silences the arp.
        let release = [PluginMidiEvent {
            offset: 0,
            message: ev_note_off(60),
        }];
        let evs = arp.process(&release, 6_000, 48_000.0, 120.0);
        assert!(
            !evs.iter()
                .any(|e| matches!(e.message, MidiEvent::NoteOn { .. })),
            "no steps after release"
        );
    }

    #[test]
    fn rests_are_silent() {
        let steps = vec![
            ArpStep::default(),
            ArpStep {
                on: false,
                ..Default::default()
            },
        ];
        let mut arp = ArpEngine::new(steps, 0.25);
        arp.process(&[note_on(60)], 6_000, 48_000.0, 120.0); // step 1 fires
        let evs = arp.process(&[], 6_000, 48_000.0, 120.0); // step 2 = rest
        assert!(
            !evs.iter()
                .any(|e| matches!(e.message, MidiEvent::NoteOn { .. })),
            "rest step emits no note"
        );
    }
}
