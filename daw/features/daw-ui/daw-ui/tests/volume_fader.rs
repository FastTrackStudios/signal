//! The fader is correct at any height, and follows the finger.
//!
//! The two halves of #141. The rail must decompose into fixed caps and a
//! stretchy run rather than scaling whole, and the value under the pointer
//! must be the UI's rather than the engine's.

use daw_proto::{Track, TrackEvent};
use daw_ui::controls::{TrackStore, VolumeFader};
use dioxus::prelude::*;

mod support;
use support::svg_rects;

fn track(guid: &str, volume: f64) -> Track {
    Track {
        guid: guid.to_string(),
        name: "Kick".into(),
        volume,
        ..Default::default()
    }
}

thread_local! {
    static STORE: std::cell::Cell<Option<TrackStore>> = const { std::cell::Cell::new(None) };
}

fn app() -> Element {
    let mut store = use_hook(TrackStore::new);
    use_hook(|| {
        store.seed([track("T1", 0.5)]);
        provide_context(store);
        STORE.with(|s| s.set(Some(store)));
    });
    rsx! { VolumeFader { track: "T1" } }
}

fn store() -> TrackStore {
    STORE.with(|s| s.get()).expect("store handle")
}

fn settle(dom: &mut VirtualDom) {
    dom.render_immediate(&mut dioxus::core::NoOpMutations);
}

/// The rail is three bands, not one scaled drawing: the caps hold their
/// source height and only the run between them takes the slack. Drawn as
/// one `<svg>` the groove would stretch into the caps, which is the bug the
/// magenta guides exist to prevent.
#[test]
fn the_rail_is_a_stack_of_bands_and_only_the_middle_grows() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);

    // Three rail bands plus the cap.
    assert_eq!(
        html.matches("<svg").count(),
        4,
        "not one svg per band:\n{html}"
    );
    // Each band draws its own slice of the groove in its own coordinates,
    // rather than windowing one drawing through a viewBox offset: Blitz
    // ignores a viewBox's min-y and clips nothing to it, so windowing made
    // all three bands paint the whole groove and a tall fader came out as
    // three disconnected dashes.
    assert_eq!(
        html.matches(r#"viewBox="0 0 23 16""#).count(),
        2,
        "the caps are not drawn at their own origin:\n{html}"
    );
    assert!(
        html.contains(r#"viewBox="0 0 23 23""#),
        "no stretch band:\n{html}"
    );

    // And the slices reassemble the traced groove: 2 rows in the top cap,
    // 23 in the run, 2 in the bottom — the 27 rows `mcp_volbg` traces.
    let rows: f32 = svg_rects(&html)
        .into_iter()
        .filter(|r| r.width == Some(1.4))
        .filter_map(|r| r.height)
        .sum();
    assert_eq!(rows, 27.0, "the groove's slices do not add up:\n{html}");
    assert!(html.contains("flex:1"), "nothing takes the slack:\n{html}");
    // A stretched band must not letterbox — the rail is meant to lengthen.
    assert!(
        html.contains(r#"preserveAspectRatio="none""#),
        "the stretch band would letterbox:\n{html}"
    );
    // Still a drawing, not a picture of one.
    assert!(!html.contains("<img"), "the fader is blitting:\n{html}");
    assert!(
        !html.contains("url(data:"),
        "the fader is blitting:\n{html}"
    );
    assert!(
        !html.contains("currentColor"),
        "a colour is left to CSS:\n{html}"
    );
}

/// At the source box the decomposition is the identity: three bands of
/// 16 + 23 + 16 cover exactly the 55 rows one band used to.
#[test]
fn the_bands_add_back_up_to_the_source_box() {
    let rail = daw_theme_art::slice::expect_art("mcp_volbg");
    let panes = rail.stack();
    let total: f32 = panes.iter().map(|p| p.view.3).sum();
    assert_eq!(total, rail.source.1);
    assert_eq!(panes.iter().filter(|p| p.grow).count(), 1);
}

/// The whole point of a draft: the cap moves on the frame the pointer
/// moves, with no engine in the loop. There is no DAW connected in this
/// test at all — if the render waited on one, nothing would move.
#[test]
fn dragging_moves_the_fader_before_any_round_trip() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let resting = dioxus_ssr::render(&dom);

    dom.in_runtime(|| store().drafts().set_volume("T1", 0.9));
    settle(&mut dom);
    let dragged = dioxus_ssr::render(&dom);

    assert_ne!(resting, dragged, "the cap did not move with the draft");
    // The cap rides higher: its offset from the top is smaller.
    //
    // 0.3104, not 0.1: the draft is a *gain* and the rail's top is +12 dB,
    // so 0.9 gain sits at (0.9/3.981)^(1/4) = 0.6896 of the travel. A fader
    // that read the gain as its position pinned unity to the very top.
    assert!(
        dragged.contains("* 0.310"),
        "the cap is not at the taper's position for 0.9 gain:\n{dragged}"
    );
}

/// The backend echoes every write back, including your own. Applying that
/// mid-drag drags the cap backwards under the pointer.
#[test]
fn an_echo_is_ignored_during_the_drag_and_obeyed_after_it() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();

    dom.in_runtime(|| {
        let mut store = store();
        let mut drafts = store.drafts();
        drafts.set_volume("T1", 0.9);

        // The engine, still catching up, reports where it had got to.
        store.apply(&TrackEvent::VolumeChanged {
            guid: "T1".into(),
            volume: 0.5,
        });
        assert_eq!(store.volume("T1"), 0.9, "the echo fought the finger");

        // Drag ends; the sync loop writes the last value, then retires.
        drafts.release_volume("T1");
        drafts.take_dirty();
        drafts.retire();

        // Now a change made anywhere else reaches the fader normally.
        store.apply(&TrackEvent::VolumeChanged {
            guid: "T1".into(),
            volume: 0.25,
        });
        assert_eq!(store.volume("T1"), 0.25, "an idle fader ignored the engine");
    });
}

/// Another track's volume is not suppressed just because this one is held.
#[test]
fn suppression_is_per_track() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();

    dom.in_runtime(|| {
        let mut store = store();
        store.seed([track("T1", 0.5), track("T2", 0.5)]);
        store.drafts().set_volume("T1", 0.9);

        store.apply(&TrackEvent::VolumeChanged {
            guid: "T2".into(),
            volume: 0.2,
        });
        assert_eq!(store.volume("T2"), 0.2, "an untouched track was suppressed");
        assert_eq!(store.volume("T1"), 0.9);
    });
}
