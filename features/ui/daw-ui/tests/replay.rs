//! A processor graph animating with no processor behind it.
//!
//! The claim the `trace` module makes is that a graph fed by a recording
//! and a graph fed by live DSP are the same graph. That is only worth
//! anything if it holds at the rendered output, so these tests compare
//! the HTML: same frame, same picture, whichever side it came from.
//!
//! Why it matters: de-clip, de-click and tuning are renders written back
//! to the item, frozen tracks have no plugin instance left, and a session
//! of two thousand tracks cannot run its processing at all. In every one
//! of those cases there is no DSP to meter and still something to show.

use daw_ui::widgets::deesser_graph::{DeEsserGraph, DeEsserMetering, DeEsserParams};
use daw_ui::widgets::trace::{Metering, Trace};
use dioxus::prelude::*;
use std::sync::Arc;

mod support;
use support::svg_rects;

/// A de-essing pass: gain reduction deepening frame by frame, the way a
/// sibilant runs through one.
fn sibilant() -> Arc<Trace<DeEsserMetering>> {
    let mut trace = Trace::at_rate(10.0);
    for i in 0..10 {
        let gr = -(i as f32) * 2.0;
        trace.push(DeEsserMetering {
            input_level: -12.0,
            sibilance_level: -18.0 + gr,
            gain_reduction: gr,
        });
    }
    Arc::new(trace)
}

fn render(source: Metering<DeEsserMetering>, live: DeEsserMetering) -> String {
    struct Case(Metering<DeEsserMetering>, DeEsserMetering);
    thread_local! {
        static CASE: std::cell::RefCell<Option<Case>> = const { std::cell::RefCell::new(None) };
    }
    CASE.with(|c| *c.borrow_mut() = Some(Case(source, live)));

    fn app() -> Element {
        let params = use_signal(DeEsserParams::default);
        let (source, live) = CASE.with(|c| {
            let case = c.borrow_mut().take().expect("case");
            (case.0, case.1)
        });
        rsx! { DeEsserGraph { params, metering: live, source, interactive: false } }
    }

    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// The whole point. Frame 4 of the trace is 8 dB of reduction; a live
/// processor reporting 8 dB draws exactly the same thing.
#[test]
fn a_recording_and_a_running_processor_draw_the_same_graph() {
    let frame = DeEsserMetering {
        input_level: -12.0,
        sibilance_level: -26.0,
        gain_reduction: -8.0,
    };
    let live = render(Metering::Live(frame.clone()), DeEsserMetering::default());
    let replay = render(
        Metering::Replay {
            trace: sibilant(),
            at: 0.4,
        },
        DeEsserMetering::default(),
    );

    assert_eq!(live, replay);
}

/// And it moves. Seeking the trace changes the gain-reduction meter with
/// nothing running — which is what a frozen track needs.
#[test]
fn seeking_the_trace_animates_the_meter() {
    let trace = sibilant();
    let heights: Vec<f32> = [0.0, 0.4, 0.9]
        .iter()
        .map(|at| {
            let html = render(
                Metering::Replay {
                    trace: Arc::clone(&trace),
                    at: *at,
                },
                DeEsserMetering::default(),
            );
            // The GR meter is the last pair of rects: background, then fill.
            svg_rects(&html)
                .last()
                .and_then(|r| r.height)
                .expect("a gain-reduction fill")
        })
        .collect();

    assert!(
        heights[0] < heights[1] && heights[1] < heights[2],
        "the meter should deepen as the trace plays: {heights:?}"
    );
}

/// With nothing recorded and nothing running, the live prop is still the
/// path — so every existing caller keeps working untouched.
#[test]
fn idle_falls_back_to_the_live_prop() {
    let live = DeEsserMetering {
        input_level: -12.0,
        sibilance_level: -26.0,
        gain_reduction: -8.0,
    };
    assert_eq!(
        render(Metering::Idle, live.clone()),
        render(Metering::Live(live), DeEsserMetering::default()),
    );
}
