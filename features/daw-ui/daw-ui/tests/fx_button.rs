//! The FX button tells the truth, and keeps telling it.
//!
//! The bug this closes is not "the button is wrong" but "the button goes
//! wrong": `Track::fx_count` was seeded once and never updated, so every
//! assertion here that matters is about the *second* state, after an event.

use daw_proto::{Track, TrackEvent};
use daw_ui::controls::{FxButton, TrackStore};
use dioxus::prelude::*;

fn track(guid: &str, fx: u32, input_fx: u32) -> Track {
    Track {
        guid: guid.to_string(),
        name: "Kick".into(),
        fx_count: fx,
        input_fx_count: input_fx,
        ..Default::default()
    }
}

thread_local! {
    static STORE: std::cell::Cell<Option<TrackStore>> = const { std::cell::Cell::new(None) };
}

/// Mounts one button over a store seeded with a single track.
fn mount(fx: u32, input_fx: u32) -> VirtualDom {
    #[derive(Props, Clone, PartialEq)]
    struct P {
        fx: u32,
        input_fx: u32,
    }

    let mut dom = VirtualDom::new_with_props(
        |p: P| {
            let mut store = use_hook(TrackStore::new);
            use_hook(|| {
                store.seed([track("T1", p.fx, p.input_fx)]);
                provide_context(store);
                STORE.with(|s| s.set(Some(store)));
            });
            rsx! { FxButton { track: "T1" } }
        },
        P { fx, input_fx },
    );
    dom.rebuild_in_place();
    dom
}

fn settle(dom: &mut VirtualDom) {
    dom.render_immediate(&mut dioxus::core::NoOpMutations);
}

#[test]
fn the_button_is_lit_by_a_non_empty_chain() {
    let empty = dioxus_ssr::render(&mount(0, 0));
    let loaded = dioxus_ssr::render(&mount(2, 0));
    assert_ne!(empty, loaded, "an empty chain draws the same as a full one");
    assert!(empty.contains("<svg"), "nothing drawn:\n{empty}");
    assert!(
        !loaded.contains("<img"),
        "the button is blitting:\n{loaded}"
    );
    assert!(
        !loaded.contains("currentColor"),
        "a colour is left to CSS:\n{loaded}"
    );
}

/// The input chain is a different chain, and gets its own mark rather than
/// lighting the button that reports the track chain.
#[test]
fn the_input_chain_reads_separately_from_the_track_chain() {
    let neither = dioxus_ssr::render(&mount(0, 0));
    let input_only = dioxus_ssr::render(&mount(0, 1));
    let both = dioxus_ssr::render(&mount(1, 1));

    assert_ne!(neither, input_only, "input FX went unreported");
    assert_ne!(
        input_only, both,
        "the track chain did not light with input FX present"
    );
}

/// The whole point. Adding a plugin arrives as an event, and the button
/// follows it — no refetch, no reopening the mixer.
#[test]
fn adding_fx_updates_the_button_without_a_refetch() {
    let mut dom = mount(0, 0);
    let before = dioxus_ssr::render(&dom);

    dom.in_runtime(|| {
        STORE
            .with(|s| s.get())
            .expect("store")
            .apply(&TrackEvent::FxCountChanged {
                guid: "T1".into(),
                fx_count: 1,
                input_fx_count: 0,
            });
    });
    settle(&mut dom);
    let after = dioxus_ssr::render(&dom);
    assert_ne!(before, after, "the button ignored the new plugin");

    // And back down again when the last plugin is removed.
    dom.in_runtime(|| {
        STORE
            .with(|s| s.get())
            .expect("store")
            .apply(&TrackEvent::FxCountChanged {
                guid: "T1".into(),
                fx_count: 0,
                input_fx_count: 0,
            });
    });
    settle(&mut dom);
    assert_eq!(
        before,
        dioxus_ssr::render(&dom),
        "the button stayed lit on an empty chain"
    );
}

/// The pill drawn wider than its art grows the flat run before the seam.
/// The rounded end and the glyph hold their size — scaled whole, the end
/// elongates into a notch and the `FX` stretches with it.
#[test]
fn a_wider_pill_grows_its_flat_run_and_not_its_ends() {
    let art = daw_theme_art::slice::expect_art("mcp_fx_norm");
    let panes = art.row();

    assert!(panes.len() >= 2, "the pill did not decompose: {panes:?}");
    let growing: Vec<_> = panes.iter().filter(|p| p.grow).collect();
    assert_eq!(growing.len(), 1, "exactly one band takes the slack");
    // `mcp_fx_norm` declares x: Middle(23, 28) — the flat run before the
    // seam, at the right-hand end of the 28-wide half.
    assert_eq!(growing[0].view.0, 23.0);
    assert_eq!(growing[0].view.2, 5.0);
    // The fixed band carries the rounded end and the glyph.
    assert_eq!(panes[0].view, (0.0, 0.0, 23.0, 22.0));
    assert!(!panes[0].grow);

    // And the bands tile the source box exactly.
    let total: f32 = panes.iter().map(|p| p.view.2).sum();
    assert_eq!(total, art.source.0);
}

/// A wider button renders more pixels of flat run, not a wider glyph.
/// The pill is one shape, drawn at the width it was asked for.
///
/// Two things this pins. It is a *single* `<svg>`: the split into `mcp.fx`
/// and `mcp.fxbyp` is REAPER's blitting constraint, and composing the two
/// halves as positioned elements put the toggle's rounded end through the
/// `FX` under Blitz. And it is drawn 1:1 rather than scaled — asked for 71
/// with `preserveAspectRatio: none`, a 46-wide drawing stretched by half
/// again, which elongated the rounded ends and blew up the lettering.
#[test]
fn the_pill_is_one_shape_drawn_at_its_asked_for_width() {
    fn app(width: f32) -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([track("T1", 1, 0)]);
            provide_context(store);
        });
        rsx! { FxButton { track: "T1", width } }
    }

    let render = |w: f32| {
        let mut dom = VirtualDom::new_with_props(app, w);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    };

    let narrow = render(28.0);
    let wide = render(43.0);
    assert_ne!(narrow, wide);

    assert_eq!(
        wide.matches("<svg").count(),
        1,
        "the pill is not one shape:\n{wide}"
    );

    // 43 of label plus `mcp.fxbyp`'s 28, and a viewBox to match — which is
    // what makes it 1:1.
    assert!(
        wide.contains("width=\"71\""),
        "the pill did not take its width:\n{wide}"
    );
    assert!(
        wide.contains("viewBox=\"0 0 71 22\""),
        "the pill is being scaled:\n{wide}"
    );
    assert!(
        narrow.contains("viewBox=\"0 0 56 22\""),
        "the narrow pill is scaled:\n{narrow}"
    );
}
