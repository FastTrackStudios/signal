//! Top-level parse orchestration.
//!
//! Parsing an AAF file proceeds in five stages:
//!
//! 1. **Load CFB** — read all `"properties"` and `"index"` streams into
//!    memory via [`CfbStore`].
//! 2. **Header → ContentStorage** — locate the `Mobs` collection.
//! 3. **First pass** — collect all `SourceMob` objects into a lookup table.
//! 4. **Second pass** — classify remaining mobs as `MasterMob` or
//!    `CompositionMob` by inspecting the first slot's segment type.
//! 5. **Third pass** — parse each `CompositionMob`, resolving `SourceClip`
//!    chains via the lookup tables.
//! 6. **Assemble** — derive `session_sample_rate` and package the result.

pub mod auid;
pub mod cfb_store;
pub mod essence;
pub mod mobs;
pub mod pids;
pub mod properties;
pub mod segments;

use crate::error::{AafError, AafResult};
use crate::parse::auid::MobId;
use crate::parse::auid::{CLASS_COMPOSITION_MOB, CLASS_MASTER_MOB, CLASS_SOURCE_MOB};
use crate::parse::cfb_store::CfbStore;
use crate::parse::mobs::{
    MasterMobData, SourceMobData, parse_composition_mob, parse_master_mob, parse_source_mob,
};
use crate::parse::pids::*;
use crate::parse::properties::Properties;
use crate::types::{AafComposition, AafMarker, AafSession, AafTimecode, AafTrack};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parse an AAF file at `path` and return a fully resolved [`AafSession`].
pub fn parse_session(path: &Path) -> AafResult<AafSession> {
    let store = CfbStore::load(path)?;

    // ── Stage 2: Header → ContentStorage ────────────────────────────────────
    let header_dir = find_header_dir(&store)?;

    let header_raw = store
        .properties(&header_dir)
        .ok_or_else(|| AafError::ObjectNotFound(header_dir.clone()))?;
    let header_props = Properties::parse(header_raw, &header_dir)?;

    let object_model_version = header_props
        .u32_le(PID_HEADER_OBJECT_MODEL_VERSION)
        .unwrap_or(1);

    // ContentStorage strong ref — try Avid PID first, then standard.
    let cs_name = header_props
        .strong_ref_name_any(&[PID_HEADER_CONTENT_STORAGE_AVID, PID_HEADER_CONTENT_STORAGE])
        .ok_or_else(|| AafError::MissingProperty {
            pid: PID_HEADER_CONTENT_STORAGE,
            path: header_dir.clone(),
        })?;
    let cs_dir = header_dir.join(&cs_name);

    let cs_raw = store
        .properties(&cs_dir)
        .ok_or_else(|| AafError::ObjectNotFound(cs_dir.clone()))?;
    let cs_props = Properties::parse(cs_raw, &cs_dir)?;

    let (_, mobs_coll_name) = cs_props
        .strong_ref_collection(PID_CONTENT_STORAGE_MOBS)
        .ok_or_else(|| AafError::MissingProperty {
            pid: PID_CONTENT_STORAGE_MOBS,
            path: cs_dir.clone(),
        })?;
    let mobs_coll_dir = cs_dir.join(&mobs_coll_name);

    // ── Stage 3: First pass — collect SourceMobs ─────────────────────────────
    let mob_dirs = store.set_elements(&mobs_coll_dir);

    let mut source_mobs: HashMap<_, SourceMobData> = HashMap::new();
    let mut unknown_mob_dirs: Vec<PathBuf> = Vec::new();

    for mob_dir in &mob_dirs {
        let mob_raw = match store.properties(mob_dir) {
            Some(r) => r,
            None => continue,
        };
        let mob_props = Properties::parse(mob_raw, mob_dir)?;

        let class = mob_props.class_auid();

        if class == Some(CLASS_SOURCE_MOB)
            || mob_props.get(PID_SOURCE_MOB_ESSENCE_DESCRIPTION).is_some()
        {
            if let Some(data) = parse_source_mob(&store, mob_dir, &mob_props)? {
                source_mobs.insert(data.mob_id, data);
            }
        } else {
            unknown_mob_dirs.push(mob_dir.clone());
        }
    }

    // ── Stage 4: Second pass — classify MasterMobs vs CompositionMobs ────────
    let mut master_mobs: HashMap<_, MasterMobData> = HashMap::new();
    let mut comp_mob_dirs: Vec<PathBuf> = Vec::new();

    for mob_dir in &unknown_mob_dirs {
        let mob_raw = store.properties(mob_dir).unwrap(); // safe: checked above
        let mob_props = Properties::parse(mob_raw, mob_dir)?;

        let class = mob_props.class_auid();

        if class == Some(CLASS_MASTER_MOB)
            || (class.is_none() && is_master_mob(&store, mob_dir, &mob_props, &source_mobs))
        {
            if let Some(data) = parse_master_mob(&store, mob_dir, &mob_props)? {
                master_mobs.insert(data.mob_id, data);
            }
        } else {
            // CLASS_COMPOSITION_MOB or unknown → treat as composition
            comp_mob_dirs.push(mob_dir.clone());
        }
    }

    // ── Derive session sample rate ───────────────────────────────────────────
    let session_sample_rate = source_mobs
        .values()
        .filter_map(|s| s.audio_info)
        .map(|a| a.sample_rate)
        .find(|&r| r >= 8000)
        .unwrap_or(48000);

    // ── Stage 5: Parse CompositionMobs ───────────────────────────────────────
    let mut compositions: Vec<AafComposition> = Vec::new();

    for comp_dir in &comp_mob_dirs {
        let comp_raw = store.properties(comp_dir).unwrap();
        let comp_props = Properties::parse(comp_raw, comp_dir)?;
        let comp =
            parse_composition_mob(&store, comp_dir, &comp_props, &master_mobs, &source_mobs)?;
        compositions.push(comp);
    }

    // ── Stage 6: Assemble ────────────────────────────────────────────────────
    let (tracks, markers, timecode_start) = primary_composition_data(&compositions);

    Ok(AafSession {
        object_model_version,
        session_sample_rate,
        tracks,
        markers,
        timecode_start,
        compositions,
    })
}

// ─── Header location ─────────────────────────────────────────────────────────

/// Find the directory containing the AAF `Header` object.
///
/// In standard AAF the `Header` is at the CFB root (`/`).  In Avid-format
/// files the root's `properties` stream contains a strong ref (PID = 0x0002)
/// pointing to a child named `Header-2` (or similar).
fn find_header_dir(store: &CfbStore) -> AafResult<PathBuf> {
    let root = PathBuf::from("/");

    if let Some(raw) = store.properties(&root)
        && let Ok(props) = Properties::parse(raw, &root)
    {
        // Standard format: root IS the Header (has ContentStorage ref).
        if props.get(PID_HEADER_CONTENT_STORAGE).is_some()
            || props.get(PID_HEADER_CONTENT_STORAGE_AVID).is_some()
        {
            return Ok(root);
        }

        // Avid format: root has PID=0x0002 pointing to the Header child.
        if let Some(header_name) = props.strong_ref_name(0x0002) {
            let header_dir = root.join(&header_name);
            if store.properties(&header_dir).is_some() {
                return Ok(header_dir);
            }
        }
    }

    // Fallback: scan root children for a directory whose name starts with
    // "Header" (e.g. "Header-2" in Avid files).
    for child in store.child_dirs(&root) {
        let name = child.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("Header") {
            return Ok(child);
        }
    }

    Ok(root)
}

// ─── Mob type heuristic ──────────────────────────────────────────────────────

/// Determine whether a mob is a `MasterMob` when the class AUID is absent
/// (Avid format).
///
/// A `MasterMob`'s first slot contains a `SourceClip` (possibly wrapped in a
/// `Sequence`) that references a known `SourceMob`.  We walk the segment tree
/// of the first slot to find the source MobID and check it against the already-
/// collected `source_mobs` table.
fn is_master_mob(
    store: &CfbStore,
    mob_dir: &Path,
    props: &Properties,
    source_mobs: &HashMap<MobId, SourceMobData>,
) -> bool {
    let slots_name = match props.strong_ref_collection(PID_MOB_SLOTS) {
        Some((_, name)) => name,
        None => return false,
    };
    let slots_coll_dir = mob_dir.join(&slots_name);
    let slot_dirs = store.vector_elements(&slots_coll_dir);

    let slot_dir = match slot_dirs.first() {
        Some(d) => d,
        None => return false,
    };

    let slot_raw = match store.properties(slot_dir) {
        Some(r) => r,
        None => return false,
    };
    let slot_props = match Properties::parse(slot_raw, slot_dir) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let seg_name =
        match slot_props.strong_ref_name_any(&[PID_MOB_SLOT_SEGMENT_AVID, PID_MOB_SLOT_SEGMENT]) {
            Some(n) => n,
            None => return false,
        };
    let seg_dir = slot_dir.join(&seg_name);

    // Walk the segment tree to find the first SourceClip, then check whether
    // its source MobID is a known SourceMob (→ MasterMob) or not (→ CompositionMob).
    match find_first_source_ref(store, &seg_dir) {
        Some(id) if !id.is_zero() => source_mobs.contains_key(&id),
        _ => false,
    }
}

/// Recursively search a segment tree for the first `SourceClip`, returning its
/// `SourceID` MobID.  Handles `Sequence` wrapping.
fn find_first_source_ref(store: &CfbStore, seg_dir: &Path) -> Option<MobId> {
    use crate::parse::auid::{CLASS_SEQUENCE, CLASS_SOURCE_CLIP};

    let raw = store.properties(seg_dir)?;
    let props = Properties::parse(raw, seg_dir).ok()?;
    let class = props.effective_class()?;

    if class == CLASS_SOURCE_CLIP {
        return Some(
            props
                .mob_id(PID_SOURCE_REF_SOURCE_ID)
                .unwrap_or(MobId::ZERO),
        );
    }

    if class == CLASS_SEQUENCE {
        let (_, coll_name) = props.strong_ref_collection(PID_SEQUENCE_COMPONENTS)?;
        let coll_dir = seg_dir.join(&coll_name);
        for child in store.vector_elements(&coll_dir) {
            if let Some(id) = find_first_source_ref(store, &child) {
                return Some(id);
            }
        }
    }

    None
}

// ─── Assemble helpers ────────────────────────────────────────────────────────

/// Extract the top-level `tracks`, `markers`, and `timecode` from the primary
/// `CompositionMob`.  When there are multiple compositions (e.g. Avid files
/// that include MasterMobs mis-classified before heuristic fix), prefer the
/// composition with the most tracks as it is most likely the main timeline.
fn primary_composition_data(
    compositions: &[AafComposition],
) -> (Vec<AafTrack>, Vec<AafMarker>, Option<AafTimecode>) {
    let best = compositions
        .iter()
        .max_by_key(|c| c.tracks.len())
        .or_else(|| compositions.first());
    match best {
        Some(c) => (c.tracks.clone(), c.markers.clone(), c.timecode),
        None => (Vec::new(), Vec::new(), None),
    }
}
