//! Clip parsing for audio and MIDI clips.
//!
//! Clips appear in two contexts:
//! - **Arrangement**: Under `ArrangerAutomation.Events` as `<AudioClip>` or `<MidiClip>`
//!   elements with `Id` and `Time` attributes.
//! - **Session**: Inside `<ClipSlot>` elements in `ClipSlotList`. Each slot
//!   has an `Id` attribute that corresponds to the scene row index.

use super::automation;
use super::samples;
use super::xml_helpers::*;
use crate::types::*;
use roxmltree::Node;

// ─── Arrangement clips ──────────────────────────────────────────────────────

/// Parse audio clips from an arrangement `Events` node.
pub fn parse_audio_clips(events: Node<'_, '_>, version: &AbletonVersion) -> Vec<AudioClip> {
    events
        .children()
        .filter(|n| n.has_tag_name("AudioClip"))
        .filter_map(|n| parse_audio_clip(n, version))
        .collect()
}

/// Parse MIDI clips from an arrangement `Events` node.
pub fn parse_midi_clips(events: Node<'_, '_>, version: &AbletonVersion) -> Vec<MidiClip> {
    events
        .children()
        .filter(|n| n.has_tag_name("MidiClip"))
        .filter_map(|n| parse_midi_clip(n, version))
        .collect()
}

// ─── Session clips ──────────────────────────────────────────────────────────

/// Parse audio clips from session view clip slots, preserving slot index.
pub fn parse_session_audio_clips(
    clip_slot_list: Node<'_, '_>,
    version: &AbletonVersion,
) -> Vec<SessionClip<AudioClip>> {
    let mut clips = Vec::new();
    for (slot_index, slot) in clip_slot_list
        .children()
        .filter(|n| n.has_tag_name("ClipSlot"))
        .enumerate()
    {
        let has_stop = child_bool(slot, "HasStop").unwrap_or(true);
        for clip_data in slot.children() {
            for clip_node in clip_data.children() {
                if clip_node.has_tag_name("AudioClip")
                    && let Some(clip) = parse_audio_clip(clip_node, version)
                {
                    clips.push(SessionClip {
                        slot_index,
                        clip,
                        has_stop,
                    });
                }
            }
        }
    }
    clips
}

/// Parse MIDI clips from session view clip slots, preserving slot index.
pub fn parse_session_midi_clips(
    clip_slot_list: Node<'_, '_>,
    version: &AbletonVersion,
) -> Vec<SessionClip<MidiClip>> {
    let mut clips = Vec::new();
    for (slot_index, slot) in clip_slot_list
        .children()
        .filter(|n| n.has_tag_name("ClipSlot"))
        .enumerate()
    {
        let has_stop = child_bool(slot, "HasStop").unwrap_or(true);
        for clip_data in slot.children() {
            for clip_node in clip_data.children() {
                if clip_node.has_tag_name("MidiClip")
                    && let Some(clip) = parse_midi_clip(clip_node, version)
                {
                    clips.push(SessionClip {
                        slot_index,
                        clip,
                        has_stop,
                    });
                }
            }
        }
    }
    clips
}

// ─── Individual clip parsing ────────────────────────────────────────────────

fn parse_clip_common(node: Node<'_, '_>) -> ClipCommon {
    let id = id_attr(node);
    let time = time_attr(node);
    let current_start = child_f64(node, "CurrentStart").unwrap_or(0.0);
    let current_end = child_f64(node, "CurrentEnd").unwrap_or(0.0);
    let name = child_value(node, "Name").unwrap_or("").to_string();
    let color = child_i32(node, "Color")
        .or_else(|| child_i32(node, "ColorIndex"))
        .unwrap_or(0);
    let disabled = child_bool(node, "Disabled").unwrap_or(false);

    let loop_settings = child(node, "Loop").map(|loop_node| LoopSettings {
        loop_start: child_f64(loop_node, "LoopStart").unwrap_or(0.0),
        loop_end: child_f64(loop_node, "LoopEnd").unwrap_or(0.0),
        loop_on: child_bool(loop_node, "LoopOn").unwrap_or(false),
        start_relative: child_f64(loop_node, "StartRelative").unwrap_or(0.0),
    });

    let follow_action = child(node, "FollowAction").map(|fa| {
        let enabled = child_bool(fa, "FollowActionEnabled").unwrap_or(false);
        let follow_time = child_f64(fa, "FollowTime").unwrap_or(4.0);
        let is_linked = child_bool(fa, "IsLinked").unwrap_or(true);
        let loop_iterations = child_i32(fa, "LoopIterations").unwrap_or(1);
        let action_a = child_i32(fa, "FollowActionA").unwrap_or(0);
        let action_b = child_i32(fa, "FollowActionB").unwrap_or(0);
        let chance_a = child_i32(fa, "FollowChanceA").unwrap_or(100);
        let chance_b = child_i32(fa, "FollowChanceB").unwrap_or(0);
        FollowAction {
            follow_time,
            is_linked,
            loop_iterations,
            action_a,
            action_b,
            chance_a,
            chance_b,
            enabled,
        }
    });

    let envelopes = automation::parse_clip_envelopes(node);

    let launch_mode = child_i32(node, "LaunchMode").unwrap_or(0);
    let launch_quantisation = child_i32(node, "LaunchQuantisation").unwrap_or(0);
    let grid = parse_clip_grid(node, "Grid");
    let legato = child_bool(node, "Legato").unwrap_or(false);
    let ram = child_bool(node, "Ram").unwrap_or(false);
    let velocity_amount = child_f64(node, "VelocityAmount").unwrap_or(0.0);
    let groove_id = child(node, "GrooveSettings")
        .and_then(|gs| child_i32(gs, "GrooveId"))
        .unwrap_or(-1);
    let freeze_start = child_f64(node, "FreezeStart").unwrap_or(0.0);
    let freeze_end = child_f64(node, "FreezeEnd").unwrap_or(0.0);
    let take_id = child_i32(node, "TakeId").unwrap_or(0);

    ClipCommon {
        id,
        time,
        current_start,
        current_end,
        name,
        color,
        disabled,
        loop_settings,
        follow_action,
        envelopes,
        launch_mode,
        launch_quantisation,
        grid,
        legato,
        ram,
        velocity_amount,
        groove_id,
        freeze_start,
        freeze_end,
        take_id,
    }
}

fn parse_clip_grid(node: Node<'_, '_>, tag: &str) -> Option<ClipGrid> {
    let grid_node = child(node, tag)?;
    Some(ClipGrid {
        fixed_numerator: child_i32(grid_node, "FixedNumerator").unwrap_or(1),
        fixed_denominator: child_i32(grid_node, "FixedDenominator").unwrap_or(4),
        grid_interval_pixel: child_i32(grid_node, "GridIntervalPixel").unwrap_or(20),
        ntoles: child_i32(grid_node, "Ntoles").unwrap_or(2),
        snap_to_grid: child_bool(grid_node, "SnapToGrid").unwrap_or(true),
        fixed: child_bool(grid_node, "Fixed").unwrap_or(false),
    })
}

fn parse_audio_clip(node: Node<'_, '_>, version: &AbletonVersion) -> Option<AudioClip> {
    let common = parse_clip_common(node);

    let sample_ref = child(node, "SampleRef").map(|sr| samples::parse_sample_ref(sr, version));

    let warp_mode = child_i32(node, "WarpMode").unwrap_or(0);
    let is_warped = child_bool(node, "IsWarped").unwrap_or(true);
    let pitch_coarse = child_f64(node, "PitchCoarse").unwrap_or(0.0);
    let pitch_fine = child_f64(node, "PitchFine").unwrap_or(0.0);
    let sample_volume = child_f64(node, "SampleVolume").unwrap_or(1.0);

    let warp_markers = child(node, "WarpMarkers")
        .map(|wm| {
            wm.children()
                .filter(|n| n.has_tag_name("WarpMarker"))
                .filter_map(|marker| {
                    let sec_time = marker
                        .attribute("SecTime")
                        .and_then(|v| v.parse::<f64>().ok())?;
                    let beat_time = marker
                        .attribute("BeatTime")
                        .and_then(|v| v.parse::<f64>().ok())?;
                    Some(WarpMarker {
                        sec_time,
                        beat_time,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let fades = child(node, "Fades").map(|f| Fades {
        fade_in_length: child_f64(f, "FadeInLength").unwrap_or(0.0),
        fade_out_length: child_f64(f, "FadeOutLength").unwrap_or(0.0),
        fade_in_curve_skew: child_f64(f, "FadeInCurveSkew").unwrap_or(0.0),
        fade_in_curve_slope: child_f64(f, "FadeInCurveSlope").unwrap_or(0.0),
        fade_out_curve_skew: child_f64(f, "FadeOutCurveSkew").unwrap_or(0.0),
        fade_out_curve_slope: child_f64(f, "FadeOutCurveSlope").unwrap_or(0.0),
    });

    let granularity_tones = child_f64(node, "GranularityTones").unwrap_or(30.0);
    let granularity_texture = child_f64(node, "GranularityTexture").unwrap_or(65.0);
    let fluctuation_texture = child_f64(node, "FluctuationTexture").unwrap_or(25.0);
    let transient_resolution = child_i32(node, "TransientResolution").unwrap_or(6);
    let transient_loop_mode = child_i32(node, "TransientLoopMode").unwrap_or(2);
    let transient_envelope = child_f64(node, "TransientEnvelope").unwrap_or(100.0);
    let complex_pro_formants = child_f64(node, "ComplexProFormants").unwrap_or(100.0);
    let complex_pro_envelope = child_f64(node, "ComplexProEnvelope").unwrap_or(128.0);
    let fade_on = child_bool(node, "Fade").unwrap_or(true);
    let hiq = child_bool(node, "HiQ").unwrap_or(true);
    let is_song_tempo_leader = child_bool(node, "IsSongTempoLeader").unwrap_or(false);

    Some(AudioClip {
        common,
        sample_ref,
        warp_mode,
        is_warped,
        warp_markers,
        pitch_coarse,
        pitch_fine,
        sample_volume,
        fades,
        granularity_tones,
        granularity_texture,
        fluctuation_texture,
        transient_resolution,
        transient_loop_mode,
        transient_envelope,
        complex_pro_formants,
        complex_pro_envelope,
        fade_on,
        hiq,
        is_song_tempo_leader,
    })
}

fn parse_midi_clip(node: Node<'_, '_>, version: &AbletonVersion) -> Option<MidiClip> {
    let common = parse_clip_common(node);

    let key_tracks = descend(node, "Notes.KeyTracks")
        .map(|kt_node| {
            kt_node
                .children()
                .filter(|n| n.has_tag_name("KeyTrack"))
                .filter_map(|kt| parse_key_track(kt))
                .collect()
        })
        .unwrap_or_default();

    let scale_info = if version.at_least(11, 0) {
        parse_scale_info(node)
    } else {
        None
    };

    let bank_select_coarse = child_i32(node, "BankSelectCoarse").unwrap_or(-1);
    let bank_select_fine = child_i32(node, "BankSelectFine").unwrap_or(-1);
    let program_change = child_i32(node, "ProgramChange").unwrap_or(-1);
    let note_spelling_preference = child_i32(node, "NoteSpellingPreference").unwrap_or(3);
    let expression_grid = parse_clip_grid(node, "ExpressionGrid");

    Some(MidiClip {
        common,
        key_tracks,
        scale_info,
        bank_select_coarse,
        bank_select_fine,
        program_change,
        note_spelling_preference,
        expression_grid,
    })
}

fn parse_key_track(node: Node<'_, '_>) -> Option<KeyTrack> {
    let midi_key = child_i32(node, "MidiKey")? as u8;

    let notes = child(node, "Notes")
        .map(|notes_node| {
            notes_node
                .children()
                .filter(|n| n.has_tag_name("MidiNoteEvent"))
                .filter_map(|note| {
                    let time = note.attribute("Time")?.parse::<f64>().ok()?;
                    let duration = note.attribute("Duration")?.parse::<f64>().ok()?;
                    let velocity = note
                        .attribute("Velocity")
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(100);
                    let velocity_deviation = note
                        .attribute("VelocityDeviation")
                        .and_then(|v| v.parse::<i8>().ok())
                        .unwrap_or(0);
                    let is_enabled = note
                        .attribute("IsEnabled")
                        .map(|v| v == "true")
                        .unwrap_or(true);
                    let probability = note
                        .attribute("Probability")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(1.0);

                    Some(MidiNote {
                        time,
                        duration,
                        velocity,
                        velocity_deviation,
                        is_enabled,
                        probability,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(KeyTrack { midi_key, notes })
}

fn parse_scale_info(clip_node: Node<'_, '_>) -> Option<KeySignature> {
    let is_in_key = child_bool(clip_node, "IsInKey").unwrap_or(false);
    if !is_in_key {
        return None;
    }

    let scale_node = child(clip_node, "ScaleInformation")?;
    let root_note = child_i32(scale_node, "RootNote")? as u32;
    let scale_name = child_value(scale_node, "Name")?.to_string();

    Some(KeySignature {
        root_note: Tonic::from_midi(root_note),
        scale: scale_name,
    })
}
