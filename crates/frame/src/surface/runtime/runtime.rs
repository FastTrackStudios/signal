//! One running surface: design + controls + live values.
//!
//! The runtime is transport-agnostic and synchronous. The host (desktop
//! window, plugin editor, iPad app, browser) feeds it input, engine updates
//! and time; it returns [`EngineCall`]s to send and a scene to draw. The
//! async glue to a [`SurfaceEngine`](crate::surface::proto::SurfaceEngine) is
//! [`crate::surface::runtime::sync`].

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::document::proto::{NodeId, NodeKind, Scene, Transform};
use crate::surface::proto::{
    Axis, Binding, Catalog, Control, ControlKind, EditPhase, EntryId, FeedRequest, HandleAxis,
    InputSource, MeterTap, ParamAddr, ParamKind, ParamValue, PlotRange, PlotStyle, Source,
    StreamFrame, StreamKind,
};

use crate::surface::runtime::coalesce::Coalescer;
use crate::surface::runtime::curves::{CurveInputs, CurveRegistry, Series};
use crate::surface::runtime::gesture::{Bounds, Gesture, Input, Key, Output};
use crate::surface::runtime::meter::{Ballistics, MeterScale, Sense};
use crate::surface::runtime::plot::{self, Trace};
use crate::surface::runtime::visual::{ControlState, control_ops, handle_offset};

/// Evaluated curves, shared by every control asking for the same provider
/// with the same inputs.
#[derive(Debug, Default)]
struct CurveCache {
    results: HashMap<u64, Arc<Vec<Series>>>,
    evaluations: u64,
}

/// Cache entries kept before the cache starts over (a drag produces one
/// per distinct value; old ones are never asked for again).
const CURVE_CACHE_MAX: usize = 512;

// Live reload: swap the design, keep the running state.
mod swap;
// Motion: `Node::motion` animations, run on the render copy.
mod motion;
// Dynamic collections: repeat templates and context subtrees.
pub mod repeat;
// Selection, popovers, menus, removal, adding, keyboard routing.
pub mod ui;
// Text params and inline editing.
pub mod text;
// Drawing the interaction layer: visibility, floating panels, menus.
mod draw;
// Shader nodes' live inputs.
mod shader_inputs;

/// A design plus the controls attached to its nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    pub scene: Scene,
    pub controls: BTreeMap<NodeId, Control>,
}

impl Surface {
    /// Read the controls stored on a scene's nodes.
    #[must_use]
    pub fn from_scene(scene: Scene) -> Self {
        let controls = scene
            .nodes
            .values()
            .filter_map(|n| n.control.clone().map(|c| (n.id, c)))
            .collect();
        Self { scene, controls }
    }

    /// The scene with every control stored on its node (what gets saved,
    /// synced, printed as text, and edited by tools).
    #[must_use]
    pub fn to_scene(&self) -> Scene {
        let mut scene = self.scene.clone();
        for node in scene.nodes.values_mut() {
            node.control = self.controls.get(&node.id).cloned();
        }
        scene
    }
}

/// Something to send to the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineCall {
    SetParam {
        addr: ParamAddr,
        value: f64,
        phase: EditPhase,
    },
    Invoke {
        action: EntryId,
    },
    Select {
        collection: EntryId,
        index: u32,
    },
    /// Set a text param (a band's name).
    SetText {
        addr: ParamAddr,
        text: String,
    },
}

pub struct Runtime {
    surface: Surface,
    catalog: Catalog,
    values: BTreeMap<ParamAddr, f64>,
    streams: HashMap<EntryId, Vec<f64>>,
    /// Ballistics per (control, stream, channel): a switched meter keeps
    /// every tap moving, so switching shows a settled reading.
    meters: HashMap<(NodeId, EntryId, u8), Ballistics>,
    gestures: HashMap<NodeId, Gesture>,
    /// XY pads' second axis, and footswitch/button positions not tied to params.
    local_positions: HashMap<NodeId, f64>,
    captured: Option<NodeId>,
    boxes: HashMap<NodeId, Bounds>,
    /// Screen → box space, for controls drawn through a transformed
    /// container (absent = identity).
    to_box: HashMap<NodeId, Transform>,
    coalescer: Coalescer,
    meter_scale: MeterScale,
    /// Time plots' rings (x = `Axis::Time`), newest last.
    histories: HashMap<NodeId, VecDeque<f64>>,
    curves: CurveRegistry,
    curve_cache: Mutex<CurveCache>,
    /// The surface's motions (stateful; stepped by `frame`).
    motion: std::sync::Mutex<crate::motion::runtime::Animator>,
    /// The input control under the pointer (for `hover` motion sources).
    hovered: Option<NodeId>,
    /// Text params' values (names).
    texts: BTreeMap<ParamAddr, String>,
    /// Every node's layout box (popover stages, anchors, menu rows).
    all_boxes: HashMap<NodeId, Bounds>,
    /// What repeat templates and context subtrees expanded to.
    expansion: repeat::Expansion,
    ui: ui::Ui,
    text_measure: Option<text::TextMeasure>,
    /// The host's clock, seconds ([`Self::set_clock`]): animated curves and
    /// shader inputs read it.
    clock_s: f64,
}

impl Runtime {
    #[must_use]
    pub fn new(surface: Surface, catalog: Catalog) -> Self {
        let (surface, expansion) = repeat::expand(surface);
        let values = catalog
            .params
            .iter()
            .map(|p| (p.addr.clone(), p.default))
            .collect();
        let gestures = surface
            .controls
            .iter()
            .map(|(id, c)| (*id, Gesture::new(c.kind.clone())))
            .collect();
        let motion = motion::animator(&surface.scene);
        Self {
            surface,
            catalog,
            values,
            streams: HashMap::new(),
            meters: HashMap::new(),
            gestures,
            local_positions: HashMap::new(),
            captured: None,
            boxes: HashMap::new(),
            to_box: HashMap::new(),
            coalescer: Coalescer::default(),
            meter_scale: MeterScale::default(),
            histories: HashMap::new(),
            curves: CurveRegistry::builtin(),
            curve_cache: Mutex::new(CurveCache::default()),
            motion,
            hovered: None,
            texts: BTreeMap::new(),
            all_boxes: HashMap::new(),
            expansion,
            ui: ui::Ui::default(),
            text_measure: None,
            clock_s: 0.0,
        }
    }

    /// Use `curves` for `Curve` controls (the builtins plus an engine's own).
    #[must_use]
    pub fn with_curves(mut self, curves: CurveRegistry) -> Self {
        self.curves = curves;
        self.cache().results.clear();
        self
    }

    /// The curve providers, to register an engine's own (overriding a
    /// builtin of the same name).
    pub fn curves_mut(&mut self) -> &mut CurveRegistry {
        self.cache().results.clear();
        &mut self.curves
    }

    /// How many times a curve provider has run (memoised curves only run
    /// when an input changes).
    #[must_use]
    pub fn curve_evaluations(&self) -> u64 {
        self.cache().evaluations
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, CurveCache> {
        self.curve_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[must_use]
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The boxes of control nodes from the host's layout pass, for hit
    /// testing and position-based gestures.
    ///
    /// Boxes are axis-aligned in the renderer's *flattened* space (each node
    /// at its parent's box origin plus its own offset, ancestors' rotation
    /// and scale not applied — `crate::render::build_scene_layout_boxes`). A control
    /// inside a rotated/scaled frame is drawn through its ancestors'
    /// transforms, so the runtime derives each control's box-to-screen
    /// transform from the scene and hit-tests through its inverse. Pass every
    /// node's box (not only the controls') so transformed ancestors are
    /// pivoted where the layout put them; missing ones fall back to the sum
    /// of their authored offsets.
    pub fn set_boxes(&mut self, boxes: impl IntoIterator<Item = (NodeId, Bounds)>) {
        let all: HashMap<NodeId, Bounds> = boxes.into_iter().collect();
        self.all_boxes.clone_from(&all);
        let transformed: Vec<(NodeId, Bounds, Option<Transform>)> = all
            .iter()
            .filter(|(id, _)| self.surface.controls.contains_key(id))
            .map(|(id, b)| (*id, *b, box_to_screen(&self.surface.scene, *id, &all)))
            .collect();
        self.set_transformed_boxes(transformed);
    }

    /// [`Self::set_boxes`] with each control's box-to-screen transform given
    /// by the host (`None` = the box is on screen as is): for hosts that
    /// already track world transforms.
    pub fn set_transformed_boxes(
        &mut self,
        boxes: impl IntoIterator<Item = (NodeId, Bounds, Option<Transform>)>,
    ) {
        self.boxes.clear();
        self.to_box.clear();
        for (id, b, t) in boxes {
            if !self.surface.controls.contains_key(&id) {
                continue;
            }
            self.boxes.insert(id, b);
            if let Some(inv) = t.and_then(|t| invert(&t)) {
                self.to_box.insert(id, inv);
            }
        }
    }

    /// A screen point in `id`'s box space.
    fn local(&self, id: NodeId, x: f64, y: f64) -> (f64, f64) {
        self.to_box.get(&id).map_or((x, y), |t| apply(t, x, y))
    }

    // ── engine → surface ──────────────────────────────────────────────────

    /// Every parameter value the runtime holds (engine state plus
    /// optimistic local edits) — to carry over into a rebuilt runtime.
    #[must_use]
    pub fn values(&self) -> Vec<ParamValue> {
        self.values
            .iter()
            .map(|(addr, value)| ParamValue {
                addr: addr.clone(),
                value: *value,
            })
            .collect()
    }

    pub fn apply_values(&mut self, values: impl IntoIterator<Item = ParamValue>) {
        for v in values {
            self.values.insert(v.addr, v.value);
        }
    }

    pub fn apply_frame(&mut self, frame: StreamFrame, t: f64) {
        let sense = match self.catalog.stream(&frame.id).map(|s| &s.kind) {
            Some(StreamKind::Level { .. }) => Some(Sense::Level),
            Some(StreamKind::GainReduction) => Some(Sense::Reduction),
            _ => None,
        };
        if let Some(sense) = sense {
            for (id, control) in &self.surface.controls {
                // Every stream the control can show: its binding's, and
                // each tap its meter switch can select.
                let bound = match &control.binding {
                    Some(Binding {
                        source: Source::Stream { id: sid, channel },
                        ..
                    }) => Some((sid, *channel)),
                    _ => None,
                };
                let taps = control
                    .select
                    .iter()
                    .flat_map(|s| s.taps.iter().flatten())
                    .map(|tap| (&tap.stream, tap.channel));
                for (sid, channel) in bound.into_iter().chain(taps) {
                    if *sid != frame.id {
                        continue;
                    }
                    let ch = channel.unwrap_or(0);
                    if let Some(db) = frame.values.get(usize::from(ch)) {
                        self.meters
                            .entry((*id, sid.clone(), ch))
                            .or_insert_with(|| Ballistics::for_sense(sense))
                            .push(*db, t);
                    }
                }
            }
        }
        // History plots keep their own ring: a reading (a level, a gain
        // reduction) is pushed; a series (a `History` feed) is the history
        // the engine kept, taken as is.
        let pushes = !self
            .catalog
            .stream(&frame.id)
            .is_some_and(|s| s.kind.is_series());
        for (id, control) in &self.surface.controls {
            let ControlKind::Plot {
                x: Axis::Time { len },
                ..
            } = control.kind
            else {
                continue;
            };
            let Some(Binding {
                source: Source::Stream { id: sid, channel },
                ..
            }) = &control.binding
            else {
                continue;
            };
            if *sid != frame.id {
                continue;
            }
            let len = len.max(2) as usize;
            let ring = self
                .histories
                .entry(*id)
                .or_insert_with(|| VecDeque::with_capacity(len));
            if pushes || frame.values.len() == 1 {
                let v = frame
                    .values
                    .get(usize::from(channel.unwrap_or(0)))
                    .copied()
                    .unwrap_or(f64::NAN);
                ring.push_back(v);
            } else {
                ring.clear();
                ring.extend(frame.values.iter().rev().take(len).rev());
            }
            while ring.len() > len {
                ring.pop_front();
            }
        }
        self.streams.insert(frame.id.clone(), frame.values);
    }

    // ── plots, curves, handles ────────────────────────────────────────────

    /// The feeds this surface's plots want, shaped to them: a spectrum at
    /// half a bin per pixel of its plot, a history `len` long, a scope a
    /// sample per pixel. Hosts pass these to the engine's `request_feeds`
    /// ([`crate::surface::runtime::sync::connect`] does).
    #[must_use]
    pub fn feed_requests(&self) -> Vec<FeedRequest> {
        let mut out: Vec<FeedRequest> = Vec::new();
        for (id, control) in &self.surface.controls {
            let ControlKind::Plot { x, .. } = &control.kind else {
                continue;
            };
            let Some(Binding {
                source: Source::Stream { id: sid, .. },
                ..
            }) = &control.binding
            else {
                continue;
            };
            let Some(info) = self.catalog.stream(sid) else {
                continue;
            };
            let width = self
                .surface
                .scene
                .node(*id)
                .map_or(0.0, |n| n.size.width)
                .max(1.0);
            let r = FeedRequest::new(sid.0.clone());
            let r = match (&info.kind, x) {
                (StreamKind::Spectrum { .. }, _) => {
                    r.bins((width / 2.0).round().clamp(16.0, 4096.0) as u16)
                }
                (StreamKind::History { .. }, Axis::Time { len }) => r.len(*len),
                (StreamKind::Waveform { .. }, _) => r.len(width.round() as u32),
                _ => continue,
            };
            // Two plots of one feed: the larger ask wins.
            match out.iter_mut().find(|o| o.id == r.id) {
                Some(o) => {
                    o.options.bins = o.options.bins.max(r.options.bins);
                    o.options.len = o.options.len.max(r.options.len);
                }
                None => out.push(r),
            }
        }
        out
    }

    /// A param's value in engineering units: a choice's option index, a
    /// toggle's 0/1, else the range's real value.
    #[must_use]
    pub fn real(&self, addr: &ParamAddr) -> Option<f64> {
        let info = self.catalog.param(addr)?;
        let v = self.values.get(addr).copied().unwrap_or(info.default);
        Some(match &info.kind {
            ParamKind::Choice { options } => {
                (v.clamp(0.0, 1.0) * options.len().saturating_sub(1) as f64).round()
            }
            ParamKind::Toggle => f64::from(u8::from(v >= 0.5)),
            ParamKind::Continuous | ParamKind::Text => info.range.to_real(v),
        })
    }

    /// The normalized value that puts `addr` at `real` engineering units.
    fn normalized(&self, addr: &ParamAddr, real: f64) -> Option<f64> {
        let info = self.catalog.param(addr)?;
        Some(match &info.kind {
            ParamKind::Choice { options } if options.len() > 1 => {
                (real.round() / (options.len() - 1) as f64).clamp(0.0, 1.0)
            }
            ParamKind::Toggle => f64::from(u8::from(real >= 0.5)),
            _ => info.range.to_normalized(real),
        })
    }

    fn stream_value(&self, id: &EntryId, channel: Option<u8>) -> Option<f64> {
        self.streams
            .get(id)
            .and_then(|v| v.get(usize::from(channel.unwrap_or(0))))
            .copied()
    }

    /// A plot's points: the bound stream's latest frame (or its history).
    fn plot_trace(
        &self,
        id: NodeId,
        control: &Control,
        style: PlotStyle,
        x: &Axis,
        y: &PlotRange,
    ) -> Option<Trace> {
        let Some(Binding {
            source: Source::Stream { id: sid, channel },
            ..
        }) = &control.binding
        else {
            return None;
        };
        let points: Vec<(f64, f64)> = match *x {
            Axis::Time { len } => {
                let ring = self.histories.get(&id)?;
                let (len, n) = (f64::from(len.max(2)), ring.len());
                ring.iter()
                    .enumerate()
                    .map(|(i, v)| ((len - n as f64 + i as f64) / (len - 1.0), y.to_unit(*v)))
                    .collect()
            }
            _ => {
                let values = self.streams.get(sid)?;
                let band = match self.catalog.stream(sid).map(|s| &s.kind) {
                    Some(StreamKind::Spectrum { f_min, f_max, .. })
                        if *f_min > 0.0 && f_max > f_min =>
                    {
                        Some((*f_min, *f_max))
                    }
                    _ => None,
                };
                let spectrum = band.is_some();
                // A level stream plotted across channels reads them all; one
                // channel picked reads just that one (a single point).
                let values: &[f64] = match channel {
                    Some(c) if !spectrum => {
                        values.get(usize::from(*c)..=usize::from(*c)).unwrap_or(&[])
                    }
                    _ => values,
                };
                let n = values.len();
                let last = (n.max(2) - 1) as f64;
                values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let u = match (x, band) {
                            // Log-spaced bins (centred in their slice of the
                            // feed's band), placed on this axis.
                            (Axis::Log { .. }, Some((lo, hi))) => {
                                x.to_unit(lo * (hi / lo).powf((i as f64 + 0.5) / n as f64))
                            }
                            _ => i as f64 / last,
                        };
                        (u, y.to_unit(*v))
                    })
                    .collect()
            }
        };
        Some(Trace {
            points: Arc::new(points),
            style,
            baseline: baseline(style, y),
            marker: None,
        })
    }

    /// A curve's evaluated series (all of them), memoised on its inputs.
    fn curve_series(
        &self,
        provider: &str,
        inputs: &[crate::surface::proto::CurveInput],
        x: &Axis,
    ) -> Option<Arc<Vec<Series>>> {
        let mut args = CurveInputs::new(x.clone());
        for input in inputs.iter().filter(|i| i.name != "marker") {
            let v = match &input.source {
                InputSource::Param { addr } => self.real(addr),
                InputSource::Stream { id, channel } => self.stream_value(id, *channel),
                InputSource::Const { value } => Some(*value),
            };
            if let Some(v) = v {
                args.values.insert(input.name.clone(), v);
            }
        }
        if self.curves.is_animated(provider) {
            args.values.insert("time".into(), self.clock_s);
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        provider.hash(&mut h);
        for (k, v) in &args.values {
            k.hash(&mut h);
            v.to_bits().hash(&mut h);
        }
        format!("{x:?}").hash(&mut h);
        args.samples.hash(&mut h);
        let key = h.finish();
        if let Some(hit) = self.cache().results.get(&key) {
            return Some(hit.clone());
        }
        let series = Arc::new(self.curves.eval(provider, &args)?);
        let mut cache = self.cache();
        cache.evaluations += 1;
        if cache.results.len() >= CURVE_CACHE_MAX {
            cache.results.clear();
        }
        cache.results.insert(key, series.clone());
        Some(series)
    }

    /// A `Curve` control's selected series in engineering units (`None`
    /// for other controls or an unknown provider).
    #[must_use]
    pub fn curve(&self, id: NodeId) -> Option<Series> {
        let ControlKind::Curve {
            provider,
            inputs,
            series,
            x,
            ..
        } = &self.surface.controls.get(&id)?.kind
        else {
            return None;
        };
        let all = self.curve_series(provider, inputs, x)?;
        pick(&all, series.as_deref()).cloned()
    }

    fn curve_trace(&self, control: &Control) -> Option<Trace> {
        let ControlKind::Curve {
            provider,
            inputs,
            series,
            style,
            x,
            y,
        } = &control.kind
        else {
            return None;
        };
        let all = self.curve_series(provider, inputs, x)?;
        let s = pick(&all, series.as_deref())?;
        let y = &self.zoomed_range(control, y);
        let points: Vec<(f64, f64)> = s
            .points
            .iter()
            .map(|(px, py)| (x.to_unit(*px), y.to_unit(*py)))
            .collect();
        let marker = inputs.iter().find(|i| i.name == "marker").and_then(|m| {
            let at = match &m.source {
                InputSource::Param { addr } => self.real(addr),
                InputSource::Stream { id, channel } => self.stream_value(id, *channel),
                InputSource::Const { value } => Some(*value),
            }?;
            let u = x.to_unit(at).clamp(0.0, 1.0);
            let v = plot::sample(&points, u)?;
            Some((u, v))
        });
        Some(Trace {
            points: Arc::new(points),
            style: *style,
            baseline: baseline(*style, y),
            marker,
        })
    }

    /// One handle axis's place (unit), or `None` when it doesn't move.
    fn handle_axis(&self, b: Option<&Binding>, map: &HandleAxis) -> Option<f64> {
        if !map.follows(b.is_some()) {
            return None;
        }
        let bound = match b.map(|b| &b.source) {
            Some(Source::Param { addr }) => self.real(addr).unwrap_or(0.0),
            _ => 0.0,
        };
        let after: f64 = map.after.iter().filter_map(|a| self.real(a)).sum();
        Some(map.axis.to_unit(bound + after + map.shift).clamp(0.0, 1.0))
    }

    /// A handle's unit point in its parent (`None` per unmoving axis).
    fn handle_uv(&self, control: &Control) -> Option<(Option<f64>, Option<f64>)> {
        let ControlKind::Handle {
            x, y, x_map, y_map, ..
        } = &control.kind
        else {
            return None;
        };
        let y_map = &HandleAxis {
            axis: self.zoomed_axis(control, &y_map.axis),
            ..y_map.clone()
        };
        Some((
            self.handle_axis(x.as_ref(), x_map),
            self.handle_axis(y.as_ref(), y_map),
        ))
    }

    /// The span a control's zoom picks now.
    fn zoom_span(&self, control: &Control) -> Option<f64> {
        let z = control.zoom.as_ref()?;
        z.span(self.real(&z.param)?)
    }

    /// A vertical axis under the control's zoom (a linear axis spans
    /// ±the zoom's span).
    pub(crate) fn zoomed_axis(&self, control: &Control, axis: &Axis) -> Axis {
        match (self.zoom_span(control), axis) {
            (Some(s), Axis::Linear { .. }) => Axis::Linear { min: -s, max: s },
            _ => axis.clone(),
        }
    }

    /// A plot range under the control's zoom.
    fn zoomed_range(&self, control: &Control, y: &PlotRange) -> PlotRange {
        match self.zoom_span(control) {
            Some(s) => PlotRange {
                min: -s,
                max: s,
                ..y.clone()
            },
            None => y.clone(),
        }
    }

    /// A handle's parent area in box space: its layout box less its
    /// authored offset, the parent's size.
    fn handle_area(&self, id: NodeId) -> Bounds {
        let scene = &self.surface.scene;
        let Some(node) = scene.node(id) else {
            return Bounds::default();
        };
        let b = self.boxes.get(&id).copied().unwrap_or_default();
        let size = node
            .parent
            .and_then(|p| scene.node(p))
            .map_or(node.size, |p| p.size);
        Bounds {
            x: b.x - node.transform.tx,
            y: b.y - node.transform.ty,
            width: size.width,
            height: size.height,
        }
    }

    /// A control's hit box now (box space; see [`Self::set_boxes`]): its
    /// layout box — for a handle, wherever its values have moved it.
    #[must_use]
    pub fn hit_box(&self, id: NodeId) -> Option<Bounds> {
        self.hit_box_of(id, self.surface.controls.get(&id)?)
    }

    fn hit_box_of(&self, id: NodeId, control: &Control) -> Option<Bounds> {
        let b = *self.boxes.get(&id)?;
        let Some(uv) = self.handle_uv(control) else {
            return Some(b);
        };
        let scene = &self.surface.scene;
        let node = scene.node(id)?;
        let parent = node
            .parent
            .and_then(|p| scene.node(p))
            .map_or(node.size, |p| p.size);
        let (tx, ty) = handle_offset(node, parent, uv);
        Some(Bounds {
            x: b.x + tx - node.transform.tx,
            y: b.y + ty - node.transform.ty,
            ..b
        })
    }

    /// A handle's edit: unit point → its params, through its axes.
    fn handle_output(&mut self, control: &Control, o: Output, t: f64, calls: &mut Vec<EngineCall>) {
        let ControlKind::Handle {
            x,
            y,
            wheel,
            x_map,
            y_map,
        } = &control.kind
        else {
            return;
        };
        match o {
            Output::SetXy { x: u, y: v, phase } => {
                let y_map = &HandleAxis {
                    axis: self.zoomed_axis(control, &y_map.axis),
                    ..y_map.clone()
                };
                for (b, map, unit) in [(x, x_map, u), (y, y_map, v)] {
                    let Some(
                        b @ Binding {
                            source: Source::Param { addr },
                            ..
                        },
                    ) = b
                    else {
                        continue;
                    };
                    let after: f64 = map.after.iter().filter_map(|a| self.real(a)).sum();
                    let real = map.axis.from_unit(unit) - after - map.shift;
                    if let Some(n) = self.normalized(addr, real) {
                        let position = b.to_position(n);
                        self.send(b, position, phase, t, calls);
                    }
                }
            }
            Output::Nudge { delta } => {
                if let Some(
                    b @ Binding {
                        source: Source::Param { addr },
                        ..
                    },
                ) = wheel
                {
                    let now = self.values.get(addr).map_or(0.0, |v| b.to_position(*v));
                    self.send(b, (now + delta).clamp(0.0, 1.0), EditPhase::End, t, calls);
                }
            }
            Output::Reset => {
                for b in [x, y, wheel].into_iter().flatten() {
                    if let Source::Param { addr } = &b.source {
                        let position = self
                            .catalog
                            .param(addr)
                            .map_or(0.0, |p| b.to_position(p.default));
                        self.send(b, position, EditPhase::End, t, calls);
                    }
                }
            }
            _ => {}
        }
    }

    /// The stream a meter reads right now: its switch's selected tap (or
    /// `None` when the switch is off), else its binding's stream.
    fn tap(&self, control: &Control) -> Option<MeterTap> {
        if let Some(sel) = &control.select {
            let info = self.catalog.param(&sel.param);
            let v = self
                .values
                .get(&sel.param)
                .copied()
                .unwrap_or_else(|| info.map_or(0.0, |p| p.default));
            let n = match info.map(|p| &p.kind) {
                Some(ParamKind::Choice { options }) => options.len(),
                Some(ParamKind::Toggle) => 2,
                _ => sel.taps.len(),
            };
            let i = if n < 2 {
                0
            } else {
                (v.clamp(0.0, 1.0) * (n - 1) as f64).round() as usize
            };
            return sel.tap(i).cloned();
        }
        match &control.binding {
            Some(Binding {
                source: Source::Stream { id, channel },
                ..
            }) => Some(MeterTap {
                stream: id.clone(),
                channel: *channel,
                offset_db: 0.0,
            }),
            _ => None,
        }
    }

    /// A meter's state over `scale` for its current tap: the ballistic
    /// level, or its rest (silence; 0 dB of gain reduction) before data.
    fn meter_state(
        &self,
        id: NodeId,
        control: &Control,
        scale: &MeterScale,
        t: f64,
    ) -> ControlState {
        let Some(tap) = self.tap(control) else {
            // Switched off: the movement rests at its left stop.
            return ControlState::default();
        };
        let sense = match self.catalog.stream(&tap.stream).map(|s| &s.kind) {
            Some(StreamKind::GainReduction) => Sense::Reduction,
            _ => Sense::Level,
        };
        let m = self
            .meters
            .get(&(id, tap.stream.clone(), tap.channel.unwrap_or(0)));
        // A level in dBFS reads against the meter's reference (0 VU =
        // −18 dBFS); gain reduction is already relative to unity.
        let offset = tap.offset_db
            - if sense == Sense::Level {
                control.reference_dbfs.unwrap_or(0.0)
            } else {
                0.0
            };
        let level = m.map_or(sense.rest_db(), |m| m.level_db(t)) + offset;
        let peak = m.map(|m| m.peak_hold_db(t) + offset);
        let fmt = control.format.clone().unwrap_or_default();
        // A bar reads its held peak; a needle its level. Before data the
        // text stays as authored (a placeholder).
        let shown = if matches!(control.kind, ControlKind::Meter { .. }) {
            peak
        } else {
            m.map(|_| level)
        };
        ControlState {
            position: scale.position(level),
            peak: peak.map(|p| scale.position(p)),
            value_text: shown.map(|db| fmt.db(db)),
            ..ControlState::default()
        }
    }

    // ── input → engine ────────────────────────────────────────────────────

    /// The topmost control under a screen point (last in paint order wins),
    /// through any rotation/scale of the control's containers.
    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<NodeId> {
        // Open menus and shown popovers float above everything.
        if let Some(id) = self.hit_floating(x, y) {
            return Some(id);
        }
        let order = self.surface.scene.walk(self.surface.scene.root);
        let under = |id: &NodeId, handles: bool| {
            let (x, y) = self.local(*id, x, y);
            self.surface.controls.get(id).is_some_and(|c| {
                c.kind.is_input()
                    && matches!(c.kind, ControlKind::Handle { .. }) == handles
                    && self.hit_box_of(*id, c).is_some_and(|b| {
                        x >= b.x && y >= b.y && x <= b.x + b.width && y <= b.y + b.height
                    })
                    && self.floating_root(*id).is_none()
                    && self.hittable(*id, c)
            })
        };
        // Handles are drawn over a docked panel, and hit first.
        if let Some(id) = order.iter().rev().find(|id| under(id, true)) {
            return Some(*id);
        }
        if let Some(id) = self.hit_floating_where(x, y, true) {
            return Some(id);
        }
        order.into_iter().rev().find(|id| {
            let (x, y) = self.local(*id, x, y);
            self.surface.controls.get(id).is_some_and(|c| {
                c.kind.is_input()
                    && self.hit_box_of(*id, c).is_some_and(|b| {
                        x >= b.x && y >= b.y && x <= b.x + b.width && y <= b.y + b.height
                    })
                    && self.floating_root(*id).is_none()
                    && self.hittable(*id, c)
            })
        })
    }

    /// Feed pointer input at time `t` (ms). A control captures the pointer
    /// from `Down` until `Up`/`Cancel`.
    pub fn pointer(&mut self, input: Input, t: f64) -> Vec<EngineCall> {
        // Hover drives `hover` motion sources.
        if let Input::Move { x, y, .. } | Input::Down { x, y, .. } = input {
            self.hovered = self.hit(x, y);
        }
        // Docked panels hold still while in use.
        match input {
            Input::Move { x, y, t: at, .. } | Input::Down { x, y, t: at, .. } => {
                self.track_docked(Some((x, y)), at)
            }
            Input::Up { x, y, t: at } => self.track_docked(Some((x, y)), at),
            _ => {}
        }
        match input {
            Input::Down { x, y, mods, .. } => self.press(x, y, mods, input, t),
            Input::Move { x, y, .. } => self.drag(x, y, input, t),
            Input::Up { .. } | Input::Cancel { .. } => self.release(input, t),
            Input::DoubleClick => self.double_click(t),
            _ => match self.captured {
                Some(id) => self.feed(id, input, t),
                None => Vec::new(),
            },
        }
    }

    /// Feed input to one control directly (focused keyboard input, a mapped
    /// hardware footswitch, wheel over a hovered control).
    pub fn feed(&mut self, id: NodeId, input: Input, t: f64) -> Vec<EngineCall> {
        let Some(control) = self.surface.controls.get(&id).cloned() else {
            return Vec::new();
        };
        let position = self.position(id, &control);
        let bounds = self.boxes.get(&id).copied().unwrap_or_default();
        // Gestures work in the control's own (box) space: a fader in a
        // rotated frame slides along its rotated track.
        // A control on a popover or menu is drawn away from its layout box.
        let (dx, dy) = self.displacement(id);
        let input = match input {
            Input::Down { x, y, t, mods } => {
                let (x, y) = self.local(id, x - dx, y - dy);
                Input::Down { x, y, t, mods }
            }
            Input::Move { x, y, t, mods } => {
                let (x, y) = self.local(id, x - dx, y - dy);
                Input::Move { x, y, t, mods }
            }
            Input::Up { x, y, t } => {
                let (x, y) = self.local(id, x - dx, y - dy);
                Input::Up { x, y, t }
            }
            other => other,
        };
        if let Some((u, v)) = self.handle_uv(&control) {
            let area = self.handle_area(id);
            // An unmoving axis drags nothing; start it mid-way.
            let uv = (u.unwrap_or(0.5), v.unwrap_or(0.5));
            return self.feed_handle(id, &control, input, uv, area, t);
        }
        // A stepped param moves a whole detent per wheel notch or arrow key:
        // the gesture's small nudges would snap straight back.
        if let Some(calls) = self.step_by_detent(id, &control, &input, t) {
            return calls;
        }
        let outputs = self
            .gestures
            .entry(id)
            .or_insert_with(|| Gesture::new(control.kind.clone()))
            .handle(input, position, bounds);
        let mut calls = Vec::new();
        for o in outputs {
            self.output(id, &control, o, t, &mut calls);
        }
        calls
    }

    /// Advance time: flush coalesced edits, fire footswitch holds.
    pub fn poll(&mut self, t: f64) -> Vec<EngineCall> {
        let mut calls: Vec<EngineCall> = self
            .coalescer
            .poll(t)
            .into_iter()
            .map(|e| EngineCall::SetParam {
                addr: e.addr,
                value: e.value,
                phase: e.phase,
            })
            .collect();
        if let Some(id) = self.captured {
            calls.extend(self.feed(id, Input::Tick { t }, t));
        }
        calls
    }

    fn output(
        &mut self,
        id: NodeId,
        control: &Control,
        o: Output,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        match o {
            Output::Set { position, phase } => {
                self.set_position(id, control.binding.as_ref(), position, phase, t, calls)
            }
            Output::SetXy { x, y, phase } => {
                self.set_position(id, control.binding.as_ref(), x, phase, t, calls);
                self.local_positions.insert(id, y);
                if let Some(yb) = &control.y_binding {
                    self.send(yb, y, phase, t, calls);
                }
            }
            Output::Reset => {
                let position = control
                    .default
                    .unwrap_or_else(|| self.default_position(control));
                self.set_position(
                    id,
                    control.binding.as_ref(),
                    position,
                    EditPhase::End,
                    t,
                    calls,
                );
            }
            Output::Fire | Output::Tap => match control.binding.as_ref().map(|b| &b.source) {
                Some(Source::Action { id: action }) => calls.push(EngineCall::Invoke {
                    action: action.clone(),
                }),
                // A cycle button steps its choice.
                Some(Source::Param { addr })
                    if matches!(
                        control.kind,
                        ControlKind::Button {
                            mode: crate::surface::proto::ButtonMode::Cycle
                        }
                    ) =>
                {
                    let cmd = [crate::surface::proto::Command::Cycle { addr: addr.clone() }];
                    calls.extend(self.run(&cmd, None, Some(id), t));
                }
                // A tapped footswitch bound to a toggle param flips it.
                Some(Source::Param { .. }) => {
                    let position = if self.position(id, control) >= 0.5 {
                        0.0
                    } else {
                        1.0
                    };
                    self.set_position(
                        id,
                        control.binding.as_ref(),
                        position,
                        EditPhase::End,
                        t,
                        calls,
                    );
                }
                _ => {}
            },
            Output::Hold | Output::HoldRelease => {
                // Hold semantics are the binding's business (a hold action);
                // record it locally so the design can show it (LED).
                self.local_positions
                    .insert(id, if matches!(o, Output::Hold) { 1.0 } else { 0.0 });
            }
            Output::Nudge { .. } => self.handle_output(control, o, t, calls),
        }
    }

    fn set_position(
        &mut self,
        id: NodeId,
        binding: Option<&Binding>,
        position: f64,
        phase: EditPhase,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        match binding {
            Some(b) => self.send(b, position, phase, t, calls),
            None => {
                self.local_positions.insert(id, position);
            }
        }
    }

    /// The number of positions a param steps through (choices, toggles,
    /// stepped ranges), when it has 2 or more.
    fn param_steps(&self, b: &Binding) -> Option<u32> {
        let Source::Param { addr } = &b.source else {
            return None;
        };
        self.catalog
            .param(addr)
            .and_then(|p| p.range.steps)
            .filter(|n| *n >= 2)
    }

    /// Wheel and arrow keys on a stepped param: one detent per notch/press.
    fn step_by_detent(
        &mut self,
        id: NodeId,
        control: &Control,
        input: &Input,
        t: f64,
    ) -> Option<Vec<EngineCall>> {
        let b = control.binding.as_ref()?;
        let n = self.param_steps(b)?;
        let delta = match input {
            Input::Wheel { notches, .. } if *notches != 0.0 => {
                notches.signum() * notches.abs().round().max(1.0)
            }
            Input::Key {
                key: Key::Up | Key::Right | Key::PageUp,
                ..
            } => 1.0,
            Input::Key {
                key: Key::Down | Key::Left | Key::PageDown,
                ..
            } => -1.0,
            _ => return None,
        };
        let Source::Param { addr } = &b.source else {
            return None;
        };
        let k = f64::from(n - 1);
        let value = self
            .values
            .get(addr)
            .copied()
            .unwrap_or_else(|| b.to_target(self.position(id, control)));
        let next = (((value * k).round() + delta).clamp(0.0, k)) / k;
        let mut calls = Vec::new();
        self.set_position(
            id,
            Some(b),
            b.to_position(next),
            EditPhase::End,
            t,
            &mut calls,
        );
        Some(calls)
    }

    fn send(
        &mut self,
        b: &Binding,
        position: f64,
        phase: EditPhase,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        let Source::Param { addr } = &b.source else {
            return;
        };
        let mut value = b.to_target(position);
        // A stepped param lands on its detents: a 3-way switch never sits
        // between positions, even mid-drag.
        if let Some(p) = self.catalog.param(addr) {
            value = p.range.quantize(value);
        }
        // Optimistic: show the edit immediately; the engine echo confirms.
        self.values.insert(addr.clone(), value);
        calls.extend(
            self.coalescer
                .push(addr.clone(), value, phase, t)
                .into_iter()
                .map(|e| EngineCall::SetParam {
                    addr: e.addr,
                    value: e.value,
                    phase: e.phase,
                }),
        );
    }

    fn default_position(&self, control: &Control) -> f64 {
        match control.binding.as_ref() {
            Some(
                b @ Binding {
                    source: Source::Param { addr },
                    ..
                },
            ) => self
                .catalog
                .param(addr)
                .map_or(0.0, |p| b.to_position(p.default)),
            _ => 0.0,
        }
    }

    /// A control's current position (0..1).
    #[must_use]
    pub fn position(&self, id: NodeId, control: &Control) -> f64 {
        // A handle reads as its x param (else its y).
        let binding = match &control.kind {
            ControlKind::Handle { x, y, .. } => x.as_ref().or(y.as_ref()),
            _ => control.binding.as_ref(),
        };
        match binding {
            Some(
                b @ Binding {
                    source: Source::Param { addr },
                    ..
                },
            ) => self.values.get(addr).map_or(0.0, |v| b.to_position(*v)),
            _ => self.local_positions.get(&id).copied().unwrap_or(0.0),
        }
    }

    // ── surface → screen ──────────────────────────────────────────────────

    /// What a control shows from its values alone (see [`Self::state`]).
    fn base_state(&self, id: NodeId, control: &Control, t: f64) -> ControlState {
        match &control.kind {
            ControlKind::Plot { style, x, y } => {
                let trace = self.plot_trace(id, control, *style, x, y);
                // A history's (or a scalar's) latest reading, for a `Value` slot.
                let latest = match (x, &control.binding) {
                    (Axis::Time { .. }, _) => {
                        self.histories.get(&id).and_then(|r| r.back().copied())
                    }
                    (
                        _,
                        Some(Binding {
                            source:
                                Source::Stream {
                                    id: sid,
                                    channel: Some(c),
                                },
                            ..
                        }),
                    ) => self.stream_value(sid, Some(*c)),
                    _ => None,
                };
                let fmt = control.format.clone().unwrap_or_default();
                return ControlState {
                    trace,
                    value_text: latest.filter(|v| v.is_finite()).map(|v| fmt.db(v)),
                    ..ControlState::default()
                };
            }
            ControlKind::Curve { .. } => {
                return ControlState {
                    trace: self.curve_trace(control),
                    ..ControlState::default()
                };
            }
            ControlKind::Handle { x, y, .. } => {
                let display = |b: &Option<Binding>| match b.as_ref().map(|b| &b.source) {
                    Some(Source::Param { addr }) => {
                        let info = self.catalog.param(addr)?;
                        Some(info.display_as(
                            self.values.get(addr).copied().unwrap_or(info.default),
                            control.format.as_ref(),
                        ))
                    }
                    _ => None,
                };
                let text: Vec<String> = [display(x), display(y)].into_iter().flatten().collect();
                let label = [x, y].into_iter().flatten().find_map(|b| match &b.source {
                    Source::Param { addr } => self
                        .catalog
                        .param(addr)
                        .map(|p| p.short_name.clone().unwrap_or_else(|| p.name.clone())),
                    _ => None,
                });
                return ControlState {
                    position: self.position(id, control),
                    handle: self.handle_uv(control),
                    value_text: (!text.is_empty()).then(|| text.join("  ")),
                    label_text: label,
                    ..ControlState::default()
                };
            }
            _ => {}
        }
        // The binding's curve shapes a meter's dB span, so a needle follows
        // a printed scale law (a VU card crowds its low end) and a ladder's
        // lamps land on their printed thresholds.
        let curve = || {
            control
                .binding
                .as_ref()
                .map_or(crate::surface::proto::Curve::Linear, |b| b.curve.clone())
        };
        let level_stream = |sid: &EntryId| {
            matches!(
                self.catalog.stream(sid).map(|s| &s.kind),
                Some(StreamKind::Level { .. } | StreamKind::GainReduction)
            )
        };
        match (&control.kind, control.binding.as_ref().map(|b| &b.source)) {
            (
                ControlKind::Gauge {
                    floor_db,
                    ceiling_db,
                    ..
                },
                Some(Source::Stream { id: sid, .. }),
            ) if level_stream(sid) || control.select.is_some() => {
                let scale = MeterScale {
                    floor_db: *floor_db,
                    ceiling_db: *ceiling_db,
                    curve: curve(),
                };
                self.meter_state(id, control, &scale, t)
            }
            (ControlKind::Meter { .. }, Some(Source::Stream { id: sid, .. }))
                if level_stream(sid) || control.select.is_some() =>
            {
                let scale = MeterScale {
                    curve: curve(),
                    ..self.meter_scale.clone()
                };
                self.meter_state(id, control, &scale, t)
            }
            (_, Some(Source::Stream { id: sid, channel })) => {
                let v = self
                    .streams
                    .get(sid)
                    .and_then(|v| v.get(usize::from(channel.unwrap_or(0))))
                    .copied();
                let range = self.catalog.stream(sid).and_then(|s| match &s.kind {
                    StreamKind::Scalar { range } => Some(range.clone()),
                    _ => None,
                });
                ControlState {
                    position: match (&range, v) {
                        (Some(r), Some(v)) => r.to_normalized(v),
                        _ => 0.0,
                    },
                    value_text: v.map(|v| match (range, &control.format) {
                        (Some(r), Some(f)) => r.format_as(v, f),
                        (Some(r), None) => r.format(v),
                        (None, Some(f)) => crate::surface::proto::Range::linear(
                            0.0,
                            1.0,
                            crate::surface::proto::Unit::None,
                        )
                        .format_as(v, f),
                        (None, None) => format!("{v:.1}"),
                    }),
                    ..ControlState::default()
                }
            }
            (_, Some(Source::Param { addr })) => {
                let info = self.catalog.param(addr);
                let value = self.values.get(addr).copied().unwrap_or(0.0);
                let lit = match control.kind {
                    ControlKind::Button {
                        mode: crate::surface::proto::ButtonMode::Select { index, count },
                    } => {
                        let at = crate::surface::proto::ButtonMode::select_position(index, count);
                        Some(
                            (self.position(id, control) - at).abs()
                                < 0.5 / f64::from(count.max(2) - 1),
                        )
                    }
                    _ => None,
                };
                ControlState {
                    lit,
                    position: self.position(id, control),
                    bipolar: info.is_some_and(|p| p.bipolar),
                    value_text: info.map(|p| p.display_as(value, control.format.as_ref())),
                    label_text: info
                        .map(|p| p.short_name.clone().unwrap_or_else(|| p.name.clone())),
                    ..ControlState::default()
                }
            }
            (_, Some(Source::Action { id: aid })) => ControlState {
                position: self.local_positions.get(&id).copied().unwrap_or(0.0),
                label_text: self.catalog.action(aid).map(|a| a.name.clone()),
                ..ControlState::default()
            },
            _ => ControlState {
                position: self.local_positions.get(&id).copied().unwrap_or(0.0),
                ..ControlState::default()
            },
        }
    }

    /// The scene to draw at time `t` (ms): the design with every control's
    /// slots driven by its current state.
    #[must_use]
    pub fn frame(&self, t: f64) -> Scene {
        let mut scene = self.surface.scene.clone();
        for (id, control) in &self.surface.controls {
            // Nothing to drive in an unused item or a closed popover.
            if !self.drawn(*id) {
                continue;
            }
            let state = self.state(*id, control, t);
            let ops = control_ops(&self.surface.scene, *id, control, &state);
            // Slot ops only touch existing nodes; a failure means a dangling
            // slot, which lint reports — skip that control rather than abort.
            let _ = scene.transact(ops);
        }
        self.animate(&mut scene, t);
        self.frame_ui(&mut scene);
        scene
    }

    /// Whether any control needs redrawing over time (meters falling,
    /// a gesture in flight) — lets hosts sleep when nothing moves.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.captured.is_some()
            || !self.meters.is_empty()
            || !self.histories.is_empty()
            || self.motion_animating()
            || self.ui.edit.is_some()
            || self.reads_clock()
    }

    /// Controls whose parameter is a choice/toggle (for selector hit-mapping).
    #[must_use]
    pub fn choice_count(&self, control: &Control) -> Option<usize> {
        match control.binding.as_ref().map(|b| &b.source) {
            Some(Source::Param { addr }) => match self.catalog.param(addr).map(|p| &p.kind) {
                Some(ParamKind::Choice { options }) => Some(options.len()),
                Some(ParamKind::Toggle) => Some(2),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Where a plot's `Area` closes (unit `y`): its baseline, else the bottom.
fn baseline(style: PlotStyle, y: &PlotRange) -> f64 {
    match style {
        PlotStyle::Area { baseline } => {
            let b = y.to_unit(baseline);
            if b.is_finite() { b } else { 0.0 }
        }
        _ => 0.0,
    }
}

/// Series `name`, or the first.
fn pick<'a>(all: &'a [Series], name: Option<&str>) -> Option<&'a Series> {
    match name {
        Some(n) => all.iter().find(|s| s.name == n),
        None => all.first(),
    }
}

// ── transforms (the renderer's group semantics) ──────────────────────────

fn is_translation(t: &Transform) -> bool {
    const EPS: f64 = 1e-9;
    (t.a - 1.0).abs() <= EPS && t.b.abs() <= EPS && t.c.abs() <= EPS && (t.d - 1.0).abs() <= EPS
}

/// `p ∘ q`: apply `q`, then `p`.
fn compose(p: &Transform, q: &Transform) -> Transform {
    Transform {
        a: p.a * q.a + p.b * q.c,
        b: p.a * q.b + p.b * q.d,
        tx: p.a * q.tx + p.b * q.ty + p.tx,
        c: p.c * q.a + p.d * q.c,
        d: p.c * q.b + p.d * q.d,
        ty: p.c * q.tx + p.d * q.ty + p.ty,
    }
}

fn invert(t: &Transform) -> Option<Transform> {
    let det = t.a * t.d - t.b * t.c;
    if det.abs() < 1e-12 {
        return None;
    }
    let (a, b, c, d) = (t.d / det, -t.b / det, -t.c / det, t.a / det);
    Some(Transform {
        a,
        b,
        tx: -(a * t.tx + b * t.ty),
        c,
        d,
        ty: -(c * t.tx + d * t.ty),
    })
}

fn apply(t: &Transform, x: f64, y: f64) -> (f64, f64) {
    (t.a * x + t.b * y + t.tx, t.c * x + t.d * y + t.ty)
}

/// How the renderer draws `id`'s flattened box on screen: every container
/// on the way down whose transform has a rotation/scale/skew wraps its
/// subtree in that linear part pivoted about its own box origin (groups and
/// boolean ops are transparent, as in Figma), and the node itself is turned
/// about its origin. `None` when nothing on the way is transformed.
fn box_to_screen(scene: &Scene, id: NodeId, boxes: &HashMap<NodeId, Bounds>) -> Option<Transform> {
    let mut chain = vec![id];
    let mut up = scene.node(id)?.parent;
    while let Some(p) = up {
        chain.push(p);
        up = scene.node(p).and_then(|n| n.parent);
    }
    chain.reverse();
    let mut at = (0.0, 0.0);
    let mut world: Option<Transform> = None;
    for nid in chain {
        let n = scene.node(nid)?;
        at = (at.0 + n.transform.tx, at.1 + n.transform.ty);
        let (ox, oy) = boxes.get(&nid).map_or(at, |b| (b.x, b.y));
        at = (ox, oy);
        let t = &n.transform;
        if is_translation(t)
            || matches!(n.kind, NodeKind::Group | NodeKind::BooleanOperation { .. })
        {
            continue;
        }
        let g = Transform {
            a: t.a,
            b: t.b,
            tx: ox - (t.a * ox + t.b * oy),
            c: t.c,
            d: t.d,
            ty: oy - (t.c * ox + t.d * oy),
        };
        world = Some(world.map_or(g, |w| compose(&w, &g)));
    }
    world
}

#[cfg(test)]
mod tests {
    use crate::document::proto::{Node, Op, Size};
    use crate::surface::proto::{Catalog, ControlKind};

    use super::*;

    fn insert(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
        let id = node.id;
        scene
            .apply(Op::Insert {
                node,
                parent,
                index: u32::MAX,
            })
            .unwrap();
        id
    }

    /// A 100×100 panel at (100, 0) turned 90° clockwise about its corner
    /// (so it covers x ∈ [0, 100]), holding a 20×40 knob at its corner —
    /// drawn at x ∈ [60, 100], y ∈ [0, 20].
    fn turned_panel() -> (Runtime, NodeId, HashMap<NodeId, Bounds>) {
        let mut scene = Scene::new("Stage");
        let page = scene.pages()[0];
        let mut panel = Node::new(NodeKind::Frame, "panel");
        panel.size = Size {
            width: 100.0,
            height: 100.0,
        };
        panel.transform = Transform {
            a: 0.0,
            b: -1.0,
            tx: 100.0,
            c: 1.0,
            d: 0.0,
            ty: 0.0,
        };
        let panel = insert(&mut scene, page, panel);
        let mut knob = Node::new(NodeKind::Frame, "knob");
        knob.size = Size {
            width: 20.0,
            height: 40.0,
        };
        let knob = insert(&mut scene, panel, knob);
        let controls = [(knob, Control::new(ControlKind::knob()))]
            .into_iter()
            .collect();
        let boxes = crate::render::build_scene_layout_boxes(&scene, page)
            .into_iter()
            .map(|(id, b)| {
                (
                    id,
                    Bounds {
                        x: b.x,
                        y: b.y,
                        width: b.width,
                        height: b.height,
                    },
                )
            })
            .collect();
        (
            Runtime::new(Surface { scene, controls }, Catalog::default()),
            knob,
            boxes,
        )
    }

    #[test]
    fn a_control_in_a_rotated_frame_is_hit_where_it_is_drawn() {
        let (mut rt, knob, boxes) = turned_panel();
        // The layout box is the unturned one: x ∈ [100, 120], y ∈ [0, 40].
        assert_eq!(
            boxes[&knob],
            Bounds {
                x: 100.0,
                y: 0.0,
                width: 20.0,
                height: 40.0
            }
        );
        rt.set_boxes(boxes.clone());
        assert_eq!(rt.hit(80.0, 10.0), Some(knob), "where it is drawn");
        assert_eq!(rt.hit(62.0, 18.0), Some(knob));
        assert_eq!(rt.hit(110.0, 30.0), None, "not where its unturned box is");
        assert_eq!(rt.hit(50.0, 10.0), None);

        // Only the controls' boxes: ancestors pivot at their authored offsets.
        rt.set_boxes([(knob, boxes[&knob])]);
        assert_eq!(rt.hit(80.0, 10.0), Some(knob));
        assert_eq!(rt.hit(110.0, 30.0), None);

        // A host that tracks world transforms can hand them in directly.
        let turn = Transform {
            a: 0.0,
            b: -1.0,
            tx: 100.0,
            c: 1.0,
            d: 0.0,
            ty: -100.0,
        };
        rt.set_transformed_boxes([(knob, boxes[&knob], Some(turn))]);
        assert_eq!(rt.hit(80.0, 10.0), Some(knob));
        rt.set_transformed_boxes([(knob, boxes[&knob], None)]);
        assert_eq!(
            rt.hit(110.0, 30.0),
            Some(knob),
            "no transform: the box as given"
        );
    }

    #[test]
    fn untransformed_controls_keep_their_boxes() {
        let (mut rt, knob, _) = turned_panel();
        let mut scene = rt.surface().scene.clone();
        let panel = scene.node(knob).and_then(|n| n.parent).unwrap();
        scene.node_mut(panel).unwrap().transform = Transform::translate(100.0, 0.0);
        rt.surface.scene = scene;
        rt.set_boxes([(
            knob,
            Bounds {
                x: 100.0,
                y: 0.0,
                width: 20.0,
                height: 40.0,
            },
        )]);
        assert!(rt.to_box.is_empty());
        assert_eq!(rt.hit(110.0, 30.0), Some(knob));
    }
}
