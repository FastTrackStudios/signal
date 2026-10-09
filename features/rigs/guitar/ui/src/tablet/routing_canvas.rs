//! The routing grid, drawn on the GPU — the rig's grid (signal-grid-ui's
//! look: modules as containers with a header, blocks as cells bordered in
//! their type's colour, a port either side, cables between them) painted as
//! one vector scene into the window's renderer, with no DOM per cell.
//!
//! The chain left to right in one row, four rows tall, fitted to the
//! view's height; square cables turning in the gaps. A block the manifest
//! has a frame face for wears it, animated, its preset along the top and
//! its name along the foot; off, it is muted.
//!
//! One finger pans, two pinch; a tap picks a block, a module (its box or
//! header) or the Core (its tag). Picks go to `on_pick`.

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
    /// One of the Core's own blocks: it wears the Core's tag.
    pub core: bool,
    /// What picks its frame face, most particular first (`delay:tape`,
    /// `delay`): the manifest's `blocks`.
    pub keys: Vec<String>,
    /// Its params by name, in their own units, and `on`: the face's values.
    pub params: Vec<(String, f64)>,
    /// A level it shows large (a boost's, a trim's dB).
    pub value: Option<String>,
    /// In its module's column but outside its box (the patch's trim under
    /// the dynamics).
    pub loose: bool,
    /// The face it wears when no block face matches its keys: its unit's
    /// own (a drive's pedal) — `(face, namespace, fills)`: fitted whole
    /// between its labels, or filling the block (an amp's faceplate).
    pub fallback: Option<(String, String, bool)>,
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
    /// A module the Core controls: it wears the Core's tag.
    pub core: bool,
    /// No box and no header: blocks that are no module's (the Input column,
    /// what each patch brings in ahead of the drives).
    pub bare: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub enum CanvasItem {
    /// Blocks in a column, the chain running down it.
    Col(Vec<CanvasCell>),
    /// Blocks in parallel, each on the lane it names (the line 0, one
    /// above −1, one below 1): Amp L over Amp R; a block alone off the line
    /// (the amps' EQ on the bottom row).
    Lanes(Vec<(i32, CanvasCell)>),
    /// Blocks in parallel on their lanes, into one block under them in the
    /// same column (the amps into their EQ on the third row).
    Merge(Vec<(i32, CanvasCell)>, (i32, CanvasCell)),
}

/// Everything the Core owns or controls, in chain order: its tagged blocks
/// and every block of its tagged modules.
pub fn core_ids(modules: &[CanvasModule]) -> Vec<String> {
    fn walk(items: &[CanvasItem], all: bool, out: &mut Vec<String>) {
        for i in items {
            match i {
                CanvasItem::Col(c) => {
                    out.extend(c.iter().filter(|c| all || c.core).map(|c| c.id.clone()));
                }
                CanvasItem::Lanes(c) => {
                    out.extend(c.iter().filter(|(_, c)| all || c.core).map(|(_, c)| c.id.clone()));
                }
                CanvasItem::Merge(c, into) => {
                    out.extend(c.iter().chain([into]).filter(|(_, c)| all || c.core).map(|(_, c)| c.id.clone()));
                }
            }
        }
    }
    let mut out = Vec::new();
    for m in modules {
        walk(&m.items, m.core, &mut out);
    }
    out
}

impl CanvasModule {
    /// Every block in it, inner modules' too, in chain order.
    pub fn ids(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| match i {
                CanvasItem::Col(c) => c.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
                CanvasItem::Lanes(c) => c.iter().map(|(_, c)| c.id.clone()).collect(),
                CanvasItem::Merge(c, into) => c.iter().chain([into]).map(|(_, c)| c.id.clone()).collect(),
            })
            .collect()
    }
}

/// What a tap picked.
#[derive(Clone, PartialEq, Debug)]
pub enum CanvasPick {
    Block(String),
    Module(String, Vec<String>),
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
    /// The modules it opens (and fits again) on, first and last.
    focus: (String, String),
    on_pick: EventHandler<CanvasPick>,
) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::canvas(modules, selected, fold, fit, focus, on_pick)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (modules, selected, fold, fit, focus, on_pick);
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
    // Every column one pitch apart, a module's edge or not: two pads and the
    // gap between modules make one gap inside one.
    const GAP: f64 = 16.0;
    const PAD: f64 = 5.0;
    /// A module's header: a finger's height, the module's tap target.
    const HEAD: f64 = 36.0;
    const MOD_GAP: f64 = GAP - 2.0 * PAD;
    const ROW_GAP: f64 = 36.0;
    const EDGE: f64 = 6.0;
    const END: f64 = 40.0;
    const PORT: f64 = 5.0;
    /// A faced block's top and foot bands, for its preset and its name.
    const BAND: f64 = 22.0;

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
            CanvasItem::Col(_) | CanvasItem::Lanes(_) | CanvasItem::Merge(..) => CELL,
        }
    }
    /// The grid's lanes, one cell and a gap apart: the chain's line is lane
    /// 0, with one above and one below. A column's blocks take them from
    /// the line down (three: from above it); a split or a pair the outer
    /// two.
    fn lanes(i: &CanvasItem) -> Vec<i32> {
        match i {
            // From the top row, so every module's header lines up; the
            // rows under a short one stay empty.
            CanvasItem::Col(c) => match c.len() {
                0 | 1 => vec![-1],
                2 => vec![-1, 0],
                3 => vec![-1, 0, 1],
                // Four rows: the line the second.
                _ => vec![-1, 0, 1, 2],
            },
            CanvasItem::Lanes(c) => c.iter().map(|(l, _)| *l).collect(),
            CanvasItem::Merge(c, (l, _)) => c.iter().map(|(l, _)| *l).chain([*l]).collect(),
        }
    }
    const LANE: f64 = CELL + GAP;

    /// How far an item reaches above and below the chain's line: its lanes,
    /// a module's box round its own.
    fn ext(i: &CanvasItem) -> (f64, f64) {
        match i {
            CanvasItem::Col(_) | CanvasItem::Lanes(_) | CanvasItem::Merge(..) => {
                let l = lanes(i);
                let lo = l.iter().copied().min().unwrap_or(0).min(0);
                let hi = l.iter().copied().max().unwrap_or(0).max(0);
                (f64::from(-lo) * LANE + CELL / 2.0, f64::from(hi) * LANE + CELL / 2.0)
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
                        out.stages.push(Stage::Block(cell(l)));
                        out.cells.push((cell(l), c.clone()));
                    }
                }
                CanvasItem::Merge(cells, (il, into)) => {
                    let rects: Vec<Rect> = cells.iter().map(|(l, _)| cell(*l)).collect();
                    out.cells.extend(rects.iter().copied().zip(cells.iter().map(|(_, c)| c.clone())));
                    out.stages.push(Stage::Split(rects, None));
                    out.stages.push(Stage::Block(cell(*il)));
                    out.cells.push((cell(*il), into.clone()));
                }
                CanvasItem::Lanes(cells) => {
                    let rects: Vec<Rect> = cells.iter().map(|(l, _)| cell(*l)).collect();
                    out.cells.extend(rects.iter().copied().zip(cells.iter().map(|(_, c)| c.clone())));
                    out.stages.push(if rects.len() == 1 { Stage::Block(rects[0]) } else { Stage::Split(rects, None) });
                }
            }
            x += item_w(it) + GAP;
        }
    }

    /// What a spot on the grid is.
    #[derive(Clone)]
    enum Hit {
        Cell(String),
        Module(String, Vec<String>),
        /// The Core's tag: the whole Core.
        Core,
    }

    enum Gesture {
        Idle,
        /// One finger down, not moved yet: a tap, unless it moves.
        Pending { at: (f64, f64), pan0: (f64, f64) },
        Pan { at: (f64, f64), pan0: (f64, f64) },
        Pinch { d0: f64, z0: f64, mid0: (f64, f64), pan0: (f64, f64) },

    }

    struct State {
        modules: Vec<CanvasModule>,
        selected: Option<CanvasSel>,
        fold: bool,
        fit: u32,
        /// The modules it opens on, first and last.
        focus: (String, String),
        /// The zoom and pan; `fitted` while the zoom follows the view.
        zoom: f64,
        pan: (f64, f64),
        fitted: bool,
        /// Back to the start on the next paint (a Fit, a fold).
        reset_pan: bool,
        /// The view's size, pt (from the last paint).
        view: (f64, f64),
        fingers: HashMap<u64, (f64, f64)>,

        gesture: Gesture,
        hits: Vec<(Rect, Hit)>,
        picks: Vec<CanvasPick>,
        dirty: bool,
        fonts: FontContext,
        layouts: LayoutContext<()>,
        texts: HashMap<(String, u32, u32), Layout<()>>,
        /// The frame faces the blocks wear (`faces.json`'s `blocks`).
        block_faces: Vec<BlockFaceDef>,
        /// Each block's open face, by the block's id (`None`: it would not
        /// open), with the face's name and the values last applied.
        surfaces: HashMap<String, Option<BlockSurface>>,
        /// The faces this frame paints again (see `FACES_PER_FRAME`): of
        /// those with something new, the ones that have waited longest.
        repaint: Vec<String>,
    }

    /// A block face in the manifest.
    struct BlockFaceDef {
        matches: Vec<String>,
        face: String,
        ns: String,
        /// How far it reaches past the block, a fraction of its side.
        bleed: f64,
    }

    struct BlockSurface {
        face: String,
        live: frame_live::LiveSurface,
        applied: Vec<(String, f64)>,
        /// Its picture as last painted, in the block's own units (its box
        /// at the origin), and that box's size: appended under the grid's
        /// pan and zoom each frame, painted again only when the face has
        /// something new (`needs_redraw`, paced to `FACE_FPS`). A grid of
        /// thirty faces otherwise rebuilt every one of them every frame.
        recorded: Option<(Scene, (f64, f64))>,
        /// When it was last painted: the longest waiting go first.
        recorded_at: std::time::Instant,
    }

    /// How often a moving face (an LFO, an echo) is painted again: its
    /// motion reads smooth at this rate on a block this size, and the grid
    /// itself still pans and zooms at the display's.
    const FACE_FPS: u32 = 30;
    /// How many faces one frame paints again, at most: the moving ones fall
    /// due together (they share a clock), and painting them all in one
    /// frame was that frame's whole budget. The rest wait a frame or two.
    const FACES_PER_FRAME: u32 = 3;

    /// The manifest's block faces.
    fn load_block_faces() -> Vec<BlockFaceDef> {
        let path = crate::frame_surface::design_dir().join("rig-faces").join("faces.json");
        let Some(doc) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            return Vec::new();
        };
        let strs = |v: &serde_json::Value| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
        doc.get("blocks")
            .and_then(|b| b.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some(BlockFaceDef {
                            matches: strs(e.get("match")?),
                            face: e.get("face")?.as_str()?.to_string(),
                            ns: e.get("ns")?.as_str()?.to_string(),
                            bleed: e.get("bleed").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn open_block_face(face: &str) -> Option<frame_live::LiveSurface> {
        let dir = crate::frame_surface::design_dir().join("rig-faces");
        let (markup, catalog) = frame_live::face_paths(&dir, face);
        frame_live::LiveSurface::open(&markup, catalog)
            .map_err(|e| tracing::warn!(target: "frame", face, error = %e, "block face did not open"))
            .ok()
    }

    type Shared = Rc<RefCell<State>>;

    pub(super) fn canvas(modules: Vec<CanvasModule>, selected: Option<CanvasSel>, fold: bool, fit: u32, focus: (String, String), on_pick: EventHandler<CanvasPick>) -> Element {
        let state: Shared = use_hook(|| {
            Rc::new(RefCell::new(State {
                modules: Vec::new(),
                selected: None,
                fold,
                fit,
                focus,
                zoom: 1.0,
                pan: (0.0, 0.0),
                fitted: true,
                reset_pan: true,
                view: (0.0, 0.0),
                fingers: HashMap::new(),

                gesture: Gesture::Idle,
                hits: Vec::new(),
                picks: Vec::new(),
                dirty: true,
                fonts: FontContext::new(),
                layouts: LayoutContext::new(),
                texts: HashMap::new(),
                block_faces: load_block_faces(),
                surfaces: HashMap::new(),
                repaint: Vec::new(),
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
                        Gesture::Pan { at, pan0 } => {
                            s.reset_pan = false;
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
                                s.reset_pan = false;
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
                        let at = Point::new((p.0 - s.pan.0) / s.zoom, (p.1 - s.pan.1) / s.zoom);
                        let hit = s.hits.iter().rev().find(|(r, _)| r.contains(at)).map(|(_, h)| h.clone());
                        let pick = match hit {
                            Some(Hit::Cell(id)) => CanvasPick::Block(id),
                            Some(Hit::Module(name, ids)) => CanvasPick::Module(name, ids),
                            Some(Hit::Core) => CanvasPick::Module("Core".into(), super::core_ids(&s.modules)),
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
                    s.reset_pan = false;
                    s.pan = (s.pan.0 + dx, s.pan.1 + dy);
                    s.dirty = true;
                }
                _ => {}
            }
        }

        fn needs_redraw(&self) -> bool {
            // A block's face moving (its LFO, its echoes) draws again.
            self.state.try_borrow().is_ok_and(|s| s.dirty || s.surfaces.values().flatten().any(|b| b.live.is_moving()))
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
            let gh = view.1.max(1.0);
            // Unfolded and fitted: the row as tall as the view.
            let natural = layout(&s.modules, None);
            // The span it opens on: from the focus's first module to its
            // last (the Amp through the Reverb), else the whole chain.
            let span = {
                let (pieces, _, _) = &natural;
                let (from, to) = &s.focus;
                let x0 = pieces.iter().find(|p| s.modules[p.module].name == *from).map(|p| p.x);
                let x1 = pieces.iter().rev().find(|p| s.modules[p.module].name == *to).map(|p| p.x + p.w);
                x0.zip(x1).filter(|(a, b)| b > a)
            };
            if s.fitted {
                let tall = gh / natural.2;
                let wide = span.map_or(f64::INFINITY, |(a, b)| view.0 / (b - a + 2.0 * EDGE));
                s.zoom = if s.fold { 1.0 } else { tall.min(wide).clamp(0.35, 1.8) };
            }
            let wrap = s.fold.then(|| view.0 / s.zoom);
            let (pieces, cw, ch) = layout(&s.modules, wrap);
            // On the focus until a finger moves it: the chain comes in
            // stages, and each must land on it too.
            if s.reset_pan {
                s.pan = (span.map_or(0.0, |(a, _)| -(a - EDGE) * s.zoom), 0.0);
            }
            // Panning stops at the content's edges; content shorter than
            // the view sits centred in it.
            let min_x = (view.0 - cw * s.zoom).min(0.0);
            let spare_y = gh - ch * s.zoom;
            let pan_y = if spare_y >= 0.0 { spare_y / 2.0 } else { s.pan.1.clamp(spare_y, 0.0) };
            s.pan = (s.pan.0.clamp(min_x, 0.0), pan_y);
            let t = Affine::scale(scale) * Affine::translate((s.pan.0, s.pan.1)) * Affine::scale(s.zoom);
            let mut hits: Vec<(Rect, Hit)> = Vec::new();
            let st = &mut *s;
            let mut due: Vec<(std::time::Instant, String)> = st
                .surfaces
                .iter()
                .filter_map(|(id, b)| b.as_ref().filter(|b| b.live.needs_redraw()).map(|b| (b.recorded_at, id.clone())))
                .collect();
            due.sort();
            st.repaint = due.into_iter().take(FACES_PER_FRAME as usize).map(|(_, id)| id).collect();

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
                let mut r = Rect::new(p.x, p.y, p.x + p.w, p.y + p.h);
                // A loose block sits under the box: the box ends above it.
                if pl.cells.iter().any(|(_, c)| c.loose) {
                    let inner = pl.cells.iter().filter(|(_, c)| !c.loose).map(|(cr, _)| cr.y1).fold(r.y0 + HEAD, f64::max);
                    r.y1 = inner + PAD;
                }
                if m.bare {
                    continue;
                }
                draw_box(&mut scene, st, t, r, &m, p.first, &mut hits);
                // A module of mixed effects (the Pre-FX, the Time): each
                // block on a tile of its own colour, so its delay reads blue
                // and its reverb violet.
                if matches!(m.name.as_str(), "Pre-FX" | "Time" | "Input") {
                    // Full bands across the box, one a block, meeting
                    // halfway between them: the box is its blocks' colours.
                    let base = Color::from_rgba8(0x14, 0x14, 0x18, 0xff);
                    let n = pl.cells.len();
                    for (i, (cr, c)) in pl.cells.iter().enumerate() {
                        let y0 = if i == 0 { r.y0 + HEAD - 4.0 } else { cr.y0 - GAP / 2.0 };
                        let y1 = if i + 1 == n { r.y1 } else { cr.y1 + GAP / 2.0 };
                        let bottom = if i + 1 == n { 10.0 } else { 0.0 };
                        let band = RoundedRect::from_rect(Rect::new(r.x0, y0, r.x1, y1), kurbo::RoundedRectRadii::new(0.0, 0.0, bottom, bottom));
                        scene.fill(Fill::NonZero, t, mix(hex(&c.colour), base, 0.2), None, &band);
                    }
                }
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
            let line = |scene: &mut Scene, a: Point, b: Point| scene.stroke(&stroke, t, cable, None, &wire(a, b, ROW_GAP));
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
                    // Parallel lanes into the block under them: each out along
                    // the gap at its right, down, along the gap above the
                    // block, and into its top.
                    (Stage::Split(rs, None), Stage::Block(b)) if rs.first().is_some_and(|r| (r.x0 - b.x0).abs() < 0.5) => {
                        let gx = b.x1 + GAP / 2.0;
                        let gy = b.y0 - GAP / 2.0;
                        for r in rs {
                            let pts = [Point::new(r.x1, r.center().y), Point::new(gx, r.center().y), Point::new(gx, gy), Point::new(b.center().x, gy), Point::new(b.center().x, b.y0)];
                            scene.stroke(&stroke, t, cable, None, &rounded_path(&pts, 6.0));
                        }
                    }
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
                    if c.core {
                        core_tag(&mut scene, st, t, Point::new(rect.x1 - 13.0, rect.y1 - 13.0), &mut hits);
                    }
                }
            }
            st.hits = hits;
            scene
        }
    }

    /// A cable from an out port to an in port, square: along the out
    /// port's lane to the middle of the gap before the in port's column,
    /// up or down it, and along into the port — every turn in a gap, so
    /// the routing reads as a diagram. The corners are softly rounded.
    fn wire(a: Point, b: Point, row_gap: f64) -> BezPath {
        let mid = if b.x >= a.x { (b.x - GAP / 2.0).max(a.x + 4.0).min(b.x) } else { a.x + 11.0 };
        let pts: Vec<Point> = if b.x >= a.x {
            vec![a, Point::new(mid, a.y), Point::new(mid, b.y), b]
        } else {
            // Folded: out, down to the gap under the row, back, down, in.
            let back = b.x - 11.0;
            let low = b.y - CELL / 2.0 - HEAD - row_gap / 2.0;
            vec![a, Point::new(mid, a.y), Point::new(mid, low), Point::new(back, low), Point::new(back, b.y), b]
        };
        rounded_path(&pts, 6.0)
    }

    /// A polyline through `pts`, its corners rounded by up to `r`.
    fn rounded_path(pts: &[Point], r: f64) -> BezPath {
        let mut p = BezPath::new();
        let Some(first) = pts.first() else { return p };
        p.move_to(*first);
        for w in pts.windows(3) {
            let (a, c, d) = (w[0], w[1], w[2]);
            let (l1, l2) = ((c - a).hypot(), (d - c).hypot());
            if l1 < 0.01 || l2 < 0.01 {
                p.line_to(c);
                continue;
            }
            let k = r.min(l1 / 2.0).min(l2 / 2.0);
            p.line_to(c - (c - a) * (k / l1));
            p.quad_to(c, c + (d - c) * (k / l2));
        }
        if let Some(last) = pts.last() {
            p.line_to(*last);
        }
        p
    }

    /// A module's box and its header: a mark, its name, what it plays.
    fn draw_box(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, m: &CanvasModule, first: bool, hits: &mut Vec<(Rect, Hit)>) {
        let colour = hex(&m.colour);
        let on = matches!(&st.selected, Some(CanvasSel::Module(n)) if *n == m.name || (m.core && n == "Core"));
        let rr = RoundedRect::from_rect(r, 10.0);
        scene.fill(Fill::NonZero, t, mix(colour, Color::from_rgba8(0x14, 0x14, 0x18, 0xff), if on { 0.16 } else { 0.07 }), None, &rr);
        let edge = if on { colour } else { Color::from_rgba8(0x2a, 0x2a, 0x31, 0xff) };
        scene.stroke(&Stroke::new(if on { 2.0 } else { 1.0 }), t, edge, None, &rr);
        // The header's band: lighter, the module's handle.
        let head = RoundedRect::from_rect(Rect::new(r.x0, r.y0, r.x1, r.y0 + HEAD - 4.0), kurbo::RoundedRectRadii::new(10.0, 10.0, 0.0, 0.0));
        scene.fill(Fill::NonZero, t, mix(colour, Color::from_rgba8(0x14, 0x14, 0x18, 0xff), if on { 0.3 } else { 0.12 }), None, &head);
        glyph(scene, t, &m.name.to_lowercase(), r.x0 + PAD + 2.0, r.y0 + 8.0, 16.0, colour);
        let name = if first { m.name.to_uppercase() } else { format!("{} ›", m.name.to_uppercase()) };
        let room = r.width() - PAD * 2.0 - 14.0;
        let nw = text(scene, st, t, &name, 12.5, 800.0, lift(colour), r.x0 + PAD + 23.0, r.y0 + 21.0, room - 9.0, false);
        if first && !m.label.is_empty() {
            text(scene, st, t, &m.label, 10.5, 600.0, Color::from_rgba8(0xa1, 0xa1, 0xaa, 0xff), r.x0 + PAD + 31.0 + nw, r.y0 + 21.0, (room - 8.0 - nw).max(0.0), false);
        }
        // Anywhere in its box that isn't a block picks the module.
        hits.push((r, Hit::Module(m.name.clone(), m.ids())));
        if m.core {
            core_tag(scene, st, t, Point::new(r.x1 - PAD - 10.0, r.y0 + 16.0), hits);
        }
    }

    fn draw_cell(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, c: &CanvasCell) {
        let colour = hex(&c.colour);
        let on = matches!(&st.selected, Some(CanvasSel::Block(id)) if *id == c.id)
            || (c.core && matches!(&st.selected, Some(CanvasSel::Module(n)) if n == "Core"));
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
        let faced = paint_face(scene, st, t, r, c);
        if !faced {
            scene.fill(Fill::NonZero, t, if c.lit { mix(colour, base, 0.16) } else { base }, None, &rr);
        } else {
            // Its preset and its name each on a band of their own, whatever
            // the picture under them; off, the picture dimmed.
            let band = Color::from_rgba8(0x08, 0x08, 0x0a, 0xc8);
            let head = RoundedRect::from_rect(Rect::new(r.x0, r.y0, r.x1, r.y0 + BAND), kurbo::RoundedRectRadii::new(9.0, 9.0, 0.0, 0.0));
            let foot = RoundedRect::from_rect(Rect::new(r.x0, r.y1 - BAND, r.x1, r.y1), kurbo::RoundedRectRadii::new(0.0, 0.0, 9.0, 9.0));
            scene.fill(Fill::NonZero, t, band, None, &head);
            scene.fill(Fill::NonZero, t, band, None, &foot);
            if !c.lit {
                // Off: muted, the picture sunk into the desk.
                scene.fill(Fill::NonZero, t, Color::from_rgba8(0x18, 0x18, 0x1c, 0xb8), None, &rr);
            }
        }
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
        // Its preset along the top, what it is and its name along the
        // foot; an edit marked at the top right.
        let ink = if c.lit { Color::from_rgba8(0xe4, 0xe4, 0xe7, 0xff) } else { Color::from_rgba8(0x71, 0x71, 0x7a, 0xff) };
        if !c.sub.is_empty() {
            let sub = if faced { Color::from_rgba8(0xd4, 0xd4, 0xd8, 0xff) } else { Color::from_rgba8(0x8a, 0x8a, 0x93, 0xff) };
            text(scene, st, t, &c.sub, 9.5, 650.0, sub, r.x0 + 8.0, r.y0 + 14.5, CELL - (if c.edited { 26.0 } else { 16.0 }), false);
        }
        if c.edited {
            scene.fill(Fill::NonZero, t, Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff), None, &Circle::new((r.x1 - 11.0, r.y0 + 12.0), 3.5));
        }
        // A level block: its level large in the middle, its name at the foot.
        if let (Some(v), false) = (&c.value, faced) {
            text(scene, st, t, v, 17.0, 800.0, ink, r.x0 + 6.0, r.center().y + 4.0, CELL - 12.0, true);
        }
        let (size, icon) = (12.0, 13.0);
        let w = measure(st, &c.name, size, 750.0).min(CELL - 16.0 - icon - 4.0);
        let x = r.center().x - (icon + 4.0 + w) / 2.0;
        let base = if faced || c.value.is_some() { r.y1 - 7.0 } else { r.center().y + 5.0 };
        glyph(scene, t, &c.kind, x, base - 10.5, icon, if c.lit { colour } else { Color::from_rgba8(0x52, 0x52, 0x5b, 0xff) });
        text(scene, st, t, &c.name, size, 750.0, ink, x + icon + 4.0, base, w + 6.0, false);
    }

    /// A block's frame face over its box (grown by the face's margin), when
    /// the manifest has one for it; whether it drew.
    fn paint_face(scene: &mut Scene, st: &mut State, t: Affine, r: Rect, c: &CanvasCell) -> bool {
        let def = c.keys.iter().find_map(|k| st.block_faces.iter().find(|d| d.matches.iter().any(|m| m == k)));
        // A block face fills the block; a unit's own face (a pedal, an amp)
        // sits whole between the bands.
        let unit = def.is_none() && !c.fallback.as_ref().is_some_and(|f| f.2);
        let Some((face, ns, bleed)) = def.map(|d| (d.face.clone(), d.ns.clone(), d.bleed)).or_else(|| c.fallback.clone().map(|(f, n, _)| (f, n, 0.0))) else {
            return false;
        };
        let slot = st.surfaces.entry(c.id.clone()).or_insert(None);
        if slot.as_ref().is_none_or(|s| s.face != face) {
            *slot = open_block_face(&face).map(|mut live| {
                live.set_max_fps(FACE_FPS);
                BlockSurface { face: face.clone(), live, applied: Vec::new(), recorded: None, recorded_at: std::time::Instant::now() }
            });
        }
        let Some(s) = slot.as_mut() else { return false };
        let values: Vec<(String, f64)> = c.params.iter().map(|(n, v)| (format!("{ns}/{n}"), *v)).collect();
        if s.applied != values {
            s.live.apply_real(values.clone());
            s.applied = values;
        }
        let m = bleed * r.width();
        if unit {
            // The unit on the block's own ground.
            scene.fill(Fill::NonZero, t, Color::from_rgba8(0x17, 0x17, 0x1b, 0xff), None, &RoundedRect::from_rect(r, 9.0));
        }
        let at = if unit { Rect::new(r.x0 + 4.0, r.y0 + BAND + 2.0, r.x1 - 4.0, r.y1 - BAND - 2.0) } else { r.inflate(m, m) };
        let size = (at.width(), at.height());
        // A face with no picture yet (or a new size) is painted at once; one
        // that has only moved waits its turn.
        let fresh = s.recorded.as_ref().is_none_or(|(_, was)| *was != size);
        if fresh || st.repaint.contains(&c.id) {
            s.recorded_at = std::time::Instant::now();
            let mut rec = Scene::new();
            s.live.paint_vectors_at(&mut rec, size.0, size.1, Affine::IDENTITY.as_coeffs());
            s.recorded = Some((rec, size));
        }
        if let Some((rec, _)) = &s.recorded {
            scene.append_scene(rec.clone(), t * Affine::translate((at.x0, at.y0)));
        }
        true
    }

    /// The Core's tag: its mark in a ring, centred at `at`. A tap on it
    /// picks the whole Core.
    fn core_tag(scene: &mut Scene, st: &State, t: Affine, at: Point, hits: &mut Vec<(Rect, Hit)>) {
        let on = matches!(&st.selected, Some(CanvasSel::Module(n)) if n == "Core");
        let ink = if on { Color::from_rgba8(0xfa, 0xfa, 0xfa, 0xff) } else { Color::from_rgba8(0xa1, 0xa1, 0xaa, 0xff) };
        glyph(scene, t, "core", at.x - 6.0, at.y - 6.0, 12.0, ink);
        hits.push((Rect::new(at.x - 14.0, at.y - 14.0, at.x + 14.0, at.y + 14.0), Hit::Core));
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
    /// `s`'s width at `size` and `weight`.
    fn measure(st: &mut State, s: &str, size: f32, weight: f32) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let key = laid(st, s, size, weight);
        f64::from(st.texts[&key].width())
    }

    /// `s` laid out (cached), by its key.
    fn laid(st: &mut State, s: &str, size: f32, weight: f32) -> (String, u32, u32) {
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
        key
    }

    fn text(scene: &mut Scene, st: &mut State, t: Affine, s: &str, size: f32, weight: f32, colour: Color, x: f64, y: f64, max_w: f64, centre: bool) -> f64 {
        if s.is_empty() || max_w <= 0.0 {
            return 0.0;
        }
        let key = laid(st, s, size, weight);
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
            "input" => &["M12 3v6", "M8 9h8v4a4 4 0 0 1-8 0z", "M12 17v4"],
            "master" => &["M6 4v16", "M12 4v16", "M18 4v16", "M4 9h4", "M10 15h4", "M16 7h4"],
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

