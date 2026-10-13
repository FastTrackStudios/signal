//! Stamp the Keyflow folder's MIDI from the song's chart.
//!
//! The counterpart to [`crate::guide`], and built the same way: resolve
//! the song, turn it into a list of notes, clear what was there, write
//! what is there now. The guide does it for the click and the cues;
//! this does it for the key and the chords.
//!
//! The scaffold's own header has promised "chords as items, and melody
//! MIDI" as a later phase since it was written. This is that phase for
//! the two tracks that have a source: LINES and HITS are created and
//! left empty, because nothing in the chart says what belongs in them.
//!
//! **Where the content comes from.** The chart — `song.parsed_chart` —
//! which carries the initial key, every key change, and each section's
//! measures of chords. Nothing here invents music: a project with no
//! chart generates nothing, the same way the guide refuses a song with
//! no SONGSTART.

use keyflow::Chart;
use keyflow::chart::ChartSection;
use keyflow::key::Key;

/// Where a key holds, in measures from the start of the chart.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeySpan {
    /// How the key is written, which is what the item is named.
    pub name: String,
    /// First measure it covers.
    pub from_measure: usize,
    /// One past the last. A span that never ends runs to the chart's.
    pub to_measure: usize,
}

/// A chord, placed and voiced.
#[derive(Clone, PartialEq, Debug)]
pub struct Voicing {
    /// Measure it starts in, from the start of the chart.
    pub measure: usize,
    /// How far into that measure, in beats.
    pub beat: f64,
    /// How long, in beats — up to the next chord.
    ///
    /// A chord holds until another one replaces it. That is what a
    /// chart means by writing one: the harmony is sounding until it
    /// says otherwise, and a chord that stopped early would leave a
    /// hole where the music has none — which the analyser reading this
    /// track back would then report as a gap in the progression.
    pub beats: f64,
    /// The pitches to sound, lowest first.
    pub pitches: Vec<u8>,
    /// The symbol as written, for the item's name and for reading the
    /// generated track back.
    pub symbol: String,
}

/// Where each key holds, across the whole chart.
///
/// The initial key runs from the first measure; each change ends the
/// one before it.
///
/// A chart that names no key still HAS one: keyflow's parser supplies a
/// default, and that default is the chart's answer rather than this
/// function's. Writing it out is therefore reporting what the chart
/// says, not guessing — and a key track that silently disagreed with
/// the chart everything else is read from would be worse than either.
#[must_use]
pub fn key_spans(chart: &Chart) -> Vec<KeySpan> {
    let total = measures_in(chart);
    let mut spans: Vec<KeySpan> = Vec::new();
    if let Some(initial) = chart.initial_key.as_ref() {
        spans.push(KeySpan {
            name: initial.to_string(),
            from_measure: 0,
            to_measure: total,
        });
    }
    for change in &chart.key_changes {
        let at = change_measure(change);
        if let Some(last) = spans.last_mut()
            && last.from_measure < at
        {
            last.to_measure = at;
        }
        spans.push(KeySpan {
            name: change_key(change).map_or_else(String::new, |key| key.to_string()),
            from_measure: at,
            to_measure: total,
        });
    }
    // A span of no length is a key that was replaced before it sounded.
    spans.retain(|span| span.to_measure > span.from_measure && !span.name.is_empty());
    spans
}

/// Every chord in the chart, voiced as pitches.
///
/// `octave` is where the voicing sits; the chord track is a reference
/// for reading and for the analyser, not a part, so it wants to be out
/// of the way of anything played.
///
/// A chord whose root cannot be resolved is skipped rather than
/// guessed at. Roman numerals and scale degrees need a key, and a chart
/// that uses them without naming one has not said enough yet.
#[must_use]
pub fn voicings(chart: &Chart, octave: i32) -> Vec<Voicing> {
    let mut out: Vec<Voicing> = Vec::new();
    let mut measure = 0usize;
    let mut key = chart.initial_key.clone();
    for section in &chart.sections {
        // The chord track's bars — the same bars, counted the same way, as
        // the layout places (`chart_to_layout`'s `measure_starts`).
        {
            for bar in section.measures() {
                // Chords divide the bar by their written durations, and
                // a bar whose durations say nothing divides evenly —
                // four chords in a bar of four is one a beat, which is
                // what a chart means when it writes them side by side.
                // The bar's own meter: a bar of 2/4 holds two.
                let beats_per_bar = crate::setlist::chart_import::quarters_in(bar.time_signature);
                let written: f64 = bar.chords.iter().map(|c| beats_of(c, chart)).sum();
                let even = beats_per_bar / bar.chords.len().max(1) as f64;
                let mut beat = 0.0;
                let Some(last) = bar.chords.last() else {
                    measure = measure.saturating_add(1);
                    continue;
                };
                for chord in &bar.chords {
                    let beats = if written > 0.0 {
                        beats_of(chord, chart).max(0.0)
                    } else {
                        even
                    };
                    let beats = if beats > 0.0 { beats } else { even };
                    // The last chord in a bar holds to the bar line
                    // rather than to its written length: what a chart
                    // writes is where a chord STARTS, and the harmony
                    // sounds until the next one. Not past it, either — a
                    // lone chord is written a whole note, which in a bar
                    // of 2/4 would run into the next bar's chord.
                    let room = beats_per_bar - beat;
                    let beats = if std::ptr::eq(chord, last) && room > 0.0 {
                        room
                    } else {
                        beats
                    };
                    // A rest or a spacer (`r`, `s` — a bar of N.C.) takes
                    // its time and sounds nothing.
                    let silent = matches!(chord.full_symbol.as_str(), "r" | "s");
                    // A push sounds early (`'4`, an eighth before its beat,
                    // across the bar line if it is on the one) and a pull
                    // late; it still ends where it would have, and the
                    // chord before it gives up (or takes) the difference.
                    let shift = chord.push_pull.as_ref().map_or(0.0, |(push, amount)| {
                        if *push {
                            -amount.to_beats()
                        } else {
                            amount.to_beats()
                        }
                    });
                    if !silent && let Some(pitches) = voice(chord, key.as_ref(), octave) {
                        if shift != 0.0
                            && let Some(before) = out.last_mut()
                        {
                            before.beats = (before.beats + shift).max(0.0);
                        }
                        out.push(Voicing {
                            measure,
                            beat: beat + shift,
                            beats: beats - shift,
                            pitches,
                            symbol: chord.full_symbol.clone(),
                        });
                    }
                    beat += beats;
                }
                measure = measure.saturating_add(1);
            }
        }
        // A key change inside the chart moves what a roman numeral
        // means from here on.
        if let Some(changed) = section_key(section) {
            key = Some(changed);
        }
    }
    out
}

/// One chord's pitches: its root, then its own intervals above it.
fn voice(chord: &keyflow::chart::ChordInstance, key: Option<&Key>, octave: i32) -> Option<Vec<u8>> {
    let note = chord.root.resolve(key)?;
    let root = i32::from(note.semitone) + (octave + 1) * 12;
    let pitches: Vec<u8> = chord
        .parsed
        .semitone_sequence()
        .into_iter()
        .filter_map(|step| {
            let pitch = root + i32::from(step);
            (0..=127).contains(&pitch).then_some(pitch as u8)
        })
        .collect();
    (!pitches.is_empty()).then_some(pitches)
}

fn measures_in(chart: &Chart) -> usize {
    chart
        .sections
        .iter()
        .flat_map(|section| section.tracks.iter())
        .map(|track| track.measures.len())
        .sum()
}

fn bar_beats(chart: &Chart) -> u32 {
    chart
        .time_signature
        .as_ref()
        .map_or(4, |sig| sig.numerator.max(1))
}

fn beats_of(chord: &keyflow::chart::ChordInstance, chart: &Chart) -> f64 {
    let per_bar = f64::from(bar_beats(chart));
    let d = &chord.duration;
    // Subdivisions are thousandths of a beat in the DAW's musical
    // position, which is what a chart duration is carried as.
    f64::from(d.measure) * per_bar + f64::from(d.beat) + f64::from(d.subdivision) / 1000.0
}

fn section_key(_section: &ChartSection) -> Option<Key> {
    // Sections do not carry their own key in the chart model; key
    // changes are a chart-level list. Kept as a seam because that is
    // where a per-section key would arrive, and the caller already
    // treats the key as something that can move.
    None
}

fn change_measure(change: &keyflow::chart::types::KeyChange) -> usize {
    // The chart counts a key change by its total duration from the
    // start, so its measure is that duration's measure count.
    usize::try_from(change.position.total_duration.measure.max(0)).unwrap_or(0)
}

fn change_key(change: &keyflow::chart::types::KeyChange) -> Option<&Key> {
    Some(&change.to_key)
}

#[cfg(test)]
mod tests {
    use super::{key_spans, voicings};
    use keyflow::text::chart::parse_chart;

    fn chart(text: &str) -> keyflow::Chart {
        parse_chart(text).expect("the test chart should parse")
    }

    /// A chart says what key it is in, and the key track says it back.
    #[test]
    fn the_initial_key_covers_the_chart() {
        let spans = key_spans(&chart("My Song\n4/4 #C\n\nVS 1: | C | F | G | C |\n"));
        assert_eq!(spans.len(), 1, "one key, one span: {spans:?}");
        assert_eq!(spans[0].from_measure, 0);
        assert!(spans[0].to_measure > 0, "the span has no length");
    }

    /// A chart that names no key still has one, and we write what the
    /// chart says rather than deciding for ourselves.
    ///
    /// This surprised me, which is why it is pinned: keyflow's parser
    /// supplies a default key, so there is no such thing downstream as
    /// a chart without one. The decision was already made upstream, and
    /// a key track that disagreed with the chart everything else is
    /// read from would be worse than either answer.
    #[test]
    fn an_unwritten_key_is_still_the_charts_answer() {
        let written = key_spans(&chart("My Song\n4/4 #C\n\nVS 1: | C | F |\n"));
        let unwritten = key_spans(&chart("VS 1: | C | F |\n"));
        assert_eq!(unwritten.len(), 1, "the parser's default key went missing");
        assert_eq!(
            unwritten[0].name, written[0].name,
            "writing the key changed what it is"
        );
    }

    /// The chords come out as pitches, in the order they were written.
    #[test]
    fn chords_become_pitches() {
        let voiced = voicings(&chart("My Song\n4/4 #C\n\nVS 1: | C | F | G | C |\n"), 3);
        assert_eq!(voiced.len(), 4, "four chords written, got {voiced:?}");
        assert_eq!(voiced[0].symbol, "C");
        assert!(
            voiced[0].pitches.len() >= 3,
            "a triad should have three notes, got {:?}",
            voiced[0].pitches
        );
        // Ascending: the root, then its intervals above it.
        assert!(
            voiced[0].pitches.windows(2).all(|p| p[0] < p[1]),
            "the voicing is not stacked upward: {:?}",
            voiced[0].pitches
        );
        // Each chord lands in its own bar, in order.
        assert!(
            voiced.windows(2).all(|v| v[0].measure < v[1].measure),
            "chords did not advance a bar at a time: {:?}",
            voiced.iter().map(|v| v.measure).collect::<Vec<_>>()
        );
    }

    /// Two chords in a bar split it; one chord fills it.
    ///
    /// This is what a chart MEANS by writing them side by side, and
    /// getting it wrong puts every later chord in the wrong place.
    #[test]
    fn chords_in_one_bar_share_it() {
        let voiced = voicings(&chart("My Song\n4/4 #C\n\nVS 1: | C F | G |\n"), 3);
        let first_bar: Vec<_> = voiced.iter().filter(|v| v.measure == 0).collect();
        assert_eq!(first_bar.len(), 2, "two chords in the bar: {voiced:?}");
        assert!(
            (first_bar[0].beat - 0.0).abs() < 1e-9,
            "the first chord did not start the bar"
        );
        assert!(
            first_bar[1].beat > first_bar[0].beat,
            "the second chord did not come after the first"
        );
        assert!(
            (first_bar[0].beats + first_bar[1].beats - 4.0).abs() < 1e-6,
            "the two of them do not fill the bar: {first_bar:?}"
        );
    }

    /// The octave is where the voicing sits, and moving it moves every
    /// note by the same twelve.
    #[test]
    fn the_octave_moves_the_whole_voicing() {
        let low = voicings(&chart("My Song\n4/4 #C\n\nVS 1: | C |\n"), 3);
        let high = voicings(&chart("My Song\n4/4 #C\n\nVS 1: | C |\n"), 4);
        assert_eq!(low.len(), 1);
        assert_eq!(high.len(), 1);
        for (a, b) in low[0].pitches.iter().zip(&high[0].pitches) {
            assert_eq!(u32::from(*b) - u32::from(*a), 12, "an octave is twelve");
        }
    }
}

#[cfg(test)]
mod holding_tests {
    use super::{key_spans, voicings};
    use keyflow::text::chart::parse_chart;

    fn chart(text: &str) -> keyflow::Chart {
        parse_chart(text).expect("the test chart should parse")
    }

    /// A chord holds until the next one, and the last holds to the bar.
    ///
    /// The hole this prevents: a chord written short would stop
    /// sounding while the harmony is still going, and the analyser
    /// reading this track back would report a gap in a progression
    /// that has none.
    /// A bar of 2/4 holds two beats of chord, however the chord is
    /// written — not a whole 4/4 bar running into the next one.
    #[test]
    fn a_chord_in_a_short_bar_ends_at_its_bar_line() {
        let voiced = voicings(&chart("My Song\n72bpm 4/4 #D\n\nVS 2\n!T2/4 2m7\n6m7\n"), 3);
        assert_eq!(voiced.len(), 2, "{voiced:?}");
        assert!(
            (voiced[0].beats - 2.0).abs() < 1e-9,
            "the 2/4 bar: {voiced:?}"
        );
        assert!(
            (voiced[1].beats - 4.0).abs() < 1e-9,
            "the 4/4 bar: {voiced:?}"
        );
    }

    /// A bar of rests is no chord: nothing on the chord track.
    #[test]
    fn rests_are_no_chord() {
        let voiced = voicings(&chart("My Song\n72bpm 4/4 #D\n\nVS 4\n1 1maj7 r r\n"), 3);
        let symbols: Vec<_> = voiced
            .iter()
            .map(|v| (v.measure, v.symbol.as_str()))
            .collect();
        assert_eq!(symbols, vec![(0, "1"), (1, "1maj7")]);
    }

    /// `'4` is pushed an eighth: it sounds on the "and" of four of the
    /// bar before, holds to its own bar's end, and the chord before it
    /// stops where it starts.
    #[test]
    fn a_pushed_chord_sounds_an_eighth_early() {
        let voiced = voicings(&chart("My Song\n120bpm 4/4 #C\n\nVS 2\n1 '4\n"), 3);
        assert_eq!(voiced.len(), 2, "{voiced:?}");
        assert!((voiced[0].beats - 3.5).abs() < 1e-9, "{voiced:?}");
        assert_eq!(voiced[1].measure, 1);
        assert!((voiced[1].beat + 0.5).abs() < 1e-9, "{voiced:?}");
        assert!((voiced[1].beats - 4.5).abs() < 1e-9, "{voiced:?}");
    }

    #[test]
    fn a_chord_holds_until_the_next() {
        let voiced = voicings(&chart("My Song\n4/4 #C\n\nVS 1: | C | F |\n"), 3);
        assert_eq!(voiced.len(), 2, "two chords: {voiced:?}");
        for chord in &voiced {
            assert!(
                (chord.beats - 4.0).abs() < 1e-6,
                "a chord alone in its bar should fill it, got {} beats",
                chord.beats
            );
        }
    }

    /// One key, one item — however many bars it covers.
    ///
    /// A key is a state, not an event: it is written where it CHANGES
    /// and holds until it changes again. Restating it every bar would
    /// say something happened when nothing did.
    #[test]
    fn a_key_is_written_once_and_held() {
        let spans = key_spans(&chart("My Song\n4/4 #C\n\nVS 1: | C | F | G | C |\n"));
        assert_eq!(spans.len(), 1, "one key should be one span: {spans:?}");
        assert!(
            spans[0].to_measure - spans[0].from_measure >= 4,
            "the span does not cover the bars it holds for: {spans:?}"
        );
    }
}
