//! The full round trip, three rows per control:
//!
//!     1. the original ReaperTips PNG cell
//!     2. the vector Dioxus component, rendered as vector
//!     3. that component rasterised back to REAPER's native pixel size
//!
//!     cargo run -p daw-theme-art --example roundtrip
//!
//! Rows 1 and 3 are what REAPER actually blits, so they are the honest
//! comparison — both are nearest-neighbour upscaled from native size, no
//! smoothing, so you see real pixels. Row 2 shows what the same component
//! gives the web, where it is not constrained to 21x20.
//!
//! Row 3 is the one that matters for "does this work as a theme": a vector
//! can look perfect at 300px and fall apart at 20px, where a 1px border and
//! a 9px glyph have nowhere to go.

use daw_theme_art::render::render_svg;
use daw_theme_art::{generated, slice, vector_controls as vector};

const DISPLAY: u32 = 84;
const PAD: u32 = 8;
const BG: [u8; 4] = [16, 16, 21, 255];

fn source_dir() -> std::path::PathBuf {
    std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art").to_path_buf()
}

/// Rasterise SVG at an exact pixel size — REAPER's, when round-tripping.
fn raster(svg: &str, w: u32, h: u32) -> Option<image::RgbaImage> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    let tree = resvg::usvg::Tree::from_str(svg, &opts).ok()?;
    let (vw, vh) = (tree.size().width(), tree.size().height());
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(w as f32 / vw, h as f32 / vh),
        &mut pixmap.as_mut(),
    );
    Some(daw_theme_art::render::to_rgba(&pixmap))
}

/// Blow a native-size image up without smoothing, so pixels stay pixels.
fn zoom(img: &image::RgbaImage, to_h: u32) -> image::RgbaImage {
    let f = (to_h / img.height().max(1)).max(1);
    image::imageops::resize(
        img,
        img.width() * f,
        img.height() * f,
        image::imageops::FilterType::Nearest,
    )
}

/// One cell of an original PNG, at native size.
fn original_cell(name: &str, cell: u32) -> Option<image::RgbaImage> {
    let art = generated::by_name(name)?;
    let img = image::open(source_dir().join(format!("{name}.png"))).ok()?;
    let img = img.to_rgba8();
    let cw = img.width() / art.cells.max(1);
    let x = (cell.min(art.cells.saturating_sub(1))) * cw;
    Some(image::imageops::crop_imm(&img, x, 0, cw, img.height()).to_image())
}

struct Entry {
    /// The REAPER image this state maps to, and which cell.
    source: (&'static str, u32),
    svg: String,
}

fn main() {
    let mut groups: Vec<(&str, Vec<Entry>)> = Vec::new();
    let n = (None, None);

    groups.push((
        "record arm",
        vec![
            Entry {
                source: ("mcp_recarm_off", 0),
                svg: render_svg(
                    vector::RecordArmButton,
                    vector::RecordArmProps {
                        art: slice::expect_art("mcp_recarm_off"),
                        housing: true,
                        state: vector::RecordArm::Off,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_recarm_on", 0),
                svg: render_svg(
                    vector::RecordArmButton,
                    vector::RecordArmProps {
                        art: slice::expect_art("mcp_recarm_on"),
                        housing: true,
                        state: vector::RecordArm::On,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_recarm_norec", 0),
                svg: render_svg(
                    vector::RecordArmButton,
                    vector::RecordArmProps {
                        art: slice::expect_art("mcp_recarm_norec"),
                        housing: true,
                        state: vector::RecordArm::NoRecord,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_recarm_auto", 0),
                svg: render_svg(
                    vector::RecordArmButton,
                    vector::RecordArmProps {
                        art: slice::expect_art("mcp_recarm_auto"),
                        housing: true,
                        state: vector::RecordArm::Auto,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "mute",
        vec![
            Entry {
                source: ("mcp_mute_off", 0),
                svg: render_svg(
                    vector::MuteButton,
                    vector::ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        art: slice::expect_art("mcp_mute_off"),
                        body: (0.0, 1.0),
                        on: false,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_mute_on", 0),
                svg: render_svg(
                    vector::MuteButton,
                    vector::ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        art: slice::expect_art("mcp_mute_on"),
                        body: (0.0, 1.0),
                        on: true,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_mute_on", 1),
                svg: render_svg(
                    vector::MuteButton,
                    vector::ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        art: slice::expect_art("mcp_mute_on"),
                        body: (0.0, 1.0),
                        on: true,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Hover,
                    },
                ),
            },
            Entry {
                source: ("mcp_mute_on", 2),
                svg: render_svg(
                    vector::MuteButton,
                    vector::ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        art: slice::expect_art("mcp_mute_on"),
                        body: (0.0, 1.0),
                        on: true,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Pressed,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "solo",
        vec![
            Entry {
                source: ("mcp_solo_off", 0),
                svg: render_svg(
                    vector::SoloButton,
                    vector::SoloProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.11,
                        legend: None,
                        art: slice::expect_art("mcp_solo_off"),
                        body: (0.0, 1.0),
                        state: vector::Solo::Off,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_solo_on", 0),
                svg: render_svg(
                    vector::SoloButton,
                    vector::SoloProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.11,
                        legend: None,
                        art: slice::expect_art("mcp_solo_on"),
                        body: (0.0, 1.0),
                        state: vector::Solo::On,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_solodefeat_on", 0),
                svg: render_svg(
                    vector::SoloButton,
                    vector::SoloProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.11,
                        legend: None,
                        art: slice::expect_art("mcp_solodefeat_on"),
                        body: (0.0, 1.0),
                        state: vector::Solo::Defeat,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "fx",
        vec![
            Entry {
                source: ("mcp_fx_empty", 0),
                svg: render_svg(
                    vector::FxButton,
                    vector::FxProps {
                        family: Default::default(),
                        state: vector::FxChain::Empty,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_fx_norm", 0),
                svg: render_svg(
                    vector::FxButton,
                    vector::FxProps {
                        family: Default::default(),
                        state: vector::FxChain::Active,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_fx_dis", 0),
                svg: render_svg(
                    vector::FxButton,
                    vector::FxProps {
                        family: Default::default(),
                        state: vector::FxChain::Bypassed,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "routing",
        vec![
            Entry {
                source: ("mcp_io", 0),
                svg: render_svg(
                    vector::RoutingButton,
                    vector::RoutingProps {
                        art: slice::expect_art("mcp_io"),
                        axis: Default::default(),
                        has_sends: false,
                        has_receives: false,
                        disabled: false,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_io_s", 0),
                svg: render_svg(
                    vector::RoutingButton,
                    vector::RoutingProps {
                        art: slice::expect_art("mcp_io_s"),
                        axis: Default::default(),
                        has_sends: true,
                        has_receives: false,
                        disabled: false,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_io_s_r", 0),
                svg: render_svg(
                    vector::RoutingButton,
                    vector::RoutingProps {
                        art: slice::expect_art("mcp_io_s_r"),
                        axis: Default::default(),
                        has_sends: true,
                        has_receives: true,
                        disabled: false,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "input monitor",
        vec![
            Entry {
                source: ("mcp_monitor_off", 0),
                svg: render_svg(
                    vector::InputMonitorIndicator,
                    vector::MonitoringProps {
                        art: slice::expect_art("mcp_monitor_off"),
                        axis: Default::default(),
                        state: vector::Monitoring::Off,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_monitor_on", 0),
                svg: render_svg(
                    vector::InputMonitorIndicator,
                    vector::MonitoringProps {
                        art: slice::expect_art("mcp_monitor_on"),
                        axis: Default::default(),
                        state: vector::Monitoring::On,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
            Entry {
                source: ("mcp_monitor_auto", 0),
                svg: render_svg(
                    vector::InputMonitorIndicator,
                    vector::MonitoringProps {
                        art: slice::expect_art("mcp_monitor_auto"),
                        axis: Default::default(),
                        state: vector::Monitoring::Auto,
                        width: n.0,
                        height: n.1,
                        at: vector::Interaction::Normal,
                    },
                ),
            },
        ],
    ));

    groups.push((
        "pan + fader",
        vec![
            Entry {
                source: ("mcp_pan_knob_small", 0),
                svg: render_svg(
                    vector::PanningKnob,
                    vector::PanProps {
                        indicator: false,
                        position: 0.0,
                        large: false,
                        width: n.0,
                        height: n.1,
                    },
                ),
            },
            Entry {
                source: ("mcp_volthumb", 0),
                svg: render_svg(
                    vector::VolumeFaderCap,
                    vector::FaderCapProps {
                        accent: None,
                        pane: None,
                        width: n.0,
                        height: n.1,
                    },
                ),
            },
            Entry {
                source: ("mcp_volbg", 0),
                svg: render_svg(
                    vector::VolumeFaderTrack,
                    vector::FaderCapProps {
                        accent: None,
                        pane: None,
                        width: n.0,
                        height: n.1,
                    },
                ),
            },
        ],
    ));

    // Column pitch has to follow the *drawn* width, not `DISPLAY`.
    //
    // A cell is zoomed by a whole factor of its height, so a wide control
    // ends up wider than `DISPLAY`: record-arm is 36x24, zoomed 3x, so
    // 108px in a 92px column — its cells overlapped each other, and the
    // overlap looked like the artwork being wrong rather than the sheet.
    let cells: Vec<Vec<Option<image::RgbaImage>>> = groups
        .iter()
        .map(|(_, entries)| {
            entries
                .iter()
                .map(|e| original_cell(e.source.0, e.source.1))
                .collect()
        })
        .collect();
    let drawn = |img: &Option<image::RgbaImage>| {
        img.as_ref()
            .map(|o| o.width() * (DISPLAY / o.height().max(1)).max(1))
            .unwrap_or(DISPLAY)
    };
    let pitch = cells
        .iter()
        .flatten()
        .map(|c| drawn(c) + PAD)
        .max()
        .unwrap_or(DISPLAY + PAD);

    let cols = groups.iter().map(|(_, e)| e.len()).max().unwrap_or(1) as u32;
    let row_h = DISPLAY + PAD;
    let group_h = row_h * 3 + PAD * 2;
    let sheet_w = PAD + cols * pitch;
    let sheet_h = PAD + groups.len() as u32 * group_h;
    let mut sheet = image::RgbaImage::from_pixel(sheet_w, sheet_h, image::Rgba(BG));

    let mut y = PAD;
    let mut report: Vec<String> = Vec::new();

    for (g, (title, entries)) in groups.iter().enumerate() {
        for (col, e) in entries.iter().enumerate() {
            let x = PAD + col as u32 * pitch;

            // 1 — the original, at native size, nearest-zoomed.
            let native = cells[g][col].clone();
            if let Some(orig) = &native {
                let z = zoom(orig, DISPLAY);
                image::imageops::overlay(&mut sheet, &z, x as i64, y as i64);
            } else {
                report.push(format!("{title}: no source for {}", e.source.0));
            }

            // 2 — the component as vector, drawn to occupy exactly the
            // footprint the original does in row 1.
            //
            // Rows 1 and 3 are integer zooms of the native cell; scaling
            // this one to a fixed display height instead made it visibly
            // larger than the thing it is supposed to be compared against,
            // which reads as the component being oversized when it is the
            // sheet that is inconsistent. The three rows must share one
            // scale or the comparison means nothing.
            if let Some(orig) = &native {
                let f = (DISPLAY / orig.height().max(1)).max(1);
                if let Some(v) = raster(&e.svg, orig.width() * f, orig.height() * f) {
                    image::imageops::overlay(&mut sheet, &v, x as i64, (y + row_h) as i64);
                }
            }

            // 3 — the component rasterised to REAPER's native size, then
            // zoomed the same way as the original. This is the real round
            // trip: what REAPER would actually blit.
            if let Some(orig) = &native
                && let Some(back) = raster(&e.svg, orig.width(), orig.height())
            {
                let z = zoom(&back, DISPLAY);
                image::imageops::overlay(&mut sheet, &z, x as i64, (y + row_h * 2) as i64);
            }
        }
        y += group_h;
    }

    let dir = std::path::Path::new("target/compare");
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("roundtrip.png");
    sheet.save(&path).unwrap();

    println!("rows per group: original PNG / vector / vector→REAPER PNG");
    for (title, e) in &groups {
        println!("  {title:<14} {} states", e.len());
    }
    for r in &report {
        println!("  ! {r}");
    }
    println!("\n{}", path.display());
}
