//! Interaction beyond one value per control: what any surface (an EQ, an
//! ADSR, a keys rig, a mixer) needs to edit *collections* of things.
//!
//! - [`Chord`]: which modifier keys a rule wants held.
//! - [`Interact`]: a control's selection, click, drag-mode and wheel rules,
//!   its double-click command, its context menu, and whether dragging it out
//!   of its area removes its item.
//! - [`Command`]: one thing a click, a key, a menu row or a button does
//!   (set, toggle, cycle, invert, reset a param; fire an action; remove the
//!   item; start editing a text field; add an item at a point).
//! - [`Condition`]: a param test (a check mark, a rule that applies only to
//!   some shapes, a node shown only while a toggle is on).
//! - [`Zoom`]: a control's vertical range picked by a choice param (an EQ's
//!   ±3 / ±6 / ±12 dB).
//! - [`AddRule`]: what a double-click on an empty [`Canvas`](crate::ControlKind::Canvas)
//!   adds — the next free item of a [`Repeat`](crate::ControlKind::Repeat), its params
//!   set from where you clicked, zones deciding the rest (a low cut near the
//!   left edge, a notch deep down).
//! - [`MenuItem`]: a row of a context [`Menu`](crate::ControlKind::Menu).
//! - [`Placement`] / [`Show`]: how a [`Popover`](crate::ControlKind::Popover)
//!   follows its anchor and when it's up.
//!
//! Addresses inside a repeat template (and inside a context popover or
//! menu) are written with `{n}` for the item index (`eq/b{n}_gain`);
//! [`substitute`] resolves them.

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::param::{EntryId, ParamAddr};

/// The index placeholder in templated addresses, names and text.
pub const INDEX: &str = "{n}";

/// `s` with every `{n}` replaced by `n`.
#[must_use]
pub fn substitute(s: &str, n: u32) -> String {
    s.replace(INDEX, &n.to_string())
}

/// Whether `s` is templated (holds `{n}`).
#[must_use]
pub fn is_templated(s: &str) -> bool {
    s.contains(INDEX)
}

/// Modifier keys held with a click, drag or wheel. `command` is Ctrl
/// (Cmd on macOS); `option` is Alt (Option on macOS). A rule matches only
/// its exact chord, so Alt+Cmd and Alt are different rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
pub struct Chord {
    #[serde(default)]
    #[facet(default)]
    pub shift: bool,
    #[serde(default)]
    #[facet(default)]
    pub command: bool,
    #[serde(default)]
    #[facet(default)]
    pub option: bool,
}

impl Chord {
    pub const NONE: Self = Self {
        shift: false,
        command: false,
        option: false,
    };
    pub const SHIFT: Self = Self {
        shift: true,
        command: false,
        option: false,
    };
    pub const COMMAND: Self = Self {
        shift: false,
        command: true,
        option: false,
    };
    pub const OPTION: Self = Self {
        shift: false,
        command: false,
        option: true,
    };
    pub const COMMAND_SHIFT: Self = Self {
        shift: true,
        command: true,
        option: false,
    };
    pub const OPTION_SHIFT: Self = Self {
        shift: true,
        command: false,
        option: true,
    };
    pub const OPTION_COMMAND: Self = Self {
        shift: false,
        command: true,
        option: true,
    };
}

/// How dragging a handle moves it while a chord is held.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum DragMode {
    /// Both axes (the default).
    Free,
    /// Only across (an EQ band's frequency; its gain stays put).
    XOnly,
    /// Only up and down.
    YOnly,
    /// The handle stays; vertical travel turns its wheel param instead
    /// (an EQ band's resonance): `per_px` position units per pixel up.
    Wheel { per_px: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct DragRule {
    pub chord: Chord,
    pub mode: DragMode,
    /// Only while this holds (a cut's handle only slides across).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub when: Option<Condition>,
}

/// The wheel over a handle, with `chord` held (and `when` true), turns
/// `target` by `step` position units a notch (a stepped param moves a whole
/// detent). The first matching rule wins; none = the handle's own `wheel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct WheelRule {
    pub chord: Chord,
    pub target: ParamAddr,
    pub step: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub when: Option<Condition>,
}

/// A letter key pressed while the control is selected (no Cmd/Ctrl) runs
/// `commands` for each selected item: an EQ's `m` / `s` / `l` / `r` / `n`
/// placing the selected bands mid, side, left, right, stereo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct KeyRule {
    /// The character (case-insensitive).
    pub key: String,
    pub commands: Vec<Command>,
}

/// What a click with a chord does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ClickAction {
    /// Select only this (kept as is when already selected, so a group
    /// drags together).
    Select,
    /// Add to / remove from the selection.
    ToggleSelection,
    /// Select every selectable sibling between the last-selected and this
    /// one (in item order), plus this one.
    RangeSelect,
    /// Run commands — on release, and only if the pointer didn't travel
    /// (a chord-click never nudges the value).
    Run { commands: Vec<Command> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ClickRule {
    pub chord: Chord,
    pub action: ClickAction,
}

/// One thing to do. Addresses may be templated (`{n}`): they resolve
/// against the item the control belongs to (or the menu was opened on).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Command {
    /// Set a param to `value` in engineering units (a choice's option
    /// index, a toggle's 0/1).
    Set { addr: ParamAddr, value: f64 },
    /// Flip a toggle.
    Toggle { addr: ParamAddr },
    /// Step a choice to its next option, wrapping.
    Cycle { addr: ParamAddr },
    /// Negate a bipolar param (an EQ band's gain).
    Invert { addr: ParamAddr },
    /// Back to the param's default.
    Reset { addr: ParamAddr },
    /// Fire an engine action.
    Invoke { id: EntryId },
    /// Remove the item this control belongs to — every selected item when
    /// it is one of them.
    Remove,
    /// Start editing the text field `node` (all of its text selected).
    Edit { node: Uuid },
    /// Add an item where the menu was opened (a canvas's [`AddRule`]).
    Add,
}

/// A test of a param's value in engineering units (option index, 0/1, the
/// real value): true when it equals one of `any_of` (within half a step
/// for choices and toggles), or — with `any_of` empty — when it is at or
/// above `at_least`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Condition {
    pub addr: ParamAddr,
    #[serde(default)]
    #[facet(default)]
    pub any_of: Vec<f64>,
    #[serde(default)]
    #[facet(default)]
    pub at_least: f64,
    /// Inverts the test.
    #[serde(default)]
    #[facet(default)]
    pub not: bool,
}

impl Condition {
    /// On (≥ 0.5 for a toggle).
    #[must_use]
    pub fn on(addr: impl Into<String>) -> Self {
        Self {
            addr: ParamAddr::new(addr),
            any_of: Vec::new(),
            at_least: 0.5,
            not: false,
        }
    }

    /// One of these option indices / values.
    #[must_use]
    pub fn any_of(addr: impl Into<String>, values: &[f64]) -> Self {
        Self {
            addr: ParamAddr::new(addr),
            any_of: values.to_vec(),
            at_least: 0.0,
            not: false,
        }
    }

    #[must_use]
    pub fn negated(mut self) -> Self {
        self.not = !self.not;
        self
    }

    /// Evaluate against a param's engineering value.
    #[must_use]
    pub fn test(&self, real: f64) -> bool {
        let hit = if self.any_of.is_empty() {
            real >= self.at_least
        } else {
            self.any_of.iter().any(|v| (v - real).abs() < 0.5)
        };
        hit != self.not
    }
}

/// A control's vertical range, picked by a choice param: option `i` spans
/// `-spans[i]..spans[i]` (an EQ's ±3 / ±6 / ±12 dB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Zoom {
    pub param: ParamAddr,
    pub spans: Vec<f64>,
}

impl Zoom {
    /// The span option `index` picks (the last for an index past the end).
    #[must_use]
    pub fn span(&self, index: f64) -> Option<f64> {
        let last = self.spans.len().checked_sub(1)?;
        Some(self.spans[(index.max(0.0).round() as usize).min(last)])
    }
}

/// Everything a control does beyond its kind's gesture. All optional.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Interact {
    /// Click-to-select, marquee, group drag, Delete.
    #[serde(default)]
    #[facet(default)]
    pub selectable: bool,
    /// Dragged out of its area and let go, or selected when Delete is
    /// pressed: its item is removed.
    #[serde(default)]
    #[facet(default)]
    pub removable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub click: Vec<ClickRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub drag: Vec<DragRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub wheel: Vec<WheelRule>,
    /// Instead of the kind's own double-click (a reset).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub double_click: Vec<Command>,
    /// Commands run by a plain click (released inside): a delete button.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub press: Vec<Command>,
    /// Letter keys acting on the selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub keys: Vec<KeyRule>,
    /// The [`Menu`](crate::ControlKind::Menu) node a right-click opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub menu: Option<Uuid>,
}

impl Interact {
    /// The click action for `chord`: its rule, else a plain select for a
    /// selectable control.
    #[must_use]
    pub fn click_for(&self, chord: Chord) -> Option<&ClickAction> {
        self.click
            .iter()
            .find(|r| r.chord == chord)
            .map(|r| &r.action)
    }

    /// The drag mode for `chord` (free when no rule names it).
    #[must_use]
    pub fn drag_for(&self, chord: Chord) -> DragMode {
        self.drag
            .iter()
            .find(|r| r.chord == chord && r.when.is_none())
            .map_or(DragMode::Free, |r| r.mode)
    }

    /// Whether anything here needs the runtime's attention.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A param set to a value (engineering units) when an item is added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Assign {
    pub addr: ParamAddr,
    pub value: f64,
}

impl Assign {
    #[must_use]
    pub fn new(addr: impl Into<String>, value: f64) -> Self {
        Self {
            addr: ParamAddr::new(addr),
            value,
        }
    }
}

/// Where on a canvas a zone is: `x` in the canvas's x-axis values (Hz),
/// `y` in unit height (0 bottom, 1 top — so it holds at any zoom). The first
/// zone containing the click wins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct AddZone {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub set: Vec<Assign>,
    /// Whether the click's height sets the `y` param (false: a cut has no gain).
    pub keep_y: bool,
}

/// A double-click on an empty canvas takes the next free item of the
/// [`Repeat`](crate::ControlKind::Repeat) `repeat` (a template node), marks
/// it used, resets its params to their defaults, then sets `x` / `y` to the
/// point clicked (through the canvas's axes) and applies `set`, then the
/// first matching zone. The new item is selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct AddRule {
    pub repeat: Uuid,
    pub x: Option<ParamAddr>,
    pub y: Option<ParamAddr>,
    #[serde(default)]
    #[facet(default)]
    pub set: Vec<Assign>,
    #[serde(default)]
    #[facet(default)]
    pub zones: Vec<AddZone>,
}

impl AddRule {
    /// The zone a click at axis value `x` and unit height `v` falls in.
    #[must_use]
    pub fn zone(&self, x: f64, v: f64) -> Option<&AddZone> {
        self.zones
            .iter()
            .find(|z| x >= z.x_min && x <= z.x_max && v >= z.y_min && v <= z.y_max)
    }
}

/// What kind of row a menu item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ItemRole {
    #[default]
    Action,
    /// A title row (not selectable).
    Header,
    /// A rule between groups (not selectable).
    Divider,
}

/// One row of a context menu: the node that is its row (for hit testing and
/// the highlight), what it runs, a check mark shown while `checked` holds,
/// and a submenu opened from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MenuItem {
    pub row: Uuid,
    #[serde(default)]
    #[facet(default)]
    pub role: ItemRole,
    #[serde(default)]
    #[facet(default)]
    pub commands: Vec<Command>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub checked: Option<Condition>,
    /// The check glyph node, shown while `checked` holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub check: Option<Uuid>,
    /// A [`Menu`](crate::ControlKind::Menu) node opened beside this row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub submenu: Option<Uuid>,
}

impl MenuItem {
    #[must_use]
    pub fn action(row: Uuid, commands: Vec<Command>) -> Self {
        Self {
            row,
            role: ItemRole::Action,
            commands,
            checked: None,
            check: None,
            submenu: None,
        }
    }

    #[must_use]
    pub fn selectable(&self) -> bool {
        self.role == ItemRole::Action
    }
}

/// Which side of its anchor a popover prefers. It flips to the opposite
/// side when that side doesn't fit inside the surface, then is clamped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Placement {
    #[default]
    Above,
    Below,
    Left,
    Right,
    /// Docked to the bottom edge of its anchor's area (a handle's plot),
    /// `gap` above it, following the anchor only horizontally — a band
    /// panel that stays put vertically while its band moves. Held still
    /// for a moment after it is touched so its own controls don't slide
    /// away from the pointer driving them.
    DockBottom,
    /// As [`Placement::DockBottom`], docked to the top edge.
    DockTop,
    /// Where it is drawn, never moving: a band panel pinned beside its plot
    /// (a touchscreen's, where a panel following the band would sit under
    /// the finger). Up while its context is (the active band).
    Pinned,
}

/// When a popover is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Show {
    /// Always (a label riding a handle).
    Always,
    /// While its anchor is the active one: being dragged, the last
    /// selected, or hovered (and while the pointer is on the popover).
    #[default]
    Active,
    /// Only while its anchor is selected.
    Selected,
    /// Only while its anchor is hovered or dragged.
    Hovered,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions() {
        let c = Condition::any_of("eq/b1_shape", &[3.0, 4.0]);
        assert!(c.test(3.0) && c.test(4.0) && !c.test(0.0));
        assert!(!c.clone().negated().test(3.0));
        assert!(Condition::on("eq/b1_on").test(1.0));
        assert!(!Condition::on("eq/b1_on").test(0.0));
    }

    #[test]
    fn templates_resolve() {
        assert_eq!(substitute("eq/b{n}_gain", 12), "eq/b12_gain");
        assert!(is_templated("b{n}") && !is_templated("b1"));
    }

    #[test]
    fn zoom_and_zones() {
        let z = Zoom {
            param: ParamAddr::new("eq/range"),
            spans: vec![3.0, 6.0, 12.0],
        };
        assert_eq!(z.span(1.0), Some(6.0));
        assert_eq!(z.span(9.0), Some(12.0));
        let rule = AddRule {
            repeat: Uuid::nil(),
            x: None,
            y: None,
            set: Vec::new(),
            zones: vec![AddZone {
                x_min: 0.0,
                x_max: 30.0,
                y_min: 0.4,
                y_max: 0.6,
                set: vec![Assign::new("s", 3.0)],
                keep_y: false,
            }],
        };
        assert!(rule.zone(25.0, 0.5).is_some());
        assert!(rule.zone(25.0, 0.9).is_none());
        assert!(rule.zone(40.0, 0.5).is_none());
    }

    #[test]
    fn interact_rules_match_exact_chords() {
        let i = Interact {
            drag: vec![DragRule {
                chord: Chord::OPTION,
                mode: DragMode::XOnly,
                when: None,
            }],
            click: vec![ClickRule {
                chord: Chord::COMMAND,
                action: ClickAction::ToggleSelection,
            }],
            ..Interact::default()
        };
        assert_eq!(i.drag_for(Chord::OPTION), DragMode::XOnly);
        assert_eq!(i.drag_for(Chord::OPTION_SHIFT), DragMode::Free);
        assert_eq!(
            i.click_for(Chord::COMMAND),
            Some(&ClickAction::ToggleSelection)
        );
        assert!(i.click_for(Chord::NONE).is_none());
        assert!(Interact::default().is_empty());
    }
}
