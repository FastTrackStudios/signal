//! Paint the hardware kit to one PNG, for review.
//!
//! ```text
//! kit_sheet --out kit.png [--family knob|meter|button|switch|lever|panel|viz|material] [--scale 2] [--swatch]
//!           [--only knob.render,panel.pedal_3d] [--fx gpu|fallback|off] [--save-markup sheet.frame]
//! ```
//! `--save-markup` also writes the sheet as markup, for
//! `render_surface --markup` (its controls bind the demo catalog's params:
//! `--set-at 0.5:delay/mix=1` turns every knob).
//! Every registered component (or one family), in a labelled grid, each
//! bound to a demo param at a mid value so pointers and meters show a
//! position. `--swatch` instead paints the kit's paint helpers (angled
//! linears, offset radials, conics) to check gradient geometry. `--only`
//! keeps the components whose ids start with one of the given prefixes; a
//! `material` swatch shows one cell per variant.

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("kit_sheet requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(e) = run() {
        eprintln!("kit_sheet: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use std::collections::BTreeMap;

    use document_proto::{
        AutoLayout, CrossAxisAlign, LayoutDirection, Node, NodeKind, Scene, TextAlign,
    };
    use surface_kit::draw;
    use surface_proto::{Binding, EntryId, ParamValue, Source, StreamFrame};
    use surface_runtime::{Runtime, Surface};
    use surface_widgets::{Builder, Theme, hug};

    let mut out = None;
    let mut family: Option<String> = None;
    let mut only: Vec<String> = Vec::new();
    let mut scale = 1.0_f64;
    let mut swatch = false;
    let mut fx = render::fx::Mode::Gpu;
    let mut save_markup: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--family" => family = args.next(),
            "--only" => {
                only = args
                    .next()
                    .ok_or("--only needs id prefixes")?
                    .split(',')
                    .map(ToString::to_string)
                    .collect()
            }
            "--scale" => {
                scale = args
                    .next()
                    .ok_or("--scale needs a factor")?
                    .parse()
                    .map_err(|e| format!("{e}"))?
            }
            "--swatch" => swatch = true,
            "--fx" => fx = args.next().ok_or("--fx needs gpu|fallback|off")?.parse()?,
            "--save-markup" => save_markup = args.next(),
            other => return Err(format!("unknown arg {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;

    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder {
        scene: &mut scene,
        controls: &mut controls,
        theme: &theme,
    };
    let mut root = Node::new(NodeKind::Frame, "sheet");
    root.fills = vec![draw::solid("#3a3a3e")];
    root.layout = Some(AutoLayout {
        direction: LayoutDirection::Vertical,
        gap: 28.0,
        padding_left: 32.0,
        padding_right: 32.0,
        padding_top: 32.0,
        padding_bottom: 32.0,
        wrap: false,
        main_align: None,
        cross_align: Some(CrossAxisAlign::Start),
        hug_main: true,
        hug_cross: true,
    });
    let root = b.add(page, root);

    let row = |b: &mut Builder<'_>, parent, name: &str| {
        let mut n = Node::new(NodeKind::Frame, name);
        n.layout = Some(AutoLayout {
            direction: LayoutDirection::Horizontal,
            gap: 28.0,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            wrap: false,
            main_align: None,
            cross_align: Some(CrossAxisAlign::Center),
            hug_main: true,
            hug_cross: true,
        });
        b.add(parent, n)
    };
    let cell = |b: &mut Builder<'_>, parent, title: &str| {
        let mut n = Node::new(NodeKind::Frame, title);
        n.layout = Some(AutoLayout {
            direction: LayoutDirection::Vertical,
            gap: 10.0,
            padding_left: 14.0,
            padding_right: 14.0,
            padding_top: 14.0,
            padding_bottom: 12.0,
            wrap: false,
            main_align: None,
            cross_align: Some(CrossAxisAlign::Center),
            hug_main: true,
            hug_cross: true,
        });
        n.fills = vec![draw::solid("#1b1b1e")];
        n.corner_radii = document_proto::CornerRadii::uniform(10.0);
        let c = b.add(parent, n);
        b.add(
            c,
            draw::text(
                "id",
                title,
                0.0,
                0.0,
                180.0,
                11.0,
                "#9a9aa2",
                TextAlign::Center,
            ),
        );
        c
    };

    let mut catalog = surface_memory::demo_catalog();
    // A gain-reduction stream for the meters that read one.
    catalog.streams.push(surface_proto::StreamInfo {
        id: EntryId::new("comp/gr"),
        name: "GR".into(),
        kind: surface_proto::StreamKind::GainReduction,
        rate_hz: 30.0,
    });
    // What the visualizers bind: an EQ, a compressor, an envelope, a scope.
    let viz_feeds = {
        use surface_kit::viz::catalog as vc;
        catalog.params.extend(
            vc::eq_params("eq", 6)
                .into_iter()
                .chain(vc::comp_params("comp"))
                .chain(vc::adsr_params("env")),
        );
        let mut feeds = vec![vc::spectrum_stream("eq", 128), vc::adsr_stream("env", 4)];
        feeds.extend(
            vc::comp_streams("comp", 300)
                .into_iter()
                .filter(|f| f.id.0 != "comp/gr"),
        );
        feeds.push(surface_proto::FeedInfo::new(
            "scope",
            "Scope",
            surface_proto::FeedKind::Waveform { len: 512 },
        ));
        catalog.streams.extend(feeds.iter().cloned());
        feeds
    };
    if swatch {
        let r = row(&mut b, root, "linear");
        for angle in [0.0, 45.0, 90.0, 135.0, 180.0, 270.0] {
            let c = cell(&mut b, r, &format!("linear {angle}°"));
            b.add(
                c,
                draw::rect(
                    "sw",
                    0.0,
                    0.0,
                    120.0,
                    80.0,
                    draw::linear(angle, &[(0.0, "#ffffff"), (1.0, "#d0342c")]),
                ),
            );
        }
        let r = row(&mut b, root, "radial");
        for (cx, cy, rr) in [
            (0.5, 0.5, 0.5),
            (0.3, 0.3, 0.6),
            (0.7, 0.25, 0.4),
            (0.5, 0.5, 0.25),
        ] {
            let c = cell(&mut b, r, &format!("radial {cx},{cy} r{rr}"));
            b.add(
                c,
                draw::circle(
                    "sw",
                    50.0,
                    50.0,
                    50.0,
                    draw::radial_at(cx, cy, rr, &[(0.0, "#ffffff"), (1.0, "#1d4f8f")]),
                ),
            );
        }
        let r = row(&mut b, root, "conic");
        for start in [0.0, 45.0, 90.0] {
            let c = cell(&mut b, r, &format!("conic from {start}°"));
            b.add(
                c,
                draw::circle(
                    "sw",
                    50.0,
                    50.0,
                    50.0,
                    draw::conic(
                        start,
                        &[
                            (0.0, "#ffffff"),
                            (0.25, "#555555"),
                            (0.5, "#ffffff"),
                            (0.75, "#555555"),
                            (1.0, "#ffffff"),
                        ],
                    ),
                ),
            );
        }
    } else {
        let defs: Vec<_> = surface_kit::components()
            .into_iter()
            .filter(|c| family.as_deref().is_none_or(|f| c.family.name() == f))
            .filter(|c| only.is_empty() || only.iter().any(|o| c.id.starts_with(o.as_str())))
            .collect();
        if defs.is_empty() {
            return Err("no components registered (for that family)".into());
        }
        let mut current = None;
        let mut per_row = 0;
        let mut r = root;
        for (i, def) in defs.iter().enumerate() {
            if current != Some(def.family) || per_row == 6 {
                r = row(&mut b, root, def.family.name());
                current = Some(def.family);
                per_row = 0;
            }
            per_row += 1;
            let c = cell(&mut b, r, def.id);
            let binding = match def.binds {
                "level stream" => Binding {
                    source: Source::Stream {
                        id: EntryId::new("out_level"),
                        channel: Some(0),
                    },
                    ..Binding::stream("out_level")
                },
                "scalar stream" => Binding::stream("tempo"),
                "choice param" => Binding::param("amp/channel"),
                "toggle param" => Binding::param(if i % 2 == 0 { "drive/on" } else { "delay/on" }),
                "action" => Binding::action("tap_tempo"),
                "none" => Binding::param(""),
                "spectrum stream" => Binding::stream("eq/spectrum"),
                "gain-reduction stream" => Binding::stream("comp/gr"),
                "waveform stream" => Binding::stream("scope"),
                _ => Binding::param("delay/mix"),
            };
            // Visualizers find their params by namespace (`viz::catalog`).
            let options: &[&str] = match def.id {
                "viz.eq_graph" => &["ns=eq"],
                "viz.comp" => &["ns=comp"],
                "viz.adsr" => &["ns=env", "playheads=env/voices"],
                _ => &[],
            };
            let place = surface_kit::Place {
                binding: (def.binds != "none" && def.binds != "params by namespace")
                    .then_some(binding)
                    .or_else(|| (def.id == "viz.eq_graph").then(|| Binding::stream("eq/spectrum"))),
                inner: Some(Box::new(
                    surface_kit::Place::new("Bass")
                        .bind(Binding::param("amp/bass"))
                        .legend("Bass"),
                )),
                options: (!options.is_empty())
                    .then(|| options.iter().map(ToString::to_string).collect()),
                ..surface_kit::Place::new(def.title)
            };
            // In a cell the size of its drawing: a tight box's print hangs out.
            surface_kit::place_in_cell(&mut b, c, def.id, &place)?;
            // A material shows every finish it comes in.
            if def.family == surface_kit::Family::Material {
                for v in def.variants.iter().skip(1) {
                    if per_row == 6 {
                        r = row(&mut b, root, def.family.name());
                        per_row = 0;
                    }
                    per_row += 1;
                    let c = cell(&mut b, r, &format!("{} {v}", def.id));
                    surface_kit::place_in_cell(
                        &mut b,
                        c,
                        def.id,
                        &surface_kit::Place::new(def.title).variant(*v),
                    )?;
                }
            }
        }
        // The options each family takes, shown once: inks, tight boxes,
        // stepped sweeps, reversed banks, gain reduction, ladders.
        let extras = variations(family.as_deref(), &only);
        if !extras.is_empty() {
            let mut r = row(&mut b, root, "variations");
            let per_row = if extras.iter().any(|e| e.1.starts_with("panel.amp")) {
                2
            } else {
                8
            };
            for (i, (title, id, place, light)) in extras.into_iter().enumerate() {
                if i > 0 && i % per_row == 0 {
                    r = row(&mut b, root, "variations");
                }
                let c = cell(&mut b, r, &title);
                if light {
                    // A light panel, for dark ink.
                    if let Some(n) = b.scene.node_mut(c) {
                        n.fills = vec![draw::solid("#c9c6bd")];
                    }
                }
                surface_kit::place_in_cell(&mut b, c, id, &place)?;
            }
        }
    }
    hug(&mut scene, root);
    for (id, c) in &controls {
        if let Some(n) = scene.node_mut(*id) {
            n.control = Some(c.clone());
        }
    }
    if let Some(path) = &save_markup {
        // The sheet as markup, for `render_surface --markup` (animations).
        std::fs::write(path, document_text::print(&scene)).map_err(|e| format!("{path}: {e}"))?;
    }

    let mut rt = Runtime::new(Surface::from_scene(scene), catalog);
    rt.apply_values([
        ParamValue {
            addr: surface_proto::ParamAddr::new("delay/mix"),
            value: 0.62,
        },
        ParamValue {
            addr: surface_proto::ParamAddr::new("amp/channel"),
            value: 0.5,
        },
        ParamValue {
            addr: surface_proto::ParamAddr::new("drive/on"),
            value: 1.0,
        },
    ]);
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("out_level"),
            values: vec![-21.0, -24.0],
        },
        0.0,
    );
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("tempo"),
            values: vec![180.0],
        },
        0.0,
    );
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("in_level"),
            values: vec![-5.0],
        },
        0.0,
    );
    // The visualizers, mid-song.
    for f in feed_memory::frames(&viz_feeds, 2.3) {
        rt.apply_frame(f, 0.0);
    }
    // A reading, sampled for four seconds: the GR history's ring fills.
    for i in 0..240 {
        rt.apply_frame(
            StreamFrame {
                id: EntryId::new("comp/gr"),
                values: vec![feed_memory::gr_db(f64::from(i) / 60.0) * 1.6],
            },
            0.0,
        );
    }
    let frame = rt.frame(0.0);
    let size = frame
        .node(
            frame
                .node(frame.pages()[0])
                .and_then(|p| p.children.first().copied())
                .ok_or("empty")?,
        )
        .ok_or("no root")?
        .size;
    let (w, h) = (
        (size.width * scale).ceil() as u32,
        (size.height * scale).ceil() as u32,
    );
    let values: std::collections::HashMap<_, _> = rt
        .surface()
        .controls
        .iter()
        .map(|(id, c)| (*id, rt.position(*id, c)))
        .collect();
    let px = render::offscreen::Offscreen::new()?.render_with_effects(&render::fx::Frame {
        transform: kurbo::Affine::scale(scale),
        base: peniko::Color::from_rgba8(58, 58, 62, 255),
        fonts: Some(render::FontBook::bundled()),
        mode: fx,
        values: Some(&values),
        ..render::fx::Frame::new(&frame, frame.pages()[0], w, h)
    })?;
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixels")?
        .save(&out)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{{\"ok\":true,\"out\":\"{out}\",\"width\":{w},\"height\":{h}}}");
    Ok(())
}

/// Extra cells showing what a family's `Place` options do:
/// `(title, component, place, on a light panel)`.
#[cfg(feature = "anyrender")]
fn variations(
    family: Option<&str>,
    only: &[String],
) -> Vec<(String, &'static str, surface_kit::Place, bool)> {
    use surface_kit::{Ink, Place, TOUCH_MIN};
    use surface_proto::Binding;
    let mix = || Binding::param("delay/mix");
    let choice = || Binding::param("amp/channel");
    let level = || Binding::stream("in_level");
    let mut v: Vec<(String, &'static str, Place, bool)> = Vec::new();
    if only.iter().any(|o| o.starts_with("knob.3d")) {
        // Every preset, then options.
        let light = [
            "fluted_cream",
            "white_knurl",
            "stepped_cream",
            "amp_pointer",
            "pointer",
            "gold_knurl",
            "spun_metal",
        ];
        for name in &surface_kit::materials::knob3d::PRESETS {
            let place = Place::new("Drive")
                .bind(mix())
                .size(64.0)
                .legend("Drive")
                .variant(*name);
            let on_light = light.contains(name);
            v.push((
                (*name).to_string(),
                "knob.3d",
                if on_light {
                    place.ink(Ink::Dark)
                } else {
                    place
                },
                on_light,
            ));
        }
        let k = |preset: &str, opts: &[&str]| {
            Place::new("Tone")
                .bind(mix())
                .size(64.0)
                .legend("Tone")
                .variant(preset)
                .options(opts)
        };
        v.push((
            "fluted_cream red".into(),
            "knob.3d",
            k("fluted_cream", &["color=#9c2a22", "pointer=#f3ead8"]),
            false,
        ));
        v.push((
            "spun_metal gold".into(),
            "knob.3d",
            k("spun_metal", &["color=#d8b47a"]),
            false,
        ));
        v.push((
            "domed rough 0.8".into(),
            "knob.3d",
            k("domed", &["rough=0.8", "color=#3b4f7a"]),
            false,
        ));
        v.push((
            "knurled ribs 24".into(),
            "knob.3d",
            k("knurled", &["ribs=24", "tilt=0"]),
            false,
        ));
        v.push((
            "stepped words".into(),
            "knob.3d",
            Place::new("Mode")
                .bind(choice())
                .size(64.0)
                .legend("Mode")
                .variant("bakelite_chicken")
                .options(&["Clean", "Crunch", "Lead"]),
            false,
        ));
        v.push((
            "32 px, touch 44".into(),
            "knob.3d",
            Place::new("Tone")
                .bind(mix())
                .size(32.0)
                .legend("Tone")
                .variant("pedal_chicken")
                .touch(TOUCH_MIN),
            false,
        ));
        v.push((
            "112 px gold".into(),
            "knob.3d",
            Place::new("Master")
                .bind(mix())
                .size(112.0)
                .legend("Master")
                .variant("gold_knurl")
                .ink(Ink::Dark),
            true,
        ));
        return v;
    }
    if only.iter().any(|o| o.starts_with("knob.render")) {
        // The rendered knob's range: shapes in materials, the look's dials, print.
        let k = |cap: &str, opts: &[&str]| {
            Place::new("Drive")
                .bind(mix())
                .size(64.0)
                .legend("Drive")
                .variant(cap)
                .options(opts)
        };
        let rows: [(&str, &str, &[&str], bool); 20] = [
            ("flat, metal (spun)", "flat", &["material=metal"], false),
            ("domed, chrome", "domed", &["material=chrome"], false),
            ("fluted, rubber", "fluted", &["material=rubber"], false),
            (
                "knurled, metal",
                "knurled",
                &["material=metal", "color=#8d9094"],
                false,
            ),
            (
                "flat, bakelite red",
                "flat",
                &["material=bakelite", "color=#a3241c", "skirt=none"],
                false,
            ),
            ("pointer, cream on ridged", "pointer", &[], true),
            (
                "chicken head, bakelite",
                "chicken_head",
                &["material=bakelite"],
                true,
            ),
            (
                "flat, flared skirt",
                "flat",
                &["skirt=flared", "color=#202022"],
                false,
            ),
            ("flat, chrome skirt", "flat", &["skirt=chrome"], false),
            ("fluted, white", "fluted", &["color=#e9e6df"], true),
            (
                "domed, bevel 2 spec 2 shadow 2",
                "domed",
                &["bevel=2", "spec=2", "shadow=2"],
                false,
            ),
            (
                "flat, bevel 0 spec 0",
                "flat",
                &["bevel=0", "spec=0", "shadow=0.5"],
                false,
            ),
            ("knurled, ticks=lines", "knurled", &["ticks=lines"], false),
            ("fluted, legend=top", "fluted", &["legend=top"], true),
            (
                "flat, green pedal knob",
                "knurled",
                &["color=#3a3b3d", "pointer=#ffffff"],
                false,
            ),
            ("domed, cream plastic", "domed", &["color=#efe6cf"], false),
            (
                "pointer, black cap",
                "pointer",
                &["color=#1b1b1c", "skirt=plain"],
                false,
            ),
            (
                "chicken head, ivory",
                "chicken_head",
                &["color=#efe9da"],
                false,
            ),
            (
                "flat, rubber, ticks none",
                "flat",
                &["material=rubber", "ticks=none"],
                false,
            ),
            ("fluted, red", "fluted", &["color=#8f1d1d"], true),
        ];
        for (title, cap, opts, light) in rows {
            let place = if light {
                k(cap, opts).ink(Ink::Dark)
            } else {
                k(cap, opts)
            };
            v.push((title.into(), "knob.render", place, light));
        }
        v.push((
            "stepped words".into(),
            "knob.render",
            Place::new("Mode")
                .bind(choice())
                .size(64.0)
                .legend("Mode")
                .variant("chicken_head")
                .options(&["Clean", "Crunch", "Lead"]),
            false,
        ));
        v.push((
            "scale 0..10".into(),
            "knob.render",
            Place::new("Gain")
                .bind(mix())
                .size(64.0)
                .legend("Gain")
                .variant("pointer")
                .scale(&["0", "2", "4", "6", "8", "10"])
                .options(&["ticks=lines"]),
            false,
        ));
        v.push((
            "32 px, touch 44".into(),
            "knob.render",
            Place::new("Tone")
                .bind(mix())
                .size(32.0)
                .legend("Tone")
                .variant("fluted")
                .touch(TOUCH_MIN),
            false,
        ));
        v.push((
            "112 px domed".into(),
            "knob.render",
            Place::new("Master")
                .bind(mix())
                .size(112.0)
                .legend("Master")
                .variant("domed"),
            false,
        ));
        return v;
    }
    if only.iter().any(|o| o.starts_with("panel.pedal_3d")) {
        let pd = |cls: &str, opts: &[&str]| Place::new("Pedal").variant(cls).options(opts);
        for (title, cls, opts) in [
            (
                "standard, powder red, screws",
                "standard",
                &["color=#c0392b", "screws"][..],
            ),
            (
                "standard, brushed",
                "standard",
                &["color=#c9cacc", "finish=brushed"],
            ),
            (
                "standard, hammertone",
                "standard",
                &["color=#56715c", "finish=hammertone"],
            ),
            (
                "standard, gloss purple",
                "standard",
                &["color=#7d5aa6", "finish=gloss"],
            ),
            (
                "wide, coarse powder",
                "wide",
                &["color=#2e3033", "finish=powder:coarse", "jacks=both"],
            ),
            ("micro, satin", "micro", &["color=#3b7d6e", "finish=satin"]),
            ("bigbox, chrome", "bigbox", &["finish=chrome", "jacks=top"]),
        ] {
            v.push((title.into(), "panel.pedal_3d", pd(cls, opts), false));
        }
        return v;
    }
    if only.iter().any(|o| o.starts_with("panel.amp")) {
        let amp = |opts: &[&str]| Place::new("Amp").size(500.0).options(opts);
        v.push((
            "head: cream bronco, chevron, bottom face".into(),
            "panel.amp_head",
            amp(&[
                "tolex=tolex:levant",
                "color=#e6dfcb",
                "grille=chevron",
                "grille_color=#e8e2d2",
                "face=powder",
                "face_color=#3a3b3e",
                "face_at=bottom",
                "face_frac=0.3",
                "piping=none",
            ]),
            false,
        ));
        v.push((
            "head: black bronco, diamond".into(),
            "panel.amp_head",
            amp(&[
                "tolex=tolex:bronco",
                "color=#1b1a19",
                "grille=diamond",
                "face=brushed",
                "face_color=#b9bbbd",
                "piping=#8a857a",
                "corners=black",
            ]),
            false,
        ));
        v.push((
            "combo: blonde, oxblood, strap".into(),
            "panel.amp_combo",
            amp(&[
                "tolex=tolex:bronco",
                "color=#d9c7a0",
                "grille=cloth:oxblood",
                "face=plastic",
                "face_color=#2a2020",
                "handle=strap",
                "handle_color=#3a2a20",
            ]),
            false,
        ));
        v.push((
            "combo: croc, basket, brass".into(),
            "panel.amp_combo",
            amp(&[
                "tolex=croc",
                "color=#2a2522",
                "grille=cloth:basket",
                "face=hammertone",
                "face_color=#7c8488",
                "corners=brass",
                "handle=leather",
            ]),
            false,
        ));
        v.push((
            "head: wood, perforated".into(),
            "panel.amp_head",
            amp(&[
                "tolex=wood",
                "grille=perforated",
                "face=chrome",
                "piping=none",
                "corners=none",
                "handle=none",
            ]),
            false,
        ));
        v.push((
            "head: elephant, expanded".into(),
            "panel.amp_head",
            amp(&[
                "tolex=tolex:elephant",
                "color=#2c2724",
                "grille=expanded",
                "face=brushed",
                "face_color=#c8b07a",
            ]),
            false,
        ));
        return v;
    }
    let all = family.is_none();
    if all || family == Some("knob") {
        v.push((
            "dark ink, kept case".into(),
            "knob.bakelite",
            Place::new("Trim")
                .bind(mix())
                .legend("Trim dB")
                .ink(Ink::Dark)
                .keep_case(),
            true,
        ));
        v.push((
            "touch(44): tight box".into(),
            "knob.collet",
            Place::new("Freq")
                .size(36.0)
                .bind(mix())
                .legend("Freq")
                .touch(TOUCH_MIN),
            false,
        ));
        v.push((
            "stepped by options".into(),
            "knob.chicken_head",
            Place::new("Mode")
                .bind(choice())
                .legend("Mode")
                .options(&["Clean", "Crunch", "Lead"]),
            false,
        ));
        v.push((
            "sweep 270, red ink".into(),
            "knob.neve",
            Place::new("Gain")
                .bind(mix())
                .legend("Gain")
                .sweep(270.0)
                .ink(Ink::Color("#d0342c".into())),
            false,
        ));
        v.push((
            "concentric grey_black".into(),
            "knob.concentric",
            Place::new("Freq")
                .bind(mix())
                .legend("kHz")
                .keep_case()
                .variant("grey_black")
                .inner(
                    Place::new("Gain")
                        .bind(Binding::param("amp/bass"))
                        .legend("Gain"),
                ),
            false,
        ));
        v.push((
            "stepped words at 44 px".into(),
            "knob.chicken_head",
            Place::new("Stage")
                .size(44.0)
                .bind(choice())
                .legend("Stage")
                .keep_case()
                .options(&["Triode", "Pentode", "Standard"]),
            false,
        ));
        v.push((
            "concentric grey, stepped".into(),
            "knob.concentric",
            Place::new("Freq")
                .bind(choice())
                .legend("kHz")
                .keep_case()
                .variant("grey")
                .options(&["Off", "·36", "·7", "1·6", "3·2"])
                .inner(Place::new("Gain").bind(Binding::param("amp/bass"))),
            false,
        ));
    }
    if all || family == Some("meter") {
        v.push((
            "VU on GR, no data: rests at 0".into(),
            "meter.vu_ivory",
            Place::new("GR")
                .bind(Binding::stream("comp/gr"))
                .size(180.0),
            false,
        ));
        v.push((
            "ladder: gr (5 dB), reversed".into(),
            "meter.led_ladder",
            Place::new("GR")
                .bind(level())
                .options(&["gr"])
                .reversed()
                .size(200.0),
            false,
        ));
        v.push((
            "column with scale".into(),
            "meter.led_column",
            Place::new("In")
                .bind(level())
                .options(&["floor=-30", "ceiling=6", "segments=12"])
                .scale(&["-30", "-18", "-6", "+6"]),
            false,
        ));
        v.push((
            "bar: colors=… per lamp".into(),
            "meter.led_bar",
            Place::new("Tail").bind(level()).options(&[
                "segments=6",
                "colors=#6f7cff,#8a7cff,#a57cff,#c07cff,#db7cff,#ff7ad9",
            ]),
            false,
        ));
        v.push((
            "VU: glass".into(),
            "meter.vu_amber",
            Place::new("GR")
                .bind(Binding::stream("comp/gr"))
                .size(180.0)
                .options(&["glass"]),
            false,
        ));
        v.push((
            "bar, dark-ink scale".into(),
            "meter.led_bar",
            Place::new("Out")
                .bind(level())
                .scale(&["-40", "-20", "0"])
                .variant("blue")
                .ink(Ink::Dark),
            true,
        ));
    }
    if all || family == Some("button") {
        v.push((
            "bank reversed, long legends".into(),
            "button.square_bank",
            Place::new("Algo")
                .bind(choice())
                .options(&["Hall", "Shimmer", "Spectral"])
                .reversed()
                .variant("yellow"),
            false,
        ));
        v.push((
            "bank fitted as one".into(),
            "button.illuminated_bank",
            Place::new("Voice")
                .bind(choice())
                .options(&["Clean", "Filtered", "Dark"]),
            false,
        ));
        v.push((
            "illuminated: momentary (TAP)".into(),
            "button.illuminated",
            Place::new("Tap")
                .bind(Binding::param("delay/tap"))
                .legend("Tap")
                .variant("red")
                .options(&["momentary"]),
            false,
        ));
        v.push((
            "push-in bank, dark ink".into(),
            "button.push_in_bank",
            Place::new("Ratio")
                .bind(choice())
                .options(&["4", "8", "12"])
                .ink(Ink::Dark),
            true,
        ));
    }
    if all || family == Some("switch") {
        v.push((
            "footswitch: momentary, blue".into(),
            "switch.footswitch",
            Place::new("Tap")
                .bind(Binding::param("delay/on"))
                .variant("blue")
                .options(&["momentary"])
                .legend("Tap"),
            false,
        ));
        v.push((
            "rotary: words, cream".into(),
            "switch.rotary",
            Place::new("Mode")
                .bind(choice())
                .options(&["Opto", "Stereo", "Harm"])
                .variant("cream")
                .legend("Mode"),
            false,
        ));
        v.push((
            "footswitch: color=#b77dff".into(),
            "switch.footswitch",
            Place::new("On")
                .bind(Binding::param("delay/on"))
                .options(&["color=#b77dff"])
                .legend("On"),
            false,
        ));
        v.push((
            "rotary: sweep 90, dark ink".into(),
            "switch.rotary",
            Place::new("Mode")
                .bind(choice())
                .options(&["A", "B", "C"])
                .sweep(90.0)
                .ink(Ink::Dark)
                .legend("Mode"),
            true,
        ));
    }
    if all || family == Some("lever") {
        v.push((
            "pultec, dark ink".into(),
            "lever.pultec",
            Place::new("CPS")
                .bind(choice())
                .options(&["20", "30", "60"])
                .ink(Ink::Dark)
                .legend("CPS"),
            true,
        ));
    }
    v
}
