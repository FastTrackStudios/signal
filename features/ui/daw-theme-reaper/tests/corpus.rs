//! Cross-theme WALTER corpus tests: evaluate every locally available theme's
//! layouts and sanity-check the resolved MCP geometry. Themes that aren't
//! extracted are skipped, so CI without the corpus stays green.

use daw_theme_reaper::ReaperTheme;
use daw_theme_reaper::walter::{Env, evaluate};

const CORPUS: &[(&str, &str)] = &[
    ("anti", "extracted/antitheme"),
    ("reapertips", "extracted/reapertips/_reapertips_theme"),
    ("neptune", "extracted/neptune"),
    ("imperial", "extracted/imperial"),
];

/// Where the third-party themes live.
///
/// `REAPER_THEME_CORPUS` wins; otherwise the checkout beside this one, then
/// the old hardcoded path. The hardcoded default was one absolute home
/// directory, so the corpus silently vanished for everyone else — and the
/// test then failed rather than skipping, which is the opposite of what its
/// own doc comment promises.
fn corpus_root() -> String {
    if let Ok(from_env) = std::env::var("REAPER_THEME_CORPUS") {
        return from_env;
    }
    let beside = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../reaper-theme");
    if std::path::Path::new(beside).is_dir() {
        return beside.to_string();
    }
    "/home/cody/Development/FastTrackStudio/reaper-theme".to_string()
}

fn env_for(rt: &ReaperTheme, w: f32, h: f32) -> Env {
    let mut env = Env::reaper_defaults(w, h);
    env.set("trackcolor_valid", 1.0);
    env.set("trackcolor_r", 200.0);
    env.set("trackcolor_g", 80.0);
    env.set("trackcolor_b", 40.0);
    for p in &rt.rtconfig.params {
        env.set(&p.name, p.default);
    }
    env
}

#[test]
fn corpus_layouts_resolve_sane_mcp_geometry() {
    let root = corpus_root();
    let mut themes_checked = 0;
    for (name, rel) in CORPUS {
        let dir = format!("{root}/{rel}");
        let Ok(rt) = ReaperTheme::load_dir(&dir) else {
            eprintln!("{name}: not extracted — skipping");
            continue;
        };
        themes_checked += 1;

        let probe = evaluate(&rt.rtconfig_src, None, &env_for(&rt, 100.0, 600.0));
        let layouts: Vec<String> = probe
            .layouts
            .iter()
            .filter(|n| !n.contains('%'))
            .cloned()
            .collect();
        assert!(!layouts.is_empty(), "{name}: no layouts declared");

        let mut layouts_with_mcp = 0;
        for layout in &layouts {
            let pass1 = evaluate(&rt.rtconfig_src, Some(layout), &env_for(&rt, 100.0, 600.0));
            // Natural width from mcp.size (cross-theme) or mcpWidth (anti).
            let w0 = pass1
                .coord("mcp.size")
                .map(|s| s[0])
                .filter(|w| *w >= 24.0)
                .or_else(|| {
                    pass1
                        .get("mcpWidth")
                        .and_then(|v| v.first().copied())
                        .filter(|w| *w >= 24.0)
                });
            let Some(w0) = w0 else { continue };
            let out = evaluate(&rt.rtconfig_src, Some(layout), &env_for(&rt, w0, 600.0));

            // Every assigned mcp.* coordinate must be finite and inside a
            // generous bounding box (4x the strip in each axis — themes park
            // helper boxes off-panel, but NaN/runaway values are interpreter
            // bugs).
            for (attr, v) in out.attrs.iter().filter(|(k, _)| k.starts_with("mcp.")) {
                for x in v.iter() {
                    assert!(
                        x.is_finite(),
                        "{name}/{layout}: {attr} has non-finite {v:?}"
                    );
                    assert!(
                        x.abs() < 8.0 * (w0 + 600.0),
                        "{name}/{layout}: {attr} runaway value {v:?}"
                    );
                }
            }

            // A usable strip needs at least a volume fader or a meter.
            let usable = |n: &str| {
                out.coord(n)
                    .map(|c| c[2] > 0.0 && c[3] > 0.0)
                    .unwrap_or(false)
            };
            if usable("mcp.volume") || usable("mcp.meter") {
                layouts_with_mcp += 1;
            }
        }
        assert!(
            layouts_with_mcp > 0,
            "{name}: no layout resolved a usable MCP strip"
        );
        eprintln!(
            "{name}: {layouts_with_mcp}/{} layouts usable",
            layouts.len()
        );
    }
    // Skipped, not failed, when the corpus is not on this machine — the
    // themes are third-party and not vendored, and a developer without them
    // should still get a green suite. A theme that IS present and resolves
    // nothing still fails, above.
    if themes_checked == 0 {
        eprintln!(
            "no corpus themes under {} — set REAPER_THEME_CORPUS to run this",
            corpus_root()
        );
    }
}
