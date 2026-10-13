//! # reaper-kbd
//!
//! Parser and types for REAPER keyboard mapping (`reaper-kb.ini`) files.
//!
//! This crate can parse the three line types found in REAPER's keyboard
//! configuration files:
//!
//! - **KEY** -- keyboard and MIDI key bindings (with optional global scope)
//! - **ACT** -- custom/composite actions
//! - **SCR** -- script bindings
//!
//! # Example
//!
//! ```
//! use reaper_kbd::KeyMap;
//!
//! let input = "KEY 1 65 40001 0\r\n";
//! let keymap = KeyMap::parse(input).unwrap();
//! let output = keymap.serialize();
//! assert_eq!(input, output);
//! ```

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl KeyMap {
    /// Parse the contents of a `reaper-kb.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of entries in this key map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if this key map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Iterate over only KEY entries.
    pub fn key_bindings(&self) -> impl Iterator<Item = &KeyBinding> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Key(kb) => Some(kb),
            _ => None,
        })
    }

    /// Iterate over only ACT entries.
    pub fn custom_actions(&self) -> impl Iterator<Item = &CustomAction> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Action(a) => Some(a),
            _ => None,
        })
    }

    /// Iterate over only SCR entries.
    pub fn script_bindings(&self) -> impl Iterator<Item = &ScriptBinding> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Script(s) => Some(s),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_key() {
        let input = "KEY 1 65 40001 0\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_key_named() {
        let input = "KEY 0 83 _SWS_SAVE 0\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_key_global() {
        let input = "KEY 1 65 40001 0\r\nKEY 1 65 1 102\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_act() {
        let input = "ACT 3 0 \"_custom_action_1\" \"My Custom Action\" 40001 _SWS_SAVE 40002\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_scr() {
        let input = "SCR 4 0 my_script_id \"Custom: My Script\" myscript.lua\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_mixed() {
        let input = "\
KEY 0 65 40001 0\r\n\
KEY 0 65 1 102\r\n\
ACT 2 32060 \"_my_act\" \"Do Thing\" 40001\r\n\
SCR 4 0 my_scr \"Custom: Test\" test.py\r\n\
KEY 8 32801 40005 0\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.serialize(), input);
    }

    #[test]
    fn round_trip_all_sections() {
        let sections = [
            ("0", Section::Main),
            ("100", Section::MainAlt),
            ("32060", Section::MidiEditor),
            ("32061", Section::MidiEventList),
            ("32062", Section::MidiInlineEditor),
            ("32063", Section::MediaExplorer),
        ];
        for (raw, expected) in &sections {
            let input = format!("KEY 0 65 40001 {}\r\n", raw);
            let km = KeyMap::parse(&input).unwrap();
            match &km.entries[0] {
                Entry::Key(kb) => assert_eq!(&kb.section, expected),
                _ => panic!("expected Key"),
            }
            assert_eq!(km.serialize(), input);
        }
    }

    #[test]
    fn convenience_iterators() {
        let input = "\
KEY 0 65 40001 0\r\n\
ACT 2 0 \"_act\" \"Act\" 40001\r\n\
SCR 4 0 scr \"Scr\" s.lua\r\n\
KEY 0 66 40002 0\r\n";
        let km = KeyMap::parse(input).unwrap();
        assert_eq!(km.len(), 4);
        assert_eq!(km.key_bindings().count(), 2);
        assert_eq!(km.custom_actions().count(), 1);
        assert_eq!(km.script_bindings().count(), 1);
    }

    #[test]
    fn midi_binding_detection() {
        // Note On channel 1, note 60
        let binding = MidiBinding::from_modifier_key(144, 60);
        assert_eq!(
            binding,
            Some(MidiBinding::NoteOn {
                channel: 1,
                note: 60,
            })
        );

        // CC channel 3, cc 7
        let binding = MidiBinding::from_modifier_key(178, 7);
        assert_eq!(
            binding,
            Some(MidiBinding::ControlChange { channel: 3, cc: 7 })
        );

        // Not a MIDI modifier
        let binding = MidiBinding::from_modifier_key(1, 65);
        assert_eq!(binding, None);
    }

    #[test]
    fn unknown_section_round_trips() {
        let input = "KEY 0 65 40001 99999\r\n";
        let km = KeyMap::parse(input).unwrap();
        match &km.entries[0] {
            Entry::Key(kb) => assert_eq!(kb.section, Section::Unknown(99999)),
            _ => panic!("expected Key"),
        }
        assert_eq!(km.serialize(), input);
    }

    // ── SpecialKey tests ──────────────────────────────────────────────────────

    #[test]
    fn special_key_multi_zoom() {
        assert_eq!(SpecialKey::from_key(72), SpecialKey::MultiZoomIn);
        assert_eq!(SpecialKey::from_key(73), SpecialKey::MultiZoomOut);
        assert_eq!(SpecialKey::from_key(74), SpecialKey::MultiZoomReset);
        assert_eq!(SpecialKey::from_key(200), SpecialKey::MultiZoomFingers(1));
        assert_eq!(SpecialKey::from_key(207), SpecialKey::MultiZoomFingers(8));
    }

    #[test]
    fn special_key_multi_rotate() {
        assert_eq!(SpecialKey::from_key(24), SpecialKey::MultiRotateCw);
        assert_eq!(SpecialKey::from_key(25), SpecialKey::MultiRotateCcw);
        assert_eq!(SpecialKey::from_key(152), SpecialKey::MultiRotateFingers(1));
        assert_eq!(SpecialKey::from_key(159), SpecialKey::MultiRotateFingers(8));
    }

    #[test]
    fn special_key_multi_swipe() {
        assert_eq!(SpecialKey::from_key(40), SpecialKey::MultiSwipeRight);
        assert_eq!(SpecialKey::from_key(56), SpecialKey::MultiSwipeLeft);
        assert_eq!(
            SpecialKey::from_key(168),
            SpecialKey::MultiSwipeHorizFingers(1)
        );
        assert_eq!(
            SpecialKey::from_key(175),
            SpecialKey::MultiSwipeHorizFingers(8)
        );
        assert_eq!(
            SpecialKey::from_key(176),
            SpecialKey::MultiSwipeVertFingers(1)
        );
        assert_eq!(
            SpecialKey::from_key(184),
            SpecialKey::MultiSwipeUpFingers(1)
        );
        assert_eq!(
            SpecialKey::from_key(191),
            SpecialKey::MultiSwipeUpFingers(8)
        );
    }

    #[test]
    fn special_key_mousewheel() {
        assert_eq!(SpecialKey::from_key(88), SpecialKey::MousewheelHorizRight);
        assert_eq!(SpecialKey::from_key(216), SpecialKey::MousewheelHorizN(1));
        assert_eq!(SpecialKey::from_key(223), SpecialKey::MousewheelHorizN(8));
        assert_eq!(SpecialKey::from_key(120), SpecialKey::MousewheelUp);
        assert_eq!(SpecialKey::from_key(121), SpecialKey::MousewheelDown);
        assert_eq!(SpecialKey::from_key(122), SpecialKey::MousewheelUpFast);
        assert_eq!(SpecialKey::from_key(123), SpecialKey::MousewheelDownFast);
        assert_eq!(SpecialKey::from_key(248), SpecialKey::MousewheelN(1));
        assert_eq!(SpecialKey::from_key(255), SpecialKey::MousewheelN(8));
    }

    #[test]
    fn special_key_media_keyboard() {
        assert_eq!(SpecialKey::from_key(232), SpecialKey::MediaKbdPlay);
        assert_eq!(SpecialKey::from_key(488), SpecialKey::MediaKbdStop);
        assert_eq!(SpecialKey::from_key(744), SpecialKey::MediaKbdPrev);
        assert_eq!(SpecialKey::from_key(1000), SpecialKey::MediaKbdNext);
        assert_eq!(SpecialKey::from_key(1256), SpecialKey::MediaKbdFastForward);
        assert_eq!(SpecialKey::from_key(1512), SpecialKey::MediaKbdRewind);
        assert_eq!(SpecialKey::from_key(1768), SpecialKey::MediaKbdRecord);
        assert_eq!(SpecialKey::from_key(2024), SpecialKey::MediaKbdMute);
        assert_eq!(SpecialKey::from_key(2280), SpecialKey::MediaKbdVolumeUp);
        assert_eq!(SpecialKey::from_key(2536), SpecialKey::MediaKbdVolumeDown);
    }

    #[test]
    fn special_key_unknown() {
        assert_eq!(SpecialKey::from_key(9999), SpecialKey::Unknown(9999));
    }

    #[test]
    fn special_key_round_trip() {
        let keys: &[u16] = &[
            24, 25, 40, 56, 72, 73, 74, 88, 120, 121, 122, 123, 152, 159, 168, 175, 176, 183, 184,
            191, 200, 207, 216, 223, 232, 248, 255, 488, 744, 1000, 1256, 1512, 1768, 2024, 2280,
            2536,
        ];
        for &k in keys {
            let decoded = SpecialKey::from_key(k);
            assert_eq!(
                decoded.to_key(),
                k,
                "round-trip failed for key {k}: {decoded:?}"
            );
        }
    }

    #[test]
    fn decode_special_key_helper() {
        assert_eq!(decode_special_key(255, 120), Some(SpecialKey::MousewheelUp));
        assert_eq!(decode_special_key(1, 120), None);
    }

    // ── key_name tests ────────────────────────────────────────────────────────

    #[test]
    fn key_name_common_keys() {
        assert_eq!(key_name(8), Some("Backspace"));
        assert_eq!(key_name(9), Some("Tab"));
        assert_eq!(key_name(13), Some("Enter"));
        assert_eq!(key_name(27), Some("Escape"));
        assert_eq!(key_name(32), Some("Space"));
        assert_eq!(key_name(112), Some("F1"));
        assert_eq!(key_name(113), Some("F2"));
        assert_eq!(key_name(123), Some("F12"));
        assert_eq!(key_name(37), Some("Left"));
        assert_eq!(key_name(38), Some("Up"));
        assert_eq!(key_name(39), Some("Right"));
        assert_eq!(key_name(40), Some("Down"));
        assert_eq!(key_name(45), Some("Insert"));
        assert_eq!(key_name(46), Some("Delete"));
        assert_eq!(key_name(33), Some("PageUp"));
        assert_eq!(key_name(34), Some("PageDown"));
        assert_eq!(key_name(36), Some("Home"));
        assert_eq!(key_name(35), Some("End"));
    }

    #[test]
    fn key_name_unknown_returns_none() {
        assert_eq!(key_name(65), None); // 'A' — printable, not in table
        assert_eq!(key_name(0), None);
    }
}
