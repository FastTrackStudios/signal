//! MIDI → action trigger mapping for signal navigation.
//!
//! Maps incoming MIDI messages to `ActionId`s so that foot controllers,
//! expression pedals, and other MIDI devices can drive signal navigation
//! without being hardwired to a specific device layout.
//!
//! # Default bindings
//!
//! | MIDI message              | Action              |
//! |---------------------------|---------------------|
//! | CC 102, value > 0         | Next Song           |
//! | CC 103, value > 0         | Previous Song       |
//! | CC 104, value > 0         | Next Section        |
//! | CC 105, value > 0         | Previous Section    |
//! | Program Change 0–23       | Load Variant 1–24   |
//!
//! CC 102–105 are in the "undefined" range (102–119) of the MIDI spec,
//! chosen to avoid collisions with common controller assignments.
//! Program change 0–23 maps directly to variants 1–24.
//!
//! All bindings are user-replaceable via [`MidiActionMap::set`].

use serde::{Deserialize, Serialize};

use crate::actions::{load_variant_action, signal_actions};

// ── Trigger types ─────────────────────────────────────────────────────────────

/// A MIDI message pattern that can trigger an action.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiActionTrigger {
    /// A MIDI Note On message with any velocity > 0.
    ///
    /// `channel`: `None` = omni (match any channel).
    NoteOn { channel: Option<u8>, note: u8 },

    /// A MIDI Note Off (or Note On with velocity 0).
    ///
    /// `channel`: `None` = omni.
    NoteOff { channel: Option<u8>, note: u8 },

    /// A Control Change message.
    ///
    /// Fires when the CC value satisfies `threshold`:
    /// - `ButtonHigh` — value >= 64 (sustain-pedal style, re-triggers on each high message)
    /// - `ButtonAny` — any value > 0 (momentary press)
    /// - `ButtonToggle` — alternates between on/off on each message > 0
    ///
    /// `channel`: `None` = omni.
    ControlChange {
        channel: Option<u8>,
        cc: u8,
        threshold: CcThreshold,
    },

    /// A Program Change message where the program number equals `program`.
    ///
    /// `channel`: `None` = omni.
    ProgramChange { channel: Option<u8>, program: u8 },
}

impl MidiActionTrigger {
    /// Convenience: omni CC trigger that fires on any value > 0.
    pub const fn cc_any(cc: u8) -> Self {
        Self::ControlChange {
            channel: None,
            cc,
            threshold: CcThreshold::ButtonAny,
        }
    }

    /// Convenience: omni program change trigger.
    pub const fn program(program: u8) -> Self {
        Self::ProgramChange {
            channel: None,
            program,
        }
    }

    /// Convenience: omni note-on trigger.
    pub const fn note_on(note: u8) -> Self {
        Self::NoteOn {
            channel: None,
            note,
        }
    }
}

/// When a CC trigger fires relative to its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CcThreshold {
    /// Fire when value >= 64 (sustain-pedal convention).
    ButtonHigh,
    /// Fire on any value > 0.
    ButtonAny,
}

// ── Mapping entry ─────────────────────────────────────────────────────────────

/// One entry in the MIDI action map: a trigger bound to an action ID string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiActionBinding {
    /// The MIDI message pattern that fires the action.
    pub trigger: MidiActionTrigger,
    /// The action ID to fire (matches a `StaticActionId::id()` string).
    pub action_id: String,
}

impl MidiActionBinding {
    pub fn new(trigger: MidiActionTrigger, action_id: &str) -> Self {
        Self {
            trigger,
            action_id: action_id.to_string(),
        }
    }
}

// ── Map ───────────────────────────────────────────────────────────────────────

/// A collection of MIDI → action bindings.
///
/// Start with [`MidiActionMap::default()`] for the built-in bindings, then
/// call [`set`] / [`remove`] to customise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiActionMap {
    bindings: Vec<MidiActionBinding>,
}

impl Default for MidiActionMap {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl MidiActionMap {
    /// Create an empty map (no bindings).
    pub fn empty() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Create a map pre-loaded with the default bindings (see module docs).
    pub fn with_defaults() -> Self {
        let mut map = Self::empty();

        // Navigation
        map.add(
            MidiActionTrigger::cc_any(102),
            signal_actions::NEXT_SONG.as_str(),
        );
        map.add(
            MidiActionTrigger::cc_any(103),
            signal_actions::PREVIOUS_SONG.as_str(),
        );
        map.add(
            MidiActionTrigger::cc_any(104),
            signal_actions::NEXT_SECTION.as_str(),
        );
        map.add(
            MidiActionTrigger::cc_any(105),
            signal_actions::PREVIOUS_SECTION.as_str(),
        );

        // Load Variant 1–24 via Program Change 0–23
        for i in 0u8..24 {
            if let Some(action) = load_variant_action((i + 1) as usize) {
                map.add(MidiActionTrigger::program(i), action.as_str());
            }
        }

        map
    }

    /// Add a binding. If a binding for the same trigger already exists it is replaced.
    pub fn add(&mut self, trigger: MidiActionTrigger, action_id: &str) {
        if let Some(existing) = self.bindings.iter_mut().find(|b| b.trigger == trigger) {
            existing.action_id = action_id.to_string();
        } else {
            self.bindings
                .push(MidiActionBinding::new(trigger, action_id));
        }
    }

    /// Bind a trigger to a different action, replacing any existing binding for that trigger.
    pub fn set(&mut self, trigger: MidiActionTrigger, action_id: &str) {
        self.add(trigger, action_id);
    }

    /// Remove all bindings for a given trigger.
    pub fn remove(&mut self, trigger: &MidiActionTrigger) {
        self.bindings.retain(|b| &b.trigger != trigger);
    }

    /// Find the action ID bound to a trigger, if any.
    pub fn find(&self, trigger: &MidiActionTrigger) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| &b.trigger == trigger)
            .map(|b| b.action_id.as_str())
    }

    /// All bindings in this map.
    pub fn bindings(&self) -> &[MidiActionBinding] {
        &self.bindings
    }

    /// Iterate over all bindings whose action matches `action_id`.
    pub fn triggers_for<'a>(
        &'a self,
        action_id: &'a str,
    ) -> impl Iterator<Item = &'a MidiActionTrigger> {
        self.bindings
            .iter()
            .filter(move |b| b.action_id == action_id)
            .map(|b| &b.trigger)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::signal_actions;

    #[test]
    fn default_map_has_navigation_bindings() {
        let map = MidiActionMap::with_defaults();
        assert_eq!(
            map.find(&MidiActionTrigger::cc_any(102)),
            Some(signal_actions::NEXT_SONG.as_str())
        );
        assert_eq!(
            map.find(&MidiActionTrigger::cc_any(103)),
            Some(signal_actions::PREVIOUS_SONG.as_str())
        );
        assert_eq!(
            map.find(&MidiActionTrigger::cc_any(104)),
            Some(signal_actions::NEXT_SECTION.as_str())
        );
        assert_eq!(
            map.find(&MidiActionTrigger::cc_any(105)),
            Some(signal_actions::PREVIOUS_SECTION.as_str())
        );
    }

    #[test]
    fn default_map_has_24_variant_program_changes() {
        let map = MidiActionMap::with_defaults();
        for i in 0u8..24 {
            let trigger = MidiActionTrigger::program(i);
            let action = map
                .find(&trigger)
                .expect(&format!("PC {i} should be bound"));
            assert!(
                action.starts_with("fts.signal.load_variant."),
                "PC {i} → {action}"
            );
        }
    }

    #[test]
    fn program_change_25_is_not_bound() {
        let map = MidiActionMap::with_defaults();
        assert!(map.find(&MidiActionTrigger::program(24)).is_none());
    }

    #[test]
    fn set_replaces_existing_binding() {
        let mut map = MidiActionMap::with_defaults();
        map.set(MidiActionTrigger::cc_any(102), "custom.action");
        assert_eq!(
            map.find(&MidiActionTrigger::cc_any(102)),
            Some("custom.action")
        );
        // Should not have duplicates
        let count = map
            .bindings()
            .iter()
            .filter(|b| b.trigger == MidiActionTrigger::cc_any(102))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn remove_clears_binding() {
        let mut map = MidiActionMap::with_defaults();
        map.remove(&MidiActionTrigger::cc_any(104));
        assert!(map.find(&MidiActionTrigger::cc_any(104)).is_none());
    }

    #[test]
    fn triggers_for_finds_all_variant_actions() {
        let map = MidiActionMap::with_defaults();
        let variant_1_id = signal_actions::LOAD_VARIANT_1.as_str();
        let triggers: Vec<_> = map.triggers_for(variant_1_id).collect();
        assert_eq!(triggers.len(), 1);
        assert_eq!(*triggers[0], MidiActionTrigger::program(0));
    }

    #[test]
    fn load_variant_action_bounds() {
        use crate::actions::load_variant_action;
        assert!(load_variant_action(0).is_none());
        assert!(load_variant_action(1).is_some());
        assert!(load_variant_action(24).is_some());
        assert!(load_variant_action(25).is_none());
    }

    #[test]
    fn empty_map_has_no_bindings() {
        let map = MidiActionMap::empty();
        assert!(map.bindings().is_empty());
    }
}
