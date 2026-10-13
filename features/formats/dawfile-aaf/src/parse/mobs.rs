//! Mob parsing — CompositionMob, MasterMob, SourceMob.
//!
//! AAF mobs are the top-level objects in a session:
//! - `CompositionMob` — the main timeline; its `TimelineMobSlot`s are tracks.
//! - `MasterMob` — a logical media asset; its slots each point to a `SourceMob`.
//! - `SourceMob` — actual on-disk media; has an `EssenceDescriptor` with locators.

use crate::error::AafResult;
use crate::parse::auid::{
    CLASS_COMMENT_MARKER, CLASS_EVENT_MOB_SLOT, CLASS_TIMELINE_MOB_SLOT, MobId,
};
use crate::parse::cfb_store::CfbStore;
use crate::parse::essence::{best_locator_url, parse_essence_descriptor};
use crate::parse::pids::*;
use crate::parse::properties::Properties;
use crate::parse::segments::{
    classify_data_def, parse_segment, parse_timecode_segment, resolve_source_clip,
};
use crate::types::{
    AafClip, AafComposition, AafMarker, AafTimecode, AafTrack, AudioEssenceInfo, EditRate,
    TrackKind,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─── Intermediate mob data ────────────────────────────────────────────────────

/// A resolved slot reference collected from a MasterMob.
#[derive(Debug)]
pub struct MasterMobSlotData {
    pub edit_rate: EditRate,
    /// The SourceMob this slot ultimately points to.
    pub source_id: MobId,
    pub source_slot_id: u32,
    pub start_position: i64,
}

/// Intermediate MasterMob data (before SourceClip resolution).
#[derive(Debug)]
pub struct MasterMobData {
    pub mob_id: MobId,
    pub name: String,
    /// slot_id → first SourceClip reference in that slot's Segment
    pub slots: HashMap<u32, MasterMobSlotData>,
}

/// Resolved SourceMob data: file location + audio essence info.
#[derive(Debug)]
pub struct SourceMobData {
    pub mob_id: MobId,
    pub name: String,
    /// Best resolved file URL (from NetworkLocator).
    pub url: Option<String>,
    pub audio_info: Option<AudioEssenceInfo>,
}

// ─── CompositionMob ───────────────────────────────────────────────────────────

/// Parse a `CompositionMob` into an [`AafComposition`].
pub fn parse_composition_mob(
    store: &CfbStore,
    mob_dir: &Path,
    props: &Properties,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
) -> AafResult<AafComposition> {
    let name = props.string(PID_MOB_NAME).unwrap_or_default();

    let (_, slots_coll_name) = match props.strong_ref_collection(PID_MOB_SLOTS) {
        Some(v) => v,
        None => {
            return Ok(AafComposition {
                name,
                tracks: Vec::new(),
                markers: Vec::new(),
                timecode: None,
            });
        }
    };

    let slots_coll_dir = mob_dir.join(&slots_coll_name);
    let slot_dirs = store.vector_elements(&slots_coll_dir);

    let mut tracks = Vec::new();
    let mut markers = Vec::new();
    let mut timecode: Option<AafTimecode> = None;

    for slot_dir in &slot_dirs {
        let slot_raw = match store.properties(slot_dir) {
            Some(r) => r,
            None => continue,
        };
        let slot_props = Properties::parse(slot_raw, slot_dir)?;
        let class = match slot_props.effective_class() {
            Some(c) => c,
            None => continue,
        };

        if class == CLASS_TIMELINE_MOB_SLOT {
            if let Some(track) =
                parse_timeline_slot(store, slot_dir, &slot_props, master_mobs, source_mobs)?
            {
                if track.kind == TrackKind::Timecode {
                    // Extract timecode from the first clip in this track.
                    if let Some(clip) = track.clips.first() {
                        let _ = clip; // Already extracted from segment directly below.
                    }
                    // Re-parse timecode segment directly.
                    if let Some(tc) = parse_tc_from_slot(store, slot_dir, &slot_props) {
                        timecode = Some(tc);
                    }
                } else {
                    tracks.push(track);
                }
            }
        } else if class == CLASS_EVENT_MOB_SLOT {
            let mut slot_markers = parse_event_slot(store, slot_dir, &slot_props)?;
            markers.append(&mut slot_markers);
        }
        // StaticMobSlots are uncommon in audio sessions; skip.
    }

    Ok(AafComposition {
        name,
        tracks,
        markers,
        timecode,
    })
}

// ─── TimelineMobSlot ─────────────────────────────────────────────────────────

fn parse_timeline_slot(
    store: &CfbStore,
    slot_dir: &Path,
    props: &Properties,
    master_mobs: &HashMap<MobId, MasterMobData>,
    source_mobs: &HashMap<MobId, SourceMobData>,
) -> AafResult<Option<AafTrack>> {
    let slot_id = props
        .u32_le_any(&[PID_MOB_SLOT_SLOT_ID, PID_MOB_SLOT_SLOT_ID_AVID])
        .unwrap_or(0);
    let name = props
        .string(PID_MOB_SLOT_SLOT_NAME)
        .or_else(|| props.string(PID_MOB_SLOT_SLOT_NAME_AVID))
        .unwrap_or_default();
    let physical = props.u32_le_any(&[
        PID_MOB_SLOT_PHYSICAL_TRACK_NUMBER,
        PID_MOB_SLOT_PHYSICAL_TRACK_NUMBER_AVID,
    ]);
    let edit_rate = props
        .edit_rate(PID_TIMELINE_MOB_SLOT_EDIT_RATE)
        .unwrap_or(EditRate::AUDIO_48K);
    let origin = props.i64_le(PID_TIMELINE_MOB_SLOT_ORIGIN).unwrap_or(0);

    // Follow strong ref to the Segment (try Avid PID first, then standard).
    let seg_name =
        match props.strong_ref_name_any(&[PID_MOB_SLOT_SEGMENT_AVID, PID_MOB_SLOT_SEGMENT]) {
            Some(n) => n,
            None => return Ok(None),
        };
    let seg_dir = slot_dir.join(seg_name);

    // Determine track kind from the Segment's DataDefinition.
    let kind = data_def_from_segment(store, &seg_dir);

    let clips = parse_segment(store, &seg_dir, origin, master_mobs, source_mobs, 0)?;

    Ok(Some(AafTrack {
        slot_id,
        name,
        physical_track_number: physical,
        edit_rate,
        kind,
        clips,
    }))
}

/// Re-parse the timecode segment from a timecode-carrying slot.
fn parse_tc_from_slot(
    store: &CfbStore,
    slot_dir: &Path,
    props: &Properties,
) -> Option<AafTimecode> {
    let edit_rate = props.edit_rate(PID_TIMELINE_MOB_SLOT_EDIT_RATE)?;
    let seg_name = props.strong_ref_name_any(&[PID_MOB_SLOT_SEGMENT_AVID, PID_MOB_SLOT_SEGMENT])?;
    let seg_dir = slot_dir.join(seg_name);
    parse_timecode_segment(store, &seg_dir, edit_rate)
}

/// Determine the [`TrackKind`] from a segment's `DataDefinition` property.
///
/// In Avid-format files, `Timecode` segments use `DATADEF_PICTURE` as their
/// DataDefinition but have class `CLASS_TIMECODE`.  Check the segment class
/// first so such slots are classified as `Timecode` rather than `Video`.
fn data_def_from_segment(store: &CfbStore, seg_dir: &Path) -> TrackKind {
    use crate::parse::auid::CLASS_TIMECODE;

    let raw = match store.properties(seg_dir) {
        Some(r) => r,
        None => return TrackKind::Other,
    };
    let props = match Properties::parse(raw, seg_dir) {
        Ok(p) => p,
        Err(_) => return TrackKind::Other,
    };
    // Class takes priority over DataDef: Avid timecode segments have Picture DataDef.
    if props.effective_class() == Some(CLASS_TIMECODE) {
        return TrackKind::Timecode;
    }
    // DataDefinition: try standard PID first, then Avid PID.
    match props.auid_any(&[
        PID_COMPONENT_DATA_DEFINITION,
        PID_COMPONENT_DATA_DEFINITION_AVID,
    ]) {
        Some(dd) => classify_data_def(dd),
        None => TrackKind::Other,
    }
}

// ─── EventMobSlot → markers ──────────────────────────────────────────────────

fn parse_event_slot(
    store: &CfbStore,
    slot_dir: &Path,
    props: &Properties,
) -> AafResult<Vec<AafMarker>> {
    let edit_rate = props
        .edit_rate(PID_EVENT_MOB_SLOT_EDIT_RATE)
        .unwrap_or(EditRate::VIDEO_25);

    let seg_name =
        match props.strong_ref_name_any(&[PID_MOB_SLOT_SEGMENT_AVID, PID_MOB_SLOT_SEGMENT]) {
            Some(n) => n,
            None => return Ok(Vec::new()),
        };
    let seg_dir = slot_dir.join(seg_name);

    parse_markers_from_segment(store, &seg_dir, edit_rate)
}

fn parse_markers_from_segment(
    store: &CfbStore,
    seg_dir: &Path,
    edit_rate: EditRate,
) -> AafResult<Vec<AafMarker>> {
    use crate::parse::auid::CLASS_SEQUENCE;
    use crate::parse::pids::{PID_EVENT_COMMENT, PID_EVENT_POSITION};

    let raw = match store.properties(seg_dir) {
        Some(r) => r,
        None => return Ok(Vec::new()),
    };
    let props = Properties::parse(raw, seg_dir)?;
    let class = match props.effective_class() {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };

    if class == CLASS_SEQUENCE {
        // Sequences in EventMobSlots contain CommentMarker Events.
        let (_, coll_name) = match props.strong_ref_collection(PID_SEQUENCE_COMPONENTS) {
            Some(v) => v,
            None => return Ok(Vec::new()),
        };
        let coll_dir = seg_dir.join(&coll_name);
        let comp_dirs = store.vector_elements(&coll_dir);

        let mut markers = Vec::new();
        for comp_dir in comp_dirs {
            let mut sub = parse_markers_from_segment(store, &comp_dir, edit_rate)?;
            markers.append(&mut sub);
        }
        return Ok(markers);
    }

    if class == CLASS_COMMENT_MARKER {
        let position = props
            .i64_le_any(&[PID_EVENT_POSITION, PID_EVENT_POSITION_AVID])
            .unwrap_or(0);
        // Comment text is on PID_EVENT_COMMENT or PID_COMMENT_MARKER_ANNOTATION.
        let comment = props
            .string(PID_EVENT_COMMENT)
            .or_else(|| props.string(PID_COMMENT_MARKER_ANNOTATION))
            .unwrap_or_default();
        return Ok(vec![AafMarker {
            position,
            edit_rate,
            comment,
        }]);
    }

    Ok(Vec::new())
}

// ─── MasterMob ───────────────────────────────────────────────────────────────

/// Collect intermediate `MasterMobData` (first SourceClip reference per slot).
pub fn parse_master_mob(
    store: &CfbStore,
    mob_dir: &Path,
    props: &Properties,
) -> AafResult<Option<MasterMobData>> {
    let mob_id = match props.mob_id(PID_MOB_MOB_ID) {
        Some(id) => id,
        None => return Ok(None),
    };
    let name = props.string(PID_MOB_NAME).unwrap_or_default();

    let (_, slots_coll_name) = match props.strong_ref_collection(PID_MOB_SLOTS) {
        Some(v) => v,
        None => {
            return Ok(Some(MasterMobData {
                mob_id,
                name,
                slots: HashMap::new(),
            }));
        }
    };

    let slots_coll_dir = mob_dir.join(&slots_coll_name);
    let slot_dirs = store.vector_elements(&slots_coll_dir);
    let mut slots = HashMap::new();

    for slot_dir in &slot_dirs {
        if let Some((id, data)) = parse_master_slot(store, slot_dir)? {
            slots.insert(id, data);
        }
    }

    Ok(Some(MasterMobData {
        mob_id,
        name,
        slots,
    }))
}

fn parse_master_slot(
    store: &CfbStore,
    slot_dir: &Path,
) -> AafResult<Option<(u32, MasterMobSlotData)>> {
    let raw = match store.properties(slot_dir) {
        Some(r) => r,
        None => return Ok(None),
    };
    let props = Properties::parse(raw, slot_dir)?;

    let class = props
        .effective_class()
        .unwrap_or(crate::parse::auid::Auid::ZERO);
    if class != CLASS_TIMELINE_MOB_SLOT {
        return Ok(None);
    }

    let slot_id = props
        .u32_le_any(&[PID_MOB_SLOT_SLOT_ID, PID_MOB_SLOT_SLOT_ID_AVID])
        .unwrap_or(0);
    let edit_rate = props
        .edit_rate(PID_TIMELINE_MOB_SLOT_EDIT_RATE)
        .unwrap_or(EditRate::AUDIO_48K);

    let seg_name =
        match props.strong_ref_name_any(&[PID_MOB_SLOT_SEGMENT_AVID, PID_MOB_SLOT_SEGMENT]) {
            Some(n) => n,
            None => return Ok(None),
        };
    let seg_dir = slot_dir.join(seg_name);

    // We need the first SourceClip in the segment to know where this slot
    // references.  Walk until we find one.
    let (source_id, source_slot_id, start_position) =
        find_first_source_clip(store, &seg_dir).unwrap_or((MobId::ZERO, 0, 0));

    if source_id.is_zero() {
        return Ok(None);
    }

    Ok(Some((
        slot_id,
        MasterMobSlotData {
            edit_rate,
            source_id,
            source_slot_id,
            start_position,
        },
    )))
}

/// Recursively walk a segment tree to find the first `SourceClip`.
fn find_first_source_clip(store: &CfbStore, seg_dir: &Path) -> Option<(MobId, u32, i64)> {
    use crate::parse::auid::CLASS_SEQUENCE;

    let raw = store.properties(seg_dir)?;
    let props = Properties::parse(raw, seg_dir).ok()?;
    let class = props.effective_class()?;

    if class == crate::parse::auid::CLASS_SOURCE_CLIP {
        let id = props
            .mob_id(PID_SOURCE_REF_SOURCE_ID)
            .unwrap_or(MobId::ZERO);
        let slot = props.u32_le(PID_SOURCE_REF_MOB_SLOT_ID).unwrap_or(0);
        let start = props.i64_le(PID_SOURCE_CLIP_START_POSITION).unwrap_or(0);
        return Some((id, slot, start));
    }

    if class == CLASS_SEQUENCE {
        let (_, coll_name) = props.strong_ref_collection(PID_SEQUENCE_COMPONENTS)?;
        let coll_dir = seg_dir.join(&coll_name);
        for child in store.vector_elements(&coll_dir) {
            if let Some(r) = find_first_source_clip(store, &child) {
                return Some(r);
            }
        }
    }

    None
}

// ─── SourceMob ───────────────────────────────────────────────────────────────

/// Parse a `SourceMob` into resolved [`SourceMobData`].
pub fn parse_source_mob(
    store: &CfbStore,
    mob_dir: &Path,
    props: &Properties,
) -> AafResult<Option<SourceMobData>> {
    let mob_id = match props.mob_id(PID_MOB_MOB_ID) {
        Some(id) => id,
        None => return Ok(None),
    };
    let name = props.string(PID_MOB_NAME).unwrap_or_default();

    // Navigate to EssenceDescriptor.
    let desc_name = match props.strong_ref_name(PID_SOURCE_MOB_ESSENCE_DESCRIPTION) {
        Some(n) => n,
        None => {
            return Ok(Some(SourceMobData {
                mob_id,
                name,
                url: None,
                audio_info: None,
            }));
        }
    };
    let desc_dir = mob_dir.join(desc_name);
    let desc = parse_essence_descriptor(store, &desc_dir)?;

    Ok(Some(SourceMobData {
        mob_id,
        name,
        url: best_locator_url(&desc),
        audio_info: desc.audio_info,
    }))
}
