//! An embed derived from any catalog: its featured params and meters.
//!
//! Every module gets an embed for free, the way it gets a generated
//! surface (`surface_widgets::surface_for_catalog`): the catalog's
//! featured params (or its first continuous ones) as small arc knobs with
//! their values, its level and gain-reduction feeds as bars, its bypass as
//! a lamp — laid out by breakpoint:
//!
//! | box | shows |
//! |---|---|
//! | narrow (< 60 px) or short (< 40 px) | the first meter, else the hero's value |
//! | small | a meter and up to 2 knobs |
//! | tall (≥ 140 px) or wide (TCP) | a meter and up to 4 knobs |

use document_proto::{NodeId, TextAlign};
use surface_proto::{
    Binding, ButtonMode, Catalog, Control, ControlKind, FeedKind, ParamInfo, ParamKind, SlotRole,
    ValueFormat,
};
use surface_widgets::sector_path;

use crate::embed::{BuildCtx, Built, EmbedView};
use crate::tone::style::{self as s, alpha};
use crate::tone::{Kit, Rect4};

/// See the module docs.
#[derive(Debug, Clone)]
pub struct FeaturedView {
    pub catalog: Catalog,
    /// Knob ink.
    pub accent: String,
}

impl FeaturedView {
    #[must_use]
    pub fn new(catalog: Catalog) -> Self {
        Self {
            catalog,
            accent: "#38bdf8".into(),
        }
    }

    /// The params worth a knob, most important first.
    #[must_use]
    pub fn knobs(&self) -> Vec<&ParamInfo> {
        let cont = |p: &&ParamInfo| {
            matches!(p.kind, ParamKind::Continuous) && !p.addr.0.ends_with("bypass")
        };
        let featured: Vec<&ParamInfo> = self
            .catalog
            .params
            .iter()
            .filter(|p| p.featured)
            .filter(cont)
            .collect();
        if featured.is_empty() {
            self.catalog.params.iter().filter(cont).take(4).collect()
        } else {
            featured
        }
    }

    /// The feed a bar shows: a gain reduction first, then a level.
    #[must_use]
    pub fn meter(&self) -> Option<(String, bool)> {
        let find = |gr: bool| {
            self.catalog.streams.iter().find(|f| {
                if gr {
                    matches!(f.kind, FeedKind::GainReduction)
                } else {
                    matches!(f.kind, FeedKind::Level { .. })
                }
            })
        };
        find(true)
            .map(|f| (f.id.0.clone(), true))
            .or_else(|| find(false).map(|f| (f.id.0.clone(), false)))
    }

    fn bypass(&self) -> Option<&ParamInfo> {
        self.catalog.params.iter().find(|p| {
            matches!(p.kind, ParamKind::Toggle)
                && (p.addr.0.ends_with("bypass") || p.addr.0.ends_with("/on"))
        })
    }
}

impl EmbedView for FeaturedView {
    fn catalog(&self) -> Catalog {
        self.catalog.clone()
    }

    fn build(&self, ctx: &BuildCtx) -> Built {
        let (w, h) = ctx.size;
        let mut k = Kit::new(w, h);
        let root = k.root;
        s::fill(&mut k.b(), root, "ground", (0.0, 0.0, w, h), s::WELL);
        let knobs = self.knobs();
        let meter = self.meter();
        let wide = w > h * 2.5;
        let tiny = w < 60.0 || h < 40.0;
        let budget = if tiny {
            0
        } else if h >= 140.0 || wide {
            4
        } else {
            2
        };
        let mut area = (2.0, 2.0, w - 4.0, h - 4.0);

        // The bypass lamp, top right.
        if let Some(b) = self.bypass()
            && !tiny
        {
            let lamp = s::dot(&mut k.b(), root, "lamp", (w - 7.0, 7.0), 3.0, "#22c55e");
            let hit = s::frame(&mut k.b(), root, "bypass", (w - 14.0, 0.0, 14.0, 14.0));
            k.controls.insert(
                hit,
                Control::new(ControlKind::Button {
                    mode: ButtonMode::Toggle,
                })
                .bound(Binding::param(b.addr.0.clone()))
                .slot(SlotRole::Led, lamp.0),
            );
            area.1 += 12.0;
            area.3 -= 12.0;
        }
        // The meter: a bar down the left (a row along the top when wide).
        if let Some((feed, gr)) = &meter {
            let bar_at = if tiny {
                area
            } else {
                (area.0, area.1, 6.0, area.3)
            };
            bar(&mut k, root, feed, bar_at, *gr);
            if !tiny {
                area.0 += 10.0;
                area.2 -= 10.0;
            }
        }
        if tiny
            && meter.is_none()
            && let Some(p) = knobs.first()
        {
            readout(&mut k, root, p, area, 9.0);
        }
        let n = knobs.len().min(budget);
        if n > 0 {
            let (cols, rows) = if wide {
                (n, 1)
            } else if area.2 >= 90.0 {
                (n.min(2), n.div_ceil(2))
            } else {
                (1, n)
            };
            let cw = area.2 / cols as f64;
            let rh = area.3 / rows as f64;
            for (i, p) in knobs.iter().take(n).enumerate() {
                let (c, r) = (i % cols, i / cols);
                mini_knob(
                    &mut k,
                    root,
                    p,
                    (area.0 + c as f64 * cw, area.1 + r as f64 * rh, cw, rh),
                    &self.accent,
                );
            }
        }
        k.finish()
    }
}

fn bar(k: &mut Kit, p: NodeId, feed: &str, (x, y, w, h): Rect4, gr: bool) {
    s::fill(&mut k.b(), p, "meter well", (x, y, w, h), "#ffffff0d");
    let mut holder = surface_kit::draw::frame("meter", x, y, w, h);
    if gr {
        holder.transform = document_proto::Transform {
            a: -1.0,
            b: 0.0,
            tx: x + w,
            c: 0.0,
            d: -1.0,
            ty: y + h,
        };
    }
    let holder = k.b().add(p, holder);
    let fill = s::fill(
        &mut k.b(),
        holder,
        "fill",
        (0.0, 0.0, w, h),
        if gr { s::REDUCTION_EDGE } else { "#22c55e" },
    );
    let (floor, ceiling) = if gr { (0.0, -24.0) } else { (-60.0, 0.0) };
    let c = Control::new(ControlKind::Gauge {
        sweep_degrees: 0.0,
        floor_db: floor,
        ceiling_db: ceiling,
    })
    .bound(Binding::stream(feed))
    .slot(SlotRole::Fill, fill.0);
    k.controls.insert(holder, c);
}

fn readout(k: &mut Kit, p: NodeId, param: &ParamInfo, (x, y, w, h): Rect4, size: f64) {
    let t = s::text(
        &mut k.b(),
        p,
        "value",
        "",
        (x, y + h / 2.0 + size * 0.35, w),
        size,
        s::TEXT,
        TextAlign::Center,
    );
    let hit = s::frame(&mut k.b(), p, "readout", (x, y, w, h));
    let c = Control::new(ControlKind::Readout)
        .bound(Binding::param(param.addr.0.clone()))
        .slot(SlotRole::Value, t.0)
        .formatted(ValueFormat::default());
    k.controls.insert(hit, c);
}

/// A small arc knob: the arc, its value inside, its name under.
fn mini_knob(k: &mut Kit, p: NodeId, param: &ParamInfo, (x, y, w, h): Rect4, accent: &str) {
    let label_h = if h >= 34.0 { 9.0 } else { 0.0 };
    let d = (w - 4.0).min(h - label_h - 2.0).clamp(12.0, 44.0);
    let root = s::frame(&mut k.b(), p, &param.name, (x, y, w, h));
    let (cx, top) = ((w - d) / 2.0, ((h - label_h - d) / 2.0).max(0.0));
    let dial = s::frame(&mut k.b(), root, "dial", (cx, top, d, d));
    let ring = (d * 0.14).max(2.0);
    let track = sector_path(
        d * 0.5,
        d * 0.5,
        d * 0.5 - ring,
        d * 0.5,
        -135f64.to_radians(),
        135f64.to_radians(),
    );
    s::shape(&mut k.b(), dial, "track", (d, d), track, "#ffffff1f");
    let arc = s::shape(&mut k.b(), dial, "value arc", (d, d), String::new(), accent);
    let size = (d * 0.26).clamp(6.0, 10.0);
    let value = s::text(
        &mut k.b(),
        dial,
        "value",
        "",
        (0.0, d / 2.0 + size * 0.35, d),
        size,
        s::TEXT,
        TextAlign::Center,
    );
    let mut c = Control::new(ControlKind::knob())
        .bound(Binding::param(param.addr.0.clone()))
        .slot(SlotRole::ValueArc, arc.0)
        .slot(SlotRole::Value, value.0);
    c.format = Some(ValueFormat {
        decimals: None,
        unit: Some(String::new()),
        ..ValueFormat::default()
    });
    if label_h > 0.0 {
        let name = param
            .short_name
            .clone()
            .unwrap_or_else(|| param.name.clone());
        s::text(
            &mut k.b(),
            root,
            "label",
            &name,
            (0.0, h - 1.0, w),
            7.0,
            &alpha(s::TEXT_DIM, 1.0),
            TextAlign::Center,
        );
    }
    k.controls.insert(root, c);
}
