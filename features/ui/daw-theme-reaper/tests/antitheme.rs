//! Integration test against the real White Tie Anti-Theme (the REAPER 7
//! default theme rebuilt human-readable) — our first import target.
//!
//! Looks for the unpacked theme via `$REAPER_ANTITHEME_DIR` or the
//! reaper-theme repo's extraction path; **skips** (passes) when absent so CI
//! without the theme corpus stays green.

use daw_theme_reaper::ReaperTheme;
use daw_theme_reaper::walter::{Env, evaluate};

fn antitheme() -> Option<ReaperTheme> {
    let candidates = [
        std::env::var("REAPER_ANTITHEME_DIR").ok(),
        Some("/home/cody/Development/FastTrackStudio/reaper-theme/extracted/antitheme".to_string()),
    ];
    for dir in candidates.into_iter().flatten() {
        if std::path::Path::new(&dir).is_dir()
            && let Ok(theme) = ReaperTheme::load_dir(&dir)
        {
            return Some(theme);
        }
    }
    eprintln!("anti-theme not found — skipping");
    None
}

#[test]
fn loads_palette_params_and_images() {
    let Some(theme) = antitheme() else { return };

    // Palette: a few known [color theme] keys decode.
    assert!(
        theme.palette.len() > 100,
        "palette has {}",
        theme.palette.len()
    );
    let arrange = theme.palette.color("col_arrangebg").expect("col_arrangebg");
    assert_eq!((arrange.r, arrange.g, arrange.b), (0x45, 0x45, 0x45));
    assert!(theme.palette.color("col_seltrack").is_some());

    // rtconfig: version global + the documented adjuster knobs.
    assert!(theme.rtconfig.global_f32("version").is_some());
    let names: Vec<&str> = theme
        .rtconfig
        .params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert!(names.contains(&"textBrightness"), "params: {names:?}");
    assert!(
        names.contains(&"customColorDepthParam"),
        "params: {names:?}"
    );

    // Images: the track/gen button vocabulary is present.
    for name in [
        "track_mute_off",
        "track_mute_on",
        "track_solo_off",
        "gen_mute_off",
    ] {
        assert!(theme.images.has(name), "missing image {name}");
    }
}

#[test]
fn slices_anti_theme_buttons_and_faders() {
    let Some(theme) = antitheme() else { return };

    // track_mute_off: plain 60×20 → three 20×20 states.
    let mute = theme
        .images
        .button3("track_mute_off")
        .expect("track_mute_off");
    assert_eq!(mute.normal.dimensions(), (20, 20));
    assert_eq!(mute.pressed.dimensions(), (20, 20));

    // mcp_io: pink-lined 62×34 (3N+2 rule) — left line + lone lower-right
    // corner → content 60 wide → three 20px states.
    let io = theme.images.button3("mcp_io").expect("mcp_io");
    assert_eq!(io.normal.dimensions(), (20, 33));

    // mcp_volbg: full marker ring, 26×22 → 24×20 content with 9-slice margins.
    let volbg = theme.images.load("mcp_volbg").expect("mcp_volbg");
    assert_eq!(volbg.image.dimensions(), (24, 20));
    assert!(volbg.markers.fixed_left > 0 && volbg.markers.fixed_right > 0);

    // mcp_volthumb: right-line markers → vertical fixed caps.
    let thumb = theme.images.load("mcp_volthumb").expect("mcp_volthumb");
    assert_eq!(thumb.image.dimensions(), (23, 53));
    assert!(thumb.markers.fixed_top > 0 && thumb.markers.fixed_bottom > 0);

    // Meter strips (the general meter vocabulary the Anti-Theme uses).
    assert!(theme.images.has("meter_strip_v"));
    assert!(theme.images.has("meter_bg_v"));
}

/// The WALTER evaluator resolves the Anti-Theme's own `Layout "A"` program
/// into the REAPER 7 default MCP geometry (regression-pinned for a fixed
/// environment: 110×600 panel, plain stereo track, default params).
#[test]
fn walter_evaluates_anti_theme_mcp_layout() {
    let Some(theme) = antitheme() else { return };
    let rt = std::fs::read_to_string(theme.images.dir().join("rtconfig.txt")).unwrap();

    let mut env = Env::new();
    for (k, v) in [
        ("w", 110.0),
        ("h", 600.0),
        ("trackpanmode", 3.0),
        ("tracknch", 2.0),
        ("trackcolor_valid", 1.0),
        ("trackcolor_r", 200.0),
        ("trackcolor_g", 80.0),
        ("trackcolor_b", 40.0),
        ("trackidx", 1.0),
        ("ntracks", 9.0),
        ("mixer_visible", 1.0),
        ("tcp_sends_enabled", 1.0),
        ("tcp_fxlist_enabled", 1.0),
        ("reaper_version", 7.0),
        ("os_type", 2.0),
        // The DPI scale variable the theme's macros multiply by (1.0 = 100%;
        // the 150%_/200%_ layout variants expect 1.5/2.0).
        ("Scale", 1.0),
    ] {
        env.set(k, v);
    }
    for p in &theme.rtconfig.params {
        env.set(&p.name, p.default);
    }

    let out = evaluate(&rt, Some("A"), &env);

    // The "Normal" (form 3) strip at default LayoutA-mcpWidth=88:
    // right-hand button column at x=62, fader + meter blocks, name bar.
    let coord = |name: &str| out.coord(name).unwrap_or_else(|| panic!("missing {name}"));
    assert_eq!(&coord("mcp.mute")[..4], &[62., 86., 20., 20.]);
    assert_eq!(&coord("mcp.solo")[..4], &[62., 106., 20., 20.]);
    assert_eq!(&coord("mcp.io")[..4], &[62., 132., 20., 22.]);
    assert_eq!(&coord("mcp.volume")[..4], &[52., 86., 25., 440.]);
    assert_eq!(&coord("mcp.meter")[..4], &[4., 86., 46., 440.]);
    assert_eq!(&coord("mcp.label")[..4], &[0., 556., 88., 24.]);
    assert_eq!(&coord("mcp.pan")[..4], &[34., 54., 20., 20.]);

    // Layout names enumerate (A/B/C + DPI variants).
    assert!(out.layouts.iter().any(|l| l == "A"));
    assert!(out.layouts.iter().any(|l| l == "150%_A"));
}
