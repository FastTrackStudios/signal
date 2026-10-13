//! Side-by-side proof sheets: the original art above, ours below.
//!
//!     cargo run -p daw-theme-art --example compare              # everything
//!     cargo run -p daw-theme-art --example compare mcp_fx_norm  # just one
//!     cargo run -p daw-theme-art --example compare fx           # anything matching
//!
//! `--example align` says *how far* a control is out, in pixels. This says
//! *what* is out, which is the other half — a number cannot tell you that
//! a button's right-hand neighbour is a scrim where yours is opaque, or
//! that two controls should share a top edge. Both faults were found by
//! eye, on crops made by hand, one control at a time; this makes that
//! sweep repeatable and covers everything at once rather than whatever
//! happened to be on screen.
//!
//! Writes `target/compare/theme/<name>.png` for each match plus a
//! `sheet.png` of all of them, at a zoom where single pixels are legible.

use daw_theme_art::{DerivedSpec, export};
use image::RgbaImage;

const ZOOM: u32 = 6;
const PAD: u32 = 10;
const LABEL_H: u32 = 14;
const BG: [u8; 4] = [40, 40, 40, 255];
const RULE: [u8; 4] = [90, 90, 90, 255];

fn source_dir() -> std::path::PathBuf {
    std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art").to_path_buf()
}

/// Nearest-neighbour zoom, so a pixel stays a pixel.
fn zoom(img: &RgbaImage, by: u32) -> RgbaImage {
    image::imageops::resize(
        img,
        img.width() * by,
        img.height() * by,
        image::imageops::FilterType::Nearest,
    )
}

/// Composite over the sheet background, since much of this art is
/// deliberately translucent and would otherwise be judged against black.
fn flatten(img: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(img.width(), img.height(), image::Rgba(BG));
    image::imageops::overlay(&mut out, img, 0, 0);
    out
}

/// A caption, rendered through the same SVG pipeline as the art so it
/// needs no font handling of its own.
fn label(text: &str, w: u32) -> Option<RgbaImage> {
    let y = LABEL_H - 4;
    let svg = format!(
        concat!(
            // `r##` because the markup contains `"#` — a hex colour right
            // after a quote — which closes an `r#` string early.
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" "##,
            r##"viewBox="0 0 {} {}"><text x="2" y="{}" "##,
            r##"font-family="Fira Sans, DejaVu Sans, sans-serif" "##,
            r##"font-size="11" fill="#c8c8c8">{}</text></svg>"##,
        ),
        w, LABEL_H, w, LABEL_H, y, text,
    );
    daw_theme_art::render::rasterise(&svg, w, LABEL_H).ok()
}

/// Source above, ours below, with the name over them.
fn tile(name: &str) -> Option<RgbaImage> {
    let src = image::open(source_dir().join(format!("{name}.png")))
        .ok()?
        .to_rgba8();
    let spec = DerivedSpec::from_image(&src);
    let ours = export::render_control(name, &spec).ok()?;

    let (a, b) = (zoom(&flatten(&src), ZOOM), zoom(&flatten(&ours), ZOOM));
    let w = a.width().max(b.width());
    let h = LABEL_H + a.height() + 2 + b.height();
    let mut out = RgbaImage::from_pixel(w, h, image::Rgba(BG));

    if let Some(l) = label(name, w) {
        image::imageops::overlay(&mut out, &l, 0, 0);
    }
    image::imageops::overlay(&mut out, &a, 0, LABEL_H as i64);
    // A hairline between them, so the eye knows where one stops.
    for x in 0..w {
        out.put_pixel(x, LABEL_H + a.height(), image::Rgba(RULE));
    }
    image::imageops::overlay(&mut out, &b, 0, (LABEL_H + a.height() + 2) as i64);
    Some(out)
}

fn main() {
    let filter = std::env::args().nth(1);
    let dir = std::path::Path::new("target/compare/theme");
    std::fs::create_dir_all(dir).expect("create output dir");

    let names: Vec<&str> = export::generatable()
        .into_iter()
        .filter(|n| filter.as_ref().is_none_or(|f| n.contains(f.as_str())))
        .collect();

    if names.is_empty() {
        eprintln!("nothing matches {:?}", filter.unwrap_or_default());
        return;
    }

    let mut tiles: Vec<(String, RgbaImage)> = Vec::new();
    for name in &names {
        match tile(name) {
            Some(t) => {
                let path = dir.join(format!("{name}.png"));
                t.save(&path).expect("write tile");
                tiles.push(((*name).to_string(), t));
            }
            None => eprintln!("  ! {name}: no source, or nothing drew it"),
        }
    }

    // One sheet, laid out in columns that fit the widest tile — a fixed
    // column count would waste most of the page on the 16px-wide controls
    // and clip the 108px ones.
    let cell_w = tiles.iter().map(|(_, t)| t.width()).max().unwrap_or(1) + PAD;
    let cell_h = tiles.iter().map(|(_, t)| t.height()).max().unwrap_or(1) + PAD;
    let cols = (1600 / cell_w).max(1);
    let rows = tiles.len().div_ceil(cols as usize) as u32;
    let mut sheet =
        RgbaImage::from_pixel(PAD + cols * cell_w, PAD + rows * cell_h, image::Rgba(BG));
    for (i, (_, t)) in tiles.iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        image::imageops::overlay(
            &mut sheet,
            t,
            (PAD + cx * cell_w) as i64,
            (PAD + cy * cell_h) as i64,
        );
    }
    let path = dir.join("sheet.png");
    sheet.save(&path).expect("write sheet");

    println!("{} compared, original above ours", tiles.len());
    println!("{}", path.display());
}
