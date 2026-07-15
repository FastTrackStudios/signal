//! Repo-free **Preset / Profile / Song** library for the standalone guitar rig.
//!
//! This is the audio-side, styx-driven projection of the Signal domain model
//! (`DOMAIN.md`) for the live rig — no storage stack, no UUIDs, names are the
//! references. The three layers it adds on top of [`RigProfile`] /
//! [`RigPatch`](crate::rig_profile::RigPatch):
//!
//! | Domain term | Type here | Holds |
//! |---|---|---|
//! | **Preset** (a playable tone) | [`RigPreset`] | named [`RigScene`]s |
//! | **Preset Snapshot** (a variant) | [`RigScene`] | an FX [`chain`](RigScene::chain) |
//! | **Profile** (sound switching) | [`RigProfile`] | `RigPatch`es → a preset scene |
//! | **Patch** (a Profile entry) | [`RigPatch`](crate::rig_profile::RigPatch) | points at `preset` + `scene` |
//! | **Song** (a performance) | [`RigSong`] | [`RigSection`]s → a patch |
//! | **Scene** (a Song entry) | [`RigSection`] | points at a profile `patch` |
//!
//! Everything the rig actually *plays* is a [`RigProfile`] (a switchable set of
//! patches), so the [`Library`] resolves each layer down to one:
//! [`preset_as_profile`](Library::preset_as_profile),
//! [`resolve_profile`](Library::resolve_profile),
//! [`song_as_profile`](Library::song_as_profile) — all hand a ready
//! [`RigProfile`] to [`ProfileRig::load_profile`](crate::ProfileRig::load_profile).
//!
//! ## On-disk layout
//!
//! A library is a directory with three subfolders of `.styx` files, one entity
//! per file:
//!
//! ```text
//! library/
//!   presets/   marshall-jcm800.styx   peavey-5150.styx   …   (RigPreset)
//!   profiles/  worship.styx           rock.styx          …   (RigProfile)
//!   songs/     amazing-grace.styx     …                      (RigSong)
//! ```

use std::path::{Path, PathBuf};

use facet::Facet;

use crate::SamplerError;
use crate::rig::RigBlock;
use crate::rig_profile::{RigPatch, RigProfile, RigStack};

/// A **Scene** — one named variant (snapshot) of a [`RigPreset`]: an ordered FX
/// chain plus scene-level trims. For an ML "full rig" capture a scene's chain is
/// a single `.nam` block (the capture already bakes in the cab); a hand-built
/// preset's scene might be drive → amp → cab → reverb.
#[derive(Debug, Clone, Facet)]
pub struct RigScene {
    /// Scene name (e.g. "Clean", "Drive", "Lead").
    pub name: String,
    /// Ordered FX chain for this scene.
    pub chain: Vec<RigBlock>,
    /// Trim applied before the chain (dB), folded into the patch's input trim.
    #[facet(default)]
    pub input_trim_db: f32,
    /// Trim applied after the chain (dB), folded into the patch's output trim.
    #[facet(default)]
    pub output_trim_db: f32,
}

impl RigScene {
    /// A scene that is a single block (e.g. one full-rig `.nam`).
    pub fn single(name: impl Into<String>, block: RigBlock) -> Self {
        Self {
            name: name.into(),
            chain: vec![block],
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }
}

/// A **Preset** — a playable tone (e.g. an amp model) holding a set of named
/// [`RigScene`]s. The standalone analogue of the domain's *Rig Preset*
/// (`signal_proto::rig::Rig`, whose variants are `RigScene`s).
#[derive(Debug, Clone, Facet)]
pub struct RigPreset {
    /// Display name (e.g. "Marshall JCM800").
    pub name: String,
    /// Named scenes (snapshots). At least one is expected.
    pub scenes: Vec<RigScene>,
    /// Index of the scene to use when no scene is named. Defaults to 0.
    #[facet(default)]
    pub default_scene: usize,
    /// Optional vendor / modeled-gear label (e.g. "ML Sound Lab").
    #[facet(default)]
    pub vendor: String,
    /// Optional category for browser grouping (e.g. "Amp", "Hi-Gain").
    #[facet(default)]
    pub category: String,
}

impl RigPreset {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scenes: Vec::new(),
            default_scene: 0,
            vendor: String::new(),
            category: String::new(),
        }
    }

    #[must_use]
    pub fn with_scene(mut self, scene: RigScene) -> Self {
        self.scenes.push(scene);
        self
    }

    /// Look up a scene by name (case-insensitive).
    pub fn scene(&self, name: &str) -> Option<&RigScene> {
        self.scenes
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// The default scene, if any.
    pub fn default_scene(&self) -> Option<&RigScene> {
        self.scenes.get(self.default_scene).or(self.scenes.first())
    }

    /// Parse a preset from a `.styx` file.
    pub fn from_styx_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }
}

/// A **Section** — one entry in a [`RigSong`]. Per the domain model a Song
/// Section *points at* a Patch (within a Profile) or, for a one-off, directly at
/// a preset scene.
#[derive(Debug, Clone, Facet)]
pub struct RigSection {
    /// Section name (e.g. "Intro", "Verse", "Chorus").
    pub name: String,
    /// Profile that owns the referenced patch. Empty = use the direct
    /// `preset`/`scene` reference instead.
    #[facet(default)]
    pub profile: String,
    /// Patch name within `profile`.
    #[facet(default)]
    pub patch: String,
    /// Direct preset reference (used when `profile`/`patch` are empty).
    #[facet(default)]
    pub preset: String,
    /// Scene within `preset` (empty = the preset's default scene).
    #[facet(default)]
    pub scene: String,
}

/// A **Song** — a performance structure: an ordered list of [`RigSection`]s,
/// each pointing at a patch. The standalone analogue of `signal_proto::song::Song`.
#[derive(Debug, Clone, Facet)]
pub struct RigSong {
    pub name: String,
    #[facet(default)]
    pub artist: String,
    pub sections: Vec<RigSection>,
    /// Index of the section to start on. Defaults to 0.
    #[facet(default)]
    pub default_section: usize,
}

impl RigSong {
    /// Parse a song from a `.styx` file.
    pub fn from_styx_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }
}

/// An in-memory catalog of [`RigPreset`]s, [`RigProfile`]s and [`RigSong`]s
/// scanned from a library directory. The browser views these; resolution
/// methods turn any of the three into a playable [`RigProfile`].
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub root: PathBuf,
    pub presets: Vec<RigPreset>,
    pub profiles: Vec<RigProfile>,
    pub songs: Vec<RigSong>,
    /// Non-fatal load errors (`<file>: <message>`), surfaced in the UI.
    pub errors: Vec<String>,
}

impl Library {
    /// Scan `root/{presets,profiles,songs}/*.styx` into a catalog. Missing
    /// subfolders are skipped; per-file parse errors are collected into
    /// [`errors`](Self::errors) rather than failing the whole load. Entries are
    /// sorted by name for a stable browser order.
    pub fn load_dir(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let mut lib = Library {
            root: root.clone(),
            ..Default::default()
        };

        for path in styx_files(&root.join("presets")) {
            match RigPreset::from_styx_file(&path) {
                Ok(p) => lib.presets.push(p),
                Err(e) => lib.errors.push(format!("{}: {e}", path.display())),
            }
        }
        for path in styx_files(&root.join("profiles")) {
            match RigProfile::from_styx_file(&path) {
                Ok(p) => lib.profiles.push(p),
                Err(e) => lib.errors.push(format!("{}: {e}", path.display())),
            }
        }
        for path in styx_files(&root.join("songs")) {
            match RigSong::from_styx_file(&path) {
                Ok(s) => lib.songs.push(s),
                Err(e) => lib.errors.push(format!("{}: {e}", path.display())),
            }
        }

        lib.presets.sort_by(|a, b| a.name.cmp(&b.name));
        lib.profiles.sort_by(|a, b| a.name.cmp(&b.name));
        lib.songs.sort_by(|a, b| a.name.cmp(&b.name));

        // Validate every block's implementation fits its type (e.g. no NAM on a
        // Delay) — surfaced in the browser without needing audio.
        for preset in &lib.presets {
            for scene in &preset.scenes {
                for block in &scene.chain {
                    if let Err(e) = block.validate() {
                        lib.errors
                            .push(format!("preset {} / {}: {e}", preset.name, scene.name));
                    }
                }
            }
        }
        for profile in &lib.profiles {
            for patch in &profile.patches {
                for block in &patch.chain {
                    if let Err(e) = block.validate() {
                        lib.errors
                            .push(format!("profile {} / {}: {e}", profile.name, patch.name));
                    }
                }
            }
        }
        lib
    }

    /// True when nothing loaded.
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty() && self.profiles.is_empty() && self.songs.is_empty()
    }

    // ── Lookups ──────────────────────────────────────────────────────────

    pub fn preset(&self, name: &str) -> Option<&RigPreset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn profile(&self, name: &str) -> Option<&RigProfile> {
        self.profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn song(&self, name: &str) -> Option<&RigSong> {
        self.songs
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    // ── Resolution to a playable RigProfile ──────────────────────────────

    /// Turn a preset into a profile whose patches are its scenes — so the whole
    /// preset can be loaded and its scenes stepped through with the patch keys.
    /// The default scene becomes the default patch.
    pub fn preset_as_profile(&self, preset: &RigPreset) -> RigProfile {
        let patches: Vec<RigPatch> = preset
            .scenes
            .iter()
            .map(|s| scene_patch(&s.name, s))
            .collect();
        let default_patch = preset.default_scene.min(patches.len().saturating_sub(1));
        RigProfile {
            name: preset.name.clone(),
            patches,
            default_patch,
            stacks: Vec::new(),
        }
    }

    /// Resolve a profile into one whose every patch carries an inline `chain`:
    /// patches that reference a `preset`/`scene` are filled from the catalog
    /// (scene trims folded into the patch's), inline-chain patches pass through.
    /// A reference that can't be resolved yields an empty-chain patch (it shows
    /// "unavailable" at load time) and records an [`error`](Self::errors)-style
    /// warning via `tracing`.
    pub fn resolve_profile(&self, profile: &RigProfile) -> RigProfile {
        let patches = profile
            .patches
            .iter()
            .map(|p| self.resolve_patch(p))
            .collect();
        RigProfile {
            name: profile.name.clone(),
            patches,
            default_patch: profile.default_patch,
            // Stacks reference patches by name, which resolution preserves, so
            // they carry straight through onto the resolved profile.
            stacks: profile.stacks.clone(),
        }
    }

    /// Resolve a single patch's `preset`/`scene` reference into an inline chain.
    ///
    /// The resolved chain is the **scene's chain** (the amp/cab core) followed by
    /// the patch's own inline `chain` (its patch-local additions, e.g. a Time
    /// module of delay/reverb). So a patch = "this preset scene + these effects".
    /// Inline-chain patches (empty `preset`) are returned unchanged.
    pub fn resolve_patch(&self, patch: &RigPatch) -> RigPatch {
        if patch.preset.is_empty() {
            return patch.clone();
        }
        let Some(scene) = self.lookup_scene(&patch.preset, &patch.scene) else {
            tracing::warn!(
                preset = %patch.preset,
                scene = %patch.scene,
                "Library: patch references an unknown preset scene — leaving it unavailable"
            );
            return RigPatch {
                name: patch.name.clone(),
                preset: patch.preset.clone(),
                scene: patch.scene.clone(),
                chain: patch.chain.clone(),
                input_trim_db: patch.input_trim_db,
                output_trim_db: patch.output_trim_db,
            };
        };
        // Scene core (amp/cab) + the patch's own appended blocks (effects).
        let mut chain = scene.chain.clone();
        chain.extend(patch.chain.iter().cloned());
        RigPatch {
            name: patch.name.clone(),
            preset: String::new(),
            scene: String::new(),
            chain,
            input_trim_db: patch.input_trim_db + scene.input_trim_db,
            output_trim_db: patch.output_trim_db + scene.output_trim_db,
        }
    }

    /// Turn a song into a profile whose patches are its sections in order — so a
    /// song loads and its sections step through with the patch keys.
    pub fn song_as_profile(&self, song: &RigSong) -> RigProfile {
        let patches: Vec<RigPatch> = song
            .sections
            .iter()
            .map(|sec| self.resolve_section(sec))
            .collect();
        let default_patch = song.default_section.min(patches.len().saturating_sub(1));
        RigProfile {
            name: song.name.clone(),
            patches,
            default_patch,
            // One stack per section so a song steps through its sections on the
            // section footswitches in order.
            stacks: song
                .sections
                .iter()
                .map(|s| RigStack::new(&s.name, [s.name.clone()]))
                .collect(),
        }
    }

    /// Resolve a section into a (named-after-the-section) patch with an inline
    /// chain. Prefers the `profile`/`patch` reference; falls back to a direct
    /// `preset`/`scene`.
    fn resolve_section(&self, sec: &RigSection) -> RigPatch {
        // Profile + patch reference.
        if !sec.profile.is_empty() {
            if let Some(patch) = self.profile(&sec.profile).and_then(|pr| {
                pr.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&sec.patch))
            }) {
                let mut resolved = self.resolve_patch(patch);
                resolved.name = sec.name.clone();
                return resolved;
            }
            tracing::warn!(
                profile = %sec.profile,
                patch = %sec.patch,
                section = %sec.name,
                "Library: song section references an unknown profile patch"
            );
        }
        // Direct preset + scene reference.
        if !sec.preset.is_empty() {
            if let Some(scene) = self.lookup_scene(&sec.preset, &sec.scene) {
                return RigPatch {
                    name: sec.name.clone(),
                    preset: String::new(),
                    scene: String::new(),
                    chain: scene.chain.clone(),
                    input_trim_db: scene.input_trim_db,
                    output_trim_db: scene.output_trim_db,
                };
            }
            tracing::warn!(
                preset = %sec.preset,
                scene = %sec.scene,
                section = %sec.name,
                "Library: song section references an unknown preset scene"
            );
        }
        // Unresolved — empty chain, shows "unavailable" at load.
        RigPatch::new(&sec.name)
    }

    /// Find a scene by preset + scene name; an empty scene name uses the
    /// preset's default scene.
    fn lookup_scene(&self, preset: &str, scene: &str) -> Option<&RigScene> {
        let preset = self.preset(preset)?;
        if scene.is_empty() {
            preset.default_scene()
        } else {
            preset.scene(scene)
        }
    }
}

/// Build a single-scene patch (used for `preset_as_profile`), folding the
/// scene's trims into the patch.
fn scene_patch(name: &str, scene: &RigScene) -> RigPatch {
    RigPatch {
        name: name.to_string(),
        preset: String::new(),
        scene: String::new(),
        chain: scene.chain.clone(),
        input_trim_db: scene.input_trim_db,
        output_trim_db: scene.output_trim_db,
    }
}

/// All `*.styx` files directly under `dir` (non-recursive), sorted by path.
fn styx_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("styx") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jcm800() -> RigPreset {
        RigPreset {
            name: "Marshall JCM800".into(),
            vendor: "ML Sound Lab".into(),
            category: "Amp".into(),
            default_scene: 0,
            scenes: vec![
                RigScene::single("Clean", RigBlock::nam("mars-clean.nam")),
                RigScene {
                    name: "Lead".into(),
                    chain: vec![RigBlock::nam("mars-lead.nam")],
                    input_trim_db: 2.0,
                    output_trim_db: -1.0,
                },
            ],
        }
    }

    #[test]
    fn preset_scene_lookup_is_case_insensitive() {
        let p = jcm800();
        assert_eq!(p.scene("clean").unwrap().name, "Clean");
        assert_eq!(p.scene("LEAD").unwrap().chain[0].nam, "mars-lead.nam");
        assert!(p.scene("nope").is_none());
        assert_eq!(p.default_scene().unwrap().name, "Clean");
    }

    #[test]
    fn preset_as_profile_maps_scenes_to_patches() {
        let lib = Library::default();
        let prof = lib.preset_as_profile(&jcm800());
        assert_eq!(prof.name, "Marshall JCM800");
        assert_eq!(prof.patches.len(), 2);
        assert_eq!(prof.patches[0].name, "Clean");
        assert_eq!(prof.patches[1].chain[0].nam, "mars-lead.nam");
        // Scene trims folded into the patch.
        assert_eq!(prof.patches[1].input_trim_db, 2.0);
        assert_eq!(prof.patches[1].output_trim_db, -1.0);
    }

    #[test]
    fn resolve_patch_fills_chain_from_preset_scene() {
        let mut lib = Library::default();
        lib.presets.push(jcm800());

        // A patch that references the preset + a specific scene, with its own trim.
        let mut patch = RigPatch::from_preset("Rhythm", "Marshall JCM800", "Lead");
        patch.output_trim_db = -2.0;

        let resolved = lib.resolve_patch(&patch);
        assert!(resolved.preset.is_empty(), "reference is consumed");
        assert_eq!(resolved.chain.len(), 1);
        assert_eq!(resolved.chain[0].nam, "mars-lead.nam");
        // Patch trim + scene trim are summed.
        assert_eq!(resolved.output_trim_db, -3.0);
        assert_eq!(resolved.input_trim_db, 2.0);
    }

    #[test]
    fn empty_scene_uses_preset_default() {
        let mut lib = Library::default();
        lib.presets.push(jcm800());
        let patch = RigPatch::from_preset("Base", "Marshall JCM800", "");
        let resolved = lib.resolve_patch(&patch);
        assert_eq!(resolved.chain[0].nam, "mars-clean.nam");
    }

    #[test]
    fn unknown_reference_yields_empty_chain() {
        let lib = Library::default();
        let patch = RigPatch::from_preset("Ghost", "No Such Amp", "Lead");
        let resolved = lib.resolve_patch(&patch);
        assert!(resolved.chain.is_empty());
        // Reference preserved so the UI can show what's missing.
        assert_eq!(resolved.preset, "No Such Amp");
    }

    #[test]
    fn song_section_resolves_through_profile_patch() {
        let mut lib = Library::default();
        lib.presets.push(jcm800());
        lib.profiles.push(RigProfile {
            name: "Rock".into(),
            default_patch: 0,
            patches: vec![RigPatch::from_preset(
                "Lead Tone",
                "Marshall JCM800",
                "Lead",
            )],
            stacks: Vec::new(),
        });

        let song = RigSong {
            name: "Test".into(),
            artist: String::new(),
            default_section: 0,
            sections: vec![RigSection {
                name: "Solo".into(),
                profile: "Rock".into(),
                patch: "Lead Tone".into(),
                preset: String::new(),
                scene: String::new(),
            }],
        };

        let prof = lib.song_as_profile(&song);
        assert_eq!(prof.patches.len(), 1);
        // Section name is used, chain resolved from the profile's patch → preset.
        assert_eq!(prof.patches[0].name, "Solo");
        assert_eq!(prof.patches[0].chain[0].nam, "mars-lead.nam");
    }

    #[test]
    fn shipped_example_library_loads() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/library");
        let lib = Library::load_dir(&root);
        assert!(
            lib.errors.is_empty(),
            "library load errors: {:?}",
            lib.errors
        );
        // All 9 ML presets present, each with ≥ 3 scenes whose chains hold a NAM.
        assert_eq!(lib.presets.len(), 9, "expected 9 ML presets");
        let jcm = lib.preset("Marshall JCM800").expect("JCM800 preset");
        assert_eq!(jcm.scenes.len(), 3);
        assert!(jcm.scene("Clean").unwrap().chain[0].is_nam());
        assert_eq!(
            jcm.scene("Clean").unwrap().chain[0].block_type,
            signal_proto::block::BlockType::Amp
        );
    }

    #[test]
    fn shipped_worship_profile_resolves_and_stacks_are_valid() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/library");
        let lib = Library::load_dir(&root);
        assert!(
            lib.errors.is_empty(),
            "library load errors: {:?}",
            lib.errors
        );

        let worship = lib.profile("Worship").expect("Worship profile");
        assert_eq!(worship.stacks.len(), 8, "worship wants 8 stacks");

        // Every stack patch name must exist in the profile's patch pool.
        for stack in &worship.stacks {
            assert!(!stack.patches.is_empty(), "stack {} is empty", stack.name);
            for pname in &stack.patches {
                assert!(
                    worship.patch_index(pname).is_some(),
                    "stack {} references unknown patch {pname:?}",
                    stack.name
                );
            }
        }

        // Resolving fills every preset-referencing patch with its amp + appended
        // effects. The amp (a NAM, role amp) must be the first block.
        let resolved = lib.resolve_profile(worship);
        let lead = resolved
            .patches
            .iter()
            .find(|p| p.name == "Lead")
            .expect("Lead patch");
        assert!(lead.chain[0].is_nam(), "Lead's first block is the amp NAM");
        assert!(
            lead.chain[0].nam.contains("FMAN") && lead.chain[0].nam.contains("Lead"),
            "Lead resolves to the Friedman BE Lead capture, got {:?}",
            lead.chain[0].nam
        );
        // The appended Time-module effects are Native blocks (no NAM/IR/plugin
        // asset) realized by the built-in delay/reverb DSP, flagged for the
        // global time-bypass.
        let time_fx: Vec<_> = lead.chain.iter().filter(|b| b.is_time_fx()).collect();
        assert_eq!(time_fx.len(), 2, "Lead has a 2-block Time module");
        assert!(time_fx.iter().all(|b| b.is_native() && b.has_backend()));

        // Funk has no Time module — it's the dry clean core.
        let funk = resolved.patches.iter().find(|p| p.name == "Funk").unwrap();
        assert!(funk.chain.iter().all(|b| !b.is_time_fx()));
    }

    #[test]
    fn shipped_example_songs_resolve_through_worship() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/library");
        let lib = Library::load_dir(&root);
        assert!(
            lib.errors.is_empty(),
            "library load errors: {:?}",
            lib.errors
        );
        assert!(lib.songs.len() >= 2, "expected the example songs");

        let song = lib.song("Amazing Grace").expect("Amazing Grace");
        let prof = lib.song_as_profile(song);
        // One section → one patch → one stack, in order.
        assert_eq!(prof.patches.len(), song.sections.len());
        assert_eq!(prof.stacks.len(), song.sections.len());
        // Every section's patch resolved to a playable amp NAM (first block).
        for patch in &prof.patches {
            assert!(
                patch.chain.first().map(|b| b.is_nam()).unwrap_or(false),
                "section {:?} did not resolve to an amp",
                patch.name
            );
        }
    }

    #[test]
    fn song_section_resolves_direct_preset_scene() {
        let mut lib = Library::default();
        lib.presets.push(jcm800());
        let song = RigSong {
            name: "Direct".into(),
            artist: String::new(),
            default_section: 0,
            sections: vec![RigSection {
                name: "Intro".into(),
                profile: String::new(),
                patch: String::new(),
                preset: "Marshall JCM800".into(),
                scene: "Clean".into(),
            }],
        };
        let prof = lib.song_as_profile(&song);
        assert_eq!(prof.patches[0].name, "Intro");
        assert_eq!(prof.patches[0].chain[0].nam, "mars-clean.nam");
    }
}
