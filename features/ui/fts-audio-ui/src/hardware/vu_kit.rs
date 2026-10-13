//! What a VU movement *looks like*, as data.
//!
//! The companion to [`knob_kit`](crate::hardware::knob_kit), and the same
//! bargain: geometry lives in [`vu_svg`](crate::hardware::vu_svg) as pure
//! functions, the *drawing* is one [`VuSpec`] const per face, and the
//! renderer walks it without knowing which unit it is painting.
//!
//! A meter is a printed card behind glass, lit from behind by a bulb or two,
//! with a needle swinging over it and — if it is mounted through the panel
//! rather than printed on one — a bezel around it. Everything a theme wants
//! to move is one of those.
//!
//! # Adding a face
//!
//! Add a [`VuFace`](crate::hardware::vu::VuFace) variant and one `VuSpec`
//! in [`vu_faces`](crate::hardware::vu_faces). Then look at it:
//!
//! ```sh
//! cargo test -p fts-audio-ui --test vu_sheet
//! ```
//!
//! which paints every face in the kit, at rest and swinging, with and without
//! its bezel.
//!
//! # What is *not* a theme
//!
//! The card's numbers and where they sit. A VU is crowded at the bottom and a
//! decibel readout is evenly spaced, and that is the difference between two
//! *instruments*, not two colour schemes — so it stays in
//! [`VuScale`](crate::hardware::vu_svg::VuScale) where the geometry can test
//! it. A face says how the meter is lit and printed; a scale says what it
//! reads.

/// The needle and the hub it swings on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Needle {
    pub color: &'static str,
    /// Stroke width, in the card's viewBox units.
    pub width: f64,
    /// The hub's radius. It sits at the pivot, below the bottom of the card,
    /// so only its top half shows — which is what a real movement looks like.
    pub hub_r: f64,
    pub hub_opacity: f64,
    /// Light bleeding off the needle itself, on a backlit movement.
    ///
    /// A needle in front of a lamp is not a line — it is a line with the
    /// light spilling around it. Drawn as wide, faint strokes under the crisp
    /// one rather than a blur filter, which does not survive every renderer.
    pub halo: Option<Halo>,
}

/// The bloom around a lit needle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Halo {
    pub color: &'static str,
    /// How much wider the bloom is than the needle.
    pub spread: f64,
}

impl Halo {
    /// The bloom's strokes, widest first, so the crisp needle can be drawn
    /// over them. Two is enough to read as light and cheap enough to be free.
    pub fn widths(&self, needle_width: f64) -> [f64; 2] {
        [
            needle_width * self.spread,
            needle_width * (1.0 + (self.spread - 1.0) * 0.45),
        ]
    }
}

/// How a face takes a colour asked for at the call site.
///
/// Most movements do not: an LA-2A's card is the colour it is. A backlit one
/// is the exception — the whole look is a lamp behind smoked glass, and which
/// colour that lamp is is a decision a *panel* makes, not the movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wash {
    /// Fixed. A tint passed to this face is ignored.
    None,
    /// The colour becomes the glow behind the needle, the pool in the card
    /// and the bloom off the needle. The print stays white — a lit meter is
    /// read by contrast against the glow, not by tinting the numbers too.
    Backlit,
}

impl Wash {
    /// The card, under `color`.
    pub fn card(self, color: &str) -> Option<String> {
        match self {
            Self::None => None,
            Self::Backlit => Some(format!(
                "radial-gradient(ellipse at 50% 104%, \
                 color-mix(in srgb, {color} 30%, #05070a) 0%, #05070a 62%)"
            )),
        }
    }

    /// The lamp behind it. `color-mix` against `transparent` is how a hex
    /// becomes a translucent glow without the caller writing rgba by hand.
    pub fn lamp(self, color: &str) -> Option<String> {
        match self {
            Self::None => None,
            Self::Backlit => Some(format!("color-mix(in srgb, {color} 50%, transparent)")),
        }
    }

    /// The bloom off the needle.
    pub fn halo(self, color: &str) -> Option<String> {
        match self {
            Self::None => None,
            Self::Backlit => Some(format!("color-mix(in srgb, {color} 34%, transparent)")),
        }
    }
}

/// The bulb behind the card.
///
/// The lamp is what actually varies between units of the same era — the card
/// is ivory on nearly all of them, and it is the glow that makes an LA-2A
/// warm and a rackmount's whiter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub color: &'static str,
    /// Where the bulb sits behind the card, as percentages across and down.
    pub x: f64,
    pub y: f64,
    /// How far the glow reaches, as a percentage of the card.
    pub reach: f64,
    /// The bulb's *hotspot* — the tight bright patch right where the filament
    /// is, inside the broad wash.
    ///
    /// A single bulb behind a card does not light it evenly: there is a spot,
    /// and on an LA-2A you can see it plainly in the middle of the face. A
    /// wash without one reads as an evenly backlit panel rather than a lamp
    /// in a box.
    pub core: Option<Core>,
}

/// The bright patch at the bulb itself, inside a [`Lamp`]'s wash.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Core {
    pub color: &'static str,
    /// Where the filament is, if not right where the wash is centred.
    pub x: f64,
    pub y: f64,
    /// How tight the spot is, as a percentage of the card.
    pub reach: f64,
}

/// The frame a movement mounted *through* a panel sits in.
///
/// Lit from above, a sunken opening has its top face in shadow and its bottom
/// face catching the light — the opposite of a raised boss, and the whole
/// difference between a movement set into a panel and one printed on it. Get
/// those two the wrong way round and the meter reads as a sticker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bezel {
    /// The frame body.
    pub frame: &'static str,
    /// Corner rounding, in design px. A pressed-steel rack frame is nearly
    /// square; a moulded surround is not.
    pub radius: f64,
    /// Light spilling out around the opening, for a movement lit from behind
    /// brightly enough that the frame catches it. `None` for an unlit one.
    pub glow: Option<Glow>,
    /// The chamfer's four faces, in shadow-to-light order.
    pub top: &'static str,
    pub left: &'static str,
    pub right: &'static str,
    pub bottom: &'static str,
    /// Chamfer depth, in design px.
    pub depth: f64,
    /// The vent under the glass, which is most of what says the movement is
    /// mounted through the panel. `None` for a frame without one.
    pub vent: Option<Vent>,
    /// The room's reflection on the frame's own surface.
    ///
    /// A frame is a moulding in front of a lamp, and it catches light like
    /// one. Without it a black frame is a black rectangle — which is what
    /// makes a meter look pasted on rather than mounted.
    pub sheen: Option<&'static str>,
    /// The shadow the frame's lip throws *onto the card inside it*.
    ///
    /// A movement sits behind its frame, not flush with it, so the lip casts
    /// across the top of the face and down one side. Leaving it out is what
    /// makes a framed meter look like a picture pasted into a hole.
    pub shade: Option<Shade>,
}

/// The frame lip's shadow on the card behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    pub color: &'static str,
    /// How far the shadow reaches down from the top lip, in design px.
    pub top: f64,
    /// How far it reaches in from the shadowed side.
    pub side: f64,
}

impl Shade {
    /// The shadow as inset box-shadow layers, at a panel's scale.
    ///
    /// Lit from above, the top lip throws the longest shadow and the left one
    /// a shorter one; the bottom and right lips throw none worth drawing,
    /// which is what keeps the direction of the light readable.
    pub fn css(&self, scale: f64) -> String {
        format!(
            "inset 0 {:.1}px {:.1}px {}, inset {:.1}px 0 {:.1}px {}",
            self.top * scale,
            self.top * 1.7 * scale,
            self.color,
            self.side * scale,
            self.side * 1.8 * scale,
            self.color,
        )
    }
}

/// Light escaping around a lit movement's opening.
///
/// A backlit meter does not stop at the glass: the bulb spills around the
/// frame, and on a dark panel that halo is most of what says the meter is
/// *on*. It is drawn as light thrown outward from the opening, so it reads
/// against the panel rather than washing out the card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub color: &'static str,
    /// How far the light reaches past the frame, in design px.
    pub spread: f64,
    /// A tighter, brighter core just at the opening's edge.
    pub core: Option<&'static str>,
}

impl Glow {
    /// The halo as box-shadow layers, at a panel's scale.
    pub fn css(&self, scale: f64) -> String {
        let outer = format!("0 0 {:.1}px {}", self.spread * scale, self.color);
        match self.core {
            Some(core) => format!("{outer}, 0 0 {:.1}px {core}", self.spread * 0.35 * scale),
            None => outer,
        }
    }
}

/// The louvre under a meter's glass.
///
/// Structured rather than a finished gradient string because the stripe pitch
/// is in *design* px and has to scale with the panel like everything else. A
/// frozen gradient stays 2 px wide on a panel drawn at three times the size,
/// where it reads as a smear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vent {
    pub dark: &'static str,
    pub light: &'static str,
    /// One dark stripe plus one light stripe, in design px.
    pub pitch: f64,
}

impl Vent {
    /// The louvre at a panel's scale.
    pub fn css(&self, scale: f64) -> String {
        let half = self.pitch * scale / 2.0;
        let full = self.pitch * scale;
        format!(
            "repeating-linear-gradient(90deg, {} 0 {half:.2}px, {} {half:.2}px {full:.2}px)",
            self.dark, self.light,
        )
    }
}

/// A VU face, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VuSpec {
    /// The printed card behind the scale.
    pub card: &'static str,
    /// Everything silkscreened on it: the arc, the ticks, the numerals, the
    /// legend.
    pub ink: &'static str,
    /// The over-zero stretch of the scale — red on every VU ever made, and
    /// not present at all on a decibel readout.
    pub hot: &'static str,
    pub needle: Needle,
    pub lamp: Option<Lamp>,
    /// The reflection on the glass over the card. `None` for an open face.
    pub glass: Option<&'static str>,
    pub bezel: Bezel,
    /// How this face takes a colour asked for at the call site.
    pub wash: Wash,
}

impl VuSpec {
    /// The lamp's CSS, ready to paint. The hotspot is layered over the wash,
    /// so a bulb reads as a spot inside a glow rather than one or the other.
    pub fn lamp_css(&self) -> Option<String> {
        self.lamp.map(|l| lamp_layers(l, l.color))
    }
}

/// A lamp's wash and its hotspot as one background, in `color`.
///
/// The hotspot is listed first because CSS paints the first background layer
/// on top — the spot has to sit *inside* the wash, not under it.
pub fn lamp_layers(lamp: Lamp, color: &str) -> String {
    let wash = format!(
        "radial-gradient(ellipse at {:.0}% {:.0}%, {color} 0%, rgba(0,0,0,0) {:.0}%)",
        lamp.x, lamp.y, lamp.reach,
    );
    match lamp.core {
        Some(c) => format!(
            "radial-gradient(ellipse at {:.0}% {:.0}%, {} 0%, rgba(0,0,0,0) {:.0}%), {wash}",
            c.x, c.y, c.color, c.reach,
        ),
        None => wash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::vu::VuFace;

    /// Every face resolves to a spec, and every spec is a meter you could
    /// actually read: something printed on it, a needle wide enough to see,
    /// a hub at the pivot.
    #[test]
    fn every_face_in_the_kit_is_well_formed() {
        for face in VuFace::ALL {
            let spec = face.spec();
            let name = format!("{face:?}");

            assert!(!spec.card.is_empty(), "{name} has no card");
            assert!(!spec.ink.is_empty(), "{name} prints in nothing");
            assert!(
                spec.needle.width > 0.0,
                "{name}'s needle has no width, so it cannot be read",
            );
            assert!(spec.needle.hub_r > 0.0, "{name} has no hub");
            assert!(
                (0.0..=1.0).contains(&spec.needle.hub_opacity),
                "{name}'s hub opacity is not an opacity",
            );
            if let Some(l) = spec.lamp {
                // The bulb may sit a little outside the card — a backlit
                // movement's is behind the *pivot*, which is below the card's
                // bottom edge, and that is exactly what makes the light pool
                // under the needle instead of washing the whole face. What it
                // may not do is sit somewhere that lights nothing.
                assert!(
                    (-25.0..=125.0).contains(&l.x) && (-25.0..=125.0).contains(&l.y),
                    "{name}'s bulb at ({}, {}) is nowhere near the card",
                    l.x,
                    l.y,
                );
                assert!(l.reach > 0.0, "{name}'s lamp reaches nowhere");
            }
        }
    }

    /// The needle has to contrast with the card, or the meter is unreadable —
    /// which is the one way a colour scheme can be *wrong* rather than merely
    /// not to taste. Checked crudely, on the leading hex of each.
    #[test]
    fn a_needle_never_matches_its_own_card() {
        for face in VuFace::ALL {
            let spec = face.spec();
            assert!(
                !spec.card.contains(spec.needle.color),
                "{face:?}'s needle is the same colour as its card",
            );
        }
    }

    /// A sunken bezel is lit from above: the top face is the darkest of the
    /// four and the bottom the lightest. Inverting them is the difference
    /// between an opening and a boss, and it is invisible in code.
    #[test]
    fn a_bezels_opening_reads_as_sunken_rather_than_raised() {
        for face in VuFace::ALL {
            let b = face.spec().bezel;
            let lum = |c: &str| -> u32 {
                u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0)
            };
            assert!(
                lum(b.top) < lum(b.bottom),
                "{face:?}'s bezel is lit like a raised boss, not an opening",
            );
            assert!(
                lum(b.left) < lum(b.right),
                "{face:?}'s bezel is lit from the wrong side",
            );
            assert!(b.depth > 0.0, "{face:?}'s bezel has no depth");
        }
    }

    /// The louvre's stripes are in design px, so they widen with the panel.
    /// A frozen gradient string would not, and at three times the size it
    /// reads as a smear.
    #[test]
    fn a_vent_scales_with_the_panel() {
        let v = Vent {
            dark: "#000",
            light: "#2a2c2e",
            pitch: 4.0,
        };
        assert!(v.css(1.0).contains("2.00px"), "{}", v.css(1.0));
        assert!(v.css(1.0).contains("4.00px"));
        assert!(v.css(3.0).contains("6.00px"), "{}", v.css(3.0));
        assert!(v.css(3.0).contains("12.00px"));
    }

    /// A halo is thrown outward, and its bright core is tighter than its
    /// reach — inverted, the meter looks fogged rather than lit.
    #[test]
    fn a_glow_throws_a_tight_core_inside_a_wider_halo() {
        let g = Glow {
            color: "rgba(255,190,90,0.55)",
            spread: 20.0,
            core: Some("rgba(255,220,150,0.40)"),
        };
        let css = g.css(1.0);
        assert!(css.contains("20.0px"), "{css}");
        assert!(
            css.contains("7.0px"),
            "the core is not tighter than the halo: {css}"
        );
        // And it scales with the panel like everything else.
        assert!(g.css(2.0).contains("40.0px"));

        let plain = Glow { core: None, ..g };
        assert_eq!(
            plain.css(1.0).matches("0 0").count(),
            1,
            "a coreless glow has one layer"
        );
    }

    /// The frame's lip shadows the card from above and from one side, never
    /// evenly — an even inset reads as a vignette, not as a cast shadow, and
    /// loses the direction the whole panel is lit from.
    #[test]
    fn a_frames_shadow_falls_from_above_and_from_one_side() {
        let sh = Shade {
            color: "rgba(0,0,0,0.45)",
            top: 7.0,
            side: 4.0,
        };
        let css = sh.css(1.0);
        assert_eq!(css.matches("inset").count(), 2, "{css}");
        assert!(
            css.contains("inset 0 7.0px"),
            "no shadow from the top lip: {css}"
        );
        assert!(
            css.contains("inset 4.0px 0"),
            "no shadow from the side lip: {css}"
        );
        // And it scales with the panel.
        assert!(sh.css(2.0).contains("inset 0 14.0px"));
    }

    /// A bulb's hotspot sits *inside* its wash: tighter, and painted over it.
    #[test]
    fn a_hotspot_is_tighter_than_the_wash_it_sits_in() {
        for face in VuFace::ALL {
            let Some(lamp) = face.spec().lamp else {
                continue;
            };
            let Some(core) = lamp.core else { continue };
            assert!(
                core.reach < lamp.reach,
                "{face:?}'s hotspot reaches further than its own wash",
            );
        }
        let lit = Lamp {
            color: "rgba(1,2,3,0.4)",
            x: 50.0,
            y: 8.0,
            reach: 68.0,
            core: Some(Core {
                color: "rgba(9,9,9,0.5)",
                x: 50.0,
                y: 52.0,
                reach: 26.0,
            }),
        };
        let css = lamp_layers(lit, lit.color);
        // CSS paints the first layer on top, so the spot must be listed first
        // or the wash covers it.
        assert!(
            css.find("rgba(9,9,9,0.5)") < css.find("rgba(1,2,3,0.4)"),
            "the hotspot is painted under its own wash: {css}",
        );
    }

    #[test]
    fn a_lamp_becomes_a_gradient_and_an_unlit_face_does_not() {
        let lit = VuSpec {
            lamp: Some(Lamp {
                color: "rgba(1,2,3,0.5)",
                x: 50.0,
                y: 8.0,
                reach: 68.0,
                core: None,
            }),
            ..*VuFace::Amber.spec()
        };
        let css = lit.lamp_css().expect("a lit face has a lamp");
        assert!(css.contains("50% 8%"), "the bulb moved: {css}");
        assert!(css.contains("68%"), "the reach was dropped: {css}");

        let dark = VuSpec {
            lamp: None,
            ..*VuFace::Amber.spec()
        };
        assert!(dark.lamp_css().is_none());
    }
}
