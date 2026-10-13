//! AAF write support — serialise an [`AafSession`] to a `.aaf` file.
//!
//! Produces a minimal, valid OP-1a AAF file:
//! - One `CompositionMob` with a `TimelineMobSlot` per track
//! - One `EventMobSlot` carrying timeline markers (if any)
//! - One `MasterMob` per unique source file (mob ID)
//! - One `SourceMob` per unique source file, with `PCMDescriptor` and
//!   `NetworkLocator` (file URL) where the data is available

pub mod mob_id;
pub mod properties_writer;

use crate::error::{AafError, AafResult};
use crate::parse::auid::{
    Auid, CLASS_COMMENT_MARKER, CLASS_COMPOSITION_MOB, CLASS_EVENT_MOB_SLOT, CLASS_FILLER,
    CLASS_MASTER_MOB, CLASS_NETWORK_LOCATOR, CLASS_PCM_DESCRIPTOR, CLASS_SEQUENCE,
    CLASS_SOURCE_CLIP, CLASS_SOURCE_MOB, CLASS_TIMELINE_MOB_SLOT, DATADEF_DESCRIPTIVE,
    DATADEF_PICTURE, DATADEF_SOUND, MobId,
};
use crate::parse::pids::*;
use crate::types::{
    AafClip, AafMarker, AafSession, AafTrack, AudioEssenceInfo, ClipKind, EditRate, TrackKind,
};
use properties_writer::{PropWriter, set_index, vector_index};
use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

// ─── Operational pattern AUID (OP-1a, single track composition) ──────────────
// Raw bytes produced by auid(0x0D010201, 0x0101, 0x0100, [0x06,0x0E,0x2B,0x34,0x04,0x01,0x01,0x05])
const AUID_OP1A: Auid = Auid([
    0x01, 0x02, 0x01, 0x0D, 0x01, 0x01, 0x00, 0x01, 0x06, 0x0E, 0x2B, 0x34, 0x04, 0x01, 0x01, 0x05,
]);

// ─── Public API ───────────────────────────────────────────────────────────────

/// Write `session` to a new AAF file at `path`.
///
/// Overwrites any existing file.  The produced file is a minimal OP-1a AAF
/// with external media references; no audio data is embedded.
pub fn write_session(path: impl AsRef<Path>, session: &AafSession) -> AafResult<()> {
    let path = path.as_ref();

    // ── Collect unique source mobs ───────────────────────────────────────────
    let source_entries = collect_source_entries(session);
    let n_source = source_entries.len();

    // ── Fresh MobIDs ─────────────────────────────────────────────────────────
    let comp_mob_id = mob_id::new_mob_id();
    // Map source_mob_id → master_mob_id (one MasterMob per SourceMob)
    let mut master_mob_map: HashMap<[u8; 32], MobId> = HashMap::new();
    for entry in &source_entries {
        master_mob_map.insert(entry.mob_id.0, mob_id::new_mob_id());
    }

    // ── Build CFB streams ────────────────────────────────────────────────────
    let total_mobs = (1 + n_source + n_source) as u32; // comp + master + source
    let mut builder = CfbBuilder::default();

    let mobs_dir = PathBuf::from("/ContentStorage/Mobs");

    // Header (root `/`)
    build_header(&mut builder, &comp_mob_id, total_mobs);

    // ContentStorage
    build_content_storage(&mut builder, total_mobs, n_source);

    // CompositionMob at Mobs/00000000
    build_composition_mob(
        &mut builder,
        &mobs_dir.join("00000000"),
        session,
        &comp_mob_id,
        &master_mob_map,
    )?;

    // MasterMobs at Mobs/00000001 … Mobs/N
    for (i, entry) in source_entries.iter().enumerate() {
        let master_id = master_mob_map[&entry.mob_id.0];
        build_master_mob(
            &mut builder,
            &mobs_dir.join(format!("{:08x}", i + 1)),
            &master_id,
            entry,
        );
    }

    // SourceMobs at Mobs/N+1 … Mobs/2N
    for (i, entry) in source_entries.iter().enumerate() {
        build_source_mob(
            &mut builder,
            &mobs_dir.join(format!("{:08x}", 1 + n_source + i)),
            entry,
        );
    }

    // Mobs set index — class AUID per mob
    let mut mob_class_auids = vec![CLASS_COMPOSITION_MOB];
    mob_class_auids.extend(vec![CLASS_MASTER_MOB; n_source]);
    mob_class_auids.extend(vec![CLASS_SOURCE_MOB; n_source]);
    builder.add_stream(mobs_dir.join("index"), set_index(&mob_class_auids));

    // EssenceData — always empty for external media
    let essence_dir = PathBuf::from("/ContentStorage/EssenceData");
    builder.add_storage(essence_dir.clone());
    builder.add_stream(essence_dir.join("index"), set_index(&[]));

    builder.write_to_file(path)
}

// ─── Source entry collection ──────────────────────────────────────────────────

/// De-duplicated metadata about a unique source file referenced in the session.
struct SourceEntry {
    /// Original SourceMob UMID (from the parsed clip data).
    mob_id: MobId,
    /// Resolved file URL or path (used for NetworkLocator).
    source_file: Option<String>,
    /// Audio format metadata (from PCMDescriptor).
    audio_info: Option<AudioEssenceInfo>,
    /// Slot ID within the SourceMob that the media lives on.
    source_slot_id: u32,
    /// Total duration of the source media in its native edit units.
    total_length: i64,
    /// Native edit rate of the source media.
    edit_rate: EditRate,
}

fn collect_source_entries(session: &AafSession) -> Vec<SourceEntry> {
    let mut entries: Vec<SourceEntry> = Vec::new();
    let mut seen: HashSet<[u8; 32]> = HashSet::new();

    for track in &session.tracks {
        for clip in &track.clips {
            if let ClipKind::SourceClip {
                source_mob_id,
                source_slot_id,
                audio_info,
                source_file,
                ..
            } = &clip.kind
                && seen.insert(*source_mob_id)
            {
                let edit_rate = audio_info
                    .map(|a| EditRate {
                        numerator: a.sample_rate as i32,
                        denominator: 1,
                    })
                    .unwrap_or(EditRate {
                        numerator: session.session_sample_rate as i32,
                        denominator: 1,
                    });
                let total_length = audio_info.map(|a| a.length_samples).unwrap_or(i64::MAX / 2);

                entries.push(SourceEntry {
                    mob_id: MobId(*source_mob_id),
                    source_file: source_file.clone(),
                    audio_info: *audio_info,
                    source_slot_id: *source_slot_id,
                    total_length,
                    edit_rate,
                });
            }
        }
    }

    entries
}

// ─── CFB builder ─────────────────────────────────────────────────────────────

/// Accumulates CFB storages and streams, then writes them to disk in one pass.
#[derive(Default)]
struct CfbBuilder {
    /// Storages in creation order (parent before child, guaranteed by ensure_storage).
    storages: Vec<PathBuf>,
    storage_set: HashSet<PathBuf>,
    streams: Vec<(PathBuf, Vec<u8>)>,
}

impl CfbBuilder {
    /// Ensure the storage at `dir` (and all its ancestors above root) exists.
    fn ensure_storage(&mut self, dir: &Path) {
        if dir == Path::new("/") || dir.as_os_str().is_empty() {
            return;
        }
        // Recurse to guarantee parent-before-child ordering.
        if let Some(parent) = dir.parent() {
            self.ensure_storage(parent);
        }
        let pb = dir.to_path_buf();
        if !self.storage_set.contains(&pb) {
            self.storage_set.insert(pb.clone());
            self.storages.push(pb);
        }
    }

    fn add_storage(&mut self, dir: PathBuf) {
        self.ensure_storage(&dir.clone());
    }

    fn add_stream(&mut self, path: PathBuf, data: Vec<u8>) {
        if let Some(parent) = path.parent() {
            self.ensure_storage(parent);
        }
        self.streams.push((path, data));
    }

    /// Add an object's properties stream (and create its storage directory).
    fn add_props(&mut self, dir: &Path, props: PropWriter) {
        self.ensure_storage(dir);
        self.streams.push((dir.join("properties"), props.finish()));
    }

    /// Write all collected storages and streams to a new CFB file.
    fn write_to_file(self, path: &Path) -> AafResult<()> {
        let mut comp = cfb::create(path).map_err(AafError::Io)?;
        for storage in &self.storages {
            comp.create_storage(storage).map_err(AafError::Io)?;
        }
        for (stream_path, data) in &self.streams {
            let mut stream = comp.create_stream(stream_path).map_err(AafError::Io)?;
            stream.write_all(data).map_err(AafError::Io)?;
        }
        Ok(())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn kind_to_data_def(kind: &TrackKind) -> Auid {
    match kind {
        TrackKind::Audio => DATADEF_SOUND,
        TrackKind::Video => DATADEF_PICTURE,
        _ => DATADEF_SOUND,
    }
}

/// A dummy 16-byte AAF timestamp (all zeros — "unknown/unset").
fn zero_timestamp() -> Vec<u8> {
    vec![0u8; 16]
}

// ─── Header ──────────────────────────────────────────────────────────────────

fn build_header(builder: &mut CfbBuilder, comp_mob_id: &MobId, total_mobs: u32) {
    let _ = (comp_mob_id, total_mobs); // used indirectly through ContentStorage

    let mut p = PropWriter::new();
    p.class_id(crate::parse::auid::CLASS_HEADER);
    // Byte order: 0x4949 = little-endian
    p.u16_prop(PID_HEADER_BYTE_ORDER, 0x4949);
    // AAF version 1.1
    p.data(PID_HEADER_VERSION, &[0x01, 0x00, 0x01, 0x00]);
    // Object model version = 1
    p.u32_prop(PID_HEADER_OBJECT_MODEL_VERSION, 1);
    // Last-modified timestamp (zero = unset)
    p.data(PID_HEADER_LAST_MODIFIED, &zero_timestamp());
    // ContentStorage strong ref
    p.strong_ref(PID_HEADER_CONTENT_STORAGE, "ContentStorage");
    // Operational pattern: OP-1a
    p.auid_prop(PID_HEADER_OPERATIONAL_PATTERN, AUID_OP1A);

    builder.add_props(Path::new("/"), p);
}

// ─── ContentStorage ───────────────────────────────────────────────────────────

fn build_content_storage(builder: &mut CfbBuilder, total_mobs: u32, _n_source: usize) {
    let cs_dir = PathBuf::from("/ContentStorage");

    let mut p = PropWriter::new();
    p.class_id(crate::parse::auid::CLASS_CONTENT_STORAGE);
    p.strong_ref_set(PID_CONTENT_STORAGE_MOBS, "Mobs", total_mobs);
    p.strong_ref_set(PID_CONTENT_STORAGE_ESSENCE_DATA, "EssenceData", 0);

    builder.add_props(&cs_dir, p);
}

// ─── CompositionMob ──────────────────────────────────────────────────────────

fn build_composition_mob(
    builder: &mut CfbBuilder,
    comp_dir: &Path,
    session: &AafSession,
    comp_mob_id: &MobId,
    master_mob_map: &HashMap<[u8; 32], MobId>,
) -> AafResult<()> {
    let name = session
        .compositions
        .first()
        .map(|c| c.name.as_str())
        .unwrap_or("");

    let has_markers = !session.markers.is_empty();
    let n_timeline_slots = session.tracks.len() as u32;
    let n_slots = n_timeline_slots + if has_markers { 1 } else { 0 };

    let mut p = PropWriter::new();
    p.class_id(CLASS_COMPOSITION_MOB);
    p.mob_id_prop(PID_MOB_MOB_ID, *comp_mob_id);
    if !name.is_empty() {
        p.string_prop(PID_MOB_NAME, name);
    }
    p.data(PID_MOB_CREATION_TIME, &zero_timestamp());
    p.data(PID_MOB_LAST_MODIFIED, &zero_timestamp());
    p.strong_ref_vector(PID_MOB_SLOTS, "Slots", n_slots);

    builder.add_props(comp_dir, p);

    // Slots collection
    let slots_dir = comp_dir.join("Slots");
    builder.add_stream(slots_dir.join("index"), vector_index(n_slots));

    // One TimelineMobSlot per track
    for (i, track) in session.tracks.iter().enumerate() {
        let slot_dir = slots_dir.join(format!("{:08x}", i));
        build_timeline_slot(builder, &slot_dir, track, master_mob_map)?;
    }

    // EventMobSlot for markers (appended after timeline slots)
    if has_markers {
        let slot_dir = slots_dir.join(format!("{:08x}", n_timeline_slots));
        let marker_rate = session
            .markers
            .first()
            .map(|m| m.edit_rate)
            .unwrap_or(EditRate {
                numerator: session.session_sample_rate as i32,
                denominator: 1,
            });
        build_event_slot(builder, &slot_dir, &session.markers, marker_rate);
    }

    Ok(())
}

fn build_timeline_slot(
    builder: &mut CfbBuilder,
    slot_dir: &Path,
    track: &AafTrack,
    master_mob_map: &HashMap<[u8; 32], MobId>,
) -> AafResult<()> {
    let data_def = kind_to_data_def(&track.kind);

    // Collect only SourceClips (sorted by position), fill gaps with Filler.
    let mut source_clips: Vec<&AafClip> = track
        .clips
        .iter()
        .filter(|c| matches!(c.kind, ClipKind::SourceClip { .. }))
        .collect();
    source_clips.sort_by_key(|c| c.start_position);

    // Compute total Sequence length and component list
    let (components, total_length) = build_component_list(&source_clips, data_def, master_mob_map);
    let n_components = components.len() as u32;

    // TimelineMobSlot properties
    let mut p = PropWriter::new();
    p.class_id(CLASS_TIMELINE_MOB_SLOT);
    p.u32_prop(PID_MOB_SLOT_SLOT_ID, track.slot_id);
    if !track.name.is_empty() {
        p.string_prop(PID_MOB_SLOT_SLOT_NAME, &track.name);
    }
    if let Some(phys) = track.physical_track_number {
        p.u32_prop(PID_MOB_SLOT_PHYSICAL_TRACK_NUMBER, phys);
    }
    p.edit_rate_prop(PID_TIMELINE_MOB_SLOT_EDIT_RATE, track.edit_rate);
    p.i64_prop(PID_TIMELINE_MOB_SLOT_ORIGIN, 0);
    p.strong_ref(PID_MOB_SLOT_SEGMENT, "Sequence");

    builder.add_props(slot_dir, p);

    // Sequence
    let seq_dir = slot_dir.join("Sequence");
    let mut sp = PropWriter::new();
    sp.class_id(CLASS_SEQUENCE);
    sp.auid_prop(PID_COMPONENT_DATA_DEFINITION, data_def);
    sp.i64_prop(PID_COMPONENT_LENGTH, total_length);
    sp.strong_ref_vector(PID_SEQUENCE_COMPONENTS, "Components", n_components);

    builder.add_props(&seq_dir, sp);

    // Components collection
    let comp_dir = seq_dir.join("Components");
    builder.add_stream(comp_dir.join("index"), vector_index(n_components));

    for (j, comp) in components.iter().enumerate() {
        let child_dir = comp_dir.join(format!("{:08x}", j));
        write_component(builder, &child_dir, comp, data_def);
    }

    Ok(())
}

// ─── Component building helpers ───────────────────────────────────────────────

enum Component<'a> {
    SourceClip {
        length: i64,
        source_id: MobId,
        source_slot_id: u32,
        start_position: i64,
    },
    Filler {
        length: i64,
    },
    Unknown {
        clip: &'a AafClip,
    },
}

fn build_component_list<'a>(
    source_clips: &[&'a AafClip],
    _data_def: Auid,
    master_mob_map: &HashMap<[u8; 32], MobId>,
) -> (Vec<Component<'a>>, i64) {
    let mut out: Vec<Component<'a>> = Vec::new();
    let mut pos: i64 = 0;
    let mut total: i64 = 0;

    for clip in source_clips {
        // Fill gap before this clip
        if clip.start_position > pos {
            let gap = clip.start_position - pos;
            out.push(Component::Filler { length: gap });
            total += gap;
        }

        if let ClipKind::SourceClip {
            source_mob_id,
            source_slot_id,
            source_start,
            ..
        } = &clip.kind
        {
            let master_id = master_mob_map
                .get(source_mob_id)
                .copied()
                .unwrap_or(MobId::ZERO);

            out.push(Component::SourceClip {
                length: clip.length,
                source_id: master_id,
                source_slot_id: *source_slot_id,
                start_position: *source_start,
            });
            total += clip.length;
        }

        pos = clip.start_position + clip.length;
    }

    // Minimum: sequences must have at least one component
    if out.is_empty() {
        out.push(Component::Filler { length: 0 });
    }

    (out, total)
}

fn write_component(builder: &mut CfbBuilder, dir: &Path, comp: &Component<'_>, data_def: Auid) {
    match comp {
        Component::SourceClip {
            length,
            source_id,
            source_slot_id,
            start_position,
        } => {
            let mut p = PropWriter::new();
            p.class_id(CLASS_SOURCE_CLIP);
            p.auid_prop(PID_COMPONENT_DATA_DEFINITION, data_def);
            p.i64_prop(PID_COMPONENT_LENGTH, *length);
            p.mob_id_prop(PID_SOURCE_REF_SOURCE_ID, *source_id);
            p.u32_prop(PID_SOURCE_REF_MOB_SLOT_ID, *source_slot_id);
            p.i64_prop(PID_SOURCE_CLIP_START_POSITION, *start_position);
            builder.add_props(dir, p);
        }
        Component::Filler { length } => {
            let mut p = PropWriter::new();
            p.class_id(CLASS_FILLER);
            p.auid_prop(PID_COMPONENT_DATA_DEFINITION, data_def);
            p.i64_prop(PID_COMPONENT_LENGTH, *length);
            builder.add_props(dir, p);
        }
        Component::Unknown { clip } => {
            // Write as a Filler placeholder preserving the length
            let mut p = PropWriter::new();
            p.class_id(CLASS_FILLER);
            p.auid_prop(PID_COMPONENT_DATA_DEFINITION, data_def);
            p.i64_prop(PID_COMPONENT_LENGTH, clip.length);
            builder.add_props(dir, p);
        }
    }
}

// ─── EventMobSlot (markers) ───────────────────────────────────────────────────

fn build_event_slot(
    builder: &mut CfbBuilder,
    slot_dir: &Path,
    markers: &[AafMarker],
    edit_rate: EditRate,
) {
    // Use a slot ID beyond any timeline slot ID (99999 is arbitrary but unlikely to clash)
    let slot_id: u32 = 99999;
    let n_markers = markers.len() as u32;

    let mut p = PropWriter::new();
    p.class_id(CLASS_EVENT_MOB_SLOT);
    p.u32_prop(PID_MOB_SLOT_SLOT_ID, slot_id);
    p.edit_rate_prop(PID_EVENT_MOB_SLOT_EDIT_RATE, edit_rate);
    p.strong_ref(PID_MOB_SLOT_SEGMENT, "Sequence");

    builder.add_props(slot_dir, p);

    // Sequence containing CommentMarker events
    let seq_dir = slot_dir.join("Sequence");
    let mut sp = PropWriter::new();
    sp.class_id(CLASS_SEQUENCE);
    sp.auid_prop(PID_COMPONENT_DATA_DEFINITION, DATADEF_DESCRIPTIVE);
    sp.i64_prop(PID_COMPONENT_LENGTH, 0);
    sp.strong_ref_vector(PID_SEQUENCE_COMPONENTS, "Components", n_markers);

    builder.add_props(&seq_dir, sp);

    let comp_dir = seq_dir.join("Components");
    builder.add_stream(comp_dir.join("index"), vector_index(n_markers));

    for (j, marker) in markers.iter().enumerate() {
        let child_dir = comp_dir.join(format!("{:08x}", j));
        let mut mp = PropWriter::new();
        mp.class_id(CLASS_COMMENT_MARKER);
        mp.auid_prop(PID_COMPONENT_DATA_DEFINITION, DATADEF_DESCRIPTIVE);
        mp.i64_prop(PID_COMPONENT_LENGTH, 0);
        mp.i64_prop(PID_EVENT_POSITION, marker.position);
        mp.string_prop(PID_COMMENT_MARKER_ANNOTATION, &marker.comment);
        builder.add_props(&child_dir, mp);
    }
}

// ─── MasterMob ───────────────────────────────────────────────────────────────

fn build_master_mob(
    builder: &mut CfbBuilder,
    mob_dir: &Path,
    master_mob_id: &MobId,
    entry: &SourceEntry,
) {
    let mut p = PropWriter::new();
    p.class_id(CLASS_MASTER_MOB);
    p.mob_id_prop(PID_MOB_MOB_ID, *master_mob_id);
    p.data(PID_MOB_CREATION_TIME, &zero_timestamp());
    p.data(PID_MOB_LAST_MODIFIED, &zero_timestamp());
    // One slot pointing at the SourceMob
    p.strong_ref_vector(PID_MOB_SLOTS, "Slots", 1);

    builder.add_props(mob_dir, p);

    let slots_dir = mob_dir.join("Slots");
    builder.add_stream(slots_dir.join("index"), vector_index(1));

    // Slot 0 — TimelineMobSlot pointing to SourceMob
    let slot_dir = slots_dir.join("00000000");
    let mut sp = PropWriter::new();
    sp.class_id(CLASS_TIMELINE_MOB_SLOT);
    sp.u32_prop(PID_MOB_SLOT_SLOT_ID, 1);
    sp.edit_rate_prop(PID_TIMELINE_MOB_SLOT_EDIT_RATE, entry.edit_rate);
    sp.i64_prop(PID_TIMELINE_MOB_SLOT_ORIGIN, 0);
    sp.strong_ref(PID_MOB_SLOT_SEGMENT, "Sequence");

    builder.add_props(&slot_dir, sp);

    // Sequence with a single SourceClip → SourceMob
    let seq_dir = slot_dir.join("Sequence");
    let mut seqp = PropWriter::new();
    seqp.class_id(CLASS_SEQUENCE);
    seqp.auid_prop(PID_COMPONENT_DATA_DEFINITION, DATADEF_SOUND);
    seqp.i64_prop(PID_COMPONENT_LENGTH, entry.total_length);
    seqp.strong_ref_vector(PID_SEQUENCE_COMPONENTS, "Components", 1);

    builder.add_props(&seq_dir, seqp);

    let comp_dir = seq_dir.join("Components");
    builder.add_stream(comp_dir.join("index"), vector_index(1));

    let clip_dir = comp_dir.join("00000000");
    let mut cp = PropWriter::new();
    cp.class_id(CLASS_SOURCE_CLIP);
    cp.auid_prop(PID_COMPONENT_DATA_DEFINITION, DATADEF_SOUND);
    cp.i64_prop(PID_COMPONENT_LENGTH, entry.total_length);
    cp.mob_id_prop(PID_SOURCE_REF_SOURCE_ID, entry.mob_id);
    cp.u32_prop(PID_SOURCE_REF_MOB_SLOT_ID, entry.source_slot_id);
    cp.i64_prop(PID_SOURCE_CLIP_START_POSITION, 0);

    builder.add_props(&clip_dir, cp);
}

// ─── SourceMob ───────────────────────────────────────────────────────────────

fn build_source_mob(builder: &mut CfbBuilder, mob_dir: &Path, entry: &SourceEntry) {
    let mut p = PropWriter::new();
    p.class_id(CLASS_SOURCE_MOB);
    p.mob_id_prop(PID_MOB_MOB_ID, entry.mob_id);
    p.data(PID_MOB_CREATION_TIME, &zero_timestamp());
    p.data(PID_MOB_LAST_MODIFIED, &zero_timestamp());
    p.strong_ref(PID_SOURCE_MOB_ESSENCE_DESCRIPTION, "EssenceDesc");
    // SourceMob has no Slots entry in our minimal implementation

    builder.add_props(mob_dir, p);

    // EssenceDescriptor — PCMDescriptor
    let desc_dir = mob_dir.join("EssenceDesc");
    build_pcm_descriptor(builder, &desc_dir, entry);
}

fn build_pcm_descriptor(builder: &mut CfbBuilder, desc_dir: &Path, entry: &SourceEntry) {
    let has_locator = entry.source_file.is_some();
    let locator_count = if has_locator { 1u32 } else { 0u32 };

    let info = entry.audio_info.unwrap_or(crate::types::AudioEssenceInfo {
        sample_rate: entry.edit_rate.numerator as u32,
        channels: 1,
        quantization_bits: 24,
        length_samples: entry.total_length,
    });

    let sample_rate = EditRate {
        numerator: info.sample_rate as i32,
        denominator: 1,
    };

    let block_align = (info.quantization_bits / 8).max(1) * info.channels;
    let avg_bps = block_align * info.sample_rate;

    let mut p = PropWriter::new();
    p.class_id(CLASS_PCM_DESCRIPTOR);
    p.edit_rate_prop(PID_FILE_DESCRIPTOR_SAMPLE_RATE, sample_rate);
    p.i64_prop(PID_FILE_DESCRIPTOR_LENGTH, info.length_samples);
    p.edit_rate_prop(PID_SOUND_DESCRIPTOR_AUDIO_SAMPLING_RATE, sample_rate);
    p.u32_prop(PID_SOUND_DESCRIPTOR_CHANNELS, info.channels);
    p.u32_prop(
        PID_SOUND_DESCRIPTOR_QUANTIZATION_BITS,
        info.quantization_bits,
    );
    p.u32_prop(PID_PCM_DESCRIPTOR_BLOCK_ALIGN, block_align);
    p.u32_prop(PID_PCM_DESCRIPTOR_AVERAGE_BPS, avg_bps);
    if locator_count > 0 {
        p.strong_ref_vector(PID_ESSENCE_DESCRIPTOR_LOCATOR, "Locators", locator_count);
    }

    builder.add_props(desc_dir, p);

    // NetworkLocator (if we have a file URL)
    if let Some(url) = &entry.source_file {
        let loc_dir = desc_dir.join("Locators");
        builder.add_stream(loc_dir.join("index"), vector_index(1));

        let nloc_dir = loc_dir.join("00000000");
        let mut lp = PropWriter::new();
        lp.class_id(CLASS_NETWORK_LOCATOR);
        lp.string_prop(PID_NETWORK_LOCATOR_URL, url);
        builder.add_props(&nloc_dir, lp);
    }
}
