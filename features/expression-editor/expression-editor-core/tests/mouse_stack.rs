//! The stacked view's mouse bindings live in the map like everything
//! else.
//!
//! They used to be `if` statements inside the pointer handler. That had
//! three costs: the drum surface could not be rebound, it could not
//! appear in the preferences beside the roll's bindings, and a gesture
//! doing the wrong thing was indistinguishable from a gesture nobody had
//! written. Putting them in the map fixes all three, and these tests are
//! what stop the map and the handler drifting apart again.

use expression_editor_core::mouse::{Action, Context, Gesture, ModKey, MouseMap};
use expression_editor_core::tools::Mods;

const NONE: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: false,
};
const SHIFT: Mods = Mods {
    shift: true,
    ctrl: false,
    alt: false,
};
const CTRL: Mods = Mods {
    shift: false,
    ctrl: true,
    alt: false,
};
const ALT: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: true,
};

// r[verify drums.mouse.contexts]
#[test]
fn every_preset_can_drive_the_stack() {
    // The stacked view is not a drums-only surface and the profile is
    // the user's choice, so bindings that existed only in the Drums
    // preset would leave the editor inert for anyone on another one.
    // They live in the base every preset is built from.
    for name in MouseMap::PRESETS {
        let m = MouseMap::preset(name);
        assert_eq!(
            m.resolve(Context::Hit, Gesture::Drag, NONE),
            Action::MoveHit,
            "{name}: dragging a hit does nothing"
        );
        assert_eq!(
            m.resolve(Context::Lane, Gesture::Click, NONE),
            Action::SelectLane,
            "{name}: clicking a lane does nothing"
        );
    }
}

// r[verify drums.mouse.contexts]
#[test]
fn a_hit_is_moved_and_shift_pins_the_takes_ends() {
    let m = MouseMap::drums();
    assert_eq!(
        m.resolve(Context::Hit, Gesture::Drag, NONE),
        Action::MoveHit
    );
    assert_eq!(
        m.resolve(Context::Hit, Gesture::Drag, SHIFT),
        Action::MoveHitBothEnds,
        "shift is the BothStretch law"
    );
}

// r[verify drums.mouse.contexts]
#[test]
fn the_lane_behind_a_hit_still_does_something() {
    // A click two pixels off a marker must not feel broken, so the lane
    // selects rather than falling through to nothing.
    let m = MouseMap::drums();
    assert_eq!(
        m.resolve(Context::Lane, Gesture::Click, NONE),
        Action::SelectLane
    );
}

// r[verify drums.mouse.contexts]
#[test]
fn adding_and_cutting_are_deliberate_modifier_gestures() {
    // Neither may be reachable by a plain click: one invents a hit and
    // the other rewrites every item in the kit, and both would then be
    // one slip of the hand away at all times.
    let m = MouseMap::drums();
    assert_eq!(
        m.resolve(Context::Lane, Gesture::Click, ALT),
        Action::AddHit
    );
    assert_eq!(
        m.resolve(Context::Lane, Gesture::Click, CTRL),
        Action::SplitTake
    );
    assert_ne!(
        m.resolve(Context::Lane, Gesture::Click, NONE),
        Action::AddHit
    );
    assert_ne!(
        m.resolve(Context::Lane, Gesture::Click, NONE),
        Action::SplitTake
    );
}

// r[verify drums.mouse.contexts]
#[test]
fn a_hit_can_be_thrown_out_and_snapped() {
    let m = MouseMap::drums();
    assert_eq!(
        m.resolve(Context::Hit, Gesture::RightClick, NONE),
        Action::RemoveHit
    );
    assert_eq!(
        m.resolve(Context::Hit, Gesture::DoubleClick, NONE),
        Action::SnapHitToGrid
    );
}

// r[verify drums.mouse.contexts]
#[test]
fn the_stack_bindings_are_rebindable_like_any_other() {
    // The whole point of moving them into the map. A user who wants
    // ctrl-drag to move a hit can have it, without a code change.
    let mut m = MouseMap::drums();
    m.set(Context::Hit, Gesture::Drag, ModKey::CTRL, Action::MoveHit);
    assert_eq!(
        m.resolve(Context::Hit, Gesture::Drag, CTRL),
        Action::MoveHit
    );
}
