//! Bars as a song counts them, for everything that says or takes a bar:
//! the bar its `SONGSTART` falls in is bar 1, the count-in before it
//! bars −1, −2 …, with no bar 0 — as the ruler and the transport number
//! them ([`crate::ruler::from_song`]). The engine counts measures from
//! the project's start; these convert both ways.

use daw::service::{Markers, ProjectContext, TempoMap};

/// The engine's measure the song's bar 1 is.
fn bar_one<D: Markers + TempoMap>(daw: &D, project: &ProjectContext) -> i32 {
    Markers::all(daw, project.clone())
        .into_iter()
        .find(|m| m.name == "SONGSTART")
        .and_then(|m| m.position.seconds())
        .map_or(1, |at| daw.time_to_musical(project.clone(), at + 1e-6).0)
}

/// `at` seconds as the song's bar, beat and fraction of a beat.
#[must_use]
pub fn musical<D: Markers + TempoMap>(
    daw: &D,
    project: &ProjectContext,
    at: f64,
) -> (i32, i32, f64) {
    let (measure, beat, fraction) = daw.time_to_musical(project.clone(), at);
    (to_song(measure, bar_one(daw, project)), beat, fraction)
}

/// The engine's measure the song's `bar` is.
#[must_use]
pub fn measure<D: Markers + TempoMap>(daw: &D, project: &ProjectContext, bar: i32) -> i32 {
    to_engine(bar, bar_one(daw, project))
}

/// The seconds of the song's `bar`, `beat` (from 1) and `fraction`.
#[must_use]
pub fn time<D: Markers + TempoMap>(
    daw: &D,
    project: &ProjectContext,
    bar: i32,
    beat: i32,
    fraction: f64,
) -> f64 {
    let measure = to_engine(bar, bar_one(daw, project));
    daw.musical_to_time(project.clone(), measure, beat, fraction)
}

/// An engine measure as a song bar, `bar_one` the measure of bar 1.
#[must_use]
pub const fn to_song(measure: i32, bar_one: i32) -> i32 {
    let shown = measure.saturating_sub(bar_one).saturating_add(1);
    if shown <= 0 {
        shown.saturating_sub(1)
    } else {
        shown
    }
}

/// A song bar as an engine measure (bar 0, which no song has, is bar 1).
#[must_use]
pub const fn to_engine(bar: i32, bar_one: i32) -> i32 {
    if bar < 0 {
        bar.saturating_add(bar_one)
    } else {
        let bar = if bar < 1 { 1 } else { bar };
        bar.saturating_add(bar_one).saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bar 1 is the song's start, the bars before it count down, and the
    /// two ways round agree.
    #[test]
    fn bars_count_from_the_song() {
        assert_eq!(to_song(2, 2), 1);
        assert_eq!(to_song(5, 2), 4);
        assert_eq!(to_song(1, 2), -1);
        assert_eq!(to_song(1, 1), 1, "a song starting at the top");
        for measure in 1..20 {
            assert_eq!(to_engine(to_song(measure, 3), 3), measure);
        }
        assert_eq!(
            crate::ruler::from_song(1, 3),
            i64::from(to_song(1, 3)),
            "the ruler counts the same"
        );
    }
}
