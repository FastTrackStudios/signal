//! Integration test: .als → .rpp → .als round-trip
//!
//! Reads a real Ableton Live set, converts it to a REAPER project (RPP),
//! then converts the RPP back to an Ableton Live set and verifies that
//! the key properties are equivalent.

#![cfg(feature = "file")]

use dawfile_ableton::*;
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::*;
use dawfile_reaper::types::item::{MidiEventType, MidiSourceEvent};
use dawfile_reaper::types::track::FolderState;

// ─── .als → .rpp ───────────────────────────────────────────────────────────

fn ableton_to_rpp(set: &AbletonLiveSet) -> dawfile_reaper::types::ReaperProject {
    let mut builder = ReaperProjectBuilder::new().tempo_with_time_sig(
        set.tempo,
        set.time_signature.numerator as i32,
        set.time_signature.denominator as i32,
    );

    // Tempo automation
    if !set.tempo_automation.is_empty() {
        builder = builder.tempo_envelope(|env| {
            let mut env = env;
            for point in &set.tempo_automation {
                env = env.point(point.time, point.value);
            }
            env
        });
    }

    // Markers
    for (i, loc) in set.locators.iter().enumerate() {
        builder = builder.marker(i as i32 + 1, loc.time, &loc.name);
    }

    // Audio tracks
    for track in &set.audio_tracks {
        builder = builder.track(&track.common.effective_name, |t: TrackBuilder| {
            let mut t = t
                .volume(track.common.mixer.volume)
                .pan(track.common.mixer.pan);

            if !track.common.mixer.speaker_on {
                t = t.muted();
            }

            for clip in &track.arrangement_clips {
                let position = clip.common.time;
                let length = clip.common.current_end - clip.common.current_start;
                if length <= 0.0 {
                    continue;
                }

                t = t.item(position, length, |item: ItemBuilder| {
                    let mut item = item.name(&clip.common.name);
                    if let Some(ref sr) = clip.sample_ref
                        && let Some(ref path) = sr.path
                    {
                        item = item.source_wave(path.to_string_lossy().to_string());
                    }
                    if clip.pitch_coarse != 0.0 {
                        item = item.pitch(clip.pitch_coarse);
                    }
                    item
                });
            }

            t
        });
    }

    // MIDI tracks
    for track in &set.midi_tracks {
        builder = builder.track(&track.common.effective_name, |t: TrackBuilder| {
            let mut t = t
                .volume(track.common.mixer.volume)
                .pan(track.common.mixer.pan);

            if !track.common.mixer.speaker_on {
                t = t.muted();
            }

            for clip in &track.arrangement_clips {
                let position = clip.common.time;
                let length = clip.common.current_end - clip.common.current_start;
                if length <= 0.0 || clip.key_tracks.is_empty() {
                    continue;
                }

                t = t.item(position, length, |item: ItemBuilder| {
                    item.name(&clip.common.name)
                        .source_midi()
                        .midi(|midi: MidiSourceBuilder| {
                            let mut midi = midi;
                            for kt in &clip.key_tracks {
                                for note in &kt.notes {
                                    if !note.is_enabled {
                                        continue;
                                    }
                                    let tick_pos = (note.time * 960.0) as u32;
                                    let tick_dur = (note.duration * 960.0) as u32;
                                    midi = midi.at(tick_pos as u64).note(
                                        0,
                                        0,
                                        kt.midi_key,
                                        note.velocity,
                                        tick_dur,
                                    );
                                }
                            }
                            midi
                        })
                });
            }

            t
        });
    }

    // Group tracks
    for track in &set.group_tracks {
        builder = builder.track(&track.common.effective_name, |t: TrackBuilder| {
            t.volume(track.common.mixer.volume)
                .pan(track.common.mixer.pan)
                .folder_start()
        });
    }

    builder.build()
}

// ─── .rpp → .als ───────────────────────────────────────────────────────────

fn rpp_to_ableton(project: &dawfile_reaper::types::ReaperProject) -> AbletonLiveSet {
    let (tempo, ts_num, ts_den) = if let Some(ref te) = project.tempo_envelope {
        let first = te.points.first();
        let ts = first.and_then(|p| p.time_signature()).unwrap_or((4, 4));
        (
            first.map(|p| p.tempo).unwrap_or(120.0),
            ts.0 as u8,
            ts.1 as u8,
        )
    } else {
        (120.0, 4, 4)
    };

    let tempo_automation: Vec<AutomationPoint> = project
        .tempo_envelope
        .as_ref()
        .map(|te| {
            te.points
                .iter()
                .skip(1)
                .map(|p| AutomationPoint {
                    time: p.position,
                    value: p.tempo,
                    curve_control_1: None,
                    curve_control_2: None,
                })
                .collect()
        })
        .unwrap_or_default();

    let locators: Vec<Locator> = project
        .markers_regions
        .markers
        .iter()
        .map(|m| Locator {
            time: m.position,
            name: m.name.clone(),
        })
        .collect();

    let mut audio_tracks = Vec::new();
    let mut midi_tracks = Vec::new();
    let mut group_tracks = Vec::new();

    for (idx, track) in project.tracks.iter().enumerate() {
        let is_folder = track
            .folder
            .as_ref()
            .is_some_and(|f| matches!(f.folder_state, FolderState::FolderParent));

        let (vol, pan) = track
            .volpan
            .as_ref()
            .map(|vp| (vp.volume, vp.pan))
            .unwrap_or((1.0, 0.0));

        let muted = track.mutesolo.as_ref().is_some_and(|ms| ms.mute);

        let common = TrackCommon {
            id: idx as i32,
            user_name: String::new(),
            effective_name: track.name.clone(),
            annotation: String::new(),
            color: track.peak_color.unwrap_or(0),
            group_id: -1,
            folded: false,
            mixer: MixerState {
                volume: vol,
                pan,
                speaker_on: !muted,
                ..MixerState::default()
            },
            devices: Vec::new(),
            automation_envelopes: Vec::new(),
            track_delay: None,
            linked_track_group_id: -1,
            view_state: None,
            memorized_first_clip_name: String::new(),
        };

        if is_folder {
            group_tracks.push(GroupTrack { common });
            continue;
        }

        let has_midi = track.items.iter().any(|item| {
            item.takes
                .first()
                .and_then(|t| t.source.as_ref())
                .is_some_and(|s| matches!(s.source_type, dawfile_reaper::types::SourceType::Midi))
        });

        if has_midi {
            let clips = track
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| MidiClip {
                    common: item_to_clip_common(i, item),
                    key_tracks: extract_midi_notes(item),
                    scale_info: None,
                    bank_select_coarse: -1,
                    bank_select_fine: -1,
                    program_change: -1,
                    note_spelling_preference: 3,
                    expression_grid: None,
                })
                .collect();

            midi_tracks.push(MidiTrack {
                common,
                arrangement_clips: clips,
                session_clips: Vec::new(),
                midi_input: RoutingTarget::default(),
                audio_output: RoutingTarget::default(),
                monitoring: 0,
                is_armed: false,
                take_counter: 0,
                take_lanes: Vec::new(),
                pitchbend_range: 48,
            });
        } else {
            let clips = track
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let source_path = item
                        .takes
                        .first()
                        .and_then(|t| t.source.as_ref())
                        .filter(|s| !s.file_path.is_empty())
                        .map(|s| s.file_path.clone());

                    let pitch = item
                        .playrate
                        .as_ref()
                        .map(|pr| pr.pitch_adjust)
                        .unwrap_or(0.0);

                    AudioClip {
                        common: item_to_clip_common(i, item),
                        sample_ref: source_path.map(|p| SampleRef {
                            path: Some(std::path::PathBuf::from(&p)),
                            relative_path: None,
                            name: std::path::Path::new(&p)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string()),
                            file_size: None,
                            crc: None,
                            last_mod_date: None,
                            default_duration: None,
                            default_sample_rate: None,
                            live_pack_name: None,
                            live_pack_id: None,
                        }),
                        warp_mode: 0,
                        is_warped: true,
                        warp_markers: Vec::new(),
                        pitch_coarse: pitch,
                        pitch_fine: 0.0,
                        sample_volume: 1.0,
                        fades: None,
                        granularity_tones: 30.0,
                        granularity_texture: 65.0,
                        fluctuation_texture: 25.0,
                        transient_resolution: 6,
                        transient_loop_mode: 2,
                        transient_envelope: 100.0,
                        complex_pro_formants: 100.0,
                        complex_pro_envelope: 128.0,
                        fade_on: true,
                        hiq: true,
                        is_song_tempo_leader: false,
                    }
                })
                .collect();

            audio_tracks.push(AudioTrack {
                common,
                arrangement_clips: clips,
                session_clips: Vec::new(),
                audio_input: RoutingTarget::default(),
                audio_output: RoutingTarget::default(),
                monitoring: 0,
                is_armed: false,
                take_counter: 0,
                take_lanes: Vec::new(),
            });
        }
    }

    AbletonLiveSet {
        version: AbletonVersion {
            major: 12,
            minor: 0,
            patch: 0,
            beta: false,
            creator: "dawfile roundtrip".to_string(),
        },
        tempo,
        time_signature: TimeSignature {
            numerator: ts_num,
            denominator: ts_den,
        },
        key_signature: None,
        audio_tracks,
        midi_tracks,
        return_tracks: Vec::new(),
        group_tracks,
        master_track: None,
        locators,
        scenes: Vec::new(),
        tempo_automation,
        transport: TransportState::default(),
        furthest_bar: 0.0,
        groove_pool: Vec::new(),
        tuning_system: None,
        pre_hear_track: None,
    }
}

fn item_to_clip_common(idx: usize, item: &dawfile_reaper::types::Item) -> ClipCommon {
    let name = if !item.name.is_empty() {
        item.name.clone()
    } else {
        item.takes
            .first()
            .map(|t| t.name.clone())
            .unwrap_or_default()
    };

    ClipCommon {
        id: idx as i32,
        time: item.position,
        current_start: 0.0,
        current_end: item.length,
        name,
        color: 0,
        disabled: item.mute.as_ref().is_some_and(|m| m.muted),
        loop_settings: Some(LoopSettings {
            loop_start: 0.0,
            loop_end: item.length,
            loop_on: false,
            start_relative: 0.0,
        }),
        follow_action: None,
        envelopes: Vec::new(),
        launch_mode: 0,
        launch_quantisation: 0,
        grid: None,
        legato: false,
        ram: false,
        velocity_amount: 0.0,
        groove_id: -1,
        freeze_start: 0.0,
        freeze_end: 0.0,
        take_id: 0,
    }
}

fn extract_midi_notes(item: &dawfile_reaper::types::Item) -> Vec<KeyTrack> {
    let midi_source = item
        .takes
        .first()
        .and_then(|t| t.source.as_ref())
        .and_then(|s| s.midi_data.as_ref());

    let midi_source = match midi_source {
        Some(m) => m,
        None => return Vec::new(),
    };

    let tpqn = midi_source.ticks_per_qn.max(1) as f64;

    // Compute absolute positions from delta ticks and group notes by key
    let mut key_map: std::collections::BTreeMap<u8, Vec<MidiNote>> =
        std::collections::BTreeMap::new();

    let mut abs_pos: u64 = 0;
    // Track note-on positions for computing duration
    let mut pending_notes: Vec<(u64, u8, u8)> = Vec::new(); // (abs_pos, note, velocity)

    for source_event in &midi_source.event_stream {
        let (delta, event) = match source_event {
            MidiSourceEvent::Midi(e) => (e.delta_ticks, Some(e)),
            MidiSourceEvent::Extended(e) => (e.delta_ticks(), None),
        };
        abs_pos += delta as u64;

        let event = match event {
            Some(e) => e,
            None => continue,
        };

        match event.event_type() {
            MidiEventType::NoteOn if event.bytes.len() >= 3 => {
                let note_num = event.bytes[1];
                let vel = event.bytes[2];
                if vel > 0 {
                    pending_notes.push((abs_pos, note_num, vel));
                } else {
                    // velocity 0 = note-off
                    complete_note(&mut pending_notes, &mut key_map, abs_pos, note_num, tpqn);
                }
            }
            MidiEventType::NoteOff if event.bytes.len() >= 2 => {
                let note_num = event.bytes[1];
                complete_note(&mut pending_notes, &mut key_map, abs_pos, note_num, tpqn);
            }
            _ => {}
        }
    }

    key_map
        .into_iter()
        .map(|(key, notes)| KeyTrack {
            midi_key: key,
            notes,
        })
        .collect()
}

fn complete_note(
    pending: &mut Vec<(u64, u8, u8)>,
    key_map: &mut std::collections::BTreeMap<u8, Vec<MidiNote>>,
    off_pos: u64,
    note_num: u8,
    tpqn: f64,
) {
    if let Some(idx) = pending.iter().rposition(|(_, n, _)| *n == note_num) {
        let (on_pos, _, vel) = pending.remove(idx);
        let dur = off_pos.saturating_sub(on_pos);
        key_map.entry(note_num).or_default().push(MidiNote {
            time: on_pos as f64 / tpqn,
            duration: dur as f64 / tpqn,
            velocity: vel,
            velocity_deviation: 0,
            is_enabled: true,
            probability: 1.0,
        });
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn als_to_rpp_to_als_lucid_dreaming() {
    let original = read_live_set("../dawfile-ableton/tests/fixtures/LucidDreaming.als")
        .expect("failed to parse LucidDreaming.als");

    // .als → .rpp
    let rpp_project = ableton_to_rpp(&original);
    let rpp_text = rpp_project.to_rpp_string();
    assert!(!rpp_text.is_empty());

    // Parse RPP back
    let reparsed_rpp =
        dawfile_reaper::parse_project_text(&rpp_text).expect("failed to parse generated RPP");

    // .rpp → .als
    let roundtripped = rpp_to_ableton(&reparsed_rpp);

    // Verify equivalence
    assert_eq!(roundtripped.tempo, original.tempo);
    assert_eq!(
        roundtripped.time_signature.numerator,
        original.time_signature.numerator
    );
    assert_eq!(
        roundtripped.time_signature.denominator,
        original.time_signature.denominator
    );
    assert_eq!(roundtripped.locators.len(), original.locators.len());

    // REAPER doesn't distinguish MIDI vs audio tracks, so compare total track count
    let orig_total =
        original.audio_tracks.len() + original.midi_tracks.len() + original.group_tracks.len();
    let rt_total = roundtripped.audio_tracks.len()
        + roundtripped.midi_tracks.len()
        + roundtripped.group_tracks.len();
    assert_eq!(rt_total, orig_total, "total track count should match");

    // Audio track names should match (in order)
    for (orig, rt) in original
        .audio_tracks
        .iter()
        .zip(roundtripped.audio_tracks.iter())
    {
        assert_eq!(rt.common.effective_name, orig.common.effective_name);
        assert_eq!(
            rt.arrangement_clips.len(),
            orig.arrangement_clips.len(),
            "track '{}' clip count",
            orig.common.effective_name
        );
        assert!(
            (rt.common.mixer.volume - orig.common.mixer.volume).abs() < 0.01,
            "track '{}' volume",
            orig.common.effective_name
        );
    }

    // Write back to .als and verify parseable
    let als_bytes = write_live_set_bytes(&roundtripped).expect("failed to write");
    let final_set = parse_live_set_bytes(&als_bytes).expect("failed to re-parse");
    let final_total =
        final_set.audio_tracks.len() + final_set.midi_tracks.len() + final_set.group_tracks.len();
    assert_eq!(final_total, orig_total);
    assert_eq!(final_set.tempo, original.tempo);
}

#[test]
fn als_to_rpp_to_als_farmhouse() {
    let original = read_live_set("../dawfile-ableton/tests/fixtures/Farmhouse.als")
        .expect("failed to parse Farmhouse.als");

    let rpp_project = ableton_to_rpp(&original);
    let rpp_text = rpp_project.to_rpp_string();

    let reparsed_rpp = dawfile_reaper::parse_project_text(&rpp_text).expect("failed to parse RPP");
    let roundtripped = rpp_to_ableton(&reparsed_rpp);

    assert_eq!(roundtripped.tempo, original.tempo);
    assert_eq!(
        roundtripped.time_signature.numerator,
        original.time_signature.numerator
    );
    assert_eq!(roundtripped.locators.len(), original.locators.len());

    let orig_total =
        original.audio_tracks.len() + original.midi_tracks.len() + original.group_tracks.len();
    let rt_total = roundtripped.audio_tracks.len()
        + roundtripped.midi_tracks.len()
        + roundtripped.group_tracks.len();
    assert_eq!(rt_total, orig_total);

    // Audio track names match in order
    for (orig, rt) in original
        .audio_tracks
        .iter()
        .zip(roundtripped.audio_tracks.iter())
    {
        assert_eq!(rt.common.effective_name, orig.common.effective_name);
    }

    let als_bytes = write_live_set_bytes(&roundtripped).expect("failed to write");
    let final_set = parse_live_set_bytes(&als_bytes).expect("failed to re-parse");
    assert_eq!(final_set.tempo, original.tempo);
    let final_total =
        final_set.audio_tracks.len() + final_set.midi_tracks.len() + final_set.group_tracks.len();
    assert_eq!(final_total, orig_total);
}

#[test]
fn rpp_output_is_valid_reaper_project() {
    let set = read_live_set("../dawfile-ableton/tests/fixtures/LucidDreaming.als")
        .expect("failed to parse");

    let rpp = ableton_to_rpp(&set);
    let text = rpp.to_rpp_string();

    assert!(text.starts_with("<REAPER_PROJECT"));
    assert!(text.contains("TEMPO "));
    assert!(text.contains("<TRACK"));
    assert!(text.contains("NAME "));

    let track_count = text.matches("<TRACK").count();
    let expected = set.audio_tracks.len() + set.midi_tracks.len() + set.group_tracks.len();
    assert_eq!(track_count, expected);
}
