//! Typed REAPER configuration structures.
//!
//! The top-level [`ReaperConfig`] struct groups all preferences into logical
//! categories matching REAPER's Preferences dialog. Each sub-struct maps
//! directly to `reaper.ini` keys under the `[REAPER]` section (unless noted).

pub mod appearance;
pub mod audio;
pub mod automation;
pub mod common;
pub mod editing;
pub mod general;
pub mod global_prefs;
pub mod media;
pub mod midi;
pub mod performance;
pub mod project_defaults;
pub mod project_save;
pub mod recording;
pub mod rendering;
pub mod video;

use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::{ConfigError, Result};

pub use appearance::Appearance;
pub use audio::{Audio, AudioCloseInactiveFlags, HardwareFadeFlags};
pub use automation::Automation;
pub use common::{
    AcidImportMode, AudioThreadPriority, AutoMuteMode, AutoSaveMode, AutomationMode, DiskReadMode,
    DiskWriteMode, EnvelopeTrimMode, FadeCurveType, GroupDisplayMode, HelpDisplay,
    InsertMasterTrackMode, ItemMixBehavior, ItemTimeBase, ItemVolumeHandlePosition,
    ModalWindowPosition, PanDisplayMode, PanLawTaper, PanMode, ResampleQuality, RippleMode,
    StartupProjectMode, TimeDisplayFormat, TrackMixBitDepth, VstBridgeMode, ZLayer,
};
pub use editing::Editing;
pub use general::{FxResizeFlags, General, NewProjectFlags, TooltipSettings, UndoFlags};
pub use global_prefs::{
    GlobalItemDefaults, GlobalMiscPrefs, GlobalMixerPrefs, GlobalPanPrefs, GlobalPitchStretchPrefs,
    GlobalPrefs, GlobalTrackDefaults,
};
pub use media::{Media, PeakCacheGenFlags};
pub use midi::Midi;
pub use performance::{FxDenormFlags, Performance};
pub use project_defaults::{
    MetronomeEnableFlags, ProjectBehaviorDefaults, ProjectDefaults, ProjectGridDefaults,
    ProjectMasterTrackDefaults, ProjectMetronomeDefaults, ProjectRenderDefaults,
    ProjectSmpteDefaults, ProjectTempoDefaults, ProjectTimeDisplayDefaults, ProjectVideoDefaults,
};
pub use project_save::ProjectSave;
pub use recording::Recording;
pub use rendering::Rendering;
pub use video::Video;

/// Complete typed representation of a `reaper.ini` file.
///
/// Fields use `Option` where the key may be absent (REAPER uses internal
/// defaults). When serializing, `None` values are omitted so the written
/// file only contains explicitly set preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReaperConfig {
    pub general: General,
    pub appearance: Appearance,
    pub audio: Audio,
    pub performance: Performance,
    pub editing: Editing,
    pub automation: Automation,
    pub media: Media,
    pub midi: Midi,
    pub project_save: ProjectSave,
    pub recording: Recording,
    pub rendering: Rendering,
    pub video: Video,

    /// Default project settings (written by "Save as Default Project Settings").
    pub project_defaults: ProjectDefaults,

    /// Global (non-project-specific) preference variables.
    pub global_prefs: GlobalPrefs,

    /// The raw INI file, preserved for round-tripping keys we don't
    /// have typed mappings for yet.
    #[serde(skip)]
    raw: Option<IniFile>,
}

impl ReaperConfig {
    /// Returns a new [`ReaperConfig`] with REAPER factory default values.
    ///
    /// All sub-struct fields are initialized via their own `factory_defaults()`
    /// methods. The `raw` INI is left empty (no round-trip data).
    pub fn factory_defaults() -> Self {
        Self {
            general: General::factory_defaults(),
            appearance: Appearance::factory_defaults(),
            audio: Audio::factory_defaults(),
            performance: Performance::factory_defaults(),
            editing: Editing::factory_defaults(),
            automation: Automation::factory_defaults(),
            media: Media::factory_defaults(),
            midi: Midi::factory_defaults(),
            project_save: ProjectSave::factory_defaults(),
            recording: Recording::factory_defaults(),
            rendering: Rendering::factory_defaults(),
            video: Video::factory_defaults(),
            project_defaults: ProjectDefaults::factory_defaults(),
            global_prefs: GlobalPrefs::factory_defaults(),
            raw: None,
        }
    }

    /// Parse a `reaper.ini` file into typed config.
    pub fn parse(text: &str) -> Result<Self> {
        let ini = IniFile::parse(text);
        Ok(Self {
            general: General::from_ini(&ini),
            appearance: Appearance::from_ini(&ini),
            audio: Audio::from_ini(&ini),
            performance: Performance::from_ini(&ini),
            editing: Editing::from_ini(&ini),
            automation: Automation::from_ini(&ini),
            media: Media::from_ini(&ini),
            midi: Midi::from_ini(&ini),
            project_save: ProjectSave::from_ini(&ini),
            recording: Recording::from_ini(&ini),
            rendering: Rendering::from_ini(&ini),
            video: Video::from_ini(&ini),
            project_defaults: ProjectDefaults::from_ini(&ini),
            global_prefs: GlobalPrefs::from_ini(&ini),
            raw: Some(ini),
        })
    }

    /// Parse from a file path.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Serialize back to INI text.
    ///
    /// If this config was parsed from an existing file, unrecognized keys
    /// are preserved. If created from scratch, only typed fields are written.
    pub fn serialize(&self) -> String {
        let mut ini = self
            .raw
            .clone()
            .unwrap_or_else(|| IniFile::parse("[REAPER]\n"));
        self.general.write_to(&mut ini);
        self.appearance.write_to(&mut ini);
        self.audio.write_to(&mut ini);
        self.performance.write_to(&mut ini);
        self.editing.write_to(&mut ini);
        self.automation.write_to(&mut ini);
        self.media.write_to(&mut ini);
        self.midi.write_to(&mut ini);
        self.project_save.write_to(&mut ini);
        self.recording.write_to(&mut ini);
        self.rendering.write_to(&mut ini);
        self.video.write_to(&mut ini);
        self.project_defaults.write_to(&mut ini);
        self.global_prefs.write_to(&mut ini);
        ini.serialize()
    }

    /// Write to a file path.
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        std::fs::write(path, self.serialize())?;
        Ok(())
    }

    /// Access the underlying raw INI for keys we don't have typed mappings for.
    pub fn raw(&self) -> Option<&IniFile> {
        self.raw.as_ref()
    }

    /// Get a raw value by key from the `[REAPER]` section.
    pub fn raw_get(&self, key: &str) -> Option<&str> {
        self.raw.as_ref().and_then(|ini| ini.get("REAPER", key))
    }
}
