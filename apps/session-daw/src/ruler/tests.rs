use super::{Bars, Timeline, bars_between_numbers, subdivisions};
use daw_ui::studio::project::TempoChange;

fn at(at: f64, bpm: f64, per_bar: u32) -> TempoChange {
    TempoChange {
        at,
        bpm,
        beats_per_bar: per_bar,
        beat_unit: 4,
    }
}

fn view() -> crate::arrangement::Viewport {
    crate::arrangement::Viewport {
        scroll_x: 0.0,
        scroll_y: 0.0,
        pps: 20.0,
        zoom_y: 1.0,
        width: 1000.0,
        height: 600.0,
        panel_w: crate::arrangement::TCP_WIDTH,
    }
}

/// The name field opens on the mark, in the mark's own lane.
#[test]
fn a_name_opens_where_the_mark_is() {
    let field = super::field(view(), (0.0, 100.0), super::SECTIONS_ROW, 10.0);
    let expected = 10.0f64.mul_add(20.0, crate::arrangement::TCP_WIDTH);
    assert!((field.x0 - expected).abs() < 1e-9, "at {}", field.x0);
    let top = 100.0 + super::LANE_H;
    assert!(field.y0 >= top && field.y1 <= top + super::LANE_H);
}

/// A band that starts off the left of the view is still renamed
/// somewhere you can see, and one near the right edge does not open
/// its field off the end of the window.
#[test]
fn a_name_field_stays_on_screen() {
    let view = view();
    let left = crate::arrangement::TCP_WIDTH;
    let offscreen = super::field(view, (0.0, 0.0), 0, -30.0);
    assert!((offscreen.x0 - left).abs() < 1e-9, "at {}", offscreen.x0);
    let far = super::field(view, (0.0, 0.0), 0, 480.0);
    assert!(far.x1 <= view.width + 1e-9, "ran to {}", far.x1);
}

/// The ruler counts MEASURES, not seconds.
///
/// Pinned because it is the question you cannot answer by looking:
/// at 120 bpm in four four a bar is two seconds, so a ruler
/// numbering seconds and one numbering bars both count 1, 2, 3.
#[test]
fn the_numbers_are_bars() {
    let beats = Timeline::new(&[at(0.0, 120.0, 4)]).beats(8.0, 100);
    let downbeats: Vec<&super::Beat> = beats.iter().filter(|b| b.is_downbeat()).collect();
    assert!(
        (downbeats[1].at - 2.0).abs() < 1e-9,
        "bar 2 is at two seconds"
    );
    assert_eq!(downbeats[1].measure, 2);
    // The tempo moves them, which a seconds ruler would not notice.
    let slow = Timeline::new(&[at(0.0, 60.0, 4)]).beats(8.0, 100);
    let bar2 = slow.iter().find(|b| b.measure == 2 && b.is_downbeat());
    assert!(
        (bar2.unwrap().at - 4.0).abs() < 1e-9,
        "at 60 a bar is four seconds"
    );
}

/// The signature decides how many beats make a bar.
#[test]
fn the_signature_decides_the_bar() {
    let beats = Timeline::new(&[at(0.0, 120.0, 3)]).beats(6.0, 100);
    let bar2 = beats.iter().find(|b| b.measure == 2 && b.is_downbeat());
    assert!(
        (bar2.unwrap().at - 1.5).abs() < 1e-9,
        "three beats at 120 is a bar and a half second"
    );
    assert_eq!(beats[2].beat, 3, "a three-four bar has a third beat");
    assert_eq!(beats[3].measure, 2, "and no fourth");
}

/// A tempo change moves every bar line AFTER it.
///
/// The thing a multiplied grid gets wrong. Four bars at 120 take
/// eight seconds; halve the tempo at eight and the next bar takes
/// four, not two.
#[test]
fn a_tempo_change_moves_the_bars_after_it() {
    let map = [at(0.0, 120.0, 4), at(8.0, 60.0, 4)];
    let beats = Timeline::new(&map).beats(20.0, 200);
    let downs: Vec<f64> = beats
        .iter()
        .filter(|b| b.is_downbeat())
        .map(|b| b.at)
        .collect();
    assert!((downs[0] - 0.0).abs() < 1e-9);
    assert!((downs[1] - 2.0).abs() < 1e-9);
    assert!(
        (downs[4] - 8.0).abs() < 1e-9,
        "the change lands on a bar line"
    );
    assert!(
        (downs[5] - 12.0).abs() < 1e-9,
        "after the change a bar takes four seconds, got {}",
        downs[5]
    );
}

/// A signature change starts a new measure.
///
/// A bar of four and a bar of three cannot share a bar line, and a
/// measure that is part one signature and part another is not a
/// measure.
#[test]
fn a_signature_change_starts_a_measure() {
    // Change mid-bar, two beats into the second bar.
    let map = [at(0.0, 120.0, 4), at(3.0, 120.0, 3)];
    let beats = Timeline::new(&map).beats(9.0, 200);
    let change = beats
        .iter()
        .find(|b| (b.at - 3.0).abs() < 1e-9)
        .expect("a beat at the change");
    assert_eq!(change.beat, 1, "the new signature starts on beat one");
    assert_eq!(change.per_bar, 3, "and counts in three from there");
    assert_eq!(change.measure, 3, "in a new measure, not the middle of one");
}

/// Zooming in numbers more bars, then ticks the beats finer.
#[test]
fn zooming_in_numbers_more_and_ticks_finer() {
    assert_eq!(bars_between_numbers(400.0), 1, "every bar, wide");
    assert!(bars_between_numbers(10.0) >= 4, "fewer, narrow");
    assert!(bars_between_numbers(400.0) <= bars_between_numbers(100.0));
    assert_eq!(subdivisions(100.0), Some(4), "sixteenths with room");
    assert_eq!(subdivisions(20.0), Some(2), "eighths, closer");
    assert_eq!(subdivisions(8.0), None, "none when they would crowd");
}

/// Bars counted from the song: the bar it starts in is 1, the two before
/// it −1 and −2, and there is no bar 0.
#[test]
fn bars_are_counted_from_the_song() {
    assert_eq!(super::bars::from_song(3, 3), 1);
    assert_eq!(super::bars::from_song(4, 3), 2);
    assert_eq!(super::bars::from_song(2, 3), -1);
    assert_eq!(super::bars::from_song(1, 3), -2);
}

/// A sane fallback when the project has no tempo at all.
#[test]
fn no_tempo_map_counts_nothing_rather_than_forever() {
    assert!(Timeline::new(&[]).beats(60.0, 100).is_empty());
    // And a nonsense tempo cannot spin the walk.
    let mad = [at(0.0, 0.0, 4)];
    let beats = Timeline::new(&mad).beats(60.0, 100);
    assert!(
        !beats.is_empty(),
        "a zero tempo should fall back, not stall"
    );
    assert!(beats.len() <= 100, "the limit holds");
}

/// `Bars` still answers for a single tempo, which the grid uses.
#[test]
fn bars_reads_the_map_at_a_time() {
    let map = [at(0.0, 120.0, 4), at(10.0, 60.0, 3)];
    assert!((Bars::at_time(&map, 0.0).secs_per_bar() - 2.0).abs() < 1e-9);
    assert!((Bars::at_time(&map, 9.9).secs_per_bar() - 2.0).abs() < 1e-9);
    assert!((Bars::at_time(&map, 10.0).secs_per_bar() - 3.0).abs() < 1e-9);
}
