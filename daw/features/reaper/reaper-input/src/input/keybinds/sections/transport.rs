//! Transport Action Sets
//!
//! Defines play, stop, record, and transport navigation controls.

use crate::input::keybinds::{ActionSet, Keybind};

/// Standard REAPER transport (Space = Play/Stop)
pub struct ReaperTransport;

impl ActionSet for ReaperTransport {
    fn name(&self) -> &'static str {
        "ReaperTransport"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Primary Transport ===
            Keybind::new("<space>", "40044").with_description("Play/stop"),
            Keybind::new("<C-space>", "40073").with_description("Play/Pause"),
            Keybind::new("r", "1013").with_description("Record"),
            Keybind::new("<S-r>", "40046").with_description("Toggle record"),
            // === Navigation ===
            Keybind::new("w", "40042").with_description("Go to start of project"),
            Keybind::new("<C-home>", "40042").with_description("Go to start of project"),
            Keybind::new("<C-end>", "40043").with_description("Go to end of project"),
            // === Rewind/Forward ===
            Keybind::new("<C-->", "40084").with_description("Rewind"),
            Keybind::new("<C-=>", "40085").with_description("Fast forward"),
            // === Loop/Repeat ===
            Keybind::new("l", "1068").with_description("Toggle repeat"),
            // === Metronome ===
            Keybind::new("<M-m>", "40364").with_description("Toggle metronome"),
            // === Tempo ===
            Keybind::new("h", "1134").with_description("Tap tempo"),
            // === Markers ===
            Keybind::new(";", "40172").with_description("Go to previous marker/project start"),
            Keybind::new("'", "40173").with_description("Go to next marker/project end"),
            Keybind::new("m", "40157").with_description("Insert marker at current position"),
            Keybind::new("<S-m>", "40171").with_description("Insert and name marker"),
            // === Regions ===
            Keybind::new("<S-r>", "40174").with_description("Insert region from time selection"),
        ]
    }
}

/// Logic Pro style transport
pub struct LogicTransport;

impl ActionSet for LogicTransport {
    fn name(&self) -> &'static str {
        "LogicTransport"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Primary Transport ===
            Keybind::new("<space>", "40044").with_description("Play/Stop"),
            Keybind::new("<enter>", "40042").with_description("Go to start of project"),
            Keybind::new("r", "1013").with_description("Record"),
            // === Numpad Transport (Logic style) ===
            Keybind::new("<kp_enter>", "1007").with_description("Play"),
            Keybind::new("<kp_0>", "1016").with_description("Stop"),
            Keybind::new("<kp_.>", "1008").with_description("Pause"),
            Keybind::new("<kp_*>", "40046").with_description("Toggle record"),
            // === Scrub/Nudge ===
            Keybind::new(",", "40084").with_description("Rewind a little"),
            Keybind::new(".", "40085").with_description("Fast forward a little"),
            Keybind::new("/", "40222").with_description("Go to next measure"),
            // === Loop/Cycle ===
            Keybind::new("c", "1068").with_description("Toggle repeat/cycle"),
            // === Metronome ===
            Keybind::new("k", "40364").with_description("Toggle metronome"),
            Keybind::new("<S-k>", "40265").with_description("Toggle count-in before recording"),
        ]
    }
}
