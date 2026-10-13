//! The editor's commands, as `#[architect::actions]` traits.
//!
//! One declaration per command, and everything else reads it: the
//! keyboard dispatch, the which-key overlay, a command palette, and —
//! through an `ActionBackend` — REAPER's own action list.
//!
//! ## What this replaces
//!
//! Every command used to exist three times in `expression-editor-ui`:
//! once in an `ACTIONS` list of id strings, once in a `LABELS` table of
//! names, and once in a `dispatch` match. Nothing tied them together, so
//! adding a command meant remembering three places and the failure mode
//! was silence — an id in `ACTIONS` with no arm did nothing, a label
//! with no id was dead text. `LABELS` was matched with `contains`, which
//! made `grid.1` shadow `grid.16` and had to be hand-ordered around.
//!
//! Here the id, the display name, the description, the category, the
//! group and the shortcut hint are one item, and the macro emits the
//! constant everything matches on.
//!
//! ## One module per group
//!
//! The macro emits its constants beside the trait, not inside a module
//! of its own, so two groups sharing a verb — a grid `TOGGLE` and a
//! razor `TOGGLE` — would collide at the crate root. A module each
//! keeps the names short *and* unambiguous: `grid::MEASURE`,
//! `razor::CLEAR`.
//!
//! It also keeps the ids readable. The macro builds them as
//! `{namespace}_{METHOD}`, so a `grid_measure` method under
//! `FTS_EDITOR_GRID` would be `FTS_EDITOR_GRID_GRID_MEASURE`. Inside
//! `mod grid` the method is just `measure`.
//!
//! ## Shape constraints, and why they fit
//!
//! `#[action]` methods take no arguments and return `()` or
//! `Result<(), E>` — a REAPER named command takes no parameters, and
//! architect models exactly that. It happens to suit a keyboard surface
//! for the same reason: a key press carries no arguments either. Where a
//! command needs a value, the value is in the name — seven chord degrees
//! are seven actions, five grid divisions are five actions.
//!
//! ## What this crate can run, and what it cannot
//!
//! [`run`] executes everything reachable from an `&mut Editor`, which is
//! most of it. Two groups are declared here but executed by
//! `expression-editor-ui`:
//!
//! - **velocity** needs `expression-editor-tools`, which depends on this
//!   crate — running it here would be a cycle.
//! - **`view::MEMAGIC`** is anchored on the pointer, which this crate
//!   has no notion of.
//!
//! They are still declared here because a command palette should list
//! every command the editor has, not the subset one crate happens to be
//! able to perform. `expression-editor-ui`'s dispatch tries [`run`]
//! first and handles the rest, and its tests assert that between them
//! nothing declared is unreachable.
//!
//! ## The shortcut hint is a hint
//!
//! `shortcut` is what an overlay *prints*, not what the keymap binds.
//! The binding lives in `input`'s keymap where a user can change it;
//! this is the surface telling you how to reach the thing it is
//! offering.

use crate::Editor;

/// Where the grid is, and how it behaves.
pub mod grid {
    #[architect::actions(namespace = "FTS_EDITOR_GRID")]
    pub trait GridActions {
        #[action(
            description = "Snap to whole measures",
            category = "Expression Editor",
            group = "Grid",
            shortcut = "g q"
        )]
        fn measure(&self);

        #[action(
            description = "Snap to half notes",
            category = "Expression Editor",
            group = "Grid",
            shortcut = "g w"
        )]
        fn half(&self);

        #[action(
            description = "Snap to quarter notes",
            category = "Expression Editor",
            group = "Grid",
            shortcut = "g e"
        )]
        fn quarter(&self);

        #[action(
            description = "Snap to eighth notes",
            category = "Expression Editor",
            group = "Grid",
            shortcut = "g r"
        )]
        fn eighth(&self);

        #[action(
            description = "Snap to sixteenth notes",
            category = "Expression Editor",
            group = "Grid",
            shortcut = "g f"
        )]
        fn sixteenth(&self);

        #[action(
            description = "Turn snapping on or off",
            category = "Expression Editor",
            group = "Grid",
            toggleable,
            shortcut = "g g"
        )]
        fn toggle(&self);

        #[action(
            description = "Dotted grid — each step half again as long",
            category = "Expression Editor",
            group = "Grid",
            toggleable,
            shortcut = "g d"
        )]
        fn dotted(&self);

        #[action(
            description = "Triplet grid — each step two thirds as long",
            category = "Expression Editor",
            group = "Grid",
            toggleable,
            shortcut = "g t"
        )]
        fn triplet(&self);

        #[action(
            description = "Let the grid follow the zoom, never finer than the setting",
            category = "Expression Editor",
            group = "Grid",
            toggleable,
            shortcut = "g a"
        )]
        fn adaptive(&self);
    }
}

/// The edit cursor, and what follows it — `hjkl`.
pub mod cursor {
    #[architect::actions(namespace = "FTS_EDITOR_CURSOR")]
    pub trait CursorActions {
        #[action(
            description = "Move the edit cursor back one measure",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "h"
        )]
        fn left_measure(&self);

        #[action(
            description = "Move the edit cursor on one measure",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "l"
        )]
        fn right_measure(&self);

        #[action(
            description = "Move the edit cursor back one grid step",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "Ctrl+h"
        )]
        fn left_grid(&self);

        #[action(
            description = "Move the edit cursor on one grid step",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "Ctrl+l"
        )]
        fn right_grid(&self);

        #[action(
            description = "Extend the time selection left by a grid step",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "Ctrl+Shift+h"
        )]
        fn left_select(&self);

        #[action(
            description = "Extend the time selection right by a grid step",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "Ctrl+Shift+l"
        )]
        fn right_select(&self);

        #[action(
            description = "Edit the previous track",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "k"
        )]
        fn track_prev(&self);

        #[action(
            description = "Edit the next track",
            category = "Expression Editor",
            group = "Navigation",
            shortcut = "j"
        )]
        fn track_next(&self);

        #[action(
            undo,
            description = "Shorten the selected notes by one grid step",
            category = "Expression Editor",
            group = "Notes",
            shortcut = "Shift+h"
        )]
        fn notes_shorten(&self);

        #[action(
            undo,
            description = "Lengthen the selected notes by one grid step",
            category = "Expression Editor",
            group = "Notes",
            shortcut = "Shift+l"
        )]
        fn notes_lengthen(&self);
    }
}

/// The chord gun — degrees, and what a degree means.
pub mod chord {
    #[architect::actions(namespace = "FTS_EDITOR_CHORD")]
    pub trait ChordActions {
        #[action(
            undo,
            description = "Fire the chord on the first scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 1"
        )]
        fn degree_1(&self);
        #[action(
            undo,
            description = "Fire the chord on the second scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 2"
        )]
        fn degree_2(&self);
        #[action(
            undo,
            description = "Fire the chord on the third scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 3"
        )]
        fn degree_3(&self);
        #[action(
            undo,
            description = "Fire the chord on the fourth scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 4"
        )]
        fn degree_4(&self);
        #[action(
            undo,
            description = "Fire the chord on the fifth scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 5"
        )]
        fn degree_5(&self);
        #[action(
            undo,
            description = "Fire the chord on the sixth scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 6"
        )]
        fn degree_6(&self);
        #[action(
            undo,
            description = "Fire the chord on the seventh scale degree",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c 7"
        )]
        fn degree_7(&self);

        #[action(
            description = "Move the tonic up a semitone",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c t"
        )]
        fn tonic_up(&self);
        #[action(
            description = "Move the tonic down a semitone",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c g"
        )]
        fn tonic_down(&self);
        #[action(
            description = "Next mode of the current scale family",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c s"
        )]
        fn mode_next(&self);
        #[action(
            description = "Previous mode of the current scale family",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c a"
        )]
        fn mode_prev(&self);
        #[action(
            description = "Deeper chords — sevenths, ninths, and up",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c d"
        )]
        fn depth_next(&self);
        #[action(
            description = "Shallower chords, back towards triads",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c f"
        )]
        fn depth_prev(&self);
        #[action(
            description = "Invert the chord up",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c i"
        )]
        fn inversion_next(&self);
        #[action(
            description = "Invert the chord down",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c u"
        )]
        fn inversion_prev(&self);
        #[action(
            description = "Fire chords an octave higher",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c o"
        )]
        fn octave_up(&self);
        #[action(
            description = "Fire chords an octave lower",
            category = "Expression Editor",
            group = "Chords",
            shortcut = "c l"
        )]
        fn octave_down(&self);
    }
}

/// Razor areas, and what they do to what they hold.
pub mod razor {
    #[architect::actions(namespace = "FTS_EDITOR_RAZOR")]
    pub trait RazorActions {
        #[action(
            undo,
            description = "Reverse the area's contents in time",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a r"
        )]
        fn reverse(&self);
        #[action(
            undo,
            description = "Reverse the pitches, keeping the rhythm",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a Ctrl+r"
        )]
        fn reverse_pitches(&self);
        #[action(
            undo,
            description = "Mirror the pitches about their own centre",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a v"
        )]
        fn invert(&self);
        #[action(
            undo,
            description = "Delete everything inside the areas",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a x"
        )]
        fn delete_contents(&self);
        #[action(
            undo,
            description = "Copy the contents on by the area's own width",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a d"
        )]
        fn duplicate(&self);
        #[action(
            undo,
            description = "Split notes at the area edges, leaving them in place",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a p"
        )]
        fn split(&self);
        #[action(
            description = "Select the notes inside the areas",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a s"
        )]
        fn select_contents(&self);
        #[action(
            description = "Take the notes inside the areas out of the selection",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a u"
        )]
        fn unselect_contents(&self);
        #[action(
            description = "Grow the areas to cover every row",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a f"
        )]
        fn full_lane(&self);
        #[action(
            undo,
            description = "Erase the active expression lane across the areas",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a c"
        )]
        fn clear_lane(&self);
        #[action(
            description = "Drop the areas, keeping the material",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a q"
        )]
        fn clear(&self);
        #[action(
            undo,
            description = "Stretch the contents to twice the length",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a 2"
        )]
        fn double(&self);
        #[action(
            undo,
            description = "Squeeze the contents to half the length",
            category = "Expression Editor",
            group = "Razor",
            shortcut = "a h"
        )]
        fn halve(&self);
    }
}

/// Velocity shaping. Declared here, run by `expression-editor-ui` —
/// these need `expression-editor-tools`, which depends on this crate.
pub mod velocity {
    #[architect::actions(namespace = "FTS_EDITOR_VELOCITY")]
    pub trait VelocityActions {
        #[action(
            undo,
            description = "Ramp the selection's velocity up, adjustable with the wheel",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v u"
        )]
        fn ramp_up(&self);
        #[action(
            undo,
            description = "Ramp the selection's velocity down",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v d"
        )]
        fn ramp_down(&self);
        #[action(
            undo,
            description = "Ramp up on a smooth S-curve",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v s"
        )]
        fn ramp_smooth(&self);
        #[action(
            undo,
            description = "Accent the first of every four",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v a"
        )]
        fn accent(&self);
        #[action(
            undo,
            description = "Narrow the selection's dynamic range",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v c"
        )]
        fn compress(&self);
        #[action(
            undo,
            description = "Widen the selection's dynamic range",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v e"
        )]
        fn expand(&self);
        #[action(
            undo,
            description = "Scatter the velocities slightly",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v r"
        )]
        fn humanise(&self);
        #[action(
            undo,
            description = "Set every selected note to the selection's average",
            category = "Expression Editor",
            group = "Velocity",
            shortcut = "v f"
        )]
        fn flatten(&self);
        #[action(
            description = "Open the velocity panel",
            category = "Expression Editor",
            group = "Velocity",
            toggleable,
            shortcut = "v p"
        )]
        fn panel(&self);
    }
}

/// Framing the view. `MEMAGIC` is run by `expression-editor-ui` — it is
/// anchored on the pointer, which this crate knows nothing about.
pub mod view {
    #[architect::actions(namespace = "FTS_EDITOR_VIEW")]
    pub trait ViewActions {
        #[action(
            description = "Zoom to whatever the pointer is over",
            category = "Expression Editor",
            group = "View",
            shortcut = "z x"
        )]
        fn memagic(&self);
        #[action(
            description = "Fit the whole item in the view",
            category = "Expression Editor",
            group = "View",
            shortcut = "z i"
        )]
        fn fit_item(&self);
        #[action(
            description = "Fit the notes in view",
            category = "Expression Editor",
            group = "View",
            shortcut = "z n"
        )]
        fn fit_notes(&self);
        #[action(
            description = "Centre on the notes",
            category = "Expression Editor",
            group = "View",
            shortcut = "z c"
        )]
        fn center(&self);
        #[action(
            description = "Frame the top of the pitch range",
            category = "Expression Editor",
            group = "View",
            shortcut = "z t"
        )]
        fn top(&self);
        #[action(
            description = "Frame the bottom of the pitch range",
            category = "Expression Editor",
            group = "View",
            shortcut = "z b"
        )]
        fn bottom(&self);
        #[action(
            description = "Reset the view",
            category = "Expression Editor",
            group = "View",
            shortcut = "z r"
        )]
        fn reset(&self);
    }
}

/// Run an action against an editor, by its architect id.
///
/// The seam between the declarations above and a surface holding an
/// `&mut Editor`. A trait impl is the other shape — and is what an
/// `ActionBackend` will want, so REAPER can trigger these — but that
/// needs an editor the impl can reach, which on this surface means a
/// dioxus `Signal`. This works for the keyboard today and does not
/// prevent the impl later: both would call the same methods on
/// [`Editor`].
///
/// `false` means "not mine, or nothing to do" — the velocity group and
/// `view::MEMAGIC` are deliberately not handled here (see the module
/// docs), and the caller falls through to whatever else it knows about.
pub fn run(ed: &mut Editor, id: &str) -> bool {
    use adaptive_grid::Density;

    // ── grid ─────────────────────────────────────────────────────────
    if id == grid::MEASURE.id {
        ed.set_grid_division(1.0);
        return true;
    }
    if id == grid::HALF.id {
        ed.set_grid_division(1.0 / 2.0);
        return true;
    }
    if id == grid::QUARTER.id {
        ed.set_grid_division(1.0 / 4.0);
        return true;
    }
    if id == grid::EIGHTH.id {
        ed.set_grid_division(1.0 / 8.0);
        return true;
    }
    if id == grid::SIXTEENTH.id {
        ed.set_grid_division(1.0 / 16.0);
        return true;
    }
    if id == grid::TOGGLE.id {
        ed.grid.enabled = !ed.grid.enabled;
        return true;
    }
    if id == grid::DOTTED.id {
        let on = !ed.grid.dotted;
        ed.set_grid_dotted(on);
        return true;
    }
    if id == grid::TRIPLET.id {
        let on = !ed.grid.triplet;
        ed.set_grid_triplet(on);
        return true;
    }
    if id == grid::ADAPTIVE.id {
        // Off ↔ the middle density. Cycling all six would make "is it
        // on?" take five presses to answer.
        let next = if ed.grid.adaptive.is_adaptive() {
            Density::Fixed
        } else {
            Density::Medium
        };
        ed.set_grid_density(next);
        return true;
    }

    // ── the cursor ───────────────────────────────────────────────────
    if id == cursor::LEFT_MEASURE.id {
        let d = ed.measure();
        return ed.move_cursor(-d);
    }
    if id == cursor::RIGHT_MEASURE.id {
        let d = ed.measure();
        return ed.move_cursor(d);
    }
    if id == cursor::LEFT_GRID.id {
        let d = ed.grid_step();
        return ed.move_cursor(-d);
    }
    if id == cursor::RIGHT_GRID.id {
        let d = ed.grid_step();
        return ed.move_cursor(d);
    }
    if id == cursor::LEFT_SELECT.id {
        let d = ed.grid_step();
        return ed.move_cursor_extending(-d);
    }
    if id == cursor::RIGHT_SELECT.id {
        let d = ed.grid_step();
        return ed.move_cursor_extending(d);
    }
    if id == cursor::TRACK_PREV.id {
        return ed.step_track(-1);
    }
    if id == cursor::TRACK_NEXT.id {
        return ed.step_track(1);
    }
    if id == cursor::NOTES_SHORTEN.id {
        let d = ed.grid_step();
        return ed.nudge_note_lengths(-d);
    }
    if id == cursor::NOTES_LENGTHEN.id {
        let d = ed.grid_step();
        return ed.nudge_note_lengths(d);
    }

    // ── chords ───────────────────────────────────────────────────────
    for (n, meta) in [
        (1, chord::DEGREE_1),
        (2, chord::DEGREE_2),
        (3, chord::DEGREE_3),
        (4, chord::DEGREE_4),
        (5, chord::DEGREE_5),
        (6, chord::DEGREE_6),
        (7, chord::DEGREE_7),
    ] {
        if id == meta.id {
            return ed.insert_chord(n);
        }
    }
    if id == chord::TONIC_UP.id {
        ed.chord_gun.transpose(1);
        return true;
    }
    if id == chord::TONIC_DOWN.id {
        ed.chord_gun.transpose(-1);
        return true;
    }
    if id == chord::MODE_NEXT.id {
        ed.chord_gun.cycle_mode(true);
        return true;
    }
    if id == chord::MODE_PREV.id {
        ed.chord_gun.cycle_mode(false);
        return true;
    }
    if id == chord::DEPTH_NEXT.id {
        ed.chord_gun.cycle_depth(true);
        return true;
    }
    if id == chord::DEPTH_PREV.id {
        ed.chord_gun.cycle_depth(false);
        return true;
    }
    if id == chord::INVERSION_NEXT.id {
        ed.chord_gun.cycle_inversion(true);
        return true;
    }
    if id == chord::INVERSION_PREV.id {
        ed.chord_gun.cycle_inversion(false);
        return true;
    }
    if id == chord::OCTAVE_UP.id {
        ed.chord_gun.octave = (ed.chord_gun.octave + 1).min(8);
        return true;
    }
    if id == chord::OCTAVE_DOWN.id {
        ed.chord_gun.octave = (ed.chord_gun.octave - 1).max(0);
        return true;
    }

    // ── razor ────────────────────────────────────────────────────────
    if id == razor::REVERSE.id {
        return ed.razor_reverse();
    }
    if id == razor::REVERSE_PITCHES.id {
        return ed.razor_reverse_pitches();
    }
    if id == razor::INVERT.id {
        return ed.razor_invert();
    }
    if id == razor::DELETE_CONTENTS.id {
        return ed.razor_delete_contents();
    }
    if id == razor::DUPLICATE.id {
        return ed.razor_duplicate();
    }
    if id == razor::SPLIT.id {
        return ed.razor_split();
    }
    if id == razor::SELECT_CONTENTS.id {
        return ed.razor_select_contents();
    }
    if id == razor::UNSELECT_CONTENTS.id {
        return ed.razor_unselect_contents();
    }
    if id == razor::FULL_LANE.id {
        return ed.razor_full_lane();
    }
    if id == razor::CLEAR_LANE.id {
        return ed.razor_clear_lane();
    }
    if id == razor::CLEAR.id {
        let had = !ed.razor.is_empty();
        ed.razor.clear();
        return had;
    }
    if id == razor::DOUBLE.id {
        return ed.razor_scale(2.0);
    }
    if id == razor::HALVE.id {
        return ed.razor_scale(0.5);
    }

    // ── the view ─────────────────────────────────────────────────────
    //
    // `MEMAGIC` is absent on purpose: it is anchored on the pointer and
    // belongs to the surface. The rest are fixed framings.
    if id == view::RESET.id {
        ed.reset_view();
        return true;
    }
    if let Some(modes) = view_modes(id) {
        let anchor = memagic::Anchor {
            t: ed
                .playhead
                .unwrap_or_else(|| ed.camera.t_at(ed.viewport.w * 0.5)),
            row: Some(ed.camera.vertical.center),
        };
        return ed.memagic_with(modes, anchor, &memagic::Config::default());
    }

    false
}

use crate::memagic;

/// The fixed framings, as MeMagic modes.
fn view_modes(id: &str) -> Option<memagic::Modes> {
    use memagic::{Horizontal, Modes, Scope, Vertical};

    Some(match id {
        i if i == view::FIT_ITEM.id => Modes {
            horizontal: Horizontal::FitItem,
            vertical: Vertical::FitNotes {
                scope: Scope::InItem,
            },
        },
        i if i == view::FIT_NOTES.id => Modes {
            horizontal: Horizontal::Keep,
            vertical: Vertical::FitNotes {
                scope: Scope::InView,
            },
        },
        i if i == view::CENTER.id => Modes {
            horizontal: Horizontal::Keep,
            vertical: Vertical::Center {
                scope: Scope::InView,
            },
        },
        i if i == view::TOP.id => Modes {
            horizontal: Horizontal::Keep,
            vertical: Vertical::Highest {
                scope: Scope::InView,
            },
        },
        i if i == view::BOTTOM.id => Modes {
            horizontal: Horizontal::Keep,
            vertical: Vertical::Lowest {
                scope: Scope::InView,
            },
        },
        _ => return None,
    })
}

/// Every action the editor declares, for a palette or an overlay.
pub fn all() -> Vec<&'static architect::action::ActionMeta> {
    grid::GridActionsActions::all()
        .iter()
        .chain(cursor::CursorActionsActions::all())
        .chain(chord::ChordActionsActions::all())
        .chain(razor::RazorActionsActions::all())
        .chain(velocity::VelocityActionsActions::all())
        .chain(view::ViewActionsActions::all())
        .collect()
}

/// The action whose id is `id`, if the editor declares one.
pub fn find(id: &str) -> Option<&'static architect::action::ActionMeta> {
    all().into_iter().find(|m| m.id == id)
}
