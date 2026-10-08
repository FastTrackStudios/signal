//! The routing grid, drawn on the GPU — the rig's grid (signal-grid-ui's
//! look: modules as containers with a header, blocks as cells bordered in
//! their type's colour, a port either side, cables between them) painted as
//! one vector scene into the window's renderer, with no DOM per cell.
//!
//! Unfolded by default: the chain left to right in one row, as tall as the
//! patch needs (a stereo pair stacks two cells), fitted to the view's
//! height. Folded, it wraps at the view's width — a long module carries on
//! down the next row, the cable bending back to it.
//!
//! One finger pans, two pinch; a tap picks a cell or a module's header, and
//! a cell's light switches the block. Picks go to `on_pick`.

use dioxus::prelude::*;

/// One block's cell.
#[derive(Clone, PartialEq, Debug)]
pub struct CanvasCell {
    pub id: String,
    pub name: String,
    pub sub: String,
    /// Its block type, lower case: its glyph.
    pub kind: String,
    /// Its type's colour, `#rrggbb`.
    pub colour: String,
    pub lit: bool,
    pub edited: bool,
    /// A slot with nothing loaded (an amp's cab with no IR): dashed.
    pub empty: bool,
}

/// One module: its name, what it plays, its colour, and what it holds left
/// to right — columns of blocks (the chain runs down each), and modules
/// inside it (Core holds the Amp).
#[derive(Clone, PartialEq, Debug)]
pub struct CanvasModule {
    pub name: String,
    pub label: String,
    pub colour: String,
    pub items: Vec<CanvasItem>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum CanvasItem {
    /// Blocks in a column, the chain running down it.
    Col(Vec<CanvasCell>),
    /// Effects in parallel with the dry: one above the line, one below,
    /// the dry straight through the middle.
    Split(Vec<CanvasCell>),
    /// Two lanes side by side, each its own (a stereo pair: Amp L over
    /// Amp R), one above the line and one below.
    Pair(Vec<CanvasCell>),
    Sub(CanvasModule),
}

impl CanvasModule {
    /// Every block in it, inner modules' too, in chain order.
    pub fn ids(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| match i {
                CanvasItem::Col(c) | CanvasItem::Split(c) | CanvasItem::Pair(c) => c.iter().map(|c| c.id.clone()).collect(),
                CanvasItem::Sub(m) => m.ids(),
            })
            .collect()
    }
}

/// What a tap picked.
#[derive(Clone, PartialEq, Debug)]
pub enum CanvasPick {
    Block(String),
    Module(String, Vec<String>),
    /// A cell's light: switch the block on or off.
    Toggle(String),
    /// Empty grid: nothing.
    Clear,
}

/// What is selected, for the highlight: a block's id or a module's name.
#[derive(Clone, PartialEq, Debug)]
pub enum CanvasSel {
    Block(String),
    Module(String),
}

#[component]
pub fn RoutingCanvas(
    modules: Vec<CanvasModule>,
    selected: Option<CanvasSel>,
    /// Wrap at the view's width (else one row).
    fold: bool,
    /// Bumped to fit again (the zoom back to the view).
    fit: u32,
    on_pick: EventHandler<CanvasPick>,
) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::canvas(modules, selected, fold, fit, on_pick)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (modules, selected, fold, fit, on_pick);
        rsx! { div {} }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use anyrender::{PaintScene, RenderContext, Scene};
    use blitz_dom::Widget;
    use blitz_dom::node::ComputedStyles;
    use blitz_traits::events::{BlitzPointerId, BlitzWheelDelta, UiEvent};
    use dioxus::prelude::*;
    use kurbo::{Affine, BezPath, Circle, Point, Rect, RoundedRect, Stroke};
    use parley::{FontContext, FontFamily, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};
    use vello::peniko::{Color, Fill};

    use super::{CanvasCell, CanvasItem, CanvasModule, CanvasPick, CanvasSel};

    // The grid's measures, pt at zoom 1 (signal-grid-ui's, sized for a finger).
    const CELL: f64 = 92.0;
    const GAP: f64 = 22.0;
    const PAD: f64 = 10.0;
    const HEAD: f64 = 28.0;
    const MOD_GAP: f64 = 28.0;
    const ROW_GAP: f64 = 36.0;
    const EDGE: f64 = 22.0;
    const END: f64 = 40.0;
    const PORT: f64 = 5.0;
    /// The navigator along the top: the whole graph, thin, as a scrollbar.
    const NAV_H: f64 = 44.0;

    /// A piece of a module on the grid (a long one folds into several).
    /// `line` is where the chain's line runs, down from its top.
    #[derive(Clone)]
    struct Piece {
        module: usize,
        items: std::ops::Range<usize>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        line: f64,
        first: bool,
    }

    fn item_w(i: &CanvasItem) -> f64 {
        match i {
            CanvasItem::Col(_) | CanvasItem::Split(_) | CanvasItem::Pair(_) => CELL,
            CanvasItem::Sub(m) => box_w(&m.items),
        }
    }
    /// The grid's lanes, one cell and a gap apart: the chain's line is lane
    /// 0, with one above and one below. A column's blocks take them from
    /// the line down (three: from above it); a split or a pair the outer
    /// two.
    fn lanes(i: &CanvasItem) -> &'static [i32] {
        match i {
            CanvasItem::Col(c) => match c.len() {
                0 | 1 => &[0],
                2 => &[0, 1],
                _ => &[-1, 0, 1],
            },
            CanvasItem::Split(c) | CanvasItem::Pair(c) if c.len() < 2 => &[-1],
            _ => &[-1, 1],
        }
    }
    const LANE: f64 = CELL + GAP;

    /// How far an item reaches above and below the chain's line: its lanes,
    /// a module's box round its own.
    fn ext(i: &CanvasItem) -> (f64, f64) {
        match i {
            CanvasItem::Col(_) | CanvasItem::Split(_) | CanvasItem::Pair(_) => {
                let l = lanes(i);
                let lo = l.iter().copied().min().unwrap_or(0).min(0);
                let hi = l.iter().copied().max().unwrap_or(0).max(0);
                let split = matches!(i, CanvasItem::Split(_));
                // A split's lanes are both sides, whatever it holds.
                let (lo, hi) = if split { (-1, 1) } else { (lo, hi) };
                (f64::from(-lo) * LANE + CELL / 2.0, f64::from(hi) * LANE + CELL / 2.0)
            }
            CanvasItem::Sub(m) => {
                let (a, b) = exts(&m.items);
                (a + HEAD, b + PAD)
            }
        }
    }
    fn exts(items: &[CanvasItem]) -> (f64, f64) {
        items.iter().map(ext).fold((CELL / 2.0, CELL / 2.0), |(a, b), (x, y)| (a.max(x), b.max(y)))
    }
    /// A module's box round `items`, side by side.
    fn box_w(items: &[CanvasItem]) -> f64 {
        PAD * 2.0 + items.iter().map(item_w).sum::<f64>() + items.len().saturating_sub(1) as f64 * GAP
    }

    /// Lay the modules out at zoom 1: in rows `width` wide (fold), or one
    /// row, every module on the row's line. The pieces, and the content's
    /// size.
    fn layout(mods: &[CanvasModule], width: Option<f64>) -> (Vec<Piece>, f64, f64) {
        let mut pieces: Vec<Piece> = Vec::new();
        let start = EDGE + END + MOD_GAP;
        let (mut x, mut row_top) = (start, EDGE);
        // The row's reach above and below its line.
        let (mut above, mut below) = (0.0_f64, 0.0_f64);
        let mut max_x = x;
        let mut row_from = 0;
        let settle = |pieces: &mut Vec<Piece>, from: usize, top: f64, above: f64| {
            for p in &mut pieces[from..] {
                p.y = top + above - p.line;
            }
        };
        for (mi, m) in mods.iter().enumerate() {
            let (a, b) = exts(&m.items);
            let (line, h) = (HEAD + a, HEAD + a + b + PAD);
            let mut c = 0;
            while c < m.items.len() {
                let room = width.map_or(f64::INFINITY, |w| w - EDGE - x);
                // As many of its items as fit here.
                let mut n = 0;
                while c + n < m.items.len() && box_w(&m.items[c..c + n + 1]) <= room {
                    n += 1;
                }
                let all = n == m.items.len() - c;
                // Whole on a fresh row, when it would fit there; a fresh row
                // when nothing fits here.
                let fits_fresh = width.is_some_and(|w| box_w(&m.items[c..]) <= w - EDGE - start);
                if x > start && (n == 0 || (!all && c == 0 && fits_fresh)) {
                    settle(&mut pieces, row_from, row_top, above);
                    row_from = pieces.len();
                    row_top += above + below + ROW_GAP;
                    (above, below) = (0.0, 0.0);
                    x = start;
                    continue;
                }
                let n = n.max(1);
                let w = box_w(&m.items[c..c + n]);
                pieces.push(Piece { module: mi, items: c..c + n, x, y: row_top, w, h, line, first: c == 0 });
                above = above.max(line);
                below = below.max(h - line);
                x += w + MOD_GAP;
                max_x = max_x.max(x);
                c += n;
            }
        }
        settle(&mut pieces, row_from, row_top, above);
        (pieces, max_x + END + EDGE, row_top + above + below + EDGE)
    }

    /// A step of the chain: a block, or effects in parallel with the dry
    /// (their boxes, and the dry's line through the middle).
    #[derive(Clone)]
    enum Stage {
        Block(Rect),
        /// Parallel lanes, and the dry's line through them (none for a pair).
        Split(Vec<Rect>, Option<(f64, f64, f64)>),
    }

    impl Stage {
        fn ins(&self) -> Vec<Point> {
            match self {
                Stage::Block(r) => vec![Point::new(r.x0, r.center().y)],
                Stage::Split(rs, dry) => rs.iter().map(|r| Point::new(r.x0, r.center().y)).chain(dry.map(|(x0, _, y)| Point::new(x0, y))).collect(),
            }
        }
        fn outs(&self) -> Vec<Point> {
            match self {
                Stage::Block(r) => vec![Point::new(r.x1, r.center().y)],
                Stage::Split(rs, dry) => rs.iter().map(|r| Point::new(r.x1, r.center().y)).chain(dry.map(|(_, x1, y)| Point::new(x1, y))).collect(),
            }
        }
    }

    /// Where everything in a run of items goes: the chain's stages, its
    /// cells, and the boxes of the modules inside it.
    #[derive(Default)]
    struct Placed {
        stages: Vec<Stage>,
        cells: Vec<(Rect, CanvasCell)>,
        boxes: Vec<(Rect, CanvasModule)>,
    }

    /// Place `items` from `x0`, on the chain's line at `line`.
    fn place(items: &[CanvasItem], x0: f64, line: f64, out: &mut Placed) {
        let mut x = x0;
        for it in items {
            let cell = |lane: i32| {
                let y = line + f64::from(lane) * LANE - CELL / 2.0;
                Rect::new(x, y, x + CELL, y + CELL)
            };
            match it {
                CanvasItem::Col(col) => {
                    for (c, l) in col.iter().zip(lanes(it)) {
                        out.stages.push(Stage::Block(cell(*l)));
                        out.cells.push((cell(*l), c.clone()));
                    }
                }
                CanvasItem::Split(cells) | CanvasItem::Pair(cells) => {
                    let rects: Vec<Rect> = lanes(it).iter().take(cells.len()).map(|l| cell(*l)).collect();
                    out.cells.extend(rects.iter().copied().zip(cells.iter().cloned()));
                    let dry = matches!(it, CanvasItem::Split(_)).then_some((x, x + CELL, line));
                    out.stages.push(Stage::Split(rects, dry));
                }
                CanvasItem::Sub(m) => {
                    let (a, b) = exts(&m.items);
                    out.boxes.push((Rect::new(x, line - a - HEAD, x + box_w(&m.items), line + b + PAD), m.clone()));
                    place(&m.items, x + PAD, line, out);
                }
            }
            x += item_w(it) + GAP;
        }
    }

    /// What a spot on the grid is.
    #[derive(Clone)]
    enum Hit {
        Cell(String),
        Light(String),
        Module(String, Vec<String>),
    }

    enum Gesture {
        Idle,
        /// One finger down, not moved yet: a tap, unless it moves.
        Pending { at: (f64, f64), pan0: (f64, f64) },
        Pan { at: (f64, f64), pan0: (f64, f64) },
        Pinch { d0: f64, z0: f64, mid0: (f64, f64), pan0: (f64, f64) },
        /// A finger on the navigator: the view follows it.
        Nav,
    }

    struct State {
        modules: Vec<CanvasModule>,
        selected: Option<CanvasSel>,
        fold: bool,
        fit: u32,
        /// The zoom and pan; `fitted` while the zoom follows the view.
        zoom: f64,
        pan: (f64, f64),
        fitted: bool,
        /// Back to the start on the next paint (a Fit, a fold).
        reset_pan: bool,
        /// The view's size, pt (from the last paint).
        view: (f64, f64),
        fingers: HashMap<u64, (f64, f64)>,
        /// The navigator's left edge and scale (from the last paint).
        nav: (f64, f64),
        gesture: Gesture,
        hits: Vec<(Rect, Hit)>,
        picks: Vec<CanvasPick>,
        dirty: bool,
        fonts: FontContext,
        layouts: LayoutContext<()>,
        texts: HashMap<(String, u32, u32), Layout<()>>,
    }

    type Shared = Rc<RefCell<State>>;

    pub(super) fn canvas(modules: Vec<CanvasModule>, selected: Option<CanvasSel>, fold: bool, fit: u32, on_pick: EventHandler<CanvasPick>) -> Element {
        let state: Shared = use_hook(|| {
            Rc::new(RefCell::new(State {
                modules: Vec::new(),
                selected: None,
                fold,
                fit,
                zoom: 1.0,
                pan: (0.0, 0.0),
                fitted: true,
                reset_pan: true,
                view: (0.0, 0.0),
                fingers: HashMap::new(),
                nav: (0.0, 0.0),
                gesture: Gesture::Idle,
                hits: Vec::new(),
                picks: Vec::new(),
                dirty: true,
                fonts: FontContext::new(),
                layouts: LayoutContext::new(),
                texts: HashMap::new(),
            }))
        });
        // A change of patch, selection or layout: drawn again (the
        // attribute below changes, so the page repaints).
        let mut rev = use_signal(|| 0u64);
        {
            let mut s = state.borrow_mut();
            let mut changed = false;
            if s.modules != modules {
                s.modules = modules;
                changed = true;
            }
            if s.selected != selected {
                s.selected = selected;
                changed = true;
            }
            if s.fold != fold || s.fit != fit {
                s.fold = fold;
                s.fit = fit;
                s.fitted = true;
                s.reset_pan = true;
                changed = true;
            }
            if changed {
                s.dirty = true;
                drop(s);
                let next = *rev.peek() + 1;
                rev.set(next);
            }
        }
        let update = {
            let schedule = dioxus_core::schedule_update();
            std::sync::Arc::new(move || schedule()) as std::sync::Arc<dyn Fn() + Send + Sync>
        };
        let attr = use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(CanvasWidget { state: Rc::clone(&state), update: update.clone() }));
        // Picks go out from a task, not from inside a render.
        let picks: Vec<CanvasPick> = std::mem::take(&mut state.borrow_mut().picks);
        if !picks.is_empty() {
            spawn(async move {
                for p in picks {
                    on_pick.call(p);
                }
            });
        }
        rsx! {
            object { style: "display: block; width: 100%; height: 100%; touch-action: none;", "data-rev": "{rev}", data: attr }
        }
    }

    struct CanvasWidget {
        state: Shared,
        update: std::sync::Arc<dyn Fn() + Send + Sync>,
    }

    fn pointer_key(id: BlitzPointerId) -> u64 {
        match id {
            BlitzPointerId::Mouse => 0,
            BlitzPointerId::Pen => u64::MAX,
            BlitzPointerId::Finger(f) => f.wrapping_add(1),
        }
    }

    fn two(fingers: &HashMap<u64, (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
        let mut it = fingers.values();
        Some((*it.next()?, *it.next()?))
    }

    impl Widget for CanvasWidget {
        fn handle_event(&mut self, event: &UiEvent) {
            let mut s = self.state.borrow_mut();
            match event {
                UiEvent::PointerDown(e) => {
                    let at = (f64::from(e.element.x), f64::from(e.element.y));
                    s.fingers.insert(pointer_key(e.id), at);
                    s.gesture = match two(&s.fingers).filter(|_| s.fingers.len() == 2) {
                        Some((a, b)) => Gesture::Pinch {
                            d0: ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt().max(1.0),
                            z0: s.zoom,
                            mid0: ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
                            pan0: s.pan,
                        },
                        None if at.1 < NAV_H => {
                            nav_to(&mut s, at.0);
                            Gesture::Nav
                        }
                        None => Gesture::Pending { at, pan0: s.pan },
                    };
                }
                UiEvent::PointerMove(e) => {
                    let key = pointer_key(e.id);
                    if !s.fingers.contains_key(&key) {
                        return;
                    }
                    let p = (f64::from(e.element.x), f64::from(e.element.y));
                    s.fingers.insert(key, p);
                    match s.gesture {
                        Gesture::Pending { at, pan0 } if ((p.0 - at.0).powi(2) + (p.1 - at.1).powi(2)).sqrt() > 6.0 => {
                            s.gesture = Gesture::Pan { at, pan0 };
                        }
                        _ => {}
                    }
                    match s.gesture {
                        Gesture::Nav => nav_to(&mut s, p.0),
                        Gesture::Pan { at, pan0 } => {
                            s.pan = (pan0.0 + p.0 - at.0, pan0.1 + p.1 - at.1);
                            s.dirty = true;
                        }
                        Gesture::Pinch { d0, z0, mid0, pan0 } => {
                            if let Some((a, b)) = two(&s.fingers) {
                                let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                                let z = (z0 * d / d0).clamp(0.4, 2.5);
                                let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                                // The content under the fingers stays under them.
                                let c = ((mid0.0 - pan0.0) / z0, (mid0.1 - pan0.1) / z0);
                                s.zoom = z;
                                s.pan = (mid.0 - c.0 * z, mid.1 - c.1 * z);
                                s.fitted = false;
                                s.dirty = true;
                            }
                        }
                        _ => {}
                    }
                }
                UiEvent::PointerUp(e) => {
                    let p = (f64::from(e.element.x), f64::from(e.element.y));
                    s.fingers.remove(&pointer_key(e.id));
                    if let Gesture::Pending { .. } = s.gesture {
                        // A tap: what is under it.
                        let at = Point::new((p.0 - s.pan.0) / s.zoom, (p.1 - NAV_H - s.pan.1) / s.zoom);
                        let hit = s.hits.iter().rev().find(|(r, _)| r.contains(at)).map(|(_, h)| h.clone());
                        let pick = match hit {
                            Some(Hit::Light(id)) => CanvasPick::Toggle(id),
                            Some(Hit::Cell(id)) => CanvasPick::Block(id),
                            Some(Hit::Module(name, ids)) => CanvasPick::Module(name, ids),
                            None => CanvasPick::Clear,
                        };
                        s.picks.push(pick);
                        drop(s);
                        (self.update)();
                        s = self.state.borrow_mut();
                    }
                    s.gesture = Gesture::Idle;
                }
                UiEvent::PointerCancel(e) => {
                    s.fingers.remove(&pointer_key(e.id));
                    s.gesture = Gesture::Idle;
                }
                UiEvent::Wheel(e) => {
                    let (dx, dy) = match e.delta {
                        BlitzWheelDelta::Lines(x, y) => (x * 30.0, y * 30.0),
                        BlitzWheelDelta::Pixels(x, y) => (x, y),
                    };
                    s.pan = (s.pan.0 + dx, s.pan.1 + dy);
                    s.dirty = true;
                }
                _ => {}
            }
        }

        fn needs_redraw(&self) -> bool {
            self.state.try_borrow().is_ok_and(|s| s.dirty)
        }

        fn paint(&mut self, _render_ctx: &mut dyn RenderContext, _styles: &ComputedStyles, width: u32, height: u32, scale: f64) -> Scene {
            let mut scene = Scene::new();
            if width < 2 || height < 2 {
                return scene;
            }
            let mut s = self.state.borrow_mut();
            s.dirty = false;
            let view = (f64::from(width) / scale, f64::from(height) / scale);
            s.view = view;
            // The graph's part of the view, under the navigator.
            let gh = (view.1 - NAV_H).max(1.0);
            // Unfolded and fitted: the row as tall as the view.
            let natural = layout(&s.modules, None);
            if s.fitted {
                s.zoom = if s.fold { 1.0 } else { (gh / natural.2).clamp(0.45, 0.9) };
            }
            let wrap = s.fold.then(|| view.0 / s.zoom);
            let (pieces, cw, ch) = layout(&s.modules, wrap);
            if s.reset_pan {
                s.reset_pan = false;
                s.pan = (0.0, 0.0);
            }
            // Panning stops at the content's edges; content shorter than
            // the view sits centred in it.
            let min_x = (view.0 - cw * s.zoom).min(0.0);
            let spare_y = gh - ch * s.zoom;
            let pan_y = if spare_y >= 0.0 { spare_y / 2.0 } else { s.pan.1.clamp(spare_y, 0.0) };
            s.pan = (s.pan.0.clamp(min_x, 0.0), pan_y);
            let t = Affine::scale(scale) * Affine::translate((s.pan.0, NAV_H + s.pan.1)) * Affine::scale(s.zoom);
            let mut hits: Vec<(Rect, Hit)> = Vec::new();
            let st = &mut *s;

            // Everything's place: each piece's box, the boxes inside it,
            // its cells.
            let placed: Vec<(Piece, Placed)> = pieces
                .iter()
                .map(|p| {
                    let mut out = Placed::default();
                    place(&st.modules[p.module].items[p.items.clone()], p.x + PAD, p.y + p.line, &mut out);
                    (p.clone(), out)
                })
                .collect();

            // The modules' boxes, then the cables over them, then the cells.
            for (p, pl) in &placed {
                let m = st.modules[p.module].clone();
                let r = Rect::new(p.x, p.y, p.x + p.w, p.y + p.h);
                draw_box(&mut scene, st, t, r, &m, p.first, &mut hits);
                for (r, inner) in &pl.boxes {
                    draw_box(&mut scene, st, t, *r, inner, true, &mut hits);
                }
            }
            // The cables, through the chain's stages:
            // down a column, on to the next, out to parallel effects and
            // back.
            let stages: Vec<Stage> = placed.iter().flat_map(|(_, pl)| pl.stages.iter().cloned()).collect();
            let cable = Color::from_rgba8(0x52, 0x52, 0x5b, 0xff);
            let stroke = Stroke::new(2.0);
            let mut line = |scene: &mut Scene, a: Point, b: Point| scene.stroke(&stroke, t, cable, None, &wire(a, b, ROW_GAP));
            if let (Some(first), Some(last)) = (stages.first(), stages.last()) {
                let y_in = first.ins().last().map_or(0.0, |p| p.y);
                let y_out = last.outs().last().map_or(0.0, |p| p.y);
                let x_end = pieces.iter().map(|p| p.x + p.w).fold(0.0, f64::max) + MOD_GAP;
                end_pill(&mut scene, st, t, Rect::new(EDGE, y_in - 15.0, EDGE + END, y_in + 15.0), "IN");
                end_pill(&mut scene, st, t, Rect::new(x_end, y_out - 15.0, x_end + END, y_out + 15.0), "OUT");
                for i in first.ins() {
                    line(&mut scene, Point::new(EDGE + END, y_in), i);
                }
                for o in last.outs() {
                    line(&mut scene, o, Point::new(x_end, y_out));
                }
            }
            for s2 in &stages {
                if let Stage::Split(_, Some((x0, x1, y))) = s2 {
                    line(&mut scene, Point::new(*x0, *y), Point::new(*x1, *y));
                }
            }
            for w in stages.windows(2) {
                match (&w[0], &w[1]) {
                    (Stage::Block(a), Stage::Block(b)) if (a.x0 - b.x0).abs() < 0.5 && b.y0 > a.y0 => {
                        // Down the column.
                        scene.stroke(&stroke, t, cable, None, &kurbo::Line::new((a.center().x, a.y1), (b.center().x, b.y0)));
                    }
                    (a, b) => {
                        let (outs, ins) = (a.outs(), b.ins());
                        let pairs = matches!((a, b), (Stage::Split(_, None), Stage::Split(_, None))) && outs.len() == ins.len();
                        if pairs {
                            // A stereo pair into a pair: lane to lane.
                            for (o, i) in outs.iter().zip(&ins) {
                                line(&mut scene, *o, *i);
                            }
                        } else if outs.len() == 1 || ins.len() == 1 {
                            for o in &outs {
                                for i in &ins {
                                    line(&mut scene, *o, *i);
                                }
                            }
                        } else {
                            // Parallel into parallel: they join on the line between.
                            let j = Point::new((outs[0].x + ins[0].x) / 2.0, ins[ins.len() - 1].y);
                            for o in &outs {
                                line(&mut scene, *o, j);
                            }
                            for i in &ins {
                                line(&mut scene, j, *i);
                            }
                        }
                    }
                }
            }

            for (_, pl) in &placed {
                for (rect, c) in &pl.cells {
                    draw_cell(&mut scene, st, t, *rect, c);
                    hits.push((*rect, Hit::Cell(c.id.clone())));
                    hits.push((Rect::new(rect.x1 - 30.0, rect.y0, rect.x1, rect.y0 + 30.0), Hit::Light(c.id.clone())));
                }
            }
            st.hits = hits;

            // The navigator: every module and block, small, the whole width;
            // the part in view framed.
            let ts = Affine::scale(scale);
            let bar = Rect::new(0.0, 0.0, view.0, NAV_H);
            scene.fill(Fill::NonZero, ts, Color::from_rgba8(0x0b, 0x0b, 0x0e, 0xff), None, &bar);
            scene.fill(Fill::NonZero, ts, Color::from_rgba8(0x27, 0x27, 0x2a, 0xff), None, &Rect::new(0.0, NAV_H - 1.0, view.0, NAV_H));
            // Stretched to the bar: the whole width, the bar's height.
            let m = (view.0 - 24.0) / cw;
            let my = (NAV_H - 10.0) / ch;
            let x0 = 12.0;
            st.nav = (x0, m);
            let mini = Affine::translate((x0, 5.0)) * Affine::scale_non_uniform(m, my);
            for (p, pl) in &placed {
                let colour = hex(&st.modules[p.module].colour);
                let dark = Color::from_rgba8(0x14, 0x14, 0x18, 0xff);
                scene.fill(Fill::NonZero, ts * mini, mix(colour, dark, 0.22), None, &RoundedRect::new(p.x, p.y, p.x + p.w, p.y + p.h, 4.0));
                for (r, inner) in &pl.boxes {
                    scene.fill(Fill::NonZero, ts * mini, mix(hex(&inner.colour), dark, 0.3), None, &RoundedRect::from_rect(*r, 4.0));
                }
                for (r, c) in &pl.cells {
                    let fill = if c.lit { hex(&c.colour) } else { Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff) };
                    scene.fill(Fill::NonZero, ts * mini, fill, None, &r.inset(-4.0));
                }
            }
            let (vx0, vx1) = (-st.pan.0 / st.zoom, (view.0 - st.pan.0) / st.zoom);
            let frame = RoundedRect::new(x0 + vx0 * m, 3.0, (x0 + vx1 * m).min(view.0 - 2.0), NAV_H - 4.0, 5.0);
            scene.fill(Fill::NonZero, ts, Color::from_rgba8(0xff, 0xff, 0xff, 0x14), None, &frame);
            scene.stroke(&Stroke::new(1.5), ts, Color::from_rgba8(0xe4, 0xe4, 0xe7, 0xff), None, &frame);
            scene
        }
    }

    /// The view centred on the navigator's `x`.
    fn nav_to(s: &mut State, x: f64) {
        let (x0, m) = s.nav;
        if m <= 0.0 {
            return;
        }
        let at = (x - x0) / m;
        s.pan.0 = s.view.0 / 2.0 - at * s.zoom;
        s.dirty = true;
    }

    /// A cable from an out port to an in port: a curve along a row, or down
    /// round the row's foot and back for a fold.
    fn wire(a: Point, b: Point, row_gap: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((a.x, a.y));
        if b.x >= a.x {
            let dx = ((b.x - a.x) / 2.0).max(10.0);
            p.curve_to((a.x + dx, a.y), (b.x - dx, b.y), (b.x, b.y));
        } else {
            // Folded: out, down to the gap under the row, back, down, in.
            let out = a.x + 11.0;
            let back = b.x - 11.0;
            let mid = b.y - CELL / 2.0 - HEAD - row_gap / 2.0;
            p.line_to((out, a.y));
            p.line_to((out, mid));
            p.line_to((back, mid));
            p.line_to((back, b.y));
            p.line_to((b.x, b.y));
        }
        p
    }

    /// A module's box and its header: a mark, its name, what it plays.
    fn draw_box(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, m: &CanvasModule, first: bool, hits: &mut Vec<(Rect, Hit)>) {
        let colour = hex(&m.colour);
        let on = matches!(&st.selected, Some(CanvasSel::Module(n)) if *n == m.name);
        let rr = RoundedRect::from_rect(r, 10.0);
        scene.fill(Fill::NonZero, t, mix(colour, Color::from_rgba8(0x14, 0x14, 0x18, 0xff), if on { 0.16 } else { 0.07 }), None, &rr);
        let edge = if on { colour } else { Color::from_rgba8(0x2a, 0x2a, 0x31, 0xff) };
        scene.stroke(&Stroke::new(if on { 2.0 } else { 1.0 }), t, edge, None, &rr);
        glyph(scene, t, &m.name.to_lowercase(), r.x0 + PAD - 1.0, r.y0 + 6.0, 14.0, colour);
        let name = if first { m.name.to_uppercase() } else { format!("{} ›", m.name.to_uppercase()) };
        let room = r.width() - PAD * 2.0 - 14.0;
        let nw = text(scene, st, t, &name, 10.5, 800.0, lift(colour), r.x0 + PAD + 14.0, r.y0 + 17.5, room, false);
        if first && !m.label.is_empty() {
            text(scene, st, t, &m.label, 10.5, 600.0, Color::from_rgba8(0xa1, 0xa1, 0xaa, 0xff), r.x0 + PAD + 22.0 + nw, r.y0 + 17.5, (room - 8.0 - nw).max(0.0), false);
        }
        // Anywhere in its box that isn't a block picks the module.
        hits.push((r, Hit::Module(m.name.clone(), m.ids())));
    }

    fn draw_cell(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, c: &CanvasCell) {
        let colour = hex(&c.colour);
        let on = matches!(&st.selected, Some(CanvasSel::Block(id)) if *id == c.id);
        let rr = RoundedRect::from_rect(r, 9.0);
        let base = Color::from_rgba8(0x17, 0x17, 0x1b, 0xff);
        if c.empty {
            let edge = if on { Color::from_rgba8(0xf4, 0xf4, 0xf5, 0xff) } else { Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff) };
            scene.stroke(&Stroke::new(1.5).with_dashes(0.0, [5.0, 4.0]), t, edge, None, &rr);
            for x in [r.x0, r.x1] {
                scene.fill(Fill::NonZero, t, Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff), None, &Circle::new((x, r.center().y), PORT));
            }
            glyph(scene, t, &c.kind, r.x0 + 9.0, r.y0 + 8.0, 15.0, Color::from_rgba8(0x52, 0x52, 0x5b, 0xff));
            text(scene, st, t, &c.name, 12.5, 700.0, Color::from_rgba8(0x52, 0x52, 0x5b, 0xff), r.x0 + 10.0, r.center().y + 4.5, CELL - 20.0, true);
            return;
        }
        scene.fill(Fill::NonZero, t, if c.lit { mix(colour, base, 0.16) } else { base }, None, &rr);
        let edge = if on {
            Color::from_rgba8(0xf4, 0xf4, 0xf5, 0xff)
        } else if c.lit {
            mix(colour, base, 0.75)
        } else {
            Color::from_rgba8(0x2c, 0x2c, 0x33, 0xff)
        };
        scene.stroke(&Stroke::new(if on { 2.5 } else { 1.5 }), t, edge, None, &rr);
        // Its ports.
        let port = if c.lit { colour } else { Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff) };
        for x in [r.x0, r.x1] {
            scene.fill(Fill::NonZero, t, port, None, &Circle::new((x, r.center().y), PORT));
        }
        // What it is.
        glyph(scene, t, &c.kind, r.x0 + 9.0, r.y0 + 8.0, 15.0, if c.lit { colour } else { Color::from_rgba8(0x52, 0x52, 0x5b, 0xff) });
        // Its light.
        let light = if c.lit { Color::from_rgba8(0x22, 0xc5, 0x5e, 0xff) } else { Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff) };
        scene.fill(Fill::NonZero, t, light, None, &Circle::new((r.x1 - 13.0, r.y0 + 13.0), 4.5));
        // Its name, and what it plays.
        let ink = if c.lit { Color::from_rgba8(0xe4, 0xe4, 0xe7, 0xff) } else { Color::from_rgba8(0x71, 0x71, 0x7a, 0xff) };
        let cy = r.center().y;
        if c.sub.is_empty() {
            text(scene, st, t, &c.name, 12.5, 700.0, ink, r.x0 + 10.0, cy + 4.5, CELL - 20.0, true);
        } else {
            text(scene, st, t, &c.name, 12.5, 700.0, ink, r.x0 + 10.0, cy - 2.0, CELL - 20.0, true);
            text(scene, st, t, &c.sub, 10.0, 550.0, Color::from_rgba8(0x71, 0x71, 0x7a, 0xff), r.x0 + 8.0, cy + 14.0, CELL - 16.0, true);
        }
        if c.edited {
            text(scene, st, t, "Edited", 9.5, 750.0, Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff), r.x0 + 8.0, r.y1 - 10.0, CELL - 16.0, true);
        }
    }

    fn end_pill(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, label: &str) {
        let rr = RoundedRect::from_rect(r, r.height() / 2.0);
        scene.fill(Fill::NonZero, t, Color::from_rgba8(0x0b, 0x0b, 0x0e, 0xff), None, &rr);
        scene.stroke(&Stroke::new(1.5), t, Color::from_rgba8(0x3f, 0x3f, 0x46, 0xff), None, &rr);
        text(scene, st, t, label, 10.0, 800.0, Color::from_rgba8(0x71, 0x71, 0x7a, 0xff), r.x0, r.center().y + 3.5, r.width(), true);
    }

    /// Draw `s` with its baseline at `y`, from `x` (or centred in `max_w`),
    /// cut at `max_w`. Returns its width.
    #[allow(clippy::too_many_arguments)]
    fn text(scene: &mut Scene, st: &mut State, t: Affine, s: &str, size: f32, weight: f32, colour: Color, x: f64, y: f64, max_w: f64, centre: bool) -> f64 {
        if s.is_empty() || max_w <= 0.0 {
            return 0.0;
        }
        let key = (s.to_string(), size.to_bits(), weight.to_bits());
        if !st.texts.contains_key(&key) {
            let mut b = st.layouts.ranged_builder(&mut st.fonts, s, 1.0, true);
            b.push_default(StyleProperty::FontFamily(FontFamily::Source("system-ui".into())));
            b.push_default(StyleProperty::FontSize(size));
            b.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
            let mut layout: Layout<()> = b.build(s);
            layout.break_all_lines(None);
            st.texts.insert(key.clone(), layout);
        }
        let layout = &st.texts[&key];
        let w = f64::from(layout.width());
        let x = if centre { x + ((max_w - w) / 2.0).max(0.0) } else { x };
        let at = t * Affine::translate((x, y));
        let limit = max_w as f32;
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else { continue };
                let r = run.run();
                scene.draw_glyphs(
                    r.font(),
                    r.font_size(),
                    false,
                    r.normalized_coords(),
                    kurbo::Vec2::ZERO,
                    Fill::NonZero,
                    &anyrender::Paint::from(colour),
                    1.0,
                    at * Affine::translate((0.0, -f64::from(run.baseline()))),
                    None,
                    run.positioned_glyphs().filter(|g| g.x + 6.0 <= limit).map(|g| anyrender::Glyph { id: g.id as _, x: g.x, y: g.y }),
                );
            }
        }
        w.min(max_w)
    }

    /// A glyph's strokes, on a 24 grid: a module's by its name, a block's
    /// by its type.
    fn glyph_paths(key: &str) -> &'static [&'static str] {
        const WAVE: &[&str] = &["M2 12c2.5-6 5-6 7.5 0s5 6 7.5 0 3.5-4 5-2"];
        const SPIN: &[&str] = &["M20 12a8 8 0 1 1-2.3-5.6", "M20 4v4h-4"];
        const DYN: &[&str] = &["M4 5v14h16", "M4 19l6-6 10-4"];
        match key {
            "core" => &["M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z", "M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z"],
            "drive" | "boost" | "saturator" => &["M13 2 4 14h7l-1 8 9-12h-7z"],
            "amp" => &["M3 7h18v12H3z", "M3 11h18", "M7 15h.01", "M11 15h.01"],
            "cabinet" => &["M4 4h16v16H4z", "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z"],
            "pre-fx" => &["M6 20V10", "M12 20V4", "M18 20v-6"],
            "delay" => &["M21 12a9 9 0 1 1-3-6.7", "M21 4v5h-5"],
            "reverb" => &["M12 3v4", "M12 17v4", "M3 12h4", "M17 12h4", "M6 6l2.5 2.5", "M15.5 15.5 18 18", "M18 6l-2.5 2.5", "M8.5 15.5 6 18"],
            "modulation" | "chorus" | "flanger" | "phaser" | "vibrato" => WAVE,
            "motion" | "trem" | "tremolo" | "rotary" => SPIN,
            "dynamics" | "compressor" | "limiter" | "gate" => DYN,
            "eq" | "filter" => &["M3 17c4 0 5-10 9-10s5 10 9 10"],
            "wah" => &["M4 6c0 9 4 12 8 12s8-3 8-12"],
            "utility" => &["M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.5-.5-.5-2.5z"],
            "volume" => &["M11 5 6 9H3v6h3l5 4z", "M15.5 8.5a5 5 0 0 1 0 7"],
            "special" | "pitch" | "doubler" => &["M9 18V5l11-2v13", "M9 18a3 3 0 1 1-6 0a3 3 0 1 1 6 0z", "M20 16a3 3 0 1 1-6 0a3 3 0 1 1 6 0z"],
            _ => &["M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z"],
        }
    }

    fn glyph(scene: &mut Scene, t: Affine, key: &str, x: f64, y: f64, size: f64, colour: Color) {
        let at = t * Affine::translate((x, y)) * Affine::scale(size / 24.0);
        let stroke = Stroke::new(2.2).with_caps(kurbo::Cap::Round).with_join(kurbo::Join::Round);
        for d in glyph_paths(key) {
            if let Ok(path) = BezPath::from_svg(d) {
                scene.stroke(&stroke, at, colour, None, &path);
            }
        }
    }

    fn hex(s: &str) -> Color {
        let h = s.trim_start_matches('#');
        let v = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).unwrap_or(0x71);
        Color::from_rgba8(v(0), v(2), v(4), 0xff)
    }

    /// `a` over `b` at `amount`.
    fn mix(a: Color, b: Color, amount: f32) -> Color {
        let (a, b) = (a.to_rgba8(), b.to_rgba8());
        let m = |x: u8, y: u8| (f32::from(x) * amount + f32::from(y) * (1.0 - amount)).round() as u8;
        Color::from_rgba8(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b), 0xff)
    }

    /// A colour lifted toward white, for a label on the dark.
    fn lift(c: Color) -> Color {
        mix(c, Color::from_rgba8(0xff, 0xff, 0xff, 0xff), 0.78)
    }
}
