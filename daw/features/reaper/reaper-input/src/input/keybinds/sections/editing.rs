//! Editing Action Sets
//!
//! Defines cut, copy, paste, split, delete, and other editing commands.

use crate::input::keybinds::{ActionSet, Keybind};

/// Standard REAPER editing (Ctrl+X/C/V)
pub struct ReaperEditing;

impl ActionSet for ReaperEditing {
    fn name(&self) -> &'static str {
        "ReaperEditing"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Clipboard ===
            Keybind::new("<C-x>", "40059").with_description("Cut items/tracks/envelope points"),
            Keybind::new("<C-c>", "40060").with_description("Copy items/tracks/envelope points"),
            Keybind::new("<C-v>", "40058").with_description("Paste items/tracks"),
            Keybind::new("<C-S-v>", "42398")
                .with_description("Paste items/tracks preserving position"),
            // === Undo/Redo ===
            Keybind::new("<C-z>", "40029").with_description("Undo"),
            Keybind::new("<C-S-z>", "40030").with_description("Redo"),
            Keybind::new("<C-M-z>", "40030").with_description("Redo"),
            // === Selection ===
            Keybind::new("<C-a>", "40182").with_description("Select all items"),
            Keybind::new("<escape>", "40289").with_description("Unselect all items"),
            // === Split ===
            Keybind::new("s", "40757").with_description("Split items at edit cursor"),
            Keybind::new("<S-s>", "40061").with_description("Split items at time selection"),
            // === Delete ===
            Keybind::new("<delete>", "40006").with_description("Remove selected items"),
            Keybind::new("<backspace>", "40006").with_description("Remove selected items"),
            // === Duplicate ===
            Keybind::new("<C-d>", "41295").with_description("Duplicate items"),
            // === Glue ===
            Keybind::new("<C-u>", "40362").with_description("Glue items"),
            Keybind::new("<C-S-g>", "40362").with_description("Glue items (smart)"),
            // === Grouping ===
            Keybind::new("<C-g>", "40032").with_description("Group items"),
            Keybind::new("<C-S-g>", "40033").with_description("Remove items from group"),
            Keybind::new("g", "40034").with_description("Select all items in group"),
            // === MIDI Editor ===
            Keybind::new("e", "40153").with_description("Open items in MIDI editor"),
            Keybind::new("<C-M-e>", "40009").with_description("Open items in external editor"),
            // === Track Controls ===
            Keybind::new("<F5>", "40281").with_description("Toggle track mute"),
            Keybind::new("<F6>", "40280").with_description("Toggle track solo"),
            Keybind::new("<F7>", "40490").with_description("Toggle track record arm"),
            Keybind::new("<F8>", "40495").with_description("Cycle track record monitoring"),
            Keybind::new("<F9>", "40410").with_description("Toggle track phase"),
            // === Item Properties ===
            Keybind::new("<C-f2>", "40652").with_description("Rename item"),
            Keybind::new("<F2>", "40340").with_description("Item properties"),
            // === Quantize ===
            Keybind::new("<C-S-q>", "40070").with_description("Quantize item positions to grid"),
            // === Nudge ===
            Keybind::new("<C-M-left>", "40794").with_description("Nudge items left by grid"),
            Keybind::new("<C-M-right>", "40793").with_description("Nudge items right by grid"),
            // === Track Management ===
            Keybind::new("<C-t>", "40001").with_description("Insert new track"),
            Keybind::new("<C-S-t>", "40001").with_description("Insert and name new track"),
            // === Trim ===
            Keybind::new("<C-S-left>", "40225")
                .with_description("Trim left edge of item to edit cursor"),
            Keybind::new("<C-S-right>", "40226")
                .with_description("Trim right edge of item to edit cursor"),
            // === Fade ===
            Keybind::new("d", "40509").with_description("Apply default fade-in to items"),
            Keybind::new("<S-d>", "40510").with_description("Apply default fade-out to items"),
            Keybind::new("<C-S-d>", "40511").with_description("Apply default crossfade to items"),
            // === Item Timebase ===
            Keybind::new("<M-t>", "40516").with_description("Toggle item timebase"),
            // === Reverse ===
            Keybind::new("<C-r>", "41051").with_description("Reverse items as new take"),
        ]
    }
}

/// Logic Pro style editing
pub struct LogicEditing;

impl ActionSet for LogicEditing {
    fn name(&self) -> &'static str {
        "LogicEditing"
    }

    fn keybinds(&self) -> Vec<Keybind> {
        vec![
            // === Split/Delete ===
            Keybind::new("t", "40757").with_description("Split items at edit cursor"),
            Keybind::new("<D-t>", "40757").with_description("Split items at edit cursor"),
            Keybind::new("<M-D-t>", "40061").with_description("Split items at time selection"),
            Keybind::new("<delete>", "40006").with_description("Remove selected items"),
            Keybind::new("<backspace>", "40006").with_description("Remove selected items"),
            // === Clipboard ===
            Keybind::new("<D-x>", "40059").with_description("Cut items/tracks/envelope points"),
            Keybind::new("<D-c>", "40060").with_description("Copy items/tracks/envelope points"),
            Keybind::new("<D-v>", "40058").with_description("Paste items/tracks"),
            // === Undo/Redo ===
            Keybind::new("<D-z>", "40029").with_description("Undo"),
            Keybind::new("<S-D-z>", "40030").with_description("Redo"),
            // === Selection ===
            Keybind::new("<D-a>", "40182").with_description("Select all items"),
            Keybind::new("<D-d>", "41295").with_description("Duplicate items"),
            // === Region Manipulation ===
            Keybind::new("l", "40636").with_description("Toggle loop item source"),
            Keybind::new("q", "40070").with_description("Quantize item positions to grid"),
            Keybind::new("<M-D-q>", "40767").with_description("Unquantize items"),
            Keybind::new("<D-r>", "41295").with_description("Repeat/duplicate items"),
            Keybind::new("<D-j>", "40362").with_description("Glue/join items"),
            Keybind::new("<S-D-g>", "40362")
                .with_description("Glue items (ignoring time selection)"),
            Keybind::new("<C-f>", "40516")
                .with_description("Item: Toggle item timebase between time and beats"),
            // === Nudging ===
            Keybind::new("<M-right>", "40793")
                .with_description("Nudge item position right by grid"),
            Keybind::new("<M-left>", "40794").with_description("Nudge item position left by grid"),
            Keybind::new("<M-S-right>", "40228").with_description("Nudge item length right"),
            Keybind::new("<M-S-left>", "40227").with_description("Nudge item length left"),
            Keybind::new(";", "41205").with_description("Move items to edit cursor"),
            // === Item Edges ===
            Keybind::new("<D-[>", "41311").with_description("Set item start to edit cursor"),
            Keybind::new("<D-]>", "41312").with_description("Set item end to edit cursor"),
            // === Shuffle/Arrangement ===
            Keybind::new("<M-[>", "41120").with_description("Shuffle items left"),
            Keybind::new("<M-]>", "41121").with_description("Shuffle items right"),
            // === Track Management ===
            Keybind::new("<M-D-n>", "40001").with_description("Insert new track"),
            Keybind::new("<M-D-a>", "40001").with_description("Insert new audio track"),
            Keybind::new("<M-D-s>", "40001")
                .with_description("Insert new software instrument track"),
            Keybind::new("<D-delete>", "40005").with_description("Delete/remove tracks"),
            Keybind::new("<M-D-delete>", "41158").with_description("Remove unused tracks"),
            // === Mute Region ===
            Keybind::new("<C-m>", "40719").with_description("Mute selected items"),
            // === Reverse ===
            Keybind::new("<C-S-r>", "41051").with_description("Reverse items"),
            // TODO: Capture recording (retroactive) - REAPER has this but needs custom action setup
            // TODO: Discard recording - Cmd+. - needs custom action
        ]
    }
}
