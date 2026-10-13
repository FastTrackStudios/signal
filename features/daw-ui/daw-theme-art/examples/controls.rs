//! Rasterise every mixer control in every state, for looking at.
//!
//!     cargo run -p daw-theme-art --example controls

use daw_theme_art::mixer_controls::*;
use daw_theme_art::render::render_svg;

fn save(name: &str, svg: &str) {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = std::sync::Arc::new(db);
    let Ok(tree) = resvg::usvg::Tree::from_str(svg, &opts) else {
        eprintln!("{name}: invalid SVG");
        return;
    };
    let scale = 3.0;
    let (w, h) = (
        (tree.size().width() * scale) as u32,
        (tree.size().height() * scale) as u32,
    );
    if w == 0 || h == 0 {
        eprintln!("{name}: empty");
        return;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let dir = std::path::Path::new("target/controls");
    std::fs::create_dir_all(dir).unwrap();
    daw_theme_art::render::to_rgba(&pixmap)
        .save(dir.join(format!("{name}.png")))
        .unwrap();
    println!("wrote {name}");
}

fn main() {
    let sz = (None, None);

    for (n, s) in [
        ("recarm-off", RecordArm::Off),
        ("recarm-on", RecordArm::On),
        ("recarm-norec", RecordArm::NoRecord),
        ("recarm-auto", RecordArm::Auto),
    ] {
        save(
            n,
            &render_svg(
                RecordArmButton,
                RecordArmProps {
                    state: s,
                    width: sz.0,
                    height: sz.1,
                    at: Default::default(),
                },
            ),
        );
    }

    save(
        "mute-off",
        &render_svg(
            MuteButton,
            ToggleProps {
                on: false,
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );
    save(
        "mute-on",
        &render_svg(
            MuteButton,
            ToggleProps {
                on: true,
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );

    for (n, s) in [
        ("solo-off", Solo::Off),
        ("solo-on", Solo::On),
        ("solo-defeat", Solo::Defeat),
    ] {
        save(
            n,
            &render_svg(
                SoloButton,
                SoloProps {
                    state: s,
                    width: sz.0,
                    height: sz.1,
                    at: Default::default(),
                },
            ),
        );
    }

    for (n, s, r) in [
        ("routing-none", false, false),
        ("routing-sends", true, false),
        ("routing-recv", false, true),
        ("routing-both", true, true),
    ] {
        save(
            n,
            &render_svg(
                RoutingButton,
                RoutingProps {
                    has_sends: s,
                    has_receives: r,
                    disabled: false,
                    width: sz.0,
                    height: sz.1,
                    at: Default::default(),
                },
            ),
        );
    }

    for (n, s) in [
        ("monitor-off", Monitoring::Off),
        ("monitor-on", Monitoring::On),
        ("monitor-auto", Monitoring::Auto),
    ] {
        save(
            n,
            &render_svg(
                InputMonitorIndicator,
                MonitoringProps {
                    state: s,
                    width: sz.0,
                    height: sz.1,
                    at: Default::default(),
                },
            ),
        );
    }

    for (n, s) in [
        ("fx-empty", FxChain::Empty),
        ("fx-active", FxChain::Active),
        ("fx-bypassed", FxChain::Bypassed),
    ] {
        save(
            n,
            &render_svg(
                FxButton,
                FxProps {
                    state: s,
                    width: sz.0,
                    height: sz.1,
                    at: Default::default(),
                },
            ),
        );
    }

    save(
        "pan-small",
        &render_svg(
            PanningKnob,
            PanProps {
                position: 0.0,
                large: false,
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );
    save(
        "pan-large",
        &render_svg(
            PanningKnob,
            PanProps {
                position: 0.0,
                large: true,
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );
    save(
        "fader-cap",
        &render_svg(
            VolumeFaderCap,
            FaderCapProps {
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );
    save(
        "fader-track",
        &render_svg(
            VolumeFaderTrack,
            FaderCapProps {
                width: sz.0,
                height: sz.1,
                at: Default::default(),
            },
        ),
    );
}
