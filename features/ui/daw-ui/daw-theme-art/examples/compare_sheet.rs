//! The full traced-vs-vector comparison: every state, every interaction.
//!
//!     cargo run -p daw-theme-art --example compare_sheet
//!
//! One block per control. Within a block, a column per state (off/on/…) and
//! a row per interaction (normal/hover/pressed), with the **traced** cell
//! above the **vector** one.
//!
//! Rendered large on purpose: at native size the two are hard to tell
//! apart, and the differences that matter only show when you zoom — which
//! is the entire reason the vector versions exist.

use daw_theme_art::render::render_svg;
use daw_theme_art::{mixer_controls as traced, slice, vector_controls as vector};

const CELL: u32 = 76;
const PAD: u32 = 10;
const LABEL_H: u32 = 22;
const BG: [u8; 4] = [16, 16, 21, 255];

fn raster(svg: &str, px: u32) -> Option<image::RgbaImage> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    let tree = resvg::usvg::Tree::from_str(svg, &opts).ok()?;
    let (vw, vh) = (tree.size().width(), tree.size().height());
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let scale = px as f32 / vh;
    let (w, h) = (((vw * scale) as u32).max(1), px.max(1));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(daw_theme_art::render::to_rgba(&pixmap))
}

/// One control: its state columns, each rendered both ways at each
/// interaction. `[state][interaction] -> (traced, vector)`.
struct Block {
    title: &'static str,
    states: Vec<&'static str>,
    /// `[interaction][state]` of `(traced, vector)`.
    grid: Vec<Vec<(String, String)>>,
}

const INTERACTIONS: [(traced::Interaction, &str); 3] = [
    (traced::Interaction::Normal, "normal"),
    (traced::Interaction::Hover, "hover"),
    (traced::Interaction::Pressed, "pressed"),
];

fn vector_at(i: usize) -> vector::Interaction {
    match i {
        1 => vector::Interaction::Hover,
        2 => vector::Interaction::Pressed,
        _ => vector::Interaction::Normal,
    }
}

fn main() {
    let mut blocks: Vec<Block> = Vec::new();

    // ── record arm ───────────────────────────────────────────────────
    {
        let states = vec!["off", "on", "norec", "auto", "auto-on", "auto-norec"];
        let ts = [
            traced::RecordArm::Off,
            traced::RecordArm::On,
            traced::RecordArm::NoRecord,
            traced::RecordArm::Auto,
            traced::RecordArm::AutoOn,
            traced::RecordArm::AutoNoRecord,
        ];
        let vs = [
            vector::RecordArm::Off,
            vector::RecordArm::On,
            vector::RecordArm::NoRecord,
            vector::RecordArm::Auto,
            vector::RecordArm::AutoOn,
            vector::RecordArm::AutoNoRecord,
        ];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                ts.iter()
                    .zip(vs.iter())
                    .map(|(t, v)| {
                        (
                            render_svg(
                                traced::RecordArmButton,
                                traced::RecordArmProps {
                                    state: *t,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::RecordArmButton,
                                vector::RecordArmProps {
                                    art: slice::expect_art("mcp_recarm_on"),
                                    housing: true,
                                    state: *v,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "record arm",
            states,
            grid,
        });
    }

    // ── mute ─────────────────────────────────────────────────────────
    {
        let states = vec!["off", "on"];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                [false, true]
                    .iter()
                    .map(|on| {
                        (
                            render_svg(
                                traced::MuteButton,
                                traced::ToggleProps {
                                    on: *on,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::MuteButton,
                                vector::ToggleProps {
                                    unlit: None,
                                    hover: 0.35,
                                    sinks: true,
                                    depth: 0.15,
                                    legend: None,
                                    body: (0.0, 1.0),
                                    art: slice::expect_art("mcp_mute_on"),
                                    on: *on,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "mute",
            states,
            grid,
        });
    }

    // ── solo ─────────────────────────────────────────────────────────
    {
        let states = vec!["off", "on", "defeat"];
        let ts = [traced::Solo::Off, traced::Solo::On, traced::Solo::Defeat];
        let vs = [vector::Solo::Off, vector::Solo::On, vector::Solo::Defeat];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                ts.iter()
                    .zip(vs.iter())
                    .map(|(t, v)| {
                        (
                            render_svg(
                                traced::SoloButton,
                                traced::SoloProps {
                                    state: *t,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::SoloButton,
                                vector::SoloProps {
                                    unlit: None,
                                    hover: 0.35,
                                    sinks: true,
                                    depth: 0.11,
                                    legend: None,
                                    body: (0.0, 1.0),
                                    art: slice::expect_art("mcp_solo_on"),
                                    state: *v,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "solo",
            states,
            grid,
        });
    }

    // ── fx ───────────────────────────────────────────────────────────
    {
        let states = vec!["empty", "active", "bypassed"];
        let ts = [
            traced::FxChain::Empty,
            traced::FxChain::Active,
            traced::FxChain::Bypassed,
        ];
        let vs = [
            vector::FxChain::Empty,
            vector::FxChain::Active,
            vector::FxChain::Bypassed,
        ];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                ts.iter()
                    .zip(vs.iter())
                    .map(|(t, v)| {
                        (
                            render_svg(
                                traced::FxButton,
                                traced::FxProps {
                                    state: *t,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::FxButton,
                                vector::FxProps {
                                    family: Default::default(),
                                    state: *v,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "fx",
            states,
            grid,
        });
    }

    // ── routing ──────────────────────────────────────────────────────
    {
        let states = vec!["none", "sends", "recv", "both", "disabled"];
        let combos = [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (true, true, true),
        ];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                combos
                    .iter()
                    .map(|(s, r, d)| {
                        (
                            render_svg(
                                traced::RoutingButton,
                                traced::RoutingProps {
                                    has_sends: *s,
                                    has_receives: *r,
                                    disabled: *d,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::RoutingButton,
                                vector::RoutingProps {
                                    art: slice::expect_art("mcp_io_s_r"),
                                    axis: Default::default(),
                                    has_sends: *s,
                                    has_receives: *r,
                                    disabled: *d,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "routing",
            states,
            grid,
        });
    }

    // ── input monitoring ─────────────────────────────────────────────
    {
        let states = vec!["off", "on", "auto"];
        let ts = [
            traced::Monitoring::Off,
            traced::Monitoring::On,
            traced::Monitoring::Auto,
        ];
        let vs = [
            vector::Monitoring::Off,
            vector::Monitoring::On,
            vector::Monitoring::Auto,
        ];
        let grid = INTERACTIONS
            .iter()
            .enumerate()
            .map(|(i, (at, _))| {
                ts.iter()
                    .zip(vs.iter())
                    .map(|(t, v)| {
                        (
                            render_svg(
                                traced::InputMonitorIndicator,
                                traced::MonitoringProps {
                                    state: *t,
                                    width: None,
                                    height: None,
                                    at: *at,
                                },
                            ),
                            render_svg(
                                vector::InputMonitorIndicator,
                                vector::MonitoringProps {
                                    art: slice::expect_art("mcp_monitor_on"),
                                    axis: Default::default(),
                                    state: *v,
                                    width: None,
                                    height: None,
                                    at: vector_at(i),
                                },
                            ),
                        )
                    })
                    .collect()
            })
            .collect();
        blocks.push(Block {
            title: "input monitor",
            states,
            grid,
        });
    }

    // ── pan + fader (no interaction states; one row) ──────────────────
    {
        let states = vec!["L", "centre", "R", "large"];
        let pans = [(-1.0f32, false), (0.0, false), (1.0, false), (0.0, true)];
        let grid = vec![
            pans.iter()
                .map(|(p, large)| {
                    (
                        render_svg(
                            traced::PanningKnob,
                            traced::PanProps {
                                position: *p,
                                large: *large,
                                width: None,
                                height: None,
                                at: traced::Interaction::Normal,
                            },
                        ),
                        render_svg(
                            vector::PanningKnob,
                            vector::PanProps {
                                indicator: false,
                                position: *p,
                                large: *large,
                                width: None,
                                height: None,
                            },
                        ),
                    )
                })
                .collect(),
        ];
        blocks.push(Block {
            title: "pan",
            states,
            grid,
        });
    }
    {
        let states = vec!["cap", "track"];
        let grid = vec![vec![
            (
                render_svg(
                    traced::VolumeFaderCap,
                    traced::FaderCapProps {
                        width: None,
                        height: None,
                        at: traced::Interaction::Normal,
                    },
                ),
                render_svg(
                    vector::VolumeFaderCap,
                    vector::FaderCapProps {
                        accent: None,
                        pane: None,
                        width: None,
                        height: None,
                    },
                ),
            ),
            (
                render_svg(
                    traced::VolumeFaderTrack,
                    traced::FaderCapProps {
                        width: None,
                        height: None,
                        at: traced::Interaction::Normal,
                    },
                ),
                render_svg(
                    vector::VolumeFaderTrack,
                    vector::FaderCapProps {
                        accent: None,
                        pane: None,
                        width: None,
                        height: None,
                    },
                ),
            ),
        ]];
        blocks.push(Block {
            title: "fader",
            states,
            grid,
        });
    }

    // ── compose ──────────────────────────────────────────────────────
    // Height: each block is (rows x 2) cells tall plus a title strip.
    let block_h = |b: &Block| LABEL_H + b.grid.len() as u32 * (CELL * 2 + PAD) + PAD;
    let sheet_h: u32 = blocks.iter().map(block_h).sum::<u32>() + PAD;
    let widest = blocks.iter().map(|b| b.states.len()).max().unwrap_or(1) as u32;
    let sheet_w = PAD + widest * (CELL + PAD);

    let mut sheet = image::RgbaImage::from_pixel(sheet_w, sheet_h, image::Rgba(BG));
    let mut y = PAD;

    for b in &blocks {
        // A thin rule marks each block, since there is no text rendering
        // here — the layout has to carry the grouping on its own.
        for x in 0..sheet_w {
            sheet.put_pixel(x, y.min(sheet_h - 1), image::Rgba([60, 60, 75, 255]));
        }
        y += LABEL_H;

        for row in &b.grid {
            for (col, (t, v)) in row.iter().enumerate() {
                let x = PAD + col as u32 * (CELL + PAD);
                for (half, svg) in [(0u32, t), (1, v)] {
                    let Some(img) = raster(svg, CELL) else {
                        continue;
                    };
                    let ox = x + CELL.saturating_sub(img.width()) / 2;
                    let oy = y + half * CELL;
                    if oy + img.height() <= sheet_h {
                        image::imageops::overlay(&mut sheet, &img, ox as i64, oy as i64);
                    }
                }
            }
            y += CELL * 2 + PAD;
        }
    }

    let dir = std::path::Path::new("target/compare");
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("traced-vs-vector.png");
    sheet.save(&path).unwrap();

    println!("{} controls -> {}", blocks.len(), path.display());
    for b in &blocks {
        println!(
            "  {:<14} {} states x {} interactions",
            b.title,
            b.states.len(),
            b.grid.len()
        );
    }
    println!("\nwithin each pair: traced above, vector below");
}
