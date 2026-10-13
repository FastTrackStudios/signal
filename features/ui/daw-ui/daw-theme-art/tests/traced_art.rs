//! The traced art must reproduce the originals exactly.
//!
//! This is the check the whole "1:1" claim rests on. Every image is
//! rendered from its traced rects in `Verbatim` mode — original colours,
//! no theming — and compared pixel-for-pixel against the source PNG. Any
//! difference is a tracing bug, and would otherwise show up much later as
//! a fidelity score that looks like a colour-mapping problem.
//!
//! Skipped when the source art isn't present (a checkout without the
//! theme), rather than failing.

use std::path::{Path, PathBuf};

use daw_theme_art::art_data::{ArtImage, ArtImageProps, ColorMode};
use daw_theme_art::generated;

fn source_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/reaper/fts-theme/FastTrackStudio/.source-art");
    dir.is_dir().then_some(dir)
}

/// resvg with fonts, matching the renderer.
fn options() -> resvg::usvg::Options<'static> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    opts
}

fn render(art: daw_theme_art::ArtData) -> image::RgbaImage {
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
    let tree = resvg::usvg::Tree::from_str(&svg, &options())
        .unwrap_or_else(|e| panic!("{} produced invalid SVG: {e}", art.name));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(art.width as u32, art.height as u32).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    daw_theme_art::render::to_rgba(&pixmap)
}

/// Replay traced rects straight into an image — no rasteriser.
///
/// This is the check that matters. Going through resvg loses precision at
/// low alpha (premultiplied storage turns 42 at alpha 8 into 32 on the way
/// back), so a rasterised comparison measures the rasteriser, not the
/// trace. Replaying the rects tests exactly the claim being made: the
/// traced data reproduces the original.
fn replay(art: daw_theme_art::ArtData) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(art.width as u32, art.height as u32);
    for r in art.rects() {
        for dy in 0..r.h {
            for dx in 0..r.w {
                img.put_pixel((r.x + dx) as u32, (r.y + dy) as u32, image::Rgba(r.rgba));
            }
        }
    }
    img
}

#[test]
fn every_traced_image_reproduces_its_source_exactly() {
    let Some(dir) = source_dir() else {
        eprintln!("no source art; skipping");
        return;
    };

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let markers = [[255u8, 0, 255, 255], [255, 255, 0, 255]];

    for art in generated::ALL {
        let path = dir.join(format!("{}.png", art.name));
        let Ok(src) = image::open(&path) else {
            continue;
        };
        let src = src.to_rgba8();
        checked += 1;

        if (src.width(), src.height()) != (art.width as u32, art.height as u32) {
            failures.push(format!(
                "{}: traced {}x{}, source {}x{}",
                art.name,
                art.width,
                art.height,
                src.width(),
                src.height()
            ));
            continue;
        }

        let ours = replay(*art);
        let mut diff = 0usize;
        for (a, b) in ours.pixels().zip(src.pixels()) {
            // Markers are deliberately not traced — they are stamped back
            // verbatim after rasterising.
            if markers.contains(&b.0) {
                continue;
            }
            // Fully transparent source pixels carry no colour worth
            // preserving; the tracer drops them and replay leaves zeroes.
            if b.0[3] == 0 && a.0[3] == 0 {
                continue;
            }
            if a.0 != b.0 {
                diff += 1;
            }
        }
        if diff > 0 {
            failures.push(format!("{}: {diff} pixels differ", art.name));
        }
    }

    assert!(
        checked > 500,
        "only checked {checked} images — is the art there?"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} images did not round-trip:\n  {}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// The rasterised render is allowed to drift, but only where it must.
///
/// Premultiplied storage costs precision in inverse proportion to alpha, so
/// the tolerance scales with it. A near-opaque pixel that is visibly wrong
/// still fails.
#[test]
fn rendering_traced_art_stays_close_to_the_source() {
    let Some(dir) = source_dir() else { return };
    let mut worst = 0u8;
    let mut worst_name = String::new();
    let markers = [[255u8, 0, 255, 255], [255, 255, 0, 255]];

    for art in generated::ALL.iter().take(200) {
        let path = dir.join(format!("{}.png", art.name));
        let Ok(src) = image::open(&path) else {
            continue;
        };
        let src = src.to_rgba8();
        if (src.width(), src.height()) != (art.width as u32, art.height as u32) {
            continue;
        }
        let ours = render(*art);
        for (a, b) in ours.pixels().zip(src.pixels()) {
            if markers.contains(&b.0) || b.0[3] < 64 {
                continue;
            }
            for i in 0..3 {
                let d = a.0[i].abs_diff(b.0[i]);
                if d > worst {
                    worst = d;
                    worst_name = art.name.to_string();
                }
            }
        }
    }
    assert!(
        worst <= 4,
        "rasterised render drifts by {worst} on {worst_name}"
    );
}

#[test]
fn the_index_and_the_blob_agree() {
    // A truncated blob decodes to fewer rects than the index claims, and
    // the missing ones are simply not drawn — silently.
    for art in generated::ALL {
        assert_eq!(
            art.rects().len(),
            art.count as usize,
            "{} claims {} rects but the blob yields {}",
            art.name,
            art.count,
            art.rects().len()
        );
    }
}

#[test]
fn lookup_finds_what_the_theme_asks_for() {
    for name in ["mcp_bg", "mcp_volbg", "mcp_solo_off", "mcp_volthumb"] {
        assert!(generated::by_name(name).is_some(), "missing {name}");
    }
    assert!(generated::by_name("definitely_not_a_theme_image").is_none());
}

/// The label on a button sits where the source puts it.
///
/// Vertical placement is not something `dominant-baseline: central` gets
/// right on its own: it centres on the font's own middle, and the source
/// glyphs are not centred in their cell either — mute's `M` occupies rows
/// 6..13 of 20, centred on 9.5. The two errors compounded and every label
/// rendered a full row low, which is easy to miss on one button and
/// obvious once a strip of them is stacked up.
///
/// Compares the rows holding near-white pixels — the glyph — rather than
/// anything about colour, so it survives the palette being retinted.
#[test]
fn button_labels_sit_where_the_source_puts_them() {
    use daw_theme_art::vector_controls as vector;

    let Some(dir) = source_dir() else { return };

    /// First and last row holding more than one near-white pixel.
    fn glyph_rows(img: &image::RgbaImage) -> Option<(u32, u32)> {
        let rows: Vec<u32> = (0..img.height())
            .filter(|&y| {
                (0..img.width())
                    .filter(|&x| {
                        let [r, g, b, a] = img.get_pixel(x, y).0;
                        let lo = r.min(g).min(b);
                        a > 128 && lo > 150 && r.max(g).max(b) - lo < 40
                    })
                    .count()
                    > 1
            })
            .collect();
        Some((*rows.first()?, *rows.last()?))
    }

    let n = (None, None);
    let cases: [(&str, u32, String); 3] = [
        (
            "mcp_mute_on",
            0,
            daw_theme_art::render_svg(
                vector::MuteButton,
                vector::ToggleProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.15,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_mute_on"),
                    on: true,
                    width: n.0,
                    height: n.1,
                    at: vector::Interaction::Normal,
                },
            ),
        ),
        (
            "mcp_solo_on",
            0,
            daw_theme_art::render_svg(
                vector::SoloButton,
                vector::SoloProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.11,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_solo_on"),
                    state: vector::Solo::On,
                    width: n.0,
                    height: n.1,
                    at: vector::Interaction::Normal,
                },
            ),
        ),
        (
            "mcp_fx_norm",
            1,
            daw_theme_art::render_svg(
                vector::FxButton,
                vector::FxProps {
                    family: Default::default(),
                    state: vector::FxChain::Active,
                    width: n.0,
                    height: n.1,
                    at: vector::Interaction::Normal,
                },
            ),
        ),
    ];

    for (name, cell, svg) in &cases {
        let art = generated::by_name(name).expect("art index entry");
        let src = image::open(dir.join(format!("{name}.png")))
            .expect("source png")
            .to_rgba8();
        let cw = src.width() / art.cells.max(1);
        let cropped = image::imageops::crop_imm(&src, cell * cw, 0, cw, src.height()).to_image();
        let want =
            glyph_rows(&cropped).unwrap_or_else(|| panic!("{name}: no glyph found in the source"));

        // Rasterise at REAPER's own size — a label can be perfectly placed
        // at 300px and a row out at 20px, and 20px is where it ships.
        let tree = resvg::usvg::Tree::from_str(svg, &options()).expect("valid svg");
        let mut pixmap = resvg::tiny_skia::Pixmap::new(cw, src.height()).expect("pixmap");
        let (tw, th) = (tree.size().width(), tree.size().height());
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(cw as f32 / tw, src.height() as f32 / th),
            &mut pixmap.as_mut(),
        );
        let got = glyph_rows(&daw_theme_art::render::to_rgba(&pixmap))
            .unwrap_or_else(|| panic!("{name}: the vector drew no glyph at all"));

        assert!(
            want.0.abs_diff(got.0) <= 1 && want.1.abs_diff(got.1) <= 1,
            "{name}: source glyph spans rows {want:?} but the vector draws it at {got:?}",
        );
    }
}

/// A vector control exports as a REAPER strip with its guides intact.
///
/// The unit tests for this use a synthetic spec; this runs it against the
/// theme's real art, where the marker layout is whatever ReaperTips
/// actually shipped rather than whatever seemed reasonable. `mcp_fx_norm`
/// is the interesting one: 86px of three cells is 28.67 each, so integer
/// division drops a column, and `mcp_io_s_r` carries live magenta guides
/// down its left edge.
#[test]
fn exported_controls_keep_the_sources_guides() {
    let Some(dir) = source_dir() else { return };

    for name in [
        "mcp_mute_on",
        "mcp_fx_norm",
        "mcp_io_s_r",
        "mcp_recarm_auto",
    ] {
        let src = image::open(dir.join(format!("{name}.png")))
            .expect("source png")
            .to_rgba8();
        let spec = daw_theme_art::DerivedSpec::from_image(&src);
        let out =
            daw_theme_art::render_control(name, &spec).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(
            out.dimensions(),
            (spec.width, spec.height),
            "{name}: wrong size for the image it replaces",
        );

        for &(x, y, rgba) in &spec.markers {
            assert_eq!(
                out.get_pixel(x, y).0,
                rgba,
                "{name}: guide at {x},{y} did not survive export",
            );
        }

        // Every cell must have been drawn into. Checked per cell rather
        // than per column: a control legitimately leaves its outermost
        // column clear — that gap is what separates the buttons in the
        // strip — so "no blank columns" fails on correct output. A
        // *skipped* cell is the thing that matters, and it is easy to miss
        // against a dark mixer while being fatal to the layout.
        for (i, &(x, w)) in spec.cells.iter().enumerate() {
            let drawn = (x..x + w)
                .flat_map(|cx| (0..out.height()).map(move |cy| (cx, cy)))
                .filter(|&(cx, cy)| out.get_pixel(cx, cy).0[3] > 8)
                .count();
            assert!(
                drawn > (w * out.height() / 8) as usize,
                "{name}: cell {i} at x={x} is essentially empty ({drawn} px)",
            );
        }
    }
}

/// Cell layouts, against the real art rather than a fixture.
///
/// Every wrong answer here is silent: too few cells stretches one state
/// across the whole strip, too many squeezes four buttons where three
/// belong, and a wrong offset shifts every control sideways. All three
/// happened while this was being written, and none failed a test — they
/// were caught by looking at a contact sheet.
///
/// **The count is asserted exactly; the origin only to within two
/// pixels.** REAPER divides a strip by its own arithmetic, and 71/3 or
/// 86/3 do not come out whole, so cells are not all the same width and
/// the gutter between two of them is two pixels wide. Which pixel of that
/// gutter a boundary falls on is not observable from the art — both
/// answers contain the whole drawing — so pinning it exactly would be
/// asserting an implementation detail of the detector rather than a fact
/// about the theme.
#[test]
fn strips_are_split_where_the_art_says() {
    let Some(dir) = source_dir() else { return };

    // Origins traced by hand from the first drawn column of each cell.
    let cases: [(&str, usize, u32, &[u32]); 9] = [
        ("mcp_recarm_on", 3, 36, &[0, 36, 72]),
        ("mcp_mute_on", 3, 21, &[0, 21, 42]),
        ("mcp_fx_norm", 3, 28, &[1, 29, 57]),
        ("mcp_io_s_r", 3, 23, &[2, 25, 48]),
        ("track_recarm_on", 3, 20, &[0, 20, 40]),
        ("track_mute_on", 3, 21, &[1, 22, 43]),
        ("track_fx_norm", 3, 20, &[1, 21, 41]),
        ("track_monitor_on", 3, 15, &[1, 16, 31]),
        // Not a strip at all: one drawing, whole width.
        ("mcp_volthumb", 1, 27, &[0]),
    ];

    for (name, cells, width, origins) in cases {
        let img = image::open(dir.join(format!("{name}.png")))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_rgba8();
        let got = daw_theme_art::derive::cell_bounds(&img);

        assert_eq!(got.len(), cells, "{name}: wrong cell count — {got:?}");
        for (i, (&(x, w), &want)) in got.iter().zip(origins).enumerate() {
            assert!(
                w.abs_diff(width) <= 1,
                "{name}: cell {i} is {w}px, expected about {width}",
            );
            assert!(
                x.abs_diff(want) <= 2,
                "{name}: cell {i} starts at {x}, expected about {want}",
            );
        }
    }
}

/// Every declared slice still equals the guides in the art it replaces.
///
/// The highest-value assertion in the set, and the reason the slice table
/// exists before anything renders from it. A `NamedArt` states a source box
/// and which band of it stretches; the source PNG states the same thing in
/// magenta, in REAPER's own notation. When they disagree, one of them is a
/// lie — either the table has gone stale, or a control has been redrawn
/// into a band it does not belong in.
///
/// The second failure is the one worth catching. It is invisible at the
/// source size, where every slice is the identity, and shows up only when a
/// mixer is resized on a machine that is not this one.
#[test]
fn declared_slices_match_the_source_art() {
    let Some(dir) = source_dir() else { return };
    let bad = daw_theme_art::export::slice_mismatches(&dir);
    assert!(
        bad.is_empty(),
        "the slice table disagrees with the art:\n  {}",
        bad.join("\n  ")
    );
}

/// The table covers every MCP image the exporter can draw.
///
/// A control wired into `cell_markup` but left out of the table is a gap
/// the audit above cannot see: it only checks what is declared. The list of
/// exclusions is the ticket's scope, written down where it can be argued
/// with rather than inferred from what happens to be present.
#[test]
fn every_mcp_image_the_exporter_draws_is_declared() {
    let out_of_scope = |name: &str| {
        name.starts_with("transport_")
            // The envelope panel's own controls and furniture. Its three
            // *plates* are in the table, because `PanelPlate` draws those
            // and the prop carrying a source box has to be one thing per
            // component; its fader, knob and buttons are drawn by `Envcp*`
            // components that keep their `cell`.
            || (name.starts_with("envcp_")
                && !matches!(name, "envcp_bg" | "envcp_bgsel" | "envcp_namebg"))
            || name == "custom_envcp_arm_bg"
            // Track-panel-only controls, which no mixer strip has room for.
            || name.starts_with("track_env")
            || name.starts_with("track_recmode_")
            || name.starts_with("track_fcomp_")
            || name.starts_with("track_folder_")
            || name.starts_with("track_fx_in_")
    };
    let missing: Vec<&str> = daw_theme_art::export::generatable()
        .into_iter()
        .filter(|n| !out_of_scope(n) && daw_theme_art::slice::art(n).is_none())
        .collect();
    assert!(missing.is_empty(), "MCP art with no NamedArt: {missing:?}");
}
