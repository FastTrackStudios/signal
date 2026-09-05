//! The preset browser panel, driven the way a person drives it.
//!
//! The browsing rules themselves are covered headlessly in `preset-browser`.
//! What these cover is the part only a mounted panel can answer: that the list
//! renders, that typing and clicking reach the model, and — the one that
//! actually matters to a plugin — that choosing a preset hands its parameters
//! to the host.

use std::sync::{Arc, Mutex, OnceLock};

use dioxus::prelude::*;
use dioxus_test::{by_testid, render, DocumentTester};
use preset_browser::{Preset, PresetBrowser};

/// A recorder for what the panel applied.
///
/// `render` takes a component with no props, so there is nowhere to thread one
/// in and it has to be reachable statically. One static per test rather than
/// one shared and cleared: these run in parallel, and a shared log gets
/// emptied out from under whichever test is mid-click.
type Applied = Arc<Mutex<Vec<Vec<(String, f64)>>>>;

macro_rules! recording_harness {
    ($log:ident, $harness:ident) => {
        fn $log() -> &'static Applied {
            static LOG: OnceLock<Applied> = OnceLock::new();
            LOG.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        }

        #[component]
        fn $harness() -> Element {
            let browser = use_signal(|| PresetBrowser::new(library()));
            rsx! {
                preset_browser_ui::PresetBrowserPanel {
                    browser,
                    title: "Reverb Presets".to_string(),
                    on_apply: move |params: Vec<(String, f64)>| {
                        $log().lock().unwrap().push(params);
                    },
                }
            }
        }
    };
}

recording_harness!(click_log, ClickHarness);
recording_harness!(step_log, StepHarness);

fn preset(name: &str, category: &str, decay: f64, err: Option<f64>) -> Preset {
    Preset {
        name: name.into(),
        category: Some(category.into()),
        author: None,
        tags: vec![],
        origin: Some("VintageVerb".into()),
        parameters: vec![("decay_time".into(), decay)],
        match_error: err,
    }
}

fn library() -> Vec<Preset> {
    vec![
        preset("Snare Plate", "Plate", 1.5, Some(0.01)),
        preset("Dark Vocal Plate", "Plate", 2.5, Some(0.20)),
        preset("Acoustic Chamber", "Chamber", 3.5, None),
    ]
}

/// The read-only harness, for tests that only look at what is rendered.
#[component]
fn Harness() -> Element {
    let browser = use_signal(|| PresetBrowser::new(library()));
    rsx! {
        preset_browser_ui::PresetBrowserPanel {
            browser,
            title: "Reverb Presets".to_string(),
            on_apply: move |_: Vec<(String, f64)>| {},
        }
    }
}

async fn mount(app: fn() -> Element) -> DocumentTester {
    let tester = render(app).with_window_size(320, 420).build();
    let _ = tester.pump().await;
    tester
}

async fn panel() -> DocumentTester {
    mount(Harness).await
}

async fn click(tester: &mut DocumentTester, testid: &str) -> dioxus_test::Result<()> {
    let el = tester.query(by_testid(testid)).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let (x, y) = (ox + f64::from(w) / 2.0, oy + f64::from(h) / 2.0);
    tester.pointer_down(x, y);
    let _ = tester.pump().await;
    tester.pointer_up(x, y);
    let _ = tester.pump().await;
    Ok(())
}

#[tokio::test]
async fn the_panel_lists_the_library() -> dioxus_test::Result<()> {
    let tester = panel().await;
    let rows: Vec<_> = tester
        .query_all(by_testid("preset-entry"))
        .immediately()
        .into_iter()
        .collect();
    assert_eq!(rows.len(), 3, "every preset is listed");
    Ok(())
}

#[tokio::test]
async fn choosing_a_preset_hands_its_parameters_over() -> dioxus_test::Result<()> {
    // The whole point of the panel: a click has to reach the DSP.
    let mut tester = mount(ClickHarness).await;
    assert!(
        click_log().lock().unwrap().is_empty(),
        "nothing applied on open"
    );

    // The list defaults to alphabetical order, so the first row is
    // "Acoustic Chamber" and not the first preset in the library.
    click(&mut tester, "preset-entry").await?;

    let calls = click_log().lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "one preset applied");
    assert_eq!(
        calls[0],
        vec![("decay_time".to_string(), 3.5)],
        "and it is the parameters of the row that was clicked",
    );
    Ok(())
}

#[tokio::test]
async fn stepping_applies_as_it_goes() -> dioxus_test::Result<()> {
    // A next/previous control is only useful if it auditions what it lands on.
    let mut tester = mount(StepHarness).await;
    click(&mut tester, "preset-next").await?;
    click(&mut tester, "preset-next").await?;

    // Stepping walks the list as displayed — alphabetically — not the order
    // the library happens to be stored in.
    let calls = step_log().lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0].1, 3.5, "first step lands on Acoustic Chamber");
    assert_eq!(calls[1][0].1, 2.5, "and the second on Dark Vocal Plate");
    Ok(())
}

#[tokio::test]
async fn a_category_narrows_the_list() -> dioxus_test::Result<()> {
    let tester = panel().await;
    // Chips are "All", then the categories in order: Chamber, Plate.
    let chips: Vec<_> = tester
        .query_all(by_testid("preset-category"))
        .immediately()
        .into_iter()
        .collect();
    assert_eq!(chips.len(), 3, "All plus the two categories");

    let el = &chips[1]; // Chamber
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    tester.pointer_down(ox + f64::from(w) / 2.0, oy + f64::from(h) / 2.0);
    let _ = tester.pump().await;
    tester.pointer_up(ox + f64::from(w) / 2.0, oy + f64::from(h) / 2.0);
    let _ = tester.pump().await;

    let rows: Vec<_> = tester
        .query_all(by_testid("preset-entry"))
        .immediately()
        .into_iter()
        .collect();
    assert_eq!(rows.len(), 1, "only the chamber remains");
    Ok(())
}

#[tokio::test]
async fn only_measured_presets_carry_a_match_badge() -> dioxus_test::Result<()> {
    // Two of the three were measured; the third never was, and saying nothing
    // is more honest than showing it as though it scored.
    let tester = panel().await;
    let badges: Vec<_> = tester
        .query_all(by_testid("preset-match"))
        .immediately()
        .into_iter()
        .collect();
    assert_eq!(badges.len(), 2);
    Ok(())
}

// ── The always-visible strip ───────────────────────────────────────────────

/// A bar with no recorder, for the tests that only read what is displayed.
///
/// Separate from the recording one on purpose: these run in parallel, and a
/// test that clicks twice to reach a measured preset would otherwise land its
/// applies in the stepping test's log.
#[component]
fn BarDisplayHarness() -> Element {
    let browser = use_signal(|| PresetBrowser::new(library()));
    rsx! {
        preset_browser_ui::PresetBar {
            browser,
            on_apply: move |_: Vec<(String, f64)>| {},
            on_browse: move |()| {},
        }
    }
}

fn bar_log() -> &'static Applied {
    static LOG: OnceLock<Applied> = OnceLock::new();
    LOG.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

/// The bar on its own, so the assertions are about the strip and not the list.
#[component]
fn BarHarness() -> Element {
    let browser = use_signal(|| PresetBrowser::new(library()));
    rsx! {
        preset_browser_ui::PresetBar {
            browser,
            on_apply: move |params: Vec<(String, f64)>| {
                bar_log().lock().unwrap().push(params);
            },
            on_browse: move |()| {},
        }
    }
}

#[tokio::test]
async fn the_bar_says_what_is_loaded_before_anything_is_chosen() -> dioxus_test::Result<()> {
    // An editor that shows nothing leaves the reader guessing what it is set
    // to, which is worse than saying "Init".
    let tester = mount(BarDisplayHarness).await;
    let name = tester.query(by_testid("preset-bar-name")).immediately()?;
    assert!(name.size().0 > 0.0, "the name is rendered, not blank");
    // Nothing is selected, so nothing has been applied.
    assert!(bar_log().lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn stepping_from_the_bar_applies() -> dioxus_test::Result<()> {
    let mut tester = mount(BarHarness).await;
    click(&mut tester, "preset-bar-next").await?;

    let calls = bar_log().lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "stepping auditions what it lands on");
    // Alphabetical order, so the first is Acoustic Chamber.
    assert_eq!(calls[0][0].1, 3.5);
    Ok(())
}

#[tokio::test]
async fn the_bar_shows_a_match_badge_only_once_a_preset_is_chosen() -> dioxus_test::Result<()> {
    let mut tester = mount(BarDisplayHarness).await;
    assert!(
        tester
            .query(by_testid("preset-bar-match"))
            .immediately()
            .is_err(),
        "nothing is selected, so there is no match quality to report",
    );

    // Acoustic Chamber was never measured, so still no badge; step again to
    // one that was.
    click(&mut tester, "preset-bar-next").await?;
    click(&mut tester, "preset-bar-next").await?;
    assert!(
        tester
            .query(by_testid("preset-bar-match"))
            .immediately()
            .is_ok(),
        "a measured preset reports how closely it matches",
    );
    Ok(())
}
