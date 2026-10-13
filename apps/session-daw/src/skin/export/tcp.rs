//! The painters' track panel row, written out as a theme role (`tcp.row`).
//!
//! The strip's counterpart: [`crate::row::Row`]'s geometry as expressions
//! over the row's height `h` (and the host's `row.indent`), and the
//! painters' controls — `tcp::draw_row`'s recording and
//! `overlay::control_row`'s live pass — as drawings, every state of each
//! shown by a binding. The full panel only: the compact panel's second
//! line is the painters' still.
//!
//! What only the host can measure it passes in: the name as elided to the
//! field (`row.name`), the track number as fitted to the rail
//! (`row.number`, where it goes), where the icon goes (`%icon`, which the
//! host draws). The folders the row sits inside are the host's too, drawn
//! into `%steps` under the rail.

use std::fmt::Write as _;

use daw_theme_art::geometry::tcp as gt;
use daw_theme_art::mixer_controls::Interaction;
use daw_theme_art::paint::tcp as art;
use daw_theme_art::paint::{Align, Drawing, Op};

use super::{Follow, Gen, Place, STATES, hex, n, q, state};
use crate::tcp::to_theme;

/// The role a theme draws one track panel row as.
pub const ROW: &str = "tcp.row";

/// The row's control band: its top and its height (`tcp::band`).
const FT: &str = "(h >= 58 ? 6 : clamp((h - min(24, max(h - 2, 1))) / 2, 1, 6))";
const FH: &str = "(h >= 58 ? 24 : min(24, max(h - 2, 1)))";

impl Gen<'_> {
    /// The `tcp.row` page.
    pub(super) fn tcp(&mut self) {
        let p = self.palette;
        let rail = f64::from(gt::COLUMN_RULE_X);
        let tint_w = f64::from(gt::TINT_W);
        let field_x = f64::from(gt::NAME_FIELD_X);
        let field_w = f64::from(gt::NAME_FIELD_W);
        let volume_x = field_x + field_w;
        let knob = format!("({FH} >= 20)");
        self.line(1, &format!("<Page name={}>", q(ROW)));
        self.line(
            2,
            &format!("<Frame name=\"Row\" w={} h=70>", n(f64::from(gt::ROW_W))),
        );
        // ── the recording (`tcp::draw_row`) ──
        self.line(3, "<Rectangle name=\"Tint\" w=343 h=70 bind:w=\"w\" bind:h=\"h\" bind:fill=\"track.tint\" />");
        self.line(
            3,
            &format!(
                "<Rectangle name=\"Gutter\" x={} w={} h=70 bind:h=\"h\" fill=\"{}\" />",
                n(tint_w),
                n(f64::from(gt::ROW_W) - tint_w),
                hex(to_theme(p.tcp_gutter))
            ),
        );
        // A band (a row too short for anything) is its tint and gutter.
        self.line(
            3,
            "<Frame name=\"Body\" w=343 h=70 bind:w=\"w\" bind:h=\"h\" bind:visible=\"h >= 11\">",
        );
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Column\" w=20 h=70 bind:w=\"row.indent + {}\" bind:h=\"h\" fill=\"{}\" />",
                n(rail),
                hex(to_theme(p.tcp_column))
            ),
        );
        self.line(
            4,
            "<Frame name=\"%steps\" w=343 h=70 bind:w=\"w\" bind:h=\"h\" />",
        );
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Rail\" w={} h=70 bind:x=\"row.indent\" bind:h=\"h\" bind:fill=\"track.band\" />",
                n(rail)
            ),
        );
        let rule = hex(to_theme(p.tcp_rule));
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Rule\" x={} w=1 h=70 bind:x=\"row.indent + {}\" bind:h=\"h\" fill=\"{rule}\" />",
                n(rail),
                n(rail)
            ),
        );
        self.line(4, &format!("<Rectangle name=\"Edge\" x=341 w=1 h=70 bind:x=\"w - 2\" bind:h=\"h\" fill=\"{rule}\" />"));
        // The folder mark, at the rail's top, smaller on a shorter row.
        let scale = "clamp(h / 18, 0.4, 1)";
        let mark = art::folder_mark(to_theme(p.text_dim));
        self.drawing(
            4,
            &mark,
            &Place {
                name: "Folder mark",
                x: &format!("row.indent + ({} - 9 * {scale}) / 2", n(rail)),
                y: &format!("2 * {scale}"),
                visible: "track.folder",
                follow: Follow::Fixed,
                track: "",
                extra: &format!("bind:scale={}", q(scale)),
            },
        );
        // The number, fitted and placed by the host; on the rail's colour.
        let ink_on = "(luma(track.band) > 0.179 ? #000000 : #e8e8ea)";
        self.line(
            4,
            &format!(
                "<Text name=\"Number\" w=400 h=15 size=11 bind:x=\"row.number_x - 1\" bind:y=\"row.number_baseline - {} * row.number_size - 1\" bind:size=\"row.number_size\" bind:text=\"row.number\" bind:fill={} bind:visible=\"row.has_number\">1</Text>",
                n(super::BASELINE),
                q(ink_on)
            ),
        );
        self.line(4, "<Frame name=\"%icon\" w=16 h=16 bind:x=\"row.icon_x\" bind:y=\"row.icon_y\" bind:w=\"row.icon_size\" bind:h=\"row.icon_size\" bind:visible=\"row.icon\" />");
        self.line(
            4,
            &format!(
                "<Text name=\"Name\" w=400 h=15 size=11 bind:x=\"57 + row.indent\" bind:y={} bind:size=\"row.name_size\" bind:text=\"row.name\" bind:fill={}>Track</Text>",
                q(&format!(
                    "{FT} + {FH} / 2 + 0.35 * row.name_size - {} * row.name_size - 1",
                    n(super::BASELINE)
                )),
                q(&format!(
                    "track.selected ? {} : {}",
                    hex(to_theme(p.text)),
                    hex(to_theme(p.text_dim))
                ))
            ),
        );
        self.row_two();
        // ── the live pass (`overlay::control_row`) ──
        self.tcp_buttons();
        self.tcp_fx_routing_phase(&knob);
        self.tcp_arm(&knob, field_x);
        self.tcp_volume(&knob, volume_x);
        self.tcp_pan(&knob);
        // The name's hover: one rule in the accent under it.
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Name hover\" w=97 h=1 bind:x=\"58 + row.indent\" bind:y={} bind:w=\"max({} - (58 + row.indent), 0)\" fill=\"{}\" bind:visible={} />",
                q(&format!("{FT} + {FH} - 1")),
                n(volume_x - 14.0),
                hex(to_theme(p.accent)),
                q(&format!("!{}", state("name", Interaction::Normal)))
            ),
        );
        self.tcp_hits(&knob, field_x, volume_x);
        self.line(3, "</Frame>");
        self.line(2, "</Frame>");
        self.line(1, "</Page>");
    }

    /// Recording's row on a full row of an armed track: the input FX slot
    /// and the input.
    fn row_two(&mut self) {
        let p = self.palette;
        let two = f64::from(gt::ROW_TWO);
        let fh = f64::from(gt::FIELD_H);
        let base = two + 14.0;
        self.line(
            4,
            "<Frame name=\"Row two\" w=343 h=70 bind:visible=\"h >= 58 && track.armed\">",
        );
        self.line(
            5,
            &format!(
                "<Rectangle name=\"Input FX\" x=56 y={} w=34 h={} fill=\"{}\" />",
                n(two),
                n(fh),
                hex(to_theme(p.tcp_field))
            ),
        );
        self.line(
            5,
            &format!(
                "<Text x=-128 y={} w=400 h=14 size=10 text-align=\"center\" fill=\"{}\">FX</Text>",
                n(base - super::BASELINE * 10.0 - 1.0),
                hex(to_theme(p.text_faint))
            ),
        );
        self.line(
            5,
            &format!(
                "<Rectangle name=\"Input\" x=91 y={} w=195 h={} fill=\"{}\" />",
                n(two),
                n(fh),
                hex(to_theme(p.tcp_combo))
            ),
        );
        self.line(
            5,
            &format!(
                "<Text name=\"Input name\" x=-12.5 y={} w=400 h=15 size=11 text-align=\"center\" bind:text=\"row.input\" fill=\"{}\">None</Text>",
                n(base - super::BASELINE * 11.0 - 1.0),
                hex(to_theme(p.text_dim))
            ),
        );
        self.line(
            5,
            &format!(
                "<Vector name=\"Caret\" x=272 y={} w=7 h=4 geometry=\"M 0 0 L 7 0 L 3.5 4 Z\" fill=\"{}\" />",
                n(two + 8.0),
                hex(to_theme(p.text_faint))
            ),
        );
        self.line(4, "</Frame>");
    }

    /// Mute and solo in the gutter, squashed on a row too short for them.
    fn tcp_buttons(&mut self) {
        let p = self.palette;
        let font = crate::text::Font::embedded().expect("the embedded face");
        let tint_w = f64::from(gt::TINT_W);
        let h = format!("min({FH}, 20)");
        let k = format!("({h} / 20)");
        for (name, label, field, lit, dx) in [
            ("mute", "M", "track.mute", crate::tcp::mute_lit(p), 0.0),
            ("solo", "S", "track.solo", crate::tcp::solo_lit(p), 22.0),
        ] {
            for on in [false, true] {
                for at in STATES {
                    let full = art::gutter_button(&p.chrome, label, on, lit, at);
                    let visible = format!(
                        "{}{field} && {}",
                        if on { "" } else { "!" },
                        state(name, at)
                    );
                    let (x, y) = (n(tint_w + 2.0 + dx), format!("{FT} + ({FH} - {h}) / 2"));
                    // The button squashed to the row; its letter shrunk
                    // instead, from where its left edge was at full size —
                    // as `art::squashed` sets it, so a short row's M is
                    // smaller rather than flattened.
                    let d = Drawing {
                        w: full.w,
                        h: full.h,
                        ops: full
                            .ops
                            .iter()
                            .filter(|o| !matches!(o, Op::Text { .. }))
                            .cloned()
                            .collect(),
                    };
                    self.drawing(
                        4,
                        &d,
                        &Place {
                            name: &format!("{label} {on} {at:?}"),
                            x: &x,
                            y: &y,
                            visible: &visible,
                            follow: Follow::Fixed,
                            track: "",
                            extra: &format!("bind:scale-y={}", q(&k)),
                        },
                    );
                    for op in &full.ops {
                        let Op::Text {
                            body,
                            x: tx,
                            baseline,
                            size,
                            color,
                            align,
                        } = op
                        else {
                            continue;
                        };
                        let left = match align {
                            Align::Left => *tx,
                            Align::Centre => *tx - font.width(body, *size) / 2.0,
                        };
                        let size = f64::from(*size);
                        let ink = self.color(*color, "");
                        self.line(
                            4,
                            &format!(
                                "<Text name=\"{label} letter\" w=400 h={} size={} bind:x={} bind:y={} bind:size={} {ink} bind:visible={}>{}</Text>",
                                n(size + 4.0),
                                n(size),
                                q(&format!("{x} + {} - 1", n(left))),
                                q(&format!(
                                    "{y} + {} * {k} - {} * {} * {k} - 1",
                                    n(*baseline),
                                    n(super::BASELINE),
                                    n(size)
                                )),
                                q(&format!("{} * {k}", n(size))),
                                q(&visible),
                                super::xml(body)
                            ),
                        );
                    }
                }
            }
        }
    }

    /// The FX pill and routing on rows with room for them, polarity in the
    /// gutter's corner on rows tall enough.
    fn tcp_fx_routing_phase(&mut self, knob: &str) {
        let p = self.palette;
        for chain in [art::Chain::Empty, art::Chain::Active] {
            let has = if chain == art::Chain::Empty {
                "track.fx == 0"
            } else {
                "track.fx > 0"
            };
            for at in STATES {
                let d = art::fx_pill(&p.chrome, crate::tcp::lit(p), chain, at);
                self.drawing(
                    4,
                    &d,
                    &Place {
                        name: &format!("FX {chain:?} {at:?}"),
                        x: &n(f64::from(gt::FX_IN_X)),
                        y: FT,
                        visible: &format!("{knob} && {has} && {}", state("fx", at)),
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
        for bits in 0..8_u8 {
            let routes = art::Routing {
                parent_send: bits & 1 != 0,
                sends: bits & 2 != 0,
                receives: bits & 4 != 0,
            };
            let when = format!(
                "{}track.parent_send && track.sends {} 0 && track.receives {} 0",
                if routes.parent_send { "" } else { "!" },
                if routes.sends { ">" } else { "==" },
                if routes.receives { ">" } else { "==" },
            );
            for at in STATES {
                let d = art::routing(
                    &p.chrome,
                    art::Axis::Horizontal,
                    routes,
                    crate::overlay::route_ink(p),
                    at,
                );
                self.drawing(
                    4,
                    &d,
                    &Place {
                        name: &format!("Routing {bits} {at:?}"),
                        x: &n(f64::from(gt::ROUTING_X)),
                        y: FT,
                        visible: &format!("{knob} && {when} && {}", state("routing", at)),
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
        for inverted in [false, true] {
            for at in STATES {
                let d = art::phase(&p.chrome, inverted, at);
                self.drawing(
                    4,
                    &d,
                    &Place {
                        name: &format!("Phase {inverted} {at:?}"),
                        x: &n(f64::from(gt::TINT_W) + f64::from(gt::GUTTER_BUTTON_X) + 3.0),
                        y: &format!("h - {}", n(f64::from(gt::PHASE_FROM_FLOOR))),
                        visible: &format!(
                            "h >= {} && {}track.phase && {}",
                            n(f64::from(gt::PHASE_HIDE_H)),
                            if inverted { "" } else { "!" },
                            state("phase", at)
                        ),
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
    }

    /// The record arm on the name field's left end, on rows that show
    /// knobs.
    fn tcp_arm(&mut self, knob: &str, field_x: f64) {
        let p = self.palette;
        for armed in [false, true] {
            for at in STATES {
                let d = art::record_arm(
                    &p.chrome,
                    crate::tcp::lit(p).rec,
                    armed,
                    at,
                    art::Arm::Panel,
                    to_theme(p.tcp_field),
                );
                self.drawing(
                    4,
                    &d,
                    &Place {
                        name: &format!("Arm {armed} {at:?}"),
                        x: &format!("{} + row.indent", n(field_x + 3.0)),
                        y: &format!("{FT} + ({FH} - 20) / 2"),
                        visible: &format!(
                            "{knob} && ui.recording && {}track.armed && {}",
                            if armed { "" } else { "!" },
                            state("arm", at)
                        ),
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
    }

    /// Volume: a knob scaled down to a short field; a row too short for
    /// one shows no volume at all — a bar there was too fine to read or
    /// to grab. The value is `track.fader`.
    fn tcp_volume(&mut self, knob: &str, volume_x: f64) {
        let p = self.palette;
        let lit = crate::tcp::lit(p).volume;
        let scale = format!("min({FH} / 22, 1)");
        // The knob: what does not move once, each state's face, the value
        // arc as far as the volume goes.
        let rest = art::volume_knob(&p.chrome, lit, 0.0, Interaction::Normal, 24.0);
        self.line(
            4,
            &format!(
                "<Frame name=\"Volume knob\" w=24 h=24 bind:x={} bind:y={} bind:scale={} bind:visible={}>",
                q(&format!("{} - 12 * {scale}", n(volume_x))),
                q(FT),
                q(&scale),
                q(knob)
            ),
        );
        let d = Drawing {
            w: rest.w,
            h: rest.h,
            ops: Vec::new(),
        };
        for op in rest.ops.iter().take(3) {
            self.op(5, &d, op, None, "");
        }
        for at in STATES {
            let face = art::volume_knob(&p.chrome, lit, 0.0, at, 24.0);
            self.line(
                5,
                &format!(
                    "<Frame name=\"Face {at:?}\" w=24 h=24 bind:visible={}>",
                    q(&state("volume", at))
                ),
            );
            for op in face.ops.iter().skip(3).take(2) {
                self.op(6, &d, op, None, "");
            }
            self.line(5, "</Frame>");
        }
        let start = f64::from(daw_theme_art::vector_controls::VOLUME_KNOB_START);
        let sweep = f64::from(daw_theme_art::vector_controls::VOLUME_KNOB_SWEEP);
        self.line(
            5,
            &format!(
                "<Vector name=\"Value\" w=24 h=24 geometry=\"M 0 0\" stroke=\"{}\" stroke-weight=3.5 stroke-join=\"round\" bind:geometry={} bind:visible=\"track.fader > 0\" />",
                hex(lit),
                q(&format!("arc(12, 12, 9, {}, {} * clamp(track.fader, 0, 1))", n(start), n(sweep)))
            ),
        );
        self.line(4, "</Frame>");
    }

    /// Pan: the mixer's knob scaled to the field; none on a row too short
    /// for it.
    fn tcp_pan(&mut self, knob: &str) {
        let scale = format!("min({FH} / 25, 1)");
        self.line(
            4,
            &format!(
                "<Frame name=\"Pan\" w=24 h=25 bind:x=\"{}\" bind:y={} bind:scale={} bind:visible={}>",
                n(f64::from(gt::PAN_KNOB_X)),
                q(&format!("{FT} + ({FH} - 25 * {scale}) / 2")),
                q(&scale),
                q(knob)
            ),
        );
        self.pan_knob(5);
        self.line(4, "</Frame>");
    }

    /// The hit targets, `Row::control_at`'s order reversed (last on top).
    fn tcp_hits(&mut self, knob: &str, field_x: f64, volume_x: f64) {
        let tint_w = f64::from(gt::TINT_W);
        let button_h = format!("min({FH}, 20)");
        let button_y = format!("{FT} + ({FH} - {button_h}) / 2");
        let volume_w = format!("({knob} ? 24 * min({FH} / 22, 1) : 24)");
        let rows: [(&str, String, String, String, String, String); 10] = [
            (
                "name",
                "58 + row.indent".into(),
                FT.into(),
                format!("max({} - (58 + row.indent), 0)", n(volume_x - 14.0)),
                FH.into(),
                String::new(),
            ),
            (
                "folder",
                "row.indent".into(),
                "0".into(),
                "20".into(),
                "h".into(),
                String::new(),
            ),
            (
                "fx",
                n(f64::from(gt::FX_IN_X)),
                FT.into(),
                "36".into(),
                FH.into(),
                knob.into(),
            ),
            (
                "routing",
                n(f64::from(gt::ROUTING_X)),
                FT.into(),
                "26".into(),
                FH.into(),
                knob.into(),
            ),
            (
                "pan",
                n(f64::from(gt::PAN_KNOB_X)),
                FT.into(),
                "25".into(),
                FH.into(),
                knob.into(),
            ),
            (
                "volume",
                format!("{} - {volume_w} / 2", n(volume_x)),
                FT.into(),
                volume_w.clone(),
                FH.into(),
                knob.into(),
            ),
            (
                "arm",
                format!("{} + row.indent", n(field_x + 3.0)),
                format!("{FT} + ({FH} - 20) / 2"),
                "20".into(),
                "20".into(),
                format!("{knob} && ui.recording"),
            ),
            (
                "solo",
                n(tint_w + 24.0),
                button_y.clone(),
                "21".into(),
                button_h.clone(),
                String::new(),
            ),
            (
                "mute",
                n(tint_w + 2.0),
                button_y,
                "21".into(),
                button_h,
                String::new(),
            ),
            (
                "phase",
                n(tint_w + f64::from(gt::GUTTER_BUTTON_X) + 3.0),
                format!("h - {}", n(f64::from(gt::PHASE_FROM_FLOOR))),
                n(f64::from(daw_theme_art::geometry::mcp::PHASE_W)),
                n(f64::from(daw_theme_art::geometry::mcp::PHASE_W)),
                format!("h >= {}", n(f64::from(gt::PHASE_HIDE_H))),
            ),
        ];
        for (name, x, y, w, h, visible) in rows {
            let mut a = format!(
                "name=\"@{name}\" w=10 h=10 bind:x={} bind:y={} bind:w={} bind:h={}",
                q(&x),
                q(&y),
                q(&w),
                q(&h)
            );
            if !visible.is_empty() {
                let _ = write!(a, " bind:visible={}", q(&visible));
            }
            self.line(4, &format!("<Frame {a} />"));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::row::{Control, Row};
    use crate::tcp::Tcp;

    /// The theme's targets are `Row::rect`'s, for every control, at every
    /// row height from the band up and at every depth of folder: the row
    /// is clicked where the painters say it is.
    #[test]
    fn the_theme_lays_the_row_out_as_row_does() {
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let theme = theme_runtime::Theme::from_markup(
            &super::super::fts_dark(&palette),
            crate::skin::schema(),
            None,
        )
        .unwrap_or_else(|e| {
            panic!(
                "{}",
                e.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        let width = f64::from(daw_theme_art::geometry::tcp::ROW_W);
        let mut checked = 0;
        for depth in 0..8 {
            let mut h = 4.0;
            while h <= 300.0 {
                let track = daw_proto::Track::default();
                let fields = crate::skin::row_fields(
                    &palette,
                    &track,
                    crate::skin::RowShown {
                        depth,
                        height: h,
                        ..Default::default()
                    },
                );
                let row = Row::new(0.0, h, depth, false, Tcp::FULL);
                for control in [
                    Control::Mute,
                    Control::Solo,
                    Control::RecArm,
                    Control::Volume,
                    Control::Pan,
                    Control::Routing,
                    Control::Fx,
                    Control::Name,
                    Control::Folder,
                    Control::Phase,
                ] {
                    let name = crate::skin::row_name(control);
                    let themed = theme.target_rect(super::ROW, (width, h), &fields, name);
                    let painted = row.rect(control);
                    match (themed, painted) {
                        (Some(t), Some(p)) => {
                            let off = (t.x0 - p.x0).abs()
                                + (t.y0 - p.y0).abs()
                                + (t.x1 - p.x1).abs()
                                + (t.y1 - p.y1).abs();
                            assert!(
                                off < 1e-6,
                                "{name} at h {h} depth {depth}: theme {t:?}, row {p:?}"
                            );
                        }
                        (None, None) => {}
                        (t, p) => panic!("{name} at h {h} depth {depth}: theme {t:?}, row {p:?}"),
                    }
                    checked += 1;
                }
                h += if h < 80.0 { 0.5 } else { 7.0 };
            }
        }
        assert!(checked > 5000, "{checked}");
    }
}
