//! Views and Windows Action Sets
//!
//! Defines window management, views, and automation controls.

use crate::input::keybinds::{ActionSet, Keybind};

/// Standard REAPER views
pub struct ReaperViews;

impl ActionSet for ReaperViews {
    fn name(&self) -> &'static str {
        "ReaperViews"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Windows ===
            Keybind::new("<C-M-m>", "40078").with_description("Toggle mixer visible"),
            Keybind::new("<M-v>", "40549").with_description("Show/hide all floating FX windows"),
            Keybind::new("<C-M-x>", "40271").with_description("Show media explorer"),
            // === Actions ===
            Keybind::new("?", "40420").with_description("Show action list"),
            Keybind::new("<F4>", "40420").with_description("Show action list"),
            Keybind::new("<S-F1>", "40378").with_description("Show keyboard shortcut list"),
            // === Project Tabs ===
            Keybind::new("<C-w>", "40031").with_description("Close current project tab"),
            Keybind::new("<C-tab>", "40861").with_description("Next project tab"),
            Keybind::new("<C-S-tab>", "40862").with_description("Previous project tab"),
            // === File Operations ===
            Keybind::new("<C-n>", "40023").with_description("New project"),
            Keybind::new("<C-o>", "40025").with_description("Open project"),
            Keybind::new("<C-s>", "40026").with_description("Save project"),
            Keybind::new("<C-M-s>", "40022").with_description("Save project as"),
            // === Render ===
            Keybind::new("<C-M-r>", "40015").with_description("Render project to disk"),
            // === Automation ===
            Keybind::new("a", "40400").with_description("Toggle automation visible in arrange"),
            Keybind::new("v", "40406").with_description("Show volume envelope for tracks"),
            Keybind::new("<S-v>", "40407").with_description("Show pan envelope for tracks"),
            // === Snap/Grid ===
            Keybind::new("<M-s>", "1157").with_description("Toggle snapping"),
            Keybind::new("<M-g>", "40145").with_description("Toggle grid lines"),
            // === Preferences ===
            Keybind::new("<C-p>", "40016").with_description("Open preferences"),
            // === Screensets ===
            Keybind::new("<C-F1>", "40454").with_description("Load screenset 1"),
            Keybind::new("<C-F2>", "40455").with_description("Load screenset 2"),
            Keybind::new("<C-F3>", "40456").with_description("Load screenset 3"),
            Keybind::new("<C-F4>", "40457").with_description("Load screenset 4"),
            Keybind::new("<C-F5>", "40458").with_description("Load screenset 5"),
            Keybind::new("<C-F6>", "40459").with_description("Load screenset 6"),
            Keybind::new("<C-F7>", "40460").with_description("Load screenset 7"),
            Keybind::new("<C-F8>", "40461").with_description("Load screenset 8"),
            Keybind::new("<C-F9>", "40462").with_description("Load screenset 9"),
            Keybind::new("<C-F10>", "40463").with_description("Load screenset 10"),
            // === Fullscreen ===
            Keybind::new("<M-enter>", "40346").with_description("Toggle fullscreen"),
            // === FX ===
            Keybind::new("f", "40291").with_description("Show FX chain for track"),
            Keybind::new("<S-f>", "40549").with_description("Show/hide all FX windows"),
        ]
    }
}

/// Logic Pro style views and windows
pub struct LogicViews;

impl ActionSet for LogicViews {
    fn name(&self) -> &'static str {
        "LogicViews"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Windows ===
            Keybind::new("x", "40078").with_description("Toggle mixer visible"),
            Keybind::new("<D-2>", "40078").with_description("Toggle mixer visible"),
            Keybind::new("p", "40153").with_description("Open item in MIDI editor"),
            Keybind::new("<D-4>", "40153").with_description("Open item in MIDI editor"),
            Keybind::new("<D-5>", "40716").with_description("View: Toggle track/envelope manager"),
            Keybind::new("<D-7>", "40420").with_description("View: Toggle actions window"),
            Keybind::new("y", "40271").with_description("Show media explorer"),
            Keybind::new("o", "40271").with_description("Show media explorer"),
            Keybind::new("v", "40549").with_description("Show/hide all floating FX windows"),
            Keybind::new("<C-D-f>", "40346").with_description("Toggle fullscreen"),
            Keybind::new("<D-w>", "40031").with_description("Close current project tab"),
            Keybind::new("<D-`>", "40861").with_description("Cycle through open windows"),
            // === Global Track Views ===
            Keybind::new("g", "40915").with_description("Toggle global track visibility"),
            // === Automation ===
            Keybind::new("a", "40400").with_description("Toggle automation visible in arrange"),
            // === Automation Envelopes ===
            Keybind::new("<C-M-v>", "40406")
                .with_description("Show volume envelope for all tracks"),
            Keybind::new("<C-M-p>", "40407").with_description("Show pan envelope for all tracks"),
            // === Automation Modes ===
            Keybind::new("<C-D-r>", "40878").with_description("Set all tracks to automation read"),
            Keybind::new("<C-D-t>", "40879").with_description("Set all tracks to automation touch"),
            Keybind::new("<C-D-l>", "40880").with_description("Set all tracks to automation latch"),
            Keybind::new("<S-w>", "40082").with_description("Write current values to automation"),
            Keybind::new("<C-w>", "40082").with_description("Write current values to automation"),
            // === Snap/Grid ===
            Keybind::new("<D-g>", "1157").with_description("Toggle snap to grid"),
            Keybind::new("<M-n>", "1157").with_description("Toggle snapping"),
            // === File Operations ===
            Keybind::new("<D-n>", "40023").with_description("New project from template"),
            Keybind::new("<S-D-n>", "41929").with_description("New empty project"),
            Keybind::new("<D-o>", "40025").with_description("Open project"),
            Keybind::new("<D-s>", "40026").with_description("Save project"),
            Keybind::new("<S-D-s>", "40022").with_description("Save project as"),
            Keybind::new("<D-e>", "40044").with_description("Export selected tracks as audio"),
            // === Bounce/Render ===
            Keybind::new("<D-b>", "40015").with_description("Render project to disk"),
            Keybind::new("<M-D-b>", "41716").with_description("Render project using last settings"),
            // === Color ===
            Keybind::new("<M-c>", "40357").with_description("Open color picker for tracks/items"),
            // Note: Cmd+number shortcuts for markers are in LogicNavigation
            // These conflict with screensets, so we prioritize markers for Logic compatibility
        ]
    }
}
