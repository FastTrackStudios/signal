//! Show exactly which pixels of one traced image differ from its source.
//!
//!     cargo run -p daw-theme-art --example diffone -- custom_mcp_folder_start

use daw_theme_art::art_data::{ArtImage, ArtImageProps, ColorMode};
use daw_theme_art::generated;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "mcp_bg".into());
    let dir = std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art");
    let art = generated::by_name(&name).unwrap_or_else(|| panic!("no art {name}"));
    let src = image::open(dir.join(format!("{name}.png")))
        .unwrap()
        .to_rgba8();

    let svg = daw_theme_art::render_svg(
        ArtImage,
        ArtImageProps {
            art,
            width: None,
            height: None,
            mode: ColorMode::Verbatim,
            cell: 0,
        },
    );
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    let tree = resvg::usvg::Tree::from_str(&svg, &opts).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(art.width as u32, art.height as u32).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let ours = daw_theme_art::render::to_rgba(&pixmap);

    println!("{name}: {}x{}, {} rects", art.width, art.height, art.count);
    let markers = [[255u8, 0, 255, 255], [255, 255, 0, 255]];
    let mut shown = 0;
    for y in 0..src.height() {
        for x in 0..src.width() {
            let (a, b) = (ours.get_pixel(x, y).0, src.get_pixel(x, y).0);
            if markers.contains(&b) {
                continue;
            }
            if (0..4).any(|i| a[i].abs_diff(b[i]) > 1) {
                println!("  ({x},{y}) ours {a:?}  src {b:?}");
                shown += 1;
                if shown >= 15 {
                    println!("  …");
                    return;
                }
            }
        }
    }
    if shown == 0 {
        println!("  identical");
    }
}
