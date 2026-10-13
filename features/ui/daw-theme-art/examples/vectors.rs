//! Rasterise the vector controls, small and very large.
//!
//!     cargo run -p daw-theme-art --example vectors
//!
//! The large pass is the point: traced art blown up shows its pixel steps,
//! these do not.

use daw_theme_art::render::render_svg;
use daw_theme_art::vector_controls::*;

fn save(name: &str, svg: &str, px: u32) {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    let Ok(tree) = resvg::usvg::Tree::from_str(svg, &opts) else {
        eprintln!("{name}: invalid SVG");
        return;
    };
    let (vw, vh) = (tree.size().width(), tree.size().height());
    let scale = px as f32 / vh;
    let (w, h) = ((vw * scale) as u32, px);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let dir = std::path::Path::new("target/vectors");
    std::fs::create_dir_all(dir).unwrap();
    daw_theme_art::render::to_rgba(&pixmap)
        .save(dir.join(format!("{name}.png")))
        .unwrap();
}

fn main() {
    for (tag, px) in [("sm", 40u32), ("xl", 320u32)] {
        save(
            &format!("mute-off-{tag}"),
            &render_svg(
                MuteButton,
                ToggleProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.15,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_mute_on"),
                    on: false,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("mute-on-{tag}"),
            &render_svg(
                MuteButton,
                ToggleProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.15,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_mute_on"),
                    on: true,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("solo-on-{tag}"),
            &render_svg(
                SoloButton,
                SoloProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.11,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_solo_on"),
                    state: Solo::On,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("solo-defeat-{tag}"),
            &render_svg(
                SoloButton,
                SoloProps {
                    unlit: None,
                    hover: 0.35,
                    sinks: true,
                    depth: 0.11,
                    legend: None,
                    body: (0.0, 1.0),
                    art: daw_theme_art::slice::expect_art("mcp_solo_on"),
                    state: Solo::Defeat,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("fx-{tag}"),
            &render_svg(
                FxButton,
                FxProps {
                    family: Default::default(),
                    state: FxChain::Active,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("rec-on-{tag}"),
            &render_svg(
                RecordArmButton,
                RecordArmProps {
                    art: daw_theme_art::slice::expect_art("mcp_recarm_on"),
                    housing: true,
                    state: RecordArm::On,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("rec-norec-{tag}"),
            &render_svg(
                RecordArmButton,
                RecordArmProps {
                    art: daw_theme_art::slice::expect_art("mcp_recarm_on"),
                    housing: true,
                    state: RecordArm::NoRecord,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("rec-auto-{tag}"),
            &render_svg(
                RecordArmButton,
                RecordArmProps {
                    art: daw_theme_art::slice::expect_art("mcp_recarm_on"),
                    housing: true,
                    state: RecordArm::Auto,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("routing-{tag}"),
            &render_svg(
                RoutingButton,
                RoutingProps {
                    art: daw_theme_art::slice::expect_art("mcp_io_s_r"),
                    axis: Default::default(),
                    has_sends: true,
                    has_receives: true,
                    disabled: false,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("monitor-{tag}"),
            &render_svg(
                InputMonitorIndicator,
                MonitoringProps {
                    art: daw_theme_art::slice::expect_art("mcp_monitor_on"),
                    axis: Default::default(),
                    state: Monitoring::On,
                    width: None,
                    height: None,
                    at: Default::default(),
                },
            ),
            px,
        );
        save(
            &format!("pan-l-{tag}"),
            &render_svg(
                PanningKnob,
                PanProps {
                    indicator: false,
                    position: -0.7,
                    large: false,
                    width: None,
                    height: None,
                },
            ),
            px,
        );
        save(
            &format!("pan-c-{tag}"),
            &render_svg(
                PanningKnob,
                PanProps {
                    indicator: false,
                    position: 0.0,
                    large: false,
                    width: None,
                    height: None,
                },
            ),
            px,
        );
        save(
            &format!("cap-{tag}"),
            &render_svg(
                VolumeFaderCap,
                FaderCapProps {
                    accent: None,
                    pane: None,
                    width: None,
                    height: None,
                },
            ),
            px,
        );
    }
    println!("wrote target/vectors");
}
