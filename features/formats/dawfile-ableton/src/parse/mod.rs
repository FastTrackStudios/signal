//! Parse orchestration for Ableton Live set files.
//!
//! Coordinates all parsing stages: version detection, tempo extraction,
//! track parsing, clip parsing, etc.

pub mod automation;
pub mod clips;
pub mod devices;
pub mod grooves;
pub mod markers;
pub mod samples;
pub mod tempo;
pub mod tracks;
pub mod version;
pub mod xml_helpers;

use crate::error::{AbletonError, AbletonResult};
use crate::types::*;
use xml_helpers::*;

/// Parse an Ableton Live set from decompressed XML.
pub fn parse_live_set(xml: &str) -> AbletonResult<AbletonLiveSet> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| AbletonError::Xml(e.to_string()))?;

    let root = doc.root_element();
    if root.tag_name().name() != "Ableton" {
        return Err(AbletonError::MissingRoot);
    }

    // 1. Version
    let version = version::parse_version(root)?;

    // 2. LiveSet element
    let live_set = child(root, "LiveSet").ok_or(AbletonError::MissingLiveSet)?;

    // 3. Master track (v12+ renamed to MainTrack)
    let master_node = child(live_set, "MainTrack").or_else(|| child(live_set, "MasterTrack"));

    // 4. Tempo and time signature from master track
    let (master_tempo, time_signature, tempo_automation) = if let Some(master) = master_node {
        (
            tempo::parse_tempo(master),
            tempo::parse_time_signature(master),
            tempo::parse_tempo_automation(master),
        )
    } else {
        (120.0, TimeSignature::default(), Vec::new())
    };

    // 5. Parse tracks
    let (audio_tracks, midi_tracks, group_tracks) = child(live_set, "Tracks")
        .map(|t| tracks::parse_tracks(t, &version))
        .unwrap_or_default();

    // 6. Return tracks (separate element)
    let return_tracks = child(live_set, "ReturnTracks")
        .map(|rt| tracks::parse_return_tracks(rt, &version))
        .unwrap_or_default();

    // 7. Master track state
    let master_track = master_node.map(|m| tracks::parse_master_track(m, &version));

    // 8. Locators (markers)
    let locators = child(live_set, "Locators")
        .map(markers::parse_locators)
        .unwrap_or_default();

    // 9. Scenes
    let scenes = child(live_set, "Scenes")
        .map(markers::parse_scenes)
        .unwrap_or_default();

    // 10. Transport
    let transport = child(live_set, "Transport")
        .map(markers::parse_transport)
        .unwrap_or_default();

    // 11. Key signature (frequency analysis across MIDI clips, v11+ only)
    let key_signature = if version.at_least(11, 0) {
        detect_key_signature(&midi_tracks)
    } else {
        None
    };

    // 12. Groove pool
    let groove_pool = child(live_set, "GroovePool")
        .map(grooves::parse_groove_pool)
        .unwrap_or_default();

    // 13. Tuning system (v12, opaque preservation)
    let tuning_system = child(live_set, "TuningSystems")
        .or_else(|| child(live_set, "TuningSystem"))
        .map(|node| {
            // Collect the raw XML text of the tuning system element.
            // We use the document text range for round-trip fidelity.
            let raw_xml = node
                .document()
                .input_text()
                .get(node.range().start..node.range().end)
                .unwrap_or("")
                .to_string();
            TuningSystem { raw_xml }
        });

    // 14. Pre-hear track
    let pre_hear_track = child(live_set, "PreHearTrack").map(tracks::parse_pre_hear_track);

    // 15. Furthest bar position
    let max_current_end = collect_max_current_end(live_set);
    let furthest_bar = if time_signature.numerator > 0 {
        max_current_end / time_signature.numerator as f64
    } else {
        max_current_end
    };

    Ok(AbletonLiveSet {
        version,
        tempo: master_tempo,
        time_signature,
        key_signature,
        audio_tracks,
        midi_tracks,
        return_tracks,
        group_tracks,
        master_track,
        locators,
        scenes,
        tempo_automation,
        transport,
        furthest_bar,
        groove_pool,
        tuning_system,
        pre_hear_track,
    })
}

/// Detect the most common key signature across all MIDI clips.
fn detect_key_signature(midi_tracks: &[MidiTrack]) -> Option<KeySignature> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for track in midi_tracks {
        for clip in all_midi_clips(track) {
            if let Some(ref ks) = clip.scale_info {
                let key = format!("{} {}", ks.root_note, ks.scale);
                *counts.entry(key).or_default() += 1;
            }
        }
    }

    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .and_then(|(key_str, _)| {
            for track in midi_tracks {
                for clip in all_midi_clips(track) {
                    if let Some(ref ks) = clip.scale_info {
                        let k = format!("{} {}", ks.root_note, ks.scale);
                        if k == key_str {
                            return Some(ks.clone());
                        }
                    }
                }
            }
            None
        })
}

/// Iterate all MIDI clips on a track (arrangement + session).
fn all_midi_clips(track: &MidiTrack) -> impl Iterator<Item = &MidiClip> {
    track
        .arrangement_clips
        .iter()
        .chain(track.session_clips.iter().map(|sc| &sc.clip))
}
