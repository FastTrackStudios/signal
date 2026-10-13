//! Type definitions for REAPER mouse modifier configuration entries.

use serde::{Deserialize, Serialize};

/// The modifier key combination index used in `mm_N` entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModifierIndex(pub u8);

impl ModifierIndex {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(2);
    pub const SHIFT_CTRL: Self = Self(3);
    pub const ALT: Self = Self(4);
    pub const SHIFT_ALT: Self = Self(5);
    pub const CTRL_ALT: Self = Self(6);
    pub const SHIFT_CTRL_ALT: Self = Self(7);
    /// Win / Cmd key alone (undocumented, possibly mm_8).
    pub const WIN: Self = Self(8);
    pub const SHIFT_WIN: Self = Self(9);
    pub const CTRL_WIN: Self = Self(10);
    pub const SHIFT_CTRL_WIN: Self = Self(11);
    pub const ALT_WIN: Self = Self(12);
    pub const SHIFT_ALT_WIN: Self = Self(13);
    pub const CTRL_ALT_WIN: Self = Self(14);
    pub const SHIFT_CTRL_ALT_WIN: Self = Self(15);

    /// Human-readable description of this modifier combination.
    pub fn description(self) -> &'static str {
        match self.0 {
            0 => "Default",
            1 => "Shift",
            2 => "Ctrl",
            3 => "Shift+Ctrl",
            4 => "Alt",
            5 => "Shift+Alt",
            6 => "Ctrl+Alt",
            7 => "Shift+Ctrl+Alt",
            8 => "Win/Cmd",
            9 => "Shift+Win",
            10 => "Ctrl+Win",
            11 => "Shift+Ctrl+Win",
            12 => "Alt+Win",
            13 => "Shift+Alt+Win",
            14 => "Ctrl+Alt+Win",
            15 => "Shift+Ctrl+Alt+Win",
            _ => "Unknown",
        }
    }
}

impl std::fmt::Display for ModifierIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The action assigned to a modifier entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionValue {
    /// A built-in mouse-modifier behaviour code (written `<N> m`).
    MouseBehavior(i32),
    /// A REAPER numeric command ID (written `<N> c`).
    CommandId(i32),
    /// A named action command ID string starting with `_` (no suffix).
    Named(String),
}

impl ActionValue {
    /// Format as it appears in the file.
    pub fn to_ini_string(&self) -> String {
        match self {
            ActionValue::MouseBehavior(n) => format!("{n} m"),
            ActionValue::CommandId(n) => format!("{n} c"),
            ActionValue::Named(s) => s.clone(),
        }
    }
}

/// A single `mm_N=<value>` entry inside a mouse context section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseBinding {
    /// The modifier index (0–15).
    pub modifier: ModifierIndex,
    /// The action assigned to this modifier.
    pub action: ActionValue,
}

/// The well-known `MM_CTX_*` context names defined by REAPER.
///
/// Variants use clean, human-readable names. Use [`Context::to_reaper_string`] to
/// get the `MM_CTX_*` identifier written in REAPER's config files, and
/// [`Context::from_reaper_str`] to parse one.
///
/// Any unrecognised name is stored as `Other(String)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Context {
    // ── Arrange view ──────────────────────────────────────────────────────
    /// Arrange view – left drag override A (MM_CTX_ARRANGE_A)
    ArrangeA,
    /// Arrange view – left drag override B (MM_CTX_ARRANGE_B)
    ArrangeB,
    /// Arrange view – left drag override C (MM_CTX_ARRANGE_C)
    ArrangeC,
    /// Arrange view – left drag override D (MM_CTX_ARRANGE_D)
    ArrangeD,
    /// Arrange view – right mouse drag (MM_CTX_ARRANGE_RMOUSE)
    ArrangeRightDrag,
    /// Arrange view – middle mouse drag (MM_CTX_ARRANGE_MMOUSE)
    ArrangeMiddleDrag,
    /// Arrange view – middle mouse click (MM_CTX_ARRANGE_MMOUSE_CLK)
    ArrangeMiddleClick,
    // ── Automation ────────────────────────────────────────────────────────
    /// Automation item – left drag (MM_CTX_POOLEDENV)
    AutomationItem,
    /// Automation item edge – left drag (MM_CTX_POOLEDENVEDGE)
    AutomationItemEdge,
    /// Automation item – double click (MM_CTX_POOLEDENV_DBLCLK)
    AutomationItemDoubleClick,
    // ── Edit cursor ───────────────────────────────────────────────────────
    /// Edit cursor handle – left drag (MM_CTX_CURSORHANDLE)
    EditCursor,
    // ── Envelope ──────────────────────────────────────────────────────────
    /// Envelope control panel – double click (MM_CTX_ENVCP_DBLCLK)
    EnvelopePanelDoubleClick,
    /// Envelope lane – left drag (MM_CTX_ENVLANE)
    EnvelopeLane,
    /// Envelope lane – double click (MM_CTX_ENVLANE_DBLCLK)
    EnvelopeLaneDoubleClick,
    /// Envelope point – left drag (MM_CTX_ENVPT)
    EnvelopePoint,
    /// Envelope point – double click (MM_CTX_ENVPT_DBLCLK)
    EnvelopePointDoubleClick,
    /// Envelope segment – left drag (MM_CTX_ENVSEG)
    EnvelopeSegment,
    /// Envelope segment – double click (MM_CTX_ENVSEG_DBLCLK)
    EnvelopeSegmentDoubleClick,
    // ── Fixed lane ────────────────────────────────────────────────────────
    /// Fixed lane header button – left click (MM_CTX_FIXEDLANETAB_CLK)
    FixedLaneHeaderClick,
    /// Fixed lane when comping is enabled – left drag (MM_CTX_LINKEDLANE)
    LinkedLane,
    /// Fixed lane comp area – left click (MM_CTX_LINKEDLANE_CLK)
    LinkedLaneClick,
    // ── MIDI editor ───────────────────────────────────────────────────────
    /// MIDI CC event – left click/drag (MM_CTX_MIDI_CCEVT)
    MidiCcEvent,
    /// MIDI CC event – double click (MM_CTX_MIDI_CCEVT_DBLCLK)
    MidiCcEventDoubleClick,
    /// MIDI CC lane – left click/drag (MM_CTX_MIDI_CCLANE)
    MidiCcLane,
    /// MIDI CC lane – double click (MM_CTX_MIDI_CCLANE_DBLCLK)
    MidiCcLaneDoubleClick,
    /// MIDI CC segment – left click/drag (MM_CTX_MIDI_CCSEG)
    MidiCcSegment,
    /// MIDI CC segment – double click (MM_CTX_MIDI_CCSEG_DBLCLK)
    MidiCcSegmentDoubleClick,
    /// MIDI source loop end marker – left drag (MM_CTX_MIDI_ENDPTR)
    MidiLoopEnd,
    /// MIDI marker/region lanes – left drag (MM_CTX_MIDI_MARKERLANES)
    MidiMarkerLanes,
    /// MIDI note – left drag (MM_CTX_MIDI_NOTE)
    MidiNote,
    /// MIDI note edge – left drag (MM_CTX_MIDI_NOTEEDGE)
    MidiNoteEdge,
    /// MIDI note – left click (MM_CTX_MIDI_NOTE_CLK)
    MidiNoteClick,
    /// MIDI note – double click (MM_CTX_MIDI_NOTE_DBLCLK)
    MidiNoteDoubleClick,
    /// MIDI piano roll – left drag (MM_CTX_MIDI_PIANOROLL)
    MidiPianoRoll,
    /// MIDI piano roll – left click (MM_CTX_MIDI_PIANOROLL_CLK)
    MidiPianoRollClick,
    /// MIDI piano roll – double click (MM_CTX_MIDI_PIANOROLL_DBLCLK)
    MidiPianoRollDoubleClick,
    /// MIDI editor – right mouse drag (MM_CTX_MIDI_RMOUSE)
    MidiRightDrag,
    /// MIDI ruler – left drag (MM_CTX_MIDI_RULER)
    MidiRuler,
    /// MIDI ruler – left click (MM_CTX_MIDI_RULER_CLK)
    MidiRulerClick,
    /// MIDI ruler – double click (MM_CTX_MIDI_RULER_DBLCLK)
    MidiRulerDoubleClick,
    // ── Media item ────────────────────────────────────────────────────────
    /// Media item – left drag (MM_CTX_ITEM)
    Item,
    /// Media item – left click (MM_CTX_ITEM_CLK)
    ItemClick,
    /// Media item – double click (MM_CTX_ITEM_DBLCLK)
    ItemDoubleClick,
    /// Media item edge – left drag (MM_CTX_ITEMEDGE)
    ItemEdge,
    /// Media item edge – double click (MM_CTX_ITEMEDGE_DBLCLK)
    ItemEdgeDoubleClick,
    /// Media item fade/autocrossfade – left drag (MM_CTX_ITEMFADE)
    ItemFade,
    /// Media item fade/autocrossfade – left click (MM_CTX_ITEMFADE_CLK)
    ItemFadeClick,
    /// Media item fade/autocrossfade – double click (MM_CTX_ITEMFADE_DBLCLK)
    ItemFadeDoubleClick,
    /// Media item bottom half – left drag (MM_CTX_ITEMLOWER)
    ItemLower,
    /// Media item bottom half – left click (MM_CTX_ITEMLOWER_CLK)
    ItemLowerClick,
    /// Media item bottom half – double click (MM_CTX_ITEMLOWER_DBLCLK)
    ItemLowerDoubleClick,
    /// Media item stretch marker – left drag (MM_CTX_ITEMSTRETCHMARKER)
    ItemStretchMarker,
    /// Media item stretch marker rate – left drag (MM_CTX_ITEMSTRETCHMARKERRATE)
    ItemStretchMarkerRate,
    /// Media item stretch marker – double click (MM_CTX_ITEMSTRETCHMARKER_DBLCLK)
    ItemStretchMarkerDoubleClick,
    /// Media item fade intersection – left drag (MM_CTX_CROSSFADE)
    Crossfade,
    /// Media item fade intersection – left click (MM_CTX_CROSSFADE_CLK)
    CrossfadeClick,
    /// Media item fade intersection – double click (MM_CTX_CROSSFADE_DBLCLK)
    CrossfadeDoubleClick,
    // ── Mixer ─────────────────────────────────────────────────────────────
    /// Mixer control panel – double click (MM_CTX_MCP_DBLCLK)
    MixerDoubleClick,
    // ── Project markers / regions ─────────────────────────────────────────
    /// Project marker/region lane – left drag (MM_CTX_MARKERLANES)
    MarkerLanes,
    /// Project marker/region edge – left drag (MM_CTX_MARKER_REGIONEDGE)
    MarkerRegionEdge,
    /// Project region – left drag (MM_CTX_REGION)
    Region,
    /// Project tempo/time signature marker – left drag (MM_CTX_TEMPOMARKER)
    TempoMarker,
    // ── Razor edit ────────────────────────────────────────────────────────
    /// Razor edit area – left drag (MM_CTX_AREASEL)
    RazorArea,
    /// Razor edit area – left click (MM_CTX_AREASEL_CLK)
    RazorAreaClick,
    /// Razor edit edge – left drag (MM_CTX_AREASEL_EDGE)
    RazorEdge,
    /// Razor edit envelope area – left drag (MM_CTX_AREASEL_ENV)
    RazorEnvelopeArea,
    // ── Ruler ─────────────────────────────────────────────────────────────
    /// Ruler – left drag (MM_CTX_RULER)
    Ruler,
    /// Ruler – left click (MM_CTX_RULER_CLK)
    RulerClick,
    /// Ruler – double click (MM_CTX_RULER_DBLCLK)
    RulerDoubleClick,
    // ── Track ─────────────────────────────────────────────────────────────
    /// Track – left drag (MM_CTX_TRACK)
    Track,
    /// Track – left click (MM_CTX_TRACK_CLK)
    TrackClick,
    /// Track control panel – double click (MM_CTX_TCP_DBLCLK)
    TcpDoubleClick,
    // ── Unknown ───────────────────────────────────────────────────────────
    /// Any other / future context name.
    Other(String),
}

/// Backward-compatible type alias.
pub type ContextKind = Context;

// Serialize as the canonical REAPER string.
impl Serialize for Context {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.to_reaper_string())
    }
}

impl<'de> Deserialize<'de> for Context {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Context::from_reaper_str(&s))
    }
}

impl Context {
    /// All well-known contexts used by reaper-input, in the canonical order from
    /// REAPER's mouse modifiers preference page. Does not include the four
    /// `ArrangeA/B/C/D` override contexts (which exist in the enum but are not
    /// exposed in the standard modifiers list).
    pub const ALL: &'static [Self] = &[
        Self::ArrangeMiddleDrag,
        Self::ArrangeMiddleClick,
        Self::ArrangeRightDrag,
        Self::AutomationItem,
        Self::AutomationItemEdge,
        Self::AutomationItemDoubleClick,
        Self::EditCursor,
        Self::EnvelopePanelDoubleClick,
        Self::EnvelopeLane,
        Self::EnvelopeLaneDoubleClick,
        Self::EnvelopePoint,
        Self::EnvelopePointDoubleClick,
        Self::EnvelopeSegment,
        Self::EnvelopeSegmentDoubleClick,
        Self::FixedLaneHeaderClick,
        Self::LinkedLane,
        Self::LinkedLaneClick,
        Self::MidiCcEvent,
        Self::MidiCcEventDoubleClick,
        Self::MidiCcLane,
        Self::MidiCcLaneDoubleClick,
        Self::MidiCcSegment,
        Self::MidiCcSegmentDoubleClick,
        Self::MidiLoopEnd,
        Self::MidiMarkerLanes,
        Self::MidiNote,
        Self::MidiNoteEdge,
        Self::MidiNoteClick,
        Self::MidiNoteDoubleClick,
        Self::MidiPianoRoll,
        Self::MidiPianoRollClick,
        Self::MidiPianoRollDoubleClick,
        Self::MidiRightDrag,
        Self::MidiRuler,
        Self::MidiRulerClick,
        Self::MidiRulerDoubleClick,
        Self::Item,
        Self::ItemClick,
        Self::ItemDoubleClick,
        Self::ItemEdge,
        Self::ItemEdgeDoubleClick,
        Self::ItemFade,
        Self::ItemFadeClick,
        Self::ItemFadeDoubleClick,
        Self::ItemLower,
        Self::ItemLowerClick,
        Self::ItemLowerDoubleClick,
        Self::ItemStretchMarker,
        Self::ItemStretchMarkerRate,
        Self::ItemStretchMarkerDoubleClick,
        Self::Crossfade,
        Self::CrossfadeClick,
        Self::CrossfadeDoubleClick,
        Self::MixerDoubleClick,
        Self::MarkerLanes,
        Self::MarkerRegionEdge,
        Self::Region,
        Self::TempoMarker,
        Self::RazorArea,
        Self::RazorAreaClick,
        Self::RazorEdge,
        Self::RazorEnvelopeArea,
        Self::Ruler,
        Self::RulerClick,
        Self::RulerDoubleClick,
        Self::Track,
        Self::TrackClick,
        Self::TcpDoubleClick,
    ];

    /// Parse a `MM_CTX_*` section name string into a `Context`.
    pub fn from_reaper_str(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "MM_CTX_ARRANGE_A" => Self::ArrangeA,
            "MM_CTX_ARRANGE_B" => Self::ArrangeB,
            "MM_CTX_ARRANGE_C" => Self::ArrangeC,
            "MM_CTX_ARRANGE_D" => Self::ArrangeD,
            "MM_CTX_ARRANGE_RMOUSE" => Self::ArrangeRightDrag,
            "MM_CTX_ARRANGE_MMOUSE" => Self::ArrangeMiddleDrag,
            "MM_CTX_ARRANGE_MMOUSE_CLK" => Self::ArrangeMiddleClick,
            "MM_CTX_POOLEDENV" => Self::AutomationItem,
            "MM_CTX_POOLEDENVEDGE" => Self::AutomationItemEdge,
            "MM_CTX_POOLEDENV_DBLCLK" => Self::AutomationItemDoubleClick,
            "MM_CTX_CURSORHANDLE" => Self::EditCursor,
            "MM_CTX_ENVCP_DBLCLK" => Self::EnvelopePanelDoubleClick,
            "MM_CTX_ENVLANE" => Self::EnvelopeLane,
            "MM_CTX_ENVLANE_DBLCLK" => Self::EnvelopeLaneDoubleClick,
            "MM_CTX_ENVPT" => Self::EnvelopePoint,
            "MM_CTX_ENVPT_DBLCLK" => Self::EnvelopePointDoubleClick,
            "MM_CTX_ENVSEG" => Self::EnvelopeSegment,
            "MM_CTX_ENVSEG_DBLCLK" => Self::EnvelopeSegmentDoubleClick,
            "MM_CTX_FIXEDLANETAB_CLK" => Self::FixedLaneHeaderClick,
            "MM_CTX_LINKEDLANE" => Self::LinkedLane,
            "MM_CTX_LINKEDLANE_CLK" => Self::LinkedLaneClick,
            "MM_CTX_MIDI_CCEVT" => Self::MidiCcEvent,
            "MM_CTX_MIDI_CCEVT_DBLCLK" => Self::MidiCcEventDoubleClick,
            "MM_CTX_MIDI_CCLANE" => Self::MidiCcLane,
            "MM_CTX_MIDI_CCLANE_DBLCLK" => Self::MidiCcLaneDoubleClick,
            "MM_CTX_MIDI_CCSEG" => Self::MidiCcSegment,
            "MM_CTX_MIDI_CCSEG_DBLCLK" => Self::MidiCcSegmentDoubleClick,
            "MM_CTX_MIDI_ENDPTR" => Self::MidiLoopEnd,
            "MM_CTX_MIDI_MARKERLANES" => Self::MidiMarkerLanes,
            "MM_CTX_MIDI_NOTE" => Self::MidiNote,
            "MM_CTX_MIDI_NOTEEDGE" => Self::MidiNoteEdge,
            "MM_CTX_MIDI_NOTE_CLK" => Self::MidiNoteClick,
            "MM_CTX_MIDI_NOTE_DBLCLK" => Self::MidiNoteDoubleClick,
            "MM_CTX_MIDI_PIANOROLL" => Self::MidiPianoRoll,
            "MM_CTX_MIDI_PIANOROLL_CLK" => Self::MidiPianoRollClick,
            "MM_CTX_MIDI_PIANOROLL_DBLCLK" => Self::MidiPianoRollDoubleClick,
            "MM_CTX_MIDI_RMOUSE" => Self::MidiRightDrag,
            "MM_CTX_MIDI_RULER" => Self::MidiRuler,
            "MM_CTX_MIDI_RULER_CLK" => Self::MidiRulerClick,
            "MM_CTX_MIDI_RULER_DBLCLK" => Self::MidiRulerDoubleClick,
            "MM_CTX_ITEM" => Self::Item,
            "MM_CTX_ITEM_CLK" => Self::ItemClick,
            "MM_CTX_ITEM_DBLCLK" => Self::ItemDoubleClick,
            "MM_CTX_ITEMEDGE" => Self::ItemEdge,
            "MM_CTX_ITEMEDGE_DBLCLK" => Self::ItemEdgeDoubleClick,
            "MM_CTX_ITEMFADE" => Self::ItemFade,
            "MM_CTX_ITEMFADE_CLK" => Self::ItemFadeClick,
            "MM_CTX_ITEMFADE_DBLCLK" => Self::ItemFadeDoubleClick,
            "MM_CTX_ITEMLOWER" => Self::ItemLower,
            "MM_CTX_ITEMLOWER_CLK" => Self::ItemLowerClick,
            "MM_CTX_ITEMLOWER_DBLCLK" => Self::ItemLowerDoubleClick,
            "MM_CTX_ITEMSTRETCHMARKER" => Self::ItemStretchMarker,
            "MM_CTX_ITEMSTRETCHMARKERRATE" => Self::ItemStretchMarkerRate,
            "MM_CTX_ITEMSTRETCHMARKER_DBLCLK" => Self::ItemStretchMarkerDoubleClick,
            "MM_CTX_CROSSFADE" => Self::Crossfade,
            "MM_CTX_CROSSFADE_CLK" => Self::CrossfadeClick,
            "MM_CTX_CROSSFADE_DBLCLK" => Self::CrossfadeDoubleClick,
            "MM_CTX_MCP_DBLCLK" => Self::MixerDoubleClick,
            "MM_CTX_MARKERLANES" => Self::MarkerLanes,
            "MM_CTX_MARKER_REGIONEDGE" => Self::MarkerRegionEdge,
            "MM_CTX_REGION" => Self::Region,
            "MM_CTX_TEMPOMARKER" => Self::TempoMarker,
            "MM_CTX_AREASEL" => Self::RazorArea,
            "MM_CTX_AREASEL_CLK" => Self::RazorAreaClick,
            "MM_CTX_AREASEL_EDGE" => Self::RazorEdge,
            "MM_CTX_AREASEL_ENV" => Self::RazorEnvelopeArea,
            "MM_CTX_RULER" => Self::Ruler,
            "MM_CTX_RULER_CLK" => Self::RulerClick,
            "MM_CTX_RULER_DBLCLK" => Self::RulerDoubleClick,
            "MM_CTX_TRACK" => Self::Track,
            "MM_CTX_TRACK_CLK" => Self::TrackClick,
            "MM_CTX_TCP_DBLCLK" => Self::TcpDoubleClick,
            other => Self::Other(other.to_string()),
        }
    }

    /// The `MM_CTX_*` identifier as written in REAPER's config files.
    pub fn to_reaper_string(&self) -> &str {
        match self {
            Self::ArrangeA => "MM_CTX_ARRANGE_A",
            Self::ArrangeB => "MM_CTX_ARRANGE_B",
            Self::ArrangeC => "MM_CTX_ARRANGE_C",
            Self::ArrangeD => "MM_CTX_ARRANGE_D",
            Self::ArrangeRightDrag => "MM_CTX_ARRANGE_RMOUSE",
            Self::ArrangeMiddleDrag => "MM_CTX_ARRANGE_MMOUSE",
            Self::ArrangeMiddleClick => "MM_CTX_ARRANGE_MMOUSE_CLK",
            Self::AutomationItem => "MM_CTX_POOLEDENV",
            Self::AutomationItemEdge => "MM_CTX_POOLEDENVEDGE",
            Self::AutomationItemDoubleClick => "MM_CTX_POOLEDENV_DBLCLK",
            Self::EditCursor => "MM_CTX_CURSORHANDLE",
            Self::EnvelopePanelDoubleClick => "MM_CTX_ENVCP_DBLCLK",
            Self::EnvelopeLane => "MM_CTX_ENVLANE",
            Self::EnvelopeLaneDoubleClick => "MM_CTX_ENVLANE_DBLCLK",
            Self::EnvelopePoint => "MM_CTX_ENVPT",
            Self::EnvelopePointDoubleClick => "MM_CTX_ENVPT_DBLCLK",
            Self::EnvelopeSegment => "MM_CTX_ENVSEG",
            Self::EnvelopeSegmentDoubleClick => "MM_CTX_ENVSEG_DBLCLK",
            Self::FixedLaneHeaderClick => "MM_CTX_FIXEDLANETAB_CLK",
            Self::LinkedLane => "MM_CTX_LINKEDLANE",
            Self::LinkedLaneClick => "MM_CTX_LINKEDLANE_CLK",
            Self::MidiCcEvent => "MM_CTX_MIDI_CCEVT",
            Self::MidiCcEventDoubleClick => "MM_CTX_MIDI_CCEVT_DBLCLK",
            Self::MidiCcLane => "MM_CTX_MIDI_CCLANE",
            Self::MidiCcLaneDoubleClick => "MM_CTX_MIDI_CCLANE_DBLCLK",
            Self::MidiCcSegment => "MM_CTX_MIDI_CCSEG",
            Self::MidiCcSegmentDoubleClick => "MM_CTX_MIDI_CCSEG_DBLCLK",
            Self::MidiLoopEnd => "MM_CTX_MIDI_ENDPTR",
            Self::MidiMarkerLanes => "MM_CTX_MIDI_MARKERLANES",
            Self::MidiNote => "MM_CTX_MIDI_NOTE",
            Self::MidiNoteEdge => "MM_CTX_MIDI_NOTEEDGE",
            Self::MidiNoteClick => "MM_CTX_MIDI_NOTE_CLK",
            Self::MidiNoteDoubleClick => "MM_CTX_MIDI_NOTE_DBLCLK",
            Self::MidiPianoRoll => "MM_CTX_MIDI_PIANOROLL",
            Self::MidiPianoRollClick => "MM_CTX_MIDI_PIANOROLL_CLK",
            Self::MidiPianoRollDoubleClick => "MM_CTX_MIDI_PIANOROLL_DBLCLK",
            Self::MidiRightDrag => "MM_CTX_MIDI_RMOUSE",
            Self::MidiRuler => "MM_CTX_MIDI_RULER",
            Self::MidiRulerClick => "MM_CTX_MIDI_RULER_CLK",
            Self::MidiRulerDoubleClick => "MM_CTX_MIDI_RULER_DBLCLK",
            Self::Item => "MM_CTX_ITEM",
            Self::ItemClick => "MM_CTX_ITEM_CLK",
            Self::ItemDoubleClick => "MM_CTX_ITEM_DBLCLK",
            Self::ItemEdge => "MM_CTX_ITEMEDGE",
            Self::ItemEdgeDoubleClick => "MM_CTX_ITEMEDGE_DBLCLK",
            Self::ItemFade => "MM_CTX_ITEMFADE",
            Self::ItemFadeClick => "MM_CTX_ITEMFADE_CLK",
            Self::ItemFadeDoubleClick => "MM_CTX_ITEMFADE_DBLCLK",
            Self::ItemLower => "MM_CTX_ITEMLOWER",
            Self::ItemLowerClick => "MM_CTX_ITEMLOWER_CLK",
            Self::ItemLowerDoubleClick => "MM_CTX_ITEMLOWER_DBLCLK",
            Self::ItemStretchMarker => "MM_CTX_ITEMSTRETCHMARKER",
            Self::ItemStretchMarkerRate => "MM_CTX_ITEMSTRETCHMARKERRATE",
            Self::ItemStretchMarkerDoubleClick => "MM_CTX_ITEMSTRETCHMARKER_DBLCLK",
            Self::Crossfade => "MM_CTX_CROSSFADE",
            Self::CrossfadeClick => "MM_CTX_CROSSFADE_CLK",
            Self::CrossfadeDoubleClick => "MM_CTX_CROSSFADE_DBLCLK",
            Self::MixerDoubleClick => "MM_CTX_MCP_DBLCLK",
            Self::MarkerLanes => "MM_CTX_MARKERLANES",
            Self::MarkerRegionEdge => "MM_CTX_MARKER_REGIONEDGE",
            Self::Region => "MM_CTX_REGION",
            Self::TempoMarker => "MM_CTX_TEMPOMARKER",
            Self::RazorArea => "MM_CTX_AREASEL",
            Self::RazorAreaClick => "MM_CTX_AREASEL_CLK",
            Self::RazorEdge => "MM_CTX_AREASEL_EDGE",
            Self::RazorEnvelopeArea => "MM_CTX_AREASEL_ENV",
            Self::Ruler => "MM_CTX_RULER",
            Self::RulerClick => "MM_CTX_RULER_CLK",
            Self::RulerDoubleClick => "MM_CTX_RULER_DBLCLK",
            Self::Track => "MM_CTX_TRACK",
            Self::TrackClick => "MM_CTX_TRACK_CLK",
            Self::TcpDoubleClick => "MM_CTX_TCP_DBLCLK",
            Self::Other(s) => s.as_str(),
        }
    }

    /// The `MM_CTX_*` identifier as a `&'static str`.
    ///
    /// Identical to [`Self::to_reaper_string`] for all well-known variants. Panics
    /// if called on an `Other` variant — only use this when iterating
    /// [`Context::ALL`], which never contains `Other`.
    pub fn to_reaper_string_static(&self) -> &'static str {
        match self {
            Self::ArrangeA => "MM_CTX_ARRANGE_A",
            Self::ArrangeB => "MM_CTX_ARRANGE_B",
            Self::ArrangeC => "MM_CTX_ARRANGE_C",
            Self::ArrangeD => "MM_CTX_ARRANGE_D",
            Self::ArrangeRightDrag => "MM_CTX_ARRANGE_RMOUSE",
            Self::ArrangeMiddleDrag => "MM_CTX_ARRANGE_MMOUSE",
            Self::ArrangeMiddleClick => "MM_CTX_ARRANGE_MMOUSE_CLK",
            Self::AutomationItem => "MM_CTX_POOLEDENV",
            Self::AutomationItemEdge => "MM_CTX_POOLEDENVEDGE",
            Self::AutomationItemDoubleClick => "MM_CTX_POOLEDENV_DBLCLK",
            Self::EditCursor => "MM_CTX_CURSORHANDLE",
            Self::EnvelopePanelDoubleClick => "MM_CTX_ENVCP_DBLCLK",
            Self::EnvelopeLane => "MM_CTX_ENVLANE",
            Self::EnvelopeLaneDoubleClick => "MM_CTX_ENVLANE_DBLCLK",
            Self::EnvelopePoint => "MM_CTX_ENVPT",
            Self::EnvelopePointDoubleClick => "MM_CTX_ENVPT_DBLCLK",
            Self::EnvelopeSegment => "MM_CTX_ENVSEG",
            Self::EnvelopeSegmentDoubleClick => "MM_CTX_ENVSEG_DBLCLK",
            Self::FixedLaneHeaderClick => "MM_CTX_FIXEDLANETAB_CLK",
            Self::LinkedLane => "MM_CTX_LINKEDLANE",
            Self::LinkedLaneClick => "MM_CTX_LINKEDLANE_CLK",
            Self::MidiCcEvent => "MM_CTX_MIDI_CCEVT",
            Self::MidiCcEventDoubleClick => "MM_CTX_MIDI_CCEVT_DBLCLK",
            Self::MidiCcLane => "MM_CTX_MIDI_CCLANE",
            Self::MidiCcLaneDoubleClick => "MM_CTX_MIDI_CCLANE_DBLCLK",
            Self::MidiCcSegment => "MM_CTX_MIDI_CCSEG",
            Self::MidiCcSegmentDoubleClick => "MM_CTX_MIDI_CCSEG_DBLCLK",
            Self::MidiLoopEnd => "MM_CTX_MIDI_ENDPTR",
            Self::MidiMarkerLanes => "MM_CTX_MIDI_MARKERLANES",
            Self::MidiNote => "MM_CTX_MIDI_NOTE",
            Self::MidiNoteEdge => "MM_CTX_MIDI_NOTEEDGE",
            Self::MidiNoteClick => "MM_CTX_MIDI_NOTE_CLK",
            Self::MidiNoteDoubleClick => "MM_CTX_MIDI_NOTE_DBLCLK",
            Self::MidiPianoRoll => "MM_CTX_MIDI_PIANOROLL",
            Self::MidiPianoRollClick => "MM_CTX_MIDI_PIANOROLL_CLK",
            Self::MidiPianoRollDoubleClick => "MM_CTX_MIDI_PIANOROLL_DBLCLK",
            Self::MidiRightDrag => "MM_CTX_MIDI_RMOUSE",
            Self::MidiRuler => "MM_CTX_MIDI_RULER",
            Self::MidiRulerClick => "MM_CTX_MIDI_RULER_CLK",
            Self::MidiRulerDoubleClick => "MM_CTX_MIDI_RULER_DBLCLK",
            Self::Item => "MM_CTX_ITEM",
            Self::ItemClick => "MM_CTX_ITEM_CLK",
            Self::ItemDoubleClick => "MM_CTX_ITEM_DBLCLK",
            Self::ItemEdge => "MM_CTX_ITEMEDGE",
            Self::ItemEdgeDoubleClick => "MM_CTX_ITEMEDGE_DBLCLK",
            Self::ItemFade => "MM_CTX_ITEMFADE",
            Self::ItemFadeClick => "MM_CTX_ITEMFADE_CLK",
            Self::ItemFadeDoubleClick => "MM_CTX_ITEMFADE_DBLCLK",
            Self::ItemLower => "MM_CTX_ITEMLOWER",
            Self::ItemLowerClick => "MM_CTX_ITEMLOWER_CLK",
            Self::ItemLowerDoubleClick => "MM_CTX_ITEMLOWER_DBLCLK",
            Self::ItemStretchMarker => "MM_CTX_ITEMSTRETCHMARKER",
            Self::ItemStretchMarkerRate => "MM_CTX_ITEMSTRETCHMARKERRATE",
            Self::ItemStretchMarkerDoubleClick => "MM_CTX_ITEMSTRETCHMARKER_DBLCLK",
            Self::Crossfade => "MM_CTX_CROSSFADE",
            Self::CrossfadeClick => "MM_CTX_CROSSFADE_CLK",
            Self::CrossfadeDoubleClick => "MM_CTX_CROSSFADE_DBLCLK",
            Self::MixerDoubleClick => "MM_CTX_MCP_DBLCLK",
            Self::MarkerLanes => "MM_CTX_MARKERLANES",
            Self::MarkerRegionEdge => "MM_CTX_MARKER_REGIONEDGE",
            Self::Region => "MM_CTX_REGION",
            Self::TempoMarker => "MM_CTX_TEMPOMARKER",
            Self::RazorArea => "MM_CTX_AREASEL",
            Self::RazorAreaClick => "MM_CTX_AREASEL_CLK",
            Self::RazorEdge => "MM_CTX_AREASEL_EDGE",
            Self::RazorEnvelopeArea => "MM_CTX_AREASEL_ENV",
            Self::Ruler => "MM_CTX_RULER",
            Self::RulerClick => "MM_CTX_RULER_CLK",
            Self::RulerDoubleClick => "MM_CTX_RULER_DBLCLK",
            Self::Track => "MM_CTX_TRACK",
            Self::TrackClick => "MM_CTX_TRACK_CLK",
            Self::TcpDoubleClick => "MM_CTX_TCP_DBLCLK",
            Self::Other(s) => panic!("to_reaper_string_static on Other({s})"),
        }
    }

    /// REAPER's compiled-in factory defaults for all 16 modifier slots of this
    /// context, as returned by `GetMouseModifier` on a clean install after
    /// "Reset all to factory default".
    ///
    /// The array index is the modifier combination (0–15, same as `mm_N` in the
    /// file). Each element is the behavior ID passed to `N m`. A value of `0`
    /// means "no action" for that slot.
    ///
    /// These values are **never written to disk** — `reaper-mouse.ini` only
    /// stores user overrides. Use this to know the effective binding when a
    /// slot is absent from the file.
    ///
    /// Captured from REAPER 7.x on Linux after "Reset all to factory default".
    pub fn factory_defaults(&self) -> [i32; 16] {
        match self {
            // ── Arrange view ──────────────────────────────────────────────
            Self::ArrangeMiddleDrag => [4, 2, 1, 4, 6, 8, 5, 4, 4, 4, 4, 4, 4, 4, 4, 4],
            Self::ArrangeMiddleClick => [3, 3, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3],
            Self::ArrangeRightDrag => [1, 7, 4, 34, 24, 26, 14, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ArrangeA => [1, 7, 6, 1, 2, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ArrangeB => [
                24, 25, 24, 24, 32, 33, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24,
            ],
            Self::ArrangeC => [
                34, 35, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
            ],
            Self::ArrangeD => [4, 5, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4],
            // ── Automation ────────────────────────────────────────────────
            Self::AutomationItem => [1, 2, 5, 6, 11, 1, 3, 4, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::AutomationItemEdge => [1, 2, 5, 6, 7, 8, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::AutomationItemDoubleClick => [1, 2, 3, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Edit cursor ───────────────────────────────────────────────
            Self::EditCursor => [2, 2, 1, 2, 4, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            // ── Envelope ──────────────────────────────────────────────────
            Self::EnvelopePanelDoubleClick => [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::EnvelopeLane => [0, 1, 3, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::EnvelopeLaneDoubleClick => [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::EnvelopePoint => [1, 2, 3, 6, 4, 1, 5, 9, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::EnvelopePointDoubleClick => [1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::EnvelopeSegment => [1, 2, 4, 7, 6, 8, 5, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::EnvelopeSegmentDoubleClick => [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            // ── Fixed lane ────────────────────────────────────────────────
            Self::FixedLaneHeaderClick => [3, 3, 5, 3, 6, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3],
            Self::LinkedLane => [1, 2, 15, 16, 7, 8, 1, 22, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::LinkedLaneClick => [5, 14, 5, 5, 9, 6, 3, 8, 5, 5, 5, 5, 5, 5, 5, 5],
            // ── MIDI editor ───────────────────────────────────────────────
            Self::MidiCcEvent => [1, 2, 18, 4, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiCcEventDoubleClick => [1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiCcLane => [2, 17, 1, 4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            Self::MidiCcLaneDoubleClick => [1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiCcSegment => [1, 2, 4, 6, 7, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiCcSegmentDoubleClick => [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::MidiLoopEnd => [1, 2, 1, 1, 3, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiMarkerLanes => [1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiNote => [1, 2, 7, 5, 9, 1, 1, 32, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiNoteEdge => [1, 2, 5, 6, 3, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiNoteClick => [2, 5, 4, 3, 1, 16, 12, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            Self::MidiNoteDoubleClick => [1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiPianoRoll => [1, 2, 27, 1, 3, 21, 23, 5, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiPianoRollClick => [1, 1, 2, 1, 3, 5, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiPianoRollDoubleClick => [1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiRightDrag => [1, 8, 7, 1, 2, 3, 11, 9, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiRuler => [1, 3, 2, 4, 5, 7, 6, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiRulerClick => [1, 3, 2, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MidiRulerDoubleClick => [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Media item ────────────────────────────────────────────────
            Self::Item => [13, 14, 2, 5, 3, 12, 7, 39, 13, 13, 13, 13, 13, 13, 13, 13],
            Self::ItemClick => [1, 6, 4, 2, 12, 19, 15, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ItemDoubleClick => [6, 2, 3, 6, 1, 7, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6],
            Self::ItemEdge => [9, 11, 5, 7, 10, 12, 6, 8, 9, 9, 9, 9, 9, 9, 9, 9],
            Self::ItemEdgeDoubleClick => [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::ItemFade => [1, 2, 5, 6, 3, 4, 9, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ItemFadeClick => [0, 0, 3, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::ItemFadeDoubleClick => [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            Self::ItemLower => [13, 14, 2, 5, 3, 12, 7, 39, 13, 13, 13, 13, 13, 13, 13, 13],
            Self::ItemLowerClick => [1, 6, 4, 2, 12, 19, 15, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ItemLowerDoubleClick => [6, 2, 3, 6, 1, 7, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6],
            Self::ItemStretchMarker => [1, 2, 3, 4, 5, 7, 6, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::ItemStretchMarkerRate => [7, 1, 3, 5, 8, 2, 4, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            Self::ItemStretchMarkerDoubleClick => [1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::Crossfade => [14, 1, 9, 2, 12, 4, 14, 5, 14, 14, 14, 14, 14, 14, 14, 14],
            Self::CrossfadeClick => [0, 0, 1, 2, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::CrossfadeDoubleClick => [1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Mixer ─────────────────────────────────────────────────────
            Self::MixerDoubleClick => [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            // ── Project markers / regions ─────────────────────────────────
            Self::MarkerLanes => [1, 13, 2, 1, 11, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::MarkerRegionEdge => [1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::Region => [1, 2, 3, 4, 5, 6, 7, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::TempoMarker => [1, 2, 3, 1, 1, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Razor edit ────────────────────────────────────────────────
            Self::RazorArea => [1, 2, 3, 4, 17, 18, 22, 23, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::RazorAreaClick => [0, 4, 0, 11, 1, 3, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0],
            Self::RazorEdge => [1, 2, 1, 1, 3, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::RazorEnvelopeArea => [1, 1, 2, 3, 1, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Ruler ─────────────────────────────────────────────────────
            Self::Ruler => [1, 3, 2, 4, 5, 7, 6, 8, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::RulerClick => [1, 4, 2, 5, 6, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::RulerDoubleClick => [1, 1, 2, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Track ─────────────────────────────────────────────────────
            Self::Track => [7, 14, 1, 2, 7, 7, 16, 17, 7, 7, 7, 7, 7, 7, 7, 7],
            Self::TrackClick => [1, 5, 2, 6, 3, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            Self::TcpDoubleClick => [1, 4, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            // ── Unknown ───────────────────────────────────────────────────
            Self::Other(_) => [0; 16],
        }
    }

    /// Human-readable display name for well-known contexts.
    ///
    /// Returns `None` for `Other` variants — callers should fall back to
    /// [`Context::to_reaper_string`] or the raw input string.
    pub fn display_name_static(&self) -> Option<&'static str> {
        Some(match self {
            Self::ArrangeA => "Arrange view override A (left drag)",
            Self::ArrangeB => "Arrange view override B (left drag)",
            Self::ArrangeC => "Arrange view override C (left drag)",
            Self::ArrangeD => "Arrange view override D (left drag)",
            Self::ArrangeRightDrag => "Arrange view (right drag)",
            Self::ArrangeMiddleDrag => "Arrange view (middle drag)",
            Self::ArrangeMiddleClick => "Arrange view (middle click)",
            Self::AutomationItem => "Automation item (left drag)",
            Self::AutomationItemEdge => "Automation item edge (left drag)",
            Self::AutomationItemDoubleClick => "Automation item (double click)",
            Self::EditCursor => "Edit cursor handle (left drag)",
            Self::EnvelopePanelDoubleClick => "Envelope control panel (double click)",
            Self::EnvelopeLane => "Envelope lane (left drag)",
            Self::EnvelopeLaneDoubleClick => "Envelope lane (double click)",
            Self::EnvelopePoint => "Envelope point (left drag)",
            Self::EnvelopePointDoubleClick => "Envelope point (double click)",
            Self::EnvelopeSegment => "Envelope segment (left drag)",
            Self::EnvelopeSegmentDoubleClick => "Envelope segment (double click)",
            Self::FixedLaneHeaderClick => "Fixed lane header button (left click)",
            Self::LinkedLane => "Fixed lane when comping is enabled (left drag)",
            Self::LinkedLaneClick => "Fixed lane comp area (left click)",
            Self::MidiCcEvent => "MIDI CC event (left click/drag)",
            Self::MidiCcEventDoubleClick => "MIDI CC event (double click)",
            Self::MidiCcLane => "MIDI CC lane (left click/drag)",
            Self::MidiCcLaneDoubleClick => "MIDI CC lane (double click)",
            Self::MidiCcSegment => "MIDI CC segment (left click/drag)",
            Self::MidiCcSegmentDoubleClick => "MIDI CC segment (double click)",
            Self::MidiLoopEnd => "MIDI source loop end marker (left drag)",
            Self::MidiMarkerLanes => "MIDI marker/region lanes (left drag)",
            Self::MidiNote => "MIDI note (left drag)",
            Self::MidiNoteEdge => "MIDI note edge (left drag)",
            Self::MidiNoteClick => "MIDI note (left click)",
            Self::MidiNoteDoubleClick => "MIDI note (double click)",
            Self::MidiPianoRoll => "MIDI piano roll (left drag)",
            Self::MidiPianoRollClick => "MIDI piano roll (left click)",
            Self::MidiPianoRollDoubleClick => "MIDI piano roll (double click)",
            Self::MidiRightDrag => "MIDI editor (right drag)",
            Self::MidiRuler => "MIDI ruler (left drag)",
            Self::MidiRulerClick => "MIDI ruler (left click)",
            Self::MidiRulerDoubleClick => "MIDI ruler (double click)",
            Self::Item => "Media item (left drag)",
            Self::ItemClick => "Media item (left click)",
            Self::ItemDoubleClick => "Media item (double click)",
            Self::ItemEdge => "Media item edge (left drag)",
            Self::ItemEdgeDoubleClick => "Media item edge (double click)",
            Self::ItemFade => "Media item fade/autocrossfade (left drag)",
            Self::ItemFadeClick => "Media item fade/autocrossfade (left click)",
            Self::ItemFadeDoubleClick => "Media item fade/autocrossfade (double click)",
            Self::ItemLower => "Media item bottom half (left drag)",
            Self::ItemLowerClick => "Media item bottom half (left click)",
            Self::ItemLowerDoubleClick => "Media item bottom half (double click)",
            Self::ItemStretchMarker => "Media item stretch marker (left drag)",
            Self::ItemStretchMarkerRate => "Media item stretch marker rate (left drag)",
            Self::ItemStretchMarkerDoubleClick => "Media item stretch marker (double click)",
            Self::Crossfade => "Media item fade intersection (left drag)",
            Self::CrossfadeClick => "Media item fade intersection (left click)",
            Self::CrossfadeDoubleClick => "Media item fade intersection (double click)",
            Self::MixerDoubleClick => "Mixer control panel (double click)",
            Self::MarkerLanes => "Project marker/region lane (left drag)",
            Self::MarkerRegionEdge => "Project marker/region edge (left drag)",
            Self::Region => "Project region (left drag)",
            Self::TempoMarker => "Project tempo/time signature marker (left drag)",
            Self::RazorArea => "Razor edit area (left drag)",
            Self::RazorAreaClick => "Razor edit area (left click)",
            Self::RazorEdge => "Razor edit edge (left drag)",
            Self::RazorEnvelopeArea => "Razor edit envelope area (left drag)",
            Self::Ruler => "Ruler (left drag)",
            Self::RulerClick => "Ruler (left click)",
            Self::RulerDoubleClick => "Ruler (double click)",
            Self::Track => "Track (left drag)",
            Self::TrackClick => "Track (left click)",
            Self::TcpDoubleClick => "Track control panel (double click)",
            Self::Other(_) => return None,
        })
    }
}

impl std::fmt::Display for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_reaper_string())
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

/// A complete `[MM_CTX_*]` section from the file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MouseContext {
    /// The context identifier.
    pub kind: Context,
    /// All `mm_N=…` bindings present in this section.
    pub bindings: Vec<MouseBinding>,
    /// Optional `name=` tooltip label.
    pub name: Option<String>,
}

impl MouseContext {
    /// Create an empty context for the given kind.
    pub fn new(kind: Context) -> Self {
        Self {
            kind,
            bindings: Vec::new(),
            name: None,
        }
    }

    /// Create a context pre-populated with REAPER's factory-default bindings
    /// (as returned by [`Context::factory_defaults`]).
    /// All 16 modifier slots are populated with their compiled-in defaults.
    pub fn with_factory_defaults(kind: Context) -> Self {
        let defaults = kind.factory_defaults();
        let bindings = defaults
            .iter()
            .enumerate()
            .map(|(i, &behavior)| MouseBinding {
                modifier: ModifierIndex(i as u8),
                action: ActionValue::MouseBehavior(behavior),
            })
            .collect();
        Self {
            kind,
            bindings,
            name: None,
        }
    }

    /// Return the binding for a specific modifier index, if present.
    pub fn binding_for(&self, modifier: ModifierIndex) -> Option<&MouseBinding> {
        self.bindings.iter().find(|b| b.modifier == modifier)
    }
}

/// A single entry in the `[hasimported]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HasImportedEntry {
    /// Context name key (e.g. `MM_CTX_ITEM`).
    pub key: String,
    /// Numeric value (typically 1 or -1).
    pub value: i32,
}

/// The complete contents of a `reaper-mouse.ini` or `.ReaperMouseMap` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseConfig {
    /// Entries from the `[hasimported]` section, if present.
    pub has_imported: Option<Vec<HasImportedEntry>>,
    /// All mouse-context sections, in the order they appear in the file.
    pub contexts: Vec<MouseContext>,
}

impl MouseConfig {
    /// Create an empty config without a `[hasimported]` section.
    pub fn new_mouse_map() -> Self {
        Self {
            has_imported: None,
            contexts: Vec::new(),
        }
    }

    /// Create an empty config with an empty `[hasimported]` section.
    pub fn new_mouse_ini() -> Self {
        Self {
            has_imported: Some(Vec::new()),
            contexts: Vec::new(),
        }
    }

    /// Find a context section by kind, returning the first match.
    pub fn context(&self, kind: &Context) -> Option<&MouseContext> {
        self.contexts.iter().find(|c| &c.kind == kind)
    }

    /// Find a context section by kind (mutable).
    pub fn context_mut(&mut self, kind: &Context) -> Option<&mut MouseContext> {
        self.contexts.iter_mut().find(|c| &c.kind == kind)
    }

    /// Return `true` if this config has a `[hasimported]` section.
    pub fn is_mouse_ini(&self) -> bool {
        self.has_imported.is_some()
    }
}

impl Default for MouseConfig {
    /// REAPER's factory-default mouse modifier state, matching a stock
    /// `reaper-mouse.ini` on a fresh REAPER 7.x install (Linux/macOS/Windows).
    ///
    /// The 22 contexts that REAPER marks as "imported" appear in `[hasimported]`.
    /// Of those, only `MM_CTX_MIDI_NOTE_CLK` carries explicit bindings; all other
    /// contexts rely on REAPER's compiled-in behavior and have no file entries.
    ///
    /// Use [`MouseConfig::new_mouse_map`] or [`MouseConfig::new_mouse_ini`] to
    /// start with a blank slate instead.
    fn default() -> Self {
        // The 22 contexts that appear in [hasimported] in a stock install,
        // in the order REAPER writes them.
        let has_imported = vec![
            HasImportedEntry {
                key: Context::ArrangeMiddleDrag.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::ArrangeMiddleClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::ArrangeRightDrag.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::EditCursor.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::EnvelopeLane.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::EnvelopePoint.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::EnvelopeSegment.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::EnvelopeSegmentDoubleClick
                    .to_reaper_string()
                    .into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::Item.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::ItemClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::ItemDoubleClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::ItemEdge.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiCcLane.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiNote.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiNoteClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiNoteDoubleClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiPianoRollClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiPianoRollDoubleClick.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiRightDrag.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::MidiRuler.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::Ruler.to_reaper_string().into(),
                value: 1,
            },
            HasImportedEntry {
                key: Context::Track.to_reaper_string().into(),
                value: 1,
            },
        ];

        // reaper-mouse.ini is empty on a fresh install — no explicit context
        // sections.  The factory defaults are compiled into REAPER and returned
        // by GetMouseModifier; use Context::factory_defaults() to query them.
        let contexts = Vec::new();

        Self {
            has_imported: Some(has_imported),
            contexts,
        }
    }
}
