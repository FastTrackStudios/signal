//! Type definitions for REAPER FX browser folder files.

use serde::{Deserialize, Serialize};

/// The plugin type for an item in a normal FX folder.
///
/// Type codes as stored in the INI file's `TypeN=` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// DirectX plugin (type code `0`).
    DX,
    /// JS plugin (type code `2`).
    JS,
    /// VST plugin — instruments, effects, MIDI (type code `3`).
    VST,
    /// Apple AU plugin (type code `5`).
    AU,
    /// Unknown type code, preserved as-is for round-trip fidelity.
    Unknown(u32),
}

impl PluginType {
    /// Convert a raw integer type code to a `PluginType`.
    pub fn from_raw(value: u32) -> Self {
        match value {
            0 => PluginType::DX,
            2 => PluginType::JS,
            3 => PluginType::VST,
            5 => PluginType::AU,
            other => PluginType::Unknown(other),
        }
    }

    /// Convert a `PluginType` back to its raw integer type code.
    pub fn to_raw(self) -> u32 {
        match self {
            PluginType::DX => 0,
            PluginType::JS => 2,
            PluginType::VST => 3,
            PluginType::AU => 5,
            PluginType::Unknown(v) => v,
        }
    }
}

/// A single plugin entry inside an FX folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FxFolderItem {
    /// A normal plugin entry with a type and registered name or absolute path.
    Plugin {
        /// The plugin type (DX, JS, VST, AU).
        plugin_type: PluginType,
        /// Registered plugin name or absolute file path.
        name: String,
    },
    /// A smart-filter entry (the folder is a "smart folder").
    ///
    /// The `filter` string uses REAPER's keyword filter syntax:
    /// `keyword [OR keyword OR NOT keyword ...]`
    SmartFilter {
        /// The filter expression string.
        filter: String,
    },
}

/// A single FX folder, as referenced by a `[FolderN]` section in the INI file.
///
/// The `name` and `folder_ref_id` come from the `[Folders]` section;
/// the `items` come from the corresponding `[FolderN]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxFolder {
    /// Display name of the folder, from `NameN=` in `[Folders]`.
    pub name: String,
    /// The folder section ID referenced by `IdN=` in `[Folders]`.
    /// This is the numeric suffix used in `[FolderID]` section headers.
    pub folder_ref_id: u32,
    /// The plugin entries belonging to this folder.
    pub items: Vec<FxFolderItem>,
}

/// The complete contents of a `reaper-fxfolders.ini` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxFolders {
    /// All FX folders in declaration order (as listed in `[Folders]`).
    pub folders: Vec<FxFolder>,
}
