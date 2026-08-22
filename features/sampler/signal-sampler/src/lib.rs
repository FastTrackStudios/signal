//! Sample library playback engine for Signal.
//!
//! Loads and plays any sample library described by a `.styx` spec file:
//! orchestral strings, brass, winds, drums, piano — anything.
//!
//! # Architecture
//!
//! ```text
//! LibrarySpec  (loaded from a .styx spec file via facet-styx)
//!   + SampleMap  (scanned from extracted WAV root directory)
//!   = PlayerPatch  (combined playback context)
//!       → SampleEngine  (MIDI-driven voice engine, one per section/instrument)
//!           → SamplerBank  (N engines, MIDI channel routing, stereo mix)
//!               → SamplerRig  (daw-backed: SamplerBank on daw's AudioEngine)
//! ```
//!
//! # Memory model
//!
//! A sampled piano is tens of gigabytes; keeping it playable is a question of
//! never holding it. Four mechanisms, in the order they save you:
//!
//! 0. **Streaming** ([`engine::stream`]) is the default for FLAC pack
//!    entries: a 0.25 s head stays resident as 16-bit PCM and the rest is
//!    decoded a chunk at a time, straight out of the mapped pack, by a
//!    background thread. The library stays compressed on disk — no
//!    conversion, no second copy — and a whole Keyscape instrument costs
//!    ~17 MB instead of 744 MB. `FTS_STREAM=off` decodes whole instead
//!    (offline renders, analysis).
//! 1. **Raw-PCM packs** (`.signalpack` kinds `pcm-i16` / `pcm-i24`) are read
//!    straight out of the file mapping — no decode, no allocation, and
//!    residency is page cache the kernel evicts under pressure. Build one
//!    with `fts signal pack transcode <in> <out> --codec pcm16`. A full
//!    Keyscape instrument costs ~1 MB of process memory this way, against
//!    ~750 MB decoded.
//! 2. **The stream cache** does the same for FLAC/Ogg packs: the first load
//!    writes the decoded audio out as raw i16 and maps it back, and every
//!    load after that (this run and later runs) skips the decoder entirely.
//!    Opt in with `FTS_STREAM_CACHE_DIR=<dir>` — see [`engine::stream_cache`].
//! 3. **The preload budget** ([`engine::budget`]) is the backstop: one
//!    process-wide ceiling on decoded bytes, past which preloads stop and
//!    samples stream on demand. Default 15% of RAM, capped at 4 GB.
//!
//! Mapped samples are warmed on preload — head faulted in, tail read ahead —
//! so the audio thread never takes a disk read inside the callback.
//!
//! # Library specs
//!
//! A library spec is a `.styx` file that describes:
//! - Instrument sections (1v, 2v, Va, Ce, Ba for strings; etc.)
//! - Articulations (Vibsus, Staccato, Leg, etc.)
//! - Dynamics (CC1 layers, velocity ranges)
//! - Legato engine (pre-delay zones, portamento threshold)
//! - Keyswitch/CC58 mapping
//!
//! Third-party libraries can be added by writing a new `.styx` spec file —
//! no code changes required.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use signal_sampler::SamplerRig;
//! use std::path::Path;
//!
//! let rig = SamplerRig::new()?;
//! rig.load_instrument(
//!     "strings_1v",
//!     Path::new("specs/cinematic-strings.styx"),
//!     Some(Path::new("/path/to/wavs")),
//!     "1v", "Mix",
//! )?;
//! rig.note_on("strings_1v", 60, 100);
//! rig.cc("strings_1v", 1, 80);
//! # Ok::<(), eyre::Error>(())
//! ```

// Native-only modules (audio devices, hardware MIDI, the NAM C++ core,
// filesystem scans, the pack CLI). The wasm32 build keeps the pure engine +
// tree renderer + the keys lane machinery — see `keys_rig::KeysRig::
// open_headless` and the browser worklet entry (signal-keys-worklet).
#[cfg(not(target_arch = "wasm32"))]
pub mod api;
pub mod audio_soundsource;
pub mod bank;
pub mod block;
pub mod convolver;
pub mod document;
pub mod document_rt;
pub mod engine;
pub mod engine_spec;
pub mod instrument;
/// Lane instruments compiled off the audio thread (wasm + threads), handed
/// over by pointer through the shared heap — see the module docs.
#[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
pub mod built_lanes;
pub mod keys_rig;
#[cfg(not(target_arch = "wasm32"))]
pub mod kit_tracks;
pub mod loudness;
pub mod midi;
pub mod mixer;
pub mod module_spec;
#[cfg(not(target_arch = "wasm32"))]
pub mod nam;
#[cfg(not(target_arch = "wasm32"))]
pub mod nam_calibrate;
pub mod native;
pub mod native_osc;
pub mod soundsource;
pub mod node_render;
pub mod nord;
#[cfg(not(target_arch = "wasm32"))]
pub mod pack_cli;
pub mod pack_plan;
#[cfg(not(target_arch = "wasm32"))]
pub mod pack_rewrite;
/// The NI Essential Pianos' Color / Dynamic Range controls, as velocity-domain
/// transforms. See `features/rigs/keys/spec/piano-voice.md`.
pub mod piano_voice;
pub mod ref_match;
#[cfg(not(target_arch = "wasm32"))]
pub mod report;
pub mod preset_registry;
pub mod preset_spec;
#[cfg(not(target_arch = "wasm32"))]
pub mod retag;
pub mod rig;
pub mod rig_library;
#[cfg(not(target_arch = "wasm32"))]
pub mod rig_manager;
pub mod rig_node;
#[cfg(not(target_arch = "wasm32"))]
pub mod rig_prefs;
pub mod rig_profile;
pub mod runtime;
pub mod sample_map;
#[cfg(not(target_arch = "wasm32"))]
pub mod sampler_rig;
pub mod spec;
pub mod stats;
pub mod styx_edit;

pub use bank::{PreloadProfile, SamplerBank};
pub use block::{BlockParams, BlockSpec, SamplerBlock};
pub use convolver::Convolver;
pub use document::{
    DocCc, DocEvent, DocNote, DocumentBusRenderResult, DocumentRenderOptions, DocumentRenderResult,
    Schedule, ScheduledEvent, TempoPoint, TrackDocument, annotate, line_for_chan,
};
pub use document_rt::{BlockTransport, RealtimeScheduler};
pub use engine::cache::SignalPcmPack;
pub use engine::trace::{RenderTrace, TraceEvent, TraceKind, VoiceSpawn};
pub use engine::{ArticClass, EmittedMarker, LegatoFireEvent, LineId, PlayMode, SampleEngine};
pub use engine_spec::{BlockRef, EngineLayerSpec, EngineSpec, FxChainSlot, PortSpec, VoiceConfig};
pub use instrument::SamplerInstrument;
pub use keys_rig::{KeysInstrument, KeysRig};
pub use mixer::{
    Bus, BusStrip, ChannelStrip, DirectChannel, DrumMixer, EngineStrip, FxBackend, FxSlotStrip,
    FxTarget, MixerLayout, MixerMeters, Send as MixerSend, SendStrip,
};
pub use module_spec::{ModulePort, ModuleSpec};
#[cfg(not(target_arch = "wasm32"))]
pub use nam::NamProcessor;
pub use audio_soundsource::AudioSoundsource;
pub use native_osc::{NativeOscillator, OscWave};
pub use soundsource::{Soundsource, SoundsourceKind, SoundsourceLeaf};
pub use node_render::{LeafBackend, RenderNode, build_node_backend};
pub use preset_registry::{PresetRegistry, PresetSource, RegisteredPreset};
pub use preset_spec::{
    MacroDef, MacroTarget, MasterFxSlot, NoteRoute, PresetEngineRef, PresetModuleRef, PresetSpec,
    RoutingRule,
};
pub use rig::{BlockImpl, ModelId, RigBlock, SlotInfo};
#[cfg(not(target_arch = "wasm32"))]
pub use rig::{DeviceInfo, GuitarRig};
pub use rig_library::{Library, RigPreset, RigScene, RigSection, RigSong};
#[cfg(not(target_arch = "wasm32"))]
pub use rig_manager::RigManager;
pub use rig_node::{Combine, Container, Param, RigNode, Role, Send, Zone};
#[cfg(not(target_arch = "wasm32"))]
pub use rig_prefs::RigAudioPrefs;
#[cfg(not(target_arch = "wasm32"))]
pub use rig_profile::ProfileRig;
pub use rig_profile::{RigPatch, RigProfile};
pub use runtime::{
    BufferRef, EngineInstance, LayerRuntime, ModuleInstance, PortRuntime, PresetRuntime,
    ResolvedEdge,
};
pub use sample_map::{SampleKey, SampleMap, SampleQuery};
pub use midicore::MidiMonitor;
#[cfg(not(target_arch = "wasm32"))]
pub use sampler_rig::{BusTrack, InstrumentTrack, SamplerRig};
// Hardware MIDI input primitives live in `midicore` (the `midir` OS backend);
// re-export the selector + handle + event types so rig consumers (e.g. the
// strings TUI) don't need a direct midicore dependency.
pub use midicore;
pub use midicore::MidiEvent;
pub use midicore::PortSelector as MidiSelection;
#[cfg(not(target_arch = "wasm32"))]
pub use midicore::midir::MidiInput as MidiInputHandle;
pub use spec::LibrarySpec;
pub use stats::AudioStatsSnapshot;

use std::path::Path;

pub mod pack {
    //! `.signalpack` reader utilities.
    //!
    //! Cheap header-only inspection without decoding any audio.

    use super::{LibrarySpec, SamplerError, SignalPcmPack};
    #[cfg(not(target_arch = "wasm32"))]
    use std::path::Path;

    /// Result of [`read_pack_header`] — parsed library spec plus pack stats.
    /// No audio data is loaded.
    #[derive(Debug, Clone)]
    pub struct PackHeader {
        pub spec: LibrarySpec,
        pub sample_count: usize,
        pub size_bytes: u64,
    }

    /// Cheap header-only read from a `.signalpack`.
    ///
    /// Returns the embedded `LibrarySpec` plus pack statistics. No audio is
    /// decoded — suitable for browse/list panels.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pack_header(pack_path: &Path) -> Result<PackHeader, SamplerError> {
        let pack = SignalPcmPack::open(pack_path)?;
        let spec = parse_embedded_spec(&pack)?;
        let size_bytes = std::fs::metadata(pack_path).map(|m| m.len()).unwrap_or(0);
        Ok(PackHeader {
            spec,
            sample_count: pack.entry_count(),
            size_bytes,
        })
    }

    pub(crate) fn parse_embedded_spec(pack: &SignalPcmPack) -> Result<LibrarySpec, SamplerError> {
        let text = pack
            .embedded_spec()
            .ok_or_else(|| SamplerError::SpecParse("pack carries no embedded spec".into()))?;
        match pack.embedded_spec_format() {
            Some("toml") => LibrarySpec::from_toml(text),
            _ => LibrarySpec::from_styx(text),
        }
    }
}

pub use pack::PackHeader;
#[cfg(not(target_arch = "wasm32"))]
pub use pack::read_pack_header;

pub mod pack_registry {
    //! In-memory `.signalpack`s, keyed by the spec-path string lanes
    //! reference (`RigBlock.sample`) — the browser seam: packs arrive as
    //! fetched bytes, not files, so `build_sample_source` consults this
    //! registry before touching disk. Native callers may use it too (tests,
    //! network-fed rigs); an installed entry always wins over the
    //! filesystem.
    //!
    //! Bytes are parsed once at [`install`] (surfacing a bad pack at the
    //! transfer boundary, not at note-on) and stored as an opened
    //! [`SignalPcmPack`](super::SignalPcmPack); handing one out clones the
    //! parsed index over the shared `Arc`'d bytes — no audio is copied.

    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use super::{SamplerError, SignalPcmPack};

    static PACKS: OnceLock<Mutex<HashMap<String, SignalPcmPack>>> = OnceLock::new();

    fn packs() -> &'static Mutex<HashMap<String, SignalPcmPack>> {
        PACKS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Parse `bytes` as a `.signalpack` and install it under `key` (the
    /// spec-path string lanes reference). Replaces any previous entry.
    pub fn install(key: &str, bytes: Vec<u8>) -> Result<(), SamplerError> {
        let pack = SignalPcmPack::open_bytes(bytes)?;
        packs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.to_string(), pack);
        Ok(())
    }

    /// Install an EXTERNAL pack under `key`: the pack's bytes stay outside
    /// this address space (the browser worklet's JS heap), reachable only
    /// through the process-wide reader installed with
    /// [`fts_sample::cache::set_external_pack_reader`]. Header + index are
    /// parsed through that reader here (surfacing a bad pack, or a missing
    /// reader, at the transfer boundary); audio entries materialize
    /// per-entry at decode time. Replaces any previous entry.
    pub fn install_external(key: &str, id: u32, len: u64) -> Result<(), SamplerError> {
        let pack = SignalPcmPack::open_external(id, len)?;
        packs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.to_string(), pack);
        Ok(())
    }

    /// Remove the entry under `key` (already-built engines keep their clone).
    pub fn remove(key: &str) {
        packs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
    }

    /// The pack installed for `path` — exact key match first, then a
    /// file-name match so `packs/foo.signalpack` resolves an entry
    /// installed as `foo.signalpack` (and vice versa).
    pub fn get(path: &str) -> Option<SignalPcmPack> {
        let map = packs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(p) = map.get(path) {
            return Some(p.clone());
        }
        let file_name = std::path::Path::new(path).file_name()?;
        map.iter()
            .find(|(k, _)| std::path::Path::new(k).file_name() == Some(file_name))
            .map(|(_, p)| p.clone())
    }

    /// Installed keys (diagnostics / soundsource-manager UIs).
    pub fn keys() -> Vec<String> {
        packs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }
}

/// Identifier for a loaded instrument within the bank.
pub type InstrumentId = String;

// ── Error ─────────────────────────────────────────────────────────────────────

// Defined in fts-sample (the audio-file engine layer); re-exported so both
// `crate::SamplerError` and `signal_sampler::SamplerError` keep working.
pub use fts_sample::SamplerError;

// ── PlayerPatch ───────────────────────────────────────────────────────────────

/// A fully loaded library patch: spec + sample index.
///
/// Two playback modes are supported:
/// - **Convention mode** — sample lookup goes through `map` and decodes
///   `(section, articulation, mic, dynamic, note, direction, rr)`. Used by
///   libraries with regular filename conventions (Cinematic Strings, Keyscape).
/// - **Zone mode** — `spec.zones` is non-empty. Sample lookup goes through
///   `zone_paths` and resolves `(note, velocity, rr_idx)` directly via
///   [`resolve_zone`](Self::resolve_zone). Used by Spectrasonics-style
///   libraries (Omnisphere, Trilian) where the keymap is metadata, not
///   filenames.
///
/// Cloning is cheap-ish (spec + path lists; the pack's audio bytes are
/// shared) — per-mic kit engines clone one loaded patch per mic.
#[derive(Clone)]
pub struct PlayerPatch {
    pub spec: LibrarySpec,
    pub map: SampleMap,
    /// Absolute paths parallel to `spec.zones` (zone mode only; empty otherwise).
    pub zone_paths: Vec<std::path::PathBuf>,
    /// Absolute paths parallel to `spec.grooves`. Empty when the spec has none.
    pub groove_paths: Vec<std::path::PathBuf>,
    /// Absolute paths parallel to `spec.wavetables`. Empty when the spec has none.
    pub wavetable_paths: Vec<std::path::PathBuf>,
    pub prepared_cache_dir: Option<std::path::PathBuf>,
    /// When loaded from a `.signalpack`, the pack itself supplies all audio.
    /// `SampleEngine` builds its cache directly from this rather than from
    /// disk, so no on-disk source files need exist.
    pub pack: Option<SignalPcmPack>,
}

/// Resolved zone match: where to read the sample plus how to play it.
#[derive(Debug, Clone)]
pub struct ResolvedZone {
    pub path: std::path::PathBuf,
    /// Pitch at which the sample plays back unchanged.
    pub root_key: u8,
    /// Per-zone gain in dB.
    pub gain_db: f32,
    /// Per-zone fine-tune in cents (added on top of the root-key transposition).
    pub tune_cents: f32,
}

impl PlayerPatch {
    /// Load a spec and scan WAV files under `samples_root`.
    ///
    /// If the spec carries explicit `zones`, the patch is built in zone mode and
    /// the on-disk filename scan is skipped (zoned libraries have arbitrary
    /// filenames that the convention parser cannot handle).
    pub fn load(spec_path: &Path, samples_root: &Path) -> Result<Self, SamplerError> {
        let spec = LibrarySpec::from_file(spec_path)?;
        let zoned = !spec.zones.is_empty();
        let zone_paths = if zoned {
            spec.zones
                .iter()
                .map(|z| samples_root.join(&z.file))
                .collect()
        } else {
            Vec::new()
        };
        let groove_paths = spec
            .grooves
            .iter()
            .map(|g| samples_root.join(&g.file))
            .collect();
        let wavetable_paths = spec
            .wavetables
            .iter()
            .map(|w| samples_root.join(&w.file))
            .collect();
        // Filename-scan only when neither zones nor grooves carry the layout.
        let convention_mode = !zoned && spec.grooves.is_empty();
        let map = if convention_mode {
            SampleMap::scan(samples_root)?
        } else {
            SampleMap::empty()
        };
        let prepared_cache_dir = engine::cache::default_prepared_cache_dir(samples_root);
        let prepared_cache_dir = prepared_cache_dir
            .join("index.tsv")
            .exists()
            .then_some(prepared_cache_dir);
        Ok(Self {
            spec,
            map,
            zone_paths,
            groove_paths,
            wavetable_paths,
            prepared_cache_dir,
            pack: None,
        })
    }

    /// Load a patch whose **samples** come from `zones_path` (a zone spec) but
    /// whose **engine config** — sections / mics / dynamics / articulations /
    /// legato engine / keyswitch + CC58 map — comes from a separate descriptive
    /// spec at `config_path`.
    ///
    /// Cinematic Studio libraries ship this way: `zones.styx` (or a per-section
    /// `_patches/<section>/library.styx`) carries only the sample paths, and its
    /// own header says to load `cinematic-strings.styx` alongside it for the
    /// engine config. Without the config, the runtime has no articulation
    /// definitions or keyswitch/CC58 map, so articulation switching can't work.
    pub fn load_merged(
        config_path: &Path,
        zones_path: &Path,
        samples_root: &Path,
    ) -> Result<Self, SamplerError> {
        let mut patch = Self::load(zones_path, samples_root)?;
        let cfg = LibrarySpec::from_file(config_path)?;
        // Overlay the descriptive config; keep the zone spec's samples
        // (zones / zone_paths / grooves / wavetables / map stay as loaded).
        patch.spec.sections = cfg.sections;
        patch.spec.mics = cfg.mics;
        patch.spec.dynamics = cfg.dynamics;
        patch.spec.articulations = cfg.articulations;
        patch.spec.legato_engine = cfg.legato_engine;
        patch.spec.short_note_timing = cfg.short_note_timing;
        patch.spec.keyswitch = cfg.keyswitch;
        patch.spec.performance = cfg.performance;
        Ok(patch)
    }

    /// Build a patch from an already-parsed spec with an empty sample map.
    pub fn from_spec(spec: LibrarySpec) -> Self {
        Self {
            spec,
            map: SampleMap::empty(),
            zone_paths: Vec::new(),
            groove_paths: Vec::new(),
            wavetable_paths: Vec::new(),
            prepared_cache_dir: None,
            pack: None,
        }
    }

    /// Load a patch directly from a `.signalpack`.
    ///
    /// The pack's embedded styx supplies the [`LibrarySpec`]; sample data
    /// decodes straight from the pack body — no on-disk audio required.
    ///
    /// Wavetables are ignored entirely (out of scope for the sampler engine).
    /// Grooves with no explicit `zones` are surfaced as one synthesized zone
    /// per groove rooted at `slice_base_note` so a single MIDI key triggers
    /// the whole loop. Slicing and time-stretch are intentionally not
    /// implemented — the loop plays sample-rate-locked at original tempo.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_pack(pack_path: &Path) -> Result<Self, SamplerError> {
        use crate::engine::cache::SignalPcmPack;
        let pack = SignalPcmPack::open(pack_path)?;
        Self::from_opened_pack(pack)
    }

    /// [`from_pack`](Self::from_pack) over a pack handed over as one
    /// in-memory buffer — the wasm path (a fetched pack), and anything else
    /// with no file to open. See also [`crate::pack_registry`].
    pub fn from_pack_bytes(bytes: Vec<u8>) -> Result<Self, SamplerError> {
        let pack = crate::engine::cache::SignalPcmPack::open_bytes(bytes)?;
        Self::from_opened_pack(pack)
    }

    /// Build a patch from an ALREADY-OPENED pack, however its bytes arrived
    /// (mmap'd file, in-memory buffer, registry clone).
    pub fn from_opened_pack(pack: crate::engine::cache::SignalPcmPack) -> Result<Self, SamplerError> {
        let mut spec = pack::parse_embedded_spec(&pack)?;

        // Stylus / groove libraries: synthesize one zone per groove so the
        // existing zone-mode playback path triggers the whole loop on
        // note-on. This is the documented simplification — no slicing, no
        // time-stretch, no tempo sync. A single key (slice_base_note, or 36
        // by default) plays the entire loop at its native sample rate.
        if spec.zones.is_empty() && !spec.grooves.is_empty() {
            for g in &spec.grooves {
                let root = if g.slice_base_note == 0 {
                    36
                } else {
                    g.slice_base_note
                };
                spec.zones.push(crate::spec::ZoneSpec {
                    file: g.file.clone(),
                    key_min: root,
                    key_max: root,
                    root_key: root,
                    vel_min: 0,
                    vel_max: 127,
                    rr_index: 0,
                    rr_mode: String::new(),
                    gain_db: 0.0,
                    pan: 0.0,
                    tune_cents: 0.0,
                    sample_start: 0,
                    sample_end: 0,
                    loop_start: 0,
                    loop_end: 0,
                    loop_xfade: 0,
                    fade_in: 0,
                    release_start: 0,
                    playback_mode: String::new(),
                    trigger_mode: String::new(),
                    trigger_cc: 0,
                    trigger_value_min: 0,
                    trigger_value_max: 0,
                    mic: String::new(),
                    articulation: String::new(),
                    dynamic: String::new(),
                    direction: String::new(),
                    interval: 0,
                    lead_in_ms: 0.0,
                    arrival_ms: 0.0,
                    group: String::new(),
                    group_polyphony: 0,
                    choke_group: String::new(),
                    off_by: Vec::new(),
                    section: String::new(),
                    variant: String::new(),
                });
            }
        }

        // Build zone_paths from the relative file fields. The pack's
        // entry_for_path uses suffix-matching, so a relative or absolute
        // path resolves the same entry — we use the relative path verbatim
        // so the lookup is direct.
        let zone_paths: Vec<std::path::PathBuf> = spec
            .zones
            .iter()
            .map(|z| std::path::PathBuf::from(&z.file))
            .collect();
        let groove_paths: Vec<std::path::PathBuf> = spec
            .grooves
            .iter()
            .map(|g| std::path::PathBuf::from(&g.file))
            .collect();

        // Convention-mode packs (older Keyscape, CSS pre-zone-rewrite) ship
        // with no `zones` array — the engine resolves samples by parsing the
        // filenames of every entry. Build the SampleMap directly from the
        // pack's entry list so playback works even when no on-disk source
        // is available.
        let map = if spec.zones.is_empty() && spec.grooves.is_empty() {
            SampleMap::from_paths(pack.entry_paths().map(|p| p.to_path_buf()))
        } else {
            SampleMap::empty()
        };

        // Re-derive each articulation's dynamic / RR list from the actual
        // sample map. The spec stored in the pack header is whatever the
        // importer wrote at pack-build time, which can be stale or wrong
        // (e.g. Keyscape Classic `clrr10` was authored with a single
        // "127" dynamic but the v01..v19 velocity-layer samples encode 19
        // dynamics — the spec list misses 18 of them). Trusting the map
        // here keeps spec + samples consistent regardless of importer
        // quirks.
        {
            use std::collections::{BTreeMap, BTreeSet};
            let mut by_artic: BTreeMap<String, (BTreeSet<String>, BTreeSet<usize>)> =
                BTreeMap::new();
            for (k, _) in map.iter() {
                let e = by_artic.entry(k.articulation.clone()).or_default();
                e.0.insert(k.dynamic.clone());
                e.1.insert(k.rr);
            }
            for a in spec.articulations.iter_mut() {
                if let Some((dyns, rrs)) = by_artic.get(&a.id) {
                    if !dyns.is_empty() {
                        a.dynamics = dyns.iter().cloned().collect();
                    }
                    if !rrs.is_empty() {
                        let max_rr = rrs.iter().max().copied().unwrap_or(0) + 1;
                        a.rr = max_rr.max(1);
                    }
                }
            }
        }

        Ok(Self {
            spec,
            map,
            zone_paths,
            groove_paths,
            wavetable_paths: Vec::new(),
            prepared_cache_dir: None,
            pack: Some(pack),
        })
    }

    /// Whether the patch is in zone mode (explicit `(key × vel × RR)` zones).
    pub fn is_zoned(&self) -> bool {
        !self.zone_paths.is_empty()
    }

    pub fn total_samples(&self) -> usize {
        let base = if self.is_zoned() {
            self.zone_paths.len()
        } else {
            self.map.total()
        };
        base + self.groove_paths.len() + self.wavetable_paths.len()
    }

    pub fn sample_paths(&self) -> Box<dyn Iterator<Item = &std::path::PathBuf> + '_> {
        let base: Box<dyn Iterator<Item = &std::path::PathBuf> + '_> = if self.is_zoned() {
            Box::new(self.zone_paths.iter())
        } else {
            Box::new(self.map.paths())
        };
        Box::new(
            base.chain(self.groove_paths.iter())
                .chain(self.wavetable_paths.iter()),
        )
    }

    /// Sample paths ordered by `|note - center|` so the middle of the
    /// keyboard decodes first and the extremes last. Used by the
    /// background preloader so the most-played range comes online first.
    /// `center` defaults to 60 (middle C) at the call site.
    ///
    /// - Zone-mode patches: zones sorted by `|root_key - center|`.
    /// - Convention-mode patches: sample-map entries sorted by `|note - center|`.
    /// - Grooves + wavetables: appended last (no inherent pitch priority).
    pub fn sample_paths_centered(&self, center: u8) -> Vec<std::path::PathBuf> {
        let center = center as i32;
        let mut out: Vec<std::path::PathBuf> = Vec::new();
        if self.is_zoned() {
            let mut indexed: Vec<(i32, &std::path::PathBuf)> = self
                .spec
                .zones
                .iter()
                .zip(self.zone_paths.iter())
                .map(|(z, p)| ((z.root_key as i32 - center).abs(), p))
                .collect();
            indexed.sort_by_key(|(d, _)| *d);
            out.extend(indexed.into_iter().map(|(_, p)| p.clone()));
        } else {
            let mut indexed: Vec<(i32, std::path::PathBuf)> = self
                .map
                .iter()
                .map(|(k, p)| ((k.note as i32 - center).abs(), p.clone()))
                .collect();
            indexed.sort_by_key(|(d, _)| *d);
            out.extend(indexed.into_iter().map(|(_, p)| p));
        }
        out.extend(self.groove_paths.iter().cloned());
        out.extend(self.wavetable_paths.iter().cloned());
        out
    }

    /// Sample paths ordered **coverage-first**: one sample for every note
    /// (nearest `center` first), then each note's second, and so on.
    ///
    /// This is the order a *bounded* preload wants.
    /// [`sample_paths_centered`](Self::sample_paths_centered) sorts purely by
    /// distance from the centre, so truncating it loads every dynamic of a
    /// narrow band of keys and leaves the rest of the keyboard with no body
    /// voice at all. Round-robining across notes spends the same budget on a
    /// playable instrument: every key sounds, dense velocity layers fill in
    /// as the budget allows.
    pub fn sample_paths_playable(&self, center: u8) -> Vec<std::path::PathBuf> {
        use std::collections::BTreeMap;
        let center = center as i32;
        // note → its samples, in declaration order.
        let mut by_note: BTreeMap<i32, Vec<std::path::PathBuf>> = BTreeMap::new();
        if self.is_zoned() {
            for (z, p) in self.spec.zones.iter().zip(self.zone_paths.iter()) {
                by_note.entry(z.root_key as i32).or_default().push(p.clone());
            }
        } else {
            for (k, p) in self.map.iter() {
                by_note.entry(k.note as i32).or_default().push(p.clone());
            }
        }
        // Notes nearest the centre get their samples first within each round.
        let mut notes: Vec<i32> = by_note.keys().copied().collect();
        notes.sort_by_key(|n| (n - center).abs());
        let depth = by_note.values().map(|v| v.len()).max().unwrap_or(0);
        let mut out = Vec::new();
        for round in 0..depth {
            for note in &notes {
                if let Some(p) = by_note.get(note).and_then(|v| v.get(round)) {
                    out.push(p.clone());
                }
            }
        }
        out.extend(self.groove_paths.iter().cloned());
        out.extend(self.wavetable_paths.iter().cloned());
        out
    }

    pub fn resolve(
        &self,
        query: &crate::sample_map::SampleQuery<'_>,
    ) -> Option<(std::path::PathBuf, u8)> {
        self.map.resolve(&self.spec, query)
    }

    /// Resolve a zone for `(note, velocity)`, RR-cycling within the matching set.
    ///
    /// Returns `None` if the patch is not in zone mode or no zone contains the
    /// `(note, velocity)` point. Multiple matching zones form a round-robin
    /// group; `rr_idx` is reduced modulo the group size.
    pub fn resolve_zone(&self, note: u8, velocity: u8, rr_idx: usize) -> Option<ResolvedZone> {
        if !self.is_zoned() {
            return None;
        }
        // Indices of zones whose (key range, vel range) contains the point.
        let candidates: Vec<usize> = self
            .spec
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.contains(note, velocity))
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let pick = candidates[rr_idx % candidates.len()];
        let z = &self.spec.zones[pick];
        Some(ResolvedZone {
            path: self.zone_paths[pick].clone(),
            root_key: z.root_key,
            gain_db: z.gain_db,
            tune_cents: z.tune_cents,
        })
    }

    /// EVERY zone that could sound for `note`/`velocity` — all the
    /// round-robin alternatives, not just the one a given rr index lands
    /// on.
    ///
    /// [`resolve_zone`](Self::resolve_zone) picks
    /// `candidates[rr_idx % candidates.len()]`, so a warm that only
    /// resolves rr 0 opens ONE of them and leaves the rest unopened. On a
    /// piano with several round-robins per key that means most presses ask
    /// for a sample nobody opened — heard as wrong or missing samples.
    /// Warming needs the whole candidate set.
    pub fn resolve_zone_candidates(&self, note: u8, velocity: u8) -> Vec<std::path::PathBuf> {
        if !self.is_zoned() {
            return Vec::new();
        }
        self.spec
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.contains(note, velocity))
            .map(|(i, _)| self.zone_paths[i].clone())
            .collect()
    }

    pub fn legato_delay_expressive(&self, velocity: u8) -> Option<u32> {
        self.spec
            .legato_engine
            .as_ref()?
            .expressive
            .as_ref()?
            .delay_for_velocity(velocity)
    }

    pub fn legato_delay_low_latency(&self, velocity: u8) -> Option<u32> {
        self.spec
            .legato_engine
            .as_ref()?
            .low_latency
            .as_ref()?
            .delay_for_velocity(velocity)
    }

    pub fn short_note_pre_delay_ms(&self) -> u32 {
        self.spec
            .short_note_timing
            .as_ref()
            .map(|t| t.pre_delay_ms)
            .unwrap_or(0)
    }
}
