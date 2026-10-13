//! Trace the source artwork into a packed blob plus an index, once.
//!
//!     cargo run -p daw-theme-art --example codegen -- <src-dir>
//!
//! Structure is the original's exactly; colour is reinterpreted at render
//! time through the theme ramp, so this does **not** need regenerating when
//! the palette changes — only when the source artwork does.
//!
//! Rects go in a binary blob rather than Rust consts. The const form was
//! 21 MB of source for 338k `Rect { .. }` literals, which compiles slowly
//! and bloats every build touching this crate for nothing.

use std::collections::BTreeMap;
use std::fmt::Write as _;

const PACKED_RECT: usize = 12;

fn main() {
    let mut args = std::env::args().skip(1);
    let src = args
        .next()
        .unwrap_or_else(|| "features/reaper/fts-theme/FastTrackStudio/.source-art".into());
    let crate_dir = args
        .next()
        .unwrap_or_else(|| "features/daw-ui/daw-theme-art".into());

    let src = std::path::Path::new(&src);
    let crate_dir = std::path::Path::new(&crate_dir);

    let mut paths: Vec<_> = std::fs::read_dir(src)
        .unwrap_or_else(|e| panic!("read {}: {e}", src.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    paths.sort();

    let mut blob: Vec<u8> = Vec::new();
    let mut index: BTreeMap<String, (u16, u16, u32, u32, u32)> = BTreeMap::new();

    for path in &paths {
        let Ok(img) = image::open(path) else { continue };
        let img = img.to_rgba8();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let rects = daw_theme_art::trace::trace(&img);

        let offset = blob.len() as u32;
        for r in &rects {
            blob.extend_from_slice(&r.x.to_le_bytes());
            blob.extend_from_slice(&r.y.to_le_bytes());
            blob.extend_from_slice(&r.w.to_le_bytes());
            blob.extend_from_slice(&r.h.to_le_bytes());
            blob.extend_from_slice(&r.rgba);
        }
        // Sprite-cell count is baked in: a consumer showing one state needs
        // it, and detecting it at render time would mean decoding the image
        // again on every draw.
        let cells = daw_theme_art::derive::sprite_cells(&img);
        index.insert(
            name,
            (
                img.width() as u16,
                img.height() as u16,
                offset,
                rects.len() as u32,
                cells,
            ),
        );
    }

    let blob_path = crate_dir.join("src/art.bin");
    std::fs::write(&blob_path, &blob).unwrap_or_else(|e| panic!("write blob: {e}"));

    let mut out = String::new();
    let _ = writeln!(
        out,
        "//! Traced theme artwork — GENERATED, do not edit.\n\
         //!\n\
         //! ```sh\n\
         //! cargo run -p daw-theme-art --example codegen\n\
         //! ```\n\
         //!\n\
         //! Geometry is traced from the source art exactly; colour is\n\
         //! reinterpreted at render time via the theme ramp — so a palette\n\
         //! change does NOT require regenerating this, only a change to the\n\
         //! source artwork does.\n\
         //!\n\
         //! {} images, {} rects, {:.1} MB of packed rect data.\n",
        index.len(),
        blob.len() / PACKED_RECT,
        blob.len() as f32 / 1_048_576.0
    );
    let _ = writeln!(out, "use crate::art_data::ArtData;\n");
    let _ = writeln!(
        out,
        "/// Packed rects: x, y, w, h as u16 LE, then rgba.\n\
         pub static BLOB: &[u8] = include_bytes!(\"art.bin\");\n"
    );

    for (name, (w, h, offset, count, cells)) in &index {
        let ident = to_ident(name);
        let _ = writeln!(
            out,
            "/// `{name}.png` — {w}x{h}, {count} rects, {cells} sprite cell(s).\n\
             pub const {ident}: ArtData = ArtData {{ name: {name:?}, width: {w}, height: {h}, offset: {offset}, count: {count}, cells: {cells}, blob: BLOB }};"
        );
    }

    let _ = writeln!(
        out,
        "\n/// Every traced image, by REAPER file name.\n\
         pub static ALL: &[ArtData] = &["
    );
    for name in index.keys() {
        let _ = writeln!(out, "    {},", to_ident(name));
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(
        out,
        "\n/// Look one up by REAPER file name.\n\
         pub fn by_name(name: &str) -> Option<ArtData> {{\n\
         \x20   ALL.iter().find(|a| a.name == name).copied()\n\
         }}"
    );

    let rs_path = crate_dir.join("src/generated.rs");
    std::fs::write(&rs_path, &out).unwrap_or_else(|e| panic!("write rs: {e}"));

    println!(
        "{} images, {} rects\n  {} ({:.1} MB)\n  {} ({:.0} KB)",
        index.len(),
        blob.len() / PACKED_RECT,
        blob_path.display(),
        blob.len() as f32 / 1_048_576.0,
        rs_path.display(),
        out.len() as f32 / 1024.0
    );
}

/// `mcp_solo_off` -> `MCP_SOLO_OFF`, made a valid identifier.
fn to_ident(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    // REAPER has names starting with a digit; a prefix keeps them valid
    // rather than silently dropping the image.
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 'X');
    }
    s
}
