//! Parse clips and their content (audio, video, MIDI notes).

use super::xml_helpers::*;
use crate::types::{
    AudioContent, Clip, ClipContent, ClipSlot, FileReference, Lane, LaneContent, LoopSettings,
    Marker, Note, Scene, SceneContent, TimeUnit, VideoContent, Warp, Warps,
};
use roxmltree::Node;

/// Parse a `<Clips>` element into a list of clips.
pub fn parse_clips(clips_node: Node<'_, '_>) -> Vec<Clip> {
    children(clips_node, "Clip").map(parse_clip).collect()
}

pub fn parse_clip(node: Node<'_, '_>) -> Clip {
    let id = attr(node, "id").unwrap_or("").to_string();
    let time = attr_f64(node, "time", 0.0);
    let duration = attr_f64(node, "duration", 0.0);
    let time_unit = attr(node, "timeUnit").map(TimeUnit::from_str);
    let content_time_unit = attr(node, "contentTimeUnit").map(TimeUnit::from_str);
    let name = attr(node, "name").map(str::to_string);
    let color = attr(node, "color").map(str::to_string);
    let comment = attr(node, "comment").map(str::to_string);
    let enabled = attr_bool(node, "enable", true);
    let play_start = attr(node, "playStart").and_then(|v| v.parse().ok());
    let play_stop = attr(node, "playStop").and_then(|v| v.parse().ok());
    let reference = attr(node, "reference").map(str::to_string);
    let fade_in_time = attr(node, "fadeInTime").and_then(|v| v.parse().ok());
    let fade_out_time = attr(node, "fadeOutTime").and_then(|v| v.parse().ok());
    let fade_time_unit = attr(node, "fadeTimeUnit").map(TimeUnit::from_str);
    let loop_settings = parse_loop(node);
    let content = parse_clip_content(node);

    Clip {
        id,
        time,
        duration,
        time_unit,
        content_time_unit,
        name,
        color,
        comment,
        enabled,
        play_start,
        play_stop,
        reference,
        fade_in_time,
        fade_out_time,
        fade_time_unit,
        loop_settings,
        content,
    }
}

fn parse_loop(node: Node<'_, '_>) -> Option<LoopSettings> {
    let loop_start = attr(node, "loopStart").and_then(|v| v.parse().ok())?;
    let loop_end = attr(node, "loopEnd").and_then(|v| v.parse().ok())?;
    let play_start = attr(node, "playStart")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    Some(LoopSettings {
        loop_start,
        loop_end,
        play_start,
    })
}

fn parse_clip_content(node: Node<'_, '_>) -> ClipContent {
    if let Some(audio_node) = child(node, "Audio") {
        return ClipContent::Audio(parse_audio(audio_node));
    }
    if let Some(video_node) = child(node, "Video") {
        return ClipContent::Video(parse_video(video_node));
    }
    if let Some(notes_node) = child(node, "Notes") {
        return ClipContent::Notes(parse_notes(notes_node));
    }
    ClipContent::Empty
}

fn parse_file_reference(node: Node<'_, '_>) -> Option<FileReference> {
    child(node, "File").map(|file_node| FileReference {
        path: attr(file_node, "path").unwrap_or("").to_string(),
        external: attr_bool(file_node, "external", false),
    })
}

fn parse_audio(node: Node<'_, '_>) -> AudioContent {
    let file = parse_file_reference(node);
    let sample_rate = attr(node, "sampleRate").and_then(|v| v.parse().ok());
    let channels = attr(node, "channels").and_then(|v| v.parse().ok());
    let duration = attr(node, "duration").and_then(|v| v.parse().ok());
    let algorithm = attr(node, "algorithm").map(str::to_string);
    let warps = child(node, "Warps").map(parse_warps).unwrap_or_default();
    AudioContent {
        file,
        sample_rate,
        channels,
        duration,
        algorithm,
        warps,
    }
}

fn parse_video(node: Node<'_, '_>) -> VideoContent {
    let file = parse_file_reference(node);
    let sample_rate = attr(node, "sampleRate").and_then(|v| v.parse().ok());
    let channels = attr(node, "channels").and_then(|v| v.parse().ok());
    let duration = attr(node, "duration").and_then(|v| v.parse().ok());
    let algorithm = attr(node, "algorithm").map(str::to_string);
    let warps = child(node, "Warps").map(parse_warps).unwrap_or_default();
    VideoContent {
        file,
        sample_rate,
        channels,
        duration,
        algorithm,
        warps,
    }
}

fn parse_warps(node: Node<'_, '_>) -> Warps {
    let content_time_unit = attr(node, "contentTimeUnit").map(TimeUnit::from_str);
    let warps = children(node, "Warp")
        .map(|w| Warp {
            time: attr_f64(w, "time", 0.0),
            content_time: attr_f64(w, "contentTime", 0.0),
        })
        .collect();
    Warps {
        content_time_unit,
        warps,
    }
}

pub fn parse_notes(notes_node: Node<'_, '_>) -> Vec<Note> {
    children(notes_node, "Note")
        .map(|n| Note {
            time: attr_f64(n, "time", 0.0),
            duration: attr_f64(n, "duration", 0.0),
            channel: attr_u8(n, "channel", 0),
            key: attr_u8(n, "key", 60),
            velocity: attr_f64(n, "vel", 1.0),
            release_velocity: attr(n, "rel").and_then(|v| v.parse().ok()),
        })
        .collect()
}

/// Parse scenes from a `<Scenes>` element.
pub fn parse_scenes(scenes_node: Node<'_, '_>) -> Vec<Scene> {
    children(scenes_node, "Scene").map(parse_scene).collect()
}

fn parse_scene(node: Node<'_, '_>) -> Scene {
    let id = attr(node, "id").unwrap_or("").to_string();
    let name = attr(node, "name").map(str::to_string);
    let color = attr(node, "color").map(str::to_string);
    let comment = attr(node, "comment").map(str::to_string);
    let content = parse_scene_content(node);
    Scene {
        id,
        name,
        color,
        comment,
        content,
    }
}

/// Parse the single timeline content element inside a `<Scene>`.
///
/// Per spec a Scene directly contains one of: Lanes, ClipSlot, Clips, Notes,
/// markers, Audio, Video, Points (or nothing).
fn parse_scene_content(node: Node<'_, '_>) -> Option<SceneContent> {
    for child_node in child_elements(node) {
        match child_node.tag_name().name() {
            "Lanes" => {
                // Import the arrangement lane parser to reuse lane logic
                let lanes = parse_scene_lanes(child_node);
                return Some(SceneContent::Lanes(lanes));
            }
            "ClipSlot" => {
                return Some(SceneContent::Slot(parse_clip_slot(child_node)));
            }
            "Clips" => {
                let clips = children(child_node, "Clip").map(parse_clip).collect();
                return Some(SceneContent::Clips(clips));
            }
            "Notes" => {
                return Some(SceneContent::Notes(parse_notes(child_node)));
            }
            "Markers" | "markers" => {
                let markers = children(child_node, "Marker").map(parse_marker).collect();
                return Some(SceneContent::Markers(markers));
            }
            _ => {}
        }
    }
    None
}

/// Parse lanes inside a scene (same structure as arrangement lanes).
fn parse_scene_lanes(lanes_node: Node<'_, '_>) -> Vec<Lane> {
    child_elements(lanes_node)
        .filter_map(|n| {
            let tag = n.tag_name().name();
            let id = attr(n, "id").unwrap_or("").to_string();
            let track = attr(n, "track").unwrap_or("").to_string();
            let time_unit = attr(n, "timeUnit").map(TimeUnit::from_str);
            let content = match tag {
                "ClipSlot" => {
                    LaneContent::Clips(vec![]) // ClipSlot handled differently
                }
                "Clips" => LaneContent::Clips(children(n, "Clip").map(parse_clip).collect()),
                "Notes" => LaneContent::Notes(parse_notes(n)),
                _ => return None,
            };
            Some(Lane {
                id,
                track,
                time_unit,
                content,
            })
        })
        .collect()
}

pub fn parse_clip_slot(node: Node<'_, '_>) -> ClipSlot {
    let id = attr(node, "id").unwrap_or("").to_string();
    let has_stop = attr_bool(node, "hasStop", false);
    let time = attr(node, "time").and_then(|v| v.parse().ok());
    let duration = attr(node, "duration").and_then(|v| v.parse().ok());
    let clip = child(node, "Clip").map(parse_clip);
    ClipSlot {
        id,
        has_stop,
        time,
        duration,
        clip,
    }
}

pub fn parse_marker(node: Node<'_, '_>) -> Marker {
    Marker {
        time: attr_f64(node, "time", 0.0),
        name: attr(node, "name").unwrap_or("").to_string(),
        color: attr(node, "color").map(str::to_string),
        comment: attr(node, "comment").map(str::to_string),
    }
}
