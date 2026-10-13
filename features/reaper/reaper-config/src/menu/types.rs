//! Type definitions for REAPER menu/toolbar configuration.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// A complete `reaper-menu.ini` (or `.ReaperMenu` / `.ReaperMenuSet`) file.
///
/// The file is a sequence of INI sections, each describing one menu or toolbar.
/// Section order is preserved on round-trips.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MenuConfig {
    pub sections: Vec<MenuSection>,
}

// ---------------------------------------------------------------------------
// Section
// ---------------------------------------------------------------------------

/// One menu or toolbar section, headed by a `[Title]` line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuSection {
    /// The section identifier – either a well-known one or a raw string.
    pub id: MenuSectionId,
    /// Optional `title=` override (the user-visible display name inside REAPER).
    /// When absent the header name itself is used.
    pub title: Option<String>,
    /// The items that make up this menu or toolbar, in order.
    pub items: Vec<MenuItem>,
}

// ---------------------------------------------------------------------------
// Item
// ---------------------------------------------------------------------------

/// A single entry in a menu or toolbar section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MenuItem {
    /// A regular clickable action (`item_N=<cmd> <title>` or `item_N=<cmd>`).
    Action(ActionItem),
    /// A horizontal separator line (`item_N=-1`).
    Separator,
    /// Begin a sub-menu (`item_N=-2 <title>` for menus or `item_N=sub "<title>"`).
    SubMenuStart {
        /// The sub-menu label.
        title: String,
    },
    /// End a sub-menu (`item_N=-3` or `item_N=endsub`).
    SubMenuEnd,
    /// A non-clickable label (`item_N=-4 <text>`).
    Label { text: String },
}

/// A clickable action item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// The command identifier (numeric built-in or `_`-prefixed named action).
    pub command: CommandId,
    /// The label shown in the menu or as the toolbar button tooltip.
    /// May be absent for toolbar items that use only an icon.
    pub title: Option<String>,
    /// Toolbar-specific icon specification.  `None` for pure menu items.
    pub icon: Option<IconSpec>,
    /// Raw flags field present on some item lines (e.g. toolbar button flags).
    /// `0` or absent in most cases.
    pub flags: u32,
}

/// A command identifier: either a numeric built-in ID or a named action string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandId {
    /// Built-in numeric command ID (positive integer).
    Native(u32),
    /// Named action string (starts with `_`, e.g. `_SWS_SAVE`).
    Named(String),
}

impl CommandId {
    /// Parse a raw token as a `CommandId`.
    pub fn from_token(s: &str) -> Self {
        if s.starts_with('_') {
            CommandId::Named(s.to_string())
        } else {
            match s.parse::<u32>() {
                Ok(n) => CommandId::Native(n),
                Err(_) => CommandId::Named(s.to_string()),
            }
        }
    }

    /// Render the command ID back to its textual form.
    pub fn to_token(&self) -> String {
        match self {
            CommandId::Native(n) => n.to_string(),
            CommandId::Named(s) => s.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Icon spec (toolbar only)
// ---------------------------------------------------------------------------

/// How a toolbar button's icon should be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconSpec {
    /// A filename from REAPER's `toolbar_icons/` folder (e.g. `"play.png"`).
    File(String),
    /// Display the button's action title as a regular-width text label.
    Text,
    /// Display the button's action title as a double-width text label.
    TextWide,
    /// Display the button's action title as a tooltip-only text label.
    TextTooltip,
}

impl IconSpec {
    /// Parse an `icon_N=` value token.
    pub fn from_token(s: &str) -> Self {
        match s {
            "text" => IconSpec::Text,
            "text_wide" => IconSpec::TextWide,
            "text_tt" => IconSpec::TextTooltip,
            other => IconSpec::File(other.to_string()),
        }
    }

    /// Render back to the wire token.
    pub fn to_token(&self) -> &str {
        match self {
            IconSpec::Text => "text",
            IconSpec::TextWide => "text_wide",
            IconSpec::TextTooltip => "text_tt",
            IconSpec::File(f) => f.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Well-known section IDs
// ---------------------------------------------------------------------------

/// Well-known `[Section Header]` names used in `reaper-menu.ini`.
///
/// Any header not listed here is preserved as [`MenuSectionId::Other`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuSectionId {
    // ---- Main menu bar ----
    /// `[Main menu bar]` – the entire main menu bar (contains sub-sections).
    MainMenuBar,
    /// `[Main file]`
    MainFile,
    /// `[Main edit]`
    MainEdit,
    /// `[Main view]`
    MainView,
    /// `[Main insert]`
    MainInsert,
    /// `[Main item]`
    MainItem,
    /// `[Main track]`
    MainTrack,
    /// `[Main options]`
    MainOptions,
    /// `[Main actions]`
    MainActions,
    /// `[Main extensions]`
    MainExtensions,

    // ---- Main toolbar ----
    /// `[Main toolbar]`
    MainToolbar,

    // ---- Floating toolbars ----
    FloatingToolbar1,
    FloatingToolbar2,
    FloatingToolbar3,
    FloatingToolbar4,
    FloatingToolbar5,
    FloatingToolbar6,
    FloatingToolbar7,
    FloatingToolbar8,
    FloatingToolbar9,
    FloatingToolbar10,
    FloatingToolbar11,
    FloatingToolbar12,
    FloatingToolbar13,
    FloatingToolbar14,
    FloatingToolbar15,
    FloatingToolbar16,

    // ---- Context menus ----
    /// `[Ruler/arrange context]`
    RulerArrangeContext,
    /// `[Track control panel context]`
    TrackControlPanelContext,
    /// `[Media item context]`
    MediaItemContext,
    /// `[Envelope context]`
    EnvelopeContext,
    /// `[Mixer context]`
    MixerContext,
    /// `[Transport context]`
    TransportContext,

    // ---- MIDI editor menus ----
    /// `[MIDI editor menu bar]`
    MidiEditorMenuBar,
    /// `[MIDI main file]`
    MidiMainFile,
    /// `[MIDI main edit]`
    MidiMainEdit,
    /// `[MIDI main navigate]`
    MidiMainNavigate,
    /// `[MIDI main options]`
    MidiMainOptions,
    /// `[MIDI main view]`
    MidiMainView,
    /// `[MIDI main contents]`
    MidiMainContents,
    /// `[MIDI main actions]`
    MidiMainActions,

    // ---- MIDI editor context menus ----
    /// `[MIDI piano roll context]`
    MidiPianoRollContext,
    /// `[MIDI event list context]`
    MidiEventListContext,

    // ---- MIDI editor toolbars ----
    /// `[MIDI piano roll toolbar]`
    MidiPianoRollToolbar,
    /// `[MIDI event list toolbar]`
    MidiEventListToolbar,

    // ---- Media Explorer ----
    /// `[Media Explorer toolbar]`
    MediaExplorerToolbar,
    /// `[Media Explorer context]`
    MediaExplorerContext,

    // ---- Catch-all ----
    /// Any header not covered above.
    Other(String),
}

impl MenuSectionId {
    /// Parse the raw bracket-stripped header string into a `MenuSectionId`.
    pub fn from_header(s: &str) -> Self {
        match s {
            "Main menu bar" => MenuSectionId::MainMenuBar,
            "Main file" => MenuSectionId::MainFile,
            "Main edit" => MenuSectionId::MainEdit,
            "Main view" => MenuSectionId::MainView,
            "Main insert" => MenuSectionId::MainInsert,
            "Main item" => MenuSectionId::MainItem,
            "Main track" => MenuSectionId::MainTrack,
            "Main options" => MenuSectionId::MainOptions,
            "Main actions" => MenuSectionId::MainActions,
            "Main extensions" => MenuSectionId::MainExtensions,
            "Main toolbar" => MenuSectionId::MainToolbar,
            "Floating toolbar 1" => MenuSectionId::FloatingToolbar1,
            "Floating toolbar 2" => MenuSectionId::FloatingToolbar2,
            "Floating toolbar 3" => MenuSectionId::FloatingToolbar3,
            "Floating toolbar 4" => MenuSectionId::FloatingToolbar4,
            "Floating toolbar 5" => MenuSectionId::FloatingToolbar5,
            "Floating toolbar 6" => MenuSectionId::FloatingToolbar6,
            "Floating toolbar 7" => MenuSectionId::FloatingToolbar7,
            "Floating toolbar 8" => MenuSectionId::FloatingToolbar8,
            "Floating toolbar 9" => MenuSectionId::FloatingToolbar9,
            "Floating toolbar 10" => MenuSectionId::FloatingToolbar10,
            "Floating toolbar 11" => MenuSectionId::FloatingToolbar11,
            "Floating toolbar 12" => MenuSectionId::FloatingToolbar12,
            "Floating toolbar 13" => MenuSectionId::FloatingToolbar13,
            "Floating toolbar 14" => MenuSectionId::FloatingToolbar14,
            "Floating toolbar 15" => MenuSectionId::FloatingToolbar15,
            "Floating toolbar 16" => MenuSectionId::FloatingToolbar16,
            "Ruler/arrange context" => MenuSectionId::RulerArrangeContext,
            "Track control panel context" => MenuSectionId::TrackControlPanelContext,
            "Media item context" => MenuSectionId::MediaItemContext,
            "Envelope context" => MenuSectionId::EnvelopeContext,
            "Mixer context" => MenuSectionId::MixerContext,
            "Transport context" => MenuSectionId::TransportContext,
            "MIDI editor menu bar" => MenuSectionId::MidiEditorMenuBar,
            "MIDI main file" => MenuSectionId::MidiMainFile,
            "MIDI main edit" => MenuSectionId::MidiMainEdit,
            "MIDI main navigate" => MenuSectionId::MidiMainNavigate,
            "MIDI main options" => MenuSectionId::MidiMainOptions,
            "MIDI main view" => MenuSectionId::MidiMainView,
            "MIDI main contents" => MenuSectionId::MidiMainContents,
            "MIDI main actions" => MenuSectionId::MidiMainActions,
            "MIDI piano roll context" => MenuSectionId::MidiPianoRollContext,
            "MIDI event list context" => MenuSectionId::MidiEventListContext,
            "MIDI piano roll toolbar" => MenuSectionId::MidiPianoRollToolbar,
            "MIDI event list toolbar" => MenuSectionId::MidiEventListToolbar,
            "Media Explorer toolbar" => MenuSectionId::MediaExplorerToolbar,
            "Media Explorer context" => MenuSectionId::MediaExplorerContext,
            other => MenuSectionId::Other(other.to_string()),
        }
    }

    /// Render back to the header string (without brackets).
    pub fn to_header(&self) -> &str {
        match self {
            MenuSectionId::MainMenuBar => "Main menu bar",
            MenuSectionId::MainFile => "Main file",
            MenuSectionId::MainEdit => "Main edit",
            MenuSectionId::MainView => "Main view",
            MenuSectionId::MainInsert => "Main insert",
            MenuSectionId::MainItem => "Main item",
            MenuSectionId::MainTrack => "Main track",
            MenuSectionId::MainOptions => "Main options",
            MenuSectionId::MainActions => "Main actions",
            MenuSectionId::MainExtensions => "Main extensions",
            MenuSectionId::MainToolbar => "Main toolbar",
            MenuSectionId::FloatingToolbar1 => "Floating toolbar 1",
            MenuSectionId::FloatingToolbar2 => "Floating toolbar 2",
            MenuSectionId::FloatingToolbar3 => "Floating toolbar 3",
            MenuSectionId::FloatingToolbar4 => "Floating toolbar 4",
            MenuSectionId::FloatingToolbar5 => "Floating toolbar 5",
            MenuSectionId::FloatingToolbar6 => "Floating toolbar 6",
            MenuSectionId::FloatingToolbar7 => "Floating toolbar 7",
            MenuSectionId::FloatingToolbar8 => "Floating toolbar 8",
            MenuSectionId::FloatingToolbar9 => "Floating toolbar 9",
            MenuSectionId::FloatingToolbar10 => "Floating toolbar 10",
            MenuSectionId::FloatingToolbar11 => "Floating toolbar 11",
            MenuSectionId::FloatingToolbar12 => "Floating toolbar 12",
            MenuSectionId::FloatingToolbar13 => "Floating toolbar 13",
            MenuSectionId::FloatingToolbar14 => "Floating toolbar 14",
            MenuSectionId::FloatingToolbar15 => "Floating toolbar 15",
            MenuSectionId::FloatingToolbar16 => "Floating toolbar 16",
            MenuSectionId::RulerArrangeContext => "Ruler/arrange context",
            MenuSectionId::TrackControlPanelContext => "Track control panel context",
            MenuSectionId::MediaItemContext => "Media item context",
            MenuSectionId::EnvelopeContext => "Envelope context",
            MenuSectionId::MixerContext => "Mixer context",
            MenuSectionId::TransportContext => "Transport context",
            MenuSectionId::MidiEditorMenuBar => "MIDI editor menu bar",
            MenuSectionId::MidiMainFile => "MIDI main file",
            MenuSectionId::MidiMainEdit => "MIDI main edit",
            MenuSectionId::MidiMainNavigate => "MIDI main navigate",
            MenuSectionId::MidiMainOptions => "MIDI main options",
            MenuSectionId::MidiMainView => "MIDI main view",
            MenuSectionId::MidiMainContents => "MIDI main contents",
            MenuSectionId::MidiMainActions => "MIDI main actions",
            MenuSectionId::MidiPianoRollContext => "MIDI piano roll context",
            MenuSectionId::MidiEventListContext => "MIDI event list context",
            MenuSectionId::MidiPianoRollToolbar => "MIDI piano roll toolbar",
            MenuSectionId::MidiEventListToolbar => "MIDI event list toolbar",
            MenuSectionId::MediaExplorerToolbar => "Media Explorer toolbar",
            MenuSectionId::MediaExplorerContext => "Media Explorer context",
            MenuSectionId::Other(s) => s.as_str(),
        }
    }

    /// Returns `true` if this section ID represents a toolbar (as opposed to a menu).
    pub fn is_toolbar(&self) -> bool {
        matches!(
            self,
            MenuSectionId::MainToolbar
                | MenuSectionId::FloatingToolbar1
                | MenuSectionId::FloatingToolbar2
                | MenuSectionId::FloatingToolbar3
                | MenuSectionId::FloatingToolbar4
                | MenuSectionId::FloatingToolbar5
                | MenuSectionId::FloatingToolbar6
                | MenuSectionId::FloatingToolbar7
                | MenuSectionId::FloatingToolbar8
                | MenuSectionId::FloatingToolbar9
                | MenuSectionId::FloatingToolbar10
                | MenuSectionId::FloatingToolbar11
                | MenuSectionId::FloatingToolbar12
                | MenuSectionId::FloatingToolbar13
                | MenuSectionId::FloatingToolbar14
                | MenuSectionId::FloatingToolbar15
                | MenuSectionId::FloatingToolbar16
                | MenuSectionId::MidiPianoRollToolbar
                | MenuSectionId::MidiEventListToolbar
                | MenuSectionId::MediaExplorerToolbar
        )
    }
}
