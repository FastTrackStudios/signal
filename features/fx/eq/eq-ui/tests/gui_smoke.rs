//! Headless GUI smoke tests for the eq-ui portable core.
//!
//! Mounts a real Dioxus SVG graph — the same generators + rsx shape the
//! detached remotes use (`features/rigs/guitar/ui/src/eq_surface.rs`) — in a
//! plain `VirtualDom` and asserts on the SSR'd markup. No renderer, no GPU,
//! no `native` feature: this runs under bare `cargo test -p eq-ui`.
//!
//! (The experimental DioxusLabs/dioxus-test harness would add real event
//! dispatch on a headless Blitz DOM, but it currently requires dioxus
//! 0.8.0-alpha; until the workspace leaves =0.7.9, interaction logic is
//! exercised directly through `eq_graph_interaction`.)

use dioxus::prelude::*;

use eq_ui::eq_graph_interaction::{GraphMapper, filter_type_for_position, nearest_band};
use eq_ui::eq_graph_model::{EqBand, EqBandShape, get_band_color};
use eq_ui::eq_graph_svg::{generate_all_eq_curves, generate_freq_labels, generate_grid_elements};

const W: f64 = 480.0;
const H: f64 = 270.0;
const MIN_FREQ: f64 = 10.0;
const MAX_FREQ: f64 = 22_050.0;
const DB_RANGE: f64 = 12.0;
const SAMPLE_RATE: f64 = 48_000.0;

fn band(index: usize, freq: f32, gain: f32, shape: EqBandShape) -> EqBand {
    EqBand {
        index,
        used: true,
        enabled: true,
        frequency: freq,
        gain,
        q: 0.707,
        shape,
        ..Default::default()
    }
}

/// The graph surface under test: eq-ui's real SVG generators re-hosted in
/// plain Dioxus SVG, exactly like the detached `EqProSurface` does.
#[component]
fn EqGraphSvg(bands: Vec<EqBand>) -> Element {
    let curves =
        generate_all_eq_curves(&bands, SAMPLE_RATE, MIN_FREQ, MAX_FREQ, DB_RANGE, 0.0, W, H, 128);
    let grid = generate_grid_elements(0.0, W, H, MIN_FREQ, MAX_FREQ, DB_RANGE);
    let labels = generate_freq_labels(0.0, W, H, MIN_FREQ, MAX_FREQ);
    let mapper = GraphMapper::new(MIN_FREQ, MAX_FREQ, DB_RANGE, W, H, 0.0);

    rsx! {
        svg {
            class: "eq-graph",
            view_box: "0 0 {W} {H}",
            for (x1, y1, x2, y2, major) in grid {
                line {
                    class: if major { "grid-major" } else { "grid-minor" },
                    x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}",
                }
            }
            for (x, y, text) in labels {
                text { class: "freq-label", x: "{x}", y: "{y}", "{text}" }
            }
            path { class: "eq-combined-fill", d: "{curves.combined_fill}" }
            path { class: "eq-combined", d: "{curves.combined_stroke}" }
            for (idx, stroke, _fill) in curves.band_curves {
                path { class: "eq-band eq-band-{idx}", stroke: "{get_band_color(idx)}", d: "{stroke}" }
            }
            for b in bands.iter().filter(|b| b.used) {
                circle {
                    class: "eq-node eq-node-{b.index}",
                    cx: "{mapper.freq_to_x(b.frequency as f64)}",
                    cy: "{mapper.db_to_y(b.gain as f64)}",
                    r: "6",
                }
            }
        }
    }
}

/// Build + SSR the component with the given bands.
fn render_graph(bands: Vec<EqBand>) -> String {
    let mut vdom = VirtualDom::new_with_props(EqGraphSvg, EqGraphSvgProps { bands });
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

#[test]
fn graph_renders_expected_structure() {
    let bands = vec![
        band(0, 100.0, 6.0, EqBandShape::Bell),
        band(1, 3_000.0, -4.0, EqBandShape::HighShelf),
    ];
    let html = render_graph(bands);

    // The SVG shell and grid are present.
    assert!(html.contains("<svg"), "no <svg> root:\n{html}");
    assert!(html.contains("grid-major"), "no major grid lines:\n{html}");
    // Frequency axis labels from generate_freq_labels.
    for label in [">20<", ">1k<", ">20k<"] {
        assert!(html.contains(label), "missing freq label {label}:\n{html}");
    }
    // The combined response curve is a real path (starts with a MoveTo).
    assert!(
        html.contains(r#"class="eq-combined" d="M"#),
        "combined curve path missing or not starting with M:\n{html}"
    );
    // One per-band curve + one node per used band.
    for idx in 0..2 {
        assert!(html.contains(&format!("eq-band-{idx}")), "band {idx} curve missing:\n{html}");
        assert!(html.contains(&format!("eq-node-{idx}")), "band {idx} node missing:\n{html}");
    }
    assert!(!html.contains("eq-band-2"), "phantom band curve rendered:\n{html}");
}

#[test]
fn disabled_band_drops_its_curve_but_boosts_still_bend_the_response() {
    let flat = render_graph(vec![]);
    let boosted = render_graph(vec![band(0, 1_000.0, 12.0, EqBandShape::Bell)]);
    let mut off = band(0, 1_000.0, 12.0, EqBandShape::Bell);
    off.enabled = false;
    let bypassed = render_graph(vec![off]);

    // A +12 dB bell must change the combined curve vs. flat.
    assert_ne!(flat, boosted, "boost did not change the rendered response");
    // Bypassed band: node still shown, per-band curve gone, response flat again.
    assert!(boosted.contains("eq-band-0"));
    assert!(!bypassed.contains("eq-band-0"), "bypassed band still draws a curve");
    assert!(bypassed.contains("eq-node-0"), "bypassed band lost its node");
    let combined_of = |html: &str| {
        html.split(r#"class="eq-combined" d=""#).nth(1).unwrap().split('"').next().unwrap().to_string()
    };
    assert_eq!(combined_of(&flat), combined_of(&bypassed), "bypassed band bent the response");
}

/// The interaction layer (what pointer events feed in the real editor):
/// hit-test a node through the same GraphMapper the component uses, and
/// infer a filter shape from a double-click position.
#[test]
fn graph_interaction_hit_test_and_shape_inference() {
    let mapper = GraphMapper::new(MIN_FREQ, MAX_FREQ, DB_RANGE, W, H, 0.0);
    let bands = vec![band(0, 100.0, 6.0, EqBandShape::Bell), band(1, 5_000.0, 0.0, EqBandShape::Bell)];

    // Click exactly on band 1's node → nearest_band finds it (distance 0).
    let (x, y) = (mapper.freq_to_x(5_000.0), mapper.db_to_y(0.0));
    assert_eq!(nearest_band(&bands, mapper, x, y, 16.0).map(|(idx, _)| idx), Some(1));
    // Click far from any node → no hit.
    assert!(
        nearest_band(&bands, mapper, mapper.freq_to_x(500.0), mapper.db_to_y(-10.0), 16.0).is_none()
    );

    // Double-click near the low edge at 0 dB → a low cut, mid at a boost → a bell.
    assert_eq!(filter_type_for_position(20.0, 0.0, DB_RANGE), EqBandShape::LowCut);
    assert_eq!(filter_type_for_position(1_000.0, 6.0, DB_RANGE), EqBandShape::Bell);
}
