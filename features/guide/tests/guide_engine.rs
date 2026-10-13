//! Unit tests for the ported count-in logic plus scheduling / render
//! smoke tests. (The legacy crate only had REAPER integration tests;
//! these pin down the behavior of the verbatim-ported pattern code.)

use session_guide::count_in::{CountInCalculator, CountInPattern};
use session_guide::{
    tts_cue_key, AudioSample, BlockClock, ClickSubdivisions, CountInOptions, CueEvent, CueSchedule,
    GuideConfig, GuideEngine, GuideSection, GuideSongTiming, ScheduleOptions,
};

/// `x` as `f64`, for the small (sub-second-sample-count) values used in
/// this file's block-rendering loops — never large enough to lose
/// precision, but `usize`→`f64` has no non-`as` conversion in `std`.
fn f64_from_usize(x: usize) -> f64 {
    u32::try_from(x).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

/// Exact-value float assertion helpers: the samples compared in these
/// tests are discrete impulses written verbatim (no summation or scaling
/// that would introduce rounding error), so an epsilon compare is exact
/// in practice while staying off `clippy::float_cmp`.
fn f_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

fn f_eq64(a: f64, b: f64) -> bool {
    (a - b).abs() < f64::EPSILON
}

// ─── CountInCalculator ──────────────────────────────────────────────────

#[test]
fn calculator_rounds_to_nearest_measure_and_clamps() {
    // 2 measures of 4/4 = 8 quarters
    assert_eq!(
        CountInCalculator::calculate_count_in_measures(0.0, 8.0, 4.0),
        2
    );
    // Slightly off distances round to the nearest measure
    assert_eq!(
        CountInCalculator::calculate_count_in_measures(0.0, 7.9, 4.0),
        2
    );
    assert_eq!(
        CountInCalculator::calculate_count_in_measures(0.0, 6.1, 4.0),
        2
    );
    // Clamped to 1..=8
    assert_eq!(
        CountInCalculator::calculate_count_in_measures(0.0, 0.5, 4.0),
        1
    );
    assert_eq!(
        CountInCalculator::calculate_count_in_measures(0.0, 400.0, 4.0),
        8
    );
}

// ─── CountInPattern ─────────────────────────────────────────────────────

fn pattern_for(total_measures: i32, num: i32, den: i32) -> Vec<Vec<Option<i32>>> {
    (0..total_measures)
        .map(|m| {
            (1..=num)
                .map(|b| CountInPattern::should_count(m, b, total_measures, num, den, true, true))
                .collect()
        })
        .collect()
}

#[test]
fn one_measure_4_4_counts_all_beats() {
    assert_eq!(
        pattern_for(1, 4, 4),
        vec![vec![Some(1), Some(2), Some(3), Some(4)]]
    );
}

#[test]
fn two_measures_4_4_half_then_full() {
    // Measure 1: "1 _ 2 _", measure 2: full count
    assert_eq!(
        pattern_for(2, 4, 4),
        vec![
            vec![Some(1), None, Some(2), None],
            vec![Some(1), Some(2), Some(3), Some(4)],
        ]
    );
}

#[test]
fn four_measures_4_4_extended_pattern() {
    // Measures 1-2: beat 1 only (numbered 1, 2); measure 3: half; measure 4: full
    assert_eq!(
        pattern_for(4, 4, 4),
        vec![
            vec![Some(1), None, None, None],
            vec![Some(2), None, None, None],
            vec![Some(1), None, Some(2), None],
            vec![Some(1), Some(2), Some(3), Some(4)],
        ]
    );
}

#[test]
fn one_measure_6_8_counts_all_beats() {
    // 6/8 is NOT split (numerator not > 6, denominator not 16)
    assert_eq!(
        pattern_for(1, 6, 8),
        vec![vec![Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)]]
    );
}

#[test]
fn one_measure_9_8_splits_into_groups_ending_with_4() {
    // 9/8: last group is beats 6-9 counted 1-4; earlier group (beats 1-5)
    // counted 1-5 when full_count_odd_time is on.
    assert_eq!(
        pattern_for(1, 9, 8),
        vec![vec![
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
        ]]
    );
}

#[test]
fn one_measure_9_8_without_full_count_falls_back_to_standard_count() {
    // Legacy behavior (ported verbatim): when full_count_odd_time is off,
    // calculate_odd_time_count returns None for early-group beats, and
    // should_count then falls through to the STANDARD full count — the
    // early beats still count 1..=9 rather than being silent.
    let row: Vec<Option<i32>> = (1..=9)
        .map(|b| CountInPattern::should_count(0, b, 1, 9, 8, true, false))
        .collect();
    assert_eq!(
        row,
        vec![
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(1),
            Some(2),
            Some(3),
            Some(4)
        ]
    );
}

#[test]
fn sixteenth_denominator_single_measure_has_no_counts() {
    assert_eq!(pattern_for(1, 7, 16), vec![vec![None; 7]]);
}

#[test]
fn sixteenth_denominator_multi_measure_counts_down_measures() {
    // 3 measures of 7/16 with offset_by_one: beat 1 of each measure,
    // numbered total - index - 1 = 2, 1, 0(dropped)
    let p = pattern_for(3, 7, 16);
    assert_eq!(p[0][0], Some(2));
    assert_eq!(p[1][0], Some(1));
    assert_eq!(p[2][0], None); // count 0 is out of range 1..=8
    for row in &p {
        assert!(row[1..].iter().all(Option::is_none));
    }
}

// ─── CueSchedule ────────────────────────────────────────────────────────

fn section(start: f64, end: f64, name: &str, type_name: &str) -> GuideSection {
    GuideSection {
        start_seconds: start,
        end_seconds: end,
        name: name.to_string(),
        count_in_position: None,
        song_end_position: None,
        is_first_section: false,
        section_type_name: type_name.to_string(),
        section_number: None,
        spoken_note: None,
    }
}

#[test]
fn schedule_one_measure_count_in_at_120_bpm() {
    // 120 bpm 4/4: beat = 0.5 s, measure = 2 s. Count-in marker 2 s before
    // the section → 1 measure → counts "1 2 3 4" at 8.0, 8.5, 9.0, 9.5.
    let mut first = section(10.0, 20.0, "Verse 1", "Verse");
    first.is_first_section = true;
    first.count_in_position = Some(8.0);

    let timing = GuideSongTiming {
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let options = ScheduleOptions {
        guide_replace_beat1: false,
        extend_songend_count: false,
        ..Default::default()
    };
    let schedule = CueSchedule::build(&[first], &timing, &options);

    let counts: Vec<(f64, usize)> = schedule
        .cues
        .iter()
        .filter_map(|c| match &c.event {
            CueEvent::Count { index } => Some((c.time_seconds, *index)),
            CueEvent::Guide { .. } => None,
        })
        .collect();
    assert_eq!(counts, vec![(8.0, 0), (8.5, 1), (9.0, 2), (9.5, 3)]);

    // Guide announcement one measure (4 beats) before the section start.
    let guides: Vec<&f64> = schedule
        .cues
        .iter()
        .filter_map(|c| match &c.event {
            CueEvent::Guide { .. } => Some(&c.time_seconds),
            CueEvent::Count { .. } => None,
        })
        .collect();
    assert_eq!(guides, vec![&8.0]);
}

#[test]
fn guide_replaces_beat_one() {
    let mut first = section(10.0, 20.0, "Verse 1", "Verse");
    first.is_first_section = true;
    first.count_in_position = Some(8.0);

    let timing = GuideSongTiming {
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let options = ScheduleOptions {
        guide_replace_beat1: true,
        extend_songend_count: false,
        ..Default::default()
    };
    let schedule = CueSchedule::build(&[first], &timing, &options);

    // The count at 8.0 coincides with the guide cue and is dropped.
    let counts: Vec<f64> = schedule
        .cues
        .iter()
        .filter_map(|c| match &c.event {
            CueEvent::Count { .. } => Some(c.time_seconds),
            CueEvent::Guide { .. } => None,
        })
        .collect();
    assert_eq!(counts, vec![8.5, 9.0, 9.5]);
}

#[test]
fn songend_gets_two_measure_count_out_and_ending_guide() {
    let mut last = section(20.0, 30.0, "Outro", "Outro");
    last.song_end_position = Some(30.0);

    let timing = GuideSongTiming {
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let options = ScheduleOptions {
        guide_replace_beat1: false,
        // Isolate the SONGEND count-out from the per-section count-in.
        count_in: CountInOptions {
            count_into_sections: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let schedule = CueSchedule::build(&[last], &timing, &options);

    // 2-measure pattern into 30.0 starts at 26.0: "1 _ 2 _ | 1 2 3 4"
    let counts: Vec<(f64, usize)> = schedule
        .cues
        .iter()
        .filter_map(|c| match &c.event {
            CueEvent::Count { index } => Some((c.time_seconds, *index)),
            CueEvent::Guide { .. } => None,
        })
        .collect();
    assert_eq!(
        counts,
        vec![
            (26.0, 0),
            (27.0, 1),
            (28.0, 0),
            (28.5, 1),
            (29.0, 2),
            (29.5, 3),
        ]
    );

    // "Ending" announcement at the count-out start.
    assert!(schedule.cues.iter().any(|c| matches!(
        &c.event,
        CueEvent::Guide { keys, .. } if f_eq64(c.time_seconds, 26.0) && keys.contains(&"Ending_None".to_string())
    )));
}

#[test]
fn push_speak_inserts_sorted_tts_cue() {
    let mut schedule = CueSchedule::default();
    schedule.push_speak(5.0, "Bridge in 2");
    schedule.push_speak(1.0, "Chorus");
    assert!(f_eq64(schedule.cues[0].time_seconds, 1.0));
    assert_eq!(
        schedule.cues[1].event,
        CueEvent::Guide {
            keys: vec![tts_cue_key("Bridge in 2")],
            section_type: None,
        }
    );
}

// ─── Engine render smoke test ───────────────────────────────────────────

/// A 1-frame impulse sample makes trigger positions directly observable.
fn impulse(sample_rate: u32) -> AudioSample {
    AudioSample::mono(vec![1.0], sample_rate)
}

#[test]
fn engine_renders_clicks_and_cues_sample_accurately() {
    const SR: f64 = 1000.0; // 1 kHz keeps offsets human-readable
    const SR_U32: u32 = 1000;
    let mut engine = GuideEngine::new(GuideConfig {
        click: ClickSubdivisions {
            measure_accent: false,
            ..Default::default()
        },
        ..Default::default()
    });
    engine.bank_mut().beat = Some(impulse(SR_U32));
    engine.bank_mut().counts[0] = Some(impulse(SR_U32));
    engine.bank_mut().insert_guide("Verse_1", impulse(SR_U32));

    // One cue schedule: count "1" at 0.25 s, guide "Verse_1" at 0.75 s.
    let mut first = section(1.0, 2.0, "Verse 1", "Verse");
    first.section_number = Some(1);
    let mut schedule = CueSchedule::default();
    schedule.cues.push(session_guide::ScheduledCue {
        time_seconds: 0.25,
        event: CueEvent::Count { index: 0 },
    });
    schedule.cues.push(session_guide::ScheduledCue {
        time_seconds: 0.75,
        event: CueEvent::Guide {
            keys: vec!["Verse_1".to_string()],
            section_type: Some(session_proto::SectionType::Verse),
        },
    });
    engine.set_schedule(schedule);

    // 120 bpm → beat every 0.5 s → every 500 samples at 1 kHz.
    // Render 1 second in two 500-frame blocks.
    let mut click = vec![0.0f32; 1000];
    let mut count = vec![0.0f32; 1000];
    let mut guide = vec![0.0f32; 1000];
    for block in 0..2 {
        let start = block * 500;
        let block_clock = BlockClock {
            playing: true,
            pos_seconds: f64_from_usize(start) / SR,
            pos_beats: (f64_from_usize(start) / SR) * 2.0, // 120 bpm = 2 quarters/s
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
            sample_rate: SR,
        };
        let (cl, cr) = (&mut click[start..start + 500], &mut vec![0.0f32; 500]);
        let (nl, nr) = (&mut count[start..start + 500], &mut vec![0.0f32; 500]);
        let (gl, gr) = (&mut guide[start..start + 500], &mut vec![0.0f32; 500]);
        let mut buses = session_guide::GuideBuses {
            click_l: cl,
            click_r: cr,
            count_l: nl,
            count_r: nr,
            guide_l: gl,
            guide_r: gr,
        };
        engine.render(&mut buses, &block_clock);
    }

    // Beat clicks at samples 0 and 500 (0.0 s and 0.5 s), nowhere else.
    let click_hits: Vec<usize> = click
        .iter()
        .enumerate()
        .filter(|(_, v)| v.abs() > 0.0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(click_hits, vec![0, 500]);

    // Count "1" at sample 250.
    let count_hits: Vec<usize> = count
        .iter()
        .enumerate()
        .filter(|(_, v)| v.abs() > 0.0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(count_hits, vec![250]);

    // Guide at sample 750.
    let guide_hits: Vec<usize> = guide
        .iter()
        .enumerate()
        .filter(|(_, v)| v.abs() > 0.0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(guide_hits, vec![750]);
}

#[test]
fn engine_stops_triggering_when_not_playing() {
    const SR: f64 = 1000.0;
    const SR_U32: u32 = 1000;
    let mut engine = GuideEngine::default();
    engine.bank_mut().beat = Some(impulse(SR_U32));
    engine.bank_mut().measure_accent = Some(impulse(SR_U32));

    let mut l = vec![0.0f32; 500];
    let mut r = vec![0.0f32; 500];
    let clock = BlockClock {
        playing: false,
        pos_seconds: 0.0,
        pos_beats: 0.0,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
        sample_rate: SR,
    };
    engine.render_stereo(&mut l, &mut r, &clock);
    assert!(l.iter().all(|v| v.abs() <= 0.0));
}

// ─── SampleBank::synthesize_defaults ────────────────────────────────────

/// Peak absolute value across all channels of a sample.
fn peak(sample: &session_guide::AudioSample) -> f32 {
    sample
        .data
        .iter()
        .flat_map(|ch| ch.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()))
}

#[test]
fn synthesize_defaults_fills_all_slots_non_silent_and_bounded() {
    const SR: u32 = 48_000;
    let mut engine = GuideEngine::default();
    engine.bank_mut().synthesize_defaults(SR);
    let bank = engine.bank();

    let named = [
        ("beat", bank.beat.as_ref()),
        ("eighth", bank.eighth.as_ref()),
        ("sixteenth", bank.sixteenth.as_ref()),
        ("triplet", bank.triplet.as_ref()),
        ("accent", bank.measure_accent.as_ref()),
    ];
    for (name, slot) in named {
        let sample = slot.unwrap_or_else(|| panic!("{name} not synthesized"));
        let p = peak(sample);
        assert!(p > 0.05, "{name} is silent (peak {p})");
        assert!(p <= 1.0, "{name} exceeds full scale (peak {p})");
        assert_eq!(sample.sample_rate, SR);
        // Bounded length: every placeholder is well under half a second.
        let sr_usize = usize::try_from(SR).unwrap_or(usize::MAX);
        assert!(sample.frames() < sr_usize / 2, "{name} too long");
    }
    for (i, slot) in bank.counts.iter().enumerate() {
        let sample = slot.as_ref().unwrap_or_else(|| panic!("count {i} missing"));
        let p = peak(sample);
        assert!(p > 0.05 && p <= 1.0, "count {i} peak {p} out of range");
    }
    // Section-guide chimes are intentionally NOT synthesized: guide
    // announcements come from real recorded samples (load_guide_dir) or TTS,
    // so a zero-asset engine has no synthesized guides (an unmatched section
    // stays silent rather than emitting a noise chime).
    assert!(bank.guides.is_empty());
}

#[test]
fn synthesize_defaults_never_overwrites_loaded_samples() {
    const SR: u32 = 48_000;
    let mut engine = GuideEngine::default();
    engine.bank_mut().beat = Some(impulse(SR));
    engine.bank_mut().counts[0] = Some(impulse(SR));
    engine.bank_mut().insert_guide("Chorus_None", impulse(SR));
    engine.bank_mut().synthesize_defaults(SR);

    let bank = engine.bank();
    assert_eq!(bank.beat.as_ref().unwrap().frames(), 1);
    assert_eq!(bank.counts[0].as_ref().unwrap().frames(), 1);
    assert_eq!(bank.guides["Chorus_None"].frames(), 1);
    // ...while empty CLICK/COUNT slots were still filled. (Guides are never
    // synthesized, so no new guide keys appear — the preloaded one above is
    // simply left untouched.)
    assert!(bank.measure_accent.is_some());
    assert!(bank.counts[1].is_some());
}

#[test]
fn synthesized_bank_renders_audible_guide() {
    // End-to-end: a zero-asset engine + demo-like schedule produces
    // non-silent output through render_stereo.
    const SR: f64 = 48_000.0;
    const SR_U32: u32 = 48_000;
    let mut engine = GuideEngine::default();
    engine.bank_mut().synthesize_defaults(SR_U32);

    let mut first = section(4.0, 12.0, "Verse 1", "Verse");
    first.is_first_section = true;
    first.count_in_position = Some(0.0);
    first.section_number = Some(1);
    engine.set_sections(
        &[first],
        &GuideSongTiming {
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        },
    );

    let mut l = vec![0.0f32; 4096];
    let mut r = vec![0.0f32; 4096];
    let clock = BlockClock {
        playing: true,
        pos_seconds: 0.0,
        pos_beats: 0.0,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
        sample_rate: SR,
    };
    engine.render_stereo(&mut l, &mut r, &clock);
    let p = l.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(p > 0.05, "synthesized guide rendered silence (peak {p})");
    assert!(p <= 2.0, "synthesized guide clipped wildly (peak {p})");
}

#[test]
fn measure_accent_replaces_beat_one() {
    const SR: f64 = 1000.0;
    const SR_U32: u32 = 1000;
    let mut engine = GuideEngine::default();
    // Distinguish accent (amplitude 2.0) from beat (1.0).
    engine.bank_mut().beat = Some(impulse(SR_U32));
    engine.bank_mut().measure_accent = Some(AudioSample::mono(vec![2.0], SR_U32));

    // Render one 4/4 measure at 120 bpm = 2 s = 2000 samples.
    let mut l = vec![0.0f32; 2000];
    let mut r = vec![0.0f32; 2000];
    let clock = BlockClock {
        playing: true,
        pos_seconds: 0.0,
        pos_beats: 0.0,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
        sample_rate: SR,
    };
    engine.render_stereo(&mut l, &mut r, &clock);

    assert!(f_eq(l[0], 2.0)); // accent on beat 1
    assert!(f_eq(l[500], 1.0)); // plain beats 2-4
    assert!(f_eq(l[1000], 1.0));
    assert!(f_eq(l[1500], 1.0));
}

// ─── sections_from_song: count-in attaches to the first MUSICAL section ──
//
// Regression for the "announced the section but never counted 1-2-3-4" bug.
// `SongBuilder` prepends a synthetic `CountIn` section and sets the song's
// `start_seconds` to the count-in START, so the count-in must attach to the
// first non-CountIn section and count INTO its downbeat — not to the
// synthetic section at t≈0, where the whole count landed at negative time
// and got dropped.

fn count_in_song() -> session_proto::Song {
    use session_proto::{Section, SectionId, SectionType, Song, SongId};

    // 120 bpm, 4/4 → beat 0.5 s, measure 2.0 s. Count-in START at 0.0, a
    // 2-measure (4.0 s) count into the Intro downbeat at 4.0 s. This mirrors
    // demo song #1, which previously produced NO count audio.
    let section = |name: &str, ty: SectionType, start: f64, end: f64| Section {
        section_id: SectionId::default(),
        id: None,
        name: name.to_string(),
        comment: None,
        section_type: ty,
        start_seconds: start,
        end_seconds: end,
        number: None,
        color: None,
    };
    Song {
        id: SongId::default(),
        name: "Count-In Song".into(),
        project_guid: "guid-ci".into(),
        start_seconds: 0.0, // count-in START, per SongBuilder
        end_seconds: 36.0,
        count_in_seconds: Some(4.0),
        sections: vec![
            section("Count-In", SectionType::CountIn, 0.0, 4.0),
            section("Intro", SectionType::Intro, 4.0, 20.0),
            section("Verse 1", SectionType::Verse, 20.0, 36.0),
        ],
        comments: vec![],
        tempo: Some(120.0),
        time_signature: None, // GuideSongTiming defaults to 4/4
        measure_positions: vec![],
        chart_text: None,
        parsed_chart: None,
        detected_chords: vec![],
        chart_fingerprint: None,
        advance_mode: None,
        color: None,
    }
}

#[test]
fn count_in_attaches_to_first_musical_section() {
    let song = count_in_song();
    let sections = session_guide::sections_from_song(&song);

    // The synthetic count-in section gets NO count-in position…
    assert_eq!(sections[0].section_type_name, "Count-In"); // CountIn full_name
    assert_eq!(sections[0].count_in_position, None);
    // …the first musical section (Intro) carries it, anchored at the count
    // START (0.0), not one count-in duration earlier (the old −4.0 bug).
    assert_eq!(sections[1].name, "Intro");
    assert_eq!(sections[1].count_in_position, Some(0.0));
}

#[test]
fn count_in_song_announces_on_the_count_bars_one() {
    let song = count_in_song();
    let sections = session_guide::sections_from_song(&song);
    let timing = GuideSongTiming::from_song(&song);
    // Pins the "Intro, 2, 3, 4" count-IN layout in isolation:
    // disable the SONGEND count-out AND the per-section count-in (both emit
    // Count cues) so only the first section's explicit count-in remains.
    let options = ScheduleOptions {
        extend_songend_count: false,
        count_in: CountInOptions {
            count_into_sections: false,
            ..Default::default()
        },
        ..ScheduleOptions::default()
    };
    let schedule = CueSchedule::build(&sections, &timing, &options);

    let counts: Vec<(f64, usize)> = schedule
        .cues
        .iter()
        .filter_map(|c| match c.event {
            CueEvent::Count { index } => Some((c.time_seconds, index)),
            CueEvent::Guide { .. } => None,
        })
        .collect();

    // The count is the final measure into the 4.0 s downbeat, and the
    // section's name takes its "1": "Intro, 2, 3, 4" — beats 2-4 at
    // 2.5/3.0/3.5, the announcement on 2.0.
    assert_eq!(
        counts,
        vec![(2.5, 1), (3.0, 2), (3.5, 3)],
        "the count bar is the announcement, then 2 3 4"
    );

    // One measure (2.0 s at this tempo) before the 4.0 s downbeat.
    let intro_guide_time = schedule.cues.iter().find_map(|c| match &c.event {
        CueEvent::Guide { keys, .. } if keys.iter().any(|k| k.contains("Intro")) => {
            Some(c.time_seconds)
        }
        CueEvent::Guide { .. } | CueEvent::Count { .. } => None,
    });
    assert_eq!(
        intro_guide_time,
        Some(2.0),
        "Intro should be announced on the count bar's downbeat"
    );
}

// ─── count_into_sections + announcement dedup ───────────────────────────
#[test]
fn counts_into_each_new_section_and_dedupes_repeats() {
    // 120 bpm 4/4 → measure 2 s. Four sections, the middle two both "Chorus 3"
    // (a repeated/continued chorus). Every REAL change gets a "1 2 3 4" count
    // and one announcement; the continuation gets neither.
    let timing = GuideSongTiming {
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mk = |start: f64, end: f64, ty: &str, num: Option<u32>| {
        let mut s = section(start, end, ty, ty);
        s.section_number = num;
        s
    };
    let sections = vec![
        mk(8.0, 16.0, "Verse", Some(1)),   // Verse 1        (change)
        mk(16.0, 24.0, "Chorus", Some(3)), // Chorus 3       (change)
        mk(24.0, 32.0, "Chorus", Some(3)), // Chorus 3 again (continuation)
        mk(32.0, 40.0, "Verse", Some(1)),  // Verse 1        (change)
    ];
    let options = ScheduleOptions {
        guide_replace_beat1: false,
        extend_songend_count: false,
        ..Default::default()
    };
    let schedule = CueSchedule::build(&sections, &timing, &options);

    // Beat-1 of the count into each NEW section = one measure (2 s) before its
    // downbeat: 6.0 (Verse1), 14.0 (Chorus3), 30.0 (Verse1). NOT 22.0 — the
    // repeated Chorus 3 is a continuation, not a new count-in.
    let count_ones: Vec<f64> = schedule
        .cues
        .iter()
        .filter_map(|c| match c.event {
            CueEvent::Count { index: 0 } => Some(c.time_seconds),
            CueEvent::Count { .. } | CueEvent::Guide { .. } => None,
        })
        .collect();
    assert!(count_ones.contains(&6.0), "no count into Verse 1");
    assert!(count_ones.contains(&14.0), "no count into Chorus 3");
    assert!(
        count_ones.contains(&30.0),
        "no count into the returning Verse 1"
    );
    assert!(
        !count_ones.contains(&22.0),
        "the repeated Chorus 3 must NOT be counted into"
    );

    // Announcements only on a real change: Verse1, Chorus3, Verse1 = 3.
    let announcements = schedule
        .cues
        .iter()
        .filter(|c| matches!(c.event, CueEvent::Guide { .. }))
        .count();
    assert_eq!(announcements, 3, "repeated Chorus 3 should not re-announce");
}

// ─── Offline render against a real tempo map ────────────────────────────
//
// The engine is host-clocked: each block's `BlockClock` carries the
// transport position in seconds AND beats plus the tempo at block start,
// exactly how daw-standalone's aux hook derives it from the project
// tempo map (`RenderSnapshot::clock_info`). This test drives the engine
// through a tempo CHANGE and proves that click and count-in impulses
// land on the exact sample positions the map implies.

/// A two-segment tempo map: `bpm_a` until `change_seconds`, `bpm_b` after.
/// Mirrors the seconds→beats / tempo-at lookups the daw-standalone
/// render snapshot performs for the aux clock.
struct TwoTempoMap {
    bpm_a: f64,
    bpm_b: f64,
    change_seconds: f64,
}

impl TwoTempoMap {
    fn tempo_at(&self, seconds: f64) -> f64 {
        if seconds < self.change_seconds {
            self.bpm_a
        } else {
            self.bpm_b
        }
    }

    fn seconds_to_beat(&self, seconds: f64) -> f64 {
        if seconds < self.change_seconds {
            seconds * self.bpm_a / 60.0
        } else {
            self.change_seconds * self.bpm_a / 60.0
                + (seconds - self.change_seconds) * self.bpm_b / 60.0
        }
    }

    /// Timeline seconds of quarter-note `beat` (inverse of the above).
    fn beat_to_seconds(&self, beat: f64) -> f64 {
        let change_beat = self.change_seconds * self.bpm_a / 60.0;
        if beat <= change_beat {
            beat * 60.0 / self.bpm_a
        } else {
            self.change_seconds + (beat - change_beat) * 60.0 / self.bpm_b
        }
    }
}

#[test]
fn tempo_map_render_places_clicks_and_counts_sample_accurately() {
    const SR: f64 = 1000.0; // 1 kHz keeps sample offsets == milliseconds
    const SR_U32: u32 = 1000;
    const BLOCK: usize = 250;
    const TOTAL: usize = 6000; // 6 s

    // 120 bpm for the first 2 s (beats 0..=4), then 60 bpm.
    let map = TwoTempoMap {
        bpm_a: 120.0,
        bpm_b: 60.0,
        change_seconds: 2.0,
    };

    let mut engine = GuideEngine::new(GuideConfig {
        click: ClickSubdivisions {
            measure_accent: false, // plain beat everywhere: positions only
            ..Default::default()
        },
        ..Default::default()
    });
    engine.bank_mut().beat = Some(AudioSample::mono(vec![1.0], SR_U32));
    for i in 0..4 {
        engine.bank_mut().counts[i] = Some(AudioSample::mono(vec![1.0], SR_U32));
    }

    // Count-in "1 2 3 4" on beats 4..8 — i.e. into a section whose
    // downbeat is beat 8. Cue times come from the tempo map, exactly as a
    // setlist-build step (or the app's schedule rebuild) would compute them.
    let mut schedule = CueSchedule::default();
    for (i, beat) in (4..8).enumerate() {
        schedule.cues.push(session_guide::ScheduledCue {
            time_seconds: map.beat_to_seconds(f64::from(beat)),
            event: CueEvent::Count { index: i },
        });
    }
    engine.set_schedule(schedule);

    let mut click = vec![0.0f32; TOTAL];
    let mut count = vec![0.0f32; TOTAL];
    let mut guide = vec![0.0f32; TOTAL];
    for start in (0..TOTAL).step_by(BLOCK) {
        let pos_seconds = f64_from_usize(start) / SR;
        let block_clock = BlockClock {
            playing: true,
            pos_seconds,
            pos_beats: map.seconds_to_beat(pos_seconds),
            tempo_bpm: map.tempo_at(pos_seconds),
            time_sig_num: 4,
            time_sig_den: 4,
            sample_rate: SR,
        };
        let mut buses = session_guide::GuideBuses {
            click_l: &mut click[start..start + BLOCK],
            click_r: &mut vec![0.0f32; BLOCK],
            count_l: &mut count[start..start + BLOCK],
            count_r: &mut vec![0.0f32; BLOCK],
            guide_l: &mut guide[start..start + BLOCK],
            guide_r: &mut vec![0.0f32; BLOCK],
        };
        engine.render(&mut buses, &block_clock);
    }

    let hits = |buf: &[f32]| -> Vec<usize> {
        buf.iter()
            .enumerate()
            .filter(|(_, v)| v.abs() > 0.0)
            .map(|(i, _)| i)
            .collect()
    };

    // Quarter notes: every 0.5 s at 120 bpm, every 1.0 s at 60 bpm.
    // Beats 0..=4 → 0, 500, 1000, 1500, 2000; beats 5.. → 3000, 4000, 5000.
    assert_eq!(
        hits(&click),
        vec![0, 500, 1000, 1500, 2000, 3000, 4000, 5000],
        "click impulses off the tempo-map beat grid"
    );

    // Count-in on beats 4..8: samples 2000, 3000, 4000, 5000 (all in the
    // 60 bpm region — beat 4 IS the tempo change).
    assert_eq!(
        hits(&count),
        vec![2000, 3000, 4000, 5000],
        "count-in impulses off the tempo-map beat grid"
    );

    // Nothing scheduled on the guide bus.
    assert_eq!(hits(&guide), Vec::<usize>::new());
}
