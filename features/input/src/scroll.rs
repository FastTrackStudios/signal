//! Scroll pattern matching and per-mode binding tables.

use crate::command::ActionId;
use crate::context::{ActionContext, WhenExpr};
use crate::event::ScrollEvent;
use crate::key::Modifiers;

/// Scroll axis selector for pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxis {
    Any,
    Horizontal,
    Vertical,
}

/// A scroll gesture pattern used for binding lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPattern {
    pub axis: ScrollAxis,
    pub modifiers: Modifiers,
}

impl ScrollPattern {
    pub fn new(axis: ScrollAxis, modifiers: Modifiers) -> Self {
        Self { axis, modifiers }
    }

    pub fn matches(&self, event: &ScrollEvent) -> bool {
        if self.modifiers != event.modifiers {
            return false;
        }

        match self.axis {
            ScrollAxis::Any => true,
            ScrollAxis::Horizontal => event.delta_x.abs() > event.delta_y.abs(),
            ScrollAxis::Vertical => event.delta_y.abs() >= event.delta_x.abs(),
        }
    }
}

/// The scroll bindings for one *surface*, resolved from a config.
///
/// A surface is a place gestures mean something: the arrange view, the
/// expression editor, a mixer. It is the key of [`KeymapConfig::scroll`],
/// which the processor otherwise reads as a mode name — surfaces and
/// modes are both "which table applies right now", and a surface that
/// also wanted modes would key them together.
///
/// This exists so a surface consumes the shared config *directly* rather
/// than hand-rolling a `KeymapConfig` inline, which is how the arrange
/// view and the expression editor ended up with three different schemes
/// for the same gestures. One place resolves a wheel event to an action
/// id, and REAPER binds the same gesture names to its command ids.
///
/// Returns an empty table when the surface is not in the config, so a
/// caller gets "nothing bound here" rather than an error it cannot act on.
pub fn table_for_surface(
    config: &crate::config::KeymapConfig,
    surface: &str,
) -> Result<ScrollBindingTable, crate::config::ConfigError> {
    let mut table = ScrollBindingTable::new();
    let Some(bindings) = config.scroll.get(surface) else {
        return Ok(table);
    };
    for (pattern, action) in bindings {
        table.insert(
            crate::config::parse_scroll_pattern(pattern)?,
            WhenExpr::True,
            ActionId::new(action),
        );
    }
    Ok(table)
}

/// Ordered set of scroll bindings for a mode.
///
/// First matching entry wins.
#[derive(Debug, Clone, Default)]
pub struct ScrollBindingTable {
    bindings: Vec<(ScrollPattern, WhenExpr, ActionId)>,
}

impl ScrollBindingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, pattern: ScrollPattern, when: WhenExpr, action: ActionId) {
        self.bindings.push((pattern, when, action));
    }

    /// Read-only access to the raw binding entries.
    pub fn bindings(&self) -> &[(ScrollPattern, WhenExpr, ActionId)] {
        &self.bindings
    }

    pub fn match_event(&self, event: &ScrollEvent, ctx: &ActionContext) -> Option<ActionId> {
        for (pattern, when, action) in &self.bindings {
            if pattern.matches(event) && when.evaluate(ctx) {
                return Some(action.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use crate::config::load_default_config;
    use crate::key::Modifiers;

    fn ev(dx: f64, dy: f64, modifiers: Modifiers) -> ScrollEvent {
        ScrollEvent {
            delta_x: dx,
            delta_y: dy,
            modifiers,
        }
    }

    fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            ctrl,
            alt,
            shift,
            ..Default::default()
        }
    }

    /// What the gesture resolves to on `surface`, in the shipped defaults.
    fn resolve(surface: &str, dx: f64, dy: f64, m: Modifiers) -> Option<String> {
        let cfg = load_default_config().expect("defaults parse");
        let table = table_for_surface(&cfg, surface).expect("surface table");
        table
            .match_event(&ev(dx, dy, m), &ActionContext::new())
            .map(|a| a.0)
    }

    /// The scheme every surface shares: plain wheel scrolls, Shift goes
    /// sideways, Alt zooms — REAPER's own.
    ///
    /// `normal` only. The expression editor's table deliberately keeps
    /// the scrolling half and drops the zooming half: `Alt` there is the
    /// modifier that *creates a note*, and a key meaning one thing on a
    /// drag and another on a wheel is exactly the overloading that
    /// surface's map exists to remove. Its zoom moved to a held `Z`.
    ///
    /// A surface may add to the shared scheme and may decline part of
    /// it; what it must not do is *redefine* a gesture to mean something
    /// else, which is what would put the two surfaces in conflict.
    #[test]
    fn the_shared_scheme_is_the_reaper_scheme() {
        for surface in ["normal"] {
            assert_eq!(
                resolve(surface, 0.0, 10.0, mods(false, false, false)).as_deref(),
                Some("view.vscroll"),
                "{surface}: plain wheel scrolls vertically"
            );
            assert_eq!(
                resolve(surface, 0.0, 10.0, mods(false, false, true)).as_deref(),
                Some("view.hscroll"),
                "{surface}: shift+wheel scrolls horizontally"
            );
            assert_eq!(
                resolve(surface, 0.0, 10.0, mods(false, true, false)).as_deref(),
                Some("view.zoom_v"),
                "{surface}: alt+wheel zooms vertically"
            );
            assert_eq!(
                resolve(surface, 0.0, 10.0, mods(false, true, true)).as_deref(),
                Some("view.zoom_h"),
                "{surface}: alt+shift+wheel zooms horizontally"
            );
            assert_eq!(
                resolve(surface, 0.0, 10.0, mods(true, true, false)).as_deref(),
                Some("view.zoom_both"),
                "{surface}: ctrl+alt+wheel zooms both axes"
            );
        }
    }

    #[test]
    fn a_trackpad_swipe_scrolls_sideways_without_a_modifier() {
        // The axis half of the scheme: a horizontal gesture is already
        // horizontal, so it must not be read as a vertical scroll.
        assert_eq!(
            resolve("normal", 10.0, 0.0, mods(false, false, false)).as_deref(),
            Some("view.hscroll")
        );
    }

    #[test]
    fn the_editor_adds_its_own_gesture_without_changing_the_shared_ones() {
        // The expression editor exists only partly in the DAW, so it gets
        // a surface of its own — but it may only *add*. A gesture that
        // meant one thing in the arrange view and another in the editor
        // is the confusion this whole arrangement exists to avoid.
        assert_eq!(
            resolve("editor", 0.0, 10.0, mods(true, false, true)).as_deref(),
            Some("edit.nudge_time"),
            "ctrl+shift nudges notes off-grid in the editor"
        );
        assert_eq!(
            resolve("normal", 0.0, 10.0, mods(true, false, true)),
            None,
            "and means nothing in the arrange view"
        );
    }

    #[test]
    fn an_unknown_surface_is_empty_rather_than_an_error() {
        let cfg = load_default_config().expect("defaults parse");
        assert!(
            table_for_surface(&cfg, "mixer")
                .expect("no error")
                .bindings()
                .is_empty()
        );
    }
}
