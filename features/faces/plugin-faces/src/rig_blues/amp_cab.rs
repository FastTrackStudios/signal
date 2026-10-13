//! The Amp/Cab section's units: the clean head's matching 2×12 cab, which
//! the head stands on, with a mic on each speaker to drag; the gate pedal
//! that sits in front of them.

use document_proto::NodeId;
use surface_kit::Place;
use surface_kit::draw::{self, Lettering, family};
use surface_kit::materials::cabinet::{self, CabinetSpec, Handle, Kind};
use surface_kit::materials::light;
use surface_kit::materials::paint::{Cloth, Grain, Material};
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::PLUGIN;
use super::drives::H as PEDAL_H;
use crate::comp_tube::ui as place;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_studio_blues::cab as studio_cab;
use crate::suite_studio_blues::parts::{self, set};

/// The cab's size: the head's width, a 2×12's height.
pub const CAB_W: f64 = 1200.0;
pub const CAB_H: f64 = 700.0;

/// The 2×12 under the clean head: the same bronco tolex, black corners
/// and white piping, black cloth over two speakers, a mic on each — a
/// ribbon on the left speaker, a dynamic on the right — dragged across the
/// cone (position) and back from it (distance). No faceplate: a cab has
/// no controls of its own; the section's rail carries the mics' levels.
#[must_use]
pub fn cab() -> Face {
    cab_sized(CAB_W, CAB_H)
}

/// The cab at `w × h` (a row sets it beside the head at the head's height).
#[must_use]
pub fn cab_sized(cab_w: f64, cab_h: f64) -> Face {
    let scene = canvas("Cab 2x12", |b, root| {
        ui::fit_root(b, root, cab_w, cab_h);
        let mut spec = CabinetSpec::new(Kind::Combo, cab_w, cab_h);
        spec.covering = Material::Tolex {
            grain: Grain::Bronco,
            color: "#1d1c1c".into(),
            scale: 1.0,
        };
        spec.grille = Material::GrilleCloth {
            cloth: Cloth::Black,
        };
        spec.face_frac = 0.0;
        spec.piping = Some("#b9babc".into());
        spec.corners = Some("black".into());
        spec.handle = Handle::None;
        let c = cabinet::cabinet(b, root, "Cabinet", &spec);
        let (gw, gh) = b
            .scene
            .node(c.grille)
            .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
        // Two speakers side by side behind the cloth, a mic on each.
        let s = (gw / 2.0).min(gh) * 0.96;
        for (i, left) in [(0_u8, true), (1, false)] {
            let cx = gw * (0.25 + 0.5 * f64::from(i));
            let mut spk = draw::frame(
                &format!("Speaker {}", i + 1),
                cx - s / 2.0,
                (gh - s) / 2.0,
                s,
                s,
            );
            spk.clips_content = false;
            let spk = b.add(c.grille, spk);
            let r = s * 0.4;
            b.add(
                spk,
                draw::circle(
                    "cone",
                    s / 2.0,
                    s / 2.0,
                    r,
                    draw::radial(&[
                        (0.0, "#ffffff12"),
                        (0.3, "#ffffff08"),
                        (0.62, "#00000038"),
                        (0.66, "#ffffff10"),
                        (0.7, "#00000000"),
                        (1.0, "#00000000"),
                    ]),
                ),
            );
            b.add(
                spk,
                draw::stroked(
                    draw::circle("rim", s / 2.0, s / 2.0, r, draw::solid("#00000000")),
                    "#ffffff16",
                    2.0,
                ),
            );
            studio_cab::mic_handle(b, spk, i + 1, left, (s, s));
        }
        // The logo plate, low on the cloth.
        let plate = draw::rounded(
            draw::rect(
                "logo plate",
                gw / 2.0 - 70.0,
                gh - 46.0,
                140.0,
                26.0,
                draw::linear(180.0, &[(0.0, "#d8d9db"), (1.0, "#8e9094")]),
            ),
            3.0,
        );
        let plate = b.add(c.grille, draw::with_effects(plate, light::stand(2.0, 1.0)));
        let slab = Lettering::family(family::PLAYFAIR_DISPLAY)
            .weight(900.0)
            .italic();
        set(
            b,
            plate,
            "OPEN ROOM",
            70.0,
            13.0,
            14.0,
            "#0e0e0e",
            &slab,
            0.02,
        );
    });
    let params = studio_cab::cab().params;
    Face {
        file: "21-cab-2x12",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The gate: a small black box with a threshold dial and a release
/// toggle, green LED. Micro, 110 × 286.
#[must_use]
pub fn gate() -> Face {
    const W: f64 = 110.0;
    const INK: &str = "#e6e6e8";
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, W, PEDAL_H);
        let opts = vec![
            format!("w={W}"),
            format!("h={PEDAL_H}"),
            "color=#26272a".into(),
            "finish=powder".into(),
            "jack_inset=8".into(),
        ];
        let p = Place {
            options: Some(opts),
            ..Place::new("Enclosure").variant("micro")
        };
        let enc = ui::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0);
        let lid = b
            .scene
            .node(enc)
            .and_then(|n| {
                n.children
                    .iter()
                    .copied()
                    .find(|c| b.scene.node(*c).is_some_and(|c| c.name == "lid"))
            })
            .expect("lid");
        let (lw, lh) = b
            .scene
            .node(lid)
            .map_or((W, PEDAL_H), |n| (n.size.width, n.size.height));
        let k = Place::new("Threshold")
            .legend(" ")
            .size(52.0)
            .variant("knurled_dome")
            .options(&["touch=44", "ticks=none"])
            .bind(ui::bind("gate/threshold"));
        place::centred_at(b, lid, "knob.3d", &k, lw / 2.0, lh * 0.16);
        parts::print(
            b,
            lid,
            "THRESHOLD",
            lw / 2.0,
            lh * 0.29,
            6.5,
            INK,
            700.0,
            0.08,
        );
        parts::print(b, lid, "FAST", lw / 2.0, lh * 0.36, 6.0, INK, 600.0, 0.06);
        parts::toggle(b, lid, "gate/release", "Release", 12.0, lw / 2.0, lh * 0.42);
        parts::print(b, lid, "SLOW", lw / 2.0, lh * 0.48, 6.0, INK, 600.0, 0.06);
        set(
            b,
            lid,
            "GATE",
            lw / 2.0,
            lh * 0.6,
            20.0,
            INK,
            &Lettering::family(family::MICHROMA),
            0.1,
        );
        parts::stomp(
            b,
            lid,
            "gate/on",
            "Bypass",
            24.0,
            lw * 0.56,
            lh * 0.8,
            "#38e060",
            true,
        );
    });
    let params = vec![
        ui::param(
            "gate/threshold",
            "Threshold",
            Range::linear(-90.0, 0.0, Unit::Db),
            -60.0,
        )
        .featured(),
        ui::choice("gate/release", "Release", &["Slow", "Fast"], 1),
        ui::toggle("gate/on", "Bypass", true).featured(),
    ];
    Face {
        file: "22-gate",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The mixing rail under the cab: each mic's level and pan, the room
/// send and level, phase, laid out in `rows` rows. Returns the rail frame.
pub fn cab_rail(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w): (f64, f64, f64),
    rows: usize,
) -> NodeId {
    const INK: &str = "#e8e6e1";
    const ROW_H: f64 = 108.0;
    let items: [(&str, &str, bool); 7] = [
        ("cab/m1_level", "MIC 1", true),
        ("cab/m1_pan", "PAN 1", true),
        ("cab/m2_level", "MIC 2", true),
        ("cab/m2_pan", "PAN 2", true),
        ("cab/room_level", "ROOM", true),
        ("cab/room_on", "ROOM SEND", false),
        ("cab/phase", "PHASE Ø", false),
    ];
    let rows = rows.max(1);
    let cols = items.len().div_ceil(rows);
    let mut rail = draw::frame("Cab rail", x, y, w, ROW_H * rows as f64);
    rail.fills = vec![draw::linear(180.0, &[(0.0, "#232428"), (1.0, "#16171a")])];
    let rail = b.add(parent, draw::rounded(rail, 10.0));
    let step = w / cols as f64;
    let (kd, ls) = ((step * 0.78).min(50.0), (step / 9.0).min(10.0));
    for (i, (addr, name, is_knob)) in items.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        let cx = step * (col as f64 + 0.5);
        let cy = ROW_H * row as f64 + 44.0;
        if *is_knob {
            let p = Place::new(*name)
                .legend(" ")
                .size(kd)
                .variant("spun_metal")
                .options(&["skirt=knurled", "touch=44", "ticks=none"])
                .bind(ui::bind(addr));
            place::centred_at(b, rail, "knob.3d", &p, cx, cy);
        } else {
            let p = Place::new(*name)
                .size(26.0)
                .variant("red")
                .bind(ui::bind(addr));
            place::centred_at(b, rail, "switch.mini_slide", &p, cx, cy);
        }
        parts::print(b, rail, name, cx, cy + 46.0, ls, INK, 650.0, 0.08);
    }
    rail
}
