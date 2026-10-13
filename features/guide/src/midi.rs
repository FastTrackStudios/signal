//! MIDI note constants and mappings (ported from fts-guide `midi/notes.rs`).
//!
//! Kept so hosts can emit the guide as a MIDI track (e.g. for external
//! samplers) using the same note layout the legacy plugin used.

/// MIDI note constants for click subdivisions
pub const MIDI_NOTE_CLICK_ACCENT: u8 = 60; // C4
pub const MIDI_NOTE_CLICK_BEAT: u8 = 61; // C#4
pub const MIDI_NOTE_CLICK_EIGHTH: u8 = 62; // D4
pub const MIDI_NOTE_CLICK_SIXTEENTH: u8 = 63; // D#4
pub const MIDI_NOTE_CLICK_TRIPLET: u8 = 65; // F4

/// MIDI notes for count samples (1-8)
pub const MIDI_NOTES_COUNT: [u8; 8] = [72, 73, 74, 75, 76, 77, 78, 79]; // C5-C6

/// Map a section *name* to a MIDI note (starting from C6 = 84).
///
/// The legacy plugin's table, kept verbatim for names that aren't
/// [`SectionType`] variants (Tag, Rap, Acapella, Exhortation — these
/// arrive as [`SectionType::Custom`]). Prefer
/// [`midi_note_for_section`], which is exhaustive over the real enum.
#[must_use]
pub fn get_midi_note_for_section_type(section_type: &str) -> Option<u8> {
    Some(match section_type {
        "Verse" => 84,                       // C6
        "Chorus" => 85,                      // C#6
        "Bridge" => 86,                      // D6
        "Intro" => 87,                       // D#6
        "Outro" => 88,                       // E6
        "Instrumental" => 89,                // F6
        "Pre Chorus" | "Pre-Chorus" => 90,   // F#6
        "Post Chorus" | "Post-Chorus" => 91, // G6
        "Breakdown" => 92,                   // G#6
        "Interlude" => 93,                   // A6
        "Tag" => 94,                         // A#6
        "Ending" => 95,                      // B6
        "Solo" => 96,                        // C7
        "Vamp" => 97,                        // C#7
        "Turnaround" => 98,                  // D7
        "Refrain" => 99,                     // D#7
        "Rap" => 100,                        // E7
        "Acapella" => 101,                   // F7
        "Exhortation" => 102,                // F#7
        _ => return None,                    // Unknown section type
    })
}

/// Map a [`SectionType`] to its MIDI note.
///
/// Exhaustive over the enum rather than matching on a rendered name:
/// `SectionType` is the canonical section vocabulary (defined in
/// `keyflow`, re-exported by `session_proto`, used everywhere), so
/// stringly-matching it here would silently miss `Pre`/`Post` spellings
/// and drift the moment a variant is added.
///
/// `None` means "no note for this" and the cue is simply not stamped —
/// the audio cue still plays. `CountIn` is deliberately `None`: count-ins
/// go to the Count track, not the Guide track.
#[must_use]
pub fn midi_note_for_section(section_type: &SectionType) -> Option<u8> {
    Some(match section_type {
        SectionType::Verse => 84,
        SectionType::Chorus => 85,
        SectionType::Bridge => 86,
        SectionType::Intro => 87,
        SectionType::Outro => 88,
        SectionType::Instrumental => 89,
        SectionType::Breakdown => 92,
        SectionType::Interlude => 93,
        // The legacy table called this "Ending".
        SectionType::End => 95,
        SectionType::Solo => 96,
        SectionType::Vamp => 97,
        SectionType::Turnaround => 98,
        SectionType::Refrain => 99,
        // The legacy table's "Tag" sample, now that Tag is a variant.
        SectionType::Tag => 94,
        // Pre-/Post- have their own notes only for Chorus, which is all
        // the legacy plugin ever had; anything else takes the inner
        // section's note so a Pre-Verse still announces as a Verse.
        SectionType::Pre(inner) => match **inner {
            SectionType::Chorus => 90,
            ref other => return midi_note_for_section(other),
        },
        SectionType::Post(inner) => match **inner {
            SectionType::Chorus => 91,
            ref other => return midi_note_for_section(other),
        },
        // Names the enum doesn't model (Rap, Acapella, Exhortation)
        // still resolve through the legacy table.
        SectionType::Custom(name) => return get_midi_note_for_section_type(name),
        // Count-ins are Count-track material; Opening and Hits had no
        // note in the legacy layout and inventing one would put an
        // unexpected trigger into someone's sampler.
        SectionType::CountIn | SectionType::Opening | SectionType::Hits => return None,
    })
}

// ── Rendering the guide as MIDI ─────────────────────────────────────────
//
// The authoring counterpart to the playback engine: same schedule, same
// note layout, emitted as notes a DAW can hold instead of PCM mixed into
// a buffer. Pure — no DAW types, no I/O — so it is testable without
// REAPER, and so the host owns *where* the notes go.

use crate::schedule::{CueEvent, CueSchedule, GuideSection, GuideSongTiming, ScheduleOptions};
use session_proto::{GuideTrackRole, SectionType};

/// One note to stamp, on the track its role names.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideMidiNote {
    /// Which guide track this belongs on.
    pub role: GuideTrackRole,
    pub time_seconds: f64,
    pub length_seconds: f64,
    pub pitch: u8,
    pub velocity: u8,
}

/// The velocity a count note carries when a spoken cue lands on it.
///
/// Both notes stay in the MIDI; the cue takes the count's place in the
/// AUDIO, so muting the Guide track brings the count back. This is how the
/// Count track's instrument knows which of its notes to give way — a mark
/// in its own notes, which it can read in any track order.
pub const COUNT_UNDER_CUE_VELOCITY: u8 = 1;

/// Mark every Count note that a Guide cue lands on (within a
/// millisecond) with [`COUNT_UNDER_CUE_VELOCITY`].
pub fn mark_counts_under_cues(notes: &mut [GuideMidiNote]) {
    let cues: Vec<f64> = notes
        .iter()
        .filter(|n| n.role == GuideTrackRole::Guide)
        .map(|n| n.time_seconds)
        .collect();
    for note in notes.iter_mut().filter(|n| n.role == GuideTrackRole::Count) {
        if cues.iter().any(|at| (at - note.time_seconds).abs() < 0.001) {
            note.velocity = COUNT_UNDER_CUE_VELOCITY;
        }
    }
}

/// A constant-tempo stretch of the timeline.
///
/// The click grid is the one part of the guide that isn't in
/// [`CueSchedule`] — the engine's `click_player` generates it live from
/// the host's block clock, so there was never a precomputed version.
/// Taking segments rather than a single tempo means a caller holding a
/// real tempo map (the REAPER action does) gets a correct click across
/// tempo and time-signature changes, while a caller that only has a
/// song's nominal tempo can pass [`TempoSegment::from_timing`].
#[derive(Debug, Clone, PartialEq)]
pub struct TempoSegment {
    pub start_seconds: f64,
    pub tempo_bpm: f64,
    pub time_sig_num: u32,
    pub time_sig_den: u32,
}

impl TempoSegment {
    /// The whole span as one segment at a song's nominal tempo — what the
    /// guide engine itself assumes.
    #[must_use]
    pub const fn from_timing(timing: &GuideSongTiming, start_seconds: f64) -> Self {
        Self {
            start_seconds,
            tempo_bpm: timing.tempo_bpm,
            time_sig_num: timing.time_sig_num,
            time_sig_den: timing.time_sig_den,
        }
    }

    /// Seconds per beat, where "beat" is one denominator unit.
    fn beat_seconds(&self) -> f64 {
        (60.0 / self.tempo_bpm) * (4.0 / f64::from(self.time_sig_den))
    }
}

/// How finely to lay down the click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickSubdivision {
    /// Eighths below [`AUTO_EIGHTHS_BELOW_BPM`], one per beat at or above
    /// it — decided per tempo segment, so a song that speeds up past the
    /// line drops to quarters there. A slow song's beats are too far apart
    /// to lock to on their own.
    #[default]
    Auto,
    /// One note per beat. Downbeats accent.
    Beat,
    Eighth,
    Sixteenth,
    Triplet,
}

/// Below this tempo the automatic click plays eighths.
pub const AUTO_EIGHTHS_BELOW_BPM: f64 = 75.0;

impl ClickSubdivision {
    /// What this subdivision plays at `tempo_bpm` — only [`Self::Auto`]
    /// depends on it.
    #[must_use]
    pub fn at_tempo(self, tempo_bpm: f64) -> Self {
        match self {
            Self::Auto if tempo_bpm < AUTO_EIGHTHS_BELOW_BPM => Self::Eighth,
            Self::Auto => Self::Beat,
            fixed => fixed,
        }
    }

    /// How many notes per beat, and the note to use for the off-positions.
    const fn divisions(self) -> (u32, u8) {
        match self {
            Self::Auto | Self::Beat => (1, MIDI_NOTE_CLICK_BEAT),
            Self::Eighth => (2, MIDI_NOTE_CLICK_EIGHTH),
            Self::Sixteenth => (4, MIDI_NOTE_CLICK_SIXTEENTH),
            Self::Triplet => (3, MIDI_NOTE_CLICK_TRIPLET),
        }
    }
}

/// Note length and velocity for stamped notes. Guide/count notes are
/// triggers, not sustained material, so they're short by default.
const TRIGGER_LENGTH_SECONDS: f64 = 0.1;
const ACCENT_VELOCITY: u8 = 112;
const NORMAL_VELOCITY: u8 = 96;

/// Lay down the click grid over `[start, end)`.
///
/// Beat 1 of each measure gets [`MIDI_NOTE_CLICK_ACCENT`]; other beats
/// get the beat note; subdivisions between beats get the subdivision's
/// own note. The measure count restarts at each segment boundary, which
/// is what a time-signature change means.
#[must_use]
pub fn click_notes(
    segments: &[TempoSegment],
    end_seconds: f64,
    subdivision: ClickSubdivision,
) -> Vec<GuideMidiNote> {
    let mut notes = Vec::new();

    for (i, segment) in segments.iter().enumerate() {
        let (per_beat, off_pitch) = subdivision.at_tempo(segment.tempo_bpm).divisions();
        let segment_end = segments
            .get(i.saturating_add(1))
            .map_or(end_seconds, |next| next.start_seconds)
            .min(end_seconds);
        if segment_end <= segment.start_seconds {
            continue;
        }

        let beat = segment.beat_seconds();
        if beat <= 0.0 {
            continue;
        }
        let step = beat / f64::from(per_beat);

        let mut index: u64 = 0;
        loop {
            let index_f = f64::from(u32::try_from(index).unwrap_or(u32::MAX));
            let time = step.mul_add(index_f, segment.start_seconds);
            if time >= segment_end {
                break;
            }
            let on_beat = index.is_multiple_of(u64::from(per_beat));
            let beat_index = index.checked_div(u64::from(per_beat)).unwrap_or(0);
            let downbeat = on_beat && beat_index.is_multiple_of(u64::from(segment.time_sig_num));

            notes.push(GuideMidiNote {
                role: GuideTrackRole::Click,
                time_seconds: time,
                length_seconds: step.min(TRIGGER_LENGTH_SECONDS),
                pitch: if downbeat {
                    MIDI_NOTE_CLICK_ACCENT
                } else if on_beat {
                    MIDI_NOTE_CLICK_BEAT
                } else {
                    off_pitch
                },
                velocity: if downbeat {
                    ACCENT_VELOCITY
                } else {
                    NORMAL_VELOCITY
                },
            });
            index = index.saturating_add(1);
        }
    }
    notes
}

/// Turn a built schedule's cues into Count- and Guide-track notes.
///
/// Nothing is recomputed here — the timing is whatever
/// [`CueSchedule::build`] decided, so the stamped MIDI lines up with what
/// the engine plays by construction rather than by two implementations
/// agreeing.
#[must_use]
pub fn cue_notes(schedule: &CueSchedule) -> Vec<GuideMidiNote> {
    schedule
        .cues
        .iter()
        .filter_map(|cue| match &cue.event {
            CueEvent::Count { index } => Some(GuideMidiNote {
                role: GuideTrackRole::Count,
                time_seconds: cue.time_seconds,
                length_seconds: TRIGGER_LENGTH_SECONDS,
                pitch: *MIDI_NOTES_COUNT.get(*index)?,
                velocity: NORMAL_VELOCITY,
            }),
            CueEvent::Guide { section_type, .. } => Some(GuideMidiNote {
                role: GuideTrackRole::Guide,
                time_seconds: cue.time_seconds,
                length_seconds: TRIGGER_LENGTH_SECONDS,
                pitch: midi_note_for_section(section_type.as_ref()?)?,
                velocity: NORMAL_VELOCITY,
            }),
        })
        .collect()
}

/// Everything: click grid plus count and guide cues, sorted by time.
///
/// `tempo` drives only the click; the cues come from `sections`/`timing`
/// via [`CueSchedule::build`]. Pass `&[]` for `tempo` to skip the click
/// track entirely.
#[must_use]
pub fn guide_midi(
    sections: &[GuideSection],
    timing: &GuideSongTiming,
    options: &ScheduleOptions,
    tempo: &[TempoSegment],
    end_seconds: f64,
    subdivision: ClickSubdivision,
) -> Vec<GuideMidiNote> {
    let schedule = CueSchedule::build(sections, timing, options);
    let mut notes = click_notes(tempo, end_seconds, subdivision);
    notes.extend(cue_notes(&schedule));
    notes.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, bpm: f64, num: u32, den: u32) -> TempoSegment {
        TempoSegment {
            start_seconds: start,
            tempo_bpm: bpm,
            time_sig_num: num,
            time_sig_den: den,
        }
    }

    #[test]
    fn click_accents_every_downbeat() {
        // 120bpm 4/4 → 0.5s per beat, 2s per measure. Two measures.
        let notes = click_notes(&[seg(0.0, 120.0, 4, 4)], 4.0, ClickSubdivision::Beat);

        assert_eq!(notes.len(), 8);
        let accents: Vec<f64> = notes
            .iter()
            .filter(|n| n.pitch == MIDI_NOTE_CLICK_ACCENT)
            .map(|n| n.time_seconds)
            .collect();
        assert_eq!(accents, vec![0.0, 2.0]);
        assert!(notes.iter().all(|n| n.role == GuideTrackRole::Click));
    }

    #[test]
    fn subdivisions_fill_between_beats_without_stealing_the_accent() {
        let notes = click_notes(&[seg(0.0, 120.0, 4, 4)], 2.0, ClickSubdivision::Eighth);

        assert_eq!(notes.len(), 8, "4 beats x 2 = 8 eighths in one measure");
        assert_eq!(notes[0].pitch, MIDI_NOTE_CLICK_ACCENT);
        assert_eq!(notes[1].pitch, MIDI_NOTE_CLICK_EIGHTH);
        assert_eq!(notes[2].pitch, MIDI_NOTE_CLICK_BEAT);
    }

    /// A time-signature change restarts the measure, so the accent lands
    /// on the new bar rather than continuing the old cycle.
    #[test]
    fn segment_boundary_restarts_the_measure() {
        let notes = click_notes(
            &[seg(0.0, 120.0, 4, 4), seg(2.0, 120.0, 3, 4)],
            5.0,
            ClickSubdivision::Beat,
        );

        let accents: Vec<f64> = notes
            .iter()
            .filter(|n| n.pitch == MIDI_NOTE_CLICK_ACCENT)
            .map(|n| n.time_seconds)
            .collect();
        assert_eq!(accents, vec![0.0, 2.0, 3.5], "3/4 bar is 1.5s");
    }

    #[test]
    fn denominator_scales_the_beat() {
        // 6/8 at 120bpm: an eighth-note beat is 0.25s.
        let notes = click_notes(&[seg(0.0, 120.0, 6, 8)], 1.5, ClickSubdivision::Beat);
        assert_eq!(notes.len(), 6);
        assert!((notes[1].time_seconds - 0.25).abs() < 1e-10);
    }

    #[test]
    fn no_tempo_segments_means_no_click() {
        assert!(click_notes(&[], 10.0, ClickSubdivision::Beat).is_empty());
    }

    #[test]
    fn the_automatic_click_plays_eighths_below_75_and_quarters_from_it() {
        // 68 bpm for 4 beats, then 120 bpm for 4 beats.
        let slow_beat = 60.0 / 68.0;
        let slow_end = 4.0 * slow_beat;
        let notes = click_notes(
            &[seg(0.0, 68.0, 4, 4), seg(slow_end, 120.0, 4, 4)],
            slow_end + 2.0,
            ClickSubdivision::Auto,
        );
        let slow: Vec<_> = notes
            .iter()
            .filter(|n| n.time_seconds < slow_end - 1e-9)
            .collect();
        let fast: Vec<_> = notes
            .iter()
            .filter(|n| n.time_seconds >= slow_end - 1e-9)
            .collect();
        assert_eq!(slow.len(), 8, "eighths at 68");
        assert_eq!(slow[1].pitch, MIDI_NOTE_CLICK_EIGHTH);
        assert_eq!(fast.len(), 4, "quarters at 120");
        assert!(fast.iter().all(|n| n.pitch != MIDI_NOTE_CLICK_EIGHTH));
        // The line itself is quarters.
        assert_eq!(
            ClickSubdivision::Auto.at_tempo(75.0),
            ClickSubdivision::Beat
        );
        assert_eq!(
            ClickSubdivision::Auto.at_tempo(74.9),
            ClickSubdivision::Eighth
        );
    }

    #[test]
    fn cues_become_count_and_guide_notes_on_their_own_tracks() {
        let schedule = CueSchedule {
            cues: vec![
                crate::schedule::ScheduledCue {
                    time_seconds: 1.0,
                    event: CueEvent::Count { index: 0 },
                },
                crate::schedule::ScheduledCue {
                    time_seconds: 2.0,
                    event: CueEvent::Guide {
                        keys: vec!["Chorus_1".into()],
                        section_type: Some(SectionType::Chorus),
                    },
                },
            ],
        };

        let notes = cue_notes(&schedule);

        assert_eq!(notes[0].role, GuideTrackRole::Count);
        assert_eq!(notes[0].pitch, MIDI_NOTES_COUNT[0]);
        assert_eq!(notes[1].role, GuideTrackRole::Guide);
        assert_eq!(
            notes[1].pitch,
            midi_note_for_section(&SectionType::Chorus).unwrap()
        );
    }

    /// A free-form spoken cue has no section behind it, and an unknown
    /// section type has no note. Neither should be stamped — silently
    /// dropping is right here, since the audio cue still plays.
    #[test]
    fn guide_cues_without_a_mappable_section_are_skipped() {
        let schedule = CueSchedule {
            cues: vec![
                crate::schedule::ScheduledCue {
                    time_seconds: 1.0,
                    event: CueEvent::Guide {
                        keys: vec!["tts:watch me".into()],
                        section_type: None,
                    },
                },
                crate::schedule::ScheduledCue {
                    time_seconds: 2.0,
                    event: CueEvent::Guide {
                        keys: vec![],
                        section_type: Some(SectionType::Custom("Kazoo Solo".into())),
                    },
                },
            ],
        };

        assert!(cue_notes(&schedule).is_empty());
    }
}

#[cfg(test)]
mod section_note_tests {
    use super::*;

    /// The mapping is over `SectionType` — the canonical vocabulary from
    /// keyflow that the whole app already shares — not over a rendered
    /// name. A new variant is a compile error here rather than a section
    /// that silently stops announcing.
    #[test]
    fn pre_and_post_chorus_have_their_own_notes() {
        assert_eq!(
            midi_note_for_section(&SectionType::Pre(Box::new(SectionType::Chorus))),
            Some(90)
        );
        assert_eq!(
            midi_note_for_section(&SectionType::Post(Box::new(SectionType::Chorus))),
            Some(91)
        );
    }

    /// Pre-/Post- of anything else falls back to the inner section, so a
    /// Pre-Verse still announces rather than going silent.
    #[test]
    fn pre_of_other_sections_falls_back_to_the_inner_note() {
        assert_eq!(
            midi_note_for_section(&SectionType::Pre(Box::new(SectionType::Verse))),
            midi_note_for_section(&SectionType::Verse)
        );
    }

    /// Names the enum doesn't model still resolve through the legacy table.
    #[test]
    fn custom_sections_use_the_legacy_name_table() {
        assert_eq!(
            midi_note_for_section(&SectionType::Custom("Tag".into())),
            Some(94)
        );
        assert_eq!(
            midi_note_for_section(&SectionType::Custom("Nope".into())),
            None
        );
    }

    /// Count-ins belong on the Count track; putting them on Guide too
    /// would double-trigger.
    #[test]
    fn count_in_has_no_guide_note() {
        assert_eq!(midi_note_for_section(&SectionType::CountIn), None);
    }

    /// `CueSchedule` stores what `SectionType::parse` makes of a section's
    /// name, so the names the guide crate carries must round-trip.
    #[test]
    fn section_names_round_trip_through_parse() {
        for ty in [
            SectionType::Verse,
            SectionType::Chorus,
            SectionType::Bridge,
            SectionType::Intro,
            SectionType::Outro,
        ] {
            assert_eq!(
                SectionType::parse(&ty.full_name()).as_ref(),
                Ok(&ty),
                "{} should round-trip",
                ty.full_name()
            );
        }
    }
}

// ── Reading the guide back from MIDI ────────────────────────────────────

/// Which sound a MIDI note triggers — the inverse of the note layout
/// above.
///
/// This is what makes a stamped guide track playable *by the plugin*
/// rather than only by an external sampler: the host sends the track's
/// notes in, this resolves each to a [`GuideTrigger`], and the engine
/// mixes it. The mapping is one table, used in both directions, so a
/// track this crate stamps is a track it can read back.
///
/// Returns `None` for notes outside the layout, so an unrelated MIDI
/// track routed in by accident stays silent rather than firing arbitrary
/// cues.
#[must_use]
pub fn trigger_for_midi_note(note: u8) -> Option<crate::GuideTrigger> {
    use crate::GuideTrigger;

    if let Some(index) = MIDI_NOTES_COUNT.iter().position(|n| *n == note) {
        return Some(GuideTrigger::Count(index));
    }
    Some(match note {
        MIDI_NOTE_CLICK_ACCENT => GuideTrigger::Accent,
        MIDI_NOTE_CLICK_BEAT => GuideTrigger::Beat,
        MIDI_NOTE_CLICK_EIGHTH => GuideTrigger::Eighth,
        MIDI_NOTE_CLICK_SIXTEENTH => GuideTrigger::Sixteenth,
        MIDI_NOTE_CLICK_TRIPLET => GuideTrigger::Triplet,
        _ => {
            // Section announcement: resolve the note to a section type and
            // ask for that section's guide sample by the bank's own key.
            let section = section_for_midi_note(note)?;
            GuideTrigger::Guide(crate::get_guide_key(&section.full_name(), None))
        }
    })
}

/// The section type a Guide-track note stands for — inverse of
/// [`midi_note_for_section`].
#[must_use]
pub fn section_for_midi_note(note: u8) -> Option<SectionType> {
    Some(match note {
        84 => SectionType::Verse,
        85 => SectionType::Chorus,
        86 => SectionType::Bridge,
        87 => SectionType::Intro,
        88 => SectionType::Outro,
        89 => SectionType::Instrumental,
        90 => SectionType::Pre(Box::new(SectionType::Chorus)),
        91 => SectionType::Post(Box::new(SectionType::Chorus)),
        92 => SectionType::Breakdown,
        93 => SectionType::Interlude,
        94 => SectionType::Custom("Tag".to_string()),
        95 => SectionType::End,
        96 => SectionType::Solo,
        97 => SectionType::Vamp,
        98 => SectionType::Turnaround,
        99 => SectionType::Refrain,
        100 => SectionType::Custom("Rap".to_string()),
        101 => SectionType::Custom("Acapella".to_string()),
        102 => SectionType::Custom("Exhortation".to_string()),
        _ => return None,
    })
}

/// Human-readable name for a note in the guide layout, for a host's
/// piano roll ("Chorus" rather than "C6").
///
/// Every note the layout uses has one; anything else is `None`.
#[must_use]
pub fn note_name(note: u8) -> Option<String> {
    if let Some(index) = MIDI_NOTES_COUNT.iter().position(|n| *n == note) {
        return Some(format!("Count {}", index.saturating_add(1)));
    }
    Some(match note {
        MIDI_NOTE_CLICK_ACCENT => "Click: Accent".to_string(),
        MIDI_NOTE_CLICK_BEAT => "Click: Beat".to_string(),
        MIDI_NOTE_CLICK_EIGHTH => "Click: 8th".to_string(),
        MIDI_NOTE_CLICK_SIXTEENTH => "Click: 16th".to_string(),
        MIDI_NOTE_CLICK_TRIPLET => "Click: Triplet".to_string(),
        _ => section_for_midi_note(note)?.full_name(),
    })
}

/// Every named note in the layout, low to high. What a host's note-name
/// list wants.
#[must_use]
pub fn note_names() -> Vec<(u8, String)> {
    (0u8..=127)
        .filter_map(|note| note_name(note).map(|name| (note, name)))
        .collect()
}

#[cfg(test)]
mod midi_input_tests {
    use super::*;
    use crate::GuideTrigger;

    /// The layout is one table read in both directions: anything this
    /// crate stamps, it must be able to read back.
    #[test]
    fn every_stamped_section_note_maps_back_to_a_trigger() {
        for ty in [
            SectionType::Verse,
            SectionType::Chorus,
            SectionType::Bridge,
            SectionType::Intro,
            SectionType::Outro,
            SectionType::Instrumental,
            SectionType::Breakdown,
            SectionType::Interlude,
            SectionType::End,
            SectionType::Solo,
            SectionType::Vamp,
            SectionType::Turnaround,
            SectionType::Refrain,
            SectionType::Pre(Box::new(SectionType::Chorus)),
            SectionType::Post(Box::new(SectionType::Chorus)),
        ] {
            let note = midi_note_for_section(&ty).expect("has a note");
            assert_eq!(
                section_for_midi_note(note).as_ref(),
                Some(&ty),
                "note {note} should map back to {}",
                ty.full_name()
            );
            assert!(matches!(
                trigger_for_midi_note(note),
                Some(GuideTrigger::Guide(_))
            ));
        }
    }

    #[test]
    fn click_and_count_notes_map_to_their_triggers() {
        assert_eq!(
            trigger_for_midi_note(MIDI_NOTE_CLICK_ACCENT),
            Some(GuideTrigger::Accent)
        );
        assert_eq!(
            trigger_for_midi_note(MIDI_NOTE_CLICK_BEAT),
            Some(GuideTrigger::Beat)
        );
        assert_eq!(
            trigger_for_midi_note(MIDI_NOTES_COUNT[3]),
            Some(GuideTrigger::Count(3))
        );
    }

    /// An unrelated MIDI track routed in by accident must stay silent
    /// rather than firing arbitrary cues.
    #[test]
    fn notes_outside_the_layout_are_ignored() {
        assert_eq!(trigger_for_midi_note(0), None);
        assert_eq!(trigger_for_midi_note(64), None);
        assert_eq!(trigger_for_midi_note(127), None);
        assert_eq!(note_name(64), None);
    }

    #[test]
    fn every_named_note_is_playable_and_vice_versa() {
        for (note, _) in note_names() {
            assert!(trigger_for_midi_note(note).is_some(), "note {note}");
        }
    }
}

#[cfg(test)]
mod note_name_list_tests {
    use super::*;

    /// `every_named_note_is_playable_and_vice_versa` iterates `note_names()`
    /// and would pass vacuously on an empty list — which is exactly the
    /// failure mode that matters, since an empty list means the host shows
    /// nothing and everything still "works". Pin the contents.
    #[test]
    fn note_names_is_not_empty_and_covers_each_family() {
        let names = note_names();
        assert!(!names.is_empty(), "note_names() returned nothing");

        let by_note: std::collections::HashMap<u8, String> = names.into_iter().collect();
        assert_eq!(
            by_note.get(&MIDI_NOTE_CLICK_ACCENT).map(String::as_str),
            Some("Click: Accent")
        );
        assert_eq!(
            by_note.get(&MIDI_NOTES_COUNT[0]).map(String::as_str),
            Some("Count 1")
        );
        assert_eq!(by_note.get(&85).map(String::as_str), Some("Chorus"));
        assert!(
            by_note.len() >= 20,
            "expected the full layout, got {}",
            by_note.len()
        );
    }
}
