//! The component registry: what exists ([`components`]) and how to put one
//! in a document ([`place`]).

use crate::document::proto::{NodeId, TextCase, Transform};
use crate::surface::proto::{Binding, Control, ControlKind, SourceSelect, ValueFormat};
use crate::surface::widgets::Builder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Knob,
    Meter,
    Button,
    Switch,
    Lever,
    Panel,
    Viz,
    Material,
}

impl Family {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Knob => "knob",
            Self::Meter => "meter",
            Self::Button => "button",
            Self::Switch => "switch",
            Self::Lever => "lever",
            Self::Panel => "panel",
            Self::Viz => "viz",
            Self::Material => "material",
        }
    }
}

/// The ink a component prints its scale, numerals and legends in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Ink {
    /// Silkscreen for a dark panel — each component's own light ink.
    #[default]
    Light,
    /// Silkscreen for a light panel (a grey LA-2A, brushed aluminium).
    Dark,
    /// Any colour, `#hex`.
    Color(String),
}

/// The dark ink `Ink::Dark` prints in.
pub const DARK_INK: &str = "#1d1f22";

/// The touch minimum `design-lint` checks input controls against, px — pass
/// it to [`Place::touch`] so a tight box never drops under a fingertip.
pub const TOUCH_MIN: f64 = 44.0;

impl Ink {
    /// The colour for a component whose own light ink is `light`.
    #[must_use]
    pub fn color(&self, light: &str) -> String {
        match self {
            Self::Light => light.to_string(),
            Self::Dark => DARK_INK.to_string(),
            Self::Color(c) => c.clone(),
        }
    }
}

/// How to place a component. Every field but `name` is optional; a
/// component picks sensible defaults for its style.
#[derive(Debug, Clone, Default)]
pub struct Place {
    pub name: String,
    /// Primary dimension in px (knob diameter, meter width, button edge).
    pub size: Option<f64>,
    pub binding: Option<Binding>,
    /// A style-specific variant (a colour, a finish: `"red"`, `"black"`).
    pub variant: Option<String>,
    /// Legend printed with it (`"INPUT"`); `None` = the bound param's name.
    pub legend: Option<String>,
    /// Printed scale labels, first at the start of travel (`["0", …, "10"]`).
    pub scale: Option<Vec<String>>,
    /// Options for multi-position components (rotary switches, button banks;
    /// on a knob: its detents, printed as its scale, stepped sweep).
    pub options: Option<Vec<String>>,
    /// The ink its printed scale, numerals and legends take (default: the
    /// component's own light silkscreen). Legends printed *on* a cap keep
    /// the cap's ink.
    pub ink: Option<Ink>,
    /// Print legends, options and scale as written instead of in capitals.
    pub keep_case: bool,
    /// Travel in degrees for rotaries (knobs, rotary switches, levers);
    /// `None` = the style's own — or, for a stepped knob with `options`, a
    /// detent sweep from their count (≤ 60° a click, ≤ 300° in all).
    pub sweep: Option<f64>,
    /// A tight hit box: the control's box is the moving part (the knob's
    /// body, the dial), padded out to at least this edge ([`TOUCH_MIN`] for
    /// the lint). The printed scale and legend hang outside it. `None` = the
    /// component's full drawn box (auto-layout friendly).
    pub touch: Option<f64>,
    /// Banks and ladders: lay the options out last-first (a column whose
    /// top is the param's last option; a ladder lit right to left).
    pub reverse: bool,
    /// The inner control of a concentric pair (`knob.concentric`): its
    /// binding, legend, scale, sweep.
    pub inner: Option<Box<Place>>,
    /// A meter's source switched by a choice param (the 1176's GR/+4/+8/Off).
    pub select: Option<SourceSelect>,
    /// How the control's readout prints its value.
    pub format: Option<ValueFormat>,
}

impl Place {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn ink(mut self, ink: Ink) -> Self {
        self.ink = Some(ink);
        self
    }

    #[must_use]
    pub fn keep_case(mut self) -> Self {
        self.keep_case = true;
        self
    }

    #[must_use]
    pub fn sweep(mut self, degrees: f64) -> Self {
        self.sweep = Some(degrees);
        self
    }

    #[must_use]
    pub fn touch(mut self, min_px: f64) -> Self {
        self.touch = Some(min_px);
        self
    }

    #[must_use]
    pub fn reversed(mut self) -> Self {
        self.reverse = true;
        self
    }

    #[must_use]
    pub fn inner(mut self, p: Place) -> Self {
        self.inner = Some(Box::new(p));
        self
    }

    #[must_use]
    pub fn select(mut self, s: SourceSelect) -> Self {
        self.select = Some(s);
        self
    }

    #[must_use]
    pub fn format(mut self, f: ValueFormat) -> Self {
        self.format = Some(f);
        self
    }

    /// The ink to print in, for a component whose own light ink is `light`.
    #[must_use]
    pub fn ink_color(&self, light: &str) -> String {
        self.ink
            .as_ref()
            .map_or_else(|| light.to_string(), |i| i.color(light))
    }

    /// How legends are cased: capitals unless [`Self::keep_case`].
    #[must_use]
    pub fn case(&self) -> TextCase {
        if self.keep_case {
            TextCase::Original
        } else {
            TextCase::Upper
        }
    }

    /// A `key=value` entry of `options` (`"floor=-40"`), for components
    /// configured that way (panels, LED meters).
    #[must_use]
    pub fn opt(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .flatten()
            .find_map(|o| o.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
    }

    /// Whether `options` holds the bare flag `flag` (`"momentary"`).
    #[must_use]
    pub fn flag(&self, flag: &str) -> bool {
        self.options.iter().flatten().any(|o| o == flag)
    }

    /// A lamp colour the place asks for, for lamp `i` (slot order — a
    /// meter's lamps from the floor up): `colors=#a,#b,…` one per lamp (the
    /// last carries on), or `color=#hex` for every lamp. Lamps, LED meters
    /// and footswitch LEDs take it over their variant's colour.
    #[must_use]
    pub fn lamp_color(&self, i: usize) -> Option<String> {
        let list: Vec<&str> = self
            .opt("colors")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|c| c.starts_with('#'))
                    .collect()
            })
            .unwrap_or_default();
        list.get(i)
            .or_else(|| list.last())
            .map(|c| (*c).to_string())
            .or_else(|| {
                self.opt("color")
                    .filter(|c| c.starts_with('#'))
                    .map(ToString::to_string)
            })
    }

    /// The control carrying this place's binding and readout format.
    #[must_use]
    pub fn control(&self, kind: ControlKind) -> Control {
        let mut c = Control::new(kind);
        c.binding.clone_from(&self.binding);
        c.format.clone_from(&self.format);
        c.select.clone_from(&self.select);
        c
    }

    #[must_use]
    pub fn size(mut self, px: f64) -> Self {
        self.size = Some(px);
        self
    }

    #[must_use]
    pub fn bind(mut self, b: Binding) -> Self {
        self.binding = Some(b);
        self
    }

    #[must_use]
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = Some(v.into());
        self
    }

    #[must_use]
    pub fn legend(mut self, l: impl Into<String>) -> Self {
        self.legend = Some(l.into());
        self
    }

    #[must_use]
    pub fn scale(mut self, labels: &[&str]) -> Self {
        self.scale = Some(labels.iter().map(ToString::to_string).collect());
        self
    }

    #[must_use]
    pub fn options(mut self, labels: &[&str]) -> Self {
        self.options = Some(labels.iter().map(ToString::to_string).collect());
        self
    }
}

/// Builds a component under `parent`; returns the control's root node.
pub type BuildFn = fn(&mut Builder<'_>, NodeId, &Place) -> NodeId;

/// One component the kit offers.
#[derive(Clone)]
pub struct ComponentDef {
    /// Stable id: `family.style` (`"knob.daka"`, `"meter.vu_ivory"`).
    pub id: &'static str,
    pub family: Family,
    pub title: &'static str,
    /// One or two sentences for a model choosing between components: what
    /// it looks like, what real unit it's from, what it's good for.
    pub description: &'static str,
    /// Accepted `Place::variant` values (empty = none).
    pub variants: &'static [&'static str],
    pub default_size: f64,
    /// What it binds to: `"param"`, `"choice param"`, `"toggle param"`,
    /// `"level stream"`, `"action"`, `"none"`.
    pub binds: &'static str,
    pub build: BuildFn,
}

impl std::fmt::Debug for ComponentDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentDef")
            .field("id", &self.id)
            .field("family", &self.family)
            .finish_non_exhaustive()
    }
}

/// Every component, grouped by family, in a stable order.
#[must_use]
pub fn components() -> Vec<ComponentDef> {
    let mut all: Vec<ComponentDef> = Vec::new();
    all.extend(crate::surface::kit::knobs::defs());
    all.extend(crate::surface::kit::knobs::concentric::defs());
    all.extend(crate::surface::kit::knobs::knurled_cap::defs());
    all.extend(crate::surface::kit::knobs::chrome_skirt::defs());
    all.extend(crate::surface::kit::knobs::studio::defs());
    all.extend(crate::surface::kit::knobs::valve::defs());
    all.extend(crate::surface::kit::knobs::outboard::defs());
    all.extend(crate::surface::kit::knobs::pedal::defs());
    all.extend(crate::surface::kit::knobs::amp::defs());
    all.extend(crate::surface::kit::knobs::pedal_tall::defs());
    all.extend(crate::surface::kit::knobs::amp_pointer::defs());
    all.extend(crate::surface::kit::meters::defs());
    all.extend(crate::surface::kit::controls::defs());
    all.extend(crate::surface::kit::panels::defs());
    all.extend(crate::surface::kit::panels::pedal::defs());
    all.extend(crate::surface::kit::viz::defs());
    all.extend(crate::surface::kit::stomp::defs());
    all.extend(crate::surface::kit::boutique::defs());
    all.extend(crate::surface::kit::deluxe::defs());
    all.extend(crate::surface::kit::materials::defs());
    all.sort_by_key(|c| (c.family, c.id));
    all
}

#[must_use]
pub fn find(id: &str) -> Option<ComponentDef> {
    components().into_iter().find(|c| c.id == id)
}

/// Place component `id` under `parent`.
///
/// # Errors
/// Unknown id (with the closest ids), or an unknown variant.
pub fn place(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> Result<NodeId, String> {
    let all = components();
    let Some(def) = all.iter().find(|c| c.id == id) else {
        let mut near: Vec<&str> = all
            .iter()
            .map(|c| c.id)
            .filter(|c| c.split('.').next() == id.split('.').next())
            .collect();
        if near.is_empty() {
            near = all.iter().map(|c| c.id).collect();
        }
        return Err(format!("no component `{id}`; try: {}", near.join(", ")));
    };
    if let Some(v) = &p.variant
        && !def.variants.is_empty()
        && !def.variants.contains(&v.as_str())
    {
        return Err(format!(
            "`{id}` has no variant `{v}`; variants: {}",
            def.variants.join(", ")
        ));
    }
    Ok((def.build)(b, parent, p))
}

/// [`place`] into a holder frame the size of everything the component
/// draws (scale, legend, overhang), so an auto-layout parent spaces it by
/// its drawing even when its control box is tight ([`Place::touch`]).
/// Returns the component's root (the holder is its parent).
///
/// # Errors
/// As [`place`].
pub fn place_in_cell(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    p: &Place,
) -> Result<NodeId, String> {
    let holder = b.add(
        parent,
        crate::surface::kit::draw::frame(&format!("{} cell", p.name), 0.0, 0.0, 0.0, 0.0),
    );
    let root = place(b, holder, id, p)?;
    let (x, y, w, h) = drawn_bounds(b.scene, root);
    if let Some(n) = b.scene.node_mut(root) {
        n.transform = Transform::translate(-x, -y);
    }
    if let Some(n) = b.scene.node_mut(holder) {
        n.size = crate::document::proto::Size {
            width: w,
            height: h,
        };
    }
    Ok(root)
}

/// The box (`x, y, w, h`, in `root`'s own space) covering `root` and every
/// visible node under it — what it draws, as opposed to its control box.
/// Offsets are summed (a turned node counts at its unturned box).
#[must_use]
pub fn drawn_bounds(scene: &crate::document::proto::Scene, root: NodeId) -> (f64, f64, f64, f64) {
    fn walk(
        scene: &crate::document::proto::Scene,
        id: NodeId,
        (ox, oy): (f64, f64),
        acc: &mut (f64, f64, f64, f64),
    ) {
        let Some(n) = scene.node(id) else { return };
        if !n.visible {
            return;
        }
        let (x, y) = (ox + n.transform.tx, oy + n.transform.ty);
        if n.size.width > 0.0 && n.size.height > 0.0 {
            acc.0 = acc.0.min(x);
            acc.1 = acc.1.min(y);
            acc.2 = acc.2.max(x + n.size.width);
            acc.3 = acc.3.max(y + n.size.height);
        }
        // What a clipping frame holds never shows past it.
        if n.clips_content {
            return;
        }
        for c in &n.children {
            walk(scene, *c, (x, y), acc);
        }
    }
    let Some(n) = scene.node(root) else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let mut acc = (0.0, 0.0, n.size.width, n.size.height);
    for c in &n.children {
        walk(scene, *c, (0.0, 0.0), &mut acc);
    }
    (acc.0, acc.1, acc.2 - acc.0, acc.3 - acc.1)
}

/// Degrees an `n`-position rotary switch turns through: ≤ 60° a click and
/// ≤ 300° in all (0 for fewer than two positions).
#[must_use]
pub fn detent_sweep(n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let step = (300.0 / (n - 1) as f64).min(60.0);
    step * (n - 1) as f64
}

/// Shrink a placed rotary's control box to its moving part: a square of
/// `body` px (at least `touch`) centred on the pivot at (`px`, `py`) in the
/// root's space. Everything it prints stays where it was on the panel — the
/// children shift by exactly the root's move — and simply hangs outside.
pub(crate) fn tighten(
    b: &mut Builder<'_>,
    root: NodeId,
    (px, py): (f64, f64),
    body: f64,
    touch: f64,
) {
    let side = body.max(touch);
    let (dx, dy) = (side / 2.0 - px, side / 2.0 - py);
    let kids = b
        .scene
        .node(root)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for k in kids {
        if let Some(n) = b.scene.node_mut(k) {
            n.transform.tx += dx;
            n.transform.ty += dy;
        }
    }
    if let Some(n) = b.scene.node_mut(root) {
        n.size = crate::document::proto::Size {
            width: side,
            height: side,
        };
    }
}
