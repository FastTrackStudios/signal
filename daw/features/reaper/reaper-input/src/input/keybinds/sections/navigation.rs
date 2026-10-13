//! Navigation Action Sets
//!
//! Defines cursor movement, track selection, and navigation controls.

use crate::input::keybinds::{ActionSet, Keybind};

/// Standard REAPER navigation (arrow keys)
pub struct ReaperNavigation;

impl ActionSet for ReaperNavigation {
    fn name(&self) -> &'static str {
        "ReaperNavigation"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Track Selection ===
            Keybind::new("<up>", "40286").with_description("Go to previous track"),
            Keybind::new("<down>", "40285").with_description("Go to next track"),
            Keybind::new("<C-up>", "40286").with_description("Select previous track"),
            Keybind::new("<C-down>", "40285").with_description("Select next track"),
            // === Item Navigation ===
            Keybind::new("<tab>", "40319").with_description("Move cursor to next transient"),
            Keybind::new("<S-tab>", "40318").with_description("Move cursor to previous transient"),
            // === Cursor Navigation ===
            Keybind::new("<left>", "40104").with_description("Move cursor left to grid"),
            Keybind::new("<right>", "40105").with_description("Move cursor right to grid"),
            Keybind::new("<C-left>", "40100").with_description("Move cursor left big"),
            Keybind::new("<C-right>", "40099").with_description("Move cursor right big"),
            Keybind::new("<home>", "40042").with_description("Go to start of project"),
            Keybind::new("<end>", "40043").with_description("Go to end of project"),
            // === Time Selection ===
            Keybind::new("<S-left>", "40102").with_description("Extend time selection left"),
            Keybind::new("<S-right>", "40103").with_description("Extend time selection right"),
            // === Markers ===
            Keybind::new(";", "40172").with_description("Go to previous marker/project start"),
            Keybind::new("'", "40173").with_description("Go to next marker/project end"),
            Keybind::new("m", "40157").with_description("Insert marker at cursor"),
            Keybind::new("<S-m>", "40171").with_description("Insert and name marker"),
            // === Marker Numbers ===
            Keybind::new("1", "40161").with_description("Go to marker 1"),
            Keybind::new("2", "40162").with_description("Go to marker 2"),
            Keybind::new("3", "40163").with_description("Go to marker 3"),
            Keybind::new("4", "40164").with_description("Go to marker 4"),
            Keybind::new("5", "40165").with_description("Go to marker 5"),
            Keybind::new("6", "40166").with_description("Go to marker 6"),
            Keybind::new("7", "40167").with_description("Go to marker 7"),
            Keybind::new("8", "40168").with_description("Go to marker 8"),
            Keybind::new("9", "40169").with_description("Go to marker 9"),
            Keybind::new("0", "40170").with_description("Go to marker 10"),
            // === Regions ===
            Keybind::new("<S-r>", "40174").with_description("Insert region from time selection"),
            Keybind::new("<C-S-r>", "40306").with_description("Insert region from selected items"),
            // === Loop Points ===
            Keybind::new("<C-S-l>", "40222").with_description("Set loop start point"),
            Keybind::new("<C-S-r>", "40223").with_description("Set loop end point"),
            // === Scroll ===
            Keybind::new("<C-home>", "40632").with_description("Go to start of loop"),
            Keybind::new("<C-end>", "40633").with_description("Go to end of loop"),
        ]
    }
}

/// Logic Pro style navigation
pub struct LogicNavigation;

impl ActionSet for LogicNavigation {
    fn name(&self) -> &'static str {
        "LogicNavigation"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Track Selection ===
            Keybind::new("<up>", "40286").with_description("Go to previous track"),
            Keybind::new("<down>", "40285").with_description("Go to next track"),
            Keybind::new("<M-up>", "40286").with_description("Select previous track"),
            Keybind::new("<M-down>", "40285").with_description("Select next track"),
            Keybind::new("m", "40281").with_description("Mute/unmute tracks"),
            Keybind::new("s", "40280").with_description("Solo/unsolo tracks"),
            Keybind::new("<C-i>", "40495").with_description("Toggle track input monitor"),
            Keybind::new("<S-r>", "40490").with_description("Arm all tracks for recording"),
            // === Item/Region Selection ===
            Keybind::new("<left>", "40416").with_description("Select previous item"),
            Keybind::new("<right>", "40417").with_description("Select next item"),
            Keybind::new("<S-left>", "40421").with_description("Extend selection to previous item"),
            Keybind::new("<S-right>", "40421").with_description("Extend selection to next item"),
            // === Selection Commands (Logic Shift+letter) ===
            Keybind::new("<S-f>", "40421").with_description("Select all following"),
            Keybind::new("<S-l>", "40717")
                .with_description("Select all items in current time selection"),
            Keybind::new("<S-o>", "40528").with_description("Select all items on selected tracks"),
            // TODO: Shift+M select muted items - needs custom action
            // TODO: Shift+C select same-colored items - needs custom action
            // TODO: Shift+E select equal regions - needs custom action
            // TODO: Shift+S select similar regions - needs custom action

            // === Markers ===
            Keybind::new(";", "40172").with_description("Go to previous marker"),
            Keybind::new("'", "40173").with_description("Go to next marker"),
            Keybind::new("<M-'>", "40157").with_description("Insert marker at cursor"),
            Keybind::new("<S-'>", "40171").with_description("Edit/rename marker near cursor"),
            Keybind::new("<M-delete>", "40613").with_description("Delete marker near cursor"),
            Keybind::new("<M-S-'>", "40171").with_description("Create marker for selection"),
            // === Numpad Markers (Logic style) ===
            Keybind::new("<kp_1>", "40161").with_description("Go to marker 1"),
            Keybind::new("<kp_2>", "40162").with_description("Go to marker 2"),
            Keybind::new("<kp_3>", "40163").with_description("Go to marker 3"),
            Keybind::new("<kp_4>", "40164").with_description("Go to marker 4"),
            Keybind::new("<kp_5>", "40165").with_description("Go to marker 5"),
            Keybind::new("<kp_6>", "40166").with_description("Go to marker 6"),
            Keybind::new("<kp_7>", "40167").with_description("Go to marker 7"),
            Keybind::new("<kp_8>", "40168").with_description("Go to marker 8"),
            Keybind::new("<kp_9>", "40169").with_description("Go to marker 9"),
            // === Cmd+Number Markers ===
            Keybind::new("<D-1>", "40161").with_description("Go to marker 1"),
            Keybind::new("<D-2>", "40162").with_description("Go to marker 2"),
            Keybind::new("<D-3>", "40163").with_description("Go to marker 3"),
            Keybind::new("<D-4>", "40164").with_description("Go to marker 4"),
            Keybind::new("<D-5>", "40165").with_description("Go to marker 5"),
            Keybind::new("<D-6>", "40166").with_description("Go to marker 6"),
            Keybind::new("<D-7>", "40167").with_description("Go to marker 7"),
            Keybind::new("<D-8>", "40168").with_description("Go to marker 8"),
            Keybind::new("<D-9>", "40169").with_description("Go to marker 9"),
            // === Loop/Locators ===
            Keybind::new("u", "40290").with_description("Set time selection to items"),
            Keybind::new("<D-u>", "40290")
                .with_description("Set locators by selection and enable cycle"),
            Keybind::new("<S-D-.>", "40039").with_description("Move loop points later"),
            Keybind::new("<S-D-,>", "40038").with_description("Move loop points earlier"),
            Keybind::new("<M-l>", "40632").with_description("Go to start of loop"),
            Keybind::new("<S-M-l>", "40633").with_description("Go to end of loop"),
            // === Bar Navigation ===
            Keybind::new("<", "40646").with_description("Move cursor left to grid"),
            Keybind::new(">", "40647").with_description("Move cursor right to grid"),
            // === Scroll to Selection ===
            Keybind::new("<S-`>", "40913").with_description("Scroll view to edit cursor"),
        ]
    }
}
