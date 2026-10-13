//! The transport readouts, rendered.
//!
//! The arithmetic (which dots are lit, how the pulse decays, how a time
//! reads) lives in `session_ui::components::transport_stats` and is
//! exercised here too — the crate's lib is `test = false`, so its own
//! `#[test]`s never run and this file is where they land.
//!
//! What only a render can be wrong about is the rest: that the bar
//! number on screen is the bar the transport is in, that the meter is
//! the song's and not four-four, and that the loop mark appears only
//! when there is a loop.

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use session_ui::components::{clock, dots, pulse, MOST_DOTS};

#[component]
fn Harness() -> Element {
    rsx! {
        session_ui::components::TransportStats {
            musical: Some(daw_proto::MusicalPosition {
                measure: 42,
                beat: 3,
                subdivision: 0,
            }),
            seconds: 64.25,
            bpm: 138.0,
            beats_per_bar: 7,
            beat_unit: 8,
            looping: true,
        }
    }
}

/// A song in seven eight reads as seven eight, with the bar it is
/// actually in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_readouts_are_the_songs_own() {
    let tester = render(Harness).build();

    let measure = tester
        .query(by_testid("stat-measure"))
        .await
        .expect("the bar number");
    assert_eq!(measure.inner_html().trim(), "42");

    let bpm = tester
        .query(by_testid("stat-bpm"))
        .await
        .expect("the tempo");
    assert_eq!(bpm.inner_html().trim(), "138");

    let meter = tester
        .query(by_testid("stat-meter"))
        .await
        .expect("the meter");
    let meter = meter.inner_html();
    assert!(meter.contains('7') && meter.contains('8'), "{meter}");

    let clock_cell = tester
        .query(by_testid("stat-clock"))
        .await
        .expect("the clock");
    assert!(clock_cell.inner_html().contains("1:04.250"));

    tester
        .query(by_testid("stat-loop"))
        .await
        .expect("a looping transport says so");
}

/// The bar fills as it is played, and stops at the end of the bar.
#[test]
fn the_dots_fill_up_to_the_beat() {
    assert_eq!(dots(4, 1), vec![true, false, false, false]);
    assert_eq!(dots(4, 3), vec![true, true, true, false]);
    assert_eq!(dots(4, 4), vec![true, true, true, true]);
    assert_eq!(dots(4, 9), vec![true, true, true, true], "it overflowed");
    assert_eq!(dots(4, 0), vec![true, false, false, false]);
}

/// A meter is whatever the song is in, and a nonsense one still draws
/// something countable.
#[test]
fn any_meter_draws_a_countable_row() {
    assert_eq!(dots(7, 5).len(), 7);
    assert_eq!(dots(1, 1), vec![true]);
    assert_eq!(dots(0, 1).len(), 1);
    assert_eq!(dots(-4, 1).len(), 4, "a negative meter did not fall back");
    assert_eq!(dots(200, 3).len(), MOST_DOTS);
}

/// The pulse is brightest on the beat and never goes out — a dot that
/// vanished would read as something broken.
#[test]
fn the_pulse_decays_across_a_beat_without_vanishing() {
    assert!((pulse(0) - 1.0).abs() < 1e-9);
    assert!(pulse(999) > 0.3, "the dot went dark: {}", pulse(999));
    assert!(pulse(500) < pulse(0));
    assert!((pulse(-50) - 1.0).abs() < 1e-9);
    assert!((pulse(5000) - pulse(999)).abs() < 1e-9);
}

#[test]
fn the_clock_reads_like_a_stopwatch() {
    assert_eq!(clock(0.0), "0:00.000");
    assert_eq!(clock(64.25), "1:04.250");
    assert_eq!(clock(-3.0), "0:00.000");
}
