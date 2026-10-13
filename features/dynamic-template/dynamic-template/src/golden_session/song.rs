//! The golden song: its key, its harmony, and every part the session's
//! MIDI tracks play of it.
//!
//! One source for all of it. The Keyflow folder's KEY and CHORD say the
//! key and the chords, LINES the melody, HITS the band's accents; the
//! keys and the synths play the same chords as parts — a piano comping,
//! a Rhodes holding sevenths, an arp running the triad — and [`chart`]
//! writes the same song as keyflow chart text. Change the progression
//! here and the chart, the chord lane and every part change with it.
//!
//! None of this is meant to be a good song. It is meant to be a REAL
//! one: a key, a progression per section the way a chart writes it, and
//! parts that are what those instruments play over it — so the editor,
//! the chord lane and the chart have something true to show.

use std::fmt::Write as _;

use dawfile_reaper::builder::MidiSourceBuilder;

use super::shape::Section;

/// What the KEY track's item is called: the key, as the session reads
/// it back (`session::key::parse_key`).
pub const KEY_NAME: &str = "F major";
/// The chart's key, as keyflow writes it.
const CHART_KEY: &str = "#F";
/// The tonic's pitch class: F.
const TONIC: u8 = 5;
/// The major scale, in semitones above the tonic.
const SCALE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Ticks per quarter note in every item.
const QN: u32 = 960;
/// A bar of 4/4.
const BAR: u32 = QN * 4;

/// `n` quarter notes, in ticks.
const fn beats(n: u32) -> u32 {
    QN.saturating_mul(n)
}

/// `ticks` past `at`.
fn plus(at: u64, ticks: u32) -> u64 {
    at.saturating_add(u64::from(ticks))
}

/// `n` modulo `m`, for an `m` that is never zero.
const fn modulo(n: usize, m: usize) -> usize {
    match n.checked_rem(m) {
        Some(r) => r,
        None => 0,
    }
}

/// A chord by its scale degree, 1–7, as a chart writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Degree(pub u8);

impl Degree {
    /// The chart's spelling: `1`, `6-` for the minor sixth.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self.0 {
            1 => "1",
            2 => "2-",
            3 => "3-",
            4 => "4",
            5 => "5",
            6 => "6-",
            _ => "7o",
        }
    }

    /// The pitch class of the chord's `k`th stacked third (0 the root,
    /// 1 the third, 2 the fifth, 3 the seventh), diatonic to the key.
    fn pc(self, k: u8) -> u8 {
        let step = modulo(
            usize::from(self.0.saturating_sub(1)).saturating_add(usize::from(k).saturating_mul(2)),
            SCALE.len(),
        );
        let above = SCALE.get(step).copied().unwrap_or(0);
        u8::try_from(modulo(usize::from(TONIC.saturating_add(above)), 12)).unwrap_or(0)
    }

    /// The root at or above `low`.
    fn root(self, low: u8) -> u8 {
        up_to(self.pc(0), low)
    }

    /// The chord's first `voices` stacked thirds in close position, the
    /// root at or above `low`, as MIDI pitches.
    #[must_use]
    pub fn pitches(self, low: u8, voices: u8) -> Vec<u8> {
        self.voicing(low, voices)
    }

    /// The chord's first `voices` stacked thirds in close position, the
    /// root at or above `low`.
    fn voicing(self, low: u8, voices: u8) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for k in 0..voices {
            let floor = out.last().map_or(low, |p| p.saturating_add(1));
            out.push(up_to(self.pc(k), floor));
        }
        out
    }
}

/// The first pitch of class `pc` at or above `low`.
fn up_to(pc: u8, low: u8) -> u8 {
    let octave = low.saturating_sub(u8::try_from(modulo(usize::from(low), 12)).unwrap_or(0));
    let p = octave.saturating_add(pc);
    if p < low {
        p.saturating_add(12)
    } else {
        p
    }
}

/// A section's progression, a degree a bar, repeated to fill it.
fn progression(section: &str) -> &'static [u8] {
    let kind = section.split_whitespace().next().unwrap_or("");
    match kind {
        "Intro" => &[1, 4, 6, 5],
        "Verse" => &[1, 1, 4, 4, 6, 5, 4, 4],
        "Chorus" => &[4, 5, 6, 1, 4, 5, 1, 1],
        "Bridge" => &[6, 4, 1, 5, 6, 4, 1, 5],
        "Outro" => &[1, 4, 6, 5, 1, 4, 1, 1],
        _ => &[1],
    }
}

/// The section a bar is in.
fn section_at(sections: &[Section], bar: u32) -> Option<&Section> {
    sections.iter().find(|s| bar >= s.start && bar < s.end)
}

/// The chord on `bar`.
#[must_use]
pub fn chord_at(sections: &[Section], bar: u32) -> Degree {
    section_at(sections, bar).map_or(Degree(1), |s| {
        let prog = progression(s.name);
        let into = usize::try_from(bar.saturating_sub(s.start)).unwrap_or(0);
        Degree(prog.get(modulo(into, prog.len())).copied().unwrap_or(1))
    })
}

/// Every chord change over the song: (first bar, bars it holds, chord).
#[must_use]
pub fn chord_changes(sections: &[Section], bars: u32) -> Vec<(u32, u32, Degree)> {
    let mut out: Vec<(u32, u32, Degree)> = Vec::new();
    for bar in 0..bars {
        let chord = chord_at(sections, bar);
        let new_section = sections.iter().any(|s| s.start == bar);
        match out.last_mut() {
            Some(last) if last.2 == chord && !new_section => last.1 = last.1.saturating_add(1),
            _ => out.push((bar, 1, chord)),
        }
    }
    out
}

/// The song as keyflow chart text: a header, then a line per section.
///
/// Title, tempo, meter and key, then each section with its bars. No
/// `--- keyflow ---` fence: that is for a document holding a chordpro
/// part too, and on a chart alone the fence is read as its title and the
/// key line goes unread.
#[must_use]
pub fn chart(title: &str, bpm: f64, sections: &[Section]) -> String {
    let mut out = format!("{title} - Golden Session\n{bpm:.0}bpm 4/4 {CHART_KEY}\n\n");
    for s in sections {
        let bars: Vec<&str> = (s.start..s.end)
            .map(|bar| chord_at(sections, bar).symbol())
            .collect();
        let _ = write!(out, "{}: | {} |\n\n", s.name, bars.join(" | "));
    }
    out
}

/// The parts the session's MIDI tracks play, by track name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The key, held: one note on the tonic for the item.
    Key,
    /// The chord, held to the next: a triad per change.
    Chord,
    /// The melody: chord tones, a phrase per four bars.
    Lines,
    /// The band's accents: section downbeats, and every other bar in a
    /// chorus.
    Hits,
    /// Comping: the bass root and the triad on 1 and 3.
    Piano,
    /// Sevenths, held a bar.
    Rhodes,
    /// Pushed eighths: 1, the and of 2, 4.
    Wurli,
    /// The triad high and held.
    Organ,
    /// The root, low, on every beat.
    SubBass,
    /// Root and fifth, high, two bars at a time.
    Texture,
    /// The triad in sixteenths over two octaves.
    Arp,
    /// Four voices, wide and soft, a bar each.
    Pad,
    /// The melody an octave up, in the choruses.
    LeadSynth,
    /// Off-beat stabs.
    ChordStab,
}

/// Where a bar is being written: the song bar, its tick in the item,
/// its chord, how many bars the item has left from it, and what the
/// song is doing there.
#[derive(Clone, Copy)]
struct At {
    bar: u32,
    tick: u64,
    chord: Degree,
    left: u32,
    chorus: bool,
    section_start: bool,
    first: bool,
}

impl Part {
    /// The part a track plays, by its name; `None` for a track the song
    /// writes nothing for.
    #[must_use]
    pub fn of(track: &str) -> Option<Self> {
        Some(match track {
            "KEY" => Self::Key,
            "CHORD" => Self::Chord,
            "LINES" => Self::Lines,
            "HITS" => Self::Hits,
            "Piano" => Self::Piano,
            "Rhodes" => Self::Rhodes,
            "Wurli" => Self::Wurli,
            "Organ" => Self::Organ,
            "Sub Bass Synth" => Self::SubBass,
            "Texture" => Self::Texture,
            "Arp" => Self::Arp,
            "Pad" => Self::Pad,
            "Lead Synth" => Self::LeadSynth,
            "Chord" => Self::ChordStab,
            _ => return None,
        })
    }

    /// Where this part's items go over the song: (first bar, bars, name).
    /// The Keyflow tracks are laid out by the song — one key, an item a
    /// chord, an item a section; the instruments return `None` and take
    /// the comped-take layout every other track has.
    #[must_use]
    pub fn items(self, sections: &[Section], bars: u32) -> Option<Vec<(u32, u32, String)>> {
        let whole = |s: &Section| (s.start, s.end.saturating_sub(s.start), s.name.to_owned());
        match self {
            Self::Key => Some(vec![(0, bars, KEY_NAME.to_owned())]),
            Self::Chord => Some(
                chord_changes(sections, bars)
                    .into_iter()
                    .map(|(at, len, c)| (at, len, c.symbol().to_owned()))
                    .collect(),
            ),
            Self::Lines | Self::Hits => Some(sections.iter().map(whole).collect()),
            Self::LeadSynth => Some(
                sections
                    .iter()
                    .filter(|s| s.name.starts_with("Chorus"))
                    .map(whole)
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Write this part for `bars` bars from song bar `from` into an item.
    #[must_use]
    pub fn write(
        self,
        m: MidiSourceBuilder,
        sections: &[Section],
        from: u32,
        bars: u32,
    ) -> MidiSourceBuilder {
        let mut m = m.ticks_per_qn(QN);
        if self == Self::Key {
            return m.note(
                0,
                0,
                Degree(1).root(41),
                70,
                BAR.saturating_mul(bars.max(1)),
            );
        }
        for i in 0..bars {
            let bar = from.saturating_add(i);
            let at = At {
                bar,
                tick: u64::from(i).saturating_mul(u64::from(BAR)),
                chord: chord_at(sections, bar),
                left: bars.saturating_sub(i),
                chorus: section_at(sections, bar).is_some_and(|s| s.name.starts_with("Chorus")),
                section_start: sections.iter().any(|s| s.start == bar),
                first: i == 0,
            };
            m = self.bar(m, at);
        }
        m
    }

    /// One bar of this part.
    fn bar(self, m: MidiSourceBuilder, at: At) -> MidiSourceBuilder {
        let c = at.chord;
        match self {
            // Held to the next change: the item IS the change.
            Self::Chord if at.first => chord(
                m,
                at.tick,
                &c.voicing(55, 3),
                85,
                BAR.saturating_mul(at.left),
            ),
            Self::Lines => melody(m, c, at, 60, 92),
            Self::LeadSynth => melody(m, c, at, 72, 100),
            Self::Hits if at.section_start || (at.chorus && at.bar.is_multiple_of(2)) => {
                m.at(at.tick).note(0, 9, 36, 120, QN / 2)
            }
            Self::Piano => {
                let m = m.at(at.tick).note(0, 0, c.root(36), 80, BAR);
                let voices = c.voicing(57, 3);
                let m = chord(m, at.tick, &voices, 78, beats(2).saturating_sub(40));
                chord(
                    m,
                    plus(at.tick, beats(2)),
                    &voices,
                    66,
                    beats(2).saturating_sub(40),
                )
            }
            Self::Rhodes => chord(m, at.tick, &c.voicing(53, 4), 70, BAR.saturating_sub(60)),
            Self::Wurli => {
                let voices = c.voicing(58, 3);
                [0, beats(1).saturating_add(QN / 2), beats(3)]
                    .into_iter()
                    .fold(m, |m, off| {
                        chord(m, plus(at.tick, off), &voices, 82, QN / 2)
                    })
            }
            Self::Organ => chord(m, at.tick, &c.voicing(65, 3), 60, BAR),
            Self::SubBass => (0..4).fold(m, |m, beat| {
                let velocity = if beat == 0 { 105 } else { 90 };
                m.at(plus(at.tick, beats(beat))).note(
                    0,
                    0,
                    c.root(29),
                    velocity,
                    QN.saturating_sub(80),
                )
            }),
            Self::Texture if at.bar.is_multiple_of(2) => {
                let root = c.root(72);
                let len = BAR.saturating_mul(at.left.min(2));
                m.at(at.tick)
                    .note(0, 0, root, 50, len)
                    .note(0, 0, up_to(c.pc(2), root), 45, len)
            }
            Self::Arp => arp(m, c, at.tick),
            Self::Pad => {
                let mut voices = c.voicing(48, 3);
                voices.push(c.root(48).saturating_add(12));
                chord(m, at.tick, &voices, 55, BAR)
            }
            Self::ChordStab => {
                let voices = c.voicing(64, 3);
                (0..4).fold(m, |m, beat| {
                    chord(
                        m,
                        plus(at.tick, beats(beat).saturating_add(QN / 2)),
                        &voices,
                        90,
                        QN / 4,
                    )
                })
            }
            // A held key, a chord past its change, a bar with no hit, a
            // texture's held second bar: nothing new to sound.
            Self::Key | Self::Chord | Self::Hits | Self::Texture => m,
        }
    }
}

/// Every pitch of `voices` at `tick`, `len` long.
fn chord(
    m: MidiSourceBuilder,
    tick: u64,
    voices: &[u8],
    velocity: u8,
    len: u32,
) -> MidiSourceBuilder {
    voices
        .iter()
        .fold(m.at(tick), |m, &p| m.note(0, 0, p, velocity, len))
}

/// A bar of sixteenths up and down the triad over two octaves.
fn arp(m: MidiSourceBuilder, c: Degree, tick: u64) -> MidiSourceBuilder {
    let tones = c.voicing(60, 3);
    let mut run: Vec<u8> = tones.clone();
    run.extend(tones.iter().map(|p| p.saturating_add(12)));
    run.push(c.root(60).saturating_add(24));
    let turn = run.len().saturating_sub(1);
    (0..16u32).fold(m, |m, step| {
        let s = usize::try_from(step).unwrap_or(0);
        let up = modulo(s, turn.saturating_mul(2).max(1));
        let idx = if up <= turn {
            up
        } else {
            turn.saturating_mul(2).saturating_sub(up)
        };
        let pitch = run.get(idx).copied().unwrap_or(60);
        let velocity = if step.is_multiple_of(4) { 95 } else { 75 };
        m.at(plus(tick, (QN / 4).saturating_mul(step))).note(
            0,
            0,
            pitch,
            velocity,
            (QN / 4).saturating_sub(20),
        )
    })
}

/// A melody's four-bar phrase: (beat, length in beats, tone) per bar.
const PHRASE: [&[(u32, u32, usize)]; 4] = [
    &[(0, 1, 0), (1, 1, 1), (2, 2, 2)],
    &[(0, 2, 3), (2, 1, 2), (3, 1, 1)],
    &[(0, 1, 1), (1, 1, 2), (2, 1, 3), (3, 1, 2)],
    &[(0, 3, 1)],
];

/// A bar of melody over `chord`: chord tones around `low`, a four-bar
/// phrase that rises, answers, and breathes on its last beat.
fn melody(m: MidiSourceBuilder, chord: Degree, at: At, low: u8, velocity: u8) -> MidiSourceBuilder {
    bar_melody(chord, at.bar, low)
        .into_iter()
        .fold(m, |m, (beat, len, pitch)| {
            m.at(plus(at.tick, beats(beat))).note(
                0,
                0,
                pitch,
                velocity,
                beats(len).saturating_sub(60),
            )
        })
}

/// One bar of the melody over `chord`: (beat, length in beats, pitch).
fn bar_melody(chord: Degree, bar: u32, low: u8) -> Vec<(u32, u32, u8)> {
    let tones = chord.voicing(low, 3);
    let pick = |k: usize| {
        let base = tones.get(modulo(k, 3)).copied().unwrap_or(low);
        if k >= 3 {
            base.saturating_add(12)
        } else {
            base
        }
    };
    PHRASE
        .get(modulo(usize::try_from(bar).unwrap_or(0), 4))
        .copied()
        .unwrap_or(&[])
        .iter()
        .map(|&(beat, len, k)| (beat, len, pick(k)))
        .collect()
}

/// The whole song's melody, around `low`: (first beat, beats, pitch) per
/// note, beats counted from the song's start — what LINES writes and
/// what the performed vocals sing (`golden_session::perform`).
#[must_use]
pub fn melody_notes(sections: &[Section], bars: u32, low: u8) -> Vec<(u32, u32, u8)> {
    (0..bars)
        .flat_map(|bar| {
            let at = bar.saturating_mul(4);
            bar_melody(chord_at(sections, bar), bar, low)
                .into_iter()
                .map(move |(beat, len, pitch)| (at.saturating_add(beat), len, pitch))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections() -> Vec<Section> {
        super::super::shape::maximal().sections
    }

    /// The chords are F major's: the one is F A C, the six-minor D F A.
    #[test]
    fn the_chords_are_the_keys() {
        assert_eq!(Degree(1).voicing(60, 3), [65, 69, 72]);
        assert_eq!(Degree(6).voicing(60, 3), [62, 65, 69]);
        assert_eq!(Degree(5).voicing(60, 4), [60, 64, 67, 70]);
        assert_eq!(Degree(4).root(29), 34);
    }

    /// Every bar of the song has a chord, a chorus starts on the four,
    /// and the changes cover the song exactly.
    #[test]
    fn the_song_is_covered() {
        let s = sections();
        let changes = chord_changes(&s, 64);
        assert_eq!(changes.iter().map(|c| c.1).sum::<u32>(), 64);
        assert_eq!(chord_at(&s, 12), Degree(4), "Chorus 1 opens on the four");
        // A section boundary is a change even when the chord is the same.
        assert!(changes.iter().any(|c| c.0 == 4));
    }

    /// The chart is keyflow text with every section and its bars.
    #[test]
    fn the_chart_names_every_section() {
        let s = sections();
        let text = chart("template", 120.0, &s);
        assert!(text.starts_with("template - Golden Session\n"));
        assert!(text.contains("120bpm 4/4 #F"));
        for section in &s {
            assert!(text.contains(&format!("{}: |", section.name)), "{text}");
        }
        assert!(text.contains("Chorus 1: | 4 | 5 | 6- | 1 |"), "{text}");
    }
}
