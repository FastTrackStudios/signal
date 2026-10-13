//! The mute button draws, reacts and stays a vector.
//!
//! A Dioxus component that type-checks may still render nothing, or render
//! a picture of itself. Both are regressions this control cannot afford:
//! the point of the vector theme is that the app and the exported REAPER
//! sprite are the same drawing, and a panel that blits a PNG has quietly
//! left that arrangement.

use daw_proto::{Track, TrackEvent};
use daw_theme_art::dress::Panel;
use daw_ui::controls::{MuteButton, TrackStore};
use dioxus::prelude::*;

fn track(guid: &str, muted: bool) -> Track {
    Track {
        guid: guid.to_string(),
        name: "Kick".into(),
        muted,
        ..Default::default()
    }
}

/// Mounts one button over a store seeded with a single track.
fn render_with(muted: bool, panel: Panel) -> String {
    let mut dom = VirtualDom::new_with_props(
        |props: Props| {
            let mut store = use_hook(TrackStore::new);
            use_hook(|| {
                store.seed([track("T1", props.muted)]);
                provide_context(store);
            });
            rsx! { MuteButton { track: "T1", panel: props.panel } }
        },
        Props { muted, panel },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[derive(Props, Clone, PartialEq)]
struct Props {
    muted: bool,
    panel: Panel,
}

#[test]
fn the_button_draws_as_vector_shapes() {
    let html = render_with(false, Panel::Mixer);
    assert!(html.contains("<svg"), "nothing drawn:\n{html}");
    assert!(html.contains("<rect"), "no vector shapes:\n{html}");
    assert!(html.contains(">M<"), "no legend:\n{html}");
    // A panel that blits has left the one-drawing-two-renderings deal.
    assert!(!html.contains("<img"), "the button is blitting:\n{html}");
    assert!(
        !html.contains("url(data:"),
        "the button is blitting:\n{html}"
    );
}

/// `currentColor` inherits from CSS the exporter's rasteriser does not
/// have, so it renders black in REAPER and correct in the browser — a
/// divergence that only shows up in the exported PNG.
#[test]
fn no_colour_is_left_to_be_inherited() {
    for muted in [false, true] {
        for panel in [Panel::Mixer, Panel::Track] {
            let html = render_with(muted, panel);
            assert!(
                !html.contains("currentColor"),
                "currentColor in {panel:?} muted={muted}:\n{html}"
            );
        }
    }
}

/// Layout must not wait for a stylesheet: Blitz, which renders the REAPER
/// panels, does not reliably load external CSS.
#[test]
fn the_box_is_sized_in_explicit_pixels() {
    let html = render_with(false, Panel::Mixer);
    // The mixer's cell is 21x20, the track panel's 21x24 — the sizes come
    // from the art's declared source box, not from a class.
    assert!(html.contains("width:21px"), "no explicit width:\n{html}");
    assert!(html.contains("height:20px"), "no explicit height:\n{html}");
    assert!(
        render_with(false, Panel::Track).contains("height:24px"),
        "the track panel's cell is 24 rows"
    );
    // The svg itself carries pixel dimensions too, not a percentage that
    // resolves against a box CSS never gave it.
    assert!(html.contains("width=\"21\""), "svg unsized:\n{html}");
}

#[test]
fn a_muted_track_lights_the_button() {
    let off = render_with(false, Panel::Mixer);
    let on = render_with(true, Panel::Mixer);
    assert_ne!(off, on, "mute state does not change the drawing");
    assert!(
        on.contains(&daw_theme::Theme::default().signal.mute.to_hex()),
        "the lit face is not the mute red:\n{on}"
    );
}

/// The backend is the source of truth: a mute performed anywhere — REAPER's
/// own menu, another client — arrives as a `MuteChanged` event, and the
/// button must follow it without asking the backend for the track again.
#[test]
fn a_backend_mute_reaches_the_button_without_a_refetch() {
    // The store a Dioxus Signal lives in is thread-local, so the handle the
    // test keeps has to be too.
    thread_local! {
        static STORE: std::cell::Cell<Option<TrackStore>> = const { std::cell::Cell::new(None) };
    }

    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([track("T1", false)]);
            provide_context(store);
            STORE.with(|s| s.set(Some(store)));
        });
        rsx! { MuteButton { track: "T1" } }
    }

    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let before = dioxus_ssr::render(&dom);

    // The event, exactly as the subscription delivers it. No fetch, no
    // reseed — only the store folding one event in.
    dom.in_runtime(|| {
        let mut store = STORE.with(|s| s.get()).expect("store handle");
        store.apply(&TrackEvent::MuteChanged {
            guid: "T1".into(),
            muted: true,
        });
    });
    settle(&mut dom);
    let after = dioxus_ssr::render(&dom);

    assert_ne!(before, after, "the button ignored the backend's mute");
    assert!(
        after.contains(&daw_theme::Theme::default().signal.mute.to_hex()),
        "the button did not light:\n{after}"
    );
}

/// A control that is handed a different track must show that track.
///
/// Hooks run once, so a memo closing over the guid it saw on the first
/// render keeps reporting the first track's state forever — and a mixer
/// that reorders its strips, or renders them without keys, hands exactly
/// that: the same component instance with a new `track` prop.
#[test]
fn re_pointing_the_button_at_another_track_re_reads_it() {
    thread_local! {
        static WHICH: std::cell::Cell<Option<Signal<String>>> =
            const { std::cell::Cell::new(None) };
    }

    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        let which = use_signal(|| "QUIET".to_string());
        use_hook(|| {
            store.seed([track("QUIET", false), track("MUTED", true)]);
            provide_context(store);
            WHICH.with(|w| w.set(Some(which)));
        });
        rsx! { MuteButton { track: which() } }
    }

    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let quiet = dioxus_ssr::render(&dom);

    // The same instance, a different track — not a fresh mount.
    dom.in_runtime(|| {
        WHICH
            .with(|w| w.get())
            .expect("signal")
            .set("MUTED".to_string());
    });
    settle(&mut dom);
    let muted = dioxus_ssr::render(&dom);

    assert_ne!(quiet, muted, "the button kept rendering its first track");
    assert!(
        muted.contains(&daw_theme::Theme::default().signal.mute.to_hex()),
        "the second track is muted and the button is not lit:\n{muted}"
    );
}

/// Hovering the wrapper changes what is drawn — driven by a real
/// hit-tested pointer, through the wrapper's own handlers, not by setting
/// the art's prop directly. If the pointer state ever stops reaching the
/// art, this is what notices.
///
/// On the vendored dioxus-test harness, which is blitz-dom — the same DOM
/// the REAPER panels render through, so a control that works here works
/// there.
#[tokio::test]
async fn hovering_the_wrapper_redraws_the_button() -> dioxus_test::Result<()> {
    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([track("T1", false)]);
            provide_context(store);
        });
        rsx! { MuteButton { track: "T1" } }
    }

    let tester = dioxus_test::render(app).build();
    let button = tester.query("div").immediately()?;
    let (ox, oy) = button.document_origin();
    let (w, h) = button.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let resting = tester.root().inner_html();

    // Off the button first: blitz only fires `mouseenter` when the hover
    // chain actually changes, and the pointer starts nowhere in particular.
    tester.pointer_move(x, y, false);
    let _ = tester.pump().await;
    let hovered = tester.root().inner_html();
    assert_ne!(resting, hovered, "hover did not reach the art");

    tester.pointer_down(x, y);
    let _ = tester.pump().await;
    let pressed = tester.root().inner_html();
    assert_ne!(hovered, pressed, "press draws the same as hover");
    assert_ne!(resting, pressed, "press draws the same as resting");

    Ok(())
}

/// Diff whatever the last interaction dirtied. Not a second
/// `rebuild_in_place`: that re-runs the tree from the root, which would
/// hide a signal that never actually propagated.
fn settle(dom: &mut VirtualDom) {
    dom.render_immediate(&mut dioxus::core::NoOpMutations);
}

/// Hover and press are *props* on the art, not CSS states, because the
/// exporter needs them as three separate drawings — and because `:hover` is
/// inert in the parsers the non-browser targets use. These are the three
/// cells REAPER blits, so all three must differ.
#[test]
fn the_three_pointer_states_are_three_drawings() {
    use daw_theme_art::vector_controls as art;

    let draw = |at: art::Interaction| {
        let mut dom = VirtualDom::new_with_props(
            |at: art::Interaction| {
                let named = daw_theme_art::dress::mute_art(Panel::Mixer, false);
                rsx! {
                    art::MuteButton { at, ..daw_theme_art::dress::mute(named, false) }
                }
            },
            at,
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    };

    let normal = draw(art::Interaction::Normal);
    let hover = draw(art::Interaction::Hover);
    let pressed = draw(art::Interaction::Pressed);
    assert_ne!(normal, hover, "hover does not change the markup");
    assert_ne!(normal, pressed, "pressed matches normal");
    assert_ne!(hover, pressed, "pressed matches hover");
}
