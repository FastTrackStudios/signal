//! The pointer glyph is a function of the resolved action.
//!
//! These pin the property the whole design rests on: nothing about the
//! cursor is written down twice. If rebinding a modifier stopped moving
//! the cursor with it, the surface would start describing gestures it no
//! longer performs — and that failure is silent on screen, which is
//! exactly what a test is for.

use expression_editor_core::Tool;
use expression_editor_core::cursor::{Aim, Cursor};
use expression_editor_core::mouse::{Action, Context, Gesture, ModKey, MouseMap};
use expression_editor_core::tools::Mods;

fn mods(shift: bool, ctrl: bool, alt: bool) -> Mods {
    Mods { shift, ctrl, alt }
}

/// The glyph for a hover, resolved the way the surface resolves it.
fn glyph(map: &MouseMap, context: Context, m: Mods, aim: Aim) -> Cursor {
    let action = map.resolve(context, Gesture::Drag, m);
    Cursor::for_action(action, context, aim)
}

#[test]
fn note_ends_get_their_bracket() {
    let map = MouseMap::reaper_like();
    assert_eq!(
        glyph(
            &map,
            Context::NoteEdge,
            mods(false, false, false),
            Aim::edge(true)
        ),
        Cursor::EdgeLeft,
    );
    assert_eq!(
        glyph(
            &map,
            Context::NoteEdge,
            mods(false, false, false),
            Aim::edge(false)
        ),
        Cursor::EdgeRight,
    );
}

/// The side is the *only* thing that separates the two brackets, so a
/// glyph that ignored `Aim` would still pass a one-sided test.
#[test]
fn the_brackets_are_not_interchangeable() {
    assert_ne!(
        Cursor::for_action(Action::MoveNoteEdge, Context::NoteEdge, Aim::edge(true)),
        Cursor::for_action(Action::MoveNoteEdge, Context::NoteEdge, Aim::edge(false)),
    );
    assert_ne!(
        Cursor::for_action(Action::StretchNotes, Context::NoteEdge, Aim::edge(true)),
        Cursor::for_action(Action::StretchNotes, Context::NoteEdge, Aim::edge(false)),
    );
}

/// Stretching reaches past the note you grabbed, so it must not look
/// like a plain edge drag.
#[test]
fn stretch_is_distinguishable_from_resize() {
    let map = MouseMap::reaper_like();
    let resize = glyph(
        &map,
        Context::NoteEdge,
        mods(false, false, false),
        Aim::edge(false),
    );
    let stretch = glyph(
        &map,
        Context::NoteEdge,
        mods(true, false, false),
        Aim::edge(false),
    );
    assert_eq!(resize, Cursor::EdgeRight);
    assert_eq!(stretch, Cursor::StretchRight);
}

#[test]
fn modifiers_change_the_glyph_on_a_note() {
    let map = MouseMap::reaper_like();
    let at = |m| glyph(&map, Context::Note, m, Aim::NONE);
    assert_eq!(at(mods(false, false, false)), Cursor::Move);
    assert_eq!(at(mods(false, false, true)), Cursor::Copy);
    assert_eq!(at(mods(false, true, false)), Cursor::Velocity);
}

/// The load-bearing claim: rebind the action, and the cursor follows
/// with no second table to edit.
#[test]
fn rebinding_moves_the_cursor_with_it() {
    let mut map = MouseMap::reaper_like();
    let alt_drag = |map: &MouseMap| glyph(map, Context::Note, mods(false, false, true), Aim::NONE);

    assert_eq!(alt_drag(&map), Cursor::Copy);
    map.set(
        Context::Note,
        Gesture::Drag,
        ModKey::ALT,
        Action::EraseNotes,
    );
    assert_eq!(alt_drag(&map), Cursor::NoteEraser);
}

/// Same claim through the host overlay's door: a mode preset that
/// disagrees with REAPER produces a different cursor without either map
/// knowing this module exists.
#[test]
fn a_preset_brings_its_own_cursors() {
    let plain = mods(false, false, false);
    // Drums paint on a plain roll drag where the REAPER map marquees.
    assert_eq!(
        glyph(
            &MouseMap::reaper_like(),
            Context::PianoRoll,
            plain,
            Aim::NONE
        ),
        Cursor::Crosshair,
    );
    assert_eq!(
        glyph(&MouseMap::drums(), Context::PianoRoll, plain, Aim::NONE),
        Cursor::Brush,
    );
    // And a plain drag on a drum note edits velocity rather than moving.
    assert_eq!(
        glyph(&MouseMap::drums(), Context::Note, plain, Aim::NONE),
        Cursor::Velocity,
    );
}

/// A handle is pressed before the map is consulted, so it must be drawn
/// before the map is consulted too.
#[test]
fn a_handle_outranks_whatever_the_map_says() {
    use expression_editor_core::Handle;
    for handle in Handle::ALL {
        assert_eq!(
            Cursor::for_action(Action::MoveNote, Context::Note, Aim::handle(handle)),
            Cursor::Handle(handle),
        );
    }
}

/// Seven handles that all drew the same arrow would tell you nothing
/// about which one you grabbed.
#[test]
fn every_handle_is_its_own_glyph() {
    use expression_editor_core::Handle;
    let mut seen = std::collections::HashSet::new();
    for handle in Handle::ALL {
        assert!(
            seen.insert(Cursor::Handle(handle)),
            "{handle:?} shares a glyph with another handle",
        );
    }
}

/// `ActiveTool` is the map declining in the tool's favour; resolving it
/// as a glyph directly would draw the wrong thing for every tool.
#[test]
fn the_armed_tool_supplies_its_own_glyph() {
    let map = MouseMap::reaper_like();
    let plain = mods(false, false, false);
    for (tool, want) in [
        (Tool::Pen, Cursor::Pencil),
        (Tool::Curve, Cursor::Curve),
        (Tool::Eraser, Cursor::Eraser),
        (Tool::NoteDraw, Cursor::Pencil),
        (Tool::NoteErase, Cursor::NoteEraser),
    ] {
        let action = map.resolve_for(Context::PianoRoll, Gesture::Drag, plain, tool);
        assert_eq!(
            action,
            Action::ActiveTool,
            "{tool:?} should claim a plain roll drag"
        );
        assert_eq!(Cursor::for_tool(tool, Context::PianoRoll), want, "{tool:?}");
    }
    // Select claims nothing, so the map answers and the roll stays a
    // crosshair.
    let action = map.resolve_for(Context::PianoRoll, Gesture::Drag, plain, Tool::Select);
    assert_eq!(action, Action::MarqueeSelect);
    assert_eq!(
        Cursor::for_action(action, Context::PianoRoll, Aim::NONE),
        Cursor::Crosshair,
    );
}

/// Panning is the one gesture whose glyph is allowed to change on press:
/// a hand that has closed on the canvas is how every surface says the
/// grab took.
#[test]
fn the_hand_closes_while_panning() {
    assert_eq!(Cursor::Hand.while_dragging(), Cursor::HandClosed);
    // Nothing else flinches mid-drag.
    for c in [
        Cursor::EdgeLeft,
        Cursor::Move,
        Cursor::Pencil,
        Cursor::Velocity,
    ] {
        assert_eq!(c.while_dragging(), c);
    }
}

/// Every CSS fallback has to be a keyword Blitz actually maps; a typo
/// here is a cursor that silently does not change in a DOM host.
#[test]
fn css_fallbacks_are_real_keywords() {
    const KNOWN: [&str; 13] = [
        "default",
        "crosshair",
        "ew-resize",
        "ns-resize",
        "col-resize",
        "move",
        "copy",
        "cell",
        "grab",
        "grabbing",
        "zoom-in",
        "pointer",
        "text",
    ];
    let mut all = vec![
        Cursor::Arrow,
        Cursor::Crosshair,
        Cursor::MarqueeAdd,
        Cursor::MarqueeToggle,
        Cursor::EdgeLeft,
        Cursor::EdgeRight,
        Cursor::StretchLeft,
        Cursor::StretchRight,
        Cursor::Move,
        Cursor::MoveH,
        Cursor::MoveV,
        Cursor::Copy,
        Cursor::Pencil,
        Cursor::Curve,
        Cursor::Brush,
        Cursor::Eraser,
        Cursor::NoteEraser,
        Cursor::Hand,
        Cursor::HandClosed,
        Cursor::Zoom,
        Cursor::Playhead,
        Cursor::Audition,
        Cursor::Velocity,
        Cursor::Scale,
        Cursor::Mute,
        Cursor::Text,
        Cursor::Split,
        Cursor::Razor,
        Cursor::RazorEdge,
        Cursor::Forbidden,
    ];
    all.extend(expression_editor_core::Handle::ALL.map(Cursor::Handle));
    for c in all {
        assert!(
            KNOWN.contains(&c.css()) || c.css() == "not-allowed",
            "{c:?} falls back to unknown keyword {:?}",
            c.css(),
        );
        assert!(!c.label().is_empty(), "{c:?} has no label");
    }
}
