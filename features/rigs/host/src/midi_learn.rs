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
            Self::Note { channel, number } => {
                format!("{} · ch {}", midi::note_name(number), channel + 1)
            }
        }
    }

    /// The trigger a message comes from and whether it is a press, for the
    /// messages a switch can be: CCs and notes.
    #[must_use]
    pub fn from_raw(status: u8, d1: u8, d2: u8) -> Option<(Self, bool)> {
        let channel = status & 0x0f;
        match status & 0xf0 {
            0xb0 => Some((
                Self::Cc {
                    channel,
                    number: d1,
                },
                d2 >= 64,
            )),
            0x90 => Some((
                Self::Note {
                    channel,
                    number: d1,
                },
                d2 > 0,
            )),
            0x80 => Some((
                Self::Note {
                    channel,
                    number: d1,
                },
                false,
            )),
            _ => None,
        }
    }
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
        Self::load_in(signal_config_dir().join(rig))
    }

    /// The table saved in `dir` (empty if none) — for a rig that keeps its
    /// own directory rather than one under the config root, so two rigs in
    /// one process each learn their own pedals.
    #[must_use]
    pub fn load_in(dir: impl Into<PathBuf>) -> Self {
        let store = StyxDir::new(dir);
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
