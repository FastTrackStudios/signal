//! The editor's chrome — toolbar, status bar, context menu — as scenes.
//!
//! The Dioxus surface builds these from components; a host that
//! paints its own frame needs them as the roll is: a picture from the
//! editor's state, a hit test from a point, and an action from a hit.
//! Nothing here holds state of its own beyond a menu's position. The
//! buttons are *derived* from the [`Editor`] on every layout, so a
//! button cannot show a tool the editor is not in, and pressing one
//! goes straight back into the editor.
//!
//! Laid out by measuring the labels, so a bar never clips a word and a
//! narrower box drops whole segments from the right rather than
//! squeezing every button.

use anyrender::{PaintScene, Scene};
use expression_editor_core::doc::{Dimension, NoteId};
use expression_editor_core::menu::{self, Command, MenuItem};
use expression_editor_core::razor::RazorAxis;
use expression_editor_core::zoom::ZoomModes;
use expression_editor_core::{Edit, Editor, Mode, Shape, StripLane, Tool, tuning};
use kurbo::{Affine, Line, Point, Rect};
use peniko::Fill;
use std::fmt::Write as _;

use crate::interaction::{self, Drag};
use crate::paint::{Look, stroke_of, with_alpha};
use crate::text::{self, Labeller};
use crate::theme;

/// The toolbar's height.
pub const TOOLBAR_H: f64 = 26.0;
/// The status bar's height.
pub const STATUS_H: f64 = 20.0;
/// The label size on both bars.
const FONT: f32 = 10.0;
/// Padding either side of a label inside its button.
const PAD_X: f64 = 7.0;
/// The gap between segments.
const GAP: f64 = 8.0;
/// The gap between buttons inside a segment.
const SEAM: f64 = 1.0;

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Mode(Mode),
    Tool(Tool),
    /// Edit this expression dimension.
    Dimension(Dimension),
    /// Draw this dimension behind the active one.
    Overlay(Dimension),
    /// Spread the selection across MPE member channels.
    Spread,
    ChannelDown,
    ChannelUp,
    /// Cycle the pitch-bend range.
    BendRange,
    Shape(Shape),
    /// Every track on one timeline.
    Stacked,
    Undo,
    Redo,
    /// Zoom to the passage at the playhead.
    Zoom,
    /// Reset the view.
    Fit,
    // ── status bar ───────────────────────────────────────────────
    SnapGrid,
    GridCoarser,
    /// The division, which is a readout rather than a button.
    GridLabel,
    GridFiner,
    GridAdaptive,
    Triplet,
    /// The key, cycled.
    Key,
    Snap12Tet,
    StripLane(StripLane),
    /// Show or hide the strip.
    Strip,
    /// The mouse-map preset, cycled.
    MousePreset,
    /// How many notes are selected — a readout.
    Selection,
}

/// A laid-out button.
#[derive(Clone, Debug)]
pub struct Button {
    pub control: Control,
    pub label: String,
    /// Drawn lit.
    pub active: bool,
    /// Drawn dim, and a press does nothing.
    pub enabled: bool,
    /// Its box, in the bar's own space.
    pub rect: Rect,
}

impl Button {
    /// Whether a press does anything at all.
    #[must_use]
    pub const fn is_readout(&self) -> bool {
        matches!(self.control, Control::GridLabel | Control::Selection)
    }
}

/// Lays buttons out along a bar, segment by segment.
struct Row<'a> {
    labels: &'a mut Labeller,
    width: f64,
    height: f64,
    x: f64,
    buttons: Vec<Button>,
    /// Whether the last thing placed was a button, so the next segment
    /// gets a gap.
    open: bool,
}

impl<'a> Row<'a> {
    const fn new(labels: &'a mut Labeller, width: f64, height: f64) -> Self {
        Self {
            labels,
            width,
            height,
            x: GAP,
            buttons: Vec::new(),
            open: false,
        }
    }

    fn gap(&mut self) {
        if self.open {
            self.x += GAP;
            self.open = false;
        }
    }

    /// Place one button. Dropped, with everything after it, once the
    /// bar is full: a half-drawn segment is worse than none.
    fn push(&mut self, control: Control, label: impl Into<String>, active: bool, enabled: bool) {
        let label = label.into();
        let w = PAD_X.mul_add(2.0, self.labels.shape(&label, FONT).width);
        if self.x + w > self.width - GAP {
            self.x = self.width;
            return;
        }
        let rect = Rect::new(self.x, 3.0, self.x + w, self.height - 3.0);
        self.buttons.push(Button {
            control,
            label,
            active,
            enabled,
            rect,
        });
        self.x += w + SEAM;
        self.open = true;
    }

    fn finish(self) -> Vec<Button> {
        self.buttons
    }
}

/// The toolbar's buttons for `ed`, inside `width`.
pub fn toolbar(ed: &Editor, width: f64, labels: &mut Labeller) -> Vec<Button> {
    let mut row = Row::new(labels, width, TOOLBAR_H);
    let tool = ed.shown_tool();
    for mode in Mode::ALL {
        row.push(Control::Mode(mode), mode.label(), ed.mode == mode, true);
    }
    row.gap();
    for t in Tool::ALL {
        row.push(Control::Tool(t), t.label(), tool == t, true);
    }
    if ed.mode.has_expression_lanes() {
        row.gap();
        for lane in Dimension::ALL {
            row.push(
                Control::Dimension(lane),
                theme::lane_label(lane),
                ed.dimension == lane,
                true,
            );
            let shown = ed.overlays.contains(&lane);
            row.push(
                Control::Overlay(lane),
                if shown { "●" } else { "○" },
                shown,
                true,
            );
        }
    }
    if ed.mode.has_mpe_channels() {
        row.gap();
        let some = !ed.selection.notes.is_empty();
        row.push(Control::Spread, "Spread", false, some);
        row.push(Control::ChannelDown, "ch−", false, some);
        row.push(Control::ChannelUp, "ch+", false, some);
        row.push(
            Control::BendRange,
            format!("±{:.0}", ed.doc.bend_range),
            false,
            true,
        );
    }
    row.gap();
    for shape in Shape::ALL {
        row.push(
            Control::Shape(shape),
            shape_short(shape),
            ed.shape == shape,
            true,
        );
    }
    row.gap();
    if ed.tracks.len() > 1 {
        row.push(Control::Stacked, "Stack", ed.stacked, true);
    }
    row.push(Control::Undo, "Undo", false, ed.can_undo());
    row.push(Control::Redo, "Redo", false, ed.can_redo());
    row.gap();
    row.push(Control::Zoom, "Zoom", false, true);
    row.push(Control::Fit, "Fit", false, true);
    row.finish()
}

/// The status bar's buttons for `ed`, inside `width`.
pub fn status(ed: &Editor, width: f64, labels: &mut Labeller) -> Vec<Button> {
    let mut row = Row::new(labels, width, STATUS_H);
    row.push(Control::SnapGrid, "Snap", ed.grid.enabled, true);
    row.push(Control::GridCoarser, "−", false, true);
    let mut grid = ed.grid.label();
    if ed.grid.is_coarsened() {
        let _ = write!(grid, " ≤{}", ed.grid.ceiling_label());
    }
    row.push(Control::GridLabel, grid, ed.grid.is_coarsened(), true);
    row.push(Control::GridFiner, "+", false, true);
    let density = ed.grid.adaptive.density;
    row.push(
        Control::GridAdaptive,
        format!(
            "AUTO {}",
            if ed.grid.adaptive.is_adaptive() {
                density_short(density)
            } else {
                "OFF"
            }
        ),
        ed.grid.adaptive.is_adaptive(),
        true,
    );
    row.push(Control::Triplet, "T", ed.grid.triplet, true);
    row.gap();
    row.push(
        Control::Key,
        format!("Key {}", tuning::pitch_class_name(ed.tuning.key_pc)),
        false,
        true,
    );
    row.push(Control::Snap12Tet, "12-TET", ed.tuning.snap_12tet, true);
    row.gap();
    row.push(Control::Strip, "Strip", ed.lane_strip_h > 0.0, true);
    for lane in StripLane::ALL {
        row.push(
            Control::StripLane(lane),
            lane.label(),
            ed.strip_lane == lane,
            true,
        );
    }
    row.gap();
    row.push(Control::MousePreset, ed.mouse.name, false, true);
    row.gap();
    let mut readout = format!("{} selected", ed.selection.notes.len());
    if ed.razor_insert {
        readout.push_str(" · I");
    }
    match ed.razor_axis {
        Some(RazorAxis::Horizontal) => readout.push_str(" · H"),
        Some(RazorAxis::Vertical) => readout.push_str(" · L"),
        None => {}
    }
    if !ed.razor.areas.is_empty() {
        let _ = write!(readout, " · {} razor", ed.razor.areas.len());
    }
    row.push(Control::Selection, readout, false, true);
    row.finish()
}

/// Draw a bar of buttons into a scene `width` by `height`, with the
/// button under the pointer, if any, lit a little.
pub fn paint(
    buttons: &[Button],
    hover: Option<Control>,
    width: f64,
    height: f64,
    labels: &mut Labeller,
    look: &Look,
) -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        look.surface_bar,
        None,
        &Rect::new(0.0, 0.0, width, height),
    );
    scene.stroke(
        &stroke_of(1.0),
        Affine::IDENTITY,
        look.panel_border,
        None,
        &Line::new((0.0, height - 0.5), (width, height - 0.5)),
    );
    for b in buttons {
        let fill = if b.active {
            look.control_active
        } else if hover == Some(b.control) && !b.is_readout() {
            look.control_hover
        } else if b.is_readout() {
            look.surface_bar
        } else {
            look.control
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fill,
            None,
            &b.rect.to_rounded_rect(2.0),
        );
        let ink = if !b.enabled {
            with_alpha(look.text, 0.35)
        } else if b.active {
            look.text_bright
        } else {
            look.text
        };
        let shaped = labels.shape(&b.label, FONT);
        let baseline = text_top(b.rect.center().y, &shaped);
        text::draw(
            &mut scene,
            &shaped,
            b.rect.center().x,
            baseline,
            text::Align::Center,
            ink,
            Affine::IDENTITY,
        );
    }
    scene
}

/// The button under a point in the bar's space.
#[must_use]
pub fn hit(buttons: &[Button], x: f64, y: f64) -> Option<Control> {
    buttons
        .iter()
        .find(|b| b.rect.contains(Point::new(x, y)))
        .map(|b| b.control)
}

/// Do what a button does. `true` when the editor changed.
///
/// `drag` is the gesture in flight, which a shape change reshapes
/// live — the same rule as the Dioxus toolbar.
pub fn activate(ed: &mut Editor, drag: &Drag, control: Control) -> bool {
    match control {
        Control::Mode(mode) => {
            ed.set_mode(mode);
            true
        }
        Control::Tool(tool) => {
            ed.tool = tool;
            true
        }
        Control::Dimension(lane) => {
            ed.dimension = lane;
            true
        }
        Control::Overlay(lane) => {
            match ed.overlays.iter().position(|&l| l == lane) {
                Some(i) => {
                    ed.overlays.remove(i);
                }
                None => ed.overlays.push(lane),
            }
            true
        }
        Control::Spread => {
            let notes = ed.selection.notes.clone();
            if notes.is_empty() {
                return false;
            }
            ed.apply(&Edit::AssignChannels {
                notes,
                seed: 0x5EED,
            })
        }
        Control::ChannelDown | Control::ChannelUp => {
            let notes = ed.selection.notes.clone();
            if notes.is_empty() {
                return false;
            }
            let delta = if control == Control::ChannelUp { 1 } else { -1 };
            ed.apply(&Edit::NudgeChannel { notes, delta })
        }
        Control::BendRange => {
            const RANGES: [f64; 4] = [2.0, 12.0, 24.0, 48.0];
            let cur = ed.doc.bend_range;
            ed.doc.bend_range = next_in(&RANGES, |r| (r - cur).abs() < 0.5);
            true
        }
        Control::Shape(shape) => {
            interaction::apply_shape(ed, drag, shape);
            true
        }
        Control::Stacked => {
            ed.stacked = !ed.stacked;
            true
        }
        Control::Undo => ed.undo(),
        Control::Redo => ed.redo(),
        Control::Zoom => {
            let t = ed
                .playhead
                .unwrap_or_else(|| ed.camera.t_at(ed.viewport.w * 0.5));
            let row = ed.camera.vertical.center;
            ed.smart_zoom(ZoomModes::NOTE_AREA, t, row);
            true
        }
        Control::Fit => {
            ed.reset_view();
            true
        }
        Control::GridLabel | Control::Selection => false,
        status => activate_status(ed, status),
    }
}

/// The element after the one `current` picks, wrapping; the first
/// when none does.
fn next_in<T: Copy>(order: &[T; 4], current: impl Fn(&T) -> bool) -> T {
    let at = order.iter().position(current);
    let next = at.map_or(0, |i| i.wrapping_add(1).rem_euclid(order.len()));
    order.iter().copied().cycle().nth(next).unwrap_or(order[0])
}

/// The status bar's half of [`activate`].
fn activate_status(ed: &mut Editor, control: Control) -> bool {
    match control {
        Control::SnapGrid => {
            ed.grid.enabled = !ed.grid.enabled;
            true
        }
        Control::GridCoarser => {
            ed.grid_coarser();
            true
        }
        Control::GridFiner => {
            ed.grid_finer();
            true
        }
        Control::GridAdaptive => {
            ed.set_grid_density(next_density(ed.grid.adaptive.density));
            true
        }
        Control::Triplet => {
            let on = ed.grid.triplet;
            ed.set_grid_triplet(!on);
            true
        }
        Control::Key => {
            ed.tuning.key_pc = ed.tuning.key_pc.wrapping_add(1).rem_euclid(12);
            true
        }
        Control::Snap12Tet => {
            ed.tuning.snap_12tet = !ed.tuning.snap_12tet;
            true
        }
        Control::StripLane(lane) => {
            ed.strip_lane = lane;
            true
        }
        Control::Strip => {
            ed.lane_strip_h = if ed.lane_strip_h > 0.0 { 0.0 } else { 96.0 };
            true
        }
        Control::MousePreset => {
            const ORDER: [&str; 4] = ["REAPER-like", "Drums", "Riffer (Ample)", "Lyrics"];
            let name = next_in(&ORDER, |n| *n == ed.mouse.name);
            ed.mouse = expression_editor_core::mouse::MouseMap::preset(name);
            true
        }
        _ => false,
    }
}

/// Where a label's line goes so its glyphs sit centred on `center_y`.
///
/// `text::draw` places the shaped line's origin; parley's glyphs carry
/// the line's ascent themselves, so the origin is the line's top, and
/// centring means backing off by about the cap height.
fn text_top(center_y: f64, shaped: &text::Shaped) -> f64 {
    shaped.ascent.mul_add(-0.68, center_y)
}

const fn shape_short(shape: Shape) -> &'static str {
    match shape {
        Shape::Linear => "Lin",
        Shape::EaseIn => "In",
        Shape::EaseOut => "Out",
        Shape::EaseInOut => "In/Out",
        Shape::Exponential => "Exp",
        Shape::SCurve => "S",
    }
}

const fn density_short(density: adaptive_grid::Density) -> &'static str {
    use adaptive_grid::Density as D;
    match density {
        D::Widest => "WIDE+",
        D::Wide => "WIDE",
        D::Medium => "MED",
        D::Narrow => "NARR",
        D::Narrowest => "NARR-",
        D::Custom(_) => "CUST",
        D::Fixed => "AUTO",
    }
}

const fn next_density(current: adaptive_grid::Density) -> adaptive_grid::Density {
    use adaptive_grid::Density as D;
    match current {
        D::Fixed => D::Widest,
        D::Widest => D::Wide,
        D::Wide => D::Medium,
        D::Medium => D::Narrow,
        D::Narrow => D::Narrowest,
        D::Narrowest | D::Custom(_) => D::Fixed,
    }
}

// ── the context menu ─────────────────────────────────────────────────

/// The menu's width.
const MENU_W: f64 = 210.0;
/// One item's height.
const ITEM_H: f64 = 20.0;
/// The rule above a group.
const BREAK_H: f64 = 7.0;
const MENU_PAD: f64 = 4.0;

/// A command the core could not complete on its own, handed back so
/// the host can open whatever it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    Lyric(NoteId),
    Articulation(NoteId),
    Properties,
}

/// What choosing from a menu came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// The point was outside every item; the menu stays.
    Missed,
    /// An item ran, or was disabled; the menu closes — with what the
    /// host still owes, if anything.
    Done(Option<Pending>),
}

/// An open context menu: where it is and what it offers.
#[derive(Clone, Debug)]
pub struct Menu {
    /// Top-left, in roll space.
    pub at: (f64, f64),
    pub under: Option<NoteId>,
    pub items: Vec<MenuItem>,
    /// Each item's box, in roll space, parallel to `items`.
    rects: Vec<Rect>,
    /// The item under the pointer.
    pub hover: Option<usize>,
}

impl Menu {
    /// Open at a right-click, over what it hit.
    #[must_use]
    pub fn open(ed: &Editor, x: f64, y: f64, under: Option<NoteId>, t: f64, row: i32) -> Self {
        let items = menu::menu_at(ed, under, t, row);
        let mut rects = Vec::with_capacity(items.len());
        let mut cy = y + MENU_PAD;
        for item in &items {
            if item.group_break {
                cy += BREAK_H;
            }
            rects.push(Rect::new(x, cy, x + MENU_W, cy + ITEM_H));
            cy += ITEM_H;
        }
        Self {
            at: (x, y),
            under,
            items,
            rects,
            hover: None,
        }
    }

    /// The whole menu's box.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let bottom = self.rects.last().map_or(self.at.1, |r| r.y1) + MENU_PAD;
        Rect::new(self.at.0, self.at.1, self.at.0 + MENU_W, bottom)
    }

    /// An item's box, in roll space.
    ///
    /// # Panics
    ///
    /// When `index` is not an item of this menu.
    #[must_use]
    pub fn item_rect(&self, index: usize) -> Rect {
        self.rects
            .get(index)
            .copied()
            .unwrap_or_else(|| panic!("menu has no item {index}"))
    }

    /// The item under a roll-space point.
    #[must_use]
    pub fn item_at(&self, x: f64, y: f64) -> Option<usize> {
        self.rects.iter().position(|r| r.contains(Point::new(x, y)))
    }

    /// Move the highlight. `true` when it changed.
    pub fn hover_at(&mut self, x: f64, y: f64) -> bool {
        let next = self.item_at(x, y);
        let changed = next != self.hover;
        self.hover = next;
        changed
    }

    /// Run the item at a point.
    pub fn choose(&self, ed: &mut Editor, x: f64, y: f64) -> Choice {
        let Some(item) = self.item_at(x, y).and_then(|i| self.items.get(i)) else {
            return Choice::Missed;
        };
        if !item.enabled {
            return Choice::Done(None);
        }
        let done = ed.run_command(&item.command, self.under);
        Choice::Done(if done {
            None
        } else {
            match &item.command {
                Command::EditLyric(id) => Some(Pending::Lyric(*id)),
                Command::SetArticulation(id) => Some(Pending::Articulation(*id)),
                Command::Properties => Some(Pending::Properties),
                _ => None,
            }
        })
    }

    /// Draw the menu, in roll space.
    pub fn paint(&self, labels: &mut Labeller, look: &Look) -> Scene {
        let mut scene = Scene::new();
        let bounds = self.bounds();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            look.panel,
            None,
            &bounds.to_rounded_rect(3.0),
        );
        scene.stroke(
            &stroke_of(1.0),
            Affine::IDENTITY,
            look.border_strong,
            None,
            &bounds.to_rounded_rect(3.0),
        );
        for (i, (item, rect)) in self.items.iter().zip(&self.rects).enumerate() {
            if item.group_break {
                let y = BREAK_H.mul_add(-0.5, rect.y0);
                scene.stroke(
                    &stroke_of(1.0),
                    Affine::IDENTITY,
                    look.panel_border,
                    None,
                    &Line::new((rect.x0 + 6.0, y), (rect.x1 - 6.0, y)),
                );
            }
            if self.hover == Some(i) && item.enabled {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    look.control_hover,
                    None,
                    &rect.inset(-2.0).inset(2.0),
                );
            }
            let ink = if item.enabled {
                look.text
            } else {
                with_alpha(look.text, 0.35)
            };
            let shaped = labels.shape(&item.label, FONT);
            let baseline = text_top(rect.center().y, &shaped);
            text::draw(
                &mut scene,
                &shaped,
                rect.x0 + 10.0,
                baseline,
                text::Align::Left,
                ink,
                Affine::IDENTITY,
            );
            if let Some(key) = item.shortcut {
                let shaped = labels.shape(key, FONT);
                text::draw(
                    &mut scene,
                    &shaped,
                    rect.x1 - 10.0,
                    baseline,
                    text::Align::Right,
                    look.text_dim,
                    Affine::IDENTITY,
                );
            }
        }
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use expression_editor_core::Viewport;

    fn editor() -> Editor {
        demo::editor(demo::Scene::Drums, Viewport::new(800.0, 400.0))
    }

    #[test]
    fn the_toolbar_shows_the_editor_s_tool_lit() {
        let mut ed = editor();
        ed.tool = Tool::Pen;
        let mut labels = Labeller::new();
        let bar = toolbar(&ed, 2000.0, &mut labels);
        let lit: Vec<_> = bar
            .iter()
            .filter(|b| matches!(b.control, Control::Tool(_)) && b.active)
            .collect();
        // Drum mode shows the pen as note-draw; the lit button is
        // whatever the editor says it is showing.
        assert_eq!(lit.len(), 1);
        assert_eq!(lit[0].control, Control::Tool(ed.shown_tool()));
    }

    #[test]
    fn a_narrow_bar_drops_from_the_right_and_never_clips() {
        let ed = editor();
        let mut labels = Labeller::new();
        let bar = toolbar(&ed, 300.0, &mut labels);
        assert!(!bar.is_empty());
        assert!(bar.iter().all(|b| b.rect.x1 <= 300.0 - GAP));
        // The first control is still the first mode.
        assert_eq!(bar[0].control, Control::Mode(Mode::ALL[0]));
    }

    #[test]
    fn a_press_on_a_tool_button_sets_the_tool() {
        let mut ed = editor();
        let mut labels = Labeller::new();
        let bar = toolbar(&ed, 2000.0, &mut labels);
        let button = bar
            .iter()
            .find(|b| b.control == Control::Tool(Tool::Eraser))
            .expect("an eraser button");
        let c = button.rect.center();
        let control = hit(&bar, c.x, c.y).expect("the button under its centre");
        assert!(activate(&mut ed, &Drag::None, control));
        assert_eq!(ed.tool, Tool::Eraser);
    }

    #[test]
    fn the_status_bar_toggles_snap_and_steps_the_grid() {
        let mut ed = editor();
        let was = ed.grid.enabled;
        assert!(activate(&mut ed, &Drag::None, Control::SnapGrid));
        assert_eq!(ed.grid.enabled, !was);
        let before = ed.grid.division;
        activate(&mut ed, &Drag::None, Control::GridFiner);
        assert!(ed.grid.division < before);
    }

    #[test]
    fn undo_is_dim_until_there_is_something_to_undo() {
        let mut ed = editor();
        let mut labels = Labeller::new();
        let dim = toolbar(&ed, 2000.0, &mut labels)
            .iter()
            .find(|b| b.control == Control::Undo)
            .map(|b| b.enabled);
        assert_eq!(dim, Some(false));
        let id = ed.doc.notes[0].id;
        ed.apply(&Edit::SetVelocity {
            notes: vec![id],
            velocity: 0.5,
        });
        let lit = toolbar(&ed, 2000.0, &mut labels)
            .iter()
            .find(|b| b.control == Control::Undo)
            .map(|b| b.enabled);
        assert_eq!(lit, Some(true));
    }

    #[test]
    fn the_menu_lays_items_out_and_runs_one() {
        let mut ed = editor();
        let id = ed.doc.notes[0].id;
        ed.selection.set_single(id);
        let menu = Menu::open(&ed, 100.0, 50.0, Some(id), 0.0, 0);
        assert!(!menu.items.is_empty());
        let delete = menu
            .items
            .iter()
            .position(|i| i.command == Command::Delete)
            .expect("a delete item");
        let c = menu.rects[delete].center();
        let count = ed.doc.notes.len();
        let outcome = menu.choose(&mut ed, c.x, c.y);
        assert_eq!(outcome, Choice::Done(None));
        assert_eq!(ed.doc.notes.len(), count - 1);
        // Outside every item is not a choice.
        assert_eq!(menu.choose(&mut ed, 5.0, 5.0), Choice::Missed);
    }
}
