//! Segment parsing — the recursive timeline content of a MobSlot.
//!
//! A `MobSlot.Segment` is the root of a tree that can contain:
//! - `Sequence` — ordered list of sub-components
//! - `SourceClip` — reference to media at a specific point in another Mob
//! - `Filler` — silence / empty space
//! - `Transition` — dissolve between two adjacent clips
//! - `OperationGroup` — speed change or other effect wrapping sub-clips
//! - `Timecode` — SMPTE timecode segment
//! - `CommentMarker` / `DescriptiveMarker` — text annotation (handled in mobs.rs)
//!
//! All positions returned here are in **edit units** of the enclosing
//! `TimelineMobSlot.EditRate`.  The caller is responsible for converting to
//! samples using [`EditRate::to_samples`].

use crate::error::{AafError, AafResult};
use crate::parse::auid::{
    Auid, CLASS_FILLER, CLASS_OPERATION_GROUP, CLASS_SELECTOR, CLASS_SEQUENCE, CLASS_SOURCE_CLIP,
    CLASS_TIMECODE, CLASS_TRANSITION, MobId,
};
use crate::parse::cfb_store::CfbStore;
use crate::parse::pids::*;
use crate::parse::properties::Properties;
use crate::types::{AafClip, AafTimecode, AudioEssenceInfo, ClipKind, EditRate};
use std::collections::HashMap;
use std::path::Path;

use super::mobs::{MasterMobData, SourceMobData};

/// Maximum recursion depth for nested sequences/operations.
const MAX_DEPTH: usize = 16;

/// Parse the `Segment` object at `segment_dir` into a flat list of
/// [`AafClip`]s accumulated from `origin` onward.
///
/// `origin` is the absolute timeline position (in edit units of the
/// enclosing `TimelineMobSlot`) at which this segment starts.
pub fn parse_segment(
    store: &CfbStore,
    segment_dir: &Path,
    origin: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
    depth: usize,
) -> AafResult<Vec<AafClip>> {
    if depth > MAX_DEPTH {
        return Ok(Vec::new());
    }

    let props_raw = store
        .properties(segment_dir)
        .ok_or_else(|| AafError::ObjectNotFound(segment_dir.to_path_buf()))?;
    let props = Properties::parse(props_raw, segment_dir)?;

    let class = match props.effective_class() {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
    let length = props
        .i64_le_any(&[PID_COMPONENT_LENGTH, PID_COMPONENT_LENGTH_AVID])
        .unwrap_or(0);

    if class == CLASS_SEQUENCE {
        parse_sequence(
            store,
            segment_dir,
            &props,
            origin,
            master_mobs,
            source_mobs,
            depth,
        )
    } else if class == CLASS_SOURCE_CLIP {
        parse_source_clip(
            segment_dir,
            &props,
            origin,
            length,
            master_mobs,
            source_mobs,
        )
    } else if class == CLASS_FILLER {
        Ok(vec![AafClip {
            start_position: origin,
            length,
            kind: ClipKind::Filler,
        }])
    } else if class == CLASS_TRANSITION {
        parse_transition(segment_dir, &props, origin, length)
    } else if class == CLASS_OPERATION_GROUP {
        parse_operation_group(
            store,
            segment_dir,
            &props,
            origin,
            length,
            master_mobs,
            source_mobs,
            depth,
        )
    } else if class == CLASS_SELECTOR {
        // A Selector wraps a preferred segment and optional alternates.
        // Parse the "Selected" strong ref only.
        parse_selector(
            store,
            segment_dir,
            &props,
            origin,
            master_mobs,
            source_mobs,
            depth,
        )
    } else if class == CLASS_TIMECODE {
        // Timecode segments are handled separately in the mob layer; skip here.
        Ok(Vec::new())
    } else {
        // Unknown / unsupported segment type — return an Other placeholder.
        Ok(vec![AafClip {
            start_position: origin,
            length,
            kind: ClipKind::Other,
        }])
    }
}

// ─── Sequence ────────────────────────────────────────────────────────────────

fn parse_sequence(
    store: &CfbStore,
    seq_dir: &Path,
    props: &Properties,
    origin: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
    depth: usize,
) -> AafResult<Vec<AafClip>> {
    let (_, coll_name) = match props.strong_ref_collection(PID_SEQUENCE_COMPONENTS) {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };

    let coll_dir = seq_dir.join(&coll_name);
    let comp_dirs = store.vector_elements(&coll_dir);

    let mut clips = Vec::new();
    let mut pos = origin;

    for comp_dir in comp_dirs {
        // Get length first so we can advance position even if parsing fails.
        let comp_len = component_length(store, &comp_dir);

        let mut sub = parse_segment(store, &comp_dir, pos, master_mobs, source_mobs, depth + 1)?;
        clips.append(&mut sub);

        pos += comp_len;
    }

    Ok(clips)
}

// ─── SourceClip ──────────────────────────────────────────────────────────────

fn parse_source_clip(
    _dir: &Path,
    props: &Properties,
    origin: i64,
    length: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
) -> AafResult<Vec<AafClip>> {
    let source_id = props
        .mob_id(PID_SOURCE_REF_SOURCE_ID)
        .unwrap_or(MobId::ZERO);
    let source_slot_id = props.u32_le(PID_SOURCE_REF_MOB_SLOT_ID).unwrap_or(0);
    let start_position = props.i64_le(PID_SOURCE_CLIP_START_POSITION).unwrap_or(0);

    if source_id.is_zero() {
        // Zero source ID = Filler in disguise (original recording placeholder).
        return Ok(vec![AafClip {
            start_position: origin,
            length,
            kind: ClipKind::Filler,
        }]);
    }

    let kind = resolve_source_clip(
        source_id,
        source_slot_id,
        start_position,
        master_mobs,
        source_mobs,
    );

    Ok(vec![AafClip {
        start_position: origin,
        length,
        kind,
    }])
}

/// Walk the `CompositionMob → MasterMob → SourceMob` reference chain and
/// produce a fully resolved [`ClipKind::SourceClip`].
pub fn resolve_source_clip(
    source_id: MobId,
    source_slot_id: u32,
    start_position: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
) -> ClipKind {
    // --- Check if source_id points directly to a SourceMob ---
    if let Some(src) = source_mobs.get(&source_id) {
        return ClipKind::SourceClip {
            source_file: src.url.clone(),
            source_mob_id: source_id.0,
            source_slot_id,
            source_start: start_position,
            audio_info: src.audio_info,
        };
    }

    // --- Walk through MasterMob → SourceMob ---
    if let Some(master) = master_mobs.get(&source_id) {
        if let Some(slot) = master.slots.get(&source_slot_id) {
            let resolved_src = source_mobs.get(&slot.source_id);
            return ClipKind::SourceClip {
                source_file: resolved_src.and_then(|s| s.url.clone()),
                source_mob_id: slot.source_id.0,
                source_slot_id: slot.source_slot_id,
                source_start: slot.start_position,
                audio_info: resolved_src.and_then(|s| s.audio_info),
            };
        }
        // MasterMob found but slot not present — return with known mob ID.
        return ClipKind::SourceClip {
            source_file: None,
            source_mob_id: source_id.0,
            source_slot_id,
            source_start: start_position,
            audio_info: None,
        };
    }

    // No matching mob at all.
    ClipKind::SourceClip {
        source_file: None,
        source_mob_id: source_id.0,
        source_slot_id,
        source_start: start_position,
        audio_info: None,
    }
}

// ─── Transition ──────────────────────────────────────────────────────────────

fn parse_transition(
    _dir: &Path,
    props: &Properties,
    origin: i64,
    length: i64,
) -> AafResult<Vec<AafClip>> {
    let cut_point = props.i64_le(PID_TRANSITION_CUT_POINT).unwrap_or(length / 2);
    Ok(vec![AafClip {
        start_position: origin,
        length,
        kind: ClipKind::Transition { cut_point },
    }])
}

// ─── OperationGroup ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn parse_operation_group(
    store: &CfbStore,
    op_dir: &Path,
    props: &Properties,
    origin: i64,
    length: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
    depth: usize,
) -> AafResult<Vec<AafClip>> {
    // Parse input segments (the actual media).
    let input_dirs = match props.strong_ref_collection(PID_OPERATION_GROUP_INPUT_SEGMENTS) {
        Some((_, name)) => {
            let coll = op_dir.join(name);
            store.vector_elements(&coll)
        }
        None => Vec::new(),
    };

    let input_count = input_dirs.len();

    if input_count == 0 {
        // No inputs — return an Operation placeholder.
        return Ok(vec![AafClip {
            start_position: origin,
            length,
            kind: ClipKind::Operation { input_count: 0 },
        }]);
    }

    // For the common case (one input = speed change, gain, etc.) just recurse
    // into the single input and return its clips unchanged.  For multi-input
    // operations (transitions, mixdowns) we still recurse but note the count.
    let mut clips = Vec::new();
    for input_dir in &input_dirs {
        let mut sub = parse_segment(
            store,
            input_dir,
            origin,
            master_mobs,
            source_mobs,
            depth + 1,
        )?;
        clips.append(&mut sub);
    }

    if input_count > 1 {
        // Wrap with an Operation marker clip so the caller knows this is a
        // multi-input operation (e.g. cross-dissolve).
        clips.insert(
            0,
            AafClip {
                start_position: origin,
                length,
                kind: ClipKind::Operation { input_count },
            },
        );
    }

    Ok(clips)
}

// ─── Selector ────────────────────────────────────────────────────────────────

fn parse_selector(
    store: &CfbStore,
    sel_dir: &Path,
    props: &Properties,
    origin: i64,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
    depth: usize,
) -> AafResult<Vec<AafClip>> {
    // PID 0x0E01 = Selected (strong ref to preferred segment).
    // Only parse the preferred ("Selected") segment; ignore alternates.
    const PID_SELECTOR_SELECTED: u16 = 0x0E01;
    if let Some(name) = props.strong_ref_name(PID_SELECTOR_SELECTED) {
        let child = sel_dir.join(name);
        return parse_segment(store, &child, origin, master_mobs, source_mobs, depth + 1);
    }
    Ok(Vec::new())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Read component length from a directory, or return 0.
fn component_length(store: &CfbStore, dir: &Path) -> i64 {
    let raw = match store.properties(dir) {
        Some(r) => r,
        None => return 0,
    };
    Properties::parse(raw, dir)
        .ok()
        .and_then(|p| p.i64_le_any(&[PID_COMPONENT_LENGTH, PID_COMPONENT_LENGTH_AVID]))
        .unwrap_or(0)
}

// ─── Timecode segment ────────────────────────────────────────────────────────

/// Parse a `Timecode` segment into an [`AafTimecode`].
pub fn parse_timecode_segment(
    store: &CfbStore,
    seg_dir: &Path,
    slot_edit_rate: EditRate,
) -> Option<AafTimecode> {
    let raw = store.properties(seg_dir)?;
    let props = Properties::parse(raw, seg_dir).ok()?;

    let class = props.effective_class()?;
    if class != CLASS_TIMECODE {
        return None;
    }

    let start = props.i64_le(PID_TIMECODE_START).unwrap_or(0);
    let fps = props.u16_le(PID_TIMECODE_FPS).unwrap_or(25);
    let drop_frame = props.u8(PID_TIMECODE_DROP).map(|v| v != 0).unwrap_or(false);

    Some(AafTimecode {
        start,
        fps,
        drop_frame,
        edit_rate: slot_edit_rate,
    })
}

/// Classify a DataDefinition AUID into a broad track-kind category.
pub fn classify_data_def(data_def: Auid) -> crate::types::TrackKind {
    use crate::parse::auid::{
        DATADEF_LEGACY_TC, DATADEF_OMF_PICTURE, DATADEF_OMF_SOUND, DATADEF_OMF_TIMECODE,
        DATADEF_PICTURE, DATADEF_SOUND, DATADEF_SOUND_V2, DATADEF_TIMECODE,
    };
    use crate::types::TrackKind;

    if data_def == DATADEF_SOUND || data_def == DATADEF_SOUND_V2 || data_def == DATADEF_OMF_SOUND {
        TrackKind::Audio
    } else if data_def == DATADEF_PICTURE || data_def == DATADEF_OMF_PICTURE {
        TrackKind::Video
    } else if data_def == DATADEF_TIMECODE
        || data_def == DATADEF_LEGACY_TC
        || data_def == DATADEF_OMF_TIMECODE
    {
        TrackKind::Timecode
    } else {
        TrackKind::Other
    }
}
