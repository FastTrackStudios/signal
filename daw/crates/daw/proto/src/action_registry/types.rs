//! Action registry data types — filters, sections, list responses,
//! execution results.

use facet::Facet;

/// Action list category used when enumerating REAPER's action list.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub enum ActionListFilter {
    All,
    Reaper,
    /// Extension, script, and custom actions.
    NonReaper,
    /// SWS/S&M actions.
    Sws,
    /// FastTrackStudio actions.
    Fts,
    /// Actions registered through this action registry instance.
    Registered,
}

/// Best-effort source classification for an action-list entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub enum ActionOrigin {
    Reaper,
    Sws,
    Fts,
    /// Any other named extension, script, or custom action.
    Extension,
}

/// REAPER action-list section selector.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub enum ActionSection {
    /// Main section, REAPER unique ID 0.
    Main,
    /// Main alt section, REAPER unique ID 100.
    MainAlt,
    /// MIDI editor section, REAPER unique ID 32060.
    MidiEditor,
    /// MIDI event list editor, REAPER unique ID 32061.
    MidiEventListEditor,
    /// MIDI inline editor, REAPER unique ID 32062.
    MidiInlineEditor,
    /// Media explorer, REAPER unique ID 32063.
    MediaExplorer,
    /// Any other known REAPER or extension section ID.
    Custom(u32),
}

impl ActionSection {
    pub fn unique_id(self) -> u32 {
        match self {
            ActionSection::Main => 0,
            ActionSection::MainAlt => 100,
            ActionSection::MidiEditor => 32060,
            ActionSection::MidiEventListEditor => 32061,
            ActionSection::MidiInlineEditor => 32062,
            ActionSection::MediaExplorer => 32063,
            ActionSection::Custom(id) => id,
        }
    }

    pub fn name(self) -> String {
        match self {
            ActionSection::Main => "main".to_string(),
            ActionSection::MainAlt => "main-alt".to_string(),
            ActionSection::MidiEditor => "midi-editor".to_string(),
            ActionSection::MidiEventListEditor => "midi-event-list-editor".to_string(),
            ActionSection::MidiInlineEditor => "midi-inline-editor".to_string(),
            ActionSection::MediaExplorer => "media-explorer".to_string(),
            ActionSection::Custom(id) => format!("section-{id}"),
        }
    }
}

/// Request parameters for action-list enumeration.
#[derive(Debug, Clone, Facet)]
pub struct ActionListRequest {
    pub filter: ActionListFilter,
    pub section: ActionSection,
    /// Optional case-insensitive search over description + command name.
    pub query: Option<String>,
    /// Optional max rows to return.
    pub limit: Option<u32>,
}

impl Default for ActionListRequest {
    fn default() -> Self {
        Self {
            filter: ActionListFilter::All,
            section: ActionSection::Main,
            query: None,
            limit: None,
        }
    }
}

/// One action from REAPER's main action list.
#[derive(Debug, Clone, Facet)]
pub struct ActionInfo {
    pub command_id: u32,
    pub section_id: u32,
    pub section_name: String,
    /// Named command identifier, if REAPER has one.
    pub command_name: Option<String>,
    /// Text shown in REAPER's action list.
    pub description: String,
    pub origin: ActionOrigin,
    /// Stable provider key (`reaper`, `fts`, `sws`, `reascript`,
    /// `extension`).
    pub provider: String,
    pub provider_tags: Vec<String>,
    /// True if this action was registered by the FastTrackStudio registry.
    pub registered_by_fts: bool,
    /// Stored toggle state for FastTrackStudio toggle actions.
    pub toggle_state: Option<bool>,
}

/// Response from action-list enumeration.
#[derive(Debug, Clone, Default, Facet)]
pub struct ActionListResponse {
    /// Total matching the filter/query before limit.
    pub total_count: u32,
    /// Returned rows after applying limit.
    pub actions: Vec<ActionInfo>,
}

/// Detailed result from executing a REAPER action.
#[derive(Debug, Clone, Facet)]
pub struct ActionExecutionResult {
    pub requested_action: String,
    /// Whether a command was resolved and dispatched.
    pub executed: bool,
    pub command_id: Option<u32>,
    pub command_name: Option<String>,
    pub description: Option<String>,
    pub origin: Option<ActionOrigin>,
    pub provider: Option<String>,
    pub provider_tags: Vec<String>,
    pub registered_by_fts: bool,
    /// Toggle state before dispatch (FastTrackStudio toggle actions).
    pub toggle_state_before: Option<bool>,
    /// Toggle state after dispatch.
    pub toggle_state_after: Option<bool>,
}
