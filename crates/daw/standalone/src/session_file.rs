//! A live project saved as — and opened from — a `.session`.
//!
//! The `.session` format (`dawfile-standalone`) is a directory holding a
//! styx manifest and a content-addressed `objects/` store; its proven
//! round trip is REAPER project text. So this module is a bridge between
//! the engine's [`ProjectState`] and RPP text, in both directions:
//!
//! - **load**: `.session` → [`DawProject::load`] → `to_rpp()` → the one
//!   loader every project goes through,
//!   [`load_rpp_text`](crate::project_loader::load_rpp_text). Nothing here
//!   decides what a loaded project contains.
//! - **save**: the exact inverse of that loader. The engine state becomes
//!   a typed `dawfile_reaper` [`ReaperProject`], serialized with
//!   [`RppSerialize`], imported with [`DawProject::import_rpp`] and saved.
//!
//! ## What a save starts from
//!
//! A project the engine opened from a file keeps everything the engine
//! never modelled: FX chains and their state blobs, unmodelled lines,
//! take pitch, source-block details, extension state. The writer compares
//! the engine against the loader's own reading of the original (the
//! `.RPP` in [`ProjectInfo::path`](daw_proto::project::ProjectInfo), or a
//! `.session`'s export when the project was opened from one) and writes
//! back only what differs:
//!
//! - a **line** (`NAME`, `VOLPAN`, `ISBUS`, `AUXRECV`, …) is rewritten only
//!   when the engine's value is not what the loader read from it;
//! - an **item** the engine left alone goes out byte-for-byte;
//! - a **MIDI source** whose events are unchanged goes out verbatim;
//! - everything else in the original — every line and block of the
//!   project, a track or an envelope that the engine does not model — is
//!   kept, in place, through a merge over `dawfile_reaper`'s lossless
//!   token tree ([`rpp_tree`]).
//!
//! Tracks, items and takes are matched by GUID (`TRACKID`, `IGUID`,
//! `GUID`), the ids the loader adopts. Track order is the engine's; tracks
//! the engine removed are gone; tracks it added are written whole.
//!
//! ## Built-in FX
//!
//! An FX the injected [`FxFactory`] created (the guide's click/count/voice
//! instruments) has no plugin state a file could carry, and no bundle on
//! disk. It is written as a CLAP node whose display name **and** plugin id
//! are the factory name:
//!
//! ```text
//! BYPASS 0 0 0
//! <CLAP "fts.guide:click" "fts.guide:click" ""
//! >
//! ```
//!
//! `dawfile_reaper` parses that back to `PluginType::Clap` with
//! `name == file == "fts.guide:click"`, and the loader hands any node whose
//! id the factory [`provides`](FxFactory::provides) straight to
//! `Effects::add` before it searches for a bundle. REAPER shows it as a
//! missing CLAP plugin and keeps the chunk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use daw_proto::automation::{EnvelopeShape, EnvelopeType};
use daw_proto::item::{FadeShape, SourceType};
use daw_proto::primitives::AutomationMode as PAutomationMode;
use daw_proto::routing::SendMode;
use daw_proto::track::{InputMonitoringMode, LaneDisplay, RecordInput};
use daw_proto::{Take as PTake, Track as PTrack, TrackRoute};
use dawfile_reaper::rpp_tree::{self, RChunk, RNodeTree};
use dawfile_reaper::types::RppSerialize;
use dawfile_reaper::types::envelope::{
    Envelope as RppEnvelope, EnvelopePoint as RppEnvelopePoint, EnvelopePointShape,
};
use dawfile_reaper::types::fx_chain::{FxChain, FxChainNode, FxPlugin, PluginType};
use dawfile_reaper::types::item::{
    ChannelMode, FadeCurveType, FadeSettings, Item as RppItem, ItemYPos, MidiEvent,
    MidiExtendedEvent, MidiSource, MidiSourceEvent, MuteSettings, PitchMode, PlayRateSettings,
    SoloState, SourceBlock, SourceType as RppSourceType, StretchMarker as RppStretchMarker,
    Take as RppTake, VolPanSettings as TakeVolPan,
};
use dawfile_reaper::types::marker_region::{MarkerRegion, MarkerRegionCollection};
use dawfile_reaper::types::project::{
    DecodeOptions, ProjectProperties, ReaperProject, RulerLane as RppRulerLane,
};
use dawfile_reaper::types::time_tempo::{TempoTimeEnvelope, TempoTimePoint};
use dawfile_reaper::types::track::{
    AutomationMode as RAutomationMode, FixedLanesSettings, FolderSettings, FolderState,
    HardwareOutputSettings, LaneNameSettings, LaneRecordSettings, LaneSoloSettings,
    MasterSendSettings, MonitorMode, MuteSoloSettings, ReceiveSettings, RecordMode, RecordSettings,
    ShowInMixerSettings, Track as RppTrack, TrackHeightSettings, TrackSoloState,
    VolPanSettings as TrackVolPan, comp_area_from_proto, lane_settings,
};
use dawfile_standalone::project::{DAW_EXTENSION, DawProject, SESSION_EXTENSION};
/// For reading a `.session` directory that is not on a disk: which file
/// is its manifest, and where its objects are.
pub use dawfile_standalone::{OBJECTS_DIR, choose_manifest};

use crate::plugin::FxFactory;
use crate::project_loader::{self as loader, LoadedProject};
use crate::sync::{EnvelopeData, EnvelopeKey, FxChainKey, ProjectState, Standalone, TrackExt};

// ────────────────────────────────────────────────────────────────────
// Public API
// ────────────────────────────────────────────────────────────────────

/// Save `project_guid` as a `.session` project directory at `dir`
/// (e.g. `Song/Song.session`). Overwrites a previous save at `dir`.
///
/// The session is named after `dir`'s stem (`Song`), so its manifest is
/// `Song.session/Song.session`. Media paths are written exactly as the
/// project names them — relative ones (`Media/Bass.wav`) stay relative
/// and, like the `.RPP`'s, resolve against the folder the `.session`
/// directory **sits in** (`Song/`), not the directory itself. A new take
/// whose absolute path lies inside that folder is written relative to it.
///
/// Returns `dir`.
///
/// # Errors
///
/// No such project, or the directory could not be written.
pub fn save_session(daw: &Standalone, project_guid: &str, dir: &Path) -> Result<PathBuf, String> {
    save_session_with_history(daw, project_guid, dir, None)
}

/// [`save_session`], storing `history` as the session's CRDT history.
///
/// `None` is what [`save_session`] does: the log starts fresh from the
/// text. `Some` is the caller's live document — every edit anyone made,
/// which a later open hands back through [`load_session_history`] as long
/// as nobody has hand-edited the manifest since.
///
/// # Errors
///
/// As [`save_session`].
pub fn save_session_with_history(
    daw: &Standalone,
    project_guid: &str,
    dir: &Path,
    history: Option<&dawfile_standalone::loro::LoroDoc>,
) -> Result<PathBuf, String> {
    // Media inside the folder this save is opened from — the session
    // folder's own folder, as an open anchors it — is written relative to
    // it; anything else stays absolute. Relative to where the song was
    // opened from instead, a save anywhere else lost its audio.
    let at = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let text = project_rpp_text_relative_to(daw, project_guid, at.parent())?;
    let name = session_name(dir);
    let (imported, _report) = DawProject::import_rpp(&text, name.clone())
        .map_err(|e| format!("session import failed: {e}"))?;
    // Saving over a session keeps its identity: a document id is minted
    // on import, and a new one on every save would make a re-save of an
    // unchanged project differ from the last.
    let mut project = match DawProject::load(dir) {
        Ok(previous) => {
            let mut document = imported.document().clone();
            document.id = previous.document().id.clone();
            DawProject::new(document, imported.objects().clone())
        }
        Err(_) => imported,
    };
    if let Some(history) = history {
        project.set_history(history.clone());
    }
    retire_other_manifests(dir, &name)?;
    project
        .save(dir)
        .map_err(|e| format!("saving {}: {e}", dir.display()))?;
    // A previous save's blobs are unreachable now; the directory holds
    // this save and nothing else.
    project
        .compact_on_disk(dir, Some(1))
        .map_err(|e| format!("compacting {}: {e}", dir.display()))?;
    tracing::info!(
        session.dir = %dir.display(),
        session.rpp_bytes = text.len(),
        "saved session"
    );
    Ok(dir.to_path_buf())
}

/// The CRDT history a `.session` was saved with, if it is still the
/// history of what the manifest says (a hand edit since discards it).
#[must_use]
pub fn load_session_history(dir: &Path) -> Option<dawfile_standalone::loro::LoroDoc> {
    DawProject::load(dir).ok()?.history().cloned()
}

/// The RPP text a `.session` directory exports to — what `load_session`
/// feeds the loader. Exposed so a caller can open it through its own
/// open path (media resolver, materialize, …).
///
/// Relative media paths in it resolve against `dir`'s parent folder.
///
/// # Errors
///
/// `dir` is not a readable `.session` project.
pub fn session_rpp_text(dir: &Path) -> Result<String, String> {
    let project =
        DawProject::load(dir).map_err(|e| format!("opening session {}: {e}", dir.display()))?;
    project
        .to_rpp()
        .map_err(|e| format!("exporting session {}: {e}", dir.display()))
}

/// [`session_rpp_text`] for a `.session` that is not on a disk: its
/// manifest's text (`origin` names it) and its objects by file name — the
/// folder as it arrived from a share link or a peer.
///
/// # Errors
///
/// An object is not what its name says, or the manifest does not parse or
/// export.
pub fn session_rpp_text_from_parts(
    manifest: &str,
    origin: &str,
    objects: impl IntoIterator<Item = (String, Vec<u8>)>,
) -> Result<String, String> {
    let objects = dawfile_standalone::ObjectStore::from_named(objects)
        .map_err(|e| format!("opening session {origin}: {e}"))?;
    DawProject::from_parts(manifest, origin, objects)
        .map_err(|e| format!("opening session {origin}: {e}"))?
        .to_rpp()
        .map_err(|e| format!("exporting session {origin}: {e}"))
}

/// Load a `.session` directory into a fresh project in `daw`, exactly as
/// `project_loader::load_rpp_text` loads an `.rpp`.
///
/// The project's path is `dir` itself, so a later [`save_session`] builds
/// on this session, and relative media paths resolve against `dir`'s
/// parent folder — the folder the `.RPP` it was prepared from sits in.
///
/// # Errors
///
/// `dir` is not a readable `.session`, or its project text did not parse.
pub fn load_session(
    daw: &Standalone,
    project_name: &str,
    dir: &Path,
) -> Result<LoadedProject, String> {
    let text = session_rpp_text(dir)?;
    loader::load_rpp_text(daw, project_name, &dir.to_string_lossy(), &text)
}

/// The REAPER project text [`save_session`] stores for `project_guid`.
///
/// # Errors
///
/// No such project, or the generated text did not re-parse.
pub fn project_rpp_text(daw: &Standalone, project_guid: &str) -> Result<String, String> {
    let beside = daw
        .read_project(project_guid, |p| {
            Path::new(&p.info.path).parent().map(Path::to_path_buf)
        })
        .flatten();
    project_rpp_text_relative_to(daw, project_guid, beside.as_deref())
}

/// [`project_rpp_text`], its media inside `media_dir` written relative to
/// it (the folder the text will be opened from), the rest absolute.
///
/// # Errors
///
/// As [`project_rpp_text`].
pub fn project_rpp_text_relative_to(
    daw: &Standalone,
    project_guid: &str,
    media_dir: Option<&Path>,
) -> Result<String, String> {
    let factory = daw.fx_factory();
    let states = builtin_states(daw, project_guid);
    daw.read_project(project_guid, |p| {
        write_project(p, factory.as_deref(), &states, media_dir)
    })
    .ok_or_else(|| format!("no project {project_guid}"))?
}

// ────────────────────────────────────────────────────────────────────
// Directory housekeeping
// ────────────────────────────────────────────────────────────────────

/// `Song.session` → `Song`.
fn session_name(dir: &Path) -> String {
    dir.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

/// A project directory holds exactly one manifest. A previous save under
/// another name would make the directory ambiguous to open, so its
/// manifest goes (its blobs are collected by the compaction after the
/// save).
fn retire_other_manifests(dir: &Path, name: &str) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let is_manifest = ext == SESSION_EXTENSION || ext == DAW_EXTENSION;
        if is_manifest && !(stem == name && ext == SESSION_EXTENSION) {
            std::fs::remove_file(&path)
                .map_err(|e| format!("replacing {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────
// The original a save builds on
// ────────────────────────────────────────────────────────────────────

/// The file the project was opened from, read three ways: its text, the
/// typed model the loader reads, and the lossless token tree the merge
/// works on.
struct Base {
    typed: ReaperProject,
    tree: RChunk,
    widths: HashMap<String, u32>,
}

fn open_base(path: &str) -> Option<Base> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    let is_session = p.is_dir()
        || p.extension()
            .is_some_and(|e| e == SESSION_EXTENSION || e == DAW_EXTENSION);
    let text = if is_session {
        session_rpp_text(p)
    } else {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    };
    let text = match text {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(
                session.base = %path,
                session.base_error = %error,
                "session save: the project's original could not be read; writing the engine state alone"
            );
            return None;
        }
    };
    let parsed = dawfile_reaper::parse_rpp_file(&text)
        .map_err(|e| format!("{e:?}"))
        .and_then(|rpp| ReaperProject::from_rpp_project_with_options(&rpp, DecodeOptions::full()));
    let tree = rpp_tree::read_rpp_chunk(&text).map_err(|e| e.to_string());
    match (parsed, tree) {
        (Ok(typed), Ok(tree)) => Some(Base {
            widths: loader::mcp_widths(&text),
            typed,
            tree,
        }),
        (Err(error), _) | (_, Err(error)) => {
            tracing::warn!(
                session.base = %path,
                session.base_error = %error,
                "session save: the project's original did not parse; writing the engine state alone"
            );
            None
        }
    }
}

/// An original track, as the file has it and as the loader read it.
struct OrigTrack<'a> {
    index: usize,
    rt: &'a RppTrack,
    view: PTrack,
    ext: TrackExt,
}

/// An original item: where it sat and what its track's lane count was
/// (the loader reads the item's lane only on a lane track).
struct OrigItem<'a> {
    ri: &'a RppItem,
    track_guid: String,
    index: usize,
    lane_count: u32,
}

// ────────────────────────────────────────────────────────────────────
// The token-tree merge
// ────────────────────────────────────────────────────────────────────

/// Which children of a chunk the generated side owns. Everything else is
/// the original's, verbatim. A key with a `sub` set is merged one level
/// down instead of replaced (an envelope keeps its `EGUID`, `VOLTYPE`,
/// `<EXT>` while its points are rewritten).
#[derive(Default)]
struct Owned {
    keys: HashSet<String>,
    sub: HashMap<String, HashSet<String>>,
}

impl Owned {
    fn key(&mut self, key: &str) {
        self.keys.insert(key.to_string());
    }

    fn sub(&mut self, key: &str, children: &[&str]) {
        self.key(key);
        self.sub.insert(
            key.to_string(),
            children.iter().map(|c| (*c).to_string()).collect(),
        );
    }
}

fn child_key(child: &RNodeTree) -> String {
    child.name().unwrap_or_default()
}

/// The original's children with every owned key's lines replaced by the
/// generated ones. A replaced group sits where the original's first line
/// of that key was; a group the original lacked goes after the nearest
/// key that precedes it in the generated order.
fn merge_children(orig: &[RNodeTree], generated: &[RNodeTree], owned: &Owned) -> Vec<RNodeTree> {
    enum Slot {
        Orig(RNodeTree),
        Group(String),
    }
    let slot_key = |slot: &Slot| match slot {
        Slot::Orig(child) => child_key(child),
        Slot::Group(key) => key.clone(),
    };

    let mut slots: Vec<Slot> = Vec::with_capacity(orig.len());
    let mut placed: HashSet<String> = HashSet::new();
    for child in orig {
        let key = child_key(child);
        if owned.keys.contains(&key) {
            if placed.insert(key.clone()) {
                slots.push(Slot::Group(key));
            }
        } else {
            slots.push(Slot::Orig(child.clone()));
        }
    }

    // Groups the original never had, placed in generated order so a later
    // one can anchor on an earlier one.
    let gen_keys: Vec<String> = generated.iter().map(child_key).collect();
    for (at, key) in gen_keys.iter().enumerate() {
        if !owned.keys.contains(key) || placed.contains(key) {
            continue;
        }
        placed.insert(key.clone());
        let anchor = gen_keys[..at]
            .iter()
            .rev()
            .find_map(|prev| slots.iter().rposition(|s| slot_key(s) == *prev));
        let pos = anchor.map_or(0, |i| i + 1);
        slots.insert(pos, Slot::Group(key.clone()));
    }

    let mut out = Vec::with_capacity(slots.len());
    for slot in slots {
        match slot {
            Slot::Orig(child) => out.push(child),
            Slot::Group(key) => {
                let ours: Vec<&RNodeTree> =
                    generated.iter().filter(|c| child_key(c) == key).collect();
                let original: Vec<&RNodeTree> =
                    orig.iter().filter(|c| child_key(c) == key).collect();
                match (owned.sub.get(&key), original.as_slice(), ours.as_slice()) {
                    (Some(sub), [RNodeTree::Chunk(o)], [RNodeTree::Chunk(g)]) => {
                        let sub_owned = Owned {
                            keys: sub.clone(),
                            sub: HashMap::new(),
                        };
                        out.push(RNodeTree::Chunk(RChunk {
                            header: o.header.clone(),
                            children: merge_children(&o.children, &g.children, &sub_owned),
                        }));
                    }
                    _ => out.extend(ours.into_iter().cloned()),
                }
            }
        }
    }
    out
}

/// A track chunk's GUID: `TRACKID`, else the `<TRACK {GUID}` header.
fn tree_track_guid(chunk: &RChunk) -> Option<String> {
    chunk
        .children
        .iter()
        .find_map(|c| match c {
            RNodeTree::Node(n) => {
                let mut n = n.clone();
                (n.get_name().as_deref() == Some("TRACKID"))
                    .then(|| n.get_param(0))
                    .flatten()
            }
            RNodeTree::Chunk(_) => None,
        })
        .or_else(|| {
            let mut h = chunk.header.clone();
            h.get_param(0)
        })
}

/// Parse a text fragment (lines or blocks) into tree children.
fn parse_fragment(text: &str) -> Vec<RNodeTree> {
    rpp_tree::read_rpp_chunk(&format!("<FRAGMENT\n{text}>\n"))
        .map(|c| c.children)
        .unwrap_or_default()
}

// ────────────────────────────────────────────────────────────────────
// Conversions (the inverses of the loader's)
// ────────────────────────────────────────────────────────────────────

/// `0xRRGGBB` → REAPER's native colour with its custom-colour flag: the
/// inverse of the loader's `native_color_to_rgb`.
fn rgb_to_native(rgb: u32) -> i32 {
    let r = (rgb >> 16) & 0xff;
    let g = (rgb >> 8) & 0xff;
    let b = rgb & 0xff;
    (r | (g << 8) | (b << 16) | 0x0100_0000) as i32
}

/// An item label as a `<NOTES>` block, one `|` line a line.
fn notes_block(label: &str) -> String {
    let mut block = String::from("<NOTES\n");
    for line in label.lines() {
        block.push('|');
        block.push_str(line);
        block.push('\n');
    }
    block.push('>');
    block
}

/// A new GUID in REAPER's spelling.
fn new_guid() -> String {
    crate::new_braced_guid()
}

fn automation_mode_to_rpp(mode: PAutomationMode) -> RAutomationMode {
    match mode {
        PAutomationMode::Read => RAutomationMode::Read,
        PAutomationMode::Touch => RAutomationMode::Touch,
        PAutomationMode::Write => RAutomationMode::Write,
        PAutomationMode::Latch => RAutomationMode::Latch,
        _ => RAutomationMode::TrimRead,
    }
}

fn monitor_to_rpp(mode: InputMonitoringMode) -> MonitorMode {
    match mode {
        InputMonitoringMode::Off => MonitorMode::Off,
        InputMonitoringMode::Normal => MonitorMode::On,
        InputMonitoringMode::NotWhenPlaying => MonitorMode::Auto,
    }
}

/// The inverse of the loader's `record_input_from_rpp`. A mono and a
/// stereo input starting on the same channel read back alike, so the
/// channel count decides which is written.
fn record_input_to_rpp(input: RecordInput, num_channels: u32) -> i32 {
    match input {
        RecordInput::None => -1,
        RecordInput::Audio { channel } => {
            let channel = channel.min(1023) as i32;
            if num_channels >= 2 {
                1024 + channel
            } else {
                channel
            }
        }
        RecordInput::Midi { device_id, channel } => {
            let device = device_id.map_or(63, i32::from);
            let channel = channel.map_or(0, |c| i32::from(c) + 1);
            4096 + device * 32 + channel
        }
        RecordInput::Raw(raw) => raw,
    }
}

fn send_mode_to_rpp(mode: SendMode) -> i32 {
    match mode {
        SendMode::PreFx => 1,
        SendMode::PostFx => 3,
        SendMode::PostFader => 0,
    }
}

/// The inverse of the loader's `fade_curve_to_shape`. The steep variants
/// have no REAPER code the loader reads back, so they take their
/// non-steep neighbour.
fn fade_shape_to_curve(shape: FadeShape) -> FadeCurveType {
    match shape {
        FadeShape::Linear => FadeCurveType::Linear,
        FadeShape::FastStart | FadeShape::FastStartSteep => FadeCurveType::FastStart,
        FadeShape::FastEnd | FadeShape::FastEndSteep => FadeCurveType::FastEnd,
        FadeShape::SlowStartEnd | FadeShape::SlowStartEndSteep => FadeCurveType::SlowStartEnd,
    }
}

fn envelope_shape_to_rpp(shape: EnvelopeShape) -> EnvelopePointShape {
    match shape {
        EnvelopeShape::Square => EnvelopePointShape::Square,
        EnvelopeShape::SlowStartEnd => EnvelopePointShape::SlowStartEnd,
        EnvelopeShape::FastStart => EnvelopePointShape::FastStart,
        EnvelopeShape::FastEnd => EnvelopePointShape::FastEnd,
        EnvelopeShape::Bezier => EnvelopePointShape::Bezier,
        _ => EnvelopePointShape::Linear,
    }
}

/// `SOURCE` tag by file extension.
fn source_type_for(path: &str, kind: SourceType) -> RppSourceType {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mp3" => RppSourceType::Mp3,
        "flac" => RppSourceType::Flac,
        "ogg" | "oga" | "opus" => RppSourceType::Vorbis,
        "mid" | "midi" => RppSourceType::Midi,
        _ if matches!(kind, SourceType::Video | SourceType::Unknown) => RppSourceType::Video,
        _ => RppSourceType::Wave,
    }
}

/// The five track envelopes the loader reads: (engine type, chunk name
/// written for a new one, chunk names the loader accepts, value mapping).
const TRACK_ENVELOPES: [(EnvelopeType, &str, &[&str], EnvMap); 5] = [
    (
        EnvelopeType::Volume,
        "VOLENV2",
        &["VOLENV2"],
        EnvMap::Direct,
    ),
    (
        EnvelopeType::VolumePrefx,
        "VOLENV",
        &["VOLENV"],
        EnvMap::Direct,
    ),
    (EnvelopeType::Pan, "PANENV2", &["PANENV2"], EnvMap::Pan),
    (EnvelopeType::PanPrefx, "PANENV", &["PANENV"], EnvMap::Pan),
    (
        EnvelopeType::Mute,
        "MUTEENV",
        &["MUTEENV", "MUTEENV2"],
        EnvMap::Mute,
    ),
];

#[derive(Clone, Copy)]
enum EnvMap {
    Direct,
    Pan,
    Mute,
}

impl EnvMap {
    /// Engine value → file value: the inverse of `convert_track_envelope`.
    fn to_file(self, v: f64) -> f64 {
        match self {
            EnvMap::Direct => v,
            EnvMap::Pan => v * 2.0 - 1.0,
            EnvMap::Mute => 1.0 - v,
        }
    }
}

/// Every automation item pool in the engine, numbered from 1 for the file,
/// by each instance's envelope and pool. Pools are the project's (an
/// instance on another track's envelope is the same pool), so every
/// instance of one pool gets one number; in a fixed order, so a project
/// saved twice is the same.
fn file_pools(p: &ProjectState) -> HashMap<(String, EnvelopeKey, i32), i32> {
    let instances: Vec<(String, EnvelopeKey, i32)> = p
        .envelopes
        .iter()
        .flat_map(|((owner, key), data)| {
            data.automation_items
                .iter()
                .map(move |(ai, _)| (owner.clone(), key.clone(), ai.pool_id))
        })
        .collect();
    let mut pools: Vec<i32> = instances.iter().map(|(_, _, pool)| *pool).collect();
    pools.sort_unstable();
    pools.dedup();
    instances
        .into_iter()
        .map(|k| {
            let n = pools.binary_search(&k.2).unwrap_or(0);
            (k, i32::try_from(n + 1).unwrap_or(i32::MAX))
        })
        .collect()
}

/// An engine envelope value as the file's for an envelope of `key`: the
/// inverse of [`loader::engine_value`].
fn file_value(key: &EnvelopeKey, value: f64) -> f64 {
    match key {
        EnvelopeKey::Track(EnvelopeType::Pan | EnvelopeType::PanPrefx) => value.mul_add(2.0, -1.0),
        EnvelopeKey::Track(EnvelopeType::Mute) => 1.0 - value,
        _ => value,
    }
}

/// Whether two envelopes read the same to the renderer.
fn envelope_same(a: Option<&EnvelopeData>, b: Option<&EnvelopeData>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.visible == b.visible
                && a.armed == b.armed
                && (a.automation_mode == PAutomationMode::Off)
                    == (b.automation_mode == PAutomationMode::Off)
                && a.points.len() == b.points.len()
                && a.points.iter().zip(&b.points).all(|(x, y)| {
                    x.time.as_seconds() == y.time.as_seconds()
                        && x.value == y.value
                        && x.shape == y.shape
                        && x.tension == y.tension
                        && x.selected == y.selected
                })
                && a.automation_items.len() == b.automation_items.len()
                && a.automation_items
                    .iter()
                    .zip(&b.automation_items)
                    .all(|((x, _), (y, _))| x == y)
        }
        _ => false,
    }
}

// ────────────────────────────────────────────────────────────────────
// MIDI: the inverse of `decode_midi_source`
// ────────────────────────────────────────────────────────────────────

/// REAPER's MIDI resolution, and what every rewritten source uses.
const TICKS_PER_QN: u32 = 960;

/// One take's channel events, by kind.
struct MidiSet<'a> {
    notes: &'a [daw_proto::midi::MidiNote],
    ccs: &'a [daw_proto::midi::MidiCC],
    pitch_bends: &'a [daw_proto::midi::MidiPitchBend],
    program_changes: &'a [daw_proto::midi::MidiProgramChange],
    sysex: &'a [daw_proto::midi::MidiSysEx],
    channel_pressures: &'a [daw_proto::midi::MidiChannelPressure],
    poly_pressures: &'a [daw_proto::midi::MidiPolyPressure],
}

impl<'a> MidiSet<'a> {
    fn of_take(p: &'a ProjectState, take_guid: &str) -> Self {
        fn get<'a, T>(map: &'a HashMap<String, Vec<T>>, key: &str) -> &'a [T] {
            map.get(key).map(Vec::as_slice).unwrap_or(&[])
        }
        Self {
            notes: get(&p.midi_notes, take_guid),
            ccs: get(&p.midi_ccs, take_guid),
            pitch_bends: get(&p.midi_pitch_bends, take_guid),
            program_changes: get(&p.midi_program_changes, take_guid),
            sysex: get(&p.midi_sysex, take_guid),
            channel_pressures: get(&p.midi_channel_pressures, take_guid),
            poly_pressures: get(&p.midi_poly_pressures, take_guid),
        }
    }

    fn of_decoded(d: &'a loader::DecodedMidiSource) -> Self {
        Self {
            notes: &d.notes,
            ccs: &d.ccs,
            pitch_bends: &d.pitch_bends,
            program_changes: &d.program_changes,
            sysex: &d.sysex,
            channel_pressures: &d.channel_pressures,
            poly_pressures: &d.poly_pressures,
        }
    }

    /// Every event at its absolute tick, in the order they are written.
    ///
    /// Within a tick: a note ending here closes before anything else
    /// happens (so a same-pitch note starting on it is not closed by it),
    /// then controllers and friends, then note-ons, then the off of a
    /// zero-length note — after its own on.
    fn events(&self) -> Vec<(u64, Vec<u8>)> {
        let tick = |q: f64| (q * f64::from(TICKS_PER_QN)).round().max(0.0) as u64;
        let mut out: Vec<(u64, u8, usize, Vec<u8>)> = Vec::new();
        for (i, n) in self.notes.iter().enumerate() {
            let ch = n.channel & 0x0f;
            let pitch = n.pitch & 0x7f;
            let on = tick(n.start_ppq);
            let off = tick(n.start_ppq + n.length_ppq.max(0.0));
            out.push((on, 2, i, vec![0x90 | ch, pitch, (n.velocity & 0x7f).max(1)]));
            out.push((
                off,
                if off > on { 0 } else { 3 },
                i,
                vec![0x80 | ch, pitch, 0],
            ));
        }
        let base = self.notes.len();
        for (i, e) in self.ccs.iter().enumerate() {
            let bytes = vec![
                0xb0 | (e.channel & 0x0f),
                e.controller & 0x7f,
                e.value & 0x7f,
            ];
            out.push((tick(e.position_ppq), 1, base + i, bytes));
        }
        let base = base + self.ccs.len();
        for (i, e) in self.pitch_bends.iter().enumerate() {
            let u = (i32::from(e.value) + 8192).clamp(0, 16383) as u16;
            let bytes = vec![0xe0 | (e.channel & 0x0f), (u & 0x7f) as u8, (u >> 7) as u8];
            out.push((tick(e.position_ppq), 1, base + i, bytes));
        }
        let base = base + self.pitch_bends.len();
        for (i, e) in self.program_changes.iter().enumerate() {
            let bytes = vec![0xc0 | (e.channel & 0x0f), e.program & 0x7f];
            out.push((tick(e.position_ppq), 1, base + i, bytes));
        }
        let base = base + self.program_changes.len();
        for (i, e) in self.channel_pressures.iter().enumerate() {
            let bytes = vec![0xd0 | (e.channel & 0x0f), e.pressure & 0x7f];
            out.push((tick(e.position_ppq), 1, base + i, bytes));
        }
        let base = base + self.channel_pressures.len();
        for (i, e) in self.poly_pressures.iter().enumerate() {
            let bytes = vec![0xa0 | (e.channel & 0x0f), e.note & 0x7f, e.pressure & 0x7f];
            out.push((tick(e.position_ppq), 1, base + i, bytes));
        }
        let base = base + self.poly_pressures.len();
        for (i, e) in self.sysex.iter().enumerate() {
            if e.data.first() == Some(&0xf0) {
                out.push((tick(e.position_ppq), 1, base + i, e.data.clone()));
            }
        }
        out.sort_by_key(|(t, class, seq, _)| (*t, *class, *seq));
        out.into_iter().map(|(t, _, _, b)| (t, b)).collect()
    }
}

/// Rebuild a MIDI source's event stream from absolute-tick events at
/// [`TICKS_PER_QN`], re-interleaving the original's `<X>` blocks (text,
/// notation, …) at their own positions.
fn fill_midi_source(midi: &mut MidiSource, events: &[(u64, Vec<u8>)]) {
    let old_tpq = u64::from(midi.ticks_per_qn.max(1));
    let mut extended: Vec<(u64, MidiExtendedEvent)> = Vec::new();
    let mut at = 0u64;
    for ev in &midi.event_stream {
        at += u64::from(ev.delta_ticks());
        if let MidiSourceEvent::Extended(x) = ev {
            extended.push((at * u64::from(TICKS_PER_QN) / old_tpq, x.clone()));
        }
    }

    let mut merged: Vec<(u64, u8, MidiSourceEvent)> = Vec::new();
    for (t, x) in extended {
        merged.push((t, 0, MidiSourceEvent::Extended(x)));
    }
    for (t, bytes) in events {
        merged.push((
            *t,
            1,
            MidiSourceEvent::Midi(MidiEvent {
                delta_ticks: 0,
                bytes: bytes.clone(),
            }),
        ));
    }
    merged.sort_by_key(|(t, class, _)| (*t, *class));

    midi.has_data = true;
    midi.ticks_per_qn = TICKS_PER_QN;
    if midi.ticks_timebase.is_none() {
        midi.ticks_timebase = Some("QN".to_string());
    }
    midi.events.clear();
    midi.extended_events.clear();
    midi.event_stream.clear();
    let mut prev = 0u64;
    for (t, _, mut ev) in merged {
        let delta = u32::try_from(t - prev).unwrap_or(u32::MAX);
        prev = t;
        match &mut ev {
            MidiSourceEvent::Midi(e) => {
                e.delta_ticks = delta;
                midi.events.push(e.clone());
            }
            MidiSourceEvent::Extended(x) => {
                if let Some(first) = x.fields.first_mut() {
                    *first = delta.to_string();
                } else {
                    x.fields.push(delta.to_string());
                }
                midi.extended_events.push(x.clone());
            }
        }
        midi.event_stream.push(ev);
    }
}

fn new_midi_source() -> MidiSource {
    MidiSource {
        has_data: true,
        ticks_per_qn: TICKS_PER_QN,
        ticks_timebase: Some("QN".to_string()),
        cc_interp: None,
        pooled_evts_guid: None,
        events: Vec::new(),
        extended_events: Vec::new(),
        event_stream: Vec::new(),
        ignore_tempo: None,
        vel_lanes: Vec::new(),
        bank_program_file: None,
        cfg_edit_view: None,
        cfg_edit: None,
        evt_filter: None,
        guid: Some(new_guid()),
        unknown_lines: Vec::new(),
    }
}

// ────────────────────────────────────────────────────────────────────
// The writer
// ────────────────────────────────────────────────────────────────────

/// What the per-track merge needs beyond the generated chunk.
#[derive(Default)]
struct TrackPlan {
    owned: Owned,
    /// A rebuilt `<FXCHAIN>` for an original track whose built-in FX
    /// changed: the original chunk with its built-in nodes replaced.
    fx_override: Option<RChunk>,
}

struct Writer<'a> {
    p: &'a ProjectState,
    base: Option<&'a Base>,
    factory: Option<&'a dyn FxFactory>,
    /// The folder relative media paths resolve against.
    media_dir: Option<PathBuf>,
    orig_tracks: HashMap<String, OrigTrack<'a>>,
    /// Original track GUIDs by file position (`AUXRECV` names its source
    /// by position).
    orig_guid_at: Vec<String>,
    orig_items: HashMap<String, OrigItem<'a>>,
    orig_track_chunks: HashMap<String, &'a RChunk>,
    /// Engine track index by GUID, in the order being written.
    new_index: HashMap<&'a str, usize>,
    /// `ISBUS` depth delta per engine track.
    folder_deltas: Vec<i32>,
    skipped_fx: std::cell::RefCell<Vec<String>>,
    /// The built-in FX's states, by FX guid ([`builtin_states`]): what the
    /// plugins hold that a project file can carry.
    states: &'a HashMap<String, Vec<u8>>,
    /// Each automation item pool the engine holds — by envelope and its
    /// pool there — numbered for the file ([`file_pools`]).
    pools: HashMap<(String, EnvelopeKey, i32), i32>,
}

/// No states: a writer given none.
static NO_STATES: std::sync::LazyLock<HashMap<String, Vec<u8>>> =
    std::sync::LazyLock::new(HashMap::new);

/// Each built-in FX's state, by FX guid, from the plugins themselves — so
/// a saved session carries what they were set to (a sampler's kit, a
/// rig's capture), and loads them so again.
fn builtin_states(daw: &Standalone, project_guid: &str) -> HashMap<String, Vec<u8>> {
    let Some(factory) = daw.fx_factory() else {
        return HashMap::new();
    };
    let guids: Vec<String> = daw
        .read_project(project_guid, |p| {
            p.fx_chains
                .values()
                .flatten()
                .filter(|entry| factory.provides(&entry.fx.name))
                .map(|entry| entry.fx.guid.clone())
                .collect()
        })
        .unwrap_or_default();
    guids
        .into_iter()
        .filter_map(|guid| {
            let state = daw.save_plugin_state(&guid).ok()?;
            (!state.is_empty()).then_some((guid, state))
        })
        .collect()
}

fn write_project(
    p: &ProjectState,
    factory: Option<&dyn FxFactory>,
    states: &HashMap<String, Vec<u8>>,
    media_dir: Option<&Path>,
) -> Result<String, String> {
    let base = open_base(&p.info.path);
    let mut writer = Writer::new(p, base.as_ref(), factory);
    writer.states = states;
    writer.media_dir = media_dir.map(Path::to_path_buf);
    let text = writer.write()?;
    let skipped = writer.skipped_fx.into_inner();
    if !skipped.is_empty() {
        tracing::warn!(
            session.skipped_fx = skipped.len(),
            session.skipped_fx_names = %skipped.join(", "),
            "session save: FX with nothing a project file can restore were not written"
        );
    }
    Ok(text)
}

impl<'a> Writer<'a> {
    fn new(
        p: &'a ProjectState,
        base: Option<&'a Base>,
        factory: Option<&'a dyn FxFactory>,
    ) -> Self {
        let media_dir = Path::new(&p.info.path).parent().map(Path::to_path_buf);
        let mut orig_tracks = HashMap::new();
        let mut orig_guid_at = Vec::new();
        let mut orig_items = HashMap::new();
        let mut orig_track_chunks = HashMap::new();
        if let Some(b) = base {
            for (index, rt) in b.typed.tracks.iter().enumerate() {
                let (view, ext) = loader::track_from_rpp(rt, index, &b.widths);
                orig_guid_at.push(view.guid.clone());
                // A track with no TRACKID got a fresh GUID from the
                // loader, so the engine cannot name it: it is written
                // whole, as a new track.
                if rt.track_id.is_none() {
                    continue;
                }
                for (i, ri) in rt.items.iter().enumerate() {
                    if let Some(g) = &ri.item_guid {
                        orig_items.insert(
                            g.clone(),
                            OrigItem {
                                ri,
                                track_guid: view.guid.clone(),
                                index: i,
                                lane_count: view.lane_count,
                            },
                        );
                    }
                }
                orig_tracks.insert(
                    view.guid.clone(),
                    OrigTrack {
                        index,
                        rt,
                        view,
                        ext,
                    },
                );
            }
            for child in &b.tree.children {
                if let RNodeTree::Chunk(c) = child
                    && c.name().as_deref() == Some("TRACK")
                    && let Some(g) = tree_track_guid(c)
                {
                    orig_track_chunks.insert(g, c);
                }
            }
        }
        let new_index = p
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.guid.as_str(), i))
            .collect();
        Self {
            p,
            base,
            factory,
            media_dir,
            orig_tracks,
            orig_guid_at,
            orig_items,
            orig_track_chunks,
            new_index,
            folder_deltas: folder_deltas(&p.tracks),
            skipped_fx: std::cell::RefCell::new(Vec::new()),
            states: &NO_STATES,
            pools: file_pools(p),
        }
    }

    fn write(&self) -> Result<String, String> {
        let p = self.p;
        let mut project = match self.base {
            Some(b) => {
                let mut project = b.typed.clone();
                project.tracks.clear();
                project
            }
            None => ReaperProject {
                version: 0.1,
                version_string: "7.0/fts-session".to_string(),
                timestamp: 0,
                properties: ProjectProperties::new(),
                tracks: Vec::new(),
                items: Vec::new(),
                envelopes: Vec::new(),
                fx_chains: Vec::new(),
                markers_regions: MarkerRegionCollection::new(),
                tempo_envelope: None,
                ruler_lanes: Vec::new(),
                ruler_height: None,
            },
        };
        let mut owned = Owned::default();
        owned.key("TRACK");

        self.tempo(&mut project, &mut owned);
        self.markers(&mut project, &mut owned);
        self.ruler_lanes(&mut project, &mut owned);

        let mut plans: HashMap<String, TrackPlan> = HashMap::new();
        for (i, e) in p.tracks.iter().enumerate() {
            let (t, plan) = self.track(i, e);
            project.tracks.push(t);
            plans.insert(e.guid.clone(), plan);
        }

        let mut extra = self.master_and_extstate(&mut owned);
        let pooled = self.pooled();
        if !pooled.is_empty() {
            owned.key("POOLEDENV");
            extra.push_str(&pooled);
        }
        let mut text = project.to_rpp_string();
        if !extra.is_empty() {
            // Before the project's closing `>`.
            let close = text.trim_end().rfind('>').unwrap_or(text.len());
            text.insert_str(close, &extra);
        }

        let Some(base) = self.base else {
            return Ok(text);
        };
        let generated = rpp_tree::read_rpp_chunk(&text)
            .map_err(|e| format!("generated project text did not re-parse: {e}"))?;
        let mut children = merge_children(&base.tree.children, &generated.children, &owned);
        for child in &mut children {
            let RNodeTree::Chunk(c) = child else {
                continue;
            };
            if c.name().as_deref() != Some("TRACK") {
                continue;
            }
            let Some(guid) = tree_track_guid(c) else {
                continue;
            };
            let (Some(orig), Some(plan)) =
                (self.orig_track_chunks.get(&guid), plans.get_mut(&guid))
            else {
                continue;
            };
            if !self.orig_tracks.contains_key(&guid) {
                continue;
            }
            if let Some(fx) = plan.fx_override.take() {
                c.children.retain(
                    |g| !matches!(g, RNodeTree::Chunk(x) if x.name().as_deref() == Some("FXCHAIN")),
                );
                let at = c
                    .children
                    .iter()
                    .position(|g| matches!(g, RNodeTree::Chunk(_)))
                    .unwrap_or(c.children.len());
                c.children.insert(at, RNodeTree::Chunk(fx));
                plan.owned.key("FXCHAIN");
            }
            *c = RChunk {
                header: orig.header.clone(),
                children: merge_children(&orig.children, &c.children, &plan.owned),
            };
        }
        let root = RChunk {
            header: base.tree.header.clone(),
            children,
        };
        let mut out = rpp_tree::stringify_rpp_node(&RNodeTree::Chunk(root));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }

    // ── automation items ───────────────────────────────────────────

    /// An envelope's automation items as the file's `POOLEDENVINST`s, each
    /// naming its pool's number in this save.
    fn instances(
        &self,
        owner: &str,
        key: &EnvelopeKey,
        data: &EnvelopeData,
    ) -> Vec<dawfile_reaper::types::envelope::AutomationItem> {
        data.automation_items
            .iter()
            .enumerate()
            .map(
                |(i, (ai, _))| dawfile_reaper::types::envelope::AutomationItem {
                    pool_index: self
                        .pools
                        .get(&(owner.to_owned(), key.clone(), ai.pool_id))
                        .copied()
                        .unwrap_or(0),
                    position: ai.position.as_seconds(),
                    length: ai.length.as_seconds(),
                    start_offset: ai.start_offset.as_seconds(),
                    play_rate: ai.play_rate,
                    selected: ai.selected,
                    baseline: ai.baseline,
                    amplitude: ai.amplitude,
                    loop_enabled: ai.loop_source,
                    position_qn: 0.0,
                    length_qn: 0.0,
                    instance_index: i32::try_from(i + 1).unwrap_or(i32::MAX),
                    muted: false,
                    start_offset_qn: 0.0,
                    transition_time: 0.0,
                    volume_envelope_max: 0,
                },
            )
            .collect()
    }

    /// Every pool's curve as a project-level `<POOLEDENV>`, in file
    /// values; empty when the engine holds no automation items.
    fn pooled(&self) -> String {
        // One block a pool, from the first of its instances in a fixed
        // order (its curve is every instance's).
        let mut numbered: Vec<(&(String, EnvelopeKey, i32), &i32)> = self.pools.iter().collect();
        numbered.sort_by_key(|((owner, key, _), id)| (**id, owner.clone(), format!("{key:?}")));
        numbered.dedup_by_key(|(_, id)| **id);
        let mut out = String::new();
        for ((owner, key, pool), id) in numbered {
            let Some(data) = self.p.envelopes.get(&(owner.clone(), key.clone())) else {
                continue;
            };
            let Some((item, curve)) = data
                .automation_items
                .iter()
                .find(|(ai, _)| ai.pool_id == *pool)
            else {
                continue;
            };
            let block = dawfile_reaper::types::envelope::PooledEnvelope {
                id: *id,
                name: if item.name.is_empty() {
                    format!("Automation item {id}")
                } else {
                    item.name.clone()
                },
                source_length: curve
                    .last()
                    .map_or(item.length.as_seconds(), |p| p.time.as_seconds())
                    .max(item.length.as_seconds()),
                points: curve
                    .iter()
                    .map(|pt| {
                        let tension = (pt.tension != 0.0).then_some(pt.tension);
                        RppEnvelopePoint {
                            position: pt.time.as_seconds(),
                            value: file_value(key, pt.value),
                            shape: envelope_shape_to_rpp(pt.shape),
                            time_sig: Some(0),
                            selected: Some(false),
                            unknown_field_6: tension.map(|_| 0),
                            bezier_tension: tension,
                        }
                    })
                    .collect(),
                other: Vec::new(),
            };
            out.push_str(&block.to_string());
        }
        out
    }

    // ── project level ──────────────────────────────────────────────

    fn tempo(&self, project: &mut ReaperProject, owned: &mut Owned) {
        let p = self.p;
        let bpm = p.transport.tempo.bpm;
        let ts = p.transport.time_signature;
        if let Some(b) = self.base {
            let view = loader::transport_tempo_from_rpp(&b.typed);
            let view_points: Vec<daw_proto::TempoPoint> = b
                .typed
                .tempo_envelope
                .as_ref()
                .map(|env| {
                    env.points
                        .iter()
                        .map(loader::tempo_point_from_rpp)
                        .collect()
                })
                .unwrap_or_default();
            // With no `TEMPO` line the loader leaves the transport at its
            // default.
            let (vt, vs) = view.unwrap_or_else(|| {
                let d = daw_proto::Transport::new();
                (d.tempo, d.time_signature)
            });
            let transport_same =
                vt.bpm == bpm && vs.numerator == ts.numerator && vs.denominator == ts.denominator;
            let points_same = view_points.len() == p.tempo_points.len()
                && view_points
                    .iter()
                    .zip(&p.tempo_points)
                    .all(|(a, b)| tempo_point_same(a, b));
            if transport_same && points_same {
                return;
            }
        }
        owned.key("TEMPO");
        owned.key("TEMPOENVEX");
        let flags = project.properties.tempo.map_or(0, |t| t.3);
        project.properties.tempo = Some((bpm, ts.numerator as i32, ts.denominator as i32, flags));
        if p.tempo_points.is_empty() {
            project.tempo_envelope = None;
            return;
        }
        let old: Vec<TempoTimePoint> = project
            .tempo_envelope
            .as_ref()
            .map(|e| e.points.clone())
            .unwrap_or_default();
        let points = p
            .tempo_points
            .iter()
            .map(|tp| {
                let pos = position_seconds(&tp.position);
                // A point that was already there keeps what the engine
                // does not model (its shape, metronome pattern, …).
                let mut pt = old
                    .iter()
                    .find(|o| (o.position - pos).abs() < 1e-9)
                    .cloned()
                    .unwrap_or_else(|| TempoTimePoint {
                        shape: tp.shape.unwrap_or(1),
                        bezier_tension: tp.bezier_tension.unwrap_or(0.0),
                        selected: tp.selected.unwrap_or(false),
                        ..TempoTimePoint::default()
                    });
                pt.position = pos;
                pt.tempo = tp.bpm;
                pt.time_signature_encoded = tp.time_signature.map(|s| {
                    (s.numerator as i32 & 0xffff) | ((s.denominator as i32 & 0xffff) << 16)
                });
                pt
            })
            .collect();
        project.tempo_envelope = Some(TempoTimeEnvelope {
            points,
            default_tempo: bpm,
            default_time_signature: (ts.numerator as i32, ts.denominator as i32),
        });
    }

    fn markers(&self, project: &mut ReaperProject, owned: &mut Owned) {
        let p = self.p;
        let mut markers: Vec<&daw_proto::Marker> = p.markers.values().collect();
        markers.sort_by(|a, b| {
            position_seconds(&a.position)
                .total_cmp(&position_seconds(&b.position))
                .then(a.id.cmp(&b.id))
        });
        let mut regions: Vec<&daw_proto::Region> = p.regions.values().collect();
        regions.sort_by(|a, b| {
            a.time_range
                .start_seconds()
                .total_cmp(&b.time_range.start_seconds())
                .then(a.id.cmp(&b.id))
        });

        let old = self.base.map(|b| &b.typed.markers_regions);
        if let Some(old) = old {
            let mut view_m: Vec<daw_proto::Marker> = old
                .markers
                .iter()
                .map(|m| loader::marker_from_rpp(m, 0))
                .collect();
            let mut view_r: Vec<daw_proto::Region> = old
                .regions
                .iter()
                .map(|r| loader::region_from_rpp(r, 0))
                .collect();
            let key_m = |m: &daw_proto::Marker| {
                (
                    position_seconds(&m.position).to_bits(),
                    m.name.clone(),
                    m.color,
                    m.guid.clone(),
                    m.lane,
                )
            };
            let key_r = |r: &daw_proto::Region| {
                (
                    r.time_range.start_seconds().to_bits(),
                    r.time_range.end_seconds().to_bits(),
                    r.name.clone(),
                    r.color,
                    r.guid.clone(),
                    r.lane,
                )
            };
            view_m.sort_by_key(key_m);
            view_r.sort_by_key(key_r);
            let mut eng_m: Vec<_> = markers.iter().map(|m| key_m(m)).collect();
            let mut eng_r: Vec<_> = regions.iter().map(|r| key_r(r)).collect();
            eng_m.sort();
            eng_r.sort();
            if eng_m == view_m.iter().map(key_m).collect::<Vec<_>>()
                && eng_r == view_r.iter().map(key_r).collect::<Vec<_>>()
            {
                return;
            }
        }
        owned.key("MARKER");

        let by_guid = |guid: &Option<String>, region: bool| -> Option<MarkerRegion> {
            let guid = guid.as_ref()?;
            old?.all
                .iter()
                .find(|m| &m.guid == guid && m.is_region() == region)
                .cloned()
        };
        let color = |base: Option<&MarkerRegion>, rgb: Option<u32>| -> i32 {
            let view = base
                .and_then(|b| (b.color != 0).then(|| loader::native_color_to_rgb(b.color as u32)));
            match base {
                Some(b) if view == rgb => b.color,
                _ => rgb.map_or(0, rgb_to_native),
            }
        };
        // Marker and region numbers are disjoint: the region parser pairs
        // an unnamed line with the region of the same number, so a marker
        // sharing a region's number could be read as its end.
        let mut all = MarkerRegionCollection::new();
        let mut id = 0;
        for m in &markers {
            id += 1;
            let base = by_guid(&m.guid, false);
            let mut mr = base.clone().unwrap_or_else(|| blank_marker(0));
            mr.id = id;
            mr.position = position_seconds(&m.position);
            mr.name = m.name.clone();
            mr.color = color(base.as_ref(), m.color);
            mr.flags &= !1;
            mr.guid = m.guid.clone().unwrap_or_else(new_guid);
            mr.lane = m.lane.map(|l| l as i32 + 1);
            mr.end_position = None;
            all.all.push(mr.clone());
            all.markers.push(mr);
        }
        for r in &regions {
            id += 1;
            let base = by_guid(&r.guid, true);
            let mut mr = base.clone().unwrap_or_else(|| blank_marker(1));
            mr.id = id;
            mr.position = r.time_range.start_seconds();
            mr.end_position = Some(r.time_range.end_seconds());
            mr.name = r.name.clone();
            mr.color = color(base.as_ref(), r.color);
            mr.flags |= 1;
            mr.guid = r.guid.clone().unwrap_or_else(new_guid);
            mr.lane = r.lane.map(|l| l as i32 + 1);
            all.all.push(mr.clone());
            all.regions.push(mr);
        }
        project.markers_regions = all;
    }

    fn ruler_lanes(&self, project: &mut ReaperProject, owned: &mut Owned) {
        let p = self.p;
        if let Some(b) = self.base {
            let view: Vec<(u32, String, u32)> = b
                .typed
                .ruler_lanes
                .iter()
                .filter_map(|l| {
                    loader::file_lane_to_api(Some(l.index))
                        .map(|i| (i, l.name.clone(), l.flags.max(0) as u32))
                })
                .map(|(i, n, f)| (i, (n, f)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .map(|(i, (n, f))| (i, n, f))
                .collect();
            let eng: Vec<(u32, String, u32)> = p
                .ruler_lanes
                .iter()
                .map(|(i, l)| (*i, l.name.clone(), l.flags))
                .collect();
            if view == eng {
                return;
            }
        }
        owned.key("RULERLANE");
        let old = std::mem::take(&mut project.ruler_lanes);
        project.ruler_lanes = p
            .ruler_lanes
            .iter()
            .map(|(i, l)| {
                let index = *i as i32 + 1;
                let prev = old.iter().find(|o| o.index == index);
                RppRulerLane {
                    index,
                    flags: l.flags as i32,
                    name: l.name.clone(),
                    color: prev.map_or(0, |o| o.color),
                    extra: prev.map_or(-1, |o| o.extra),
                }
            })
            .collect();
    }

    /// `MASTER_VOLUME`, `MASTERMUTESOLO` and the `<EXTSTATE>` mixer-width
    /// block, as text for the end of the project chunk — the typed model
    /// does not write them.
    fn master_and_extstate(&self, owned: &mut Owned) -> String {
        let p = self.p;
        let mut out = String::new();
        let props = self.base.map(|b| &b.typed.properties);

        let (view_vol, view_pan) = props
            .and_then(|pr| pr.master_volume)
            .map_or((1.0, 0.0), |(v, pan, ..)| {
                (v.max(0.0), pan.clamp(-1.0, 1.0))
            });
        if self.base.is_none() || view_vol != p.master_volume || view_pan != p.master_pan {
            owned.key("MASTER_VOLUME");
            let (_, _, a, b, c) = props
                .and_then(|pr| pr.master_volume)
                .unwrap_or((1.0, 0.0, -1.0, -1.0, 1.0));
            out.push_str(&format!(
                "  MASTER_VOLUME {} {} {} {} {}\n",
                p.master_volume, p.master_pan, a, b, c
            ));
        }
        let view_ms = props.and_then(|pr| pr.master_mute_solo).unwrap_or(0);
        if self.base.is_none() || (view_ms & 1 != 0) != p.master_muted {
            owned.key("MASTERMUTESOLO");
            let ms = (view_ms & !1) | i32::from(p.master_muted);
            out.push_str(&format!("  MASTERMUTESOLO {ms}\n"));
        }

        // Mixer widths — ours, not REAPER's; see the loader's `mcp_widths`.
        let widths: Vec<(&str, u32)> = p
            .tracks
            .iter()
            .filter_map(|t| t.width.map(|w| (t.guid.as_str(), w)))
            .collect();
        let view_same = self.base.is_some_and(|b| {
            p.tracks
                .iter()
                .all(|t| t.width == b.widths.get(&t.guid).copied())
        });
        if view_same || (self.base.is_none() && widths.is_empty()) {
            return out;
        }
        owned.key("EXTSTATE");
        let ours = if widths.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = widths.iter().map(|(g, w)| format!("{g}={w}")).collect();
            format!("<FTSMCP\nWIDTHS {}\n>\n", pairs.join(" "))
        };
        let orig_ext = self.base.and_then(|b| {
            b.tree.children.iter().find_map(|c| match c {
                RNodeTree::Chunk(x) if x.name().as_deref() == Some("EXTSTATE") => Some(x),
                _ => None,
            })
        });
        let ext = match orig_ext {
            Some(orig) => {
                let mut ours_owned = Owned::default();
                ours_owned.key("FTSMCP");
                RChunk {
                    header: orig.header.clone(),
                    children: merge_children(&orig.children, &parse_fragment(&ours), &ours_owned),
                }
            }
            None => RChunk {
                header: rpp_tree::create_rnode_from_line("EXTSTATE"),
                children: parse_fragment(&ours),
            },
        };
        out.push_str(&rpp_tree::stringify_rpp_node(&RNodeTree::Chunk(ext)));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    // ── tracks ─────────────────────────────────────────────────────

    fn track(&self, new_idx: usize, e: &PTrack) -> (RppTrack, TrackPlan) {
        let p = self.p;
        let ext = p.track_ext.get(&e.guid).cloned().unwrap_or_default();
        let orig = self.orig_tracks.get(&e.guid);
        let view = orig.map(|o| &o.view);
        let vext = orig.map(|o| &o.ext);
        let mut plan = TrackPlan::default();
        let owned = &mut plan.owned;
        // `true` when the engine's value is not what the loader read — or
        // there is nothing to compare against.
        let differs = |same: Option<bool>| same != Some(true);

        let mut t = orig.map(|o| o.rt.clone()).unwrap_or_else(new_track);
        t.raw_content.clear();
        t.track_id = Some(e.guid.clone());

        if differs(view.map(|v| v.name == e.name)) {
            t.name = e.name.clone();
            owned.key("NAME");
        }
        // Its ext-state, as the engine holds it now.
        let mut ext_state: Vec<(String, String)> = p
            .track_ext_state
            .iter()
            .filter(|((track, section, _), _)| *track == e.guid && section.is_empty())
            .map(|((_, _, key), value)| (key.clone(), value.clone()))
            .collect();
        ext_state.sort();
        if t.extension_data != ext_state {
            t.extension_data = ext_state;
            owned.key("EXT");
        }
        if differs(view.map(|v| v.color == e.color)) {
            t.peak_color = e.color.map(rgb_to_native);
            owned.key("PEAKCOL");
        }
        if differs(view.map(|v| v.selected == e.selected)) {
            t.selected = e.selected;
            owned.key("SEL");
        }
        if differs(view.map(|v| v.automation_mode == e.automation_mode)) {
            t.automation_mode = automation_mode_to_rpp(e.automation_mode);
            owned.key("AUTOMODE");
        }
        if differs(view.map(|v| v.volume == e.volume && v.pan == e.pan)) {
            let pan_law = t.volpan.as_ref().map_or(-1.0, |v| v.pan_law);
            t.volpan = Some(TrackVolPan {
                volume: e.volume,
                pan: e.pan,
                pan_law,
            });
            owned.key("VOLPAN");
        }
        if differs(view.map(|v| v.muted == e.muted && v.soloed == e.soloed)) {
            let old = t.mutesolo.as_ref();
            let solo = match old.map(|m| m.solo) {
                Some(s) if e.soloed && s != TrackSoloState::NoSolo => s,
                _ if e.soloed => TrackSoloState::SoloInPlace,
                _ => TrackSoloState::NoSolo,
            };
            t.mutesolo = Some(MuteSoloSettings {
                mute: e.muted,
                solo,
                solo_defeat: old.is_some_and(|m| m.solo_defeat),
            });
            owned.key("MUTESOLO");
        }
        if differs(view.map(|v| v.phase_inverted == e.phase_inverted)) {
            t.invert_phase = e.phase_inverted;
            owned.key("IPHASE");
        }
        let delta = self.folder_deltas[new_idx];
        if differs(view.map(|v| v.folder_depth == delta)) {
            t.folder = Some(folder_settings(delta));
            owned.key("ISBUS");
        }
        if differs(view.map(|v| {
            v.visible_in_mixer == e.visible_in_mixer && v.visible_in_tcp == e.visible_in_tcp
        })) {
            let mut sim = t.show_in_mixer.clone().unwrap_or(ShowInMixerSettings {
                show_in_mixer: true,
                unknown_field_2: 0.6667,
                unknown_field_3: 0.5,
                show_in_track_list: true,
                unknown_field_5: 0.5,
                unknown_field_6: 0,
                unknown_field_7: 0,
                unknown_field_8: 0,
            });
            sim.show_in_mixer = e.visible_in_mixer;
            sim.show_in_track_list = e.visible_in_tcp;
            t.show_in_mixer = Some(sim);
            owned.key("SHOWINMIX");
        }
        if differs(view.map(|v| v.height == e.height)) {
            t.track_height = e.height.map(|h| TrackHeightSettings {
                height: h as i32,
                folder_override: t.track_height.as_ref().is_some_and(|x| x.folder_override),
            });
            owned.key("TRACKHEIGHT");
        }
        let record_input = ext.record_input;
        if differs(view.map(|v| {
            v.armed == e.armed
                && v.input_monitor == e.input_monitor
                && v.record_input == record_input
        })) {
            let old = t.record.clone();
            let input = match (&old, vext) {
                // The same input, however the file spelled it.
                (Some(r), Some(x)) if x.record_input == record_input => r.input,
                _ => record_input_to_rpp(record_input, ext.num_channels),
            };
            t.record = Some(RecordSettings {
                armed: e.armed,
                input,
                monitor: monitor_to_rpp(e.input_monitor),
                record_mode: old.as_ref().map_or(RecordMode::Input, |r| r.record_mode),
                monitor_track_media: old.as_ref().is_some_and(|r| r.monitor_track_media),
                preserve_pdc_delayed: old.as_ref().is_some_and(|r| r.preserve_pdc_delayed),
                record_path: old.as_ref().map_or(0, |r| r.record_path),
            });
            owned.key("REC");
        }
        let parent_send = ext.parent_send_enabled;
        if differs(vext.map(|x| x.parent_send_enabled == parent_send)) {
            t.master_send = Some(MasterSendSettings {
                enabled: parent_send,
                unknown_field_2: t.master_send.as_ref().map_or(0, |m| m.unknown_field_2),
            });
            owned.key("MAINSEND");
        }
        if differs(vext.map(|x| x.num_channels == ext.num_channels)) {
            t.channel_count = ext.num_channels.clamp(1, 128);
            owned.key("NCHAN");
        }
        if differs(view.map(|v| v.grouping == e.grouping)) {
            let (low, high) = e.grouping.to_rpp_fields();
            t.group_flags = Some(low);
            t.group_flags_high = Some(high);
            owned.key("GROUP_FLAGS");
            owned.key("GROUP_FLAGS_HIGH");
        }
        if differs(view.zip(vext).map(|(v, x)| {
            v.lane_count == e.lane_count
                && v.lane_play_mask == e.lane_play_mask
                && v.lane_names == e.lane_names
                && v.lane_display == e.lane_display
                && x.comping == ext.comping
        })) {
            write_lanes(&mut t, e, &ext);
            for key in [
                "FIXEDLANES",
                "LANESOLO",
                "LANENAME",
                "LANEREC",
                "ITEMLANES",
                "LINKEDLANE",
            ] {
                owned.key(key);
            }
        }

        self.receives(&mut t, e, orig, owned);
        self.hardware_outputs(&mut t, e, orig, owned);
        self.envelopes(&mut t, e, orig, owned);
        plan.fx_override = self.fx(&mut t, e, orig.is_some(), owned);

        // Items.
        let item_guids = p.items_by_track.get(&e.guid).cloned().unwrap_or_default();
        let mut unchanged = orig.is_some_and(|o| o.rt.items.len() == item_guids.len());
        let mut items = Vec::with_capacity(item_guids.len());
        for (k, ig) in item_guids.iter().enumerate() {
            let Some(entry) = p.items.get(ig) else {
                unchanged = false;
                continue;
            };
            let (ri, same) = self.item(&entry.item, e.lane_count);
            unchanged &= same
                && self
                    .orig_items
                    .get(ig)
                    .is_some_and(|o| o.track_guid == e.guid && o.index == k);
            items.push(ri);
        }
        if !unchanged {
            owned.key("ITEM");
        }
        t.items = items;
        (t, plan)
    }

    /// `AUXRECV` on this (destination) track: every engine send into it,
    /// by source track position.
    fn receives(&self, t: &mut RppTrack, e: &PTrack, orig: Option<&OrigTrack>, owned: &mut Owned) {
        let p = self.p;
        let mut into: Vec<(usize, u32, &TrackRoute)> = Vec::new();
        for (src, sends) in &p.sends {
            let Some(&si) = self.new_index.get(src.as_str()) else {
                continue;
            };
            for r in sends {
                if r.dest_track_guid.as_deref() == Some(e.guid.as_str()) {
                    into.push((si, r.index, r));
                }
            }
        }
        into.sort_by_key(|(si, idx, _)| (*si, *idx));

        let fields = |src: &str, r: &TrackRoute| {
            (
                src.to_string(),
                r.send_mode,
                r.volume.to_bits(),
                r.pan.to_bits(),
                r.muted,
                r.phase_inverted,
            )
        };
        if let Some(o) = orig {
            let view: Vec<_> =
                o.rt.receives
                    .iter()
                    .map(|recv| {
                        let src = usize::try_from(recv.source_track_index)
                            .ok()
                            .and_then(|i| self.orig_guid_at.get(i))
                            .cloned()
                            .unwrap_or_default();
                        // The same send, still numbering its source right.
                        let still = self.new_index.get(src.as_str()).copied()
                            == usize::try_from(recv.source_track_index).ok();
                        (
                            (
                                src,
                                loader::send_mode_from_rpp(recv.mode),
                                recv.volume.to_bits(),
                                recv.pan.to_bits(),
                                recv.mute,
                                recv.invert_polarity,
                            ),
                            still,
                        )
                    })
                    .collect();
            let eng: Vec<_> = into
                .iter()
                .map(|(si, _, r)| fields(&p.tracks[*si].guid, r))
                .collect();
            if view.iter().all(|(_, still)| *still)
                && view.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>() == eng
            {
                return;
            }
        }
        owned.key("AUXRECV");
        let old = std::mem::take(&mut t.receives);
        let mut used = vec![false; old.len()];
        t.receives = into
            .iter()
            .map(|(si, _, r)| {
                let src_guid = &p.tracks[*si].guid;
                let prev = old.iter().enumerate().position(|(i, o)| {
                    !used[i]
                        && usize::try_from(o.source_track_index)
                            .ok()
                            .and_then(|x| self.orig_guid_at.get(x))
                            == Some(src_guid)
                });
                let mut recv = match prev {
                    Some(i) => {
                        used[i] = true;
                        old[i].clone()
                    }
                    None => ReceiveSettings {
                        source_track_index: 0,
                        mode: 0,
                        volume: 1.0,
                        pan: 0.0,
                        mute: false,
                        mono_sum: false,
                        invert_polarity: false,
                        source_audio_channels: 0,
                        dest_audio_channels: 0,
                        pan_law: -1.0,
                        midi_channels: 0,
                        automation_mode: -1,
                    },
                };
                recv.source_track_index = *si as i32;
                if loader::send_mode_from_rpp(recv.mode) != r.send_mode {
                    recv.mode = send_mode_to_rpp(r.send_mode);
                }
                recv.volume = r.volume;
                recv.pan = r.pan;
                recv.mute = r.muted;
                recv.invert_polarity = r.phase_inverted;
                // The channels, when they say something REAPER's field
                // can: a sidechain onto 3/4 written as a plain send would
                // be heard.
                if loader::channels_to_rpp(&r.dest_channels)
                    != loader::channels_to_rpp(&loader::channels_from_rpp(recv.dest_audio_channels))
                {
                    recv.dest_audio_channels = loader::channels_to_rpp(&r.dest_channels);
                }
                // And where it takes from: a plugin's 3/4 or 5/6.
                if loader::channels_to_rpp(&r.source_channels)
                    != loader::channels_to_rpp(&loader::channels_from_rpp(
                        recv.source_audio_channels,
                    ))
                {
                    recv.source_audio_channels = loader::channels_to_rpp(&r.source_channels);
                }
                recv
            })
            .collect();
    }

    fn hardware_outputs(
        &self,
        t: &mut RppTrack,
        e: &PTrack,
        orig: Option<&OrigTrack>,
        owned: &mut Owned,
    ) {
        let outs: &[TrackRoute] = self
            .p
            .hw_outputs
            .get(&e.guid)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let key =
            |i: i32, v: f64, pan: f64, m: bool, ph: bool| (i, v.to_bits(), pan.to_bits(), m, ph);
        if let Some(o) = orig {
            let view: Vec<_> =
                o.rt.hardware_outputs
                    .iter()
                    .map(|h| key(h.output_index, h.volume, h.pan, h.mute, h.invert_polarity))
                    .collect();
            let eng: Vec<_> = outs
                .iter()
                .map(|r| {
                    key(
                        r.hw_output_index.unwrap_or(0) as i32,
                        r.volume,
                        r.pan,
                        r.muted,
                        r.phase_inverted,
                    )
                })
                .collect();
            if view == eng {
                return;
            }
        }
        owned.key("HWOUT");
        let old = std::mem::take(&mut t.hardware_outputs);
        t.hardware_outputs = outs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut hw = old.get(i).cloned().unwrap_or(HardwareOutputSettings {
                    output_index: 0,
                    send_mode: 0,
                    volume: 1.0,
                    pan: 0.0,
                    mute: false,
                    invert_polarity: false,
                    send_source_channel: 0,
                    unknown_field_8: 0,
                    automation_mode: -1,
                });
                hw.output_index = r.hw_output_index.unwrap_or(0) as i32;
                hw.volume = r.volume;
                hw.pan = r.pan;
                hw.mute = r.muted;
                hw.invert_polarity = r.phase_inverted;
                hw
            })
            .collect();
    }

    fn envelopes(&self, t: &mut RppTrack, e: &PTrack, orig: Option<&OrigTrack>, owned: &mut Owned) {
        if orig.is_none() {
            t.envelopes.clear();
        }
        for (ty, new_name, names, map) in TRACK_ENVELOPES {
            let eng = self
                .p
                .envelopes
                .get(&(e.guid.clone(), EnvelopeKey::Track(ty)))
                .filter(|d| !d.points.is_empty() || !d.automation_items.is_empty());
            let orig_env = orig.and_then(|o| {
                o.rt.envelopes
                    .iter()
                    .find(|en| names.contains(&en.envelope_type.as_str()))
            });
            let view = orig_env
                .and_then(loader::convert_track_envelope)
                .map(|(_, d)| d);
            if orig.is_some() && envelope_same(eng, view.as_ref()) {
                continue;
            }
            let chunk = orig_env
                .map_or(new_name, |o| o.envelope_type.as_str())
                .to_string();
            t.envelopes.retain(|en| en.envelope_type != chunk);
            if let Some(d) = eng {
                let mut env = orig_env.cloned().unwrap_or_else(|| RppEnvelope {
                    envelope_type: chunk.clone(),
                    guid: new_guid(),
                    active: true,
                    visible: true,
                    show_in_lane: false,
                    lane_height: 0,
                    armed: false,
                    default_shape: 0,
                    points: Vec::new(),
                    automation_items: Vec::new(),
                    extension_data: Vec::new(),
                });
                env.active = d.automation_mode != PAutomationMode::Off;
                env.visible = d.visible;
                env.armed = d.armed;
                env.points = d
                    .points
                    .iter()
                    .map(|pt| {
                        let tension = (pt.tension != 0.0).then_some(pt.tension);
                        RppEnvelopePoint {
                            position: pt.time.as_seconds(),
                            value: map.to_file(pt.value),
                            shape: envelope_shape_to_rpp(pt.shape),
                            time_sig: Some(0),
                            selected: Some(pt.selected),
                            unknown_field_6: tension.map(|_| 0),
                            bezier_tension: tension,
                        }
                    })
                    .collect();
                env.automation_items = self.instances(&e.guid, &EnvelopeKey::Track(ty), d);
                t.envelopes.push(env);
            }
            owned.sub(&chunk, &["ACT", "VIS", "ARM", "PT", "POOLEDENVINST"]);
        }
    }

    /// The FX `fx` on `track`'s parameter automation, as `<PARMENV>`s: the
    /// engine's FX-param envelopes, by parameter.
    fn param_envelopes(
        &self,
        track: &str,
        fx: &str,
    ) -> Vec<dawfile_reaper::types::fx_chain::FxParamEnvelope> {
        use dawfile_reaper::types::fx_chain::{FxEnvelopePoint, FxParamEnvelope, FxParamRef};
        let mut out: Vec<FxParamEnvelope> = self
            .p
            .envelopes
            .iter()
            .filter_map(|((owner, key), data)| match key {
                EnvelopeKey::FxParam {
                    fx_guid,
                    param_index,
                } if owner == track && fx_guid == fx => {
                    let off = data.automation_mode == PAutomationMode::Off;
                    Some(FxParamEnvelope {
                        param: FxParamRef {
                            index: *param_index,
                            name: None,
                        },
                        mode: if off { 0 } else { data.automation_mode as i32 },
                        range_max: 1.0,
                        default_value: data.points.first().map_or(0.0, |p| p.value),
                        eguid: None,
                        active: !off,
                        visible: data.visible,
                        armed: data.armed,
                        points: data
                            .points
                            .iter()
                            .map(|pt| FxEnvelopePoint {
                                time: pt.time.as_seconds(),
                                value: pt.value,
                                flags: vec![f64::from(pt.shape as u8)],
                            })
                            .collect(),
                        automation_items: self.instances(track, key, data),
                        raw_block: String::new(),
                    })
                }
                _ => None,
            })
            .collect();
        out.sort_by_key(|env| env.param.index);
        out
    }

    /// The built-in FX of the injected factory, by name, on an engine
    /// track.
    fn engine_builtins(&self, guid: &str) -> Vec<&'a daw_proto::fx::Fx> {
        let Some(factory) = self.factory else {
            return Vec::new();
        };
        self.p
            .fx_chains
            .get(&FxChainKey::Track(guid.to_string()))
            .map(|chain| {
                chain
                    .iter()
                    .map(|entry| &entry.fx)
                    .filter(|fx| factory.provides(&fx.name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The track's FX chain. An original track keeps its `<FXCHAIN>`
    /// verbatim unless its built-in FX changed; then the returned chunk
    /// is the original with the built-in nodes replaced. A new track gets
    /// its built-ins and any plugin loaded from a bundle path.
    fn fx(
        &self,
        t: &mut RppTrack,
        e: &PTrack,
        has_orig: bool,
        owned: &mut Owned,
    ) -> Option<RChunk> {
        let builtins = self.engine_builtins(&e.guid);
        if has_orig {
            let orig_chunk = self.orig_track_chunks.get(&e.guid).and_then(|c| {
                c.children.iter().find_map(|x| match x {
                    RNodeTree::Chunk(f) if f.name().as_deref() == Some("FXCHAIN") => Some(f),
                    _ => None,
                })
            });
            let (kept, orig_builtins) = split_builtin_groups(orig_chunk, self.factory);
            // Unchanged — the same built-ins, each holding what the file
            // has it holding — and the chunk goes out as it came in.
            let engine: Vec<(String, Vec<u8>, Vec<String>)> = builtins
                .iter()
                .map(|fx| {
                    (
                        fx.name.clone(),
                        self.states.get(&fx.guid).cloned().unwrap_or_default(),
                        self.param_envelopes(&e.guid, &fx.guid)
                            .iter()
                            .map(envelope_print)
                            .collect(),
                    )
                })
                .collect();
            if orig_builtins == engine {
                return None;
            }
            owned.key("FXCHAIN");
            let mut text = String::new();
            for fx in &builtins {
                let mut node = builtin_fx_node(fx, self.states.get(&fx.guid));
                node.param_envelopes = self.param_envelopes(&e.guid, &fx.guid);
                node.write_rpp(&mut text, "");
            }
            let mut children = match (orig_chunk, kept) {
                (Some(_), kept) => kept,
                (None, _) => parse_fragment("SHOW 0\nLASTSEL 0\nDOCKED 0\n"),
            };
            children.extend(parse_fragment(&text));
            let header = orig_chunk.map_or_else(
                || rpp_tree::create_rnode_from_line("FXCHAIN"),
                |c| c.header.clone(),
            );
            return Some(RChunk { header, children });
        }

        let mut nodes = Vec::new();
        let chain = self
            .p
            .fx_chains
            .get(&FxChainKey::Track(e.guid.clone()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for entry in chain {
            let fx = &entry.fx;
            if builtins.iter().any(|b| b.guid == fx.guid) {
                let mut node = builtin_fx_node(fx, self.states.get(&fx.guid));
                node.param_envelopes = self.param_envelopes(&e.guid, &fx.guid);
                nodes.push(FxChainNode::Plugin(node));
            } else if let Some(node) = bundle_fx_node(fx) {
                nodes.push(FxChainNode::Plugin(node));
            } else {
                self.skipped_fx
                    .borrow_mut()
                    .push(format!("{} on {}", fx.name, e.name));
            }
        }
        t.fx_chain = (!nodes.is_empty()).then(|| FxChain {
            window_rect: None,
            show: 0,
            last_sel: 0,
            docked: false,
            nodes,
            raw_content: String::new(),
        });
        None
    }

    // ── items ──────────────────────────────────────────────────────

    /// One item, and whether it is exactly the original (then the
    /// returned item still carries its raw block and goes out verbatim).
    fn item(&self, e: &daw_proto::Item, lane_count: u32) -> (RppItem, bool) {
        let p = self.p;
        let orig = self.orig_items.get(&e.guid);
        let tl = p.takes.get(&e.guid);
        let takes: &[PTake] = tl.map_or(&[], |t| t.takes.as_slice());
        let active = tl.map_or(0, |t| t.active_idx);
        let view = orig.map(|o| loader::item_from_rpp(o.ri, &e.track_guid, o.index, o.lane_count));

        let item_same = |v: &daw_proto::Item| {
            v.position.as_seconds() == e.position.as_seconds()
                && v.length.as_seconds() == e.length.as_seconds()
                && v.snap_offset.as_seconds() == e.snap_offset.as_seconds()
                && v.muted == e.muted
                && v.selected == e.selected
                && v.volume == e.volume
                && v.fade_in_length.as_seconds() == e.fade_in_length.as_seconds()
                && v.fade_in_shape == e.fade_in_shape
                && v.fade_out_length.as_seconds() == e.fade_out_length.as_seconds()
                && v.fade_out_shape == e.fade_out_shape
                && v.color == e.color
                && v.loop_source == e.loop_source
                && v.fixed_lane == e.fixed_lane
                && v.label == e.label
        };
        if let (Some(o), Some(v)) = (orig, view.as_ref())
            && item_same(v)
            && o.ri.takes.len() == takes.len()
            && loader::active_take_index(o.ri) == active
            && o.ri
                .takes
                .iter()
                .zip(takes)
                .all(|(rt, te)| self.take_same(rt, te))
        {
            return (o.ri.clone(), true);
        }

        let mut ri = orig.map(|o| o.ri.clone()).unwrap_or_default();
        ri.raw_content.clear();
        ri.item_guid = Some(e.guid.clone());
        ri.position = e.position.as_seconds();
        ri.length = e.length.as_seconds();
        ri.snap_offset = e.snap_offset.as_seconds();
        ri.selected = e.selected;
        ri.loop_source = e.loop_source;
        ri.mute = Some(MuteSettings {
            muted: e.muted,
            solo_state: ri
                .mute
                .as_ref()
                .map_or(SoloState::NotSoloed, |m| m.solo_state),
        });
        let v = view.as_ref();
        let fade = |old: &Option<FadeSettings>,
                    vlen: Option<f64>,
                    vshape: Option<FadeShape>,
                    len: f64,
                    shape: FadeShape| {
            if vlen == Some(len) && vshape == Some(shape) {
                return old.clone();
            }
            let mut f = old.clone().unwrap_or(FadeSettings {
                curve_type: FadeCurveType::Linear,
                time: 0.0,
                unknown_field_3: 0.0,
                unknown_field_4: 0,
                unknown_field_5: 0,
                unknown_field_6: 0,
                unknown_field_7: 0,
            });
            if vshape != Some(shape) {
                f.curve_type = fade_shape_to_curve(shape);
            }
            f.time = len;
            Some(f)
        };
        ri.fade_in = fade(
            &ri.fade_in,
            v.map(|v| v.fade_in_length.as_seconds()),
            v.map(|v| v.fade_in_shape),
            e.fade_in_length.as_seconds(),
            e.fade_in_shape,
        );
        ri.fade_out = fade(
            &ri.fade_out,
            v.map(|v| v.fade_out_length.as_seconds()),
            v.map(|v| v.fade_out_shape),
            e.fade_out_length.as_seconds(),
            e.fade_out_shape,
        );
        if v.is_none_or(|v| v.color != e.color) {
            ri.color = e.color.map(rgb_to_native);
        }
        if v.is_none_or(|v| v.fixed_lane != e.fixed_lane) {
            let n = f64::from(lane_count.max(1));
            ri.lane = e.fixed_lane.map(|l| l as i32);
            ri.y_pos = e.fixed_lane.map(|l| ItemYPos {
                y: f64::from(l) / n,
                height: 1.0 / n,
                mode: 0,
            });
        }

        let old_takes = std::mem::take(&mut ri.takes);
        ri.takes = takes
            .iter()
            .enumerate()
            .map(|(k, te)| {
                let prev = old_takes
                    .iter()
                    .find(|rt| rt.take_guid.as_deref() == Some(te.guid.as_str()));
                self.take(te, prev, k as u32 == active)
            })
            .collect();

        // The label is the item's `<NOTES>` (REAPER's `P_NOTES`), which a
        // file keeps inside take #0's lines: the chord and key items' names
        // live here, and without it a saved song's CHORDS lane was blank.
        for take in &mut ri.takes {
            take.extra_blocks.retain(|b| !loader::is_notes_block(b));
        }
        if let Some(label) = e.label.as_deref().filter(|l| !l.is_empty()) {
            if ri.takes.is_empty() {
                ri.takes.push(RppTake::default());
            }
            if let Some(t0) = ri.takes.first_mut() {
                t0.extra_blocks.push(notes_block(label));
            }
        }

        // Take #0 is written inline on the item — mirror it there.
        if let Some(t0) = ri.takes.first_mut() {
            let vp = t0.volpan.get_or_insert(TakeVolPan {
                item_trim: 1.0,
                take_pan: 0.0,
                take_volume: 1.0,
                take_pan_law: -1.0,
            });
            vp.item_trim = e.volume;
            ri.name = t0.name.clone();
            ri.volpan = t0.volpan.clone();
            ri.slip_offset = t0.slip_offset;
            ri.playrate = t0.playrate.clone();
            ri.channel_mode = t0.channel_mode;
            ri.take_guid = t0.take_guid.clone();
            ri.rec_pass = t0.rec_pass;
            ri.stretch_markers = t0.stretch_markers.clone();
        } else {
            ri.name.clear();
            ri.take_guid = None;
            ri.stretch_markers.clear();
            ri.volpan = Some(TakeVolPan {
                item_trim: e.volume,
                take_pan: 0.0,
                take_volume: 1.0,
                take_pan_law: -1.0,
            });
        }
        (ri, false)
    }

    /// Whether the engine's take is exactly what the loader read from
    /// `rt` — fields, stretch markers and MIDI events.
    fn take_same(&self, rt: &RppTake, te: &PTake) -> bool {
        let v = loader::build_take("", 0, rt);
        let fields = v.guid == te.guid
            && v.name == te.name
            && v.volume == te.volume
            && v.play_rate == te.play_rate
            && v.channel_mode == te.channel_mode
            && v.start_offset.as_seconds() == te.start_offset.as_seconds()
            && v.source_type == te.source_type
            && v.source_file_path == te.source_file_path;
        fields && self.stretch_same(rt, te) && self.midi_same(rt, te)
    }

    fn stretch_same(&self, rt: &RppTake, te: &PTake) -> bool {
        let eng = self
            .p
            .stretch_markers
            .get(&te.guid)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let view = loader::stretch_markers_from_rpp(rt);
        view.len() == eng.len()
            && view.iter().zip(eng).all(|(a, b)| {
                a.position == b.position
                    && a.source_position == b.source_position
                    && a.slope == b.slope
            })
    }

    fn midi_same(&self, rt: &RppTake, te: &PTake) -> bool {
        if !te.is_midi {
            return true;
        }
        let eng = MidiSet::of_take(self.p, &te.guid).events();
        let view = rt
            .source
            .as_ref()
            .and_then(|s| s.midi_data.as_ref())
            .map(|m| {
                let decoded = loader::decode_midi_source(m);
                MidiSet::of_decoded(&decoded).events()
            })
            .unwrap_or_default();
        eng == view
    }

    fn take(&self, te: &PTake, prev: Option<&RppTake>, selected: bool) -> RppTake {
        let v = prev.map(|rt| loader::build_take("", 0, rt));
        let v = v.as_ref();
        let mut rt = prev.cloned().unwrap_or_default();
        rt.take_guid = Some(te.guid.clone());
        rt.is_selected = selected;
        if v.is_none_or(|v| v.name != te.name) {
            rt.name = te.name.clone();
        }
        if v.is_none_or(|v| v.volume != te.volume) || rt.volpan.is_none() {
            let old = rt.volpan.clone();
            rt.volpan = Some(TakeVolPan {
                item_trim: old.as_ref().map_or(1.0, |o| o.item_trim),
                take_pan: old.as_ref().map_or(0.0, |o| o.take_pan),
                take_volume: te.volume,
                take_pan_law: old.as_ref().map_or(-1.0, |o| o.take_pan_law),
            });
        }
        if v.is_none_or(|v| v.play_rate != te.play_rate) || rt.playrate.is_none() {
            let old = rt.playrate.clone();
            rt.playrate = Some(PlayRateSettings {
                rate: te.play_rate,
                preserve_pitch: old.as_ref().map_or(true, |o| o.preserve_pitch),
                pitch_adjust: old.as_ref().map_or(0.0, |o| o.pitch_adjust),
                pitch_mode: old
                    .as_ref()
                    .map_or(PitchMode::ProjectDefault, |o| o.pitch_mode),
                unknown_field_5: old.as_ref().map_or(0, |o| o.unknown_field_5),
                unknown_field_6: old.as_ref().map_or(0.0, |o| o.unknown_field_6),
            });
        }
        if v.is_none_or(|v| v.channel_mode != te.channel_mode) {
            rt.channel_mode = ChannelMode::from(te.channel_mode as i32);
        }
        if v.is_none_or(|v| v.start_offset.as_seconds() != te.start_offset.as_seconds()) {
            rt.slip_offset = te.start_offset.as_seconds();
        }
        if prev.is_none_or(|prev| !self.stretch_same(prev, te)) {
            rt.stretch_markers = self
                .p
                .stretch_markers
                .get(&te.guid)
                .map(|ms| {
                    ms.iter()
                        .map(|m| RppStretchMarker {
                            position: m.position,
                            source_position: m.source_position,
                            rate: (m.slope != 0.0).then_some(m.slope),
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        let source_same = v.is_some_and(|v| {
            v.source_type == te.source_type && v.source_file_path == te.source_file_path
        });
        if !source_same {
            rt.source = self.new_source(te);
        } else if let Some(prev) = prev
            && !self.midi_same(prev, te)
        {
            let events = MidiSet::of_take(self.p, &te.guid).events();
            let mut src = rt.source.take().unwrap_or(SourceBlock {
                source_type: RppSourceType::Midi,
                file_path: String::new(),
                midi_data: None,
                raw_content: String::new(),
            });
            src.raw_content.clear();
            let midi = src.midi_data.get_or_insert_with(new_midi_source);
            fill_midi_source(midi, &events);
            rt.source = Some(src);
        }
        rt
    }

    fn new_source(&self, te: &PTake) -> Option<SourceBlock> {
        if te.is_midi || te.source_type == SourceType::Midi {
            let mut midi = new_midi_source();
            fill_midi_source(&mut midi, &MidiSet::of_take(self.p, &te.guid).events());
            return Some(SourceBlock {
                source_type: RppSourceType::Midi,
                file_path: String::new(),
                midi_data: Some(midi),
                raw_content: String::new(),
            });
        }
        let path = te.source_file_path.as_deref().filter(|p| !p.is_empty())?;
        Some(SourceBlock {
            source_type: source_type_for(path, te.source_type),
            file_path: self.media_path(path),
            midi_data: None,
            raw_content: String::new(),
        })
    }

    /// An absolute path inside the project folder, relative to it.
    fn media_path(&self, path: &str) -> String {
        let p = Path::new(path);
        if let Some(dir) = &self.media_dir
            && p.is_absolute()
            && let Ok(rel) = p.strip_prefix(dir)
        {
            return rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
        }
        path.to_string()
    }
}

fn position_seconds(pos: &daw_proto::Position) -> f64 {
    pos.time.map_or(0.0, |t| t.as_seconds())
}

fn tempo_point_same(a: &daw_proto::TempoPoint, b: &daw_proto::TempoPoint) -> bool {
    position_seconds(&a.position) == position_seconds(&b.position)
        && a.bpm == b.bpm
        && a.time_signature.map(|s| (s.numerator, s.denominator))
            == b.time_signature.map(|s| (s.numerator, s.denominator))
}

fn blank_marker(flags: i32) -> MarkerRegion {
    MarkerRegion {
        id: 0,
        position: 0.0,
        name: String::new(),
        color: 0,
        flags,
        locked: 0,
        guid: String::new(),
        additional: 0,
        end_position: None,
        lane: None,
        beat_position: None,
    }
}

/// A track the engine added, with REAPER's defaults for everything the
/// engine does not set.
fn new_track() -> RppTrack {
    RppTrack {
        automation_mode: RAutomationMode::TrimRead,
        mutesolo: Some(MuteSoloSettings {
            mute: false,
            solo: TrackSoloState::NoSolo,
            solo_defeat: false,
        }),
        master_send: Some(MasterSendSettings {
            enabled: true,
            unknown_field_2: 0,
        }),
        ..RppTrack::default()
    }
}

/// `ISBUS` for a depth delta: `1 1` opens a folder, `2 -n` closes `n`.
fn folder_settings(delta: i32) -> FolderSettings {
    match delta.signum() {
        1 => FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 1,
        },
        -1 => FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: delta,
        },
        _ => FolderSettings {
            folder_state: FolderState::Regular,
            indentation: 0,
        },
    }
}

/// The `ISBUS` depth delta of every track, from the engine's parent links
/// and order — never its stored `folder_depth`, which a reparenting pass
/// does not keep current.
///
/// Walks the tracks with a stack of open folders: a track closes every
/// open folder that is not its parent, then (tentatively) opens one of
/// its own. A track whose parent is not an enclosing folder at that point
/// (not contiguous with it) sits at the level it can: the file cannot say
/// anything else. An empty folder (no child follows it) writes as a
/// plain track, for the same reason.
fn folder_deltas(tracks: &[PTrack]) -> Vec<i32> {
    let mut depths = Vec::with_capacity(tracks.len());
    let mut stack: Vec<&str> = Vec::new();
    for t in tracks {
        match t.parent_guid.as_deref() {
            Some(parent) if stack.contains(&parent) => {
                while stack.last() != Some(&parent) {
                    stack.pop();
                }
            }
            _ => stack.clear(),
        }
        depths.push(stack.len() as i32);
        stack.push(t.guid.as_str());
    }
    (0..depths.len())
        .map(|i| depths.get(i + 1).copied().unwrap_or(0) - depths[i])
        .collect()
}

/// The fixed-lane lines for an engine track: the inverse of
/// `FixedLaneFields::decode` and `Track::lane_comping`.
fn write_lanes(t: &mut RppTrack, e: &PTrack, ext: &TrackExt) {
    if e.lane_count == 0 {
        t.fixed_lanes = None;
        t.lane_solo = None;
        t.lane_names = None;
        t.lane_record = None;
        t.item_lanes = None;
        t.comp_areas.clear();
        return;
    }
    let old = t.fixed_lanes.clone();
    let mut bitfield = old.as_ref().map_or(0, |f| f.bitfield) & !lane_settings::BIG_LANES;
    if e.lane_display == LaneDisplay::Big {
        bitfield |= lane_settings::BIG_LANES;
    }
    t.fixed_lanes = Some(FixedLanesSettings {
        bitfield,
        allow_editing: old.as_ref().is_some_and(|f| f.allow_editing),
        show_play_only_lane: e.lane_display == LaneDisplay::One,
        mask_playback: old.as_ref().is_some_and(|f| f.mask_playback),
        recording_behavior: old.as_ref().map_or(0, |f| f.recording_behavior),
    });
    let old_solo = t.lane_solo.clone();
    t.lane_solo = Some(LaneSoloSettings {
        playing_lanes: e.lane_play_mask as u32 as i32,
        unknown_field_2: (e.lane_play_mask >> 32) as u32 as i32,
        unknown_field_3: old_solo.as_ref().map_or(0, |s| s.unknown_field_3),
        unknown_field_4: old_solo.as_ref().map_or(0, |s| s.unknown_field_4),
        unknown_field_5: old_solo.as_ref().map_or(0, |s| s.unknown_field_5),
        unknown_field_6: old_solo.as_ref().map_or(0, |s| s.unknown_field_6),
        unknown_field_7: old_solo.as_ref().map_or(0, |s| s.unknown_field_7),
        unknown_field_8: old_solo.as_ref().map_or(0, |s| s.unknown_field_8),
    });
    t.lane_names = Some(LaneNameSettings {
        lane_count: e.lane_names.len() as i32,
        lane_names: e.lane_names.clone(),
    });
    t.item_lanes = Some(e.lane_count as i32);
    let c = &ext.comping;
    let lane = |l: Option<u32>| l.map_or(-1, |l| l as i32);
    t.lane_record = (*c != daw_proto::track::LaneComping::default()).then(|| LaneRecordSettings {
        record_enabled_lane: lane(c.record_lane),
        comping_enabled_lane: lane(c.comp_lane),
        last_comping_lane: lane(c.last_comp_lane),
    });
    t.comp_areas = c.areas.iter().map(comp_area_from_proto).collect();
}

/// The RPP node for a built-in FX: a CLAP node whose display name and
/// plugin id are both the factory name (see the module docs).
fn builtin_fx_node(fx: &daw_proto::fx::Fx, state: Option<&Vec<u8>>) -> FxPlugin {
    FxPlugin {
        name: fx.name.clone(),
        custom_name: None,
        plugin_type: PluginType::Clap,
        file: fx.name.clone(),
        bypassed: !fx.enabled,
        offline: fx.offline,
        fxid: None,
        preset_name: None,
        float_pos: None,
        wak: None,
        parallel: false,
        // What the plugin holds, as a CLAP block's lines: the loader
        // hands it back (`project_loader`'s built-in path).
        state_data: state.map_or_else(Vec::new, |s| crate::rpp_state::state_to_reaper_clap(s)),
        raw_block: String::new(),
        param_envelopes: Vec::new(),
        params_on_tcp: Vec::new(),
        header_extra: None,
    }
}

/// A CLAP / VST3 plugin the engine loaded from a bundle path, written so
/// the loader finds the same bundle again (an absolute `file` that exists
/// is taken as is). Its state is not written: the engine does not keep a
/// plugin's state in a form a project file can carry.
fn bundle_fx_node(fx: &daw_proto::fx::Fx) -> Option<FxPlugin> {
    use daw_proto::fx::FxType;
    let (plugin_type, name) = match fx.plugin_type {
        FxType::Clap => (PluginType::Clap, fx.plugin_name.clone()),
        FxType::Vst3 => (PluginType::Vst3, format!("VST3: {}", fx.plugin_name)),
        _ => return None,
    };
    if !Path::new(&fx.name).exists() {
        return None;
    }
    let mut node = builtin_fx_node(fx, None);
    node.plugin_type = plugin_type;
    node.name = name;
    Some(node)
}

/// A parameter envelope written down for comparing: its parameter, whether
/// it plays, and its points — the same for an engine envelope and the
/// file's `<PARMENV>` it would be written as.
fn envelope_print(env: &dawfile_reaper::types::fx_chain::FxParamEnvelope) -> String {
    let points: Vec<String> = env
        .points
        .iter()
        .map(|p| {
            format!(
                "{:.6}:{:.6}:{}",
                p.time,
                p.value,
                p.flags.first().copied().unwrap_or(0.0)
            )
        })
        .collect();
    let mode = if env.active {
        env.mode.to_string()
    } else {
        "off".to_owned()
    };
    let items: Vec<String> = env
        .automation_items
        .iter()
        .map(|ai| format!("{}@{:.6}+{:.6}", ai.pool_index, ai.position, ai.length))
        .collect();
    format!(
        "{} {mode} {} {}",
        env.param.index,
        points.join(" "),
        items.join(" ")
    )
}

/// A `<PARMENV>` chunk's lines, header first and `>` last.
fn chunk_lines(chunk: &RChunk) -> Vec<String> {
    let mut header = chunk.header.clone();
    let mut lines = vec![format!("<{}", header.get_tokens_as_line())];
    for child in &chunk.children {
        if let RNodeTree::Node(node) = child {
            let mut node = node.clone();
            lines.push(
                node.line
                    .clone()
                    .unwrap_or_else(|| node.get_tokens_as_line()),
            );
        }
    }
    lines.push(">".to_owned());
    lines
}

/// An original `<FXCHAIN>`'s children without its built-in FX, and the
/// built-ins' names, states (as its `<CLAP>` block holds it, empty for
/// none) and parameter envelopes ([`envelope_print`]) in order. An FX in
/// the chunk is the run of lines from its `BYPASS` to the next one.
fn split_builtin_groups(
    chunk: Option<&RChunk>,
    factory: Option<&dyn FxFactory>,
) -> (Vec<RNodeTree>, Vec<(String, Vec<u8>, Vec<String>)>) {
    let Some(chunk) = chunk else {
        return (Vec::new(), Vec::new());
    };
    /// Move one FX's lines to `kept`, or record it as a built-in.
    fn flush(
        group: &mut Vec<RNodeTree>,
        kept: &mut Vec<RNodeTree>,
        names: &mut Vec<(String, Vec<u8>, Vec<String>)>,
        factory: Option<&dyn FxFactory>,
    ) {
        let builtin = group.iter().find_map(|c| match c {
            RNodeTree::Chunk(p) if p.name().as_deref() == Some("CLAP") => {
                let factory = factory?;
                let mut h = p.header.clone();
                let file = h.get_param(1).unwrap_or_default();
                let name = h.get_param(0).unwrap_or_default();
                let name = [file, name]
                    .into_iter()
                    .find(|n| !n.is_empty() && factory.provides(n))?;
                let lines: Vec<String> = p
                    .children
                    .iter()
                    .filter_map(|c| match c {
                        RNodeTree::Node(n) => n.line.as_ref().map(|l| l.trim().to_string()),
                        RNodeTree::Chunk(_) => None,
                    })
                    .filter(|l| !l.is_empty())
                    .collect();
                let state = if lines.is_empty() {
                    Vec::new()
                } else {
                    crate::rpp_state::reaper_clap_to_state(&lines).unwrap_or_default()
                };
                Some((name, state))
            }
            _ => None,
        });
        // Its `<PARMENV>`s, as the engine's would be written.
        let envelopes: Vec<String> = group
            .iter()
            .filter_map(|c| match c {
                RNodeTree::Chunk(env) if env.name().as_deref() == Some("PARMENV") => {
                    let lines = chunk_lines(env);
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    Some(envelope_print(
                        &dawfile_reaper::types::fx_chain::parse_parmenv(&lines),
                    ))
                }
                _ => None,
            })
            .collect();
        let builtin = builtin.map(|(name, state)| (name, state, envelopes));
        match builtin {
            Some(n) => names.push(n),
            None => kept.append(group),
        }
        group.clear();
    }
    let mut kept = Vec::new();
    let mut names = Vec::new();
    let mut group: Vec<RNodeTree> = Vec::new();
    let mut in_group = false;
    for child in &chunk.children {
        if child_key(child) == "BYPASS" {
            if in_group {
                flush(&mut group, &mut kept, &mut names, factory);
            }
            in_group = true;
        }
        if in_group {
            group.push(child.clone());
        } else {
            kept.push(child.clone());
        }
    }
    if in_group {
        flush(&mut group, &mut kept, &mut names, factory);
    }
    (kept, names)
}
