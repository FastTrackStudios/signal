//! The Tone rack, as frame documents.
//!
//! session's mixer strip carries a rack of its processors — EQ, gate,
//! compressor, saturator, de-esser, delay, reverb — drawn by hand in
//! `apps/session-daw/src/tone.rs`. This is the same rack built from frame
//! nodes and controls: curves from the curve registry (the plugins' own
//! maths once the host registers it), plots bound to feeds for everything
//! that moves, handles for everything you grab. The geometry, the level-of-
//! detail tiers, the inks and the phase containers are session's, so the
//! two draw the same rack at the same widths.
//!
//! - [`ToneRack`]: the whole column — phase headers and units — for a strip.
//! - [`TonePanel`]: one unit alone, for a REAPER MCP/TCP slot.
//! - [`catalog`]: the params and feeds each unit binds (`<unit>/…`).

pub mod catalog;
pub mod demo;
pub mod panels;
pub mod style;

use crate::document::proto::{NodeId, Scene};
use crate::surface::proto::{Catalog, ParamAddr};
use crate::surface::runtime::Surface;
use crate::surface::widgets::{Builder, Theme};

use crate::embed::embed::{BuildCtx, Built, Derived, EmbedView, Values};
use crate::embed::form::{EmbedHost, SizeHints, Tier};
use style::{GAP, HEAD_H};

/// A mix phase: the container a run of units is read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase {
    Rescue,
    Tone,
    Polish,
    Relational,
    Depth,
}

impl Phase {
    /// The rack's phases, in signal order (session `RACK_PHASES`).
    pub const ALL: [Self; 5] = [
        Self::Rescue,
        Self::Tone,
        Self::Polish,
        Self::Relational,
        Self::Depth,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rescue => "Rescue",
            Self::Tone => "Tone",
            Self::Polish => "Polish",
            Self::Relational => "Relational",
            Self::Depth => "Depth",
        }
    }

    /// The phase's own colour (the toolbar icons' hues).
    #[must_use]
    pub const fn tint(self) -> &'static str {
        match self {
            Self::Rescue => "#ef4444",
            Self::Tone => "#facc15",
            Self::Polish => "#06b6d4",
            Self::Relational => "#3b82f6",
            Self::Depth => "#8b5cf6",
        }
    }

    /// The param that folds it shut (`rack/fold_<phase>`).
    #[must_use]
    pub fn fold_addr(self) -> String {
        format!("rack/fold_{}", self.name().to_lowercase())
    }
}

/// One unit of the rack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Which {
    RescueEq,
    Gate,
    RescueComp,
    Eq,
    Comp,
    Sat,
    DeEss,
    PolishEq,
    Space,
    Delay,
    Reverb,
}

impl Which {
    /// Every unit, in signal order (session `ALL_PANELS`).
    pub const ALL: [Self; 11] = [
        Self::RescueEq,
        Self::Gate,
        Self::RescueComp,
        Self::Eq,
        Self::Comp,
        Self::Sat,
        Self::DeEss,
        Self::PolishEq,
        Self::Space,
        Self::Delay,
        Self::Reverb,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RescueEq => "RESCUE EQ",
            Self::Gate => "GATE",
            Self::RescueComp => "RESCUE COMP",
            Self::Eq => "EQ",
            Self::Comp => "COMP",
            Self::Sat => "SAT",
            Self::DeEss => "DE-ESS",
            Self::PolishEq => "POLISH EQ",
            Self::Space => "SPACE",
            Self::Delay => "DELAY",
            Self::Reverb => "REVERB",
        }
    }

    /// The unit's param namespace.
    #[must_use]
    pub const fn ns(self) -> &'static str {
        match self {
            Self::RescueEq => "rescue_eq",
            Self::Gate => "gate",
            Self::RescueComp => "rescue_comp",
            Self::Eq => "eq",
            Self::Comp => "comp",
            Self::Sat => "sat",
            Self::DeEss => "deess",
            Self::PolishEq => "polish_eq",
            Self::Space => "space",
            Self::Delay => "delay",
            Self::Reverb => "reverb",
        }
    }

    #[must_use]
    pub const fn phase(self) -> Phase {
        match self {
            Self::RescueEq | Self::Gate | Self::RescueComp => Phase::Rescue,
            Self::Eq | Self::Comp | Self::Sat => Phase::Tone,
            Self::DeEss | Self::PolishEq => Phase::Polish,
            Self::Space => Phase::Relational,
            Self::Delay | Self::Reverb => Phase::Depth,
        }
    }

    #[must_use]
    pub const fn is_eq(self) -> bool {
        matches!(
            self,
            Self::RescueEq | Self::Eq | Self::PolishEq | Self::Space
        )
    }

    #[must_use]
    pub const fn is_comp(self) -> bool {
        matches!(self, Self::RescueComp | Self::Comp)
    }

    /// How tall the unit is, px, at every tier (session `Which::natural`:
    /// height is a fact about the unit, not about how wide it is).
    #[must_use]
    pub const fn natural(self) -> f64 {
        match self {
            Self::RescueEq | Self::Eq | Self::Space | Self::PolishEq => 175.0,
            Self::DeEss => 130.0,
            Self::Comp => 170.0,
            Self::RescueComp | Self::Gate => 120.0,
            Self::Sat => 130.0 + style::SELECTOR + 2.0,
            Self::Delay | Self::Reverb => 100.0 + style::SELECTOR + 2.0,
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|w| w.ns() == s || w.name().eq_ignore_ascii_case(s))
    }
}

/// A row of the column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A phase's container bar.
    Head(Phase),
    /// A phase this chain has no units in, kept so the phases after it
    /// line up with the strips beside it.
    Blank(Phase),
    Unit(Which),
}

/// `(x, y, w, h)`.
pub type Rect4 = (f64, f64, f64, f64);

/// Which phases are folded shut, from their params.
#[must_use]
pub fn folded(values: &Values) -> Vec<Phase> {
    Phase::ALL
        .into_iter()
        .filter(|p| values.get(&p.fold_addr()).is_some_and(|v| *v >= 0.5))
        .collect()
}

/// The column: every phase header and every unit under it, top to bottom
/// from `y` (session `tone::chain`, the same rows at the same heights).
#[must_use]
pub fn chain(units: &[Which], (x, y, w): (f64, f64, f64), folded: &[Phase]) -> Vec<(Row, Rect4)> {
    let mut out = Vec::new();
    if units.is_empty() {
        return out;
    }
    let mut y = y;
    // A chain that starts where a channel's does keeps a bar for every
    // phase, so what follows lines up across the mixer.
    let level = units
        .iter()
        .any(|u| matches!(u.phase(), Phase::Rescue | Phase::Tone));
    for phase in Phase::ALL {
        let mine: Vec<Which> = units
            .iter()
            .copied()
            .filter(|u| u.phase() == phase)
            .collect();
        if mine.is_empty() {
            if level {
                out.push((Row::Blank(phase), (x, y, w, HEAD_H)));
                y += HEAD_H;
            }
            continue;
        }
        out.push((Row::Head(phase), (x, y, w, HEAD_H)));
        y += HEAD_H;
        if folded.contains(&phase) {
            continue;
        }
        for u in mine {
            out.push((Row::Unit(u), (x, y, w, u.natural())));
            y += u.natural() + GAP;
        }
    }
    out
}

/// How tall the whole column comes to.
#[must_use]
pub fn tall(units: &[Which], folded: &[Phase]) -> f64 {
    chain(units, (0.0, 0.0, 1.0), folded)
        .last()
        .map_or(0.0, |(_, r)| r.1 + r.3)
}

/// The chains session gives each track role (`tone::*_CHAIN`).
pub mod chains {
    use super::Which::{self, *};

    /// An instrument or a voice: the whole chain.
    pub const CHANNEL: [Which; 11] = Which::ALL;
    /// A folder / bus.
    pub const BUS: [Which; 2] = [Eq, Comp];
    /// A delay return.
    pub const DELAY: [Which; 3] = [DeEss, Delay, Space];
    /// A reverb return.
    pub const REVERB: [Which; 3] = [DeEss, Reverb, Space];
    /// A fundamental.
    pub const FUND: [Which; 3] = [Gate, Eq, Sat];
    /// The Tone phase alone.
    pub const TONE: [Which; 3] = [Eq, Comp, Sat];
}

// ── the column as a view ─────────────────────────────────────────────────

/// A track's whole rack, for a strip: the phase containers and every
/// unit, laid out for the strip's width at session's tiers. Taller than
/// its box, it clips (the host scrolls by moving the box up).
#[derive(Debug, Clone)]
pub struct ToneRack {
    pub units: Vec<Which>,
    /// EQ bands per EQ unit.
    pub bands: usize,
    /// Scroll: how far the column is moved up, px.
    pub scroll: f64,
}

impl ToneRack {
    #[must_use]
    pub fn new(units: &[Which]) -> Self {
        Self {
            units: units.to_vec(),
            bands: 4,
            scroll: 0.0,
        }
    }

    /// The whole eleven-unit channel chain.
    #[must_use]
    pub fn channel() -> Self {
        Self::new(&chains::CHANNEL)
    }
}

impl EmbedView for ToneRack {
    fn catalog(&self) -> Catalog {
        catalog::rack(&self.units, self.bands)
    }

    fn build(&self, ctx: &BuildCtx) -> Built {
        let (w, h) = ctx.size;
        let mut k = Kit::new(w, h);
        let fold = folded(&ctx.values);
        let root = k.root;
        let col = style::clip(&mut k.b(), root, "rack", (0.0, 0.0, w, h));
        let rows = chain(&self.units, (0.0, -self.scroll, w), &fold);
        panels::column(&mut k, col, ctx, &rows, self.bands);
        let mut built = k.finish();
        built
            .rebuild_on
            .extend(Phase::ALL.map(|p| ParamAddr::new(p.fold_addr())));
        for u in &self.units {
            built
                .rebuild_on
                .extend(catalog::layout_params(*u).into_iter().map(ParamAddr::new));
        }
        built
    }

    fn hints(&self, host: EmbedHost) -> SizeHints {
        let mut h = host.hints();
        if host == EmbedHost::Strip {
            h.max.1 = tall(&self.units, &[]).max(h.min.1);
        }
        h
    }

    fn height_for(&self, _width: f64, values: &Values) -> Option<f64> {
        Some(tall(&self.units, &folded(values)))
    }
}

/// One unit alone — what a REAPER MCP or TCP slot shows for one plugin.
/// No phase bar; the unit takes the whole box.
#[derive(Debug, Clone)]
pub struct TonePanel {
    pub which: Which,
    pub bands: usize,
}

impl TonePanel {
    #[must_use]
    pub fn new(which: Which) -> Self {
        Self { which, bands: 4 }
    }
}

impl EmbedView for TonePanel {
    fn catalog(&self) -> Catalog {
        catalog::rack(&[self.which], self.bands)
    }

    fn build(&self, ctx: &BuildCtx) -> Built {
        let (w, h) = ctx.size;
        let mut k = Kit::new(w, h);
        let root = k.root;
        let rows = [(Row::Unit(self.which), (0.0, 0.0, w, h))];
        panels::column(&mut k, root, ctx, &rows, self.bands);
        let mut built = k.finish();
        built.rebuild_on.extend(
            catalog::layout_params(self.which)
                .into_iter()
                .map(ParamAddr::new),
        );
        built
    }

    fn hints(&self, host: EmbedHost) -> SizeHints {
        let mut h = host.hints();
        if host == EmbedHost::Mcp {
            h.preferred_aspect = 96.0 / self.which.natural();
        }
        h
    }

    fn height_for(&self, _width: f64, _values: &Values) -> Option<f64> {
        Some(self.which.natural())
    }
}

/// A scene under construction: its page, root frame, controls and the
/// derived properties the panels register.
pub struct Kit {
    pub scene: Scene,
    pub controls: std::collections::BTreeMap<NodeId, crate::surface::proto::Control>,
    pub theme: Theme,
    pub page: NodeId,
    pub root: NodeId,
    pub derived: Vec<Derived>,
    pub rebuild_on: Vec<ParamAddr>,
}

impl Kit {
    /// A page named `embed:strip` holding a `w × h` root frame.
    #[must_use]
    pub fn new(w: f64, h: f64) -> Self {
        let mut scene = Scene::new(EmbedHost::Strip.page());
        let page = scene.pages()[0];
        let mut controls = std::collections::BTreeMap::new();
        let theme = Theme::default();
        let root = {
            let mut b = Builder {
                scene: &mut scene,
                controls: &mut controls,
                theme: &theme,
            };
            let mut n = crate::surface::kit::draw::frame("embed", 0.0, 0.0, w, h);
            n.clips_content = true;
            b.add(page, n)
        };
        Self {
            scene,
            controls,
            theme,
            page,
            root,
            derived: Vec::new(),
            rebuild_on: Vec::new(),
        }
    }

    /// A builder over this kit's scene.
    pub fn b(&mut self) -> Builder<'_> {
        Builder {
            scene: &mut self.scene,
            controls: &mut self.controls,
            theme: &self.theme,
        }
    }

    pub fn derive(
        &mut self,
        node: NodeId,
        f: impl Fn(&Values, &mut crate::document::proto::Node) + Send + Sync + 'static,
    ) {
        self.derived.push(Derived::new(node, f));
    }

    #[must_use]
    pub fn finish(self) -> Built {
        Built {
            surface: Surface {
                scene: self.scene,
                controls: self.controls,
            },
            page: self.page,
            derived: self.derived,
            rebuild_on: self.rebuild_on,
            scale: 1.0,
        }
    }
}

/// The tier a unit draws at, for a box `w` wide.
#[must_use]
pub fn tier_of(w: f64) -> Tier {
    Tier::at(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_column_is_sessions() {
        let rows = chain(&chains::CHANNEL, (0.0, 0.0, 86.0), &[]);
        // Five heads, eleven units.
        assert_eq!(
            rows.iter()
                .filter(|(r, _)| matches!(r, Row::Head(_)))
                .count(),
            5
        );
        assert_eq!(
            rows.iter()
                .filter(|(r, _)| matches!(r, Row::Unit(_)))
                .count(),
            11
        );
        // A bus keeps a bar for every phase so its Tone lines up.
        let bus = chain(&chains::BUS, (0.0, 0.0, 86.0), &[]);
        let tone_y = |rows: &[(Row, Rect4)]| {
            rows.iter()
                .find(|(r, _)| *r == Row::Head(Phase::Tone))
                .map(|(_, r)| r.1)
        };
        assert!(bus.iter().any(|(r, _)| *r == Row::Blank(Phase::Rescue)));
        // A return has nothing to line up with: no blank bars.
        let ret = chain(&chains::REVERB, (0.0, 0.0, 86.0), &[]);
        assert!(!ret.iter().any(|(r, _)| matches!(r, Row::Blank(_))));
        // Folding Rescue on a channel moves Tone up to where a bus's is.
        let folded = chain(&chains::CHANNEL, (0.0, 0.0, 86.0), &[Phase::Rescue]);
        assert_eq!(tone_y(&folded), tone_y(&bus));
        assert!(tall(&chains::CHANNEL, &[Phase::Rescue]) < tall(&chains::CHANNEL, &[]));
    }
}
