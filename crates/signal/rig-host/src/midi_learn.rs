//! **MIDI learn** for a rig's switches — right-click a switch, "MIDI learn",
//! press a pedal or pad, and from then on that pedal presses that switch.
//!
//! One table per rig: triggers (a CC or a note, on a channel) bound to
//! targets, which are the rig's own strings (`"stack:2"`, `"tap"`). The
//! table is the rig's to interpret — this module only learns, persists and
//! matches. Feed it every short message ([`MidiLearn::on_raw`]); it says
//! whether the message was learned, fires a target, or is none of its
//! business.
//!
//! A pedal presses on a CC at 64 or above (footswitches send 127 down, 0
//! up) or a note-on; its release is the CC below 64 or the note-off. A
//! bound trigger's messages — press and release — are the switch's, so the
//! rig should not also play them.
//!
//! Persisted per rig at `~/.config/signal/<rig>/midi-learn.styx`.

use std::path::PathBuf;

use facet::Facet;

use crate::store::{StyxDir, signal_config_dir};

/// What a pedal or pad sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum Trigger {
    /// A control change: channel (0–15), controller number.
    Cc { channel: u8, number: u8 },
    /// A note: channel (0–15), key.
    Note { channel: u8, number: u8 },
}

impl Trigger {
    /// For a switch's badge and menu: `CC 101 · ch 1`, `C2 · ch 10`.
    #[must_use]
    pub fn label(&self) -> String {
        match *self {
            Self::Cc { channel, number } => format!("CC {number} · ch {}", channel + 1),
            Self::Note { channel, number } => format!("{} · ch {}", note_name(number), channel + 1),
        }
    }

    /// The trigger a message comes from and whether it is a press, for the
    /// messages a switch can be: CCs and notes.
    #[must_use]
    pub fn from_raw(status: u8, d1: u8, d2: u8) -> Option<(Self, bool)> {
        let channel = status & 0x0f;
        match status & 0xf0 {
            0xb0 => Some((Self::Cc { channel, number: d1 }, d2 >= 64)),
            0x90 => Some((Self::Note { channel, number: d1 }, d2 > 0)),
            0x80 => Some((Self::Note { channel, number: d1 }, false)),
            _ => None,
        }
    }
}

fn note_name(key: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[usize::from(key % 12)], i32::from(key / 12) - 1)
}

/// One binding: this trigger presses this target.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct Binding {
    pub trigger: Trigger,
    pub target: String,
}

/// The persisted table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
struct LearnFile {
    #[facet(default)]
    bindings: Vec<Binding>,
}

/// What a message meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Not a switch's: play it.
    Pass,
    /// It was learned onto `target` (the table changed; save done).
    Learned { target: String },
    /// A bound pedal went down (`pressed`) or up.
    Fire { target: String, pressed: bool },
}

/// A rig's MIDI learn table and learn state.
#[derive(Debug)]
pub struct MidiLearn {
    store: Option<StyxDir>,
    bindings: Vec<Binding>,
    learning: Option<String>,
}

const FILE: &str = "midi-learn.styx";

impl MidiLearn {
    /// The table saved for `rig` (empty if none).
    #[must_use]
    pub fn load(rig: &str) -> Self {
        let store = StyxDir::new(signal_config_dir().join(rig));
        let file: LearnFile = store.read(FILE).unwrap_or_default();
        Self {
            store: Some(store),
            bindings: file.bindings,
            learning: None,
        }
    }

    /// A table that is never saved (tests, headless tools).
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            store: None,
            bindings: Vec::new(),
            learning: None,
        }
    }

    /// Where it is saved, if anywhere.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|s| s.dir().join(FILE))
    }

    /// Learn the next pedal or pad pressed onto `target`.
    pub fn learn(&mut self, target: &str) {
        self.learning = Some(target.to_string());
    }

    /// Stop learning without binding anything.
    pub fn cancel(&mut self) {
        self.learning = None;
    }

    /// Unbind `target`.
    pub fn clear(&mut self, target: &str) {
        let before = self.bindings.len();
        self.bindings.retain(|b| b.target != target);
        if self.bindings.len() != before {
            self.save();
        }
    }

    /// Target `{prefix}{removed}` is gone and the ones numbered after it
    /// moved down one (a deleted stack): its pedal is freed, theirs follow
    /// them.
    pub fn renumber(&mut self, prefix: &str, removed: usize) {
        let index = |t: &str| t.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok());
        let before = self.bindings.clone();
        self.bindings.retain(|b| index(&b.target) != Some(removed));
        for b in &mut self.bindings {
            if let Some(n) = index(&b.target).filter(|&n| n > removed) {
                b.target = format!("{prefix}{}", n - 1);
            }
        }
        if self.learning.as_deref().and_then(index) == Some(removed) {
            self.learning = None;
        }
        if self.bindings != before {
            self.save();
        }
    }

    /// The target being learned.
    #[must_use]
    pub fn learning(&self) -> Option<&str> {
        self.learning.as_deref()
    }

    /// Every binding, as `(target, trigger label)`.
    #[must_use]
    pub fn labels(&self) -> Vec<(String, String)> {
        self.bindings
            .iter()
            .map(|b| (b.target.clone(), b.trigger.label()))
            .collect()
    }

    /// The bindings.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Feed one short message.
    pub fn on_raw(&mut self, status: u8, d1: u8, d2: u8) -> Outcome {
        let Some((trigger, pressed)) = Trigger::from_raw(status, d1, d2) else {
            return Outcome::Pass;
        };
        // Learning takes the first PRESS — a release left over from the
        // click that started it is not a choice.
        if pressed {
            if let Some(target) = self.learning.take() {
                // One pedal does one thing, and learning a switch again
                // replaces what it had.
                self.bindings
                    .retain(|b| b.trigger != trigger && b.target != target);
                self.bindings.push(Binding {
                    trigger,
                    target: target.clone(),
                });
                self.save();
                return Outcome::Learned { target };
            }
        }
        match self.bindings.iter().find(|b| b.trigger == trigger) {
            Some(b) => Outcome::Fire {
                target: b.target.clone(),
                pressed,
            },
            None => Outcome::Pass,
        }
    }

    fn save(&self) {
        if let Some(store) = &self.store {
            store.write(
                FILE,
                &LearnFile {
                    bindings: self.bindings.clone(),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_learned_pedal_presses_its_switch_and_lets_go() {
        let mut m = MidiLearn::in_memory();
        assert_eq!(m.on_raw(0xb0, 101, 127), Outcome::Pass);
        m.learn("stack:2");
        // The release of whatever was down is not learned.
        assert_eq!(m.on_raw(0xb0, 101, 0), Outcome::Pass);
        assert_eq!(m.learning(), Some("stack:2"));
        assert_eq!(
            m.on_raw(0xb0, 101, 127),
            Outcome::Learned { target: "stack:2".into() }
        );
        assert_eq!(m.learning(), None);
        assert_eq!(
            m.on_raw(0xb0, 101, 127),
            Outcome::Fire { target: "stack:2".into(), pressed: true }
        );
        assert_eq!(
            m.on_raw(0xb0, 101, 0),
            Outcome::Fire { target: "stack:2".into(), pressed: false }
        );
        // Another channel is another pedal.
        assert_eq!(m.on_raw(0xb1, 101, 127), Outcome::Pass);
    }

    #[test]
    fn pads_learn_as_notes() {
        let mut m = MidiLearn::in_memory();
        m.learn("tap");
        assert_eq!(m.on_raw(0x99, 36, 100), Outcome::Learned { target: "tap".into() });
        assert_eq!(m.labels(), vec![("tap".to_string(), "C2 · ch 10".to_string())]);
        assert_eq!(m.on_raw(0x89, 36, 0), Outcome::Fire { target: "tap".into(), pressed: false });
        assert_eq!(m.on_raw(0x99, 36, 0), Outcome::Fire { target: "tap".into(), pressed: false });
    }

    #[test]
    fn relearning_moves_a_pedal_and_replaces_a_switch() {
        let mut m = MidiLearn::in_memory();
        m.learn("stack:0");
        m.on_raw(0xb0, 20, 127);
        // The same pedal onto another switch: it leaves the first.
        m.learn("stack:1");
        m.on_raw(0xb0, 20, 127);
        assert_eq!(m.bindings().len(), 1);
        assert_eq!(m.bindings()[0].target, "stack:1");
        // Another pedal onto that switch: it replaces the first pedal.
        m.learn("stack:1");
        m.on_raw(0xb0, 21, 127);
        assert_eq!(m.labels(), vec![("stack:1".to_string(), "CC 21 · ch 1".to_string())]);
        m.clear("stack:1");
        assert!(m.bindings().is_empty());
    }

    #[test]
    fn deleting_a_stack_frees_its_pedal_and_moves_the_rest_down() {
        let mut m = MidiLearn::in_memory();
        for (i, cc) in [(0, 10), (1, 11), (2, 12)] {
            m.learn(&format!("stack:{i}"));
            m.on_raw(0xb0, cc, 127);
        }
        m.learn("tap");
        m.on_raw(0xb0, 13, 127);
        m.renumber("stack:", 1);
        assert_eq!(m.on_raw(0xb0, 11, 127), Outcome::Pass);
        assert_eq!(m.on_raw(0xb0, 12, 127), Outcome::Fire { target: "stack:1".into(), pressed: true });
        assert_eq!(m.on_raw(0xb0, 10, 127), Outcome::Fire { target: "stack:0".into(), pressed: true });
        assert_eq!(m.on_raw(0xb0, 13, 127), Outcome::Fire { target: "tap".into(), pressed: true });
    }

    #[test]
    fn the_table_survives_the_file() {
        let dir = std::env::temp_dir().join(format!("midi-learn-{}", std::process::id()));
        let store = StyxDir::new(&dir);
        let mut m = MidiLearn { store: Some(store.clone()), bindings: Vec::new(), learning: None };
        m.learn("stack:3");
        m.on_raw(0xb2, 64, 127);
        let file: LearnFile = store.read(FILE).expect("saved");
        assert_eq!(file.bindings, m.bindings().to_vec());
        let _ = std::fs::remove_dir_all(dir);
    }
}
