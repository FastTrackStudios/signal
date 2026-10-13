//! Materials and rendered parts: what surfaces are made of, and the 3D
//! parts built from them.
//!
//! - [`light`] — the kit's one light (upper left) and its shadows, bevels
//!   and recesses.
//! - [`texture`] — tileable noise, cell and weave tiles generated in code
//!   and stored as small PNG assets.
//! - [`paint`] — [`Material`] and [`fill`]: tolex, leather, croc, powder
//!   coat, hammertone, brushed and spun metal, wood, grille cloths,
//!   perforated and expanded metal, diamond and chevron grilles, chrome,
//!   nickel, glossy plastic, rubber.
//! - [`knob`] — `knob.render`, the generic rendered knob.
//! - [`knob3d`] — `knob.3d`, the same knob rendered per pixel on the GPU.
//! - [`enclosure`] — `panel.pedal_3d`.
//! - [`cabinet`] — `panel.amp_head`, `panel.amp_combo`.
//!
//! Each material is also a `material.*` swatch component (family
//! `material`), so the kit sheet and agents can see and place them.

pub mod cabinet;
pub mod enclosure;
pub mod knob;
pub mod knob3d;
pub mod light;
pub mod paint;
pub mod texture;

use document_proto::NodeId;
use surface_widgets::Builder;

pub use paint::{Cloth, Dir, Grain, Material, fill};

use crate::registry::{ComponentDef, Family, Place};

/// The material a swatch component draws, from its id and place.
fn swatch_material(id: &str, p: &Place) -> Material {
    let v = p.variant.as_deref().unwrap_or("");
    let color = p.opt("color").filter(|c| c.starts_with('#'));
    let c = |d: &str| color.unwrap_or(d).to_string();
    let scale = p.opt("scale").and_then(|s| s.parse().ok()).unwrap_or(1.0);
    match id {
        "material.tolex" => {
            let (g, d) = match v {
                "bronco" => (Grain::Bronco, "#191817"),
                "elephant" => (Grain::Elephant, "#2c2724"),
                "fine" => (Grain::Fine, "#e9e2cf"),
                "cream" => (Grain::Levant, "#e6dfcb"),
                "blonde" => (Grain::Bronco, "#d9c7a0"),
                _ => (Grain::Levant, "#1c1b1a"),
            };
            Material::Tolex {
                grain: g,
                color: c(d),
                scale,
            }
        }
        "material.leather" => Material::Leather {
            color: c(match v {
                "black" => "#1f1b19",
                "tan" => "#a8744a",
                _ => "#6e4128",
            }),
        },
        "material.croc" => Material::Croc {
            color: c(match v {
                "brown" => "#4a2e1f",
                "red" => "#5e1a1a",
                _ => "#2a2522",
            }),
            scale,
        },
        "material.powder" => Material::Powder {
            color: c(match v {
                "grey" => "#a9abae",
                "blue" => "#3f63a8",
                "coarse" => "#2e3033",
                _ => "#c9464a",
            }),
            coarse: v == "coarse",
        },
        "material.hammertone" => Material::Hammertone {
            color: c(match v {
                "blue" => "#4f6f8f",
                "green" => "#56715c",
                "copper" => "#8a5a3a",
                _ => "#7c8488",
            }),
            scale,
        },
        "material.brushed" => Material::Brushed {
            color: c(match v {
                "black" => "#3a3b3e",
                "gold" => "#c8b07a",
                _ => "#b9bbbd",
            }),
            dir: if v == "vertical" {
                Dir::Vertical
            } else {
                Dir::Horizontal
            },
        },
        "material.spun" => Material::Spun {
            color: c(if v == "gold" { "#c9ad6e" } else { "#c3c5c7" }),
        },
        "material.wood" => Material::Wood {
            color: c(match v {
                "maple" | "maple_vertical" => "#c89a5c",
                "rosewood" => "#4a2618",
                _ => "#6b3f22",
            }),
            dir: if v.ends_with("vertical") {
                Dir::Vertical
            } else {
                Dir::Horizontal
            },
        },
        "material.grille_cloth" => Material::GrilleCloth {
            cloth: paint::cloth(v),
        },
        "material.perforated" => Material::Perforated {
            color: c(if v == "black" { "#35373a" } else { "#b9bcbf" }),
            pitch: 7.0 * scale,
        },
        "material.expanded" => Material::Expanded {
            color: c(if v == "black" { "#3c3e41" } else { "#8d9196" }),
            pitch: 16.0 * scale,
        },
        "material.grille_diamond" => Material::DiamondGrille {
            cloth: c("#1a1717"),
            stripes: vec!["#b8324a".into(), "#e6e2d6".into(), "#5f9e86".into()],
            pitch: 90.0 * scale,
        },
        "material.grille_chevron" => Material::ChevronGrille {
            bars: c(if v == "black" { "#2a2b2d" } else { "#e8e2d2" }),
            pitch: 18.0 * scale,
        },
        "material.chrome" => {
            if v == "nickel" {
                Material::Nickel
            } else {
                Material::Chrome
            }
        }
        "material.plastic" => Material::Plastic {
            color: c(match v {
                "white" => "#eeece6",
                "red" => "#b3261e",
                "cream" => "#e8dcc0",
                _ => "#141414",
            }),
            gloss: 1.0,
        },
        _ => Material::Rubber {
            color: c("#1a1a1b"),
        },
    }
}

fn swatch(b: &mut Builder<'_>, parent: NodeId, p: &Place, id: &str) -> NodeId {
    let w = p
        .opt("w")
        .and_then(|v| v.parse().ok())
        .or(p.size)
        .unwrap_or(220.0);
    let h = p.opt("h").and_then(|v| v.parse().ok()).unwrap_or(w * 0.64);
    let radius = p.opt("radius").and_then(|v| v.parse().ok()).unwrap_or(6.0);
    let root = b.add(parent, crate::draw::frame(&p.name, 0.0, 0.0, w, h));
    fill(b, root, (0.0, 0.0, w, h), radius, &swatch_material(id, p));
    root
}

macro_rules! swatches {
    ($( $fn:ident, $id:literal, $title:literal, $variants:expr, $desc:literal; )*) => {
        $( fn $fn(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId { swatch(b, parent, p, $id) } )*
        fn swatch_defs() -> Vec<ComponentDef> {
            vec![$( ComponentDef {
                id: $id,
                family: Family::Material,
                title: $title,
                description: concat!($desc, " A swatch of `materials::fill`: `w=`/`h=` (or size = width), `color=#hex`, `scale=`. No control."),
                variants: $variants,
                default_size: 220.0,
                binds: "none",
                build: $fn,
            }, )*]
        }
    };
}

swatches! {
    tolex, "material.tolex", "Tolex", &["levant", "bronco", "elephant", "fine", "cream", "blonde"], "Vinyl amp covering: fine pebble (Levant), coarse (Bronco), wrinkled (Elephant) or smooth, lit from the upper left.";
    leather, "material.leather", "Leather", &["brown", "black", "tan"], "Soft leather with pores and a waxy gloss (handles, straps, covers).";
    croc, "material.croc", "Croc", &["black", "brown", "red"], "Crocodile-embossed covering: domed scales between dark creases.";
    powder, "material.powder", "Powder coat", &["red", "grey", "blue", "coarse"], "Powder-coated steel: fine sparkle over faint orange peel (`coarse`: a wrinkle coat). Pedal lids.";
    hammertone, "material.hammertone", "Hammertone", &["silver", "blue", "green", "copper"], "Hammered metallic paint: shallow dimples catching the light. Vintage chassis and pedals.";
    brushed, "material.brushed", "Brushed metal", &["horizontal", "vertical", "black", "gold"], "Brushed aluminium: streaks along the grain and the anisotropic band across it. Faceplates, lids.";
    spun, "material.spun", "Spun metal", &["silver", "gold"], "Spun (turned) metal: concentric rings under a bow-tie sheen. Knob tops, footswitch caps.";
    wood, "material.wood", "Wood", &["walnut", "maple", "rosewood", "walnut_vertical", "maple_vertical"], "Lacquered wood: wavering figure bands and fibre, grain horizontal or vertical. Cheeks and cabinets.";
    grille_cloth, "material.grille_cloth", "Grille cloth", &["basket", "wheat", "oxblood", "black", "silver"], "Speaker grille cloth, woven: basketweave, wheat or oxblood twill, fine black, silver sparkle. Recessed.";
    perforated, "material.perforated", "Perforated metal", &["silver", "black"], "Round-hole perforated sheet on a staggered grid; each hole's far wall catches the light.";
    expanded, "material.expanded", "Expanded metal", &["silver", "black"], "Expanded-metal diamond mesh with lit strand faces.";
    grille_diamond, "material.grille_diamond", "Diamond grille", &[], "Stitched piping crossing in diamonds over black cloth (the JMX / Vox diamond).";
    grille_chevron, "material.grille_chevron", "Chevron grille", &["cream", "black"], "Embossed chevron (V) bars over a dark baffle.";
    chrome, "material.chrome", "Chrome / nickel", &["chrome", "nickel"], "Polished chrome or warmer nickel: a studio reflected in hard bands, a crisp hotspot.";
    plastic, "material.plastic", "Glossy plastic", &["black", "white", "red", "cream"], "Glossy moulded plastic: a soft reflected window and a hotspot.";
    rubber, "material.rubber", "Rubber", &[], "Matte rubber with a fine tooth (feet, knob grips, pads).";
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut all = swatch_defs();
    all.extend(knob::defs());
    all.extend(knob3d::defs());
    all.extend(enclosure::defs());
    all.extend(cabinet::defs());
    all
}
