//! The runtime bass library — everything the rig plays from, as plain styx
//! text files in one directory. Portable, git-trackable, and directly
//! editable: an LLM (or a human in a text editor) can add presets, point
//! them at new `.nam` captures / IR wavs, rename things — then hit reload.
//!
//! ```text
//! <config>/signal/bass/         (override: SIGNAL_BASS_DIR)
//!   presets.styx        BassLib — the preset pool ("Bass", "Synth Bass", …)
//!   midi.styx           BassMidiMapDef — program-change + footswitch CCs
//!   last-state.styx     BassLastState — active preset + trim (auto-saved)
//!   models/…            .nam captures the presets reference (user-supplied)
//!   irs/…               cabinet IR wavs
//! ```
//!
//! First run bootstraps the styx files from the in-repo default config
//! (`features/rigs/bass/default-config/`, embedded at compile time). Asset
//! paths in presets are stored relative to the bass dir (`models/…`,
//! `irs/…`) and resolve against it at load; absolute paths pass through.

use std::path::PathBuf;

use facet::Facet;

/// The library directory (`SIGNAL_BASS_DIR` overrides).
pub fn bass_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_BASS_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    signal_sampler::rig_prefs::signal_config_dir().join("bass")
}

/// One preset in the bass library — a complete tone the rig switches to.
/// The `kind` field is the Signal-Live extensibility hook: "Bass" and
/// "Synth Bass" are both `audio` presets of this one rig (same DI → chain →
/// out path, different chains); a sampled bass is `kind sample` — another
/// preset, not another rig.
#[derive(Clone, Debug, Facet)]
pub struct BassPresetDef {
    /// Display name (globally unique in the library).
    pub name: String,
    /// Preset kind: `audio` (DI → NAM → IR, the live path) or `sample`
    /// (sampled bass — declared but not wired yet).
    #[facet(default)]
    pub kind: String,
    /// Optional drive/pedal `.nam` capture before the amp (e.g. a synth-bass
    /// fuzz/filter capture). Empty = no drive block.
    #[facet(default)]
    pub drive_nam: String,
    /// The amp `.nam` capture. Empty = clean DI passthrough.
    #[facet(default)]
    pub nam: String,
    /// Optional cabinet impulse-response wav after the amp.
    #[facet(default)]
    pub ir: String,
    /// For `kind sample`: the sample-library spec (`library.styx` /
    /// `.signalpack`). Unused by `audio` presets.
    #[facet(default)]
    pub sample: String,
    /// Preset-level trim before the chain (dB).
    #[facet(default)]
    pub input_trim_db: f32,
    /// Preset-level trim after the chain (dB).
    #[facet(default)]
    pub output_trim_db: f32,
}

impl BassPresetDef {
    /// Is this the live DI path (vs the sampled stub)?
    pub fn is_audio(&self) -> bool {
        self.kind.is_empty() || self.kind.eq_ignore_ascii_case("audio")
    }
}

/// `presets.styx` — the preset pool.
#[derive(Clone, Debug, Facet)]
pub struct BassLib {
    pub presets: Vec<BassPresetDef>,
}

/// `midi.styx` — how hardware switches presets. Program change `n` selects
/// preset `n`; the prev/next CCs step through available presets
/// (footswitch taps, edge-detected on value > 0). `port` filters which MIDI
/// input feeds the rig (substring match; empty = omni, all inputs merged).
#[derive(Clone, Debug, Facet)]
pub struct BassMidiMapDef {
    #[facet(default)]
    pub program_change: bool,
    #[facet(default)]
    pub prev_cc: u32,
    #[facet(default)]
    pub next_cc: u32,
    #[facet(default)]
    pub port: String,
}

impl Default for BassMidiMapDef {
    fn default() -> Self {
        default_midi_map()
    }
}

/// Code-built default MIDI map (matches the embedded `midi.styx`).
pub fn default_midi_map() -> BassMidiMapDef {
    BassMidiMapDef { program_change: true, prev_cc: 101, next_cc: 102, port: String::new() }
}

/// `last-state.styx` — the rig's last-active position, flushed by the meter
/// pump on preset/trim changes and restored on the next open, so a crash
/// restart lands back on the same tone. Names are re-validated against the
/// (possibly hand-edited) library on restore.
#[derive(Clone, Debug, Default, Facet)]
pub struct BassLastState {
    /// Active preset by name; empty = none saved.
    #[facet(default)]
    pub active_preset: String,
    /// Master output trim (dB).
    #[facet(default)]
    pub master_trim_db: f32,
}

/// Everything loaded from the bass directory.
#[derive(Clone, Debug)]
pub struct BassLibrary {
    pub presets: Vec<BassPresetDef>,
    pub midi_map: BassMidiMapDef,
}

// The in-repo default config, embedded so installed binaries can seed a
// fresh machine without a checkout.
const DEFAULT_PRESETS: &str = include_str!("../default-config/presets.styx");
const DEFAULT_MIDI: &str = include_str!("../default-config/midi.styx");

/// Code-built default presets (fallback if the embedded text fails to
/// parse). "Bass DI" works out of the box (clean passthrough); "Bass" and
/// "Synth Bass" wait for user-supplied captures under `models/`.
fn default_presets() -> Vec<BassPresetDef> {
    let blank = |name: &str| BassPresetDef {
        name: name.to_string(),
        kind: String::new(),
        drive_nam: String::new(),
        nam: String::new(),
        ir: String::new(),
        sample: String::new(),
        input_trim_db: 0.0,
        output_trim_db: 0.0,
    };
    vec![
        blank("Bass DI"),
        BassPresetDef { nam: "models/Bass Amp.nam".into(), ir: "irs/Bass Cab.wav".into(), ..blank("Bass") },
        BassPresetDef {
            drive_nam: "models/Synth Bass Drive.nam".into(),
            nam: "models/Bass Amp.nam".into(),
            ..blank("Synth Bass")
        },
    ]
}

/// Resolve a bass-dir-relative asset path ("models/…", "irs/…") to
/// absolute; absolute paths and empties pass through.
fn resolve_asset(path: &mut String) {
    if !path.is_empty() && !std::path::Path::new(path.as_str()).is_absolute() {
        *path = bass_dir().join(path.as_str()).to_string_lossy().into_owned();
    }
}

/// Inverse of [`resolve_asset`] for saves: paths under the bass dir are
/// stored relative, so the on-disk library stays portable.
fn relativize_asset(path: &mut String) {
    if let Ok(rel) = std::path::Path::new(path.as_str()).strip_prefix(bass_dir()) {
        *path = rel.to_string_lossy().into_owned();
    }
}

fn resolve_preset(p: &mut BassPresetDef) {
    resolve_asset(&mut p.drive_nam);
    resolve_asset(&mut p.nam);
    resolve_asset(&mut p.ir);
    resolve_asset(&mut p.sample);
}

fn read<T: for<'a> Facet<'a>>(file: &str) -> Option<T> {
    let path = bass_dir().join(file);
    let text = std::fs::read_to_string(&path).ok()?;
    match facet_styx::from_str::<T>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("bass library: {file} failed to parse ({e}) — using defaults");
            None
        }
    }
}

fn write<T: for<'a> Facet<'a>>(file: &str, value: &T) {
    let dir = bass_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("bass library: cannot create {}: {e}", dir.display());
        return;
    }
    match facet_styx::to_string(value) {
        Ok(text) => {
            if let Err(e) = std::fs::write(dir.join(file), text) {
                tracing::warn!("bass library: write {file} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("bass library: serialize {file} failed: {e}"),
    }
}

/// Read `file`, seeding it from the embedded in-repo default text when
/// missing (written verbatim so the on-disk copy matches the repo
/// snapshot). Falls back to the code-built default if the embedded text
/// fails to parse.
fn read_or_seed<T: for<'a> Facet<'a>>(file: &str, seed: &str, fallback: impl FnOnce() -> T) -> T {
    if let Some(v) = read(file) {
        return v;
    }
    let dir = bass_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("bass library: cannot create {}: {e}", dir.display());
    } else if let Err(e) = std::fs::write(dir.join(file), seed) {
        tracing::warn!("bass library: seed {file} failed: {e}");
    } else {
        tracing::info!("bass library: seeded {file} from the in-repo default");
    }
    match facet_styx::from_str::<T>(seed) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("bass library: embedded default {file} failed to parse ({e})");
            let v = fallback();
            write(file, &v);
            v
        }
    }
}

impl BassLibrary {
    /// Load the library, bootstrapping any missing file from the embedded
    /// in-repo default config, so the directory is always complete.
    pub fn load_or_bootstrap() -> Self {
        let mut presets = read_or_seed::<BassLib>("presets.styx", DEFAULT_PRESETS, || BassLib {
            presets: default_presets(),
        })
        .presets;
        let midi_map = read_or_seed::<BassMidiMapDef>("midi.styx", DEFAULT_MIDI, default_midi_map);
        for p in &mut presets {
            resolve_preset(p);
        }
        Self { presets, midi_map }
    }

    /// Persist the preset pool (asset paths under the bass dir stored
    /// relative — the file stays portable).
    pub fn save_presets(presets: &[BassPresetDef]) {
        let mut presets = presets.to_vec();
        for p in &mut presets {
            relativize_asset(&mut p.drive_nam);
            relativize_asset(&mut p.nam);
            relativize_asset(&mut p.ir);
            relativize_asset(&mut p.sample);
        }
        write("presets.styx", &BassLib { presets });
    }

    pub fn save_midi_map(map: &BassMidiMapDef) {
        write("midi.styx", map);
    }

    pub fn save_last_state(state: &BassLastState) {
        write("last-state.styx", state);
    }

    /// `None` when the file is missing (fresh install) or unparsable.
    pub fn load_last_state() -> Option<BassLastState> {
        read("last-state.styx")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_defaults_parse() {
        let lib = facet_styx::from_str::<BassLib>(DEFAULT_PRESETS).expect("presets.styx parses");
        assert!(lib.presets.iter().any(|p| p.name == "Bass"));
        assert!(lib.presets.iter().any(|p| p.name == "Synth Bass"));
        assert!(lib.presets.iter().all(|p| p.is_audio() || !p.sample.is_empty() || p.kind.eq_ignore_ascii_case("sample")));
        let map = facet_styx::from_str::<BassMidiMapDef>(DEFAULT_MIDI).expect("midi.styx parses");
        assert!(map.program_change);
        assert_eq!((map.prev_cc, map.next_cc), (101, 102));
    }

    #[test]
    fn asset_paths_roundtrip_relative() {
        std::env::set_var("SIGNAL_BASS_DIR", "/tmp/fts-test-bass");
        let mut p = String::from("models/x.nam");
        super::resolve_asset(&mut p);
        assert_eq!(p, "/tmp/fts-test-bass/models/x.nam");
        super::relativize_asset(&mut p);
        assert_eq!(p, "models/x.nam");
        let mut abs = String::from("/elsewhere/y.nam");
        super::resolve_asset(&mut abs);
        super::relativize_asset(&mut abs);
        assert_eq!(abs, "/elsewhere/y.nam");
    }

    #[test]
    fn last_state_roundtrips() {
        std::env::set_var("SIGNAL_BASS_DIR", "/tmp/fts-test-bass");
        let state = BassLastState { active_preset: "Synth Bass".into(), master_trim_db: -3.0 };
        BassLibrary::save_last_state(&state);
        let back = BassLibrary::load_last_state().expect("last-state.styx roundtrip");
        assert_eq!(back.active_preset, "Synth Bass");
        assert_eq!(back.master_trim_db, -3.0);
    }
}
