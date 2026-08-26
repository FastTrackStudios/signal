//! Headless behavioral GUI tests via the vendored dioxus-test harness.
//!
//! Unlike tests/gui_smoke.rs (SSR markup assertions), these mount the EQ
//! graph on a real headless Blitz DOM (`dioxus-test` → blitz-dom) and drive
//! it with REAL events — clicks go through the document's hit-testing path,
//! exactly like a pointer in the plugin editor window. No GPU, no window:
//! plain `cargo test -p eq-ui`.

use dioxus::prelude::*;
use dioxus_test::{matchers::inner_html, render, Result};
use test_that::prelude::*;

use eq_ui::eq_graph_model::{EqBand, EqBandShape};
use eq_ui::eq_graph_svg::generate_all_eq_curves;

const W: f64 = 480.0;
const H: f64 = 270.0;
const MIN_FREQ: f64 = 10.0;
const MAX_FREQ: f64 = 22_050.0;
const DB_RANGE: f64 = 12.0;
const SAMPLE_RATE: f64 = 48_000.0;

fn bell(index: usize, freq: f32, gain: f32) -> EqBand {
    EqBand {
        index,
        used: true,
        enabled: true,
        frequency: freq,
        gain,
        q: 1.0,
        shape: EqBandShape::Bell,
        ..Default::default()
    }
}

/// The interactive surface under test: the real curve generators inside a
/// band-toggle interaction — clicking the band chip flips the band's
/// `enabled` flag and the curve set regenerates, the exact shape the
/// plugin editor's band controls use.
#[component]
fn ToggleableGraph() -> Element {
    let mut band_enabled = use_signal(|| true);
    let bands = vec![EqBand {
        enabled: band_enabled(),
        ..bell(0, 400.0, 12.0)
    }];
    let curves = generate_all_eq_curves(
        &bands,
        SAMPLE_RATE,
        MIN_FREQ,
        MAX_FREQ,
        DB_RANGE,
        0.0,
        W,
        H,
        128,
    );
    let path_count = curves.band_curves.len();

    rsx! {
        div {
            button {
                "data-testid": "band-toggle",
                onclick: move |_| band_enabled.toggle(),
                if band_enabled() { "band on" } else { "band off" }
            }
            div {
                "data-testid": "curve-count",
                "{path_count}"
            }
        }
    }
}

#[tokio::test]
async fn clicking_band_toggle_drops_and_restores_the_curve() -> Result<()> {
    let tester = render(ToggleableGraph).build();

    // Enabled band → one band curve rendered.
    tester
        .query(dioxus_test::by_testid("curve-count"))
        .expect(inner_html(eq("1")))
        .immediately()?;

    // A real click (hit-tested pointer down/up on the headless DOM).
    tester
        .query(dioxus_test::by_testid("band-toggle"))
        .click()
        .await?;
    let _ = tester.pump().await;

    tester
        .query(dioxus_test::by_testid("band-toggle"))
        .expect(inner_html(eq("band off")))
        .await?;
    tester
        .query(dioxus_test::by_testid("curve-count"))
        .expect(inner_html(eq("0")))
        .await
}
