//! Where everything on a track panel row is — computed once.
//!
//! The mixer's counterpart is [`crate::strip::Strip`], and this exists
//! for the same reason: drawing, hit testing and the live overlay must
//! read ONE layout rather than each working the geometry out again.
//!
//! The row is harder than the strip in one way. A strip's controls are
//! either shown or not; a row's CHANGE SHAPE — past a threshold the
//! volume knob becomes a flattened fader and the pan knob becomes a
//! line, because both are bars whose length is the value and squashing
//! that axis carries no meaning. So a control's rect is not simply
//! present or absent, and [`Row::indicator`] says which form it is in.

use daw_theme_art::geometry::tcp as g;
use vello::kurbo::Rect;

use crate::tcp::{BUTTON, BUTTON_GAP, Density, KNOB_LEGIBLE, Tcp};

/// A control on a track panel row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Mute,
    Solo,
    RecArm,
    Volume,
    Pan,
    Routing,
    Fx,
    Name,
    /// The folder mark in the rail — clicked to fold.
    Folder,
    /// Polarity, in the bottom corner of the gutter. The one control
    /// that changes the signal rather than a level, which is why the
    /// theme puts it on its own away from the button column.
    Phase,
    /// The fixed lanes' step back: the lane before the playing one plays.
    LanePrev,
    /// And forward.
    LaneNext,
    /// The playing lane's name, between them: shows all the lanes, or only
    /// the one that plays again.
    LaneName,
    /// The `+` beside the lane line: every lane shown, stacked, to hear and
    /// comp from — or only the one that plays again.
    LaneShow,
    /// The automation button, at the head of row two: what the track can
    /// automate, to show in lanes ([`crate::automation_menu`]).
    Automation,
}

impl Control {
    /// Set by dragging rather than by clicking.
    #[must_use]
    pub const fn is_continuous(self) -> bool {
        matches!(self, Self::Volume | Self::Pan)
    }
}

/// The automation button on a row at `y`, `height` tall: at the head of
/// row two, on a full panel's row tall enough for one.
#[must_use]
pub fn automation_button(y: f64, height: f64, tcp: Tcp) -> Option<Rect> {
    let two = f64::from(g::ROW_TWO);
    let h = f64::from(g::FIELD_H);
    (!tcp.compact && height >= two + h + 2.0).then(|| {
        let x = f64::from(g::NAME_FIELD_X);
        Rect::new(x, y + two, x + 21.0, y + two + h)
    })
}

/// Which form a value indicator has taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Indicator {
    /// A knob, at rows tall enough to turn one.
    Knob,
    /// A flattened bar, on rows that are not.
    Bar,
}

/// One row's geometry, in the panel's coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub y: f64,
    pub height: f64,
    pub indent: f64,
    pub density: Density,
    /// The panel this row is in — full, or compact.
    pub tcp: Tcp,
    /// The row's control band — where the name, knobs and arm sit.
    field_top: f64,
    field_h: f64,
    band: f64,
    /// The track has fixed lanes, so the gutter shows which plays.
    lanes: bool,
}

impl Row {
    /// Resolve a row.
    #[must_use]
    pub fn new(y: f64, height: f64, depth: i32, is_folder: bool, tcp: Tcp) -> Self {
        let indent = (f64::from(depth.max(0)) * tcp.indent()).min(tcp.max_indent());
        let density = Density::at(height);
        // The control band, as `draw_row` lays it out (`tcp::band`).
        let (field_top, field_h) = crate::tcp::band(y, height, density);
        let _ = is_folder;
        Self {
            y,
            height,
            indent,
            density,
            tcp,
            field_top,
            field_h,
            band: field_h,
            lanes: false,
        }
    }

    /// The row of a track with fixed lanes (more than one): its gutter
    /// shows the playing lane's name between steps back and forward.
    #[must_use]
    pub const fn with_lanes(mut self, lanes: bool) -> Self {
        self.lanes = lanes;
        self
    }

    /// The lane line: in the gutter, under mute and solo and above
    /// polarity, as wide as the two buttons — on a row tall enough to
    /// hold it between them, on a track with fixed lanes.
    #[must_use]
    pub fn lane_line(&self) -> Option<Rect> {
        /// Its height: a name small enough to sit between the steps.
        const LINE_H: f64 = 16.0;
        /// Clear of the buttons above and polarity below.
        const GAP: f64 = 3.0;
        if !self.lanes || self.tcp.compact || self.density == Density::Bar {
            return None;
        }
        let above = self.rect(Control::Mute)?;
        let floor = self
            .rect(Control::Phase)
            .map_or(self.y + self.height, |phase| phase.y0);
        let top = above.y1 + GAP;
        (top + LINE_H + GAP <= floor).then(|| {
            Rect::new(
                above.x0,
                top,
                BUTTON.0.mul_add(2.0, above.x0) + BUTTON_GAP,
                top + LINE_H,
            )
        })
    }

    /// Whether the band is tall enough for the routing and FX buttons.
    ///
    /// On every row that has the room, not only the full ones: the panel
    /// is the same width at every height, so a compact row has the same
    /// space along it that a full row does. The same line the knobs use,
    /// since the buttons are the same height as a knob.
    fn fits_buttons(&self) -> bool {
        self.band >= KNOB_LEGIBLE
    }

    /// Whether a value indicator is a knob or a flattened bar.
    ///
    /// The threshold is the row's own, not a guess: below it a knob is
    /// too small to turn and REAPER swaps it for a bar, so a hit test
    /// that assumed a knob would be testing a control that is not
    /// there.
    #[must_use]
    pub fn indicator(&self) -> Indicator {
        if self.band >= KNOB_LEGIBLE {
            Indicator::Knob
        } else {
            Indicator::Bar
        }
    }

    /// A compact row's second line, under its name: mute, solo and the
    /// arm, in that order across the name field, as Logic's taller rows
    /// put them. Only on a row tall enough for two lines (`Density::Full`),
    /// and only where the second one has room for a button.
    fn second_line(&self, control: Control) -> Option<Rect> {
        const GAP: f64 = 4.0;
        if self.density != Density::Full {
            return None;
        }
        let index = match control {
            Control::Mute => 0.0,
            Control::Solo => 1.0,
            Control::RecArm => 2.0,
            _ => return None,
        };
        let top = self.field_top + self.field_h + GAP;
        let h = (self.y + self.height - top - GAP).min(26.0);
        if h < 16.0 {
            return None;
        }
        let (x, w) = self.tcp.name_field();
        let x = x + self.indent;
        let across = (w - self.indent).max(0.0);
        let cell = ((across - GAP * 2.0) / 3.0).min(34.0);
        let x = x + index * (cell + GAP);
        Some(Rect::new(x, top, x + cell, top + h))
    }

    /// A lane step: the lane line's left end (back) or right (`next`).
    fn lane_step(&self, next: bool) -> Option<Rect> {
        /// A step's width.
        const STEP_W: f64 = 11.0;
        let line = self.lane_line()?;
        Some(if next {
            Rect::new(line.x1 - STEP_W, line.y0, line.x1, line.y1)
        } else {
            Rect::new(line.x0, line.y0, line.x0 + STEP_W, line.y1)
        })
    }

    /// The `+` beside the lane line, at its left: the tint's last pixels
    /// before the gutter, clear of the record-input combo an armed row
    /// draws there.
    fn lane_show(&self) -> Option<Rect> {
        /// Its width.
        const SHOW_W: f64 = 10.0;
        let line = self.lane_line()?;
        let x1 = line.x0 - 1.0;
        Some(Rect::new(x1 - SHOW_W, line.y0, x1, line.y1))
    }

    /// Where a control is, or `None` if this row does not show it.
    #[must_use]
    pub fn rect(&self, control: Control) -> Option<Rect> {
        if self.density == Density::Bar {
            // A row drawn as a band has no controls — it is two
            // rectangles saying which track this is and how it is
            // coloured. Nothing to hit.
            return None;
        }
        if !self.tcp.shows(control) {
            return None;
        }
        let rail = f64::from(g::COLUMN_RULE_X);
        let field_x = self.tcp.name_field().0 + self.indent;
        let volume_x = self.tcp.volume_x();
        match control {
            Control::LanePrev | Control::LaneNext => self.lane_step(control == Control::LaneNext),
            Control::LaneShow => self.lane_show(),
            Control::LaneName => {
                let (prev, next) = (self.lane_step(false)?, self.lane_step(true)?);
                Some(Rect::new(prev.x1, prev.y0, next.x0, next.y1))
            }
            // At the head of row two, where a row is tall enough to have
            // one (`tcp::row_two` draws it from the same numbers).
            Control::Automation => automation_button(self.y, self.height, self.tcp),
            Control::Folder => Some(Rect::new(
                self.indent,
                self.y,
                self.indent + rail,
                self.y + self.height,
            )),
            // In the gutter at the right, in row one where there is a
            // row one, and lying down in the control band where there
            // is not: a compact row keeps its mute and its solo — they
            // are what a row is scanned for — and the routing and FX
            // that share the band at full height are the ones that go.
            // A touchscreen's: the two filling the gutter, side by side,
            // and as tall as the row less a margin.
            // The compact panel's: mute, solo and the arm on a second line
            // under the name, where the row is tall enough to have one.
            Control::RecArm if !self.tcp.recording => None,
            Control::Mute | Control::Solo | Control::RecArm if self.tcp.compact => {
                self.second_line(control)
            }
            Control::Mute | Control::Solo => {
                let x = self.tcp.tint_w()
                    + 2.0
                    + if control == Control::Mute {
                        0.0
                    } else {
                        BUTTON.0 + BUTTON_GAP
                    };
                // Centred on the band, the line the name is on, at
                // every height (`tcp::band`).
                let h = self.field_h.min(BUTTON.1);
                let top = self.field_top + (self.field_h - h) / 2.0;
                Some(Rect::new(x, top, x + BUTTON.0, top + h))
            }
            Control::RecArm => (self.indicator() == Indicator::Knob).then(|| {
                let x = field_x + 3.0;
                let top = self.field_top + (self.field_h - 20.0) / 2.0;
                Some(Rect::new(x, top, x + 20.0, top + 20.0))
            })?,
            // A row too short for the knobs has no volume or pan: a
            // bar there was too fine to read or to grab.
            Control::Volume | Control::Pan if self.indicator() != Indicator::Knob => None,
            Control::Volume => {
                // Scaled DOWN to a field too short to hold the knob,
                // never up past the size it was drawn at. A control is
                // the size it was authored: a taller track is a taller
                // lane with the same knob on it, which is what REAPER
                // does and what the pan knob beside it always did.
                let w = if self.indicator() == Indicator::Knob {
                    24.0 * (self.field_h / 22.0).min(1.0)
                } else {
                    24.0
                };
                Some(Rect::new(
                    volume_x - w / 2.0,
                    self.field_top,
                    volume_x + w / 2.0,
                    self.field_top + self.field_h,
                ))
            }
            Control::Pan => {
                let x = f64::from(g::PAN_KNOB_X);
                Some(Rect::new(
                    x,
                    self.field_top,
                    x + 25.0,
                    self.field_top + self.field_h,
                ))
            }
            Control::Routing => self.fits_buttons().then(|| {
                let x = f64::from(g::ROUTING_X);
                Some(Rect::new(
                    x,
                    self.field_top,
                    x + 26.0,
                    self.field_top + self.field_h,
                ))
            })?,
            // Hidden on rows too short for it, by the theme's own
            // formula — the row's shape must not depend on its height.
            Control::Phase => (self.height >= f64::from(g::PHASE_HIDE_H)).then(|| {
                let x = self.tcp.tint_w() + f64::from(g::GUTTER_BUTTON_X) + 3.0;
                let y = self.y + self.height - f64::from(g::PHASE_FROM_FLOOR);
                Some(Rect::new(
                    x,
                    y,
                    // The glyph's own width, which is one measurement
                    // shared by both panels — the phase button is the
                    // same art in the strip and in the row.
                    x + f64::from(daw_theme_art::geometry::mcp::PHASE_W),
                    y + f64::from(daw_theme_art::geometry::mcp::PHASE_W),
                ))
            })?,
            Control::Fx => self.fits_buttons().then(|| {
                let x = f64::from(g::FX_IN_X);
                Some(Rect::new(
                    x,
                    self.field_top,
                    x + 36.0,
                    self.field_top + self.field_h,
                ))
            })?,
            Control::Name => {
                let x = self.tcp.name_x() + self.indent;
                Some(Rect::new(
                    x,
                    self.field_top,
                    (volume_x - 14.0).max(x),
                    self.field_top + self.field_h,
                ))
            }
        }
    }

    /// Which control is at a point in the panel's coordinates.
    #[must_use]
    pub fn control_at(&self, x: f64, y: f64) -> Option<Control> {
        // Small before large, so the arm inside the name field wins
        // over the field and the knobs win over the row.
        [
            Control::LaneShow,
            Control::LanePrev,
            Control::LaneNext,
            Control::LaneName,
            Control::Phase,
            Control::Mute,
            Control::Solo,
            Control::RecArm,
            Control::Volume,
            Control::Pan,
            Control::Routing,
            Control::Fx,
            Control::Automation,
            Control::Folder,
            Control::Name,
        ]
        .into_iter()
        .find(|control| {
            self.rect(*control)
                .is_some_and(|r| x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tcp::Tcp;

    fn tall() -> Row {
        Row::new(100.0, 70.0, 0, false, Tcp::FULL)
    }

    /// What is drawn is what is hit, for every control the row shows.
    #[test]
    fn a_control_is_hit_where_it_is_drawn() {
        let row = tall();
        for control in [
            Control::Mute,
            Control::Solo,
            Control::RecArm,
            Control::Volume,
            Control::Pan,
            Control::Routing,
            Control::Fx,
        ] {
            let Some(rect) = row.rect(control) else {
                continue;
            };
            assert_eq!(
                row.control_at(rect.center().x, rect.center().y),
                Some(control),
                "{control:?} at {rect:?}"
            );
        }
    }

    /// A row too short to turn a knob shows bars instead, and the arm
    /// goes entirely — a five-pixel ring is neither readable nor
    /// hittable, and claiming it is there would make a dead zone.
    /// Zooming rows taller never stretches the name or moves it off the
    /// line the mute and solo sit on: every row has the same band, at the
    /// same place from its top.
    #[test]
    fn the_name_and_its_buttons_keep_their_size_at_every_height() {
        let name_at = |h: f64| {
            Row::new(100.0, h, 1, false, Tcp::FULL)
                .rect(Control::Name)
                .unwrap()
        };
        let reference = name_at(70.0);
        for h in [40.0, 57.0, 58.0, 70.0, 120.0, 300.0] {
            let name = name_at(h);
            assert!(
                (name.height() - reference.height()).abs() < 1e-9,
                "{h}: {name:?}"
            );
            assert!((name.y0 - reference.y0).abs() < 1e-9, "{h}: {name:?}");
            let row = Row::new(100.0, h, 1, false, Tcp::FULL);
            let mute = row.rect(Control::Mute).unwrap();
            let solo = row.rect(Control::Solo).unwrap();
            let middle = |r: Rect| (r.y0 + r.y1) / 2.0;
            assert!(
                (middle(mute) - middle(name)).abs() < 1e-9,
                "{h}: mute off the name's line"
            );
            assert!(
                (middle(solo) - middle(name)).abs() < 1e-9,
                "{h}: solo off the name's line"
            );
        }
        // A row too short for the whole band shrinks it, and keeps it
        // inside the row.
        let short = Row::new(100.0, 20.0, 0, false, Tcp::FULL)
            .rect(Control::Name)
            .unwrap();
        assert!(short.y0 >= 101.0 && short.y1 <= 119.0, "{short:?}");
    }

    #[test]
    fn the_arm_is_there_only_when_recording() {
        let tall = Row::new(0.0, 60.0, 0, false, Tcp::FULL);
        assert!(tall.rect(Control::RecArm).is_some());
        let mixing = Row::new(
            0.0,
            60.0,
            0,
            false,
            Tcp {
                recording: false,
                ..Tcp::FULL
            },
        );
        assert!(mixing.rect(Control::RecArm).is_none());
        // Everything else stays where it was.
        assert_eq!(mixing.rect(Control::Volume), tall.rect(Control::Volume));
    }

    #[test]
    fn a_short_row_drops_the_knobs_and_the_arm() {
        let short = Row::new(0.0, 16.0, 0, false, Tcp::FULL);
        assert_eq!(short.indicator(), Indicator::Bar);
        assert!(short.rect(Control::RecArm).is_none());
        // No volume or pan either: a bar there was too fine to read or
        // to grab.
        assert!(short.rect(Control::Volume).is_none());
        assert!(short.rect(Control::Pan).is_none());
    }

    /// Routing and FX are on every row with room for them: the panel is
    /// the same width at every height, so a compact row has the space.
    #[test]
    fn a_compact_row_keeps_routing_and_fx() {
        let compact = Row::new(0.0, 40.0, 0, false, Tcp::FULL);
        assert_eq!(compact.density, Density::Compact);
        assert!(compact.rect(Control::Routing).is_some());
        assert!(compact.rect(Control::Fx).is_some());
        // Too short for a button that size: they go, as the arm does.
        let short = Row::new(0.0, 16.0, 0, false, Tcp::FULL);
        assert!(short.rect(Control::Routing).is_none());
        assert!(short.rect(Control::Fx).is_none());
    }

    /// A row drawn as a band has nothing to hit at all.
    #[test]
    fn a_band_has_no_controls() {
        let band = Row::new(0.0, 4.0, 0, false, Tcp::FULL);
        assert_eq!(band.density, Density::Bar);
        for control in [
            Control::Mute,
            Control::Volume,
            Control::Name,
            Control::Folder,
        ] {
            assert!(band.rect(control).is_none(), "{control:?} on a band");
        }
        assert!(band.control_at(100.0, 2.0).is_none());
    }

    /// Indentation moves a row's own controls but not the columns
    /// REAPER fixes — the knobs and the routing widget are at measured
    /// offsets from the panel's right, not from the row's content.
    #[test]
    fn indent_moves_the_name_but_not_the_knobs() {
        let flat = Row::new(0.0, 70.0, 0, false, Tcp::FULL);
        let deep = Row::new(0.0, 70.0, 3, false, Tcp::FULL);
        assert!(
            deep.rect(Control::Name).unwrap().x0 > flat.rect(Control::Name).unwrap().x0,
            "the name should move with the indent"
        );
        assert_eq!(flat.rect(Control::Pan), deep.rect(Control::Pan));
        assert_eq!(flat.rect(Control::Volume), deep.rect(Control::Volume));
    }

    /// A track with lanes has its `+` beside the lane line, hit where it is
    /// drawn and clear of the steps; a track without lanes has neither.
    #[test]
    fn the_lanes_plus_sits_beside_the_lane_line() {
        let laned = Row::new(100.0, 70.0, 0, false, Tcp::FULL).with_lanes(true);
        let line = laned.lane_line().expect("a lane line");
        let plus = laned.rect(Control::LaneShow).expect("the plus");
        assert!(plus.x1 <= line.x0, "left of the line: {plus:?} {line:?}");
        assert!((plus.y0 - line.y0).abs() < 1e-9 && (plus.y1 - line.y1).abs() < 1e-9);
        assert_eq!(
            laned.control_at(plus.center().x, plus.center().y),
            Some(Control::LaneShow)
        );
        let prev = laned.rect(Control::LanePrev).expect("the step back");
        assert_eq!(
            laned.control_at(prev.center().x, prev.center().y),
            Some(Control::LanePrev),
            "the step is still the step's"
        );
        let plain = Row::new(100.0, 70.0, 0, false, Tcp::FULL);
        assert!(plain.rect(Control::LaneShow).is_none());
    }

    /// Knobs and faders are dragged; buttons are clicked.
    #[test]
    fn only_the_value_controls_are_dragged() {
        assert!(Control::Volume.is_continuous());
        assert!(Control::Pan.is_continuous());
        for click in [Control::Mute, Control::Solo, Control::RecArm, Control::Fx] {
            assert!(!click.is_continuous(), "{click:?}");
        }
    }
}
