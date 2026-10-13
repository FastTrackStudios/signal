//! What a hardware knob is *made of*, as data.
//!
//! A knob is not a shape with options — it is a short stack of concentric
//! tiers, sometimes with a knurl cut into one of them, an index you read the
//! value by, and a highlight where the room's light falls. Every unit's knob
//! is that same anatomy with different numbers.
//!
//! Before this existed, each style was spread across eleven `match self`
//! methods and eighteen `if style == ...` branches in the renderer, so adding
//! a knob meant finding all of them and getting every one right. The bug that
//! prompted the split is the shape of the problem: the 1073's teeth were on
//! the wrong tier, and moving them touched `body_fraction`, `has_skirt`,
//! `flutes`, `flute_band`, `pointer` and three render blocks.
//!
//! Now a knob is one [`KnobSpec`] const. The renderer walks it and knows
//! nothing about which unit it is drawing.
//!
//! # Adding a knob
//!
//! Add a [`KnobStyle`](crate::hardware::knob::KnobStyle) variant and one
//! `KnobSpec` describing it, outermost tier first. Then look at it:
//!
//! ```sh
//! cargo test -p fts-audio-ui --features native --test knob_sheet
//! ```
//!
//! which paints every style in the kit to one PNG. Nothing else needs
//! touching unless the knob has a silhouette no existing [`Index`] covers.
//!
//! Radii here are **fractions of the knob's outer radius**, so a spec is
//! independent of the diameter a panel asks for.

/// A tier's outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    /// A plain circle.
    Round,
    /// Rounded lobes: the Pultec's scalloped skirt, the 1073's geared cap.
    ///
    /// `depth` is also a fraction of the knob's outer radius, so teeth stay
    /// proportional when a panel asks for a bigger knob.
    Toothed { teeth: usize, depth: f64 },
}

/// How a surface takes the light, which is what makes it read as a material.
///
/// Only the hue moves when a control is tinted — the gradient's *shape* is
/// the material, so a tinted Marconi is still moulded plastic and a tinted
/// collet is still a flat-topped cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Finish {
    /// A dome: moulded plastic, lit from the upper left.
    Moulded,
    /// A flat top lit as a face rather than a sphere.
    FlatTop,
    /// Painted metal: an even face with the light only at the very top.
    Matte,
    /// Turned or brushed metal: a sweep across the surface.
    Brushed,
}

impl Finish {
    /// This finish in `color`.
    pub fn tinted(self, color: &str) -> String {
        match self {
            Self::Moulded => format!(
                "radial-gradient(circle at 36% 24%, color-mix(in oklab, {color} 72%, white) 0%, \
                 {color} 42%, color-mix(in oklab, {color} 62%, black) 100%)"
            ),
            Self::FlatTop => format!(
                "linear-gradient(162deg, color-mix(in oklab, {color} 76%, white) 0%, \
                 {color} 44%, color-mix(in oklab, {color} 72%, black) 100%)"
            ),
            Self::Matte => format!(
                "linear-gradient(160deg, color-mix(in oklab, {color} 88%, white) 0%, \
                 {color} 38%, color-mix(in oklab, {color} 82%, black) 100%)"
            ),
            Self::Brushed => format!(
                "radial-gradient(circle at 34% 26%, color-mix(in oklab, {color} 55%, white) 0%, \
                 {color} 58%, color-mix(in oklab, {color} 70%, black) 100%)"
            ),
        }
    }
}

/// How a tier is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    /// A CSS gradient on a div.
    ///
    /// Moulded plastic and brushed metal need one: a flat SVG fill cannot
    /// read as a material. `tint` says whether the control's colour replaces
    /// the gradient, through [`Finish::tinted`].
    Surface {
        css: &'static str,
        finish: Finish,
        tint: bool,
    },
    /// A flat SVG fill — matte paint, and the tiers of a stepped knob.
    Flat(&'static str),
    /// A flat SVG fill taking the control's colour, falling back to this.
    Tinted(&'static str),
    /// Stroke only: a turned groove, a seam between two tiers.
    Groove { color: &'static str, width: f64 },
}

/// Which half of a dual-concentric knob turns a tier.
///
/// On an ordinary knob both resolve to the same angle, so this only matters
/// where a panel has bound two controls to one placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turns {
    /// With the outer control — the collar. A band's frequency, on a 1073.
    Collar,
    /// With the inner control — the cap. That band's gain.
    Cap,
}

/// One concentric tier, from the outside in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// Radius, as a fraction of the knob's outer radius.
    pub r: f64,
    pub edge: Edge,
    pub paint: Paint,
    pub turns: Turns,
    /// A contact shadow dropped this far below the tier, in fractions of the
    /// outer radius. Zero for none.
    pub shadow: f64,
    /// An outline around the tier: `(colour, width)`.
    pub stroke: Option<(&'static str, f64)>,
}

impl Tier {
    /// A plain tier with no shadow and no outline — the common case.
    pub const fn new(r: f64, paint: Paint, turns: Turns) -> Self {
        Self {
            r,
            edge: Edge::Round,
            paint,
            turns,
            shadow: 0.0,
            stroke: None,
        }
    }

    pub const fn toothed(mut self, teeth: usize, depth: f64) -> Self {
        self.edge = Edge::Toothed { teeth, depth };
        self
    }

    pub const fn shadowed(mut self, drop: f64) -> Self {
        self.shadow = drop;
        self
    }

    pub const fn outlined(mut self, color: &'static str, width: f64) -> Self {
        self.stroke = Some((color, width));
        self
    }
}

/// What the value is read by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Index {
    /// A bar between two radii — the great majority of knobs.
    Bar {
        from: f64,
        to: f64,
        width: f64,
        color: &'static str,
    },
    /// A tapered blade running out from the hub.
    Blade {
        to: f64,
        half_width: f64,
        color: &'static str,
    },
    /// A Marconi wing: a coloured moulding laid across the knob and
    /// overhanging it, with a line down the middle. The overhang is the
    /// silhouette — see `WING_REACH` in the renderer.
    ///
    /// `body` is the disc the wing is proportioned to, as a fraction of the
    /// knob — its *width* follows that, while its reach past the rim is
    /// measured off the knob itself.
    Wing { color: &'static str, body: f64 },
    /// A pointer knob's moulded nose, reaching past the body toward the
    /// panel's printed scale.
    Nose { color: &'static str },
    /// None: the knob is read another way. A Distressor's numerals turn with
    /// its skirt past a fixed panel mark.
    None,
}

impl Index {
    /// Which of a concentric knob's controls this index reports.
    ///
    /// An index always belongs to the tier it is cut into, and every style
    /// here cuts it into the cap — including the 1073, whose collar gets its
    /// own through [`KnobSpec::collar_index`].
    pub fn turns(self) -> Turns {
        Turns::Cap
    }
}

/// A knurl cut into one band of the knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flutes {
    pub count: usize,
    /// The band the flutes are cut into, as fractions of the outer radius.
    pub from: f64,
    pub to: f64,
    /// How the ridges catch the light. Dark phenolic ridges read as
    /// highlights; a light cap's read as the shadow between them.
    pub stroke: &'static str,
    pub width: f64,
    /// The shadowed side of each ridge, half a flute over.
    ///
    /// A moulded knurl has a lit face and a dark one, and drawing both is
    /// what makes it read as cut rather than printed. Fine ridging on a dark
    /// body does not: at that size the pair merges into a bright band, which
    /// is why this is optional.
    pub shadow: Option<&'static str>,
    pub turns: Turns,
}

/// The room's light on the knob's face.
///
/// Drawn *outside* every rotating group, because a reflection that turns with
/// the control is the first thing that reads as wrong — the lamp above the
/// rack does not move when you turn a knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Specular {
    /// The blob's size and offset, as fractions of the knob's outer
    /// **diameter**. It is a CSS div rather than an SVG shape because the
    /// soft falloff is the whole point and a flat fill has a hard edge.
    pub w: f64,
    pub h: f64,
    pub dx: f64,
    pub dy: f64,
    /// The gradient painted into it.
    pub fill: &'static str,
    /// Tilt, in degrees. A light smear on a moulded face runs with the
    /// surface, not with the screen's axes.
    pub rotate: f64,
    /// A hairline of the same light along the top rim, as an SVG arc.
    pub rim: Option<&'static str>,
}

/// The soft highlight a moulded or turned surface takes from the light every
/// panel is lit by: offset up and left, and small — a big one reads as gloss
/// paint rather than plastic.
///
/// `body` is the lit tier's diameter as a fraction of the knob's, so a knob
/// whose face is a cap inside a collar gets a highlight sized to the cap.
pub const fn dome(body: f64) -> Specular {
    Specular {
        w: 0.62 * body,
        h: 0.42 * body,
        dx: -0.46 * body,
        dy: -0.40 * body,
        fill: "radial-gradient(ellipse at 50% 50%, rgba(255,255,255,0.15) 0%, \
               rgba(255,255,255,0.0) 70%)",
        rotate: 0.0,
        rim: None,
    }
}

/// A knob, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnobSpec {
    /// Concentric tiers, **outermost first**. The renderer stacks them in
    /// this order, so a later tier paints over an earlier one.
    pub tiers: &'static [Tier],
    /// The cap's index — what the inner control (or the whole knob, when
    /// there is only one) is read by.
    pub index: Index,
    /// A second index on the collar, for knobs whose two halves are separate
    /// controls. `None` on everything but the 1073.
    pub collar_index: Option<Index>,
    pub flutes: Option<Flutes>,
    pub specular: Option<Specular>,
    /// How far out the panel's printed scale must sit, in the knob's viewBox
    /// units. A nose or a wing reaches past the body, and a ring drawn for a
    /// flush knob lands underneath it.
    pub ring_offset: f64,
    /// The scale is printed on the knob's own skirt and turns with it, rather
    /// than on the panel around it.
    pub numerals_on_knob: bool,
    /// A small shadow at the very centre — the hub the index radiates from.
    ///
    /// Fixed, not rotating: it is the shaft the knob is pressed onto, and it
    /// does not turn with the cap. `None` for knobs whose own tiers already
    /// give the middle somewhere to be.
    pub hub: Option<&'static str>,
}

impl KnobSpec {
    /// The cap's radius as a fraction of the outer radius — the innermost
    /// tier that turns with the inner control.
    ///
    /// This is what a dual-concentric knob sizes its cap's drag region to, so
    /// pressing the middle gets you the inner control and pressing the ring
    /// around it gets you the outer one.
    pub fn cap_fraction(&self) -> f64 {
        self.tiers
            .iter()
            .filter(|t| t.turns == Turns::Cap)
            .map(|t| t.r)
            .fold(0.0, f64::max)
            .max(0.0)
    }

    /// Whether this knob's two halves are separate controls.
    pub fn is_concentric(&self) -> bool {
        self.collar_index.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tier = Tier::new(1.0, Paint::Flat("#000"), Turns::Collar);

    #[test]
    fn a_builder_tier_keeps_its_defaults() {
        assert_eq!(T.edge, Edge::Round);
        assert_eq!(T.shadow, 0.0);
        assert!(T.stroke.is_none());
    }

    #[test]
    fn the_builders_only_change_what_they_name() {
        let t = T.toothed(16, 0.07).shadowed(0.05).outlined("#111", 0.7);
        assert_eq!(t.r, 1.0);
        assert_eq!(t.turns, Turns::Collar);
        assert_eq!(
            t.edge,
            Edge::Toothed {
                teeth: 16,
                depth: 0.07
            }
        );
        assert_eq!(t.shadow, 0.05);
        assert_eq!(t.stroke, Some(("#111", 0.7)));
    }

    #[test]
    fn the_cap_fraction_is_the_widest_tier_that_turns_with_the_cap() {
        static TIERS: &[Tier] = &[
            Tier::new(1.0, Paint::Flat("#a"), Turns::Collar),
            Tier::new(0.62, Paint::Flat("#b"), Turns::Cap),
            Tier::new(0.30, Paint::Flat("#c"), Turns::Cap),
        ];
        let spec = KnobSpec {
            tiers: TIERS,
            index: Index::None,
            collar_index: None,
            flutes: None,
            specular: None,
            ring_offset: 0.0,
            numerals_on_knob: false,
            hub: None,
        };
        assert_eq!(spec.cap_fraction(), 0.62);
    }

    #[test]
    fn a_knob_with_no_cap_tier_reports_no_cap() {
        static TIERS: &[Tier] = &[Tier::new(1.0, Paint::Flat("#a"), Turns::Collar)];
        let spec = KnobSpec {
            tiers: TIERS,
            index: Index::None,
            collar_index: None,
            flutes: None,
            specular: None,
            ring_offset: 0.0,
            numerals_on_knob: false,
            hub: None,
        };
        assert_eq!(spec.cap_fraction(), 0.0);
    }

    #[test]
    fn tinting_moves_the_hue_and_keeps_the_finish() {
        // Each finish has its own gradient shape, and the colour lands in it.
        for finish in [
            Finish::Moulded,
            Finish::FlatTop,
            Finish::Matte,
            Finish::Brushed,
        ] {
            let css = finish.tinted("#c0ffee");
            assert!(css.contains("#c0ffee"), "{finish:?} dropped the colour");
            assert!(
                css.contains("gradient"),
                "{finish:?} is not a gradient, so it cannot read as a material",
            );
        }
        assert!(Finish::Moulded.tinted("#fff").starts_with("radial"));
        assert!(Finish::FlatTop.tinted("#fff").starts_with("linear"));
    }
}
