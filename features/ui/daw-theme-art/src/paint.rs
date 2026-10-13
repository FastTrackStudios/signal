//! Controls as SHAPES, so a third surface can draw them.
//!
//! The crate's premise is that artwork is authored once and consumed
//! twice — live SVG on the web, rasterised PNG for REAPER. Both of those
//! consume *markup*, which was fine while both were SVG consumers.
//!
//! A GPU canvas is not. `anyrender` wants fills and strokes of shapes,
//! and it cannot be handed an `<svg>` string. Drawing the controls a
//! second time for it is exactly the drift this crate exists to prevent:
//! the numbers here were measured off REAPER pixel by pixel — a knob's
//! 11 rim, its 3.5 ring and its 7.5 body — and a canvas that re-invented
//! them would be a second, worse set of the same controls.
//!
//! So the geometry moves one layer down, into data:
//!
//! ```text
//!              paint::volume_knob(value, at) -> Drawing
//!                          │
//!            ┌─────────────┼──────────────┐
//!            ▼             ▼              ▼
//!        to SVG        to anyrender    rasterised
//!      (web, REAPER)   (GPU canvas)     via SVG
//! ```
//!
//! A [`Drawing`] is pure data: no Dioxus, no kurbo, no colour library
//! beyond the theme's own. Coordinates are `f64` — SVG writes them into
//! strings and takes any precision, while a GPU canvas is `f64`
//! throughout, so `f64` is the one choice that makes neither consumer
//! convert on the way in. That is deliberate — the crate is consumed by
//! the PNG rasteriser, which should not grow a GPU stack, and by the
//! canvas, which should not grow a DOM.
//!
//! # Coordinates
//!
//! Every drawing is in its own box, origin at the top-left, sized by
//! [`Drawing::w`] / [`Drawing::h`]. Placing it on a row is the caller's
//! transform. Angles are **degrees clockwise from twelve o'clock**,
//! which is the convention the knobs were measured in.

use daw_theme::Color;

/// How a shape is painted.
#[derive(Clone, PartialEq, Debug)]
pub enum Brush {
    Solid(Color),
    /// Axis-aligned linear gradient, in the drawing's own coordinates.
    Linear {
        from: (f64, f64),
        to: (f64, f64),
        /// Offsets are `f32` because every gradient API downstream —
        /// SVG's `offset`, peniko's `ColorStop` — takes one.
        stops: Vec<(f32, Color)>,
    },
    /// Radial gradient centred in the drawing's own coordinates.
    Radial {
        centre: (f64, f64),
        radius: f64,
        /// Offsets are `f32` because every gradient API downstream —
        /// SVG's `offset`, peniko's `ColorStop` — takes one.
        stops: Vec<(f32, Color)>,
    },
}

/// A shape, in the drawing's own coordinates.
///
/// Deliberately a small closed set rather than a path language. Every
/// control in this crate is built from these five, and a shape nobody
/// draws is a shape nobody has to port to a new backend.
#[derive(Clone, PartialEq, Debug)]
pub enum Shape {
    /// `r` is the corner radius; zero for a square corner.
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        r: f64,
    },
    Ellipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
    },
    /// An arc of a circle, for the knobs' rings.
    Arc {
        cx: f64,
        cy: f64,
        r: f64,
        /// Degrees clockwise from twelve o'clock.
        start: f64,
        sweep: f64,
    },
    Line {
        from: (f64, f64),
        to: (f64, f64),
    },
    /// A closed polygon — carets, triangles, the folder arrow.
    Poly(Vec<(f64, f64)>),
}

/// How a stroke is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stroke {
    pub width: f64,
    /// Round caps on an arc read as a soft ring; butt caps as a gauge.
    /// REAPER's knob uses butt, which is why it is the default.
    pub round_cap: bool,
}

impl Stroke {
    #[must_use]
    pub const fn new(width: f64) -> Self {
        Self {
            width,
            round_cap: false,
        }
    }

    #[must_use]
    pub const fn round(width: f64) -> Self {
        Self {
            width,
            round_cap: true,
        }
    }
}

/// Where a text run sits horizontally within its box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Centre,
}

/// One drawing operation.
#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    Fill(Shape, Brush),
    Stroke(Shape, Brush, Stroke),
    /// A label. The backend owns the font — this says what to draw and
    /// where the baseline is, not which face to use, because the DOM and
    /// the canvas resolve fonts completely differently.
    Text {
        body: String,
        x: f64,
        baseline: f64,
        /// In points, `f32` because that is what every text API takes.
        size: f32,
        color: Color,
        align: Align,
    },
}

/// A control, drawn.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Drawing {
    pub w: f64,
    pub h: f64,
    pub ops: Vec<Op>,
}

impl Drawing {
    #[must_use]
    pub const fn new(w: f64, h: f64) -> Self {
        Self {
            w,
            h,
            ops: Vec::new(),
        }
    }

    pub fn fill(&mut self, shape: Shape, brush: impl Into<Brush>) -> &mut Self {
        self.ops.push(Op::Fill(shape, brush.into()));
        self
    }

    pub fn stroke(&mut self, shape: Shape, brush: impl Into<Brush>, stroke: Stroke) -> &mut Self {
        self.ops.push(Op::Stroke(shape, brush.into(), stroke));
        self
    }

    pub fn text(
        &mut self,
        body: impl Into<String>,
        x: f64,
        baseline: f64,
        size: f32,
        color: Color,
        align: Align,
    ) -> &mut Self {
        self.ops.push(Op::Text {
            body: body.into(),
            x,
            baseline,
            size,
            color,
            align,
        });
        self
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

/// Parse one of the theme's hex constants.
///
/// The measured colours are stated as hex in `daw_theme::defaults`
/// because that is how they were sampled out of REAPER's images. A
/// malformed one is a typo in a constant, not a runtime condition, so it
/// falls back to black rather than returning a `Result` nobody could act
/// on.
#[must_use]
pub fn hex(text: &str) -> Color {
    let digits = text.trim_start_matches('#');
    let byte = |i: usize| {
        digits
            .get(i..i.saturating_add(2))
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .unwrap_or(0)
    };
    Color {
        r: byte(0),
        g: byte(2),
        b: byte(4),
        a: 255,
    }
}

/// The track panel's controls, as drawings.
///
/// Every number in here came out of the corresponding component in
/// [`crate::vector_controls`], which got it by measuring REAPER. Nothing
/// is re-derived and nothing is rounded on the way through: a knob's rim
/// is 11 here because it is 11 there, and if REAPER is re-measured both
/// change together.
pub mod tcp {
    use super::{Align, Brush, Drawing, Shape, Stroke};
    use crate::vector_controls::{Interaction, VOLUME_KNOB_START, VOLUME_KNOB_SWEEP, ink_in};
    use daw_theme::{Chrome, Color};

    /// The volume knob: a rim, a track, a value arc and a moulded cap.
    ///
    /// `value` is 0..1 across the knob's 320° sweep.
    ///
    /// The detail matters more than it looks. REAPER's knob is not a
    /// circle with a pointer — it is ringed in near-black, casts a soft
    /// drop below itself, and its ring runs from the lit token at the
    /// bottom to a darker one at the top because this theme's hardware
    /// is lit from below. Drawn flat it reads as a printed circle.
    #[must_use]
    pub fn volume_knob(
        chrome: &Chrome,
        lit: Color,
        value: f64,
        at: Interaction,
        drawn_px: f64,
    ) -> Drawing {
        // 24, the height of the field it straddles. A knob shorter than
        // its box reads as sunk into it rather than seated on its edge.
        let (w, h) = (24.0_f64, 24.0_f64);
        let (cx, cy) = (w * 0.5, h * 0.5);
        // Measured across the knob's centre row and down its centre
        // column: an 11 rim, a 3.5 ring on a radius of 9, a 7.5 body.
        let (rim, r, stroke, body) = (11.0_f64, 9.0_f64, 3.5_f64, 7.5_f64);

        let value = value.clamp(0.0, 1.0);
        let ink = ink_in(chrome, None, at, true, 0.35);
        let mut drawing = Drawing::new(w, h);

        // Ornament the eye cannot resolve, dropped.
        //
        // The drop shadow and the two gradients are what make this read
        // as hardware rather than as a filled circle — at full size. On
        // a fourteen-pixel track they are a radial gradient and two
        // linear ones per row, a hundred rows to a screen, for shading
        // spread across four pixels. `drawn_px` is how tall the caller
        // will actually draw this, so the control can decide rather than
        // the caller reaching inside it.
        let ornament = drawn_px >= 18.0;

        if ornament {
            // The drop it casts, which is most of what makes it read as
            // hardware rather than as a filled circle.
            drawing.fill(
                Shape::Ellipse {
                    cx,
                    cy: h.mul_add(0.055, cy),
                    rx: rim * 1.07,
                    ry: rim * 1.23,
                },
                Brush::Radial {
                    centre: (cx, h.mul_add(0.055, cy)),
                    radius: rim * 1.23,
                    stops: vec![
                        (
                            0.90_f32,
                            Color {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 38,
                            },
                        ),
                        (
                            1.0,
                            Color {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 0,
                            },
                        ),
                    ],
                },
            );
        }
        // The black outline the ring is inset into. Without it the ring
        // runs to the cell's edge and the knob reads a size larger than
        // the one beside it.
        // The rim is near-black in the source. Taken off the theme's own
        // edge rather than stated, so a light theme gets a rim and not a
        // hole punched in it.
        drawing.fill(circle(cx, cy, rim), chrome.hardware_edge.shade(-0.62));
        // The unlit track, all the way round.
        drawing.stroke(
            Shape::Arc {
                cx,
                cy,
                r,
                start: f64::from(VOLUME_KNOB_START),
                sweep: f64::from(VOLUME_KNOB_SWEEP),
            },
            // The track the value is read against: the theme's sunken
            // surface, which is what a groove is everywhere else.
            chrome.surface_sunken.shade(0.10),
            Stroke::new(stroke),
        );
        // The body: #303030 at the top to #2d2d2d at the bottom. Three
        // units, which is the difference between a moulded cap and a
        // filled circle — and nothing at all once the cap is six pixels
        // across, so it flattens to the mean there.
        if ornament {
            drawing.fill(
                circle(cx, cy, body),
                Brush::Linear {
                    from: (cx, cy - body),
                    to: (cx, cy + body),
                    // Three units of difference top to bottom, which is
                    // the difference between a moulded cap and a filled
                    // circle — as a RELATION on the theme's hardware,
                    // not as the two greys the source happens to use.
                    // `ink.face` rather than `chrome.hardware`: they
                    // are the same colour at rest — `ink_in` takes the
                    // hardware grey when nothing lights the control —
                    // and differ only under the pointer, which is the
                    // whole reason this function is handed an
                    // `Interaction` at all. Reaching past it for the
                    // raw grey made the knob the one control in the
                    // panel that did not answer the hand.
                    stops: vec![(0.0, ink.face.shade(0.04)), (1.0, ink.face.shade(-0.04))],
                },
            );
        } else {
            drawing.fill(circle(cx, cy, body), ink.face);
        }
        drawing.stroke(circle(cx, cy, body), ink.border, Stroke::new(0.8));
        // The value, over the track. Last, so its end sits on top of the
        // unlit stroke rather than under it.
        if value > 0.0 {
            // Flat, and the same blue the fader fills with.
            //
            // The ring used to run from the lit token at the bottom to a
            // darker one at the top, which is how REAPER's is shaded —
            // and at a 40 degree arc, which is what a quiet track shows,
            // the whole arc sat in the dark half and the knob read as
            // having no value at all. A track's level has to be legible
            // from the same colour whether its row is tall enough for a
            // knob or short enough for a bar.
            let lit: Brush = lit.into();
            drawing.stroke(
                Shape::Arc {
                    cx,
                    cy,
                    r,
                    start: f64::from(VOLUME_KNOB_START),
                    sweep: f64::from(VOLUME_KNOB_SWEEP) * value,
                },
                lit,
                Stroke::new(stroke),
            );
        }
        drawing
    }

    /// The fader cap, traced off `mcp_volthumb` row by row.
    ///
    /// ```text
    ///     y5      #0e0e0e   top border
    ///     y6-7    #696969   bevel, catching the light
    ///     y8-12   #414141   body above the grip
    ///     y13-39  ribs, alternating light and dark every row and
    ///             brightening downward
    ///     y40-46  #2b2b2b   body below
    ///     y47     #0b0b0b   bottom border
    ///     y48-52  a soft drop shadow, fading to nothing
    /// ```
    ///
    /// The grip is NOT a comb of full-width bands: it is a light panel
    /// spanning x7..x17 with short centre notches at x10..x14, so three
    /// columns of silver run unbroken down each side, and one full-width
    /// dark row halfway down is the seam between the two halves. Drawn as
    /// full-width grooves it flattens into a grille and loses both the
    /// side rails and the seam — which, with the border and the bevel, is
    /// most of what makes the cap read as an object at all.
    #[must_use]
    pub fn fader_cap(chrome: &Chrome, grip: Color) -> Drawing {
        fader_cap_through(chrome, grip, 0.0)
    }

    /// The same cap with a WINDOW cut in it: solid ring, open middle.
    ///
    /// `through` opens the middle, 0 for the solid cap above and 1 for
    /// a pane with nothing behind it. It exists because the fader's
    /// groove became the meter: a solid cap parked over a meter is a
    /// hole in the level at exactly the height you are reading.
    ///
    /// CUT, not tinted. A translucent fill over the moulding is still
    /// the moulding — the ring came out looking like glass too, which
    /// is the opposite of the point. So the body is drawn as four
    /// bands AROUND the window and the window is simply never painted:
    /// what shows through it is whatever the caller drew underneath.
    /// The ring stays solid plastic, which is what makes it read as
    /// something you can take hold of.
    #[must_use]
    pub fn fader_cap_through(chrome: &Chrome, grip: Color, through: f64) -> Drawing {
        let (vw, vh) = (27.0, 53.0);
        let body = chrome.hardware;
        let edge = chrome.hardware_edge.shade(-0.35);
        let open = through.clamp(0.0, 1.0) > 0.01;

        // Fractions of the cell, all measured. x2..x22 INCLUSIVE — the
        // border pixel at x22 is part of the cap — so the right edge is
        // at 23, not 21.
        let (x0, x1) = (vw * 2.0 / 27.0, vw * 23.0 / 27.0);
        let (top, bot) = (vh * 5.0 / 53.0, vh * 48.0 / 53.0);
        let (gx0, gw) = (vw * CAP_PANE_X, vw * CAP_PANE_W);
        let (gy0, gy1) = (vh * CAP_PANE_Y0, vh * CAP_PANE_Y1);

        let mut drawing = Drawing::new(vw, vh);
        // The shadow it casts.
        drawing.fill(
            rect(x0 + 1.0, bot - 1.0, x1 - x0 - 2.0, vh - bot - 1.0, vw * 0.1),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 51,
            },
        );
        // The border, drawn as a fill beneath the face so the face
        // cannot bleed past the frame.
        drawing.fill(rect(x0, top, x1 - x0, bot - top, vw * 0.16), edge);

        let face = |from: f64, to: f64| Brush::Linear {
            from: (0.0, from),
            to: (0.0, to),
            stops: vec![
                (0.0, body.shade(0.06)),
                (0.65, body.shade(-0.02)),
                (1.0, body.shade(-0.32)),
            ],
        };
        let (fx0, fy0) = (x0 + 1.0, top + 1.0);
        let (fx1, fy1) = (x1 - 1.0, bot - 1.0);
        if open {
            // The four bands of plastic around the pane. The gradient
            // is handed the WHOLE face's span in each of them, so the
            // ring shades as one moulded piece rather than as four
            // strips that each start over.
            let brush = face(fy0, fy1);
            drawing.fill(
                rect(fx0, fy0, fx1 - fx0, gy0 - fy0, vw * 0.13),
                brush.clone(),
            );
            drawing.fill(
                rect(fx0, gy1, fx1 - fx0, fy1 - gy1, vw * 0.13),
                brush.clone(),
            );
            drawing.fill(rect(fx0, gy0, gx0 - fx0, gy1 - gy0, 0.0), brush.clone());
            drawing.fill(rect(gx0 + gw, gy0, fx1 - (gx0 + gw), gy1 - gy0, 0.0), brush);
            // The pane's own rim, so it reads as set INTO the ring
            // rather than as a gap where the plastic ran out.
            for (rx, rw) in [(gx0 - 1.0, 1.0), (gx0 + gw, 1.0)] {
                drawing.fill(
                    rect(rx, gy0 - 1.0, rw, gy1 - gy0 + 2.0, 0.0),
                    body.shade(-0.5),
                );
            }
            for (ry, rh) in [(gy0 - 1.0, 1.0), (gy1, 1.0)] {
                drawing.fill(rect(gx0 - 1.0, ry, gw + 2.0, rh, 0.0), body.shade(-0.5));
            }
        } else {
            drawing.fill(
                rect(fx0, fy0, fx1 - fx0, fy1 - fy0, vw * 0.13),
                face(top, bot),
            );
        }
        // The lit bevel across the top — two rows, the brighter above.
        drawing.fill(
            rect(x0 + 2.0, top + 1.0, x1 - x0 - 4.0, 1.0, 0.0),
            body.shade(0.28),
        );
        drawing.fill(
            rect(x0 + 2.0, top + 2.0, x1 - x0 - 4.0, 1.0, 0.0),
            body.shade(0.16),
        );
        if !open {
            // The grip sits in a recess, so a ring of shadow runs round
            // it. Without it the panel looks stuck on the front rather
            // than set into the moulding.
            drawing.fill(
                rect(gx0 - 1.0, gy0 - 1.0, gw + 2.0, gy1 - gy0 + 2.0, vw * 0.09),
                body.shade(-0.43),
            );
            drawing.fill(
                rect(gx0, gy0, gw, gy1 - gy0, vw * 0.055),
                Brush::Linear {
                    from: (0.0, gy0),
                    to: (0.0, gy1),
                    stops: vec![(0.0, grip.shade(-0.03)), (1.0, grip.shade(0.34))],
                },
            );
        }
        // Five notches, the seam, five more. Drawn across the pane too:
        // they are what says "grip", and a bare window would have
        // nothing left to take hold of.
        for i in 0..11_u32 {
            let step = f32::from(u16::try_from(i).unwrap_or(0));
            let y = gy0 + vh * 2.0_f64.mul_add(f64::from(step), 3.0) / 53.0;
            let seam = i == 5;
            // The seam is a good deal darker than the notches — 51
            // against their 85 and 96 — so it reads as the join between
            // two halves rather than one more groove.
            let ink_line = if seam {
                grip.shade(-0.69)
            } else {
                grip.shade(0.12_f32.mul_add(step / 10.0, -0.52))
            };
            let (nx, nw) = if seam {
                (gx0, gw)
            } else {
                (gw.mul_add(0.27, gx0), gw * 0.46)
            };
            drawing.fill(rect(nx, y, nw, vh / 53.0, 0.0), ink_line);
        }
        drawing
    }

    /// The mixer's fader groove, as the METER.
    ///
    /// The groove used to be lit to the fader's own value, which was
    /// the one thing on the strip you could already read from the cap
    /// sitting on top of it. So it carries the signal instead: this is
    /// the meter, and there is no separate one. A strip too narrow for
    /// a fader beside a meter — which is most of them — gets both,
    /// because they are the same column.
    ///
    /// Wider than the old 35% groove, because a meter is read at a
    /// glance and a seven-pixel one is a line. Still narrower than the
    /// cap, so the cap reads as riding ON it rather than as part of it.
    ///
    /// Two channels, split down the middle by a hairline. A summed
    /// meter cannot tell you that a stereo source has collapsed to one
    /// side, or that one leg of a pair is dead — which are two of the
    /// things you look at a meter to catch.
    #[must_use]
    pub fn fader_track(
        chrome: &Chrome,
        level: (f64, f64),
        hold: (f64, f64),
        clipped: bool,
        zones: [Color; 3],
        w: f64,
        h: f64,
    ) -> Drawing {
        let groove = groove_w(w);
        let groove_x = (w - groove) / 2.0;
        let mut drawing = Drawing::new(w, h);
        let span = (h - 1.0).max(0.0);
        // The groove, and nothing else behind the meter. It used to be
        // a well the full width of both channels, which blacked out the
        // column and made the cap hard to pick out against it — REAPER
        // runs the fader on a thin line, and the level reads better
        // growing out of the strip than filling a trough. The line also
        // does the work the seam used to: it is what the two channels
        // sit either side of.
        drawing.fill(
            rect(divider_x(groove_x, groove), 0.5, SEAM, span, 0.0),
            chrome.surface_sunken.shade(-0.5),
        );
        for (at, wide, value) in channels(groove_x, groove, level) {
            let lit = span * value.clamp(0.0, 1.0);
            if lit > 0.5 {
                // Square, not rounded: the top of the bar IS the
                // reading, and a domed end puts its own apex a pixel or
                // two above the level it is reporting.
                drawing.fill(
                    rect(at, h - 0.5 - lit, wide, lit, 0.0),
                    level_brush(zones, h),
                );
            }
        }
        // The peak hold, one line per channel. The bar is an
        // instantaneous reading sampled thirty times a second, which
        // simply cannot show a transient — and on drums the transient
        // is the whole question. The line is where the signal HAS
        // been, which is what "did that hit" actually asks.
        for (at, wide, value) in channels(groove_x, groove, hold) {
            if value <= 0.001 {
                continue;
            }
            let y = span.mul_add(-value.clamp(0.0, 1.0), h - 0.5);
            drawing.fill(
                rect(at, (y - HOLD / 2.0).max(0.5), wide, HOLD, 0.0),
                zone_at(zones, value),
            );
        }
        // The groove again, over the bars, so a full-scale signal does
        // not swallow the line the cap rides on.
        drawing.fill(
            rect(divider_x(groove_x, groove), 0.5, SEAM, span, 0.0),
            chrome.surface_sunken.shade(-0.5).with_alpha(210),
        );
        // The clip latch. A peak-hold decays, which is right for
        // reading a level and wrong for reporting a fault: the whole
        // value of "this clipped" is that it is still saying so when
        // you look up. So it stays until it is cleared, across the
        // whole column so it cannot be mistaken for a channel's level.
        if clipped {
            drawing.fill(
                rect(groove_x, 0.5, groove, CLIP_H, CLIP_H / 3.0),
                over(zones[2]),
            );
        }
        drawing
    }

    /// The pane cut in the cap, as fractions of the cap's own cell.
    ///
    /// Public because two drawings have to agree about it: the cap
    /// leaves this rectangle unpainted, and the level drawn over the
    /// cap has to land INSIDE it. They disagreed at first — the level
    /// was drawn over a taller, wider box than the hole — so it painted
    /// across the plastic ring as well, which made the whole cap look
    /// transparent however solid the ring was.
    pub const CAP_PANE_X: f64 = 7.0 / 27.0;
    pub const CAP_PANE_W: f64 = 11.0 / 27.0;
    pub const CAP_PANE_Y0: f64 = 13.0 / 53.0;
    pub const CAP_PANE_Y1: f64 = 40.0 / 53.0;

    /// The rule that says a strip is the selected one.
    ///
    /// A line along the top of the name band, not a panel behind it.
    /// The name has no plate — it is text on the strip, the way a name
    /// written on tape is — and a box drawn permanently behind it was
    /// only ever a box. The one time a field belongs there is while it
    /// is being TYPED in, and the rename editor draws its own.
    ///
    /// Along the top edge because that is where the band borders
    /// everything else on the strip: a line there marks the whole strip
    /// above it rather than just the label, which is what selection
    /// means.
    #[must_use]
    pub fn name_selected(accent: Color, w: f64, h: f64) -> Drawing {
        let mut drawing = Drawing::new(w, h);
        drawing.fill(rect(0.0, 0.0, w, 2.0, 0.0), accent);
        drawing
    }

    /// How tall the clip latch sits at the top of the column.
    ///
    /// Also its target: while it is lit it is what a click there
    /// clears, and while it is not there is nothing to hit, so the
    /// fader behaves exactly as it would without it.
    pub const CLIP_H: f64 = 5.0;

    /// The two channel columns inside a groove: where each starts, how
    /// wide it is, and what it reads.
    ///
    /// One function because the meter, the part of it seen through the
    /// cap and anything else measuring against a channel have to agree
    /// about where the seam falls.
    fn channels(groove_x: f64, groove: f64, level: (f64, f64)) -> [(f64, f64, f64); 2] {
        let each = ((groove - SEAM) / 2.0).max(1.0);
        [
            (groove_x, each, level.0),
            (divider_x(groove_x, groove) + SEAM, each, level.1),
        ]
    }

    fn divider_x(groove_x: f64, groove: f64) -> f64 {
        groove_x + ((groove - SEAM) / 2.0).max(1.0)
    }

    /// How wide the line between the two channels is.
    const SEAM: f64 = 1.0;

    /// And how thick a peak-hold line is. Two pixels: one disappears
    /// against the bar below it, three reads as a level of its own.
    const HOLD: f64 = 2.0;

    /// The lit level where it passes BEHIND the cap, drawn over it.
    ///
    /// The cap is glass, but glass over a dark moulding is still a
    /// smudge: the level went in at the bottom of the cap and came out
    /// at the top with a muddy gap between, which is a gap at exactly
    /// the height you are reading. This is that segment, drawn again on
    /// top at an alpha that reads as "through something" — so the level
    /// is one continuous column and the cap is a frame over it.
    ///
    /// Only the overlap, so the rest of the meter is not lit twice.
    #[must_use]
    pub fn fader_through(
        level: (f64, f64),
        zones: [Color; 3],
        w: f64,
        h: f64,
        cap_y: f64,
        cap_h: f64,
    ) -> Drawing {
        let groove = groove_w(w);
        let groove_x = (w - groove) / 2.0;
        let mut drawing = Drawing::new(w, h);
        // The PANE, exactly — not the cap, and not a guess at it. The
        // level shows through the hole in the moulding and nowhere
        // else; painted over the ring as well it made a solid cap look
        // like a transparent one.
        let (from, to) = (
            cap_y + cap_h * CAP_PANE_Y0,
            (cap_y + cap_h * CAP_PANE_Y1).min(h - 0.5),
        );
        // And the pane is narrower than the groove, so a channel is
        // clipped to it rather than running out past the ring.
        let cap_left = (w - cap_w(w)) / 2.0;
        let pane_x0 = cap_left + cap_w(w) * CAP_PANE_X;
        let pane_x1 = pane_x0 + cap_w(w) * CAP_PANE_W;
        let span = (h - 1.0).max(0.0);
        let seen = |color: Color| color.with_alpha(THROUGH_CAP);
        for (at, wide, value) in channels(groove_x, groove, level) {
            let lit_top = span.mul_add(-value.clamp(0.0, 1.0), h - 0.5);
            let top = lit_top.max(from);
            let (left, right) = (at.max(pane_x0), (at + wide).min(pane_x1));
            if to - top < 0.5 || right - left < 0.5 {
                continue;
            }
            drawing.fill(
                rect(left, top, right - left, to - top, 0.0),
                Brush::Linear {
                    from: (0.0, h),
                    to: (0.0, 0.0),
                    stops: vec![
                        (0.0, seen(zones[0])),
                        (0.75, seen(zones[1])),
                        (1.0, seen(zones[2])),
                    ],
                },
            );
        }
        drawing
    }

    /// How much of the level survives the cap.
    ///
    /// Enough to be one column with the rest of the meter, little
    /// enough that the cap still reads as in front of it.
    const THROUGH_CAP: u8 = 130;

    /// How wide the meter is in a fader column of width `w`.
    ///
    /// Thin, and well inside the cap. It was briefly wider than the cap
    /// so each channel would show down one side of it; that read as two
    /// fat bars with a handle lost in the middle, and the thing it was
    /// protecting — the level under the cap — is better served by the
    /// glass, which shows the whole column rather than its edges.
    ///
    /// Stated once, because the drawing, the part seen through the cap
    /// and anything else measuring against a channel have to agree.
    #[must_use]
    pub fn groove_w(w: f64) -> f64 {
        (w * 0.30).max(4.0)
    }

    /// And how wide the CAP is in the same column.
    ///
    /// Narrower than the meter by a few pixels each side. The cap keeps
    /// its authored 27:53 proportions — it is a moulded object and a
    /// stretched one stops looking like one — so this sets its height
    /// too, through [`fader_cap_at`].
    #[must_use]
    pub fn cap_w(w: f64) -> f64 {
        (w * 0.72).max(8.0)
    }

    /// The gradient a level is lit with, pinned to DECIBELS.
    ///
    /// Its stops sit where [`METER_ZONES`] says rather than at tidy
    /// fractions of the column, so the colour changes at the levels a
    /// mix is actually judged against. Each zone holds its colour over
    /// most of its span and turns over a short blend either side —
    /// enough to be recognised as a zone, not so hard that the meter
    /// reads as three lamps instead of a level.
    ///
    /// Shared, so nothing drawn into the column can disagree with it.
    fn level_brush(zones: [Color; 3], h: f64) -> Brush {
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 fraction")]
        let at = |db: f64| meter_norm(db) as f32;
        // Gradients run bottom to top, and a stop's offset is measured
        // from the `from` end — so the stops are 1 − the height.
        let blend = 0.035_f32;
        Brush::Linear {
            from: (0.0, h),
            to: (0.0, 0.0),
            stops: vec![
                (0.0, zones[0]),
                (at(METER_ZONES[0]) - blend, zones[0]),
                (at(METER_ZONES[0]) + blend, zones[1]),
                (at(METER_ZONES[1]) - blend, zones[1]),
                (at(METER_ZONES[1]) + blend, zones[2]),
                (at(METER_ZONES[2]) - blend, zones[2]),
                // Over. The only part of the column that is pure red,
                // and the cap cannot reach it.
                (at(METER_ZONES[2]), over(zones[2])),
                (1.0, over(zones[2])),
            ],
        }
    }

    /// The colour of the over-range band: the danger zone, pushed.
    fn over(danger: Color) -> Color {
        danger.shade(0.35)
    }

    /// The top of the fader's travel, in dB.
    ///
    /// Measured, not assumed. The dB labels down REAPER's mixer fader
    /// were read off a screenshot at x 9..24 and fitted: the mapping is
    /// LINEAR IN dB at 2.204 px/dB, and extrapolating it to 0 dB lands
    /// at y 124.88 against a groove that starts at 125. Every residual
    /// is under a pixel across the whole travel.
    ///
    /// That fit describes REAPER's own fader, which tops out at unity:
    /// there is no boost on it. Ours goes to +12, deliberately — a
    /// fader that cannot add gain makes you reach for a plugin to do
    /// the most ordinary thing in a mix, and the measurement is kept
    /// above because it is what the SCALE's spacing is derived from,
    /// not a limit we are bound by.
    pub const FADER_TOP_DB: f64 = 12.0;

    /// And the bottom of the travel.
    ///
    /// The same fit puts the groove's last pixel at −55.86 dB. Rounded
    /// to −56 rather than to a tidier −60, because −60 would put every
    /// label seven pixels off the ones REAPER draws — a tidy constant
    /// that is visibly wrong is worse than an untidy one that is right.
    pub const FADER_BOTTOM_DB: f64 = -56.0;

    /// Where a level sits on the fader, 0 at the bottom and 1 at the top.
    ///
    /// The one definition of the fader's scale: the cap, the lit groove
    /// and the dB labels beside it all come through here, so a label
    /// cannot drift from the position it labels.
    #[must_use]
    pub fn fader_norm(db: f64) -> f64 {
        let span = FADER_TOP_DB - FADER_BOTTOM_DB;
        ((db - FADER_BOTTOM_DB) / span).clamp(0.0, 1.0)
    }

    /// The level at a position on the fader. The inverse of
    /// [`fader_norm`], for hit testing and for dragging.
    #[must_use]
    pub fn fader_db(norm: f64) -> f64 {
        FADER_BOTTOM_DB + norm.clamp(0.0, 1.0) * (FADER_TOP_DB - FADER_BOTTOM_DB)
    }

    /// A linear gain as a position on the fader.
    ///
    /// `Track::volume` is a gain, not a dB value — 1.0 is unity — so
    /// this is the conversion the strip needs. Silence is a gain of
    /// zero, whose logarithm is not a number, so it is answered before
    /// the log rather than after it.
    #[must_use]
    pub fn gain_norm(gain: f64) -> f64 {
        if gain <= 0.0 {
            return 0.0;
        }
        fader_norm(20.0 * gain.log10())
    }

    /// The labels REAPER prints down its fader.
    ///
    /// Not a round series: REAPER steps by 12, starting at −6. Twelve dB
    /// is a doubling and a halving twice over, which is the interval a
    /// mixing decision is actually made in.
    /// The ceiling itself is NOT among them. A mark at the very top of
    /// a column sits half a glyph off the end of it and labels the one
    /// position you can find without a number — the fader will not go
    /// further, which the fader already tells you by stopping. +6 is
    /// the last mark; the travel still runs to [`FADER_TOP_DB`].
    pub const FADER_MARKS: [f64; 7] = [6.0, 0.0, -6.0, -18.0, -30.0, -42.0, -54.0];

    /// The meter's ceiling and floor, which are the fader's.
    ///
    /// One scale for the whole column, which is what lets one set of
    /// numbers label both the bar and the cap beside it. It also gives
    /// the meter what it most needs: room above 0 dBFS. A meter whose
    /// ceiling is unity pins there, so a clean −1 and a signal 5 dB
    /// into the red draw the identical full bar — the one distinction a
    /// meter exists to make.
    pub const METER_TOP_DB: f64 = FADER_TOP_DB;
    pub const METER_BOTTOM_DB: f64 = FADER_BOTTOM_DB;

    /// Where a level sits in the column, 0 at the floor and 1 at the
    /// top.
    ///
    /// The one scale the column is drawn to: the lit bars, the hold
    /// lines, the numbers beside them and the cap's own travel all come
    /// through here, so nothing in the column can disagree with the
    /// numbers labelling it.
    #[must_use]
    pub fn meter_norm(db: f64) -> f64 {
        fader_norm(db)
    }

    /// The decibels the meter's zones change colour at.
    ///
    /// Landmarks rather than a smooth ramp: −18 is where a track is
    /// sitting where it should, −6 is getting hot, 0 is over. A
    /// continuous green-to-red gradient makes every level a position to
    /// measure; these make the common ones a colour to recognise.
    pub const METER_ZONES: [f64; 3] = [-18.0, -6.0, 0.0];

    /// The dB scale beside the fader.
    ///
    /// `h` is the travel the fader runs in, so the marks land on the
    /// positions [`fader_norm`] puts the cap at — the scale and the
    /// thing it measures cannot disagree.
    ///
    /// Without this the fader is a handle on an unmarked line: you can
    /// see that one track is louder than another and not by how much,
    /// which is most of what a mixer is for.
    /// The dB scale beside the fader, as a connected inscription.
    ///
    /// Each number gets a tick running from it to the meter's edge, so
    /// the two read as one instrument rather than as a column of
    /// figures that happens to sit near a column of light. A ruler's
    /// numbers are attached to its ticks; these were floating.
    ///
    /// And they LIGHT: a mark the signal has passed is drawn in the
    /// meter's own ink instead of the scale's. That turns reading a
    /// level from "measure the top of the bar against the numbers" into
    /// "read the last number that is lit", which is the thing the eye
    /// can actually do at a glance across forty strips.
    ///
    /// `level` is the louder channel, because the question a lit number
    /// answers is "has anything reached this", not "have both".
    #[must_use]
    pub fn fader_scale(
        w: f64,
        h: f64,
        ink: Color,
        zones: [Color; 3],
        size: f32,
        level: f64,
    ) -> Drawing {
        let mut drawing = Drawing::new(w, h);
        // Top down, so a mark that will not fit is dropped rather than
        // drawn over the one above it — the scale thins out on a short
        // strip instead of turning into a smear.
        let mut last = f64::NEG_INFINITY;
        for db in FADER_MARKS {
            let at = meter_norm(db);
            let y = h * (1.0 - at);
            if y - last < f64::from(size) + 2.0 {
                continue;
            }
            last = y;
            let passed = level >= at;
            // Lit in the colour the METER is at that height, not one
            // flat highlight: a lit −6 and a lit −42 are very different
            // news, and a scale that reported them the same colour
            // would be throwing away the thing the gradient exists to
            // say.
            let colour = if passed { zone_at(zones, at) } else { ink };
            // The tick, from the number's edge to the meter. Short and
            // dim: it is the join, not a mark to read against.
            drawing.fill(
                rect(w - TICK, y - 0.5, TICK, 1.0, 0.0),
                colour.with_alpha(if passed { 200 } else { 90 }),
            );
            // Baseline rather than centre: text sits ON the mark, the
            // way a ruler's numbers sit on its ticks.
            // Positives carry their sign, negatives do not: below zero
            // is what a meter's numbers mean unless they say otherwise,
            // and now that the scale runs above unity an unsigned "6"
            // would appear twice on the same column.
            drawing.text(
                if db > 0.0 {
                    format!("+{db:.0}")
                } else {
                    format!("{:.0}", db.abs())
                },
                (w - TICK - 2.0) / 2.0,
                y + f64::from(size) / 3.0,
                size,
                colour,
                Align::Centre,
            );
        }
        drawing
    }

    /// How far the tick reaches from the numbers towards the meter.
    const TICK: f64 = 5.0;

    /// The meter's colour at a height, on the same stops the gradient
    /// uses — so a lit mark is the colour the bar is where it passes.
    #[must_use]
    fn zone_at(zones: [Color; 3], at: f64) -> Color {
        let at = at.clamp(0.0, 1.0);
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 fraction")]
        if at <= 0.75 {
            zones[0].mix(zones[1], (at / 0.75) as f32)
        } else {
            zones[1].mix(zones[2], ((at - 0.75) / 0.25) as f32)
        }
    }

    /// Where the cap sits on a fader of this size.
    ///
    /// Returns its top and its height. The cap is its own drawing at its
    /// own cell size, so the caller places it the way the strip places
    /// every other control, and the fader draws only the groove it runs
    /// in — one definition of the cap, used at both ends.
    ///
    /// Measured from the TOP, because the cap's top edge is what moves:
    /// computed from the bottom it sat half a cap out at both ends of
    /// the travel.
    #[must_use]
    pub fn fader_cap_at(value: f64, w: f64, h: f64) -> (f64, f64) {
        let cap_h = (cap_w(w) * 53.0 / 27.0).min(h * 0.5);
        // On the METER's scale, not the column's full height: the
        // column now runs past unity so an over has somewhere to go,
        // and a cap that used the whole height would put its 0 dB at
        // the top of a scale whose top is +6. The cap simply never
        // enters the over-range band, which is what a fader that stops
        // at unity should look like.
        // The cap is CENTRED on the level it is set to, so its middle
        // and the meter beside it read against the same number.
        let at = value.clamp(0.0, 1.0);
        let centre = h * at;
        let top = (h - centre - cap_h / 2.0).clamp(0.0, (h - cap_h).max(0.0));
        (top, cap_h.max(1.0))
    }

    /// A level meter: a well, and however much of it is lit.
    ///
    /// The lit part runs safe to warn to danger up its own height, which
    /// is why it is a gradient rather than three thresholds — a meter
    /// that changed colour in steps reads as three states instead of as
    /// a level.
    #[must_use]
    pub fn meter(chrome: &Chrome, level: f64, zones: [Color; 3], w: f64, h: f64) -> Drawing {
        let level = level.clamp(0.0, 1.0);
        let mut drawing = Drawing::new(w, h);
        drawing.fill(rect(0.0, 0.0, w, h, 1.0), chrome.surface_sunken);
        let lit = h * level;
        if lit > 0.5 {
            drawing.fill(rect(0.0, h - lit, w, lit, 1.0), level_brush(zones, h));
        }
        drawing
    }

    /// The folder mark: a tab and a body.
    ///
    /// Traced off `track_folder_off.png`'s first mark — the tab is 4x2
    /// with a SQUARE right edge over a 9x5 body, one flat ink. The
    /// slanted edge this was first drawn with is not in the art.
    ///
    /// All right angles, so it is a polygon rather than a path: six
    /// points, and no curve for a backend to flatten.
    #[must_use]
    pub fn folder_mark(ink: Color) -> Drawing {
        let (w, h) = (9.0, 7.0);
        let mut drawing = Drawing::new(w, h);
        drawing.fill(
            Shape::Poly(vec![
                (0.0, 0.0),
                (4.0, 0.0),
                (4.0, 2.0),
                (9.0, 2.0),
                (9.0, 7.0),
                (0.0, 7.0),
            ]),
            ink,
        );
        drawing
    }

    /// Volume as a horizontal fader, for rows too short for a knob.
    ///
    /// A knob says its value with the angle of a ring, and an angle
    /// needs a circle big enough to have angles in it. Below about
    /// twenty pixels the ring is three pixels of arc and the value stops
    /// being readable at a glance — which is the height most tracks sit
    /// at once a session is collapsed to fit, so it is the height where
    /// seeing roughly how loud something is matters most.
    ///
    /// Horizontal, like [`pan_line`], because the axis being squashed is
    /// the vertical one: a bar whose LENGTH is its value keeps saying it
    /// at any height, where one whose height is its value has nothing
    /// left to say at three pixels. The two also read as a pair this
    /// way, which is what they are.
    #[must_use]
    pub fn volume_fader(chrome: &Chrome, lit: Color, value: f64) -> Drawing {
        // Quiet: a thin track, the level along it in the volume's
        // colour, muted, and a small thumb at the level — drawn at its
        // own height and centred in the row, never squashed to it. A
        // row too short for a knob is read in passing; a block of
        // colour shouted over the names.
        let (w, h) = (24.0, 8.0);
        let value = value.clamp(0.0, 1.0);
        let mut drawing = Drawing::new(w, h);
        drawing.fill(rect(0.0, 3.0, w, 2.0, 1.0), chrome.surface_sunken);
        let filled = w * value;
        if filled > 0.5 {
            drawing.fill(rect(0.0, 3.0, filled, 2.0, 1.0), lit.with_alpha(140));
        }
        drawing.fill(
            rect((filled - 3.0).clamp(0.0, w - 6.0), 1.0, 6.0, 6.0, 3.0),
            chrome.hardware_mark,
        );
        drawing
    }

    /// Pan as a line, for rows too short for a knob.
    ///
    /// Centre-marked, with the position as a tick along it. The same
    /// argument as [`volume_fader`]: a pointer needs a circle, a tick
    /// needs a line, and a line is still a line at two pixels.
    ///
    /// `ink` is pan's own colour and is not volume's — the two sit in
    /// neighbouring columns on every row, and telling them apart at a
    /// glance is worth more than either matching the accent.
    #[must_use]
    pub fn pan_line(chrome: &Chrome, position: f64, ink: Color) -> Drawing {
        // The volume's pair: a thin track, a faint centre mark, the pan
        // from the centre outward in its own colour, muted, and a small
        // thumb at the position — at its own height, centred in the row.
        let (w, h) = (24.0, 8.0);
        let position = position.clamp(-1.0, 1.0);
        let span = w - 4.0;
        let mut drawing = Drawing::new(w, h);
        drawing.fill(rect(0.0, 3.0, w, 2.0, 1.0), chrome.surface_sunken);
        drawing.fill(
            rect(w / 2.0 - 0.5, 1.0, 1.0, 6.0, 0.0),
            chrome.hardware_edge,
        );
        let to = (span / 2.0).mul_add(position, w / 2.0);
        let (from, to) = if to < w / 2.0 {
            (to, w / 2.0)
        } else {
            (w / 2.0, to)
        };
        if (to - from) > 0.5 {
            drawing.fill(rect(from, 3.0, to - from, 2.0, 1.0), ink.with_alpha(140));
        }
        drawing.fill(
            rect(
                (span / 2.0)
                    .mul_add(position, w / 2.0 - 3.0)
                    .clamp(0.0, w - 6.0),
                1.0,
                6.0,
                6.0,
                3.0,
            ),
            chrome.hardware_mark,
        );
        drawing
    }

    /// The pan knob: a rim, a face, a pointer and a cap.
    ///
    /// `position` is -1..1, and the pointer sweeps 135° either side of
    /// twelve o'clock — REAPER's range, not a full rotation.
    #[must_use]
    pub fn pan_knob(chrome: &Chrome, position: f64, ink: Color, at: Interaction) -> Drawing {
        let (w, h) = (24.0_f64, 25.0_f64);
        let (cx, cy, r) = (12.0_f64, 12.08_f64, 9.37_f64);
        let (cap_cy, cap_r) = (12.05_f64, 4.06_f64);
        // The ring the value fills, and the face inside it.
        let ring = 2.4_f64;
        let ring_r = r - ring / 2.0 - 0.35;
        let face_r = r - ring - 0.7;

        let pos = position.clamp(-1.0, 1.0);
        let sweep = pos * 135.0;
        let point_w = w * 0.083;
        // The pointer starts inside the face, not under the ring.
        let point_top = cy - r + ring + 1.4;
        let point_bot = h.mul_add(-0.01, cy - cap_r);

        let point = ink;
        let rim = chrome.hardware_edge.shade(-0.45);

        let mut drawing = Drawing::new(w, h);
        // Rim first, face inset — a stroke would centre itself on the
        // boundary and eat half of each.
        drawing.fill(circle(cx, cy, r), rim);
        // The track the value is read against, so an untouched knob
        // shows an empty ring rather than nothing at all.
        drawing.stroke(
            Shape::Arc {
                cx,
                cy,
                r: ring_r,
                start: -135.0,
                sweep: 270.0,
            },
            // Visible, not merely present: against the rim an unlit
            // track in the surface colour vanished, and the value arc
            // read as floating in a gap rather than filling a ring.
            chrome.hardware_edge,
            Stroke::new(ring),
        );
        // The value, filled from TWELVE O'CLOCK to the pointer rather
        // than from one end of the travel. Pan's zero is the centre, so
        // an arc growing out of the top says which way and how far at a
        // glance; one growing from hard left would make centre look like
        // half of something.
        if sweep.abs() > 0.5 {
            drawing.stroke(
                Shape::Arc {
                    cx,
                    cy,
                    r: ring_r,
                    start: 0.0,
                    sweep,
                },
                ink,
                Stroke::new(ring),
            );
        }
        // The face lifts under the pointer and sinks when held, the
        // same way the volume knob's does — at rest `ink_in` hands back
        // the hardware grey this used to state outright, so nothing
        // moves until a hand is on it.
        drawing.fill(
            circle(cx, cy, face_r),
            ink_in(chrome, None, at, true, 0.35).face,
        );
        // The pointer, as a rotated quad rather than a rotation applied
        // to the whole drawing: the canvas replays these under a
        // transform that may scale the axes differently, and a rotation
        // baked into the shape survives that where a rotated coordinate
        // system would shear.
        drawing.fill(
            Shape::Poly(
                [
                    (-point_w / 2.0, point_top - cy),
                    (point_w / 2.0, point_top - cy),
                    (point_w / 2.0, point_bot - cy),
                    (-point_w / 2.0, point_bot - cy),
                ]
                .into_iter()
                .map(|(px, py)| rotate(px, py, sweep, cx, cy))
                .collect(),
            ),
            point,
        );
        drawing.fill(circle(cx, cap_cy, cap_r), chrome.hardware_mark);
        drawing
    }

    /// A gutter button — mute or solo — lit in its own colour.
    #[must_use]
    pub fn gutter_button(
        chrome: &Chrome,
        label: &str,
        on: bool,
        lit: Color,
        at: Interaction,
    ) -> Drawing {
        gutter_button_sized(chrome, label, on, lit, at, (21.0, 20.0))
    }

    /// The same button at `size`: a touchscreen's, grown to fill the room
    /// it has. The letter grows with it, from the measured button's 10 at
    /// 20 high.
    #[must_use]
    pub fn gutter_button_sized(
        chrome: &Chrome,
        label: &str,
        on: bool,
        lit: Color,
        at: Interaction,
        (w, h): (f64, f64),
    ) -> Drawing {
        let ink = ink_in(chrome, on.then_some(lit), at, false, 0.25);
        let mut drawing = Drawing::new(w, h);
        let radius = (h * 0.15).max(3.0);
        drawing.fill(rect(0.0, 0.0, w, h, radius), ink.face);
        drawing.stroke(
            rect(0.5, 0.5, w - 1.0, h - 1.0, radius),
            ink.border,
            Stroke::new(1.0),
        );
        let size = (h * 0.5).max(10.0);
        #[expect(clippy::cast_possible_truncation, reason = "a type size")]
        let points = size as f32;
        drawing.text(
            label,
            w / 2.0,
            h / 2.0 + size * 0.35,
            points,
            ink.text,
            Align::Centre,
        );
        drawing
    }

    /// Input monitoring: the dome and the two arcs under it.
    ///
    /// REAPER's `mcp_monitor_*` cell — a small dome with two widening
    /// arcs spreading beneath it, which reads as sound coming toward
    /// you. It sits directly under the record arm, because the two are
    /// one decision made twice: what the track records, and whether you
    /// hear it while it does.
    ///
    /// Three states rather than two, and they are not brightnesses of
    /// each other: OFF is the outline unlit, NORMAL is lit through, and
    /// NOT-WHEN-PLAYING lights the dome alone — the one that says
    /// "sometimes" has to look like neither of the other two or it is
    /// just a dimmer ON.
    #[must_use]
    pub fn monitor(
        chrome: &Chrome,
        mode: Monitoring,
        lit: Color,
        at: Interaction,
        facing: Facing,
    ) -> Drawing {
        let (w, h) = (21.0_f64, 20.0_f64);
        // Mirrored top to bottom when the glyph sits OVER the arm: the
        // arcs open towards the ring they belong to, so the two read
        // as one control however they are stacked.
        let flip = |y: f64| match facing {
            Facing::Down => y,
            Facing::Up | Facing::Right => h - y,
        };
        // A point of the upward glyph, turned to face the way it does:
        // a quarter clockwise about the cell's centre for Right, then
        // against the cell's left edge — the side the arm is on — so the
        // two read as one control rather than two with a gap.
        let place = |x: f64, y: f64| match facing {
            Facing::Right => (w / 2.0 - (y - h / 2.0) - 3.0, h / 2.0 + (x - w / 2.0)),
            Facing::Down | Facing::Up => (x, y),
        };
        let turned = facing == Facing::Right;
        // No plate: REAPER's monitor is a bare glyph on the strip,
        // unlike the mute and solo it sits above. That is not an
        // oversight in the theme — the buttons are things you press and
        // this is a lamp you read, and giving it a housing made a third
        // button out of an indicator.
        //
        // Hover still has to land somewhere, so it lifts the glyph
        // rather than lighting a face that is not there.
        let hovered = at != Interaction::Normal;
        let mut drawing = Drawing::new(w, h);
        let dim = chrome
            .hardware_mark
            .shade(if hovered { -0.1 } else { -0.33 });
        let on = |part_lit: bool| if part_lit { lit } else { dim };
        let _ = chrome;
        let cx = w / 2.0;
        // The dome, then two arcs widening under it. Measured off the
        // cell as proportions so the glyph survives a different size.
        let (dome_x, dome_y) = place(cx, flip(h * 0.34));
        let (rx, ry) = (w * 0.14, h * 0.15);
        drawing.fill(
            Shape::Ellipse {
                cx: dome_x,
                cy: dome_y,
                rx: if turned { ry } else { rx },
                ry: if turned { rx } else { ry },
            },
            on(mode != Monitoring::Off),
        );
        let (arc_x, arc_y) = place(cx, flip(h * 0.42));
        for (index, (radius, thickness)) in [(w * 0.24, 2.0_f64), (w * 0.37, 2.2_f64)]
            .into_iter()
            .enumerate()
        {
            // The outer arc is the one that means "through": the dome
            // alone is the tape-style mode.
            let through = match mode {
                Monitoring::Off => false,
                Monitoring::Normal => true,
                Monitoring::NotWhenPlaying => index == 0,
            };
            // A vertical mirror sends a clockwise-from-twelve angle θ
            // to 180 − θ, so the arcs under the dome become the same
            // arcs over it.
            let start = match facing {
                Facing::Down => 115.0,
                Facing::Up => -65.0,
                Facing::Right => 25.0,
            };
            drawing.stroke(
                Shape::Arc {
                    cx: arc_x,
                    cy: arc_y,
                    r: radius,
                    start,
                    sweep: 130.0,
                },
                on(through),
                Stroke::new(thickness),
            );
        }
        drawing
    }

    /// Which way the monitor glyph opens: arcs under the dome, or
    /// over it.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub enum Facing {
        /// Under the arm, arcs opening downwards — REAPER's own.
        #[default]
        Down,
        /// Over the arm, in the band, arcs opening upwards.
        Up,
        /// Right of the arm, arcs opening rightwards: [`Facing::Up`]
        /// turned a quarter clockwise.
        Right,
    }

    /// Which input-monitoring state a strip is in.
    ///
    /// The art's own three, so a caller maps its domain type once here
    /// rather than the drawing knowing what a `daw_proto` track is.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub enum Monitoring {
        #[default]
        Off,
        Normal,
        NotWhenPlaying,
    }

    /// What a track's routing button has to say.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct Routing {
        /// The track feeds its parent.
        pub parent_send: bool,
        /// It has sends to other tracks.
        pub sends: bool,
        /// Other tracks send to it.
        pub receives: bool,
    }

    /// The colours a control lights up in, from the caller's theme.
    ///
    /// The ported art carried REAPER's own hex — `#5ec3ff` for a lit
    /// volume ring, `#60c2fe` for an FX lamp, `#0d0d0d` for a knob's rim
    /// — which is right for the theme it was traced from and wrong for
    /// every other. The SHAPES are measured and belong to the art; the
    /// colours belong to whoever is drawing.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Lit {
        /// Volume: the fader's fill and the knob's value arc.
        pub volume: Color,
        /// Pan.
        pub pan: Color,
        /// Record arm, and anything else that means "armed".
        pub rec: Color,
        /// A bypassed FX chain.
        pub bypass: Color,
    }

    /// The three lit routing colours.
    ///
    /// Supplied by the caller rather than taken from the art's default
    /// theme, so a REAPER theme's own accent and meter colours reach the
    /// button. The art's choices are `accent`, `meter_warn` and
    /// `meter_danger`, in that order.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct RouteInk {
        /// The output lane. Blue in the source art.
        pub out: Color,
        /// Sends. Amber.
        pub send: Color,
        /// Receives. Red — the source uses a brighter red for a lit lane
        /// than the record ring's.
        pub recv: Color,
    }

    /// Which way the routing lanes lie.
    ///
    /// The whole difference between the theme's two routing images, and
    /// not a rotation of one: the cells are different sizes with
    /// different insets, and the track panel's plate is a scrim where
    /// the mixer's is opaque plastic.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub enum Axis {
        /// The track panel: three bars across.
        #[default]
        Horizontal,
        /// The mixer: three bars down.
        Vertical,
    }

    /// The routing widget: three lanes, each lit by its own kind of
    /// connection.
    ///
    /// Traced off the source cells per family rather than composed from
    /// fractions. The mixer runs three 11x4 bars DOWN a 23x32 cell at
    /// y = 6, 13, 20; the track panel runs three 4x10 bars ACROSS a
    /// 28x22 cell at x = 5, 12, 19. Same rhythm — pitch 7 — turned a
    /// quarter turn, but not the same fractions: guessing them as "56%
    /// of the width, centred" put every lane a pixel and a bit off.
    ///
    /// The first lane is the track's OUTPUT and is lit in every source
    /// cell — a track always has one — so it greys only when the parent
    /// send is cut. Colouring it by whether anything is routed made an
    /// unrouted track look broken rather than merely unrouted.
    #[must_use]
    /// The VERTICAL routing cell, and where its panel sits inside it.
    ///
    /// The cell is REAPER's own `mcp_io` image size and is not ours to
    /// change: `slice` declares it, `declared_slices_match_the_source_art`
    /// checks it against the real PNGs, and this function is what
    /// EXPORTS those PNGs — a 21-wide cell would emit a theme image of
    /// the wrong size.
    ///
    /// So the padding is a fact about the art, stated here. The panel
    /// inside it is 21 wide, which is exactly
    /// `geometry::mcp::BUTTON_W`: a layout that places the CELL where
    /// it places a button therefore puts the PANEL one pixel right of
    /// that button's edge. Place the cell a pixel left of the column
    /// and the panel lands on it.
    pub const ROUTING_CELL_V: (f64, f64) = (23.0, 32.0);
    pub const ROUTING_INSET_V: f64 = 1.0;

    pub fn routing(
        chrome: &Chrome,
        axis: Axis,
        state: Routing,
        ink: RouteInk,
        at: Interaction,
    ) -> Drawing {
        let horizontal = axis == Axis::Horizontal;
        let (vw, vh) = if horizontal {
            (28.0, 22.0)
        } else {
            (23.0, 32.0)
        };
        let plate = ink_in(chrome, None, at, true, 0.35);

        // The panel does not fill its cell, and the inset differs by
        // family rather than being one margin: 28x20 in a 28x22 cell,
        // against 21x28 in a 23x32. See `ROUTING_INSET_V` — a caller
        // lining this up with the buttons beside it has to know that
        // the cell is not the panel.
        let edge = vh * 0.03;
        let (box_x, box_y, box_w, box_h) = if horizontal {
            (0.0, vh / 22.0, vw, vh * 20.0 / 22.0)
        } else {
            (vw / 23.0, vh / 32.0, vw * 21.0 / 23.0, vh * 28.0 / 32.0)
        };

        let mut drawing = Drawing::new(vw, vh);
        // The two families are not one fill at two brightnesses. The
        // track panel's plate is BLACK AT 35% — a scrim that lets the
        // row's colour through, which is why it looks near-black on a
        // dark row and tinted on a coloured one. The mixer's is an
        // opaque grey a touch lighter than plain hardware. Painting both
        // opaque made the track buttons sit on the strip instead of in
        // it.
        let face = if horizontal {
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 89,
            }
        } else {
            plate.face.shade(0.04)
        };
        let plate_rect = rect(
            box_x + edge / 2.0,
            box_y + edge / 2.0,
            box_w - edge,
            box_h - edge,
            vw.min(vh) * 0.16,
        );
        drawing.fill(plate_rect.clone(), face);
        drawing.stroke(plate_rect, plate.border, Stroke::new(edge));
        // The lip along the top.
        drawing.fill(
            rect(
                vw.mul_add(0.08, box_x),
                box_y + edge,
                vw.mul_add(-0.16, box_w),
                vh * 0.04,
                0.0,
            ),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 18,
            },
        );

        // An unlit lane is grey in both families but only opaque in one:
        // the mixer's is solid on plastic, the track panel's half alpha
        // over the strip. Drawing both solid left the panel's unrouted
        // lanes reading as lit.
        let dim = if horizontal {
            alpha(chrome.hardware_mark.shade(-0.29), 0.49)
        } else {
            chrome.hardware_mark.shade(-0.33)
        };
        let lanes = [
            // Disabled greys the OUTPUT lane and nothing else: compared
            // cell for cell, the `_dis` variant differs in one place.
            if state.parent_send { ink.out } else { dim },
            if state.sends { ink.send } else { dim },
            if state.receives { ink.recv } else { dim },
        ];
        let (bar_w, bar_h) = if horizontal {
            (vw * 4.0 / 28.0, vh * 10.0 / 22.0)
        } else {
            (vw * 11.0 / 23.0, vh * 4.0 / 32.0)
        };
        // The traced lane positions, stated rather than stepped from an
        // index: they came off the source cells, and a pitch computed
        // from a loop counter is a number nobody measured.
        let (starts, cell) = if horizontal {
            ([5.0, 12.0, 19.0], vw / 28.0)
        } else {
            ([6.0, 13.0, 20.0], vh / 32.0)
        };
        let cross = if horizontal {
            (vh - bar_h) / 2.0
        } else {
            (vw - bar_w) / 2.0
        };
        for (at_lane, lit) in starts.into_iter().zip(lanes) {
            let along = at_lane * cell;
            let (x, y) = if horizontal {
                (along, cross)
            } else {
                (cross, along)
            };
            drawing.fill(rect(x, y, bar_w, bar_h, bar_w.min(bar_h) / 2.0), lit);
        }
        drawing
    }

    /// `c` at `a` of its opacity.
    fn alpha(color: Color, amount: f64) -> Color {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "an 0..1 opacity scaled into a u8 channel"
        )]
        let scaled = (f64::from(color.a) * amount.clamp(0.0, 1.0)).round() as u8;
        Color { a: scaled, ..color }
    }

    /// What a track's FX chain is doing.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub enum Chain {
        /// Nothing in it.
        #[default]
        Empty,
        /// Loaded and running.
        Active,
        /// Loaded and bypassed.
        Bypassed,
    }

    /// The FX control: a bypass lamp and the labelled button, one pill.
    ///
    /// REAPER blits this as two images side by side — `track_fx_norm` at
    /// 20 wide and `track_fxempty_h` at 16 — which is why the pill is 36
    /// across with a split in it. Drawn as one element rather than two
    /// positioned halves, because composing them put the toggle's
    /// rounded end through the letters.
    ///
    /// The plate is black at 35%, not an opaque grey: on the track panel
    /// it is a scrim that lets the row's colour through.
    #[must_use]
    pub fn fx_pill(chrome: &Chrome, lit: Lit, chain: Chain, at: Interaction) -> Drawing {
        // Traced: 36x22, body rows 1..21 of 22, split at 20 — the
        // labelled half first and the bypass toggle after it, which is
        // the order REAPER blits `track_fx_norm` (20 wide) and
        // `track_fxon_h` (16) in.
        fx_pill_wide(chrome, lit, chain, at, 36.0)
    }

    /// [`fx_pill`] at `w` wide: the mixer's, which spans its strip. The
    /// bypass toggle keeps its traced 16 at the right end; the labelled
    /// half takes the rest, with "FX" centred in it.
    #[must_use]
    pub fn fx_pill_wide(
        chrome: &Chrome,
        lit: Lit,
        chain: Chain,
        at: Interaction,
        w: f64,
    ) -> Drawing {
        let (w, h) = (w.max(36.0), 22.0);
        let split = w - 16.0;
        let plate = ink_in(chrome, None, at, true, 0.35);
        let (body_y, body_h) = (h / 22.0, h * 20.0 / 22.0);
        let radius = h * 0.12;

        let mut drawing = Drawing::new(w, h);
        drawing.fill(
            rect(0.0, body_y, w, body_h, radius),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 89,
            },
        );
        drawing.stroke(
            rect(0.5, body_y + 0.5, w - 1.0, body_h - 1.0, radius),
            plate.border,
            Stroke::new(1.0),
        );
        // The seam between the halves.
        drawing.fill(
            rect(split, body_y + 2.0, 1.0, body_h - 4.0, 0.0),
            plate.border,
        );

        // The lamp: a vertical capsule, measured straight off
        // `track_fxon_h` and `track_fxoff_h` in the installed ReaperTips
        // theme — 4 wide by 10 tall in a 16x22 cell, fully rounded, and
        // #60c2fe lit against #ff6975 bypassed. It is a capsule rather
        // than a square because that is what the art is; drawn as a dot
        // it reads as an LED on a different control.
        let lamp = match chain {
            Chain::Active => lit.volume,
            Chain::Bypassed => lit.bypass,
            // Empty shows a dark slug: there is nothing to bypass, and
            // the slot still has to read as a slot.
            Chain::Empty => chrome.hardware_mark.shade(-0.45),
        };
        drawing.fill(
            rect(
                split + (w - split) / 2.0 - 2.0,
                (h - 10.0) / 2.0,
                4.0,
                10.0,
                2.0,
            ),
            lamp,
        );

        // Neutral letters, printed on the scrim rather than lit: the
        // source is #c1 at 0.61 empty and #eb at 0.87 active, and the
        // chrome ramp's blues read as an indicator instead of as print.
        let (ink, ink_alpha) = match chain {
            Chain::Empty => (chrome.hardware_mark.shade(0.33), 0.61),
            Chain::Active => (chrome.hardware_mark.shade(0.78), 0.87),
            Chain::Bypassed => (lit.bypass, 1.0),
        };
        drawing.text(
            "FX",
            split / 2.0,
            h / 2.0 + 3.5,
            10.5,
            alpha(ink, ink_alpha),
            Align::Centre,
        );
        drawing
    }

    /// The width of [`fx_pill_compact`].
    pub const FX_COMPACT_W: f64 = 26.0;

    /// [`fx_pill`] for a strip too narrow for its 36: one 26-wide pill, no
    /// seam, the lamp a slimmer capsule (2 by 8) at the right end and
    /// "FX" centred in what is left. Same plate, same lamp colours, same
    /// print — the same control, folded up.
    #[must_use]
    pub fn fx_pill_compact(chrome: &Chrome, lit: Lit, chain: Chain, at: Interaction) -> Drawing {
        let (w, h) = (FX_COMPACT_W, 22.0);
        let plate = ink_in(chrome, None, at, true, 0.35);
        let (body_y, body_h) = (h / 22.0, h * 20.0 / 22.0);
        let radius = h * 0.12;

        let mut drawing = Drawing::new(w, h);
        drawing.fill(
            rect(0.0, body_y, w, body_h, radius),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 89,
            },
        );
        drawing.stroke(
            rect(0.5, body_y + 0.5, w - 1.0, body_h - 1.0, radius),
            plate.border,
            Stroke::new(1.0),
        );
        let lamp = match chain {
            Chain::Active => lit.volume,
            Chain::Bypassed => lit.bypass,
            Chain::Empty => chrome.hardware_mark.shade(-0.45),
        };
        drawing.fill(rect(w - 5.0, (h - 8.0) / 2.0, 2.0, 8.0, 1.0), lamp);
        let (ink, ink_alpha) = match chain {
            Chain::Empty => (chrome.hardware_mark.shade(0.33), 0.61),
            Chain::Active => (chrome.hardware_mark.shade(0.78), 0.87),
            Chain::Bypassed => (lit.bypass, 1.0),
        };
        drawing.text(
            "FX",
            (w - 8.0) / 2.0 + 0.5,
            h / 2.0 + 3.5,
            10.5,
            alpha(ink, ink_alpha),
            Align::Centre,
        );
        drawing
    }

    /// The envelope button: the ramp it stands for.
    #[must_use]
    pub fn envelope(chrome: &Chrome, at: Interaction) -> Drawing {
        let (w, h) = (22.0_f64, 20.0_f64);
        let ink = ink_in(chrome, None, at, false, 0.25);
        let mut drawing = Drawing::new(w, h);
        drawing.fill(rect(0.0, 0.0, w, h, 2.0), ink.face);
        drawing.stroke(
            rect(0.5, 0.5, w - 1.0, h - 1.0, 2.0),
            ink.border,
            Stroke::new(1.0),
        );
        // A rising ramp with a handle at each end, which is what an
        // automation lane looks like at this size.
        drawing.stroke(
            Shape::Line {
                from: (4.0, h - 5.0),
                to: (w - 4.0, 5.0),
            },
            ink.text,
            Stroke::round(1.4),
        );
        drawing.fill(circle(4.0, h - 5.0, 1.8), ink.text);
        drawing.fill(circle(w - 4.0, 5.0, 1.8), ink.text);
        drawing
    }

    /// Phase: the slashed circle, lit when inverted.
    #[must_use]
    pub fn phase(chrome: &Chrome, inverted: bool, at: Interaction) -> Drawing {
        let (w, h) = (16.0_f64, 16.0_f64);
        let ink = ink_in(chrome, inverted.then_some(chrome.accent), at, false, 0.25);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let mut drawing = Drawing::new(w, h);
        drawing.fill(circle(cx, cy, 8.0), ink.face);
        drawing.stroke(circle(cx, cy, 5.5), ink.text, Stroke::new(1.3));
        drawing.stroke(
            Shape::Line {
                from: (cx - 5.5, cy + 5.5),
                to: (cx + 5.5, cy - 5.5),
            },
            ink.text,
            Stroke::round(1.3),
        );
        drawing
    }

    /// Which record arm — the mixer's, or the track panel's.
    ///
    /// Not one drawing at two sizes. The mixer's sits in a HOUSING: a
    /// moulding that grows out of the background, with the ring set into
    /// it. The track panel has no room for one, so its ring is bare on
    /// the strip and, with nothing competing for the cell, proportionally
    /// larger — radius 7.40 of a 20 cell against 7.45 of a 36 one.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Arm {
        /// A bare ring, 20x20.
        Panel,
        /// A ring in its housing, 36x24. The housing's straight base is
        /// meant to be invisible — see [`crate::vector_controls::HOUSING_SHOULDER`]
        /// and `geometry::mcp::ARM_OVERHANG`, which say how far to sink
        /// it under the coloured band so only the flare emerges.
        Mixer,
    }

    /// The record arm.
    ///
    /// The ring is an annulus: an outer disc with the hole painted back
    /// over it.
    ///
    /// `behind` is both the housing and what shows through the hole,
    /// because with a housing they are the same thing — the hole is that
    /// housing showing through. Pass the colour of whatever the control
    /// sits ON: the strip's body in the mixer, so the bump reads as the
    /// background growing up into the coloured band rather than as a
    /// grey lump placed on it; the name field in the panel, where there
    /// is no housing and the hole is a hole.
    ///
    /// Radii are traced by sub-pixel coverage rather than by
    /// thresholding: down the mixer's widest row the alpha runs 103, 255
    /// … 255, 102, so the outer edge stands at 2.60 and the radius is
    /// 7.45 — not the 8 that reading the first lit column gives.
    #[must_use]
    pub fn record_arm(
        chrome: &Chrome,
        lit: Color,
        armed: bool,
        at: Interaction,
        arm: Arm,
        behind: Color,
    ) -> Drawing {
        let housing = arm == Arm::Mixer;
        let (vw, vh) = if housing { (36.0, 24.0) } else { (20.0, 20.0) };
        // Traced in EDGE coordinates, not pixel indices: the mixer's ring
        // covers columns 10..24 — the span [10, 25) — so it is centred on
        // 17.5, and rows 5..19 centre it on 12.5. Reading the indices
        // directly gives 17 and 12 and puts the control half a pixel up
        // and to the left.
        let (cx, cy) = if housing {
            (17.5, 12.5)
        } else {
            (vw / 2.0, vh / 2.0)
        };
        let (outer, inner) = if housing { (7.45, 3.67) } else { (7.40, 3.38) };

        let ring = if armed { lit } else { chrome.hardware_mark };
        // The ring lifts a little on hover and sinks when pressed.
        //
        // The mixer's housing lifts with it; the panel's bare ring has
        // no housing, but the ring is the control either way, and a
        // record arm that does not answer the pointer is the one button
        // you most want to be sure of before you click it.
        let ring = match at {
            Interaction::Hover => ring.shade(0.15),
            Interaction::Pressed => ring.shade(-0.12),
            Interaction::Normal => ring,
        };

        let mut drawing = Drawing::new(vw, vh);
        if housing {
            // A circle of radius 11.5 concentric with the ring that goes
            // flat near its widest point, on a base whose top corners are
            // 45 degree flares. No vertical section until the shoulder —
            // every earlier reading of this shape had a straight edge
            // that a coverage trace shows is not there.
            let moulding = behind;
            let shoulder = f64::from(crate::vector_controls::HOUSING_SHOULDER);
            let flare = vw * 0.3194;
            let base = vw * 0.4028;
            drawing.fill(
                Shape::Ellipse {
                    cx,
                    cy: vh * 0.5208,
                    rx: flare,
                    ry: flare,
                },
                moulding,
            );
            drawing.fill(
                Shape::Poly(vec![
                    (cx - flare, vh * 0.592),
                    (cx - base, vh * shoulder),
                    (cx - base, vh),
                    (cx + base, vh),
                    (cx + base, vh * shoulder),
                    (cx + flare, vh * 0.592),
                ]),
                moulding,
            );
        }

        // The ring, lit from above.
        drawing.fill(
            circle(cx, cy, outer),
            Brush::Linear {
                from: (0.0, cy - outer),
                to: (0.0, cy + outer),
                stops: vec![(0.0, ring.shade(0.18)), (1.0, ring)],
            },
        );
        drawing.fill(circle(cx, cy, inner), behind);
        drawing
    }

    /// Turn `(x, y)` about `(cx, cy)` by `deg` clockwise.
    fn rotate(x: f64, y: f64, deg: f64, cx: f64, cy: f64) -> (f64, f64) {
        let angle = deg.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        (
            y.mul_add(-sin, x.mul_add(cos, cx)),
            y.mul_add(cos, x.mul_add(sin, cy)),
        )
    }

    const fn circle(cx: f64, cy: f64, r: f64) -> Shape {
        Shape::Ellipse {
            cx,
            cy,
            rx: r,
            ry: r,
        }
    }

    const fn rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Shape {
        Shape::Rect {
            x,
            y,
            w: width,
            h: height,
            r: radius,
        }
    }
}

#[cfg(test)]
mod fader_scale_tests {
    use super::tcp::{
        FADER_BOTTOM_DB, FADER_MARKS, FADER_TOP_DB, fader_db, fader_norm, fader_scale, gain_norm,
    };

    /// The fit this scale was derived from, checked against the pixels
    /// it was read off. REAPER's groove ran y 125..248 for −56..0 dB and
    /// its labels sat at these measured centres.
    ///
    /// Our travel now runs past unity to +12, so the marks do not sit
    /// at those absolute pixels any more — but the FIT has to survive,
    /// because it is the whole reason the spacing is what it is. So the
    /// scale is rescaled onto REAPER's own −56..0 sub-range and checked
    /// there: linear in dB at 2.204 px/dB, every residual under a pixel.
    #[test]
    fn the_spacing_is_still_reapers_measured_fit() {
        const TOP: f64 = 125.0;
        const BOTTOM: f64 = 248.0;
        let floor = fader_norm(FADER_BOTTOM_DB);
        let unity = fader_norm(0.0);
        let measured = [
            (-6.0, 137.5),
            (-18.0, 165.5),
            (-30.0, 190.5),
            (-42.0, 218.0),
            (-54.0, 243.5),
        ];
        for (db, want) in measured {
            let sub = (fader_norm(db) - floor) / (unity - floor);
            let got = TOP + (BOTTOM - TOP) * (1.0 - sub);
            assert!(
                (got - want).abs() < 1.0,
                "{db} dB: drew at {got:.2}, REAPER has it at {want:.2}"
            );
        }
    }

    /// The travel runs to +12, not to unity.
    ///
    /// REAPER's own fader stops at 0 — that is the measurement, and it
    /// is kept in `FADER_TOP_DB`'s docs — but a fader that cannot add
    /// gain makes you reach for a plugin to do the most ordinary thing
    /// in a mix. Unity is a mark near the top, not the top.
    #[test]
    fn the_travel_runs_past_unity() {
        assert!((fader_norm(FADER_TOP_DB) - 1.0).abs() < f64::EPSILON);
        assert!((FADER_TOP_DB - 12.0).abs() < f64::EPSILON);
        let unity = fader_norm(0.0);
        assert!(unity < 1.0 && unity > 0.8, "unity sits at {unity}");
        assert!(fader_norm(FADER_BOTTOM_DB).abs() < f64::EPSILON);
    }

    #[test]
    fn the_scale_round_trips() {
        for db in [-56.0, -40.0, -12.0, -0.5, 0.0] {
            assert!((fader_db(fader_norm(db)) - db).abs() < 1e-9, "{db} dB");
        }
    }

    /// A gain, not a dB value, is what a track carries.
    #[test]
    fn a_gain_converts_before_it_is_placed() {
        // Unity gain is 0 dB, which is the mark near the top rather
        // than the top itself.
        assert!((gain_norm(1.0) - fader_norm(0.0)).abs() < 1e-9);
        // Half the gain is −6 dB, which is one mark down.
        assert!((gain_norm(0.5) - fader_norm(-6.0206)).abs() < 1e-3);
        // Silence has no logarithm, and must not produce one.
        assert!(gain_norm(0.0).abs() < f64::EPSILON);
        assert!(gain_norm(-1.0).abs() < f64::EPSILON);
    }

    /// Every mark is inside the travel, so none is drawn off the end of
    /// the groove it belongs to.
    #[test]
    fn every_mark_is_on_the_fader() {
        for db in FADER_MARKS {
            let norm = fader_norm(db);
            assert!(norm > 0.0 && norm < 1.0, "{db} dB sits at {norm}");
        }
        // And the ceiling is deliberately unlabelled: a mark there
        // would hang off the end of the column to say what the fader
        // stopping already says.
        assert!(
            !FADER_MARKS.contains(&FADER_TOP_DB),
            "the ceiling grew a label"
        );
        // A tick and a number for each mark, on a column with room for
        // all of them.
        let ink = super::hex("#FF4000");
        let zones = [
            super::hex("#40FF80"),
            super::hex("#FFD040"),
            super::hex("#FF4040"),
        ];
        let roomy = fader_scale(20.0, 400.0, ink, zones, 8.0, 0.0);
        assert_eq!(roomy.ops.len(), FADER_MARKS.len() * 2);
        // And on one with room for fewer, the scale THINS rather than
        // drawing its numbers over each other.
        let tight = fader_scale(20.0, 90.0, ink, zones, 8.0, 0.0);
        assert!(tight.ops.len() < roomy.ops.len(), "{}", tight.ops.len());
        assert!(
            tight.ops.len() >= 4,
            "the scale vanished: {}",
            tight.ops.len()
        );
    }

    /// A mark the signal has passed is drawn in the meter's ink, so the
    /// level can be read as "the last number that is lit" rather than
    /// by measuring the bar against the numbers.
    #[test]
    fn the_marks_light_as_the_level_passes_them() {
        let ink = super::hex("#FF4000");
        let zones = [
            super::hex("#40FF80"),
            super::hex("#FFD040"),
            super::hex("#FF4040"),
        ];
        let count = |level: f64| {
            fader_scale(20.0, 124.0, ink, zones, 8.0, level)
                .ops
                .iter()
                .filter(|op| matches!(op, crate::paint::Op::Text { color, .. } if *color != ink))
                .count()
        };
        assert_eq!(count(0.0), 0, "silence lit a mark");
        assert_eq!(count(1.0), FADER_MARKS.len(), "full scale left one dark");
        // And it climbs: more signal is never fewer lit marks.
        let mut last = 0;
        for step in 0..=10 {
            let now = count(f64::from(step) / 10.0);
            assert!(now >= last, "the scale unlit itself at {step}");
            last = now;
        }
    }
}
