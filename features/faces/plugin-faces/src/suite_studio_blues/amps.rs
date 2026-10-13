//! The three amps, drawn straight on as their product renders show them —
//! a tolex box with lit edges, metal corners, a handle and feet, a
//! faceplate standing proud and a grille sat back (`materials::cabinet`) —
//! with their faceplates as the control surface.
//!
//! - `clean-head` — a boutique clean head: an aged brushed-steel
//!   faceplate, chrome caps on ribbed skirts, red slide switches, a heavy
//!   slab-serif logo, red-and-green piped diamond grille cloth, black
//!   corners. Head, 1200 × 548.
//! - `lead-head` — a lead head: black faceplate, black bakelite
//!   chicken-heads under mini toggles, a script logo with its model in gold
//!   marker, black basketweave behind gold piping, a black handle. Head,
//!   1200 × 548.
//! - `silk-combo` — a two-channel '60s combo in brown tolex: a black
//!   faceplate in a silver frame, black skirted knobs numbered round the
//!   dial, a script logo, wheat-and-oxblood cloth. Combo, 1000 × 753.

use frame::document::proto::{Node, NodeId};
use frame::surface::kit::draw::{self, Lettering, family};
use frame::surface::kit::materials::cabinet::{self, CabinetSpec, Handle, Kind};
use frame::surface::kit::materials::light;
use frame::surface::kit::materials::paint::{Cloth, Dir, Grain, Material};
use frame::surface::kit::{Place, TOUCH_MIN};
use frame::surface::proto::{Binding, Range, Unit};
use frame::surface::widgets::Builder;

use super::parts::{self, Cap, PLUGIN, knob, knob_scaled, print, set};
use crate::face::{Face, canvas};
use crate::reverb::ui;

/// Build `spec` into `root` (sized to it); returns the faceplate and the
/// grille frames and the faceplate's size.
fn build(b: &mut Builder<'_>, root: NodeId, spec: &CabinetSpec) -> (NodeId, NodeId, (f64, f64)) {
    ui::fit_root(b, root, spec.w, spec.h);
    let c = cabinet::cabinet(b, root, "Cabinet", spec);
    // These heads' handle brackets are blackened steel, like their corners.
    if spec.handle == Handle::Leather {
        let hh = spec.h * 0.09;
        let bs = hh * 0.55;
        for cx in [spec.w * 0.38 + hh * 0.15, spec.w * 0.62 - hh * 0.15] {
            let br = draw::rounded(
                draw::rect(
                    "bracket",
                    cx - bs * 0.6,
                    hh - bs * 0.55,
                    bs * 1.2,
                    bs * 0.9,
                    draw::linear(
                        light::sheen_angle(),
                        &[(0.0, "#6a6c70"), (0.45, "#2a2b2e"), (1.0, "#111214")],
                    ),
                ),
                bs * 0.25,
            );
            b.add(c.root, br);
        }
    }
    let size = b
        .scene
        .node(c.faceplate)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    (c.faceplate, c.grille, size)
}

/// A cloth (the kit's woven grille) over the whole of `grille`, recessed.
fn cloth(b: &mut Builder<'_>, grille: NodeId, variant: &str, extra: &[&str]) {
    let (w, h) = b
        .scene
        .node(grille)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let mut opts = vec![format!("w={w}"), format!("h={h}"), "r=0".into()];
    opts.extend(extra.iter().map(ToString::to_string));
    let p = Place {
        options: Some(opts),
        ..Place::new("Cloth").variant(variant)
    };
    let g = ui::put_at(b, grille, "panel.amp_grille", &p, 0.0, 0.0);
    // The recess is lit as an edge, not an inner shadow (a big effect layer).
    if let Some(n) = b.scene.node_mut(g) {
        n.effects.clear();
    }
    for e in light::edge("cloth recess", (0.0, 0.0, w, h), 2.0, 10.0, 1.2, false) {
        b.add(grille, e);
    }
}

/// An input jack: a chrome nut round a black bore.
fn jack(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    let (hx, hy) = light::hotspot();
    b.add(
        parent,
        draw::with_effects(
            draw::circle(
                "jack nut",
                cx,
                cy,
                r,
                draw::radial_at(
                    hx,
                    hy,
                    0.9,
                    &[(0.0, "#fafaf8"), (0.5, "#9a9b9e"), (1.0, "#3e3f42")],
                ),
            ),
            light::stand(1.5, 1.0),
        ),
    );
    b.add(
        parent,
        draw::circle(
            "jack bore",
            cx,
            cy,
            r * 0.55,
            draw::radial_at(0.5, 0.4, 0.8, &[(0.0, "#2a2a2c"), (1.0, "#000000")]),
        ),
    );
}

/// A slotted screw head.
fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    frame::surface::kit::materials::enclosure::screw(b, parent, cx, cy, r);
}

/// The pilot/bypass jewel: a kit jewel on `addr`. Its lamp burns steady:
/// a flicker keeps the face (and the whole window around it) redrawing
/// thirty times a second for a wobble nobody sees.
fn jewel(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    variant: &str,
    d: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    let p = Place::new("Power")
        .size(d)
        .variant(variant)
        .bind(Binding::param(addr))
        .legend("")
        .touch(TOUCH_MIN);
    let j = ui::put(b, parent, "button.jewel", &p, cx, cy);
    ui::strip_legend(b, j);
    j
}

/// A mini slide switch on `addr`.
#[allow(clippy::too_many_arguments)]
fn slide(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    variant: &str,
    h: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    let p = Place::new(name)
        .size(h)
        .variant(variant)
        .bind(Binding::param(addr))
        .legend("")
        .touch(TOUCH_MIN);
    ui::put(b, parent, "switch.mini_slide", &p, cx, cy)
}

fn pct(ns: &str, id: &str, name: &str, default: f64) -> frame::surface::proto::ParamInfo {
    ui::param(
        &format!("{ns}/{id}"),
        name,
        Range::linear(0.0, 10.0, Unit::None),
        default,
    )
}

fn with_shadow(n: Node, a: &str) -> Node {
    draw::with_effects(n, vec![draw::drop_shadow(a, 0.0, 1.0, 1.0)])
}

/// The boutique clean head.
pub fn clean_head() -> Face {
    clean_head_with(|_, _| {})
}

/// The clean head with `inside` building into its grille once the head is
/// done (a rig page seats its drive pedals there).
pub fn clean_head_with(inside: impl FnOnce(&mut Builder<'_>, NodeId)) -> Face {
    clean_head_as(548.0, Handle::Leather, 0.29, 52.0, inside)
}

/// The three amps the blues rig's slots choose between, as the rig row
/// draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpModel {
    /// Dumble Steel String Singer #002: the boutique clean head's brushed
    /// steel panel, chrome caps, slab logo, diamond cloth.
    SteelStringSinger,
    /// 1964 Fender Vibroverb (blackface): black tolex, black panel with
    /// white print, numbered black skirts, silver sparkle cloth.
    Vibroverb,
    /// The Two-Rock signature prototype: black panel, bakelite
    /// chicken-heads, silver italic logo, black cloth behind gold piping.
    TwoRock,
    /// Fender Deluxe Reverb (blackface), as the Vibroverb.
    DeluxeReverb,
    /// Dumble Overdrive Special, in the Steel String Singer's dress.
    DumbleOds,
    /// Vox AC30: black tolex, the fawn diamond cloth, the copper panel with
    /// cream pointer knobs, the gold VOX badge.
    Ac30,
    /// Fender Twin Reverb (blackface), as the Vibroverb.
    TwinReverb,
}

impl AmpModel {
    pub const ALL: [Self; 3] = [Self::SteelStringSinger, Self::Vibroverb, Self::TwoRock];
    /// Every model with a face.
    pub const EVERY: [Self; 7] = [
        Self::SteelStringSinger,
        Self::DumbleOds,
        Self::Vibroverb,
        Self::TwoRock,
        Self::DeluxeReverb,
        Self::TwinReverb,
        Self::Ac30,
    ];

    /// Its short name, for pickers.
    #[must_use]
    pub fn short(self) -> &'static str {
        match self {
            Self::SteelStringSinger => "DUMBLE SSS",
            Self::Vibroverb => "'64 VIBROVERB",
            Self::TwoRock => "TWO-ROCK",
            Self::DeluxeReverb => "DELUXE REVERB",
            Self::TwinReverb => "TWIN REVERB",
            Self::DumbleOds => "DUMBLE ODS",
            Self::Ac30 => "VOX AC30",
        }
    }

    /// A blackface Fender.
    #[must_use]
    pub fn blackface(self) -> bool {
        matches!(
            self,
            Self::Vibroverb | Self::DeluxeReverb | Self::TwinReverb
        )
    }

    /// The model name printed under the Fender script.
    fn fender_name(self) -> &'static str {
        match self {
            Self::DeluxeReverb => "DELUXE REVERB",
            Self::TwinReverb => "TWIN REVERB",
            _ => "VIBROVERB",
        }
    }
}

/// An upright amp's panel (`pw × ph`): the maker's mark across the top,
/// GAIN and LEVEL under it as big as the panel allows, their names under
/// them, the pilot jewel at the top right — the same knobs, caps and
/// lettering as the wide panel, stacked for a narrow one.
#[allow(clippy::too_many_arguments)]
fn upright_panel(
    b: &mut Builder<'_>,
    plate: NodeId,
    model: AmpModel,
    prefix: &str,
    (pw, ph): (f64, f64),
    ink: &str,
    cap: Cap,
    legend: &Lettering,
) {
    let big = (pw * 0.16).min(ph * 0.14);
    let (lx, ly) = (pw * 0.46, ph * 0.17);
    match model {
        AmpModel::SteelStringSinger | AmpModel::DumbleOds => {
            let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
            set(
                b,
                plate,
                "Dumble",
                lx,
                ly,
                big * 1.3,
                "#0e0e0e",
                &script,
                0.0,
            );
        }
        AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => {
            let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
            set(
                b,
                plate,
                "Fender",
                lx,
                ly,
                big * 1.3,
                "#f4f4f2",
                &script,
                0.0,
            );
        }
        AmpModel::Ac30 => {
            set(
                b,
                plate,
                "VOX",
                lx,
                ly,
                big * 1.1,
                "#e8c36a",
                &Lettering::family(family::INTER).weight(900.0),
                0.18,
            );
        }
        AmpModel::TwoRock => {
            set(
                b,
                plate,
                "TWO-ROCK",
                lx,
                ly,
                big * 0.8,
                "#d9d9dc",
                &Lettering::family(family::OSWALD).weight(700.0).italic(),
                0.06,
            );
        }
    }
    jewel(
        b,
        plate,
        &format!("{prefix}/on"),
        "red",
        (pw * 0.07).min(ph * 0.08),
        pw * 0.9,
        ly,
    );
    let d = (pw * 0.34).min(ph * 0.42);
    let numbered: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
    let dots: &[&str] = &["", "", "", "", "", "", "", "", "", "", ""];
    for (i, (id, name)) in [("drive", "GAIN"), ("level", "LEVEL")]
        .into_iter()
        .enumerate()
    {
        let addr = format!("{prefix}/{id}");
        let (cx, cy) = (pw * (0.27 + 0.46 * i as f64), ph * 0.56);
        match model {
            AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => {
                knob_scaled(b, plate, cap, &addr, name, d * 0.9, cx, cy, numbered, ink);
            }
            AmpModel::SteelStringSinger | AmpModel::DumbleOds => {
                knob_scaled(b, plate, cap, &addr, name, d * 0.9, cx, cy, dots, ink);
            }
            AmpModel::TwoRock | AmpModel::Ac30 => {
                knob(b, plate, cap, &addr, name, d, cx, cy);
            }
        }
        set(
            b,
            plate,
            name,
            cx,
            ph * 0.9,
            (ph * 0.075).min(pw * 0.06),
            ink,
            legend,
            0.08,
        );
    }
}

/// An amp as a rig row shows it, `w × h`: no handle, and a panel carrying
/// only GAIN and LEVEL (on `<prefix>/gain`, `<prefix>/level`), the logo
/// and the pilot jewel (`<prefix>/on`) over its cloth. The tone stack and
/// effects are the rig's own EQ and effects; the whole amp is the
/// zoomed-in view.
#[must_use]
pub fn amp_compact(model: AmpModel, prefix: &str, w: f64, h: f64) -> Face {
    amp_compact_for(model, prefix, w, h, false)
}

/// [`amp_compact`] for a phone (`phone`): the controls take two thirds of
/// the amp whatever its proportion (the grille a strip), upright for a
/// narrow box, the knobs no bigger than a hand needs.
#[must_use]
pub fn amp_compact_for(model: AmpModel, prefix: &str, w: f64, h: f64, phone: bool) -> Face {
    let scene = canvas(model.short(), |b, root| {
        let mut spec = CabinetSpec::new(
            if model.blackface() || model == AmpModel::Ac30 {
                Kind::Combo
            } else {
                Kind::Head
            },
            w,
            h,
        );
        spec.handle = Handle::None;
        // Upright (a phone's three across): the controls take the most of
        // it, the grille a strip under them.
        let upright = w < h * 1.2;
        spec.face_frac = if upright || phone { 0.66 } else { 0.48 };
        spec.corners = Some("black".into());
        let (ink, cap, legend) = dress(model, &mut spec);
        let (plate, _grille, (pw, ph)) = build(b, root, &spec);
        if upright {
            upright_panel(b, plate, model, prefix, (pw, ph), ink, cap, &legend);
            return;
        }
        let d = (ph * 0.58)
            .min(pw * 0.2)
            .min(if phone { 96.0 } else { f64::MAX });
        let numbered: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
        let dots: &[&str] = &["", "", "", "", "", "", "", "", "", "", ""];
        // Laid out from the knobs' size, not the panel's width: a wider amp
        // keeps its controls together at the left and its badge and jewel
        // at the right, the panel and grille taking the width between, as
        // a wider real one does.
        let gap = (d * 1.75).min(pw * 0.24);
        let first = (d * 0.95 + pw * 0.02).min(pw * 0.16);
        for (i, (id, name)) in [("drive", "GAIN"), ("level", "LEVEL")]
            .into_iter()
            .enumerate()
        {
            let addr = format!("{prefix}/{id}");
            let (cx, cy) = (first + gap * i as f64, ph * 0.43);
            match model {
                AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => {
                    knob_scaled(b, plate, cap, &addr, name, d * 0.9, cx, cy, numbered, ink);
                }
                AmpModel::SteelStringSinger | AmpModel::DumbleOds => {
                    knob_scaled(b, plate, cap, &addr, name, d, cx, cy, dots, ink);
                }
                AmpModel::TwoRock | AmpModel::Ac30 => {
                    knob(b, plate, cap, &addr, name, d, cx, cy);
                }
            }
            let size = (ph * 0.12).min(pw * 0.045);
            set(b, plate, name, cx, ph * 0.88, size, ink, &legend, 0.08);
        }
        let jewel_x = pw - (pw * 0.07).min(d * 0.7);
        let lx = jewel_x - (pw * 0.23).min(d * 2.2);
        let big = (ph * 0.2).min(pw * 0.075);
        match model {
            AmpModel::SteelStringSinger | AmpModel::DumbleOds => {
                let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
                set(
                    b,
                    plate,
                    "Dumble",
                    lx,
                    ph * 0.38,
                    big * 1.4,
                    "#0e0e0e",
                    &script,
                    0.0,
                );
                let sub = if model == AmpModel::DumbleOds {
                    "Overdrive Special"
                } else {
                    "Steel String Singer"
                };
                set(
                    b,
                    plate,
                    sub,
                    lx,
                    ph * 0.72,
                    big * 0.5,
                    "#1a1a1a",
                    &Lettering::family(family::PLAYFAIR_DISPLAY)
                        .weight(700.0)
                        .italic(),
                    0.04,
                );
            }
            AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => {
                let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
                set(
                    b,
                    plate,
                    "Fender",
                    lx,
                    ph * 0.4,
                    big * 1.3,
                    "#f4f4f2",
                    &script,
                    0.0,
                );
                set(
                    b,
                    plate,
                    model.fender_name(),
                    lx,
                    ph * 0.74,
                    big * 0.5,
                    "#e6e6e4",
                    &Lettering::family(family::INTER).weight(700.0),
                    0.2,
                );
            }
            AmpModel::Ac30 => {
                let badge = Lettering::family(family::INTER).weight(900.0);
                set(
                    b,
                    plate,
                    "VOX",
                    lx,
                    ph * 0.4,
                    big * 1.3,
                    "#e8c36a",
                    &badge,
                    0.18,
                );
                set(
                    b,
                    plate,
                    "AC30",
                    lx,
                    ph * 0.74,
                    big * 0.55,
                    "#1c1208",
                    &Lettering::family(family::INTER).weight(800.0),
                    0.2,
                );
            }
            AmpModel::TwoRock => {
                let logo = Lettering::family(family::OSWALD).weight(700.0).italic();
                set(
                    b,
                    plate,
                    "TWO-ROCK",
                    lx,
                    ph * 0.42,
                    big * 0.95,
                    "#d9d9dc",
                    &logo,
                    0.06,
                );
                set(
                    b,
                    plate,
                    "Signature Prototype",
                    lx,
                    ph * 0.72,
                    big * 0.45,
                    "#c9a864",
                    &Lettering::family(family::DANCING_SCRIPT).weight(700.0),
                    0.0,
                );
            }
        }
        jewel(
            b,
            plate,
            &format!("{prefix}/on"),
            "red",
            (ph * 0.22).min(pw * 0.06),
            jewel_x,
            ph * 0.45,
        );
    });
    let params = vec![
        ui::param(
            &format!("{prefix}/drive"),
            "Gain",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        )
        .featured(),
        ui::param(
            &format!("{prefix}/level"),
            "Level",
            Range::linear(-24.0, 12.0, Unit::Db),
            0.0,
        )
        .featured(),
        ui::toggle(&format!("{prefix}/on"), "Power", true),
    ];
    Face {
        file: "07-amp-compact",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

fn clean_head_as(
    h: f64,
    handle: Handle,
    face_frac: f64,
    knob_d: f64,
    inside: impl FnOnce(&mut Builder<'_>, NodeId),
) -> Face {
    const W: f64 = 1200.0;
    const INK: &str = "#141414";
    let big = knob_d / 52.0;
    let scene = canvas("Clean Head", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Head, W, h);
        spec.covering = Material::Tolex {
            grain: Grain::Bronco,
            color: "#1d1c1c".into(),
            scale: 1.0,
        };
        spec.grille = Material::DiamondGrille {
            cloth: "#161314".into(),
            stripes: vec!["#b8324a".into(), "#e6e2d6".into(), "#5f9e86".into()],
            pitch: 96.0,
        };
        spec.face = Material::Brushed {
            color: "#c3c4c6".into(),
            dir: Dir::Horizontal,
        };
        spec.face_frac = face_frac;
        spec.piping = Some("#b9babc".into());
        spec.corners = Some("black".into());
        spec.handle = handle;
        spec.handle_color = "#1e1c1b".into();
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        // Age: grime specks and a few rust spots on the steel.
        let mut rng = 7_u64;
        let mut next = |m: f64| {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((rng >> 20) as f64 % 10_000.0) / 10_000.0 * m
        };
        let mut specks = Vec::new();
        for _ in 0..320 {
            let (x, y) = (next(pw), next(ph));
            specks.push(vec![(x, y), (x + 0.8, y + 0.5)]);
        }
        let mut rust = Vec::new();
        for _ in 0..40 {
            let (x, y) = (next(pw), next(ph));
            rust.push(vec![(x, y), (x + 1.4, y + 0.9)]);
        }
        b.add(
            plate,
            draw::with_opacity(
                ui::strokes("age", &specks, 1.2, draw::solid("#3a3434")),
                0.3,
            ),
        );
        b.add(
            plate,
            draw::with_opacity(
                ui::strokes("rust", &rust, 2.2, draw::solid("#8a2a1c")),
                0.45,
            ),
        );
        let sy = ph / 132.0;
        let sx = pw / 1120.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        // The panel's legends: a quirky display face, hand-set.
        let legend = Lettering::family(family::PIRATA_ONE);
        let small = Lettering::family(family::INTER).weight(500.0);
        let ky = y(66.0);
        let ly = y(112.0);
        print(b, plate, "FET", x(48.0), y(24.0), 11.0, INK, 700.0, 0.02);
        jack(b, plate, x(78.0), y(26.0), 9.0);
        jack(b, plate, x(78.0), y(100.0), 9.0);
        let dots: &[&str] = &["", "", "", "", "", "", "", "", "", "", ""];
        let chrome = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            knob_scaled(
                b,
                plate,
                Cap::ChromeSkirt,
                addr,
                name,
                knob_d,
                x(cx),
                ky,
                dots,
                INK,
            );
            set(
                b,
                plate,
                name,
                x(cx),
                ly + (big - 1.0) * 12.0,
                15.0 * big.sqrt(),
                INK,
                &legend,
                0.04,
            );
        };
        chrome(b, "clean/volume", "VOLUME", 129.0);
        for (addr, name, top, bottom, cx) in [
            ("clean/bright", "Bright", "BRIGHT", "", 181.0),
            ("clean/deep", "Deep", "DEEP", "", 227.0),
            ("clean/voice", "Guitar", "GUITAR", "MIC", 273.0),
        ] {
            set(b, plate, top, x(cx), y(18.0), 7.0, INK, &small, 0.02);
            screw(b, plate, x(cx), y(33.0), 3.2);
            slide(b, plate, addr, name, "red", 26.0, x(cx), ky);
            screw(b, plate, x(cx), y(100.0), 3.2);
            if !bottom.is_empty() {
                set(b, plate, bottom, x(cx), y(116.0), 7.0, INK, &small, 0.02);
            }
        }
        for (addr, name, cx) in [
            ("clean/treble", "TREBLE", 327.0),
            ("clean/middle", "MIDDLE", 395.0),
            ("clean/bass", "BASS", 466.0),
            ("clean/high", "HIGH", 538.0),
            ("clean/low", "LOW", 609.0),
        ] {
            chrome(b, addr, name, cx);
        }
        set(
            b,
            plate,
            "FILTER",
            x(573.0),
            y(14.0),
            14.0,
            INK,
            &legend,
            0.04,
        );
        set(b, plate, "ON", x(663.0), y(20.0), 7.0, INK, &small, 0.02);
        screw(b, plate, x(663.0), y(34.0), 3.2);
        slide(
            b,
            plate,
            "clean/reverb_on",
            "Reverb on",
            "red",
            26.0,
            x(663.0),
            ky,
        );
        screw(b, plate, x(663.0), y(100.0), 3.2);
        set(b, plate, "OFF", x(663.0), y(116.0), 7.0, INK, &small, 0.02);
        set(
            b,
            plate,
            "REVERB",
            x(752.0),
            y(14.0),
            14.0,
            INK,
            &legend,
            0.04,
        );
        chrome(b, "clean/send", "SEND", 716.0);
        chrome(b, "clean/return", "RETURN", 789.0);
        set(b, plate, "MID", x(839.0), y(20.0), 7.0, INK, &small, 0.02);
        screw(b, plate, x(839.0), y(34.0), 3.2);
        slide(b, plate, "clean/mid", "Mid", "red", 26.0, x(839.0), ky);
        screw(b, plate, x(839.0), y(100.0), 3.2);
        chrome(b, "clean/level", "LEVEL", 892.0);
        // The logo: heavy slab capitals, a little italic, inked on.
        let slab = Lettering::family(family::PLAYFAIR_DISPLAY)
            .weight(900.0)
            .italic();
        set(
            b,
            plate,
            "OPEN",
            x(1010.0),
            y(40.0),
            25.0,
            "#0e0e0e",
            &slab,
            0.0,
        );
        set(
            b,
            plate,
            "ROOM",
            x(1010.0),
            y(72.0),
            25.0,
            "#0e0e0e",
            &slab,
            0.0,
        );
        set(
            b,
            plate,
            "hand wired",
            x(1010.0),
            y(104.0),
            17.0,
            "#1a1a1a",
            &legend,
            0.08,
        );
        jewel(b, plate, "clean/on", "red", 30.0, x(1088.0), y(70.0));
        inside(b, grille);
    });
    let ns = "clean";
    let params = vec![
        pct(ns, "volume", "Volume", 4.0).featured(),
        ui::toggle("clean/bright", "Bright", true),
        ui::toggle("clean/deep", "Deep", true),
        ui::choice("clean/voice", "Voicing", &["Mic", "Guitar"], 1),
        pct(ns, "treble", "Treble", 5.0).featured(),
        pct(ns, "middle", "Middle", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "high", "Filter high", 5.0),
        pct(ns, "low", "Filter low", 5.0),
        ui::toggle("clean/reverb_on", "Reverb", true),
        pct(ns, "send", "Reverb send", 5.0),
        pct(ns, "return", "Reverb return", 5.0),
        ui::toggle("clean/mid", "Mid boost", true),
        pct(ns, "level", "Level", 5.0).featured(),
        ui::toggle("clean/on", "Power", true),
    ];
    Face {
        file: "07-amp-head-clean-head",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The lead head.
pub fn lead_head() -> Face {
    const W: f64 = 1200.0;
    const H: f64 = 548.0;
    const INK: &str = "#ebe7df";
    let scene = canvas("Lead Head", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Head, W, H);
        spec.covering = Material::Tolex {
            grain: Grain::Bronco,
            color: "#1b1a1a".into(),
            scale: 0.8,
        };
        spec.grille = Material::GrilleCloth {
            cloth: Cloth::Black,
        };
        spec.face = Material::Plastic {
            color: "#161415".into(),
            gloss: 0.35,
        };
        spec.face_frac = 0.32;
        spec.piping = Some("#a8997c".into());
        spec.corners = Some("black".into());
        spec.handle = Handle::Leather;
        spec.handle_color = "#1c1b1c".into();
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        cloth(b, grille, "basket", &[]);
        let sy = ph / 152.0;
        let sx = pw / 1120.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        let sans = Lettering::family(family::INTER).weight(400.0);
        print(b, plate, "INPUT", x(71.0), y(40.0), 11.0, INK, 400.0, 0.0);
        jack(b, plate, x(71.0), y(80.0), 12.0);
        let toggles = [
            ("lead/bright", "Bright", "BRIGHT", 138.0),
            ("lead/mid", "Mid", "MID", 218.0),
            ("lead/deep", "Deep", "DEEP", 299.0),
            ("lead/eq", "EQ", "EQ1", 379.0),
            ("lead/loop", "Loop bypass", "BYPASS", 540.0),
            ("lead/lead", "Lead", "LEAD", 622.0),
        ];
        for (addr, name, legend, cx) in toggles {
            set(b, plate, legend, x(cx), y(12.0), 7.5, INK, &sans, 0.0);
            parts::toggle(b, plate, addr, name, 11.0, x(cx), y(32.0));
        }
        set(b, plate, "EQ2", x(379.0), y(50.0), 7.5, INK, &sans, 0.0);
        // Treble to presence: the reference's pointers sit near noon, a
        // little either side.
        let knobs = [
            ("lead/treble", "TREBLE", 138.0),
            ("lead/middle", "MIDDLE", 218.0),
            ("lead/bass", "BASS", 299.0),
            ("lead/gain", "GAIN", 379.0),
            ("lead/master", "MASTER", 460.0),
            ("lead/lead_gain", "LEAD GAIN", 540.0),
            ("lead/lead_master", "LEAD MASTER", 622.0),
            ("lead/reverb", "REVERB", 701.0),
            ("lead/presence", "PRESENCE", 780.0),
        ];
        for (addr, name, cx) in knobs {
            set(b, plate, name, x(cx), y(64.0), 12.0, INK, &sans, -0.01);
            knob(
                b,
                plate,
                Cap::ChickenHead,
                addr,
                name,
                60.0,
                x(cx),
                y(110.0),
            );
        }
        // The logo in a painted script, a model number under it in gold marker.
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        b.add(
            plate,
            with_shadow(
                draw::styled_text(
                    "logo",
                    "Studio",
                    x(840.0),
                    y(60.0) - 24.0,
                    180.0,
                    34.0,
                    "#f4f1ea",
                    frame::document::proto::TextAlign::Center,
                    &script,
                ),
                "#000000a0",
            ),
        );
        parts::marker(b, plate, "Lead", x(955.0), y(100.0), 17.0, "#d9b76a");
        set(
            b,
            plate,
            "BY THE SHOP",
            x(880.0),
            y(104.0),
            7.0,
            INK,
            &Lettering::family(family::INTER).weight(700.0),
            0.04,
        );
        print(b, plate, "ON", x(1028.0), y(38.0), 10.0, INK, 400.0, 0.0);
        parts::toggle(b, plate, "lead/on", "Standby", 15.0, x(1028.0), y(76.0));
        print(
            b,
            plate,
            "STANDBY",
            x(1028.0),
            y(128.0),
            10.0,
            INK,
            400.0,
            0.0,
        );
        // The pilot light, steady (see `jewel`).
        b.add(
            plate,
            draw::circle(
                "pilot bezel",
                x(1085.0),
                y(76.0),
                10.0,
                draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#060607")]),
            ),
        );
        let lens = draw::circle(
            "pilot",
            x(1085.0),
            y(76.0),
            7.0,
            draw::radial_at(
                0.4,
                0.35,
                0.9,
                &[(0.0, "#ffe0e0"), (0.35, "#ff4a4a"), (1.0, "#8a0a0a")],
            ),
        );
        b.add(plate, draw::emissive(lens, draw::hex("#ff2a2a"), 0.9, 18.0));
    });
    let ns = "lead";
    let params = vec![
        ui::toggle("lead/bright", "Bright", false),
        ui::toggle("lead/mid", "Mid", false),
        ui::toggle("lead/deep", "Deep", false),
        ui::choice("lead/eq", "EQ", &["EQ2", "EQ1"], 1),
        ui::toggle("lead/loop", "Loop bypass", false),
        ui::toggle("lead/lead", "Lead channel", false),
        pct(ns, "treble", "Treble", 5.5).featured(),
        pct(ns, "middle", "Middle", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "gain", "Gain", 5.0).featured(),
        pct(ns, "master", "Master", 5.0).featured(),
        pct(ns, "lead_gain", "Lead gain", 5.0),
        pct(ns, "lead_master", "Lead master", 4.5),
        pct(ns, "reverb", "Reverb", 5.5),
        pct(ns, "presence", "Presence", 4.5),
        ui::toggle("lead/on", "Standby", true),
    ];
    Face {
        file: "08-amp-head-lead-head",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The two-channel combo.
pub fn silk_combo() -> Face {
    const W: f64 = 1000.0;
    const H: f64 = 753.0;
    const INK: &str = "#f1ece4";
    let scene = canvas("Silk Combo", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Combo, W, H);
        spec.covering = Material::Tolex {
            grain: Grain::Levant,
            color: "#4c2c20".into(),
            scale: 1.0,
        };
        spec.grille = Material::GrilleCloth {
            cloth: Cloth::Wheat,
        };
        spec.face = Material::Plastic {
            color: "#141313".into(),
            gloss: 0.3,
        };
        spec.face_frac = 0.135;
        spec.piping = Some("#d8cbb2".into());
        spec.corners = None;
        spec.handle = Handle::Strap;
        spec.handle_color = "#2a1a14".into();
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        cloth(b, grille, "wheat", &[]);
        // The silver frame round the black plate.
        b.add(
            plate,
            draw::stroked(
                draw::rounded(
                    draw::rect(
                        "frame",
                        3.0,
                        3.0,
                        pw - 6.0,
                        ph - 6.0,
                        draw::solid("#00000000"),
                    ),
                    2.0,
                ),
                "#cfc9be",
                1.4,
            ),
        );
        let sy = ph / 76.0;
        let sx = pw / 952.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        let sans = Lettering::family(family::INTER).weight(700.0);
        let ky = y(34.0);
        let ly = y(64.0);
        let numbered = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            knob_scaled(
                b,
                plate,
                Cap::BlackSkirt,
                addr,
                name,
                42.0,
                x(cx),
                ky,
                &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
                INK,
            );
            set(b, plate, name, x(cx), ly, 7.0, INK, &sans, 0.02);
        };
        // Normal channel.
        for (label, cx) in [("1", 36.0), ("2", 71.0)] {
            set(b, plate, label, x(cx), y(11.0), 8.0, INK, &sans, 0.0);
            jack(b, plate, x(cx), y(32.0), 9.0);
        }
        set(b, plate, "NORMAL", x(54.0), y(56.0), 11.0, INK, &sans, 0.02);
        set(b, plate, "BRIGHT", x(100.0), y(9.0), 5.5, INK, &sans, 0.0);
        slide(
            b,
            plate,
            "combo/normal_bright",
            "Normal bright",
            "black",
            20.0,
            x(100.0),
            y(32.0),
        );
        for (addr, name, cx) in [
            ("combo/normal_volume", "VOLUME", 147.0),
            ("combo/normal_treble", "TREBLE", 195.0),
            ("combo/normal_bass", "BASS", 243.0),
        ] {
            numbered(b, addr, name, cx);
        }
        b.add(
            plate,
            draw::rect(
                "divider",
                x(273.0),
                6.0,
                1.2,
                ph - 12.0,
                draw::solid("#cfc9be"),
            ),
        );
        // Vibrato channel.
        for (label, cx) in [("1", 304.0), ("2", 340.0)] {
            set(b, plate, label, x(cx), y(11.0), 8.0, INK, &sans, 0.0);
            jack(b, plate, x(cx), y(32.0), 9.0);
        }
        set(
            b,
            plate,
            "VIBRATO",
            x(322.0),
            y(56.0),
            11.0,
            INK,
            &sans,
            0.02,
        );
        set(b, plate, "BRIGHT", x(366.0), y(9.0), 5.5, INK, &sans, 0.0);
        slide(
            b,
            plate,
            "combo/bright",
            "Bright",
            "black",
            20.0,
            x(366.0),
            y(32.0),
        );
        for (addr, name, cx) in [
            ("combo/volume", "VOLUME", 412.0),
            ("combo/treble", "TREBLE", 458.0),
            ("combo/bass", "BASS", 504.0),
            ("combo/reverb", "REVERB", 550.0),
            ("combo/speed", "SPEED", 596.0),
            ("combo/intensity", "INTENSITY", 642.0),
        ] {
            numbered(b, addr, name, cx);
        }
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        b.add(
            plate,
            with_shadow(
                draw::styled_text(
                    "logo",
                    "Silk",
                    x(690.0),
                    y(4.0),
                    140.0,
                    28.0,
                    "#f2dcd6",
                    frame::document::proto::TextAlign::Center,
                    &script,
                ),
                "#00000080",
            ),
        );
        b.add(
            plate,
            with_shadow(
                draw::styled_text(
                    "logo",
                    "Tone",
                    x(735.0),
                    y(30.0),
                    140.0,
                    28.0,
                    "#f2dcd6",
                    frame::document::proto::TextAlign::Center,
                    &script,
                ),
                "#00000080",
            ),
        );
        set(
            b,
            plate,
            "TWO CHANNEL",
            x(840.0),
            y(62.0),
            5.5,
            INK,
            &sans,
            0.06,
        );
        jewel(b, plate, "combo/on", "red", 24.0, x(912.0), y(30.0));
    });
    let ns = "combo";
    let params = vec![
        ui::toggle("combo/normal_bright", "Normal bright", false),
        pct(ns, "normal_volume", "Normal volume", 5.0),
        pct(ns, "normal_treble", "Normal treble", 5.0),
        pct(ns, "normal_bass", "Normal bass", 5.0),
        ui::toggle("combo/bright", "Bright", true),
        pct(ns, "volume", "Volume", 5.0).featured(),
        pct(ns, "treble", "Treble", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "reverb", "Reverb", 5.0),
        pct(ns, "speed", "Vibrato speed", 5.0),
        pct(ns, "intensity", "Vibrato intensity", 5.0),
        ui::toggle("combo/on", "Power", true),
    ];
    Face {
        file: "09-amp-combo-silk-combo",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// An amp's dress: its covering, grille cloth, faceplate, piping and
/// corners on `spec`, and its panel's ink, knob caps and lettering.
fn dress(model: AmpModel, spec: &mut CabinetSpec) -> (&'static str, Cap, Lettering) {
    match model {
        AmpModel::SteelStringSinger | AmpModel::DumbleOds => {
            spec.covering = Material::Tolex {
                grain: Grain::Bronco,
                color: "#1d1c1c".into(),
                scale: 1.0,
            };
            spec.grille = Material::DiamondGrille {
                cloth: "#161314".into(),
                stripes: vec!["#b8324a".into(), "#e6e2d6".into(), "#5f9e86".into()],
                pitch: 96.0,
            };
            spec.face = Material::Brushed {
                color: "#c3c4c6".into(),
                dir: Dir::Horizontal,
            };
            spec.piping = Some("#b9babc".into());
            (
                "#141414",
                Cap::ChromeSkirt,
                Lettering::family(family::PIRATA_ONE),
            )
        }
        AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => {
            spec.covering = Material::Tolex {
                grain: Grain::Levant,
                color: "#161616".into(),
                scale: 0.8,
            };
            spec.grille = Material::GrilleCloth {
                cloth: Cloth::Silver,
            };
            spec.face = Material::Plastic {
                color: "#121213".into(),
                gloss: 0.3,
            };
            spec.piping = Some("#d8d8d8".into());
            spec.corners = Some("chrome".into());
            (
                "#f2f2f0",
                Cap::BlackSkirt,
                Lettering::family(family::INTER).weight(600.0),
            )
        }
        AmpModel::Ac30 => {
            spec.covering = Material::Tolex {
                grain: Grain::Levant,
                color: "#141313".into(),
                scale: 0.8,
            };
            spec.grille = Material::DiamondGrille {
                cloth: "#5a3e26".into(),
                stripes: vec!["#caa272".into(), "#2e1e10".into()],
                pitch: 44.0,
            };
            spec.face = Material::Brushed {
                color: "#b8703e".into(),
                dir: Dir::Horizontal,
            };
            spec.piping = Some("#d8c7a0".into());
            spec.corners = Some("chrome".into());
            (
                "#1c1208",
                Cap::Cream,
                Lettering::family(family::INTER).weight(700.0),
            )
        }
        AmpModel::TwoRock => {
            spec.covering = Material::Tolex {
                grain: Grain::Bronco,
                color: "#1b1a1a".into(),
                scale: 0.8,
            };
            spec.grille = Material::GrilleCloth {
                cloth: Cloth::Black,
            };
            spec.face = Material::Plastic {
                color: "#161415".into(),
                gloss: 0.35,
            };
            spec.piping = Some("#a8997c".into());
            (
                "#ebe7df",
                Cap::ChickenHead,
                Lettering::family(family::INTER).weight(500.0),
            )
        }
    }
}

/// An amp composed for a block of Signal's routing grid (`s` square): its
/// front as much as the block allows — covering, grille cloth and a thin
/// panel bearing its name, its maker's badge on the grille — and no knobs.
#[must_use]
pub fn amp_block(model: AmpModel, prefix: &str, s: f64) -> Face {
    let scene = canvas(model.short(), |b, root| {
        let mut spec = CabinetSpec::new(Kind::Combo, s, s);
        spec.handle = Handle::None;
        spec.feet = false;
        spec.corners = Some("black".into());
        let (ink, _, legend) = dress(model, &mut spec);
        spec.face_frac = 0.2;
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        let (gw, gh) = b
            .scene
            .node(grille)
            .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        let (name, brand): (&str, Option<(&str, &str, Lettering, f64)>) = match model {
            AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb => (
                model.fender_name(),
                Some(("Fender", "#f4f4f2", script, 0.0)),
            ),
            AmpModel::Ac30 => (
                "AC30",
                Some((
                    "VOX",
                    "#e8c36a",
                    Lettering::family(family::INTER).weight(900.0),
                    0.18,
                )),
            ),
            AmpModel::SteelStringSinger => (
                "Steel String Singer",
                Some(("Dumble", "#e8e2d2", script, 0.0)),
            ),
            AmpModel::DumbleOds => (
                "Overdrive Special",
                Some(("Dumble", "#e8e2d2", script, 0.0)),
            ),
            AmpModel::TwoRock => (
                "Signature",
                Some((
                    "TWO-ROCK",
                    "#d9d9dc",
                    Lettering::family(family::OSWALD).weight(700.0).italic(),
                    0.06,
                )),
            ),
        };
        set(
            b,
            plate,
            name,
            pw / 2.0,
            ph * 0.52,
            (ph * 0.42).min(pw * 0.07),
            ink,
            &legend,
            0.12,
        );
        // A blackface's silver cloth, its weave drawn large enough to read
        // at a block's size: dark threads across, light threads over them.
        if matches!(
            model,
            AmpModel::Vibroverb | AmpModel::DeluxeReverb | AmpModel::TwinReverb
        ) {
            let pitch = 6.0;
            let n = ((gw + gh) / pitch) as i32;
            let threads = |shift: f64, up: bool| -> Vec<Vec<(f64, f64)>> {
                (0..n)
                    .map(|i| {
                        let x = f64::from(i) * pitch - gh + shift;
                        if up {
                            vec![(x, gh), (x + gh, 0.0)]
                        } else {
                            vec![(x, 0.0), (x + gh, gh)]
                        }
                    })
                    .collect()
            };
            b.add(
                grille,
                ui::strokes(
                    "weave dark",
                    &threads(0.0, true),
                    1.6,
                    draw::solid("#1c1e2299"),
                ),
            );
            b.add(
                grille,
                ui::strokes(
                    "weave light",
                    &threads(pitch / 2.0, false),
                    1.0,
                    draw::solid("#f4f6f566"),
                ),
            );
        }
        if let Some((word, colour, l, track)) = brand {
            set(
                b,
                grille,
                word,
                gw / 2.0,
                gh * 0.24,
                (gh * 0.24).min(gw * 0.24),
                colour,
                &l,
                track,
            );
        }
    });
    let params = vec![ui::toggle(&format!("{prefix}/on"), "Power", true)];
    Face {
        file: "07-amp-block",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
