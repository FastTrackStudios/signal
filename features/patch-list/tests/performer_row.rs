//! What the performer row's two controls do to a cue mix.

use patch_list::performer_row::{MAX_GAIN, Row, Send, ratio_to, scaled};

fn sends(gains: &[f64]) -> Vec<Send> {
    gains.iter().map(|gain| Send { gain: *gain }).collect()
}

/// **The rule "more of me" exists for.** A performer's own sends move
/// together and *relatively*, so the balance the engineer set between
/// their mics survives being asked for more of all of them.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn more_of_me_keeps_the_balance_between_a_performers_mics() {
    let before = sends(&[0.8, 0.4, 0.2]);
    let after = scaled(&before, 1.5);
    // The ratios between them are what the engineer set; the control
    // asked for more, not for different.
    for (a, b) in before.windows(2).zip(after.windows(2)) {
        let was = a[0].gain / a[1].gain;
        let now = b[0].gain / b[1].gain;
        assert!((was - now).abs() < 1e-9, "the balance moved: {was} → {now}");
    }
    assert!(after[0].gain > before[0].gain, "nothing got louder");
}

/// A ratio rather than an offset, and the difference is visible: adding
/// a fixed amount would close the gap between a loud mic and a quiet
/// one and flatten the mix.
#[test]
fn a_ratio_is_not_an_offset() {
    let before = sends(&[0.8, 0.1]);
    let after = scaled(&before, 2.0);
    let gap_before = before[0].gain - before[1].gain;
    let gap_after = after[0].gain - after[1].gain;
    assert!(
        gap_after > gap_before,
        "a ratio widens the gap; an offset would have kept it"
    );
}

/// **A cue mix lives on somebody's head.** A runaway gain there is not
/// a bad mix, it is an injury, so the ceiling holds however hard the
/// control is pushed.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn the_ceiling_holds() {
    let after = scaled(&sends(&[1.0, 2.0]), 100.0);
    assert!(
        after.iter().all(|s| s.gain <= MAX_GAIN),
        "a cue send ran past the ceiling: {after:?}"
    );
}

/// Asking for a level asks by the **loudest** member, so one quiet send
/// cannot let the rest run past the ceiling on the way to an average.
#[test]
fn a_level_is_asked_for_by_the_loudest_send() {
    let group = sends(&[1.0, 0.01]);
    let by = ratio_to(&group, 2.0);
    let after = scaled(&group, by);
    assert!((after[0].gain - 2.0).abs() < 1e-9, "{after:?}");
    assert!(after[1].gain < 0.1, "the quiet one was dragged up with it");
}

/// Silence stays silent: a group with nothing in it is not turned up
/// from nowhere.
#[test]
fn a_silent_group_is_left_silent() {
    assert!((ratio_to(&sends(&[0.0, 0.0]), 2.0) - 1.0).abs() < 1e-9);
}

/// The row reads its levels from the sends and keeps nothing: a level
/// stored on the row would be a second opinion about a number REAPER
/// already has.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn the_row_reads_its_levels_and_stores_none() {
    let row = Row {
        performer: "Cody".to_owned(),
        bus: Some("HP Cody".to_owned()),
        me: sends(&[0.8, 0.3]),
        band: sends(&[0.5]),
    };
    assert!((row.me_level() - 0.8).abs() < 1e-9);
    assert!((row.band_level() - 0.5).abs() < 1e-9);
    assert!(row.is_listening());
}

/// A performer with no bus can hear nothing, and the row says so rather
/// than drawing controls that would write nowhere.
#[test]
fn a_performer_with_no_bus_is_not_listening() {
    let row = Row {
        performer: "Nobody".to_owned(),
        bus: None,
        me: Vec::new(),
        band: Vec::new(),
    };
    assert!(!row.is_listening());
}
