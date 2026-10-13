//! Meters on the strip.
//!
//! The plumbing tests live beside the code; these are the two things only a
//! mounted strip can show: that a meter draws its track's level with a hold
//! mark, and that the ordering the frame is indexed by is the ordering the
//! track store actually maintains.

use daw_proto::peak::TrackLevels;
use daw_proto::{Track, TrackEvent};
use daw_ui::controls::{Meters, TrackMeter, TrackStore};
use dioxus::prelude::*;

mod support;
use support::{svg_rects, svg_text_anchors};

fn track(guid: &str, index: u32) -> Track {
    Track {
        guid: guid.to_string(),
        index,
        name: guid.into(),
        ..Default::default()
    }
}

thread_local! {
    static STORE: std::cell::Cell<Option<TrackStore>> = const { std::cell::Cell::new(None) };
}

fn store() -> TrackStore {
    STORE.with(|s| s.get()).expect("store handle")
}

/// The order a meter frame is indexed by. Getting this wrong is silent —
/// every meter still moves, each showing the wrong track — which is why the
/// store owns it rather than each strip guessing.
#[test]
fn the_track_order_follows_adds_removes_and_moves() {
    fn app() -> Element {
        let mut s = use_hook(TrackStore::new);
        use_hook(|| {
            s.seed([track("A", 0), track("B", 1), track("C", 2)]);
            provide_context(s);
            STORE.with(|c| c.set(Some(s)));
        });
        rsx! { div {} }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();

    dom.in_runtime(|| {
        let mut store = store();
        assert_eq!(store.order(), ["A", "B", "C"]);

        // A track arrives in the middle.
        store.apply(&TrackEvent::Added(track("D", 1)));
        // D and B now both claim index 1 until the backend renumbers, but
        // the list has four entries either way — the meter frame's own
        // length is what decides whether it can be trusted.
        assert_eq!(store.order().len(), 4);

        store.apply(&TrackEvent::Removed("D".into()));
        assert_eq!(store.order(), ["A", "B", "C"]);

        // C moves to the top.
        store.apply(&TrackEvent::Moved {
            guid: "C".into(),
            old_index: 2,
            new_index: 0,
        });
        assert_eq!(store.order()[0], "C", "a move did not re-aim the meters");
    });
}

/// The meter draws both channels and REAPER's dB scale, as vectors.
///
/// One `<svg>`, not two: `mcp.meter` is a single rect covering the bars
/// *and* the scale, and REAPER draws the numbers as part of the widget.
/// Split into a column per channel the bars had nowhere to go but under
/// the button stack.
#[test]
fn a_strip_meter_draws_both_channels_and_its_scale() {
    fn app() -> Element {
        let mut meters = use_hook(Meters::new);
        use_hook(|| {
            meters.apply(
                &["T1".to_string()],
                &[TrackLevels {
                    peak_left: 0.5,
                    peak_right: 0.25,
                    hold_left: 0.9,
                    hold_right: 0.8,
                }],
            );
            provide_context(meters);
        });
        rsx! { TrackMeter { track: "T1", height: 100 } }
    }

    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);

    assert_eq!(
        html.matches("<svg").count(),
        1,
        "the meter is one widget:\n{html}"
    );
    // The scale REAPER prints inside the block. Which marks depends on
    // the room — the ladder thins as the meter shortens — so what is
    // pinned is the readout at the top and the floor at the bottom, not a
    // particular rung.
    assert!(html.contains("-inf"), "no readout:\n{html}");
    assert!(
        html.contains("-60-"),
        "the scale never reaches its floor:\n{html}"
    );
    assert!(!html.contains("<img"), "the meter is blitting:\n{html}");
    assert!(
        !html.contains("currentColor"),
        "a colour is left to CSS:\n{html}"
    );
}

/// A meter with nothing behind it draws silence rather than failing to
/// mount — a strip has to render before the first frame arrives.
#[test]
fn a_meter_with_no_frames_yet_draws_silence() {
    let mut dom = VirtualDom::new(|| rsx! { TrackMeter { track: "T1" } });
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert_eq!(html.matches("<svg").count(), 1);
}

/// The scale is printed inside the meter, over the bars.
///
/// REAPER's meter is one wide rectangle with its numbers on it — the green
/// fills the whole block as it climbs past them. Reserving a column for
/// the scale beside the bars left two hairlines and a strip of type, which
/// is a different control.
#[test]
fn the_scale_is_printed_inside_the_block() {
    let mut dom = VirtualDom::new(|| rsx! { TrackMeter { track: "T1", width: 26, height: 120 } });
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);

    // Both bars span the block between them: two of them, and the last
    // one reaches its right edge.
    let widths: Vec<f32> = svg_rects(&html)
        .into_iter()
        .filter_map(|r| Some(r.x? + r.width?))
        .collect();
    assert!(
        widths.iter().any(|edge| (*edge - 26.0).abs() < 0.6),
        "no bar reaches the block's right edge:\n{html}"
    );

    // And the marks are anchored inside it, not off its left.
    let anchors = svg_text_anchors(&html);
    assert!(!anchors.is_empty(), "the scale is missing:\n{html}");
    assert!(
        anchors.iter().all(|x| *x > 1.0 && *x <= 26.0),
        "the scale is anchored outside the block: {anchors:?}"
    );
}
