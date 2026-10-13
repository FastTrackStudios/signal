//! Interpret keyflow chart text into a laid-out session song structure.
//!
//! The keyflow chart is the single source of *what* — section types, order,
//! bar counts, tempo/key. [`chart_to_layout`] turns that into absolute-time
//! [`LaidSection`]s a setlist/demo can stamp. This is the bridge behind live
//! keyflow-text editing (edit the chart → re-lay the sections) and the Praise
//! demo song; the sung/audio *when* comes separately (guide analysis / lyric
//! sync).

use crate::section_kinds::SectionKind;

/// Failure modes of [`chart_to_layout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChartImportError {
    /// The keyflow parser rejected the text.
    Parse(String),
    /// The chart parsed but has no sections.
    Empty,
}

impl std::fmt::Display for ChartImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "chart parse failed: {e}"),
            Self::Empty => write!(f, "chart has no sections"),
        }
    }
}

impl std::error::Error for ChartImportError {}

/// One section placed on the song timeline (absolute seconds from the region
/// start, count-in included at the front).
#[derive(Debug, Clone, PartialEq)]
pub struct LaidSection {
    /// Session section kind (marker/colour driver).
    pub kind: SectionKind,
    /// Custom label from a quoted chart comment (`Interlude "Breakdown"`).
    pub label: Option<String>,
    /// Auto-assigned occurrence number ("Verse 1"), if any.
    pub number: Option<u32>,
    /// Start position in seconds (from region start; count-in is at 0).
    pub start_seconds: f64,
    /// End position in seconds.
    pub end_seconds: f64,
    /// Length in measures.
    pub measures: u32,
}

/// A keyflow chart laid out as a playable song skeleton.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLayout {
    /// Song title from the chart header.
    pub title: Option<String>,
    /// Artist from the chart header.
    pub artist: Option<String>,
    /// Tempo in BPM (defaults to 120 if the chart omits it).
    pub tempo_bpm: f64,
    /// Time-signature numerator.
    pub time_sig_num: u32,
    /// Time-signature denominator.
    pub time_sig_den: u32,
    /// Initial key (note name, e.g. "A"), if the chart declares one.
    pub key: Option<String>,
    /// Count-in duration before the first musical downbeat (0 if no leading
    /// `CountIn` section).
    pub count_in_seconds: f64,
    /// The first musical downbeat, in seconds (== `count_in_seconds`).
    pub song_start_seconds: f64,
    /// End of the last section, in seconds.
    pub song_end_seconds: f64,
    /// All sections in order, count-in first (if present).
    pub sections: Vec<LaidSection>,
    /// Where each measure starts, in seconds, in order through the whole
    /// chart — the chord track's measures, each at its own meter. What a
    /// chord's `(measure, beat)` is placed by.
    pub measure_starts: Vec<f64>,
    /// Where the meter changes, as `(seconds, numerator, denominator)`: a bar
    /// of 2/4 in a song of 4/4 is a change to 2/4 at its start and back to
    /// 4/4 after it. The header's meter at 0 is not listed.
    pub meter_changes: Vec<(f64, u32, u32)>,
}

/// A bar of `(numerator, denominator)`, in quarter notes — what the tempo
/// counts. Six-eight is three.
#[must_use]
pub fn quarters_in(meter: (u8, u8)) -> f64 {
    f64::from(meter.0.max(1)) * 4.0 / f64::from(meter.1.max(1))
}

const DEFAULT_TEMPO_BPM: f64 = 120.0;

/// Parse keyflow chart text and lay its sections out on a seconds timeline.
///
/// Measure length is `numerator * 60 / bpm` (worship charts are `*/4`, so the
/// beat unit is a quarter). Sections are placed contiguously from region start
/// 0; a leading `CountIn` section occupies the front, so the first musical
/// section starts at `count_in_seconds`. A section with no explicit bar count
/// falls back to its kind's default.
///
/// # Errors
///
/// Returns [`ChartImportError::Parse`] if the keyflow chart parser rejects the
/// input text. Returns [`ChartImportError::Empty`] if the chart parses
/// successfully but contains no sections.
pub fn chart_to_layout(chart_text: &str) -> Result<ChartLayout, ChartImportError> {
    let chart = keyflow::text::chart::parse_chart(chart_text).map_err(ChartImportError::Parse)?;
    if chart.sections.is_empty() {
        return Err(ChartImportError::Empty);
    }

    let tempo_bpm = chart
        .tempo
        .map(|t| t.bpm)
        .filter(|b| *b > 0.0)
        .unwrap_or(DEFAULT_TEMPO_BPM);
    let (num, den) = chart
        .time_signature
        .map_or((4, 4), |ts| (ts.numerator, ts.denominator));
    // A quarter note is 60/bpm. Each measure lasts its OWN meter — a chart
    // that writes one bar of 2/4 (`!T2/4`) in a song of 4/4 has a bar half
    // as long there, and everything after it lands two beats earlier than
    // a single meter would put it.
    let quarter = 60.0 / tempo_bpm;
    let header = (
        u8::try_from(num).unwrap_or(4),
        u8::try_from(den).unwrap_or(4),
    );
    let key = chart.initial_key.as_ref().map(|k| k.root.name.clone());

    let mut sections: Vec<LaidSection> = Vec::with_capacity(chart.sections.len());
    let mut measure_starts: Vec<f64> = Vec::new();
    let mut meter_changes: Vec<(f64, u32, u32)> = Vec::new();
    let mut meter = header;
    let mut at = 0.0_f64;
    for cs in &chart.sections {
        let s = &cs.section;
        let kind = SectionKind::from_section_type(&s.section_type);
        let measures = s
            .measure_count
            .and_then(|m| u32::try_from(m).ok())
            .filter(|m| *m > 0)
            .unwrap_or_else(|| kind.default_measure_count());
        // The parsed bars carry their meters; a section whose bars do not
        // add up to its count (the parser did not expand it) is its count
        // at the prevailing meter.
        let bars: Vec<(u8, u8)> = cs.measures().iter().map(|m| m.time_signature).collect();
        let bars = if bars.len() == measures as usize {
            bars
        } else {
            vec![meter; measures as usize]
        };
        let start_seconds = at;
        for bar in bars {
            if bar != meter {
                meter_changes.push((at, u32::from(bar.0), u32::from(bar.1)));
                meter = bar;
            }
            measure_starts.push(at);
            at += quarters_in(bar) * quarter;
        }
        sections.push(LaidSection {
            kind,
            label: s.comment.clone(),
            number: s.number,
            start_seconds,
            end_seconds: at,
            measures,
        });
    }

    // The leading CountIn section (if any) is the count-in; the first musical
    // downbeat is right after it.
    let count_in_seconds = sections
        .first()
        .filter(|s| s.kind == SectionKind::CountIn)
        .map_or(0.0, |s| s.end_seconds - s.start_seconds);
    let song_end_seconds = sections.last().map_or(0.0, |s| s.end_seconds);

    Ok(ChartLayout {
        title: chart.metadata.title.clone(),
        artist: chart.metadata.artist,
        tempo_bpm,
        time_sig_num: num,
        time_sig_den: den,
        key,
        count_in_seconds,
        song_start_seconds: count_in_seconds,
        song_end_seconds,
        sections,
        measure_starts,
        meter_changes,
    })
}

#[cfg(test)]
mod meter_tests {
    use super::chart_to_layout;

    /// One bar of 2/4 in a song of 4/4: half a bar long, everything after
    /// it two beats earlier than one meter would put it, and the change to
    /// 2/4 and back is listed where it happens.
    #[test]
    fn a_bar_of_two_four_is_half_a_bar() {
        let layout =
            chart_to_layout("Song\n60bpm 4/4 #D\n\nCount 1\nCH 2\nBreakdown 1\n!T2/4\nVS 2\n")
                .expect("lays out");
        // At 60 bpm a quarter is a second: 4 + 8 = 12 s to the breakdown,
        // which lasts 2 s, so the verse starts at 14 and ends at 22.
        let starts: Vec<(f64, f64)> = layout
            .sections
            .iter()
            .map(|s| (s.start_seconds, s.end_seconds))
            .collect();
        assert_eq!(
            starts,
            vec![(0.0, 4.0), (4.0, 12.0), (12.0, 14.0), (14.0, 22.0)]
        );
        assert_eq!(layout.meter_changes, vec![(12.0, 2, 4), (14.0, 4, 4)]);
        assert_eq!(layout.measure_starts, vec![0.0, 4.0, 8.0, 12.0, 14.0, 18.0]);
        assert!((layout.song_end_seconds - 22.0).abs() < 1e-9);
    }

    /// A chart with no meter change lists none.
    #[test]
    fn one_meter_throughout_lists_no_changes() {
        let layout = chart_to_layout("Song\n120bpm 4/4 #C\n\nVS 4\nCH 4\n").expect("lays out");
        assert!(layout.meter_changes.is_empty());
        assert_eq!(layout.measure_starts.len(), 8);
    }
}
