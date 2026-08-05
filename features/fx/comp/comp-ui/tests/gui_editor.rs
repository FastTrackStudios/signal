//! Behavioral tests for the REAL FTS Comp plugin editor UI.
//!
//! Mounts `comp_ui::control_view::App` — the exact Dioxus surface the
//! CLAP/VST3 plugin embeds — on the vendored dioxus-test harness (headless
//! Blitz DOM, no GPU, no window) and drives it with real hit-tested pointer
//! events, including a knob drag that must change a parameter through
//! recorded host automation gestures.
//!
//! Requires the `native` feature (declared via `[[test]] required-features`):
//!
//! ```sh
//! cargo test -p comp-ui --features native --test gui_editor
//! ```
//!
//! ## Harness notes
//!
//! - `App` injects its CSS through `document::Style` head elements. The
//!   headless harness has no head-element provider (dioxus falls back to
//!   `NoOpDocument`, which drops them), so the [`support::Harness`] wrapper
//!   re-injects the same stylesheets as ordinary body `<style>` elements —
//!   blitz-dom processes `<style>` anywhere in the tree. Without them every
//!   Tailwind class (`flex-1`, `grid`, …) is undefined and the layout
//!   collapses.
//! - Knob dragging goes through `fts_ui_audio`'s `DragProvider`: mousedown on
//!   the knob's overlay starts the gesture, mousemoves anywhere in the window
//!   move the value (vertical, up = increase), mouseup ends it.

use dioxus_test::{
    by_testid,
    matchers::{contains_substring, inner_html},
};

use comp_ui::comp_graph_svg::db_to_y;

use audiocore_core::prelude::Param;

// ─────────────────────────────────────────────────────────────────────────
// Fixture
// ─────────────────────────────────────────────────────────────────────────

#[path = "support/mod.rs"]
mod support;

use support::{mount, mount_sized, ptr_key, Fixture, Gesture};

/// Click profile `index` in the shell rail — the face switch.
async fn select_profile(fx: &mut Fixture, index: usize) -> dioxus_test::Result<()> {
    let id = comp_ui::faces::PROFILE_IDS[index];
    let item = fx.tester.query(by_testid(&format!("rail-item-{id}"))).immediately()?;
    let (ox, oy) = item.document_origin();
    let (w, h) = item.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;
    Ok(())
}

/// Rendered size of the faceplate itself (the drawing, not the space around
/// it).
fn panel_size(fx: &Fixture) -> (f32, f32) {
    fx.tester
        .query(by_testid("hardware-panel"))
        .immediately()
        .expect("no hardware panel mounted")
        .size()
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

/// The editor mounts headless: the rail, all eight classic-comp knobs, and
/// the graph's own readouts render with real (non-collapsed) layout.
#[tokio::test]
async fn editor_mounts_headless_with_knobs_and_readouts() -> dioxus_test::Result<()> {
    let fx = mount();

    let html = fx.tester.query(":root").immediately()?.inner_html();
    assert!(html.contains("FTS Comp"), "plugin identity missing from the rail");
    for name in [
        "Threshold",
        "Ratio",
        "Attack",
        "Release",
        "Knee",
        "Makeup",
        "Mix",
        "Stereo Link",
    ] {
        assert!(html.contains(name), "knob label {name:?} missing from DOM");
    }
    // Metering lives in the graph, not in a meter strip: the GR readout and
    // the threshold/ratio/knee line are what a glance reads.
    assert!(html.contains("GR"), "GR readout missing from the graph");
    assert!(
        html.contains("Thr · Ratio · Knee"),
        "param readout missing from the graph"
    );

    // Every knob got a real (non-collapsed) layout box.
    for id in [
        "knob-threshold",
        "knob-ratio",
        "knob-attack",
        "knob-release",
        "knob-knee",
        "knob-makeup",
        "knob-mix",
        "knob-link",
    ] {
        let el = fx.tester.query(by_testid(id)).immediately()?;
        let (w, h) = el.size();
        assert!(
            w > 20.0 && h > 20.0,
            "knob {id} collapsed to {w}x{h}px — Tailwind/layout broken"
        );
    }

    // Default threshold value is rendered by the knob (nice_plug formatter).
    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("-20.0")))
        .immediately()?;
    Ok(())
}

/// Press-and-release on a knob without moving is not a value change: the
/// drag begins and ends but records no Set, and the param keeps its default.
#[tokio::test]
async fn clicking_a_knob_without_dragging_changes_nothing() -> dioxus_test::Result<()> {
    let fx = mount();
    let key = ptr_key(fx.params.ratio.as_ptr());
    let before = fx.params.ratio.value();

    let (x, y) = fx.knob_center("knob-ratio");
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    let _ = fx.tester.pump().await;

    assert_eq!(fx.params.ratio.value(), before, "click alone moved the ratio");
    let log = fx.log.lock().unwrap();
    let sets = log.iter().filter(|g| matches!(g, Gesture::Set(k, _) if *k == key)).count();
    assert_eq!(sets, 0, "click without drag recorded value sets: {log:?}");
    Ok(())
}

/// THE drag test: press on the Threshold knob, drag 30 px up in steps,
/// release. Vertical knob drags map up → increase (DragProvider, sensitivity
/// 150 px per full sweep), so the threshold must rise from its −20 dB default
/// by 30/150 of the −60..0 range = +12 dB, through recorded host gestures.
#[tokio::test]
async fn dragging_threshold_knob_up_raises_threshold() -> dioxus_test::Result<()> {
    let fx = mount();
    let tp = &fx.params.threshold_db;
    let key = ptr_key(tp.as_ptr());

    let before = tp.value();
    assert!((before - (-20.0)).abs() < 1e-4, "default threshold: {before}");

    let (sx, sy) = fx.knob_center("knob-threshold");
    let dy = -30.0; // up

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        let t = step as f64 / 3.0;
        fx.tester.pointer_move(sx, sy + dy * t, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + dy);
    let _ = fx.tester.pump().await;

    let after = tp.value();
    assert!(
        after > before,
        "drag up did not raise threshold: {before} → {after}"
    );
    // 30 px at 150 px-per-sweep over the linear −60..0 dB range = +12 dB.
    let expected = before + (30.0 / 150.0) * 60.0;
    assert!(
        (after - expected).abs() < 0.5,
        "threshold landed at {after} dB, expected ~{expected} dB"
    );

    // The host saw a real automation gesture: begin, one set per drag step
    // (monotonically nondecreasing — every step moved up), then end.
    let log = fx.log.lock().unwrap();
    let begins = log.iter().filter(|g| matches!(g, Gesture::Begin(k) if *k == key)).count();
    let ends = log.iter().filter(|g| matches!(g, Gesture::End(k) if *k == key)).count();
    let sets: Vec<f32> = log
        .iter()
        .filter_map(|g| match g {
            Gesture::Set(k, v) if *k == key => Some(*v),
            _ => None,
        })
        .collect();
    assert!(begins >= 1, "no begin gesture for threshold: {log:?}");
    assert!(ends >= 1, "no end gesture for threshold: {log:?}");
    assert!(sets.len() >= 3, "expected ≥3 set gestures, got {}", sets.len());
    assert!(
        sets.windows(2).all(|w| w[1] >= w[0]),
        "threshold sets not monotonic: {sets:?}"
    );
    Ok(())
}

/// The compressor graph renders: the container has real (non-collapsed)
/// layout, its height and its viewBox height agree (which is what makes
/// pointer y viewBox y), and the SVG transfer-curve path is present and
/// well-formed (61 polyline points across the display range).
#[tokio::test]
async fn graph_renders_transfer_curve() -> dioxus_test::Result<()> {
    let fx = mount();

    let el = fx.tester.query(by_testid("comp-graph")).immediately()?;
    let (w, h) = el.size();
    assert!(w > 300.0, "graph too narrow: {w}px");
    assert!(h as f64 >= 220.0, "graph collapsed to {h}px");
    // The viewBox has to be the container's pixel size on both axes: blitz
    // scales a viewBox uniformly (it ignores preserveAspectRatio), so any
    // other aspect letterboxes the graph and breaks pointer ↔ viewBox.
    assert!(
        el.inner_html()
            .contains(&format!("viewBox=\"0 0 {} {}\"", w.round(), h.round())),
        "graph viewBox does not match its {w}x{h}px container — \
         pointer↔viewBox mapping broken"
    );

    let d = fx.transfer_curve_d();
    assert!(d.starts_with("M "), "transfer path malformed: {d}");
    assert_eq!(d.matches("L ").count(), 60, "expected 61 curve points: {d}");

    // The threshold line + readouts rendered too.
    let html = el.inner_html();
    assert!(html.contains("GR"), "GR readout missing from graph");
    assert!(html.contains("Thr · Ratio · Knee"), "param readout missing from graph");
    Ok(())
}

/// Grab the threshold line (drawn at db_to_y(−20 dB)) and drag it 45 px down.
/// The threshold must fall to the dB the pointer lands on (−29 dB on the
/// 60 dB / 300 px scale), through recorded host gestures — and the rendered
/// transfer-curve path must move with it.
#[tokio::test]
async fn dragging_threshold_line_on_graph_lowers_threshold() -> dioxus_test::Result<()> {
    let fx = mount();
    let tp = &fx.params.threshold_db;
    let key = ptr_key(tp.as_ptr());
    let before = tp.value();
    assert!((before - (-20.0)).abs() < 1e-4, "default threshold: {before}");

    let d_before = fx.transfer_curve_d();

    let (gx, gy) = fx.graph_origin();
    let graph_h = fx.graph_h();
    let ty = db_to_y(before as f64, graph_h); // a third of the way down for −20 dB
    let (sx, sy) = (gx + 180.0, gy + ty);

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy + 15.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + 45.0);
    let _ = fx.tester.pump().await;

    // 45 px down on the 60 dB / graph-height scale.
    let after = tp.value();
    assert!(after < before, "drag down did not lower threshold: {before} → {after}");
    let expected = -(((ty + 45.0) / graph_h) * 60.0) as f32;
    assert!(
        (after - expected).abs() < 0.5,
        "threshold landed at {after} dB, expected ~{expected} dB"
    );

    // Real host gestures: begin, one set per move (monotonically falling —
    // every step moved down), end.
    {
        let log = fx.log.lock().unwrap();
        let begins = log.iter().filter(|g| matches!(g, Gesture::Begin(k) if *k == key)).count();
        let ends = log.iter().filter(|g| matches!(g, Gesture::End(k) if *k == key)).count();
        let sets: Vec<f32> = log
            .iter()
            .filter_map(|g| match g {
                Gesture::Set(k, v) if *k == key => Some(*v),
                _ => None,
            })
            .collect();
        assert!(begins >= 1, "no begin gesture for threshold: {log:?}");
        assert!(ends >= 1, "no end gesture for threshold: {log:?}");
        assert!(sets.len() >= 3, "expected ≥3 set gestures, got {}", sets.len());
        assert!(
            sets.windows(2).all(|w| w[1] <= w[0]),
            "threshold sets not monotonically falling: {sets:?}"
        );
    }

    // The rendered curve tracked the param change.
    let d_after = fx.transfer_curve_d();
    assert_ne!(d_before, d_after, "transfer-curve path did not move with the threshold");
    Ok(())
}

/// Press in the compressed region well above the threshold line and drag
/// 60 px down: the slope tilts — one full doubling of the ratio (4:1 → ~8:1)
/// through recorded host gestures.
#[tokio::test]
async fn dragging_above_knee_on_graph_raises_ratio() -> dioxus_test::Result<()> {
    let fx = mount();
    let rp = &fx.params.ratio;
    let key = ptr_key(rp.as_ptr());
    let thr_key = ptr_key(fx.params.threshold_db.as_ptr());
    let before = rp.value();
    assert!((before - 4.0).abs() < 1e-4, "default ratio: {before}");

    let (gx, gy) = fx.graph_origin();
    let ty = db_to_y(-20.0, fx.graph_h()); // the threshold line
    // 60 px above the line — outside the ±16 px threshold grab zone, inside
    // the compressed region.
    let (sx, sy) = (gx + 180.0, gy + ty - 60.0);

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy + 20.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + 60.0);
    let _ = fx.tester.pump().await;

    // 60 px at 60 px-per-doubling = ratio × 2 (through the skewed range's
    // normalized round-trip).
    let after = rp.value();
    assert!(after > before, "drag down did not raise ratio: {before} → {after}");
    assert!(
        (after - 8.0).abs() < 1.0,
        "ratio landed at {after}:1, expected ~8:1"
    );

    let log = fx.log.lock().unwrap();
    let begins = log.iter().filter(|g| matches!(g, Gesture::Begin(k) if *k == key)).count();
    let ends = log.iter().filter(|g| matches!(g, Gesture::End(k) if *k == key)).count();
    let sets: Vec<f32> = log
        .iter()
        .filter_map(|g| match g {
            Gesture::Set(k, v) if *k == key => Some(*v),
            _ => None,
        })
        .collect();
    assert!(begins >= 1, "no begin gesture for ratio: {log:?}");
    assert!(ends >= 1, "no end gesture for ratio: {log:?}");
    assert!(sets.len() >= 3, "expected ≥3 set gestures, got {}", sets.len());
    assert!(
        sets.windows(2).all(|w| w[1] >= w[0]),
        "ratio sets not monotonically rising: {sets:?}"
    );
    // A ratio drag must not touch the threshold.
    assert!(
        !log.iter().any(|g| matches!(g, Gesture::Set(k, _) if *k == thr_key)),
        "ratio drag leaked threshold sets: {log:?}"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Sectioned surface / Basic-Advanced disclosure
// ─────────────────────────────────────────────────────────────────────────

/// Every classic knob lives in a labelled section, and Basic mode hides the
/// extended stages — the advanced-only sections must not be in the DOM until
/// the disclosure is opened.
#[tokio::test]
async fn basic_mode_shows_core_sections_only() -> dioxus_test::Result<()> {
    let fx = mount();

    for id in ["section-dynamics", "section-detector", "section-character", "section-output"] {
        let el = fx.tester.query(by_testid(id)).immediately()?;
        let (w, h) = el.size();
        assert!(w > 40.0 && h > 30.0, "section {id} collapsed to {w}x{h}px");
    }

    for id in ["section-sidechain", "section-expander", "section-upward"] {
        assert!(
            fx.tester.query(by_testid(id)).immediately().is_err(),
            "advanced section {id} rendered while in Basic mode"
        );
    }

    // The style selector and the rail's profile list are always available.
    fx.tester.query(by_testid("select-style")).immediately()?;
    for id in comp_ui::faces::PROFILE_IDS {
        fx.tester.query(by_testid(&format!("rail-item-{id}"))).immediately()?;
    }
    Ok(())
}

/// Clicking the disclosure reveals the extended stages with real layout, and
/// does so without recording any host automation — the Basic/Advanced split is
/// local UI state, not a parameter.
#[tokio::test]
async fn advanced_toggle_reveals_extended_sections() -> dioxus_test::Result<()> {
    let mut fx = mount();

    let el = fx.tester.query(by_testid("advanced-toggle")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.settle().await;

    for id in ["section-sidechain", "section-expander", "section-upward"] {
        let el = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("advanced section {id} still missing: {e:?}"));
        let (w, h) = el.size();
        assert!(w > 40.0 && h > 30.0, "advanced section {id} collapsed to {w}x{h}px");
    }

    // The advanced-only knobs of every stage are now hit-testable.
    for id in [
        "knob-schp", "knob-sclp", "knob-expthresh", "knob-expratio", "knob-upthresh",
        "knob-upratio", "knob-ingain", "knob-ceiling", "knob-rmsmix", "knob-feedback",
        "knob-lookahead", "knob-inertia", "knob-inertiadecay", "knob-hold", "knob-range",
    ] {
        let el = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("advanced knob {id} missing: {e:?}"));
        let (w, h) = el.size();
        assert!(w > 20.0 && h > 20.0, "advanced knob {id} collapsed to {w}x{h}px");
    }

    // Advanced *replaces* the page — the Basic-only sections are gone, which
    // is what keeps every view inside the editor's fixed height.
    for id in ["section-dynamics", "section-output"] {
        assert!(
            fx.tester.query(by_testid(id)).immediately().is_err(),
            "Basic section {id} still rendered on the Advanced page"
        );
    }

    assert!(
        fx.log.lock().unwrap().is_empty(),
        "toggling Basic/Advanced recorded host automation — it must stay local UI state"
    );
    Ok(())
}

/// An advanced knob drives its parameter through real host gestures, proving
/// the extended surface is wired end to end and not just decorative.
#[tokio::test]
async fn dragging_an_advanced_knob_drives_its_param() -> dioxus_test::Result<()> {
    let mut fx = mount();

    let el = fx.tester.query(by_testid("advanced-toggle")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.settle().await;

    let dp = &fx.params.ceiling;
    let key = ptr_key(dp.as_ptr());
    let before = dp.value();
    assert!(before.abs() < 1e-6, "ceiling should default to 0: {before}");

    let (sx, sy) = fx.knob_center("knob-ceiling");
    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy - 10.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy - 30.0);
    let _ = fx.tester.pump().await;

    // 30 px at 150 px per full sweep over the linear 0..1 range = +0.2.
    let after = dp.value();
    assert!(
        (after - 0.2).abs() < 0.02,
        "ceiling landed at {after}, expected ~0.2"
    );

    let log = fx.log.lock().unwrap();
    assert!(
        log.iter().any(|g| matches!(g, Gesture::Begin(k) if *k == key)),
        "no begin gesture for ceiling: {log:?}"
    );
    assert!(
        log.iter().any(|g| matches!(g, Gesture::End(k) if *k == key)),
        "no end gesture for ceiling: {log:?}"
    );
    Ok(())
}

/// Selecting a hardware profile swaps the whole UI for that unit's front
/// panel: the FTS surface (graph + sections) is gone, and the LA-2A's meter,
/// knobs and mode switch are in the DOM with real layout.
#[tokio::test]
async fn choosing_a_profile_swaps_in_its_faceplate() -> dioxus_test::Result<()> {
    let mut fx = mount();

    // The FTS surface is what we start on.
    fx.tester.query(by_testid("section-dynamics")).immediately()?;
    fx.tester.query(by_testid("comp-graph")).immediately()?;

    select_profile(&mut fx, 1).await?;
    assert_eq!(fx.params.profile.value(), 1, "profile param did not move to LA-2A");

    // The FTS surface is gone — this is a face swap, not a re-tint.
    for id in ["section-dynamics", "section-output", "comp-graph", "meters"] {
        assert!(
            fx.tester.query(by_testid(id)).immediately().is_err(),
            "{id} survived the swap to the LA-2A faceplate"
        );
    }

    // The panel and its controls are there, and laid out.
    for id in [
        "hardware-panel",
        "vu-meter",
        "hw-knob-gain",
        "hw-knob-peak-reduction",
        "hw-switch-mode",
    ] {
        let el = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("{id} missing from the LA-2A face: {e:?}"));
        let (w, h) = el.size();
        assert!(w > 10.0 && h > 10.0, "{id} collapsed to {w}x{h}px");
    }

    // The Basic/Advanced toggle belongs to the FTS surface — a unit has the
    // controls it has.
    assert!(
        fx.tester.query(by_testid("advanced-toggle")).immediately().is_err(),
        "the Advanced toggle followed us onto a hardware face"
    );
    Ok(())
}

/// Every profile has a face, and switching between them leaves no debris from
/// the previous one.
#[tokio::test]
async fn every_profile_renders_its_own_face() -> dioxus_test::Result<()> {
    let mut fx = mount();

    // (profile index, a control only that face has)
    for (index, marker) in [
        (1usize, "hw-knob-peak-reduction"),
        (2, "hw-knob-makeup"),
        (3, "hw-buttons-ratio"),
    ] {
        select_profile(&mut fx, index).await?;
        assert_eq!(fx.params.profile.value(), index as i32);
        fx.tester
            .query(by_testid(marker))
            .immediately()
            .unwrap_or_else(|e| panic!("profile {index} face missing {marker}: {e:?}"));
    }

    // …and back to the FTS surface.
    select_profile(&mut fx, 0).await?;
    fx.tester.query(by_testid("section-dynamics")).immediately()?;
    assert!(
        fx.tester.query(by_testid("hardware-panel")).immediately().is_err(),
        "a faceplate survived the return to the Control surface"
    );
    Ok(())
}

/// The LA-2A's PEAK REDUCTION is a *macro*: one knob writing five engine
/// params on linked curves. Dragging it has to move all of them — that is the
/// difference between a faceplate that drives the engine and one that
/// decorates it — and to store its own position, since it cannot be recovered
/// from any single param it wrote.
#[tokio::test]
async fn dragging_peak_reduction_drives_every_param_behind_it() -> dioxus_test::Result<()> {
    let mut fx = mount();
    select_profile(&mut fx, 1).await?;

    // Park the knob first. Until the macro has been turned once, the engine
    // still holds the plugin's own defaults, which are not what any macro
    // position maps to — so a first touch necessarily jumps. Compare two
    // positions of the macro, not the macro against the defaults.
    let (kx, ky) = fx.knob_center("hw-knob-peak-reduction");
    fx.tester.pointer_down(kx, ky);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_move(kx, ky - 40.0, true);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(kx, ky - 40.0);
    fx.settle().await;

    let before = (
        fx.params.threshold_db.value(),
        fx.params.ratio.value(),
        fx.params.knee_db.value(),
        fx.params.range_db.value(),
        fx.params.drive.value(),
        fx.params.macro1.value(),
    );

    let (sx, sy) = fx.knob_center("hw-knob-peak-reduction");
    // Down = less peak reduction; a big move so every curve clears its
    // rounding.
    let dy = 60.0;
    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy + dy * step as f64 / 3.0, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + dy);
    fx.settle().await;

    let after = (
        fx.params.threshold_db.value(),
        fx.params.ratio.value(),
        fx.params.knee_db.value(),
        fx.params.range_db.value(),
        fx.params.drive.value(),
        fx.params.macro1.value(),
    );

    assert!(after.5 < before.5, "the macro slot did not store the new position");
    // Every curve in the LA-2A's compound mapping rises with peak reduction,
    // so turning it down has to raise the threshold and lower the rest.
    assert!(after.0 > before.0, "threshold: {} → {}", before.0, after.0);
    assert!(after.1 < before.1, "ratio: {} → {}", before.1, after.1);
    assert!(after.2 < before.2, "knee: {} → {}", before.2, after.2);
    assert!(after.3 < before.3, "range: {} → {}", before.3, after.3);
    assert!(after.4 < before.4, "drive: {} → {}", before.4, after.4);

    // The host saw one bracketed gesture per param it moved, not one for the
    // knob — automation records the engine, not the macro alone.
    let log = fx.log.lock().unwrap();
    for (name, key) in [
        ("threshold", ptr_key(fx.params.threshold_db.as_ptr())),
        ("ratio", ptr_key(fx.params.ratio.as_ptr())),
        ("knee", ptr_key(fx.params.knee_db.as_ptr())),
        ("macro1", ptr_key(fx.params.macro1.as_ptr())),
    ] {
        assert!(
            log.iter().any(|g| matches!(g, Gesture::Begin(k) if *k == key)),
            "no begin gesture for {name}"
        );
        assert!(
            log.iter().any(|g| matches!(g, Gesture::End(k) if *k == key)),
            "no end gesture for {name}"
        );
    }
    Ok(())
}

/// The 1176's ratio buttons are radio-like: pressing one sets the ratio to
/// that button's value and leaves it pressed.
#[tokio::test]
async fn pressing_a_ratio_button_sets_that_ratio() -> dioxus_test::Result<()> {
    let mut fx = mount();
    select_profile(&mut fx, 3).await?;

    // Button 3 of 4:8:12:20:All.
    let el = fx.tester.query(by_testid("hw-button-ratio-3")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;

    assert!(
        (fx.params.ratio.value() - 20.0).abs() < 0.01,
        "ratio is {} after pressing the 20 button",
        fx.params.ratio.value()
    );
    // …and the button stays in, read back off the ratio the engine now holds.
    let group = fx.tester.query(by_testid("hw-buttons-ratio")).immediately()?;
    assert_eq!(
        group.attribute("data-index").as_deref(),
        Some("3"),
        "the pressed button did not stay in"
    );
    Ok(())
}

/// A faceplate is a fixed drawing: a bigger editor draws the same panel
/// larger rather than reflowing it, and a smaller one draws it smaller.
#[tokio::test]
async fn the_faceplate_scales_with_the_editor() -> dioxus_test::Result<()> {
    let mut small = mount_sized(comp_ui::control_view::EDITOR_W, comp_ui::control_view::EDITOR_H);
    select_profile(&mut small, 1).await?;
    let (sw, sh) = panel_size(&small);

    let mut large = mount_sized(1600, 1000);
    select_profile(&mut large, 1).await?;
    let (lw, lh) = panel_size(&large);

    assert!(lw > sw && lh > sh, "panel did not grow: {sw}x{sh} → {lw}x{lh}");
    // Uniform scale — the panel must not stretch on one axis.
    let (small_ar, large_ar) = (sw / sh, lw / lh);
    assert!(
        (small_ar - large_ar).abs() < 0.02,
        "aspect ratio drifted: {small_ar:.3} → {large_ar:.3}"
    );
    Ok(())
}

/// The compressor graph is no longer pinned to a fixed height: it takes its
/// share of a taller editor. The container and the viewBox have to agree, or
/// pointer-y stops mapping onto dB — so this checks the rendered container.
#[tokio::test]
async fn the_graph_grows_with_a_taller_editor() -> dioxus_test::Result<()> {
    let short = mount_sized(comp_ui::control_view::EDITOR_W, comp_ui::control_view::EDITOR_H);
    let tall = mount_sized(comp_ui::control_view::EDITOR_W, 1000);

    let (_, short_h) = short.tester.query(by_testid("comp-graph")).immediately()?.size();
    let (_, tall_h) = tall.tester.query(by_testid("comp-graph")).immediately()?.size();

    assert!(
        tall_h > short_h,
        "graph height did not grow with the editor: {short_h} → {tall_h}"
    );
    Ok(())
}


/// The Advanced page must fit the editor size the plugin shell requests from
/// the host (`DioxusState::new(|| (EDITOR_W, EDITOR_H))` in `comp-plugin`).
/// Blitz will not overflow-scroll a height-constrained container, so anything
/// that does not fit is not merely clipped — it collapses to 0×0 and becomes
/// unreachable. This is the regression guard for that.
#[tokio::test]
async fn advanced_page_fits_the_plugin_editor_size() -> dioxus_test::Result<()> {
    let mut fx = mount_sized(comp_ui::control_view::EDITOR_W, comp_ui::control_view::EDITOR_H);

    let el = fx.tester.query(by_testid("advanced-toggle")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.settle().await;

    for id in [
        "section-detector", "section-sidechain", "section-expander", "section-upward",
        "section-character",
    ] {
        let el = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("{id} missing at editor size: {e:?}"));
        let (w, h) = el.size();
        assert!(
            w > 40.0 && h > 30.0,
            "{id} collapsed to {w}x{h}px at the plugin's {}x{} editor size",
            comp_ui::control_view::EDITOR_W,
            comp_ui::control_view::EDITOR_H,
        );
    }

    // Every advanced knob stays hit-testable at that size.
    for id in ["knob-schp", "knob-expratio", "knob-upratio", "knob-ceiling", "knob-inertia"] {
        let (w, h) = fx.tester.query(by_testid(id)).immediately()?.size();
        assert!(w > 20.0 && h > 20.0, "{id} collapsed to {w}x{h}px at editor size");
    }
    Ok(())
}

/// The editor declares itself resizable down to
/// `MIN_EDITOR_W` x `MIN_EDITOR_H`, and `DioxusEditorHandle::set_size` enforces
/// that floor. This checks the floor is honest: the Advanced page — the densest
/// one — must still lay out at exactly the declared minimum.
///
/// It matters more than a normal layout test because Blitz does not clip what
/// does not fit, it collapses it to 0x0. A minimum that is a little too small
/// does not produce a cramped editor, it produces unreachable controls.
#[tokio::test]
async fn advanced_page_survives_the_declared_minimum_size() -> dioxus_test::Result<()> {
    let mut fx = mount_sized(
        comp_ui::control_view::MIN_EDITOR_W as u32,
        comp_ui::control_view::MIN_EDITOR_H as u32,
    );

    let el = fx.tester.query(by_testid("advanced-toggle")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.settle().await;

    for id in [
        "section-detector", "section-sidechain", "section-expander", "section-upward",
        "section-character",
    ] {
        let el = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("{id} missing at the declared minimum size: {e:?}"));
        let (w, h) = el.size();
        assert!(
            w > 40.0 && h > 30.0,
            "{id} collapsed to {w}x{h}px at the declared minimum {}x{} — \
             the minimum in control_view::resize_hint() is too small",
            comp_ui::control_view::MIN_EDITOR_W,
            comp_ui::control_view::MIN_EDITOR_H,
        );
    }

    // And the knobs inside them stay hit-testable.
    for id in ["knob-schp", "knob-expratio", "knob-upratio", "knob-ceiling"] {
        let (w, h) = fx.tester.query(by_testid(id)).immediately()?.size();
        assert!(w > 20.0 && h > 20.0, "{id} collapsed to {w}x{h}px at the minimum size");
    }
    Ok(())
}
