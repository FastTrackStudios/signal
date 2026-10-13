//! What a gesture means where it lands.
//!
//! REAPER's mouse behaviour is a table: for each CONTEXT — a media
//! item, its left edge, a fade, the ruler, the empty arrange area —
//! and each modifier combination, an action. [`crate::hit`] answers
//! "what did you touch, and in which context"; this answers "and what
//! does that do", so the two stay separately configurable: a hit test
//! is geometry, a mouse map is preference.
//!
//! The table here is the FTS default. A mouse profile
//! (`input_config_proto::MouseProfileConfig`) is the same shape — a
//! context, a modifier set, a behaviour — and is where a user's
//! overrides will load into once the profile system reaches this
//! window; the contexts are already its contexts.

use input_config_proto::MouseModifierContext as Context;

/// The keys held during a gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// The gesture itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Click,
    DoubleClick,
    Drag,
}

/// What a gesture does, and whether the grid applies to it.
///
/// Two answers and not one, because "move the item" and "move the item
/// off the grid" are the same verb done differently — pairing them as
/// separate actions doubles the table for every draggable thing and
/// then doubles it again for the next modifier. The verb is what the
/// gesture MEANS; the snap is how precisely it is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bound {
    pub action: Action,
    /// Whether the result lands on the grid. Off is the fine gesture:
    /// REAPER spells it "ignoring snap" and puts it on Shift for
    /// everything that can be dragged, which is the one convention
    /// worth keeping wholesale — a modifier that means different
    /// things on different objects is a modifier nobody learns.
    pub snap: bool,
}

/// What a gesture does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Move the edit cursor (and the transport) to the time.
    SetEditCursor,
    /// Drag out a time selection.
    TimeSelection,
    /// Drag the fade-in's length.
    FadeIn,
    /// Drag the fade-out's length.
    FadeOut,
    /// Change the fade's shape rather than its length.
    FadeShape,
    /// Move the item along its lane.
    MoveItem,
    /// Leave the item where it is and drag a copy of it.
    CopyItem,
    /// Leave the item where it is and drag its CONTENTS inside it.
    SlipItem,
    /// Trim the item's left edge.
    TrimLeft,
    /// Trim the item's right edge.
    TrimRight,
    /// Select the item, and nothing else.
    SelectItem,
    /// Add the item to the selection, or take it out again — the
    /// modified click, for building a selection up a piece at a time.
    ToggleItemSelection,
    /// Sweep a box over empty lanes, selecting the items it touches —
    /// the Editor's marquee over notes, with the same rules
    /// (`view_gestures::Marquee`).
    MarqueeSelect,
    /// The same, adding to the selection.
    MarqueeAdd,
    /// Draw a razor area out of the drag.
    RazorArea,
    /// Move a razor area already drawn, by its middle.
    MoveRazorArea,
    /// Resize a razor area by one edge.
    ResizeRazorArea,
    /// Take a razor area out of the set.
    RemoveRazorArea,
    /// Nothing bound here.
    Nothing,
}

/// The context for a razor area already on screen.
///
/// `Custom` rather than a new variant on `MouseModifierContext`: the
/// enum lives in the daw repo and this is the arrangement's own
/// surface, which is exactly the case `Custom` was added for. REAPER
/// models razor areas as their own contexts too — area left drag, area
/// click, area edge — so the shape matches what a profile will
/// eventually load into.
pub const RAZOR_AREA: Context = Context::Custom("razor-area");

/// The context for a razor area's edge — the Editor's `RazorEdge`.
pub const RAZOR_EDGE: Context = Context::Custom("razor-edge");

/// The FTS default: what each context does under each gesture.
///
/// Stated as a table so it reads like the preference page it will
/// become, and so a profile override is a row that wins over one of
/// these rather than a branch somewhere in the window.
#[must_use]
pub fn resolve(context: Context, gesture: Gesture, mods: Mods) -> Bound {
    Bound {
        action: verb(context, gesture, mods),
        // Shift is the fine gesture everywhere something can be
        // dragged. On a click there is nothing to snap, so the answer
        // is the harmless one.
        // And never while the toolbar has snapping off.
        snap: !mods.shift && crate::options::SNAP.get(),
    }
}

/// What the gesture means, before the question of precision.
fn verb(context: Context, gesture: Gesture, mods: Mods) -> Action {
    use Gesture as G;
    match (context, gesture) {
        // The razor, on Ctrl, and first in the table because a
        // modifier that has to beat an unguarded default has to be
        // asked about before it.
        //
        // Not the plain drag, which is a time selection and is worth
        // keeping: the two gestures are the same shape and the razor is
        // the rarer one. Not Alt either — that is the zoom tool's sweep
        // in this window, and a modifier meaning two things depending
        // on what else is held is what the table exists to prevent.
        //
        // Over an item's body and its edges as much as over empty
        // ground. A razor with a dead zone at every item boundary would
        // be one that depends on where items begin and end, which is
        // the one thing it is defined as not doing.
        //
        // The fade handles are the exception, knowingly: Ctrl there is
        // already the fade's SHAPE, bound below for a reason worth
        // keeping. A handle is a small corner and a razor can start a
        // pixel away from one, where a whole edge could not be worked
        // around.
        // Slip on Ctrl+Alt, which is what is left.
        //
        // REAPER slips on plain Alt-drag. Alt is copy here and Ctrl is
        // the razor, both deliberate and both confirmed, so slip takes
        // the pair. It has to be tested before either of them: a guard
        // that checks one modifier matches a chord containing it, and
        // the more specific row has to be asked first or it is
        // unreachable.
        (Context::MediaItemBottomHalf, G::Drag) if mods.ctrl && mods.alt => Action::SlipItem,
        (Context::ArrangeView, G::Drag) if mods.ctrl => Action::RazorArea,
        (Context::MediaItemBottomHalf, G::Drag) if mods.ctrl => Action::RazorArea,
        (Context::MediaItemLeftEdge, G::Drag) if mods.ctrl => Action::RazorArea,
        (Context::MediaItemRightEdge, G::Drag) if mods.ctrl => Action::RazorArea,
        // An area already drawn answers for itself: drag moves it,
        // click takes it out. No modifier on either — by the time the
        // pointer is over an area there is nothing else it could mean.
        (RAZOR_AREA, G::Drag) => Action::MoveRazorArea,
        // Its edge resizes it, the other edge staying put.
        (RAZOR_EDGE, G::Drag) => Action::ResizeRazorArea,
        // A click on either takes the area out.
        (RAZOR_AREA | RAZOR_EDGE, G::Click) => Action::RemoveRazorArea,
        // The ruler: a click places the cursor, a drag selects time.
        (Context::Ruler, G::Click) => Action::SetEditCursor,
        (Context::Ruler, G::Drag) => Action::TimeSelection,
        // The empty arrange area: a click places the cursor, as on the
        // ruler; a drag is a marquee, as on the Editor's empty roll —
        // Shift adds to the selection, Ctrl (above) cuts a razor. Time
        // is selected on the ruler, in both.
        (Context::ArrangeView, G::Click) => Action::SetEditCursor,
        (Context::ArrangeView, G::Drag) if mods.shift => Action::MarqueeAdd,
        (Context::ArrangeView, G::Drag) => Action::MarqueeSelect,
        // A fade's handle: drag its length; with Ctrl, its shape.
        //
        // Ctrl and not Shift, which is where this started: Shift is
        // "ignore the grid" on every other draggable thing, and a
        // modifier that means one thing on six objects and something
        // else on the seventh is a modifier nobody learns. REAPER puts
        // the curve on Ctrl for the same reason.
        (Context::MediaItemFade, G::Drag) if mods.ctrl => Action::FadeShape,
        (Context::MediaItemFade, G::Drag) => Action::FadeIn,
        // An item (REAPER calls the body its bottom half): click selects,
        // drag moves, its edges trim. The
        // moves and trims are bound here and not yet built — the
        // window says so rather than doing something else.
        (Context::MediaItemBottomHalf, G::Click) if mods.ctrl => Action::ToggleItemSelection,
        (Context::MediaItemBottomHalf, G::Click) => Action::SelectItem,
        // Copy on Alt, not on Ctrl.
        //
        // REAPER copies on Ctrl-drag; here Ctrl is the razor, and the
        // razor has the better claim on it — it is the gesture that
        // needs to start anywhere over the lanes, including on an
        // item's edges, where copy only ever needs the body. Alt is
        // free on an item: the zoom tool's Alt sweep only exists while
        // `z` is held, which is a mode and not a modifier.
        (Context::MediaItemBottomHalf, G::Drag) if mods.alt => Action::CopyItem,
        (Context::MediaItemBottomHalf, G::Drag) => Action::MoveItem,
        (Context::MediaItemLeftEdge, G::Drag) => Action::TrimLeft,
        (Context::MediaItemRightEdge, G::Drag) => Action::TrimRight,
        _ => Action::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(shift: bool, ctrl: bool, alt: bool) -> Mods {
        Mods { shift, ctrl, alt }
    }

    #[test]
    fn a_fade_handle_drags_the_fade_and_ctrl_drags_its_shape() {
        assert_eq!(
            resolve(Context::MediaItemFade, Gesture::Drag, Mods::default()).action,
            Action::FadeIn
        );
        assert_eq!(
            resolve(
                Context::MediaItemFade,
                Gesture::Drag,
                with(false, true, false)
            )
            .action,
            Action::FadeShape
        );
        // And NOT on shift, which means something else everywhere else.
        assert_eq!(
            resolve(
                Context::MediaItemFade,
                Gesture::Drag,
                with(true, false, false)
            )
            .action,
            Action::FadeIn
        );
        assert_eq!(
            resolve(Context::MediaItemFade, Gesture::Click, Mods::default()).action,
            Action::Nothing
        );
    }

    /// Ctrl draws a razor wherever the lanes are, and the plain drag
    /// still means what it meant.
    ///
    /// The table is the authority, so the razor's reach is stated here
    /// rather than discovered by dragging: a dead zone at an item
    /// boundary would make the razor depend on where items begin and
    /// end, which is the one thing it is defined as not doing.
    #[test]
    fn ctrl_draws_a_razor_over_everything_the_lanes_have() {
        let ctrl = with(false, true, false);
        for context in [
            Context::ArrangeView,
            Context::MediaItemBottomHalf,
            Context::MediaItemLeftEdge,
            Context::MediaItemRightEdge,
        ] {
            assert_eq!(
                resolve(context, Gesture::Drag, ctrl).action,
                Action::RazorArea,
                "{context:?} has a hole in it"
            );
        }
        // And without Ctrl, everything still means what it did.
        assert_eq!(
            resolve(Context::ArrangeView, Gesture::Drag, Mods::default()).action,
            Action::MarqueeSelect
        );
        assert_eq!(
            resolve(Context::MediaItemBottomHalf, Gesture::Drag, Mods::default()).action,
            Action::MoveItem
        );
        assert_eq!(
            resolve(Context::MediaItemLeftEdge, Gesture::Drag, Mods::default()).action,
            Action::TrimLeft
        );
        // The fade's shape keeps Ctrl. Stated so that giving the razor
        // the whole lane later is a deliberate change and not a
        // surprise.
        assert_eq!(
            resolve(Context::MediaItemFade, Gesture::Drag, ctrl).action,
            Action::FadeShape
        );
        // A ctrl CLICK on an item is still the selection toggle: the
        // razor is a drag, and the press stands in for both until it
        // is one or the other.
        assert_eq!(
            resolve(Context::MediaItemBottomHalf, Gesture::Click, ctrl).action,
            Action::ToggleItemSelection
        );
    }

    /// Alt copies, Ctrl razors, and a plain drag still moves.
    ///
    /// The three that share an item's body, stated together: a modifier
    /// budget is only legible as a whole, and the one this window has
    /// left is small.
    #[test]
    fn an_items_body_spends_its_modifiers_on_copy_and_razor() {
        let body = Context::MediaItemBottomHalf;
        assert_eq!(
            resolve(body, Gesture::Drag, Mods::default()).action,
            Action::MoveItem
        );
        assert_eq!(
            resolve(body, Gesture::Drag, with(false, false, true)).action,
            Action::CopyItem,
            "Alt should copy — REAPER puts this on Ctrl, which is the razor here"
        );
        assert_eq!(
            resolve(body, Gesture::Drag, with(false, true, false)).action,
            Action::RazorArea
        );
        // Shift is the grid, on the copy as much as on the move.
        assert!(!resolve(body, Gesture::Drag, with(true, false, true)).snap);
    }

    /// An area already drawn answers for itself, without a modifier.
    #[test]
    fn an_area_is_moved_by_a_drag_and_removed_by_a_click() {
        assert_eq!(
            resolve(RAZOR_AREA, Gesture::Drag, Mods::default()).action,
            Action::MoveRazorArea
        );
        assert_eq!(
            resolve(RAZOR_AREA, Gesture::Click, Mods::default()).action,
            Action::RemoveRazorArea
        );
    }

    /// Shift is "ignore the grid", on everything that can be dragged.
    ///
    /// The point of it being in the table rather than read off the keys
    /// wherever a drag happens to be handled: one rule, stated once,
    /// and the same answer for an item, an edge, a fade and a region.
    #[test]
    fn shift_takes_everything_off_the_grid() {
        let draggable = [
            Context::MediaItemBottomHalf,
            Context::MediaItemLeftEdge,
            Context::MediaItemRightEdge,
            Context::MediaItemFade,
            Context::Ruler,
            Context::ArrangeView,
        ];
        for context in draggable {
            assert!(
                resolve(context, Gesture::Drag, Mods::default()).snap,
                "{context:?} should snap by default"
            );
            assert!(
                !resolve(context, Gesture::Drag, with(true, false, false)).snap,
                "{context:?} should ignore the grid with shift"
            );
        }
    }

    /// A plain click selects one item; the modified click builds a
    /// selection up.
    #[test]
    fn ctrl_click_adds_to_the_selection_instead_of_replacing_it() {
        assert_eq!(
            resolve(
                Context::MediaItemBottomHalf,
                Gesture::Click,
                Mods::default()
            )
            .action,
            Action::SelectItem
        );
        assert_eq!(
            resolve(
                Context::MediaItemBottomHalf,
                Gesture::Click,
                with(false, true, false)
            )
            .action,
            Action::ToggleItemSelection
        );
    }

    /// A click on the ruler or on empty lanes places the cursor; a drag
    /// on the ruler selects time, on empty lanes it is a marquee (Shift
    /// adds) — the Editor's ruler and empty roll, the same.
    #[test]
    fn the_ruler_selects_time_and_empty_lanes_sweep_a_marquee() {
        for context in [Context::Ruler, Context::ArrangeView] {
            assert_eq!(
                resolve(context, Gesture::Click, Mods::default()).action,
                Action::SetEditCursor
            );
        }
        assert_eq!(
            resolve(Context::Ruler, Gesture::Drag, Mods::default()).action,
            Action::TimeSelection
        );
        assert_eq!(
            resolve(Context::ArrangeView, Gesture::Drag, Mods::default()).action,
            Action::MarqueeSelect
        );
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(
            resolve(Context::ArrangeView, Gesture::Drag, shift).action,
            Action::MarqueeAdd
        );
    }

    #[test]
    fn a_mixer_strip_is_not_in_the_map() {
        assert_eq!(
            resolve(Context::MixerStrip, Gesture::Drag, Mods::default()).action,
            Action::Nothing
        );
    }
}
