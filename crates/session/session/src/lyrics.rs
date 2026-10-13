//! A song's lyrics, in layers, on the song's own timeline.
//!
//! Seven layers, coarse to fine:
//!
//! | layer | what it is | where it comes from |
//! |---|---|---|
//! | [`Layer::Song`] | every line | derived |
//! | [`Layer::Section`] | a section's lines at a time | derived: lines ⨯ the song's sections |
//! | [`Layer::Slide`] | projector slides, broken automatically | derived: each section's lines, a few at a time |
//! | [`Layer::Line`] | line by line | timed lines (a synced `.lrc`) |
//! | [`Layer::Word`] | word by word | timed words (word-synced LRC, alignment) |
//! | [`Layer::Syllable`] | syllable by syllable | a hyphenation of the words |
//! | [`Layer::SyllableMidi`] | syllables with their melody | syllables plus pitches |
//!
//! Having a layer gives every layer above it: timed lines give sections
//! (placed against the song's own section regions), slides (a section's
//! lines broken [`SLIDE_LINES`] at a time) and the song. Words and
//! syllables are carried when a source has them — nothing makes them up
//! yet, and [`Lyrics::deepest`] says how far a song's lyrics go.
//!
//! Times are seconds on the song's timeline (the project's), not the
//! recording's: [`Lyrics::from_lrc`] is told where the recording's 0
//! lands (the song's start, SONGSTART, by default).

use std::ops::Range;

/// How many lines a slide holds, as a church projects them: two lines
/// read at a glance from the back of the room.
pub const SLIDE_LINES: usize = 2;

/// A layer of the lyrics, coarse to fine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Song,
    Section,
    Slide,
    Line,
    Word,
    Syllable,
    SyllableMidi,
}

impl Layer {
    pub const ALL: [Self; 7] = [
        Self::Song,
        Self::Section,
        Self::Slide,
        Self::Line,
        Self::Word,
        Self::Syllable,
        Self::SyllableMidi,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Song => "Song",
            Self::Section => "Section",
            Self::Slide => "Slides",
            Self::Line => "Lines",
            Self::Word => "Words",
            Self::Syllable => "Syllables",
            Self::SyllableMidi => "Syllables + melody",
        }
    }
}

/// One syllable: its span, its text, and — for [`Layer::SyllableMidi`] —
/// the melody note it is sung on.
#[derive(Clone, Debug, PartialEq)]
pub struct Syllable {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub pitch: Option<u8>,
}

/// One word, and its syllables when something has split it.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub syllables: Vec<Syllable>,
}

/// One sung line, and its words when the source timed them.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// A section of the song, as the lyrics are placed against it.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionSpan {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// A section's lyrics: the section, and the lines sung in it (empty for
/// an instrumental).
#[derive(Clone, Debug, PartialEq)]
pub struct LyricSection {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub lines: Range<usize>,
}

/// A slide: a few of a section's lines, shown together.
#[derive(Clone, Debug, PartialEq)]
pub struct Slide {
    /// Which of [`Lyrics::sections`]' sections it belongs to.
    pub section: usize,
    pub lines: Range<usize>,
    pub start: f64,
    pub end: f64,
}

/// The song's lyrics: its lines, in order, on the song's timeline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
}

impl Lyrics {
    /// Timed lines from a synced `.lrc`, the recording's 0 placed at
    /// `at` on the song's timeline, the last line held to `end`.
    ///
    /// A line lasts until the next one starts. A timed blank line is a
    /// gap — an instrumental, a breath — which ends the line before it
    /// and is not a line itself. Word stamps (enhanced LRC) become the
    /// line's words, each until the next.
    #[must_use]
    pub fn from_lrc(text: &str, at: f64, end: f64) -> Self {
        let lrc = keyflow::lrc::parse(text);
        let stamps: Vec<(f64, &keyflow::lrc::LrcLine)> = lrc
            .lines
            .iter()
            .map(|line| (at + f64::from(line.start), line))
            .collect();
        let mut lines = Vec::new();
        for (i, (start, line)) in stamps.iter().enumerate() {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            let until = stamps.get(i + 1).map_or(end.max(*start), |(next, _)| *next);
            let words = line
                .words
                .iter()
                .enumerate()
                .filter(|(_, w)| !w.text.trim().is_empty())
                .map(|(k, w)| Word {
                    start: at + f64::from(w.start),
                    end: line
                        .words
                        .get(k + 1)
                        .map_or(until, |next| at + f64::from(next.start)),
                    text: w.text.trim().to_owned(),
                    syllables: Vec::new(),
                })
                .collect();
            lines.push(Line {
                start: *start,
                end: until,
                text: text.to_owned(),
                words,
            });
        }
        Self { lines }
    }

    /// The finest layer these lyrics have; every layer above it comes
    /// with it. `None` when there are no lyrics at all.
    #[must_use]
    pub fn deepest(&self) -> Option<Layer> {
        let words = || self.lines.iter().flat_map(|l| &l.words);
        let syllables = || words().flat_map(|w| &w.syllables);
        if self.lines.is_empty() {
            None
        } else if syllables().next().is_none() {
            Some(if words().next().is_some() {
                Layer::Word
            } else {
                Layer::Line
            })
        } else if syllables().all(|s| s.pitch.is_some()) {
            Some(Layer::SyllableMidi)
        } else {
            Some(Layer::Syllable)
        }
    }

    /// Every layer these lyrics can show: the deepest, and all above it.
    #[must_use]
    pub fn layers(&self) -> Vec<Layer> {
        self.deepest().map_or_else(Vec::new, |deepest| {
            Layer::ALL.into_iter().filter(|l| *l <= deepest).collect()
        })
    }

    /// The line being sung at `at`, if any.
    #[must_use]
    pub fn line_at(&self, at: f64) -> Option<usize> {
        self.lines.iter().position(|l| l.start <= at && at < l.end)
    }

    /// The line sung at `at`, or the last one sung before it — what a
    /// display holds on through a gap.
    #[must_use]
    pub fn line_reached(&self, at: f64) -> Option<usize> {
        self.lines.iter().rposition(|l| l.start <= at)
    }

    /// The song's sections with their lines. A line belongs to the
    /// section it overlaps most, so a pickup sung just before a downbeat
    /// stays with the section it leads into.
    #[must_use]
    pub fn sections(&self, spans: &[SectionSpan]) -> Vec<LyricSection> {
        let home = |line: &Line| {
            spans
                .iter()
                .enumerate()
                .map(|(i, s)| (i, line.end.min(s.end) - line.start.max(s.start)))
                .filter(|(_, overlap)| *overlap > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        let homes: Vec<Option<usize>> = self.lines.iter().map(home).collect();
        spans
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let first = homes.iter().position(|h| *h == Some(i));
                let last = homes.iter().rposition(|h| *h == Some(i));
                let lines = match (first, last) {
                    (Some(a), Some(b)) => a..b + 1,
                    _ => 0..0,
                };
                LyricSection {
                    name: s.name.clone(),
                    start: s.start,
                    end: s.end,
                    lines,
                }
            })
            .collect()
    }

    /// Slides: each section's lines, `per_slide` at a time. A slide never
    /// crosses a section, and an instrumental section has none.
    #[must_use]
    pub fn slides(&self, sections: &[LyricSection], per_slide: usize) -> Vec<Slide> {
        let per_slide = per_slide.max(1);
        let mut slides = Vec::new();
        for (index, section) in sections.iter().enumerate() {
            let mut from = section.lines.start;
            while from < section.lines.end {
                let to = (from + per_slide).min(section.lines.end);
                slides.push(Slide {
                    section: index,
                    lines: from..to,
                    start: self.lines[from].start,
                    end: self.lines[to - 1].end,
                });
                from = to;
            }
        }
        slides
    }
}

/// Where a song's lyrics are pinned to it: a line of the lyrics (the
/// first that begins with `line`) starts `beats` after the downbeat of
/// the section named `section` (negative: before it — a pickup). Every
/// line moves with it; with `drop_before`, the lines ahead of it go (a
/// recording's opening the song's version does not have).
///
/// A synced `.lrc` is timed to its recording, and a multitrack's song is
/// rarely laid out the same from its first beat — the anchor is what
/// places one on the other. It is kept in the `.lrc` itself, as a
/// comment the lyrics' own readers skip:
///
/// ```text
/// [#anchor: VS 1 | -1 | And I'm clean | drop-before]
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub section: String,
    pub beats: f64,
    pub line: String,
    pub drop_before: bool,
}

impl Anchor {
    /// Whether a section region named `name` is the anchor's: its name
    /// exactly, or one of its lettered parts — `VS 1` is `VS 1A` when a
    /// song's first verse comes in two.
    #[must_use]
    pub fn names(&self, name: &str) -> bool {
        let (name, wanted) = (name.trim(), self.section.trim());
        name.eq_ignore_ascii_case(wanted)
            || (name.len() == wanted.len() + 1
                && name.is_char_boundary(wanted.len())
                && name[..wanted.len()].eq_ignore_ascii_case(wanted)
                && name[wanted.len()..]
                    .chars()
                    .all(|c| c.is_ascii_alphabetic()))
    }

    /// The anchor a `.lrc` carries, if it has one.
    #[must_use]
    pub fn in_lrc(text: &str) -> Option<Self> {
        text.lines().find_map(|line| {
            let body = line.trim().strip_prefix("[#anchor:")?.strip_suffix(']')?;
            let mut parts = body.split('|').map(str::trim);
            let section = parts.next().filter(|s| !s.is_empty())?.to_owned();
            let beats = parts.next()?.parse().ok()?;
            let line = parts.next().filter(|s| !s.is_empty())?.to_owned();
            let drop_before = parts.any(|flag| flag.eq_ignore_ascii_case("drop-before"));
            Some(Self {
                section,
                beats,
                line,
                drop_before,
            })
        })
    }
}

/// Text as a line is matched: lower case, curly apostrophes straight.
fn matchable(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
}

impl Lyrics {
    /// Move every line so the first that begins with `line` starts at
    /// `at`, dropping the lines ahead of it when `drop_before`; the last
    /// line is held to `end`. `false`, and nothing moved, when no line
    /// begins that way.
    pub fn align(&mut self, line: &str, at: f64, drop_before: bool, end: f64) -> bool {
        let wanted = matchable(line);
        let Some(index) = self
            .lines
            .iter()
            .position(|l| matchable(&l.text).starts_with(&wanted))
        else {
            return false;
        };
        let shift = at - self.lines[index].start;
        if drop_before {
            self.lines.drain(..index);
        }
        for line in &mut self.lines {
            line.start += shift;
            line.end += shift;
            for word in &mut line.words {
                word.start += shift;
                word.end += shift;
            }
        }
        if let Some(last) = self.lines.last_mut() {
            last.end = end.max(last.start);
        }
        true
    }
}

/// The Keyflow folder's track the lyrics are kept on: one empty item a
/// line, spanning it, labelled with its text — so the lines are part of
/// the song (saved in its `.session`, moved and trimmed in the
/// arrangement) and the display reads them from the song, not from a
/// file beside it. (Not LINES, beside it: that is the chart's melodies.)
pub const LYRICS_TRACK: &str = "Lyrics";

/// Whether `name` is the lyrics track's.
#[must_use]
pub fn is_lyrics_track(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case(LYRICS_TRACK)
}

impl Lyrics {
    /// The lines as a track holds them: `(start, length, label)` per item.
    /// Items with no text are skipped; the order is by start.
    #[must_use]
    pub fn from_items<'a>(items: impl IntoIterator<Item = (f64, f64, &'a str)>) -> Self {
        let mut lines: Vec<Line> = items
            .into_iter()
            .filter(|(_, _, text)| !text.trim().is_empty())
            .map(|(start, length, text)| Line {
                start,
                end: start + length.max(0.0),
                text: text.trim().to_owned(),
                words: Vec::new(),
            })
            .collect();
        lines.sort_by(|a, b| a.start.total_cmp(&b.start));
        Self { lines }
    }
}

/// Put `lyrics` on the Lyrics track — made in the Keyflow folder, before
/// its closing HITS, if the song has none — in place of whatever lines
/// were there. Returns how many lines were stamped.
///
/// # Errors
///
/// A line's item could not be created or labelled.
pub fn stamp_lines<D>(
    daw: &D,
    project: daw::service::ProjectContext,
    lyrics: &Lyrics,
) -> daw_proto::DawResult<usize>
where
    D: daw::service::Tracks + daw::service::Items,
{
    use daw::service::{Duration, ItemRef, PositionInSeconds, TrackRef};
    let tracks = daw.all(project.clone());
    let track = match tracks.iter().find(|t| is_lyrics_track(&t.name)) {
        Some(track) => TrackRef::Guid(track.guid.clone()),
        None => {
            let before_hits = tracks
                .iter()
                .find(|t| t.name.trim().eq_ignore_ascii_case("HITS"))
                .map(|t| t.index);
            TrackRef::Guid(daw.add(project.clone(), LYRICS_TRACK, before_hits)?)
        }
    };
    for item in daw.get_items(project.clone(), track.clone()) {
        daw.delete_item(project.clone(), ItemRef::Guid(item.guid))?;
    }
    for line in &lyrics.lines {
        let guid = daw
            .add_item(
                project.clone(),
                track.clone(),
                PositionInSeconds::from_seconds(line.start),
                Duration::from_seconds((line.end - line.start).max(0.05)),
            )
            .ok_or_else(|| {
                daw_proto::DawError::OperationFailed(format!(
                    "could not create a line at {:.2} s",
                    line.start
                ))
            })?;
        daw.set_label(project.clone(), ItemRef::Guid(guid), &line.text)?;
    }
    Ok(lyrics.lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An anchor pins a line to a spot in the song: everything moves with
    /// it, and what comes before it can go.
    #[test]
    fn an_anchor_places_the_lyrics() {
        let text = "[#anchor: VS 1 | -1 | And I\u{2019}m clean | drop-before]\n\
                    [00:00.50]Washed in the water\n[00:21.11]And I'm clean\n[00:22.67]Sin was stained on me\n";
        let anchor = Anchor::in_lrc(text).expect("an anchor");
        assert_eq!(
            anchor,
            Anchor {
                section: "VS 1".into(),
                beats: -1.0,
                line: "And I\u{2019}m clean".into(),
                drop_before: true
            }
        );
        let mut lyrics = Lyrics::from_lrc(text, 0.0, 60.0);
        assert!(lyrics.align(&anchor.line, 10.0, anchor.drop_before, 60.0));
        let got: Vec<(f64, &str)> = lyrics
            .lines
            .iter()
            .map(|l| (l.start, l.text.as_str()))
            .collect();
        assert_eq!(got.len(), 2);
        assert!((got[0].0 - 10.0).abs() < 1e-6 && got[0].1 == "And I'm clean");
        assert!((got[1].0 - 11.56).abs() < 1e-6);
        assert!((lyrics.lines[1].end - 60.0).abs() < 1e-9);
        assert!(!lyrics.align("Not a line", 0.0, false, 60.0));
        assert!(
            anchor.names("VS 1")
                && anchor.names("vs 1a")
                && !anchor.names("VS 10")
                && !anchor.names("VS 2A")
        );
    }

    /// A track's labelled items read back as the lines they were.
    #[test]
    fn lines_read_back_from_a_track() {
        let lyrics =
            Lyrics::from_items([(9.0, 2.0, "Second"), (7.0, 2.0, "First"), (12.0, 1.0, "  ")]);
        let got: Vec<(f64, f64, &str)> = lyrics
            .lines
            .iter()
            .map(|l| (l.start, l.end, l.text.as_str()))
            .collect();
        assert_eq!(got, [(7.0, 9.0, "First"), (9.0, 11.0, "Second")]);
    }

    const LRC: &str = "[ti:Song]\n[00:01.00]First line\n[00:03.00]Second line\n[00:05.00]\n[00:08.00]Third line\n[00:10.00]Fourth line\n";

    fn span(name: &str, start: f64, end: f64) -> SectionSpan {
        SectionSpan {
            name: name.into(),
            start,
            end,
        }
    }

    /// Lines land at the song's start plus their stamp; each holds until
    /// the next, and a blank stamp is a gap that ends the line before.
    #[test]
    fn synced_lines_land_on_the_songs_timeline() {
        let lyrics = Lyrics::from_lrc(LRC, 6.0, 20.0);
        let spans: Vec<(f64, f64, &str)> = lyrics
            .lines
            .iter()
            .map(|l| (l.start, l.end, l.text.as_str()))
            .collect();
        assert_eq!(
            spans,
            [
                (7.0, 9.0, "First line"),
                (9.0, 11.0, "Second line"),
                (14.0, 16.0, "Third line"),
                (16.0, 20.0, "Fourth line"),
            ]
        );
        assert_eq!(lyrics.line_at(12.0), None, "the gap");
        assert_eq!(lyrics.line_reached(12.0), Some(1));
        assert_eq!(lyrics.line_at(14.5), Some(2));
    }

    /// Lines give every layer down to lines; timed words go one deeper.
    #[test]
    fn a_deeper_layer_gives_every_layer_above_it() {
        let lines = Lyrics::from_lrc(LRC, 0.0, 20.0);
        assert_eq!(lines.deepest(), Some(Layer::Line));
        assert_eq!(
            lines.layers(),
            [Layer::Song, Layer::Section, Layer::Slide, Layer::Line]
        );
        let words = Lyrics::from_lrc("[00:01.00]<00:01.00>Holy <00:01.50>forever\n", 0.0, 4.0);
        assert_eq!(words.deepest(), Some(Layer::Word));
        let w: Vec<(f64, f64, &str)> = words.lines[0]
            .words
            .iter()
            .map(|w| (w.start, w.end, w.text.as_str()))
            .collect();
        assert_eq!(w, [(1.0, 1.5, "Holy"), (1.5, 4.0, "forever")]);
        assert_eq!(Lyrics::default().deepest(), None);
    }

    /// A line belongs to the section it mostly sits in; an instrumental
    /// section has no lines and no slides; slides stay inside sections.
    #[test]
    fn lines_make_sections_and_slides() {
        let lyrics = Lyrics::from_lrc(LRC, 0.0, 12.0);
        // The first line starts in the intro but is mostly the verse's.
        let sections = lyrics.sections(&[
            span("Intro", 0.0, 1.5),
            span("Verse", 1.5, 5.0),
            span("Inst", 5.0, 8.0),
            span("Chorus", 8.0, 12.0),
        ]);
        let lines: Vec<(&str, Range<usize>)> = sections
            .iter()
            .map(|s| (s.name.as_str(), s.lines.clone()))
            .collect();
        assert_eq!(
            lines,
            [
                ("Intro", 0..0),
                ("Verse", 0..2),
                ("Inst", 0..0),
                ("Chorus", 2..4)
            ]
        );

        let slides = lyrics.slides(&sections, 1);
        let each: Vec<(usize, Range<usize>)> = slides
            .iter()
            .map(|s| (s.section, s.lines.clone()))
            .collect();
        assert_eq!(each, [(1, 0..1), (1, 1..2), (3, 2..3), (3, 3..4)]);
        let two = lyrics.slides(&sections, SLIDE_LINES);
        assert_eq!(two.len(), 2);
        assert!((two[1].start - 8.0).abs() < 1e-9 && (two[1].end - 12.0).abs() < 1e-9);
    }
}
