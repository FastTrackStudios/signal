//! The compressors' sidechain EQ.
//!
//! Every compressor face (rack, `-lane`, `-tall`) carries an **SC** key — a
//! latching console key (`button.key`) whose lamp lights yellow while it is
//! on — bound to `<ns>/sc_view` (toggle, off), `<ns>` the face's own
//! namespace (`fet`, `opto`, `optocomp`, `twinvalve`, `varimu`, `buscomp`,
//! `vcacomp`, `charcomp`). A host shows the sidechain EQ in the unit's place
//! while it is on.
//!
//! The sidechain EQ is its own face, `00-sc-eq`: `viz.eq_graph` over
//! namespace `sc` (`sc/b{n}_{used,on,freq,gain,q,shape,slope,name,
//! placement,focus,dyn_range}`, n = 1..=8, every shape — the sidechain is
//! never inverted, so the cuts are what it is for; band 1 starts as a
//! 12 dB/oct high-pass at 90 Hz, the rest unused), its filters drawn in
//! solid yellow; a title row over it — `sc/title` (a text param: the host
//! names the unit, "FET 1176 · Sidechain") at the left, and the SC key on
//! `sc/sc_view` (on, so lit: pressing it goes back to the compressor) at
//! the right.

use std::collections::BTreeMap;

use frame::document::proto::{NodeId, NodeKind, ResizeX, ResizeY, TextAlign};
use frame::surface::kit::{Place, draw};
use frame::surface::proto::{Binding, Control, ControlKind, ParamInfo, SlotRole};
use frame::surface::widgets::{Builder, Theme};

use crate::delay::parts;
use crate::face::{Face, canvas};

/// The sidechain's colour: its filters, its glow, the SC lamps.
pub const YELLOW: &str = "#facc15";
/// The sidechain EQ's namespace and band count.
pub const NS: &str = "sc";
pub const BANDS: usize = 8;

/// `<ns>/sc_view`: on, the host shows the sidechain EQ.
#[must_use]
pub fn view(ns: &str) -> String {
    format!("{ns}/sc_view")
}

fn view_param(ns: &str, on: bool) -> ParamInfo {
    parts::toggle(&view(ns), "Sidechain View", on).short("SC")
}

/// Where a face's SC key goes: its centre, its cap's width and finish —
/// a clean spot on each plate, nothing moved.
fn spot(file: &str) -> Option<((f64, f64), f64, &'static str)> {
    Some(match file {
        // FET: rack 1120 × 204, lane 760 × 164, tall 760 × 328.
        "01-fet-black" => ((462.0, 172.0), 40.0, "black"),
        "02-fet-silver" => ((478.0, 170.0), 40.0, "black"),
        "03-fet-late" => ((472.0, 172.0), 40.0, "black"),
        "01-fet-black-lane" | "02-fet-silver-lane" | "03-fet-late-lane" => {
            ((351.0, 19.0), 36.0, "black")
        }
        // Clear of the ratio legends on the black one; the silver and the
        // late have their power switch there.
        "01-fet-black-tall" => ((150.0, 128.0), 40.0, "black"),
        "02-fet-silver-tall" | "03-fet-late-tall" => ((199.0, 128.0), 40.0, "black"),
        // Opto leveler: rack 906 × 258.
        "04-opto-leveler" => ((438.0, 224.0), 40.0, "grey"),
        "04-opto-leveler-lane" => ((172.0, 100.0), 36.0, "grey"),
        "04-opto-leveler-tall" => ((690.0, 182.0), 40.0, "grey"),
        // Opto comp: rack 1000 × 270.
        "05-opto-comp" => ((735.0, 152.0), 40.0, "black"),
        "05-opto-comp-lane" => ((354.0, 20.0), 36.0, "black"),
        "05-opto-comp-tall" => ((700.0, 172.0), 40.0, "black"),
        // Twin valve: rack 1060 × 410.
        "06-twin-valve" => ((230.0, 320.0), 44.0, "black"),
        "06-twin-valve-lane" => ((650.0, 130.0), 36.0, "black"),
        "06-twin-valve-tall" => ((600.0, 140.0), 40.0, "black"),
        // Vari-mu: rack 1100 × 200.
        "07-vari-mu" => ((44.0, 100.0), 34.0, "black"),
        "07-vari-mu-lane" => ((705.0, 143.0), 34.0, "black"),
        "07-vari-mu-tall" => ((380.0, 108.0), 40.0, "black"),
        // Bus comp: rack 1480 × 130 — by its external side-chain key.
        "08-bus-comp" => ((1084.0, 112.0), 30.0, "grey"),
        "08-bus-comp-lane" => ((465.0, 146.0), 30.0, "grey"),
        "08-bus-comp-tall" => ((530.0, 152.0), 40.0, "grey"),
        // VCA: rack 860 × 360.
        "09-vca-comp" => ((514.0, 292.0), 40.0, "cream"),
        "09-vca-comp-lane" => ((478.0, 128.0), 36.0, "cream"),
        "09-vca-comp-tall" => ((480.0, 262.0), 40.0, "cream"),
        // Character: rack 1500 × 136.
        "10-character-comp" => ((140.0, 105.0), 40.0, "black"),
        "10-character-comp-lane" => ((700.0, 25.0), 36.0, "black"),
        "10-character-comp-tall" => ((674.0, 298.0), 40.0, "black"),
        _ => return None,
    })
}

/// The SC key on `ns`'s view at (`cx`, `cy`) of `parent`, `w` wide.
fn key(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    (cx, cy): (f64, f64),
    w: f64,
    variant: &str,
) -> NodeId {
    let opts = [
        format!("h={}", (w * 0.36).round()),
        format!("color={YELLOW}"),
    ];
    let p = Place {
        options: Some(opts.to_vec()),
        ..Place::new("SC")
            .size(w)
            .variant(variant)
            .legend("SC")
            .bind(parts::bind(&view(ns)))
    };
    parts::put(b, parent, "button.key", &p, cx, cy)
}

/// Put the SC key on comp face `f` (its plate, at its spot) and its param
/// in the face's params. Faces without a spot are left alone.
///
/// # Panics
/// If the face's top frame has no plate (every comp face has one).
pub fn add_key(f: &mut Face) {
    let Some((at, w, variant)) = spot(f.file) else {
        return;
    };
    let ns = f
        .params
        .first()
        .and_then(|p| p.addr.0.as_str().split('/').next().map(str::to_string))
        .expect("comp face params");
    let scene = &mut f.scene;
    let page = scene.pages()[0];
    let top = scene
        .node(page)
        .and_then(|p| p.children.first().copied())
        .expect("top frame");
    let plate = scene
        .node(top)
        .and_then(|t| t.children.first().copied())
        .expect("plate");
    let theme = Theme::stage_dark();
    let mut controls = BTreeMap::new();
    {
        let mut b = Builder {
            scene,
            controls: &mut controls,
            theme: &theme,
        };
        key(&mut b, plate, &ns, at, w, variant);
    }
    for (id, c) in controls {
        if let Some(n) = scene.node_mut(id) {
            n.control = Some(c);
        }
    }
    f.params.push(view_param(&ns, false));
}

// ── the sidechain EQ face ────────────────────────────────────────────────

const W: f64 = 760.0;
const H: f64 = 328.0;
/// The title row's height.
const ROW: f64 = 30.0;

fn pin(b: &mut Builder<'_>, id: NodeId, x: ResizeX, y: ResizeY) {
    if let Some(n) = b.scene.node_mut(id) {
        n.layout_child.resize_x = Some(x);
        n.layout_child.resize_y = Some(y);
    }
}

/// The band params: signal's EQ family over `sc`, band 1 a high-pass.
fn params() -> Vec<ParamInfo> {
    let mut v = frame::surface::kit::viz::catalog::eq_params(NS, BANDS);
    let shapes = frame::surface::kit::viz::catalog::SHAPES.len() as f64 - 1.0;
    for p in &mut v {
        let Some(k) = p.addr.0.as_str().strip_prefix("sc/b") else {
            continue;
        };
        let (n, field) = k.split_once('_').unwrap_or(("", ""));
        match (n, field) {
            ("1", "used") => p.default = 1.0,
            (_, "used") => p.default = 0.0,
            ("1", "shape") => p.default = 3.0 / shapes,
            ("1", "freq") => *p = p.clone().with_default_real(90.0),
            ("1", "gain") => *p = p.clone().with_default_real(0.0),
            _ => {}
        }
    }
    v.extend(frame::surface::kit::viz::catalog::eq_view_params(NS));
    v.push(
        ParamInfo::new(
            format!("{NS}/title"),
            "Title",
            frame::surface::proto::Range::default(),
        )
        .text()
        .short("Title"),
    );
    v.push(view_param(NS, true));
    v
}

/// The sidechain EQ: a slim title row over the graph, 760 × 328, every
/// part pinned so the face stretches to its column (the graph takes the
/// height).
#[must_use]
pub fn face() -> Face {
    let scene = canvas("Sidechain EQ", |b, root| {
        if let Some(n) = b.scene.node_mut(root) {
            n.size = frame::document::proto::Size {
                width: W,
                height: H,
            };
        }
        let mut bg = draw::frame("Sidechain panel", 0.0, 0.0, W, H);
        bg.fills = vec![draw::linear(180.0, &[(0.0, "#17171a"), (1.0, "#0e0e10")])];
        bg.clips_content = true;
        let bg = b.add(root, draw::stroked(draw::rounded(bg, 6.0), "#2a2a2e", 1.0));
        pin(b, bg, ResizeX::Stretch, ResizeY::Stretch);

        // The title: the host's name for the unit, a placeholder until then.
        let field = b.add(bg, draw::frame("Title", 12.0, 0.0, 520.0, ROW));
        let style = |mut n: frame::document::proto::Node| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.font_weight = Some(700.0);
                t.letter_spacing = 0.4;
            }
            n
        };
        let hint = b.add(
            field,
            style(draw::text(
                "placeholder",
                "Sidechain EQ",
                0.0,
                8.0,
                520.0,
                12.0,
                "#facc15b3",
                TextAlign::Left,
            )),
        );
        let value = b.add(
            field,
            style(draw::text(
                "value",
                "",
                0.0,
                8.0,
                520.0,
                12.0,
                YELLOW,
                TextAlign::Left,
            )),
        );
        let caret = b.add(
            field,
            draw::rect("caret", 0.0, 8.0, 1.5, 14.0, draw::solid("#00000000")),
        );
        let c = Control::new(ControlKind::TextField {
            click_to_edit: false,
            max_len: 48,
        })
        .bound(Binding::param(format!("{NS}/title")))
        .slot(SlotRole::Value, value.0)
        .slot(SlotRole::Placeholder, hint.0)
        .slot(SlotRole::Caret, caret.0);
        b.controls.insert(field, c);
        pin(b, field, ResizeX::Left, ResizeY::Top);

        // The SC key, right of the title: lit while this face shows.
        let k = parts::put_at(
            b,
            bg,
            "button.key",
            &Place {
                options: Some(vec!["h=14".into(), format!("color={YELLOW}")]),
                ..Place::new("SC")
                    .size(40.0)
                    .variant("black")
                    .legend("SC")
                    .bind(parts::bind(&view(NS)))
            },
            0.0,
            0.0,
        );
        let (kw, kh) = b
            .scene
            .node(k)
            .map_or((40.0, 14.0), |n| (n.size.width, n.size.height));
        if let Some(n) = b.scene.node_mut(k) {
            n.transform =
                frame::document::proto::Transform::translate(W - 10.0 - kw, (ROW - kh) / 2.0);
        }
        pin(b, k, ResizeX::Right, ResizeY::Top);

        // The graph, filling the rest.
        let inset = 4.0;
        let (gw, gh) = (W - 2.0 * inset, H - ROW - inset);
        let opts = [
            format!("ns={NS}"),
            format!("bands={BANDS}"),
            format!("tint={YELLOW}"),
            format!("h={gh}"),
        ];
        let p = Place {
            options: Some(opts.to_vec()),
            ..Place::new("Sidechain").size(gw)
        };
        let g = parts::put_at(b, bg, "viz.eq_graph", &p, inset, ROW);
        pin(b, g, ResizeX::Stretch, ResizeY::Stretch);
        solid_filters(b, g);
    });
    Face {
        file: "00-sc-eq",
        plugin: "comp",
        scene,
        params: params(),
        streams: Vec::new(),
    }
}

/// The filters solid: each band's area and the sum's well filled, not the
/// graph's faint wash.
fn solid_filters(b: &mut Builder<'_>, graph: NodeId) {
    for id in b.scene.walk(graph) {
        let Some(n) = b.scene.node(id) else { continue };
        if n.name != "fill" {
            continue;
        }
        let holder = n
            .parent
            .and_then(|p| b.scene.node(p))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let fill = match holder.as_str() {
            // White, tinted by the band's colour (yellow).
            "curve" => "#ffffff73",
            "response" => "#facc1540",
            _ => continue,
        };
        if let Some(n) = b.scene.node_mut(id) {
            n.fills = vec![draw::solid(fill)];
        }
    }
}
