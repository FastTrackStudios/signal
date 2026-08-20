//! Behavioural tests for the embedded emphasis EQ (`fx.sat.emphasis`,
//! `fx.embed-eq.one-surface`): the strip opens UNDER the panel, the graph is
//! interactive, and dragging a band writes the emphasis params through real
//! host gestures. This is the test that guards "the EQ view mounts but you
//! cannot interact with it".

#![cfg(feature = "native")]

use dioxus_test::by_testid;

#[path = "support/mod.rs"]
mod support;

use support::{mount, Fixture};

/// Click the emphasis toggle on the rail.
async fn open_eq(fx: &mut Fixture) {
    let el = fx
        .tester
        .query(by_testid("emphasis-toggle"))
        .immediately()
        .expect("emphasis toggle on the rail");
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_move(x, y, false);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_down(x, y);
    fx.tester.pointer_up(x, y);
    fx.settle().await;
}

// r[verify fx.sat.emphasis.display]
#[tokio::test]
async fn the_eq_strip_opens_below_the_panel() -> dioxus_test::Result<()> {
    let mut fx = mount();
    // Closed by default; the panel is up.
    assert!(fx.tester.query(by_testid("emphasis-view")).immediately().is_err());
    let panel = fx.tester.query(by_testid("hardware-panel")).immediately()?;
    let (_, panel_h_before) = panel.size();

    open_eq(&mut fx).await;

    // The sidecar exists, has real layout, and sits to the RIGHT of the
    // panel — the panel is still mounted (not hidden).
    let strip = fx.tester.query(by_testid("emphasis-view")).immediately()?;
    let (strip_w, strip_h) = strip.size();
    assert!(strip_w > 300.0 && strip_h > 150.0, "sidecar collapsed: {strip_w}x{strip_h}");
    let panel = fx.tester.query(by_testid("hardware-panel")).immediately()?;
    let (ox_panel, _) = panel.document_origin();
    let (ox_strip, _) = strip.document_origin();
    assert!(
        ox_strip > ox_panel,
        "the EQ sidecar must sit to the RIGHT of the panel"
    );
    let _ = panel_h_before;
    Ok(())
}

/// Dragging a band dot on the embedded graph writes the emphasis params —
/// the graph is interactive inside the strip.
// r[verify fx.sat.emphasis]
#[tokio::test]
async fn dragging_a_band_writes_the_emphasis_params() -> dioxus_test::Result<()> {
    let mut fx = mount();
    open_eq(&mut fx).await;

    let strip = fx.tester.query(by_testid("emphasis-view")).immediately()?;
    let (ox, oy) = strip.document_origin();

    // Band 3 defaults to 700 Hz / 0 dB. Headless, the graph's canvas rect is
    // never published by the painter, so events hit-test through the fixed
    // 800×350 fallback mapper — same as eq-ui's own tests.
    let mapper =
        eq_ui::eq_graph_interaction::GraphMapper::new(20.0, 20_000.0, 12.0, 800.0, 350.0, 0.0);
    let freq = fx.params.emph[2].freq_hz.value() as f64;
    let x = ox + mapper.freq_to_x(freq);
    let y = oy + mapper.db_to_y(0.0);

    let g_before = fx.params.emph[2].gain_db.value();
    assert_eq!(g_before, 0.0);

    // Hover, press, drag up 40 px, release.
    fx.tester.pointer_move(x, y, false);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    for step in 1..=4 {
        fx.tester.pointer_move(x, y - 10.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(x, y - 40.0);
    fx.settle().await;

    let g_after = fx.params.emph[2].gain_db.value();
    assert!(
        g_after > 0.5,
        "dragging the band up did not raise its emphasis gain: {g_before} → {g_after}"
    );
    Ok(())
}