//! MIDI Editor Action Sets
//!
//! Defines keybindings specific to the MIDI Editor context.
//! These bindings only activate when the MIDI Editor has focus.

use crate::input::keybinds::{ActionSet, Keybind, KeybindContext, WheelBind};

/// Standard REAPER MIDI Editor keybindings
pub struct ReaperMidiEditor;

impl ActionSet for ReaperMidiEditor {
    fn name(&self) -> &'static str {
        "ReaperMidiEditor"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Selection ===
            Keybind::new("<C-a>", "40003")
                .with_context(KeybindContext::Midi)
                .with_description("Select all notes"),
            Keybind::new("<escape>", "40214")
                .with_context(KeybindContext::Midi)
                .with_description("Unselect all"),
            // === Navigation ===
            Keybind::new("<up>", "40138")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes up one semitone"),
            Keybind::new("<down>", "40139")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes down one semitone"),
            Keybind::new("<S-up>", "40140")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes up one octave"),
            Keybind::new("<S-down>", "40141")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes down one octave"),
            Keybind::new("<left>", "40183")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes left by grid"),
            Keybind::new("<right>", "40181")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes right by grid"),
            // === Editing ===
            Keybind::new("<delete>", "40002")
                .with_context(KeybindContext::Midi)
                .with_description("Delete notes"),
            Keybind::new("<backspace>", "40002")
                .with_context(KeybindContext::Midi)
                .with_description("Delete notes"),
            Keybind::new("<C-c>", "40010")
                .with_context(KeybindContext::Midi)
                .with_description("Copy notes"),
            Keybind::new("<C-x>", "40011")
                .with_context(KeybindContext::Midi)
                .with_description("Cut notes"),
            Keybind::new("<C-v>", "40012")
                .with_context(KeybindContext::Midi)
                .with_description("Paste notes"),
            Keybind::new("<C-d>", "40006")
                .with_context(KeybindContext::Midi)
                .with_description("Duplicate notes"),
            // === Quantize ===
            Keybind::new("q", "40039")
                .with_context(KeybindContext::Midi)
                .with_description("Quantize notes to grid"),
            Keybind::new("<S-q>", "40421")
                .with_context(KeybindContext::Midi)
                .with_description("Quantize notes dialog"),
            // === Velocity ===
            Keybind::new("<C-up>", "40462")
                .with_context(KeybindContext::Midi)
                .with_description("Increase velocity"),
            Keybind::new("<C-down>", "40463")
                .with_context(KeybindContext::Midi)
                .with_description("Decrease velocity"),
            // === Length ===
            Keybind::new("<S-left>", "40444")
                .with_context(KeybindContext::Midi)
                .with_description("Shorten note length"),
            Keybind::new("<S-right>", "40443")
                .with_context(KeybindContext::Midi)
                .with_description("Lengthen note length"),
            // === Zoom ===
            Keybind::new("<C-=>", "40111")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom in horizontal"),
            Keybind::new("<C-->", "40112")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom out horizontal"),
            Keybind::new("<C-S-=>", "40113")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom in vertical"),
            Keybind::new("<C-S-->", "40114")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom out vertical"),
            // === Grid ===
            Keybind::new("<M-g>", "40047")
                .with_context(KeybindContext::Midi)
                .with_description("Toggle grid"),
            Keybind::new("<M-s>", "40455")
                .with_context(KeybindContext::Midi)
                .with_description("Toggle snap"),
            // === View ===
            Keybind::new("<C-home>", "40434")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll to start"),
            Keybind::new("<C-end>", "40435")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll to end"),
            // === Tools ===
            Keybind::new("1", "40042")
                .with_context(KeybindContext::Midi)
                .with_description("Select tool: Draw/Select"),
            Keybind::new("2", "40043")
                .with_context(KeybindContext::Midi)
                .with_description("Select tool: Erase"),
            Keybind::new("3", "40044")
                .with_context(KeybindContext::Midi)
                .with_description("Select tool: Paint"),
            Keybind::new("4", "40045")
                .with_context(KeybindContext::Midi)
                .with_description("Select tool: Marquee"),
            // === Split/Join ===
            Keybind::new("s", "40456")
                .with_context(KeybindContext::Midi)
                .with_description("Split notes at cursor"),
            Keybind::new("j", "40456")
                .with_context(KeybindContext::Midi)
                .with_description("Join notes"),
            // === Humanize ===
            Keybind::new("h", "40422")
                .with_context(KeybindContext::Midi)
                .with_description("Humanize notes"),
        ]
    }

    fn wheel_binds(&self) -> Vec<WheelBind> {
        vec![
            // Vertical scroll with wheel (no modifiers)
            WheelBind::new("", "40432")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll view vertically"),
            // Horizontal scroll with Shift+wheel
            WheelBind::new("<S->", "40433")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll horizontally (Shift+wheel)"),
        ]
    }
}

/// Logic Pro style MIDI Editor keybindings
pub struct LogicMidiEditor;

impl ActionSet for LogicMidiEditor {
    fn name(&self) -> &'static str {
        "LogicMidiEditor"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Selection ===
            Keybind::new("<D-a>", "40003")
                .with_context(KeybindContext::Midi)
                .with_description("Select all notes"),
            Keybind::new("<escape>", "40214")
                .with_context(KeybindContext::Midi)
                .with_description("Unselect all"),
            // === Note Movement (Logic uses Option+arrow) ===
            Keybind::new("<M-up>", "40138")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes up one semitone"),
            Keybind::new("<M-down>", "40139")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes down one semitone"),
            Keybind::new("<M-S-up>", "40140")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes up one octave"),
            Keybind::new("<M-S-down>", "40141")
                .with_context(KeybindContext::Midi)
                .with_description("Move notes down one octave"),
            // === Editing ===
            Keybind::new("<delete>", "40002")
                .with_context(KeybindContext::Midi)
                .with_description("Delete notes"),
            Keybind::new("<backspace>", "40002")
                .with_context(KeybindContext::Midi)
                .with_description("Delete notes"),
            Keybind::new("<D-c>", "40010")
                .with_context(KeybindContext::Midi)
                .with_description("Copy notes"),
            Keybind::new("<D-x>", "40011")
                .with_context(KeybindContext::Midi)
                .with_description("Cut notes"),
            Keybind::new("<D-v>", "40012")
                .with_context(KeybindContext::Midi)
                .with_description("Paste notes"),
            Keybind::new("<D-d>", "40006")
                .with_context(KeybindContext::Midi)
                .with_description("Duplicate notes"),
            // === Quantize ===
            Keybind::new("q", "40039")
                .with_context(KeybindContext::Midi)
                .with_description("Quantize notes to grid"),
            // === Velocity (Logic style) ===
            Keybind::new("<M-C-up>", "40462")
                .with_context(KeybindContext::Midi)
                .with_description("Increase velocity"),
            Keybind::new("<M-C-down>", "40463")
                .with_context(KeybindContext::Midi)
                .with_description("Decrease velocity"),
            // === Length ===
            Keybind::new("<S-\\>", "40443")
                .with_context(KeybindContext::Midi)
                .with_description("Force legato (stretch to next note)"),
            Keybind::new("\\", "40444")
                .with_context(KeybindContext::Midi)
                .with_description("Remove overlap"),
            // === Zoom (Logic uses Cmd+arrow) ===
            Keybind::new("<D-right>", "40111")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom in horizontal"),
            Keybind::new("<D-left>", "40112")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom out horizontal"),
            Keybind::new("<D-up>", "40113")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom in vertical"),
            Keybind::new("<D-down>", "40114")
                .with_context(KeybindContext::Midi)
                .with_description("Zoom out vertical"),
            // === Tools (Logic style) ===
            Keybind::new("t", "40042")
                .with_context(KeybindContext::Midi)
                .with_description("Pointer tool"),
            Keybind::new("p", "40044")
                .with_context(KeybindContext::Midi)
                .with_description("Pencil tool"),
            Keybind::new("e", "40043")
                .with_context(KeybindContext::Midi)
                .with_description("Eraser tool"),
            // TODO: Logic has many more MIDI-specific shortcuts
            // - Transform functions
            // - Step input mode
            // - MIDI Functions menu equivalents
        ]
    }

    fn wheel_binds(&self) -> Vec<WheelBind> {
        vec![
            // Vertical scroll with wheel
            WheelBind::new("", "40432")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll view vertically"),
            // Horizontal scroll with Shift+wheel
            WheelBind::new("<S->", "40433")
                .with_context(KeybindContext::Midi)
                .with_description("Scroll horizontally"),
        ]
    }
}
