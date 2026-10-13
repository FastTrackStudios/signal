//! Paint every bundled font family to one PNG, for review.
//!
//! ```text
//! font_sheet --out fonts.png [--scale 2]
//! ```
//! One row per family (`frame::render::fonts::BUNDLED`): its name, what it suits,
//! and a sample set at its lightest, regular and heaviest weights, then
//! italic; then Inter's weight ramp and Saira's width axis. Everything is
//! plain `<Text>` nodes set by the renderer's font book — what a surface
//! gets when it names these families.

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("font_sheet requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(e) = run() {
        eprintln!("font_sheet: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use frame::document::proto::{Node, NodeId, Scene, TextAlign};
    use frame::surface::kit::draw::{self, Lettering};

    let mut out = None;
    let mut scale = 1.0_f64;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--scale" => {
                scale = args
                    .next()
                    .ok_or("--scale needs a factor")?
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?
            }
            other => return Err(format!("unknown arg {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;

    fn add(scene: &mut Scene, parent: NodeId, mut n: Node) -> NodeId {
        let id = n.id;
        n.parent = Some(parent);
        if let Some(p) = scene.node_mut(parent) {
            p.children.push(id);
        }
        scene.nodes.insert(id, n);
        id
    }
    fn sample(name: &str) -> &'static str {
        match name {
            "DSEG7 Classic" => "88:88  440.0",
            "DSEG14 Classic" => "PRESET 07  CLEAN",
            "Doto" | "VT323" => "BPM 120.0",
            "JetBrains Mono" => "-12.5 dB 1k2",
            "UnifrakturCook" | "Pirata One" => "Blackout Gain",
            "Dancing Script" | "Pacifico" => "Funk Works",
            "Caveat Brush" | "Knewave" => "Djent Forge!",
            "Michroma" | "Orbitron" => "STEREO DELAY",
            _ => "Studio Blues 1959",
        }
    }

    const W: f64 = 1500.0;
    const ROW: f64 = 92.0;
    let fams = frame::render::fonts::BUNDLED;
    let h = 90.0 + fams.len() as f64 * ROW + 250.0;
    let mut scene = Scene::new("Fonts");
    let page = scene.pages()[0];
    let mut bg = draw::rect("sheet", 0.0, 0.0, W, h, draw::solid("#141519"));
    bg.name = "Font sheet".into();
    let root = add(&mut scene, page, bg);
    let ink = "#f2efe6";
    let dim = "#8b8f98";
    add(
        &mut scene,
        root,
        draw::styled_text(
            "title",
            "frame — bundled fonts",
            24.0,
            20.0,
            900.0,
            30.0,
            ink,
            TextAlign::Left,
            &Lettering::family("Oswald").weight(600.0),
        ),
    );
    add(
        &mut scene,
        root,
        draw::text(
            "sub",
            "each family at its lightest · regular · heaviest weight, then italic (real, or slanted when it has none); all SIL OFL 1.1",
            24.0,
            58.0,
            1400.0,
            13.0,
            dim,
            TextAlign::Left,
        ),
    );

    for (i, f) in fams.iter().enumerate() {
        let y = 90.0 + i as f64 * ROW;
        add(
            &mut scene,
            root,
            draw::rect("rule", 24.0, y, W - 48.0, 1.0, draw::solid("#2a2c33")),
        );
        add(
            &mut scene,
            root,
            draw::styled_text(
                "name",
                f.name,
                24.0,
                y + 10.0,
                230.0,
                17.0,
                ink,
                TextAlign::Left,
                &Lettering::default().weight(600.0),
            ),
        );
        let weights = if f.weights.0 == f.weights.1 {
            f.weights.0.to_string()
        } else {
            format!("{}–{}", f.weights.0, f.weights.1)
        };
        add(
            &mut scene,
            root,
            draw::text(
                "weights",
                &format!(
                    "weight {weights}{}",
                    if f.italic { " · italic" } else { "" }
                ),
                24.0,
                y + 34.0,
                230.0,
                11.0,
                dim,
                TextAlign::Left,
            ),
        );
        let role = f.role.split(':').next().unwrap_or(f.role);
        add(
            &mut scene,
            root,
            draw::text(
                "role",
                role,
                24.0,
                y + 52.0,
                230.0,
                11.0,
                dim,
                TextAlign::Left,
            ),
        );
        let (lo, hi) = (f64::from(f.weights.0), f64::from(f.weights.1));
        let mut cells: Vec<Lettering> = vec![Lettering::family(f.name).weight(lo)];
        if lo < 400.0 && hi > 400.0 {
            cells.push(Lettering::family(f.name).weight(400.0));
        }
        if hi > lo {
            cells.push(Lettering::family(f.name).weight(hi));
        } else {
            cells.push(Lettering::family(f.name).weight(800.0));
        }
        cells.push(
            Lettering::family(f.name)
                .weight(lo.max(400.0).min(hi))
                .italic(),
        );
        let cw = (W - 280.0 - 24.0) / cells.len() as f64;
        for (k, l) in cells.iter().enumerate() {
            let x = 280.0 + k as f64 * cw;
            let tag = format!(
                "{}{}",
                l.weight.map_or(String::new(), |w| format!("{w}")),
                if l.italic { " italic" } else { "" }
            );
            let tag = if hi == lo && l.weight == Some(800.0) {
                "800 (emboldened)".to_string()
            } else {
                tag
            };
            add(
                &mut scene,
                root,
                draw::text(
                    "tag",
                    &tag,
                    x,
                    y + 8.0,
                    cw - 10.0,
                    10.0,
                    dim,
                    TextAlign::Left,
                ),
            );
            add(
                &mut scene,
                root,
                draw::styled_text(
                    "sample",
                    sample(f.name),
                    x,
                    y + 26.0,
                    cw - 10.0,
                    30.0,
                    ink,
                    TextAlign::Left,
                    l,
                ),
            );
        }
    }

    // Inter's weight ramp and Saira's width axis.
    let y = 90.0 + fams.len() as f64 * ROW + 20.0;
    add(
        &mut scene,
        root,
        draw::styled_text(
            "ramp",
            "Inter 100 → 900",
            24.0,
            y,
            250.0,
            17.0,
            ink,
            TextAlign::Left,
            &Lettering::default().weight(600.0),
        ),
    );
    for k in 0..9 {
        let w = 100.0 * f64::from(k + 1);
        add(
            &mut scene,
            root,
            draw::styled_text(
                "w",
                "Gain",
                280.0 + f64::from(k) * 130.0,
                y - 4.0,
                128.0,
                34.0,
                ink,
                TextAlign::Left,
                &Lettering::default().weight(w),
            ),
        );
    }
    let y = y + 70.0;
    add(
        &mut scene,
        root,
        draw::styled_text(
            "widths",
            "Saira width 50 → 125",
            24.0,
            y,
            250.0,
            17.0,
            ink,
            TextAlign::Left,
            &Lettering::default().weight(600.0),
        ),
    );
    let mut x = 280.0;
    for wd in [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0] {
        let l = Lettering::family("Saira").weight(700.0).width(wd);
        let adv = frame::render::text_advance(
            frame::render::FontBook::bundled(),
            &frame::render::fonts::FontSpec {
                family: l.family.clone(),
                weight: l.weight,
                style: None,
                width: l.width,
            },
            "RACK",
            30.0,
            0.0,
        );
        add(
            &mut scene,
            root,
            draw::styled_text(
                "wd",
                "RACK",
                x,
                y - 4.0,
                adv + 20.0,
                30.0,
                ink,
                TextAlign::Left,
                &l,
            ),
        );
        add(
            &mut scene,
            root,
            draw::text(
                "wdtag",
                &format!("{wd}"),
                x,
                y + 34.0,
                80.0,
                10.0,
                dim,
                TextAlign::Left,
            ),
        );
        x += adv + 34.0;
    }
    let y = y + 80.0;
    add(
        &mut scene,
        root,
        draw::styled_text(
            "fallback",
            "Fallback",
            24.0,
            y,
            250.0,
            17.0,
            ink,
            TextAlign::Left,
            &Lettering::default().weight(600.0),
        ),
    );
    add(
        &mut scene,
        root,
        draw::styled_text(
            "fb1",
            "DSEG7 \"Tuner é\" → 440.0 Hz",
            280.0,
            y - 4.0,
            600.0,
            30.0,
            ink,
            TextAlign::Left,
            &Lettering::family("DSEG7 Classic"),
        ),
    );
    add(
        &mut scene,
        root,
        draw::styled_text(
            "fb2",
            "\"Helvetica Neue\" → Inter",
            900.0,
            y - 4.0,
            560.0,
            30.0,
            ink,
            TextAlign::Left,
            &Lettering::family("Helvetica Neue").weight(700.0),
        ),
    );

    let (w, hh) = ((W * scale).ceil() as u32, (h * scale).ceil() as u32);
    let mut vs = vello::Scene::new();
    frame::render::paint_scene_into(
        &mut vs,
        &scene,
        page,
        kurbo::Affine::scale(scale),
        Some(frame::render::FontBook::bundled()),
    );
    let px = frame::render::offscreen::Offscreen::new()?.render(
        &vs,
        w,
        hh,
        peniko::Color::from_rgba8(20, 21, 25, 255),
    )?;
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixels")?
        .save(&out)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{out}: {} families, {w}×{hh}", fams.len());
    Ok(())
}
