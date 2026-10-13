//! Rasterise each primitive to a PNG, for looking at.
//!
//!     cargo run -p daw-theme-art --example contact
//!
//! Rendered at 3× so a one-pixel border is judgeable by eye — these are
//! drawn at the sizes REAPER actually uses, which are tiny.

use daw_theme::{Color, Theme};
use daw_theme_art::primitives::*;
use daw_theme_art::render::render_svg;

fn save(name: &str, svg: &str, w: u32, h: u32) {
    let mut opts = resvg::usvg::Options::default();
    // An empty font database renders SVG text as nothing at all — silently.
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    opts.fontdb = std::sync::Arc::new(db);
    let tree = match resvg::usvg::Tree::from_str(svg, &opts) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{name}: invalid SVG: {e}");
            return;
        }
    };
    let scale = 3.0;
    let (pw, ph) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let dir = std::path::Path::new("target/art-contact");
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    image::RgbaImage::from_raw(pw, ph, pixmap.take())
        .unwrap()
        .save(&path)
        .unwrap();
    println!("wrote {}", path.display());
}

fn main() {
    let t = Theme::default();

    save(
        "panel",
        &render_svg(
            Panel,
            SizeProps {
                width: 60,
                height: 40,
            },
        ),
        60,
        40,
    );
    save(
        "groove",
        &render_svg(
            Groove,
            SizeProps {
                width: 22,
                height: 90,
            },
        ),
        22,
        90,
    );
    save(
        "thumb",
        &render_svg(
            Thumb,
            ThumbProps {
                width: 22,
                height: 14,
                accent: None,
            },
        ),
        22,
        14,
    );
    save(
        "thumb-track",
        &render_svg(
            Thumb,
            ThumbProps {
                width: 22,
                height: 14,
                accent: Some(Color::rgb(0x3d, 0xdc, 0x97)),
            },
        ),
        22,
        14,
    );

    for (name, state) in [
        ("button-off", ControlState::Off),
        ("button-on", ControlState::On),
        ("button-hover", ControlState::Hover),
        ("button-pressed", ControlState::Pressed),
        ("button-disabled", ControlState::Disabled),
    ] {
        save(
            name,
            &render_svg(
                Button,
                ButtonProps {
                    width: 24,
                    height: 16,
                    state,
                    label: Some("M".into()),
                    on_color: None,
                },
            ),
            24,
            16,
        );
    }

    save(
        "button-mute-on",
        &render_svg(
            Button,
            ButtonProps {
                width: 24,
                height: 16,
                state: ControlState::On,
                label: Some("M".into()),
                on_color: Some(t.signal.mute),
            },
        ),
        24,
        16,
    );

    for (name, level) in [("meter-low", 0.25), ("meter-mid", 0.6), ("meter-hot", 0.95)] {
        save(
            name,
            &render_svg(
                Meter,
                MeterProps {
                    tag: String::new(),
                    hold: None,
                    width: 10,
                    height: 90,
                    level,
                },
            ),
            10,
            90,
        );
    }
}
