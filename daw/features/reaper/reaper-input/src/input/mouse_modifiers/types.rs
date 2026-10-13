//! Type-safe mouse modifier representation
//!
//! Provides typesafe structures for representing mouse modifier assignments,
//! contexts, actions, and complete modifier maps.
//!
//! All types use enums for maximum type safety and database compatibility.

use crate::input::mouse_modifiers::contexts::get_display_name;
use crate::input::mouse_modifiers::core::MouseModifierFlag;
use facet::Facet;
use std::collections::HashMap;
use std::fmt;

/// Base mouse button input types that can be used across all contexts
/// Each context uses only the variants that are applicable to it
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum MouseButtonInput {
    /// Left drag (default action)
    LeftDrag,
    /// Left click
    Click,
    /// Double click
    DoubleClick,
    /// Right drag
    RightDrag,
    /// Middle drag
    MiddleDrag,
    /// Middle click
    MiddleClick,
}

impl fmt::Display for MouseButtonInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MouseButtonInput::LeftDrag => write!(f, "Left Drag"),
            MouseButtonInput::Click => write!(f, "Click"),
            MouseButtonInput::DoubleClick => write!(f, "Double Click"),
            MouseButtonInput::RightDrag => write!(f, "Right Drag"),
            MouseButtonInput::MiddleDrag => write!(f, "Middle Drag"),
            MouseButtonInput::MiddleClick => write!(f, "Middle Click"),
        }
    }
}

/// All possible mouse modifier contexts in REAPER
/// Each context is an enum with variants for different interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum MouseModifierContext {
    // Arrange view contexts
    ArrangeView(ArrangeViewInteraction),

    // Automation contexts
    AutomationItem(AutomationItemInteraction),

    // Edit cursor
    CursorHandle,

    // Envelope contexts
    Envelope(EnvelopeInteraction),

    // Fixed lane contexts
    FixedLane(FixedLaneInteraction),

    // MIDI contexts
    Midi(MidiInteraction),

    // Media item contexts
    MediaItem(MediaItemInteraction),

    // Mixer contexts
    Mixer(MixerInteraction),

    // Project marker/region contexts
    Project(ProjectInteraction),

    // Razor edit contexts
    RazorEdit(RazorEditInteraction),

    // Ruler contexts
    Ruler(RulerInteraction),

    // Track contexts
    Track(TrackInteraction),
}

/// Arrange view interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum ArrangeViewInteraction {
    Middle(MouseButtonInput), // MiddleDrag, MiddleClick
    Right(MouseButtonInput),  // RightDrag
}

/// Automation item interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum AutomationItemInteraction {
    Default(MouseButtonInput), // LeftDrag
    Edge,
    DoubleClick(MouseButtonInput), // DoubleClick
}

/// Envelope interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum EnvelopeInteraction {
    ControlPanel(MouseButtonInput), // DoubleClick
    Lane(MouseButtonInput),         // LeftDrag, DoubleClick
    Point(MouseButtonInput),        // LeftDrag, DoubleClick
    Segment(MouseButtonInput),      // LeftDrag, DoubleClick
}

/// Fixed lane interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum FixedLaneInteraction {
    HeaderButton(MouseButtonInput), // Click, DoubleClick
    LinkedLane(MouseButtonInput),   // LeftDrag, Click, DoubleClick
}

/// MIDI interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum MidiInteraction {
    CcEvent(MouseButtonInput),   // LeftDrag, DoubleClick
    CcLane(MouseButtonInput),    // LeftDrag, DoubleClick
    CcSegment(MouseButtonInput), // LeftDrag, DoubleClick
    EndPointer,
    MarkerLanes,
    Note(NoteInteraction),
    PianoRoll(MouseButtonInput), // LeftDrag, Click, DoubleClick
    Right(MouseButtonInput),     // RightDrag
    Ruler(MouseButtonInput),     // LeftDrag, Click, DoubleClick
}

/// Note interaction - has edge as a separate variant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum NoteInteraction {
    Default(MouseButtonInput), // LeftDrag, Click, DoubleClick
    Edge,
}

/// Media item interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum MediaItemInteraction {
    Default(MouseButtonInput), // LeftDrag, Click, DoubleClick
    Edge(EdgeInteraction),
    Fade(MouseButtonInput),  // LeftDrag, Click, DoubleClick
    Lower(MouseButtonInput), // LeftDrag, Click, DoubleClick
    StretchMarker(StretchMarkerInteraction),
    Crossfade(MouseButtonInput), // LeftDrag, Click, DoubleClick
}

/// Edge interaction - separate context for item edges
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum EdgeInteraction {
    Default(MouseButtonInput),     // LeftDrag
    DoubleClick(MouseButtonInput), // DoubleClick
}

/// Stretch marker interaction - has rate as a separate variant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum StretchMarkerInteraction {
    Default(MouseButtonInput), // LeftDrag
    Rate,
    DoubleClick(MouseButtonInput), // DoubleClick
}

/// Mixer interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum MixerInteraction {
    ControlPanel(MouseButtonInput), // DoubleClick
}

/// Project interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum ProjectInteraction {
    MarkerLanes,
    MarkerRegionEdge,
    Region,
    TempoMarker,
}

/// Razor edit interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum RazorEditInteraction {
    Area(MouseButtonInput), // LeftDrag, Click
    Edge,
    EnvelopeArea,
}

/// Ruler interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum RulerInteraction {
    Default(MouseButtonInput), // LeftDrag, Click, DoubleClick
}

/// Track interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum TrackInteraction {
    Default(MouseButtonInput),      // LeftDrag, Click, DoubleClick
    ControlPanel(MouseButtonInput), // DoubleClick
}

impl MouseModifierContext {
    /// Get the internal REAPER context string (e.g., "MM_CTX_ITEM")
    pub fn to_reaper_string(self) -> &'static str {
        match self {
            MouseModifierContext::ArrangeView(ArrangeViewInteraction::Middle(
                MouseButtonInput::MiddleDrag,
            )) => "MM_CTX_ARRANGE_MMOUSE",
            MouseModifierContext::ArrangeView(ArrangeViewInteraction::Middle(
                MouseButtonInput::MiddleClick,
            )) => "MM_CTX_ARRANGE_MMOUSE_CLK",
            MouseModifierContext::ArrangeView(ArrangeViewInteraction::Right(
                MouseButtonInput::RightDrag,
            )) => "MM_CTX_ARRANGE_RMOUSE",

            MouseModifierContext::AutomationItem(AutomationItemInteraction::Default(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_POOLEDENV",
            MouseModifierContext::AutomationItem(AutomationItemInteraction::Edge) => {
                "MM_CTX_POOLEDENVEDGE"
            }
            MouseModifierContext::AutomationItem(AutomationItemInteraction::DoubleClick(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_POOLEDENV_DBLCLK",

            MouseModifierContext::CursorHandle => "MM_CTX_CURSORHANDLE",

            MouseModifierContext::Envelope(EnvelopeInteraction::ControlPanel(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ENVCP_DBLCLK",
            MouseModifierContext::Envelope(EnvelopeInteraction::Lane(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ENVLANE",
            MouseModifierContext::Envelope(EnvelopeInteraction::Lane(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ENVLANE_DBLCLK",
            MouseModifierContext::Envelope(EnvelopeInteraction::Point(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ENVPT",
            MouseModifierContext::Envelope(EnvelopeInteraction::Point(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ENVPT_DBLCLK",
            MouseModifierContext::Envelope(EnvelopeInteraction::Segment(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ENVSEG",
            MouseModifierContext::Envelope(EnvelopeInteraction::Segment(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ENVSEG_DBLCLK",

            MouseModifierContext::FixedLane(FixedLaneInteraction::HeaderButton(
                MouseButtonInput::Click,
            )) => "MM_CTX_FIXEDLANETAB_CLK",
            MouseModifierContext::FixedLane(FixedLaneInteraction::HeaderButton(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_FIXEDLANETAB_DBLCLK",
            MouseModifierContext::FixedLane(FixedLaneInteraction::LinkedLane(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_LINKEDLANE",
            MouseModifierContext::FixedLane(FixedLaneInteraction::LinkedLane(
                MouseButtonInput::Click,
            )) => "MM_CTX_LINKEDLANE_CLK",
            MouseModifierContext::FixedLane(FixedLaneInteraction::LinkedLane(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_LINKEDLANE_DBLCLK",

            MouseModifierContext::Midi(MidiInteraction::CcEvent(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_MIDI_CCEVT"
            }
            MouseModifierContext::Midi(MidiInteraction::CcEvent(MouseButtonInput::DoubleClick)) => {
                "MM_CTX_MIDI_CCEVT_DBLCLK"
            }
            MouseModifierContext::Midi(MidiInteraction::CcLane(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_MIDI_CCLANE"
            }
            MouseModifierContext::Midi(MidiInteraction::CcLane(MouseButtonInput::DoubleClick)) => {
                "MM_CTX_MIDI_CCLANE_DBLCLK"
            }
            MouseModifierContext::Midi(MidiInteraction::CcSegment(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_MIDI_CCSEG"
            }
            MouseModifierContext::Midi(MidiInteraction::CcSegment(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_MIDI_CCSEG_DBLCLK",
            MouseModifierContext::Midi(MidiInteraction::EndPointer) => "MM_CTX_MIDI_ENDPTR",
            MouseModifierContext::Midi(MidiInteraction::MarkerLanes) => "MM_CTX_MIDI_MARKERLANES",
            MouseModifierContext::Midi(MidiInteraction::Note(NoteInteraction::Default(
                MouseButtonInput::LeftDrag,
            ))) => "MM_CTX_MIDI_NOTE",
            MouseModifierContext::Midi(MidiInteraction::Note(NoteInteraction::Edge)) => {
                "MM_CTX_MIDI_NOTEEDGE"
            }
            MouseModifierContext::Midi(MidiInteraction::Note(NoteInteraction::Default(
                MouseButtonInput::Click,
            ))) => "MM_CTX_MIDI_NOTE_CLK",
            MouseModifierContext::Midi(MidiInteraction::Note(NoteInteraction::Default(
                MouseButtonInput::DoubleClick,
            ))) => "MM_CTX_MIDI_NOTE_DBLCLK",
            MouseModifierContext::Midi(MidiInteraction::PianoRoll(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_MIDI_PIANOROLL"
            }
            MouseModifierContext::Midi(MidiInteraction::PianoRoll(MouseButtonInput::Click)) => {
                "MM_CTX_MIDI_PIANOROLL_CLK"
            }
            MouseModifierContext::Midi(MidiInteraction::PianoRoll(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_MIDI_PIANOROLL_DBLCLK",
            MouseModifierContext::Midi(MidiInteraction::Right(MouseButtonInput::RightDrag)) => {
                "MM_CTX_MIDI_RMOUSE"
            }
            MouseModifierContext::Midi(MidiInteraction::Ruler(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_MIDI_RULER"
            }
            MouseModifierContext::Midi(MidiInteraction::Ruler(MouseButtonInput::Click)) => {
                "MM_CTX_MIDI_RULER_CLK"
            }
            MouseModifierContext::Midi(MidiInteraction::Ruler(MouseButtonInput::DoubleClick)) => {
                "MM_CTX_MIDI_RULER_DBLCLK"
            }

            MouseModifierContext::MediaItem(MediaItemInteraction::Default(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ITEM",
            MouseModifierContext::MediaItem(MediaItemInteraction::Default(
                MouseButtonInput::Click,
            )) => "MM_CTX_ITEM_CLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Default(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ITEM_DBLCLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
                EdgeInteraction::Default(MouseButtonInput::LeftDrag),
            )) => "MM_CTX_ITEMEDGE",
            MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
                EdgeInteraction::DoubleClick(MouseButtonInput::DoubleClick),
            )) => "MM_CTX_ITEMEDGE_DBLCLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ITEMFADE",
            MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
                MouseButtonInput::Click,
            )) => "MM_CTX_ITEMFADE_CLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ITEMFADE_DBLCLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Lower(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_ITEMLOWER",
            MouseModifierContext::MediaItem(MediaItemInteraction::Lower(
                MouseButtonInput::Click,
            )) => "MM_CTX_ITEMLOWER_CLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Lower(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_ITEMLOWER_DBLCLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::StretchMarker(
                StretchMarkerInteraction::Default(MouseButtonInput::LeftDrag),
            )) => "MM_CTX_ITEMSTRETCHMARKER",
            MouseModifierContext::MediaItem(MediaItemInteraction::StretchMarker(
                StretchMarkerInteraction::Rate,
            )) => "MM_CTX_ITEMSTRETCHMARKERRATE",
            MouseModifierContext::MediaItem(MediaItemInteraction::StretchMarker(
                StretchMarkerInteraction::DoubleClick(MouseButtonInput::DoubleClick),
            )) => "MM_CTX_ITEMSTRETCHMARKER_DBLCLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Crossfade(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_CROSSFADE",
            MouseModifierContext::MediaItem(MediaItemInteraction::Crossfade(
                MouseButtonInput::Click,
            )) => "MM_CTX_CROSSFADE_CLK",
            MouseModifierContext::MediaItem(MediaItemInteraction::Crossfade(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_CROSSFADE_DBLCLK",

            MouseModifierContext::Mixer(MixerInteraction::ControlPanel(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_MCP_DBLCLK",

            MouseModifierContext::Project(ProjectInteraction::MarkerLanes) => "MM_CTX_MARKERLANES",
            MouseModifierContext::Project(ProjectInteraction::MarkerRegionEdge) => {
                "MM_CTX_MARKER_REGIONEDGE"
            }
            MouseModifierContext::Project(ProjectInteraction::Region) => "MM_CTX_REGION",
            MouseModifierContext::Project(ProjectInteraction::TempoMarker) => "MM_CTX_TEMPOMARKER",

            MouseModifierContext::RazorEdit(RazorEditInteraction::Area(
                MouseButtonInput::LeftDrag,
            )) => "MM_CTX_AREASEL",
            MouseModifierContext::RazorEdit(RazorEditInteraction::Area(
                MouseButtonInput::Click,
            )) => "MM_CTX_AREASEL_CLK",
            MouseModifierContext::RazorEdit(RazorEditInteraction::Edge) => "MM_CTX_AREASEL_EDGE",
            MouseModifierContext::RazorEdit(RazorEditInteraction::EnvelopeArea) => {
                "MM_CTX_AREASEL_ENV"
            }

            MouseModifierContext::Ruler(RulerInteraction::Default(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_RULER"
            }
            MouseModifierContext::Ruler(RulerInteraction::Default(MouseButtonInput::Click)) => {
                "MM_CTX_RULER_CLK"
            }
            MouseModifierContext::Ruler(RulerInteraction::Default(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_RULER_DBLCLK",

            MouseModifierContext::Track(TrackInteraction::Default(MouseButtonInput::LeftDrag)) => {
                "MM_CTX_TRACK"
            }
            MouseModifierContext::Track(TrackInteraction::Default(MouseButtonInput::Click)) => {
                "MM_CTX_TRACK_CLK"
            }
            MouseModifierContext::Track(TrackInteraction::Default(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_TRACK_DBLCLK",
            MouseModifierContext::Track(TrackInteraction::ControlPanel(
                MouseButtonInput::DoubleClick,
            )) => "MM_CTX_TCP_DBLCLK",

            _ => panic!("Invalid context/interaction combination: {:?}", self),
        }
    }

    /// Get the display name for this context
    pub fn display_name(&self) -> &'static str {
        get_display_name(self.to_reaper_string())
    }

    /// Parse from REAPER context string
    pub fn from_reaper_string(s: &str) -> Option<Self> {
        match s {
            "MM_CTX_ARRANGE_MMOUSE" => Some(MouseModifierContext::ArrangeView(
                ArrangeViewInteraction::Middle(MouseButtonInput::MiddleDrag),
            )),
            "MM_CTX_ARRANGE_MMOUSE_CLK" => Some(MouseModifierContext::ArrangeView(
                ArrangeViewInteraction::Middle(MouseButtonInput::MiddleClick),
            )),
            "MM_CTX_ARRANGE_RMOUSE" => Some(MouseModifierContext::ArrangeView(
                ArrangeViewInteraction::Right(MouseButtonInput::RightDrag),
            )),
            "MM_CTX_POOLEDENV" => Some(MouseModifierContext::AutomationItem(
                AutomationItemInteraction::Default(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_POOLEDENVEDGE" => Some(MouseModifierContext::AutomationItem(
                AutomationItemInteraction::Edge,
            )),
            "MM_CTX_POOLEDENV_DBLCLK" => Some(MouseModifierContext::AutomationItem(
                AutomationItemInteraction::DoubleClick(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_CURSORHANDLE" => Some(MouseModifierContext::CursorHandle),
            "MM_CTX_ENVCP_DBLCLK" => Some(MouseModifierContext::Envelope(
                EnvelopeInteraction::ControlPanel(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ENVLANE" => Some(MouseModifierContext::Envelope(EnvelopeInteraction::Lane(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_ENVLANE_DBLCLK" => Some(MouseModifierContext::Envelope(
                EnvelopeInteraction::Lane(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ENVPT" => Some(MouseModifierContext::Envelope(EnvelopeInteraction::Point(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_ENVPT_DBLCLK" => Some(MouseModifierContext::Envelope(
                EnvelopeInteraction::Point(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ENVSEG" => Some(MouseModifierContext::Envelope(
                EnvelopeInteraction::Segment(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_ENVSEG_DBLCLK" => Some(MouseModifierContext::Envelope(
                EnvelopeInteraction::Segment(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_FIXEDLANETAB_CLK" => Some(MouseModifierContext::FixedLane(
                FixedLaneInteraction::HeaderButton(MouseButtonInput::Click),
            )),
            "MM_CTX_FIXEDLANETAB_DBLCLK" => Some(MouseModifierContext::FixedLane(
                FixedLaneInteraction::HeaderButton(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_LINKEDLANE" => Some(MouseModifierContext::FixedLane(
                FixedLaneInteraction::LinkedLane(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_LINKEDLANE_CLK" => Some(MouseModifierContext::FixedLane(
                FixedLaneInteraction::LinkedLane(MouseButtonInput::Click),
            )),
            "MM_CTX_LINKEDLANE_DBLCLK" => Some(MouseModifierContext::FixedLane(
                FixedLaneInteraction::LinkedLane(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MIDI_CCEVT" => Some(MouseModifierContext::Midi(MidiInteraction::CcEvent(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_MIDI_CCEVT_DBLCLK" => Some(MouseModifierContext::Midi(
                MidiInteraction::CcEvent(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MIDI_CCLANE" => Some(MouseModifierContext::Midi(MidiInteraction::CcLane(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_MIDI_CCLANE_DBLCLK" => Some(MouseModifierContext::Midi(
                MidiInteraction::CcLane(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MIDI_CCSEG" => Some(MouseModifierContext::Midi(MidiInteraction::CcSegment(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_MIDI_CCSEG_DBLCLK" => Some(MouseModifierContext::Midi(
                MidiInteraction::CcSegment(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MIDI_ENDPTR" => Some(MouseModifierContext::Midi(MidiInteraction::EndPointer)),
            "MM_CTX_MIDI_MARKERLANES" => {
                Some(MouseModifierContext::Midi(MidiInteraction::MarkerLanes))
            }
            "MM_CTX_MIDI_NOTE" => Some(MouseModifierContext::Midi(MidiInteraction::Note(
                NoteInteraction::Default(MouseButtonInput::LeftDrag),
            ))),
            "MM_CTX_MIDI_NOTEEDGE" => Some(MouseModifierContext::Midi(MidiInteraction::Note(
                NoteInteraction::Edge,
            ))),
            "MM_CTX_MIDI_NOTE_CLK" => Some(MouseModifierContext::Midi(MidiInteraction::Note(
                NoteInteraction::Default(MouseButtonInput::Click),
            ))),
            "MM_CTX_MIDI_NOTE_DBLCLK" => Some(MouseModifierContext::Midi(MidiInteraction::Note(
                NoteInteraction::Default(MouseButtonInput::DoubleClick),
            ))),
            "MM_CTX_MIDI_PIANOROLL" => Some(MouseModifierContext::Midi(
                MidiInteraction::PianoRoll(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_MIDI_PIANOROLL_CLK" => Some(MouseModifierContext::Midi(
                MidiInteraction::PianoRoll(MouseButtonInput::Click),
            )),
            "MM_CTX_MIDI_PIANOROLL_DBLCLK" => Some(MouseModifierContext::Midi(
                MidiInteraction::PianoRoll(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MIDI_RMOUSE" => Some(MouseModifierContext::Midi(MidiInteraction::Right(
                MouseButtonInput::RightDrag,
            ))),
            "MM_CTX_MIDI_RULER" => Some(MouseModifierContext::Midi(MidiInteraction::Ruler(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_MIDI_RULER_CLK" => Some(MouseModifierContext::Midi(MidiInteraction::Ruler(
                MouseButtonInput::Click,
            ))),
            "MM_CTX_MIDI_RULER_DBLCLK" => Some(MouseModifierContext::Midi(MidiInteraction::Ruler(
                MouseButtonInput::DoubleClick,
            ))),
            "MM_CTX_ITEM" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Default(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_ITEM_CLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Default(MouseButtonInput::Click),
            )),
            "MM_CTX_ITEM_DBLCLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Default(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ITEMEDGE" => Some(MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
                EdgeInteraction::Default(MouseButtonInput::LeftDrag),
            ))),
            "MM_CTX_ITEMEDGE_DBLCLK" => {
                Some(MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
                    EdgeInteraction::DoubleClick(MouseButtonInput::DoubleClick),
                )))
            }
            "MM_CTX_ITEMFADE" => Some(MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_ITEMFADE_CLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Fade(MouseButtonInput::Click),
            )),
            "MM_CTX_ITEMFADE_DBLCLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Fade(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ITEMLOWER" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Lower(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_ITEMLOWER_CLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Lower(MouseButtonInput::Click),
            )),
            "MM_CTX_ITEMLOWER_DBLCLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Lower(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_ITEMSTRETCHMARKER" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::StretchMarker(StretchMarkerInteraction::Default(
                    MouseButtonInput::LeftDrag,
                )),
            )),
            "MM_CTX_ITEMSTRETCHMARKERRATE" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::StretchMarker(StretchMarkerInteraction::Rate),
            )),
            "MM_CTX_ITEMSTRETCHMARKER_DBLCLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::StretchMarker(StretchMarkerInteraction::DoubleClick(
                    MouseButtonInput::DoubleClick,
                )),
            )),
            "MM_CTX_CROSSFADE" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Crossfade(MouseButtonInput::LeftDrag),
            )),
            "MM_CTX_CROSSFADE_CLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Crossfade(MouseButtonInput::Click),
            )),
            "MM_CTX_CROSSFADE_DBLCLK" => Some(MouseModifierContext::MediaItem(
                MediaItemInteraction::Crossfade(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MCP_DBLCLK" => Some(MouseModifierContext::Mixer(
                MixerInteraction::ControlPanel(MouseButtonInput::DoubleClick),
            )),
            "MM_CTX_MARKERLANES" => Some(MouseModifierContext::Project(
                ProjectInteraction::MarkerLanes,
            )),
            "MM_CTX_MARKER_REGIONEDGE" => Some(MouseModifierContext::Project(
                ProjectInteraction::MarkerRegionEdge,
            )),
            "MM_CTX_REGION" => Some(MouseModifierContext::Project(ProjectInteraction::Region)),
            "MM_CTX_TEMPOMARKER" => Some(MouseModifierContext::Project(
                ProjectInteraction::TempoMarker,
            )),
            "MM_CTX_AREASEL" => Some(MouseModifierContext::RazorEdit(RazorEditInteraction::Area(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_AREASEL_CLK" => Some(MouseModifierContext::RazorEdit(
                RazorEditInteraction::Area(MouseButtonInput::Click),
            )),
            "MM_CTX_AREASEL_EDGE" => {
                Some(MouseModifierContext::RazorEdit(RazorEditInteraction::Edge))
            }
            "MM_CTX_AREASEL_ENV" => Some(MouseModifierContext::RazorEdit(
                RazorEditInteraction::EnvelopeArea,
            )),
            "MM_CTX_RULER" => Some(MouseModifierContext::Ruler(RulerInteraction::Default(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_RULER_CLK" => Some(MouseModifierContext::Ruler(RulerInteraction::Default(
                MouseButtonInput::Click,
            ))),
            "MM_CTX_RULER_DBLCLK" => Some(MouseModifierContext::Ruler(RulerInteraction::Default(
                MouseButtonInput::DoubleClick,
            ))),
            "MM_CTX_TRACK" => Some(MouseModifierContext::Track(TrackInteraction::Default(
                MouseButtonInput::LeftDrag,
            ))),
            "MM_CTX_TRACK_CLK" => Some(MouseModifierContext::Track(TrackInteraction::Default(
                MouseButtonInput::Click,
            ))),
            "MM_CTX_TRACK_DBLCLK" => Some(MouseModifierContext::Track(TrackInteraction::Default(
                MouseButtonInput::DoubleClick,
            ))),
            "MM_CTX_TCP_DBLCLK" => Some(MouseModifierContext::Track(
                TrackInteraction::ControlPanel(MouseButtonInput::DoubleClick),
            )),
            _ => None,
        }
    }

    /// Get all possible contexts
    pub fn all() -> Vec<MouseModifierContext> {
        let mut all = Vec::new();

        // Arrange view
        all.push(MouseModifierContext::ArrangeView(
            ArrangeViewInteraction::Middle(MouseButtonInput::MiddleDrag),
        ));
        all.push(MouseModifierContext::ArrangeView(
            ArrangeViewInteraction::Middle(MouseButtonInput::MiddleClick),
        ));
        all.push(MouseModifierContext::ArrangeView(
            ArrangeViewInteraction::Right(MouseButtonInput::RightDrag),
        ));

        // Automation
        all.push(MouseModifierContext::AutomationItem(
            AutomationItemInteraction::Default(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::AutomationItem(
            AutomationItemInteraction::Edge,
        ));
        all.push(MouseModifierContext::AutomationItem(
            AutomationItemInteraction::DoubleClick(MouseButtonInput::DoubleClick),
        ));

        // Cursor
        all.push(MouseModifierContext::CursorHandle);

        // Envelope
        all.push(MouseModifierContext::Envelope(
            EnvelopeInteraction::ControlPanel(MouseButtonInput::DoubleClick),
        ));
        all.push(MouseModifierContext::Envelope(EnvelopeInteraction::Lane(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Envelope(EnvelopeInteraction::Lane(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Envelope(EnvelopeInteraction::Point(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Envelope(EnvelopeInteraction::Point(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Envelope(
            EnvelopeInteraction::Segment(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::Envelope(
            EnvelopeInteraction::Segment(MouseButtonInput::DoubleClick),
        ));

        // Fixed lane
        all.push(MouseModifierContext::FixedLane(
            FixedLaneInteraction::HeaderButton(MouseButtonInput::Click),
        ));
        all.push(MouseModifierContext::FixedLane(
            FixedLaneInteraction::LinkedLane(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::FixedLane(
            FixedLaneInteraction::LinkedLane(MouseButtonInput::Click),
        ));

        // MIDI
        all.push(MouseModifierContext::Midi(MidiInteraction::CcEvent(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::CcEvent(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::CcLane(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::CcLane(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::CcSegment(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::CcSegment(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::EndPointer));
        all.push(MouseModifierContext::Midi(MidiInteraction::MarkerLanes));
        all.push(MouseModifierContext::Midi(MidiInteraction::Note(
            NoteInteraction::Default(MouseButtonInput::LeftDrag),
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Note(
            NoteInteraction::Edge,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Note(
            NoteInteraction::Default(MouseButtonInput::Click),
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Note(
            NoteInteraction::Default(MouseButtonInput::DoubleClick),
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::PianoRoll(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::PianoRoll(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::PianoRoll(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Right(
            MouseButtonInput::RightDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Ruler(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Ruler(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::Midi(MidiInteraction::Ruler(
            MouseButtonInput::DoubleClick,
        )));

        // Media item
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Default(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Default(MouseButtonInput::Click),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Default(MouseButtonInput::DoubleClick),
        ));
        all.push(MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
            EdgeInteraction::Default(MouseButtonInput::LeftDrag),
        )));
        all.push(MouseModifierContext::MediaItem(MediaItemInteraction::Edge(
            EdgeInteraction::DoubleClick(MouseButtonInput::DoubleClick),
        )));
        all.push(MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::MediaItem(MediaItemInteraction::Fade(
            MouseButtonInput::DoubleClick,
        )));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Lower(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Lower(MouseButtonInput::Click),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Lower(MouseButtonInput::DoubleClick),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::StretchMarker(StretchMarkerInteraction::Default(
                MouseButtonInput::LeftDrag,
            )),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::StretchMarker(StretchMarkerInteraction::Rate),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::StretchMarker(StretchMarkerInteraction::DoubleClick(
                MouseButtonInput::DoubleClick,
            )),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Crossfade(MouseButtonInput::LeftDrag),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Crossfade(MouseButtonInput::Click),
        ));
        all.push(MouseModifierContext::MediaItem(
            MediaItemInteraction::Crossfade(MouseButtonInput::DoubleClick),
        ));

        // Mixer
        all.push(MouseModifierContext::Mixer(MixerInteraction::ControlPanel(
            MouseButtonInput::DoubleClick,
        )));

        // Project
        all.push(MouseModifierContext::Project(
            ProjectInteraction::MarkerLanes,
        ));
        all.push(MouseModifierContext::Project(
            ProjectInteraction::MarkerRegionEdge,
        ));
        all.push(MouseModifierContext::Project(ProjectInteraction::Region));
        all.push(MouseModifierContext::Project(
            ProjectInteraction::TempoMarker,
        ));

        // Razor edit
        all.push(MouseModifierContext::RazorEdit(RazorEditInteraction::Area(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::RazorEdit(RazorEditInteraction::Area(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::RazorEdit(RazorEditInteraction::Edge));
        all.push(MouseModifierContext::RazorEdit(
            RazorEditInteraction::EnvelopeArea,
        ));

        // Ruler
        all.push(MouseModifierContext::Ruler(RulerInteraction::Default(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Ruler(RulerInteraction::Default(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::Ruler(RulerInteraction::Default(
            MouseButtonInput::DoubleClick,
        )));

        // Track
        all.push(MouseModifierContext::Track(TrackInteraction::Default(
            MouseButtonInput::LeftDrag,
        )));
        all.push(MouseModifierContext::Track(TrackInteraction::Default(
            MouseButtonInput::Click,
        )));
        all.push(MouseModifierContext::Track(TrackInteraction::ControlPanel(
            MouseButtonInput::DoubleClick,
        )));

        all
    }
}

/// All possible modifier flag combinations (0-15)
/// This enum provides type safety for modifier combinations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum ModifierFlag {
    /// No modifiers (default action)
    None = 0,
    /// Shift
    Shift = 1,
    /// Control/Cmd
    Control = 2,
    /// Shift + Control/Cmd
    ShiftControl = 3,
    /// Alt/Opt
    Alt = 4,
    /// Shift + Alt/Opt
    ShiftAlt = 5,
    /// Control/Cmd + Alt/Opt
    ControlAlt = 6,
    /// Shift + Control/Cmd + Alt/Opt
    ShiftControlAlt = 7,
    /// Win/Ctrl
    Win = 8,
    /// Shift + Win/Ctrl
    ShiftWin = 9,
    /// Control/Cmd + Win/Ctrl
    ControlWin = 10,
    /// Shift + Control/Cmd + Win/Ctrl
    ShiftControlWin = 11,
    /// Alt/Opt + Win/Ctrl
    AltWin = 12,
    /// Shift + Alt/Opt + Win/Ctrl
    ShiftAltWin = 13,
    /// Control/Cmd + Alt/Opt + Win/Ctrl
    ControlAltWin = 14,
    /// Shift + Control/Cmd + Alt/Opt + Win/Ctrl
    All = 15,
}

impl ModifierFlag {
    /// Convert to numeric flag value (0-15)
    pub fn to_flag(self) -> i32 {
        self as i32
    }

    /// Create from numeric flag value (0-15)
    pub fn from_flag(flag: i32) -> Option<Self> {
        match flag {
            0 => Some(ModifierFlag::None),
            1 => Some(ModifierFlag::Shift),
            2 => Some(ModifierFlag::Control),
            3 => Some(ModifierFlag::ShiftControl),
            4 => Some(ModifierFlag::Alt),
            5 => Some(ModifierFlag::ShiftAlt),
            6 => Some(ModifierFlag::ControlAlt),
            7 => Some(ModifierFlag::ShiftControlAlt),
            8 => Some(ModifierFlag::Win),
            9 => Some(ModifierFlag::ShiftWin),
            10 => Some(ModifierFlag::ControlWin),
            11 => Some(ModifierFlag::ShiftControlWin),
            12 => Some(ModifierFlag::AltWin),
            13 => Some(ModifierFlag::ShiftAltWin),
            14 => Some(ModifierFlag::ControlAltWin),
            15 => Some(ModifierFlag::All),
            _ => None,
        }
    }

    /// Convert from MouseModifierFlag struct
    pub fn from_mouse_modifier_flag(flag: &MouseModifierFlag) -> Self {
        Self::from_flag(flag.to_flag()).unwrap_or(ModifierFlag::None)
    }

    /// Convert to MouseModifierFlag struct
    pub fn to_mouse_modifier_flag(self) -> MouseModifierFlag {
        MouseModifierFlag::from_flag(self.to_flag())
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            ModifierFlag::None => "Default action",
            ModifierFlag::Shift => "Shift",
            ModifierFlag::Control => "Cmd",
            ModifierFlag::ShiftControl => "Shift+Cmd",
            ModifierFlag::Alt => "Opt",
            ModifierFlag::ShiftAlt => "Shift+Opt",
            ModifierFlag::ControlAlt => "Cmd+Opt",
            ModifierFlag::ShiftControlAlt => "Shift+Cmd+Opt",
            ModifierFlag::Win => "Ctrl",
            ModifierFlag::ShiftWin => "Shift+Ctrl",
            ModifierFlag::ControlWin => "Cmd+Ctrl",
            ModifierFlag::ShiftControlWin => "Shift+Cmd+Ctrl",
            ModifierFlag::AltWin => "Opt+Ctrl",
            ModifierFlag::ShiftAltWin => "Shift+Opt+Ctrl",
            ModifierFlag::ControlAltWin => "Cmd+Opt+Ctrl",
            ModifierFlag::All => "Shift+Cmd+Opt+Ctrl",
        }
    }

    /// Get all possible modifier flags
    pub fn all() -> &'static [ModifierFlag] {
        &[
            ModifierFlag::None,
            ModifierFlag::Shift,
            ModifierFlag::Control,
            ModifierFlag::ShiftControl,
            ModifierFlag::Alt,
            ModifierFlag::ShiftAlt,
            ModifierFlag::ControlAlt,
            ModifierFlag::ShiftControlAlt,
            ModifierFlag::Win,
            ModifierFlag::ShiftWin,
            ModifierFlag::ControlWin,
            ModifierFlag::ShiftControlWin,
            ModifierFlag::AltWin,
            ModifierFlag::ShiftAltWin,
            ModifierFlag::ControlAltWin,
            ModifierFlag::All,
        ]
    }
}

/// A mouse modifier action assignment
///
/// Actions are represented as strings like "1 m", "2 m", "13 m", etc.
/// The format is: "{action_id} m" where action_id is a number.
/// An empty string or "0" means no action assigned (default).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct MouseModifierAction {
    /// The action string as returned by REAPER (e.g., "1 m", "13 m", "")
    pub action_string: String,
}

impl MouseModifierAction {
    /// Create a new action from a string
    pub fn new(action_string: String) -> Self {
        Self { action_string }
    }

    /// Create an empty action (no assignment)
    pub fn empty() -> Self {
        Self {
            action_string: String::new(),
        }
    }

    /// Check if this action is empty/unassigned
    pub fn is_empty(&self) -> bool {
        self.action_string.is_empty() || self.action_string == "0"
    }

    /// Get the action ID if available
    /// Returns None if empty or if parsing fails
    pub fn action_id(&self) -> Option<u32> {
        if self.is_empty() {
            return None;
        }

        // Parse "123 m" -> 123
        self.action_string
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u32>().ok())
    }

    /// Get the full action string
    pub fn as_str(&self) -> &str {
        &self.action_string
    }
}

impl From<String> for MouseModifierAction {
    fn from(action_string: String) -> Self {
        Self::new(action_string)
    }
}

impl From<&str> for MouseModifierAction {
    fn from(action_string: &str) -> Self {
        Self::new(action_string.to_string())
    }
}

/// A complete mouse modifier assignment for a single context
/// Maps all 16 modifier flag combinations to their actions
#[derive(Debug, Clone, Facet)]
pub struct ContextModifierMap {
    /// The context this map is for
    pub context: MouseModifierContext,

    /// Map of modifier flags to actions
    pub assignments: HashMap<ModifierFlag, MouseModifierAction>,
}

impl ContextModifierMap {
    /// Create a new empty context map
    pub fn new(context: MouseModifierContext) -> Self {
        Self {
            context,
            assignments: HashMap::new(),
        }
    }

    /// Get the action for a specific modifier flag
    pub fn get_action(&self, flag: ModifierFlag) -> Option<&MouseModifierAction> {
        self.assignments.get(&flag)
    }

    /// Set the action for a specific modifier flag
    pub fn set_action(&mut self, flag: ModifierFlag, action: MouseModifierAction) {
        self.assignments.insert(flag, action);
    }

    /// Get all non-empty assignments
    pub fn non_empty_assignments(&self) -> Vec<(ModifierFlag, &MouseModifierAction)> {
        self.assignments
            .iter()
            .filter(|(_, action)| !action.is_empty())
            .map(|(flag, action)| (*flag, action))
            .collect()
    }

    /// Check if this context has any assignments
    pub fn has_assignments(&self) -> bool {
        !self.non_empty_assignments().is_empty()
    }
}

/// A complete mouse modifier map for all contexts
/// This is the typesafe representation of all mouse modifier assignments
#[derive(Debug, Clone, Facet)]
pub struct MouseModifierMap {
    /// Map of contexts to their modifier assignments
    pub contexts: HashMap<MouseModifierContext, ContextModifierMap>,

    /// Metadata
    pub name: Option<String>,
    pub description: Option<String>,
}

impl MouseModifierMap {
    /// Create a new empty mouse modifier map
    pub fn new() -> Self {
        Self {
            contexts: HashMap::new(),
            name: None,
            description: None,
        }
    }

    /// Create with a name
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Create with a description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Get or create a context map for a context
    pub fn get_or_create_context(
        &mut self,
        context: MouseModifierContext,
    ) -> &mut ContextModifierMap {
        self.contexts
            .entry(context)
            .or_insert_with(|| ContextModifierMap::new(context));
        self.contexts.get_mut(&context).unwrap()
    }

    /// Get a context map (immutable)
    pub fn get_context(&self, context: MouseModifierContext) -> Option<&ContextModifierMap> {
        self.contexts.get(&context)
    }

    /// Get a context map (mutable)
    pub fn get_context_mut(
        &mut self,
        context: MouseModifierContext,
    ) -> Option<&mut ContextModifierMap> {
        self.contexts.get_mut(&context)
    }

    /// Get the action for a specific context and modifier flag
    pub fn get_action(
        &self,
        context: MouseModifierContext,
        flag: ModifierFlag,
    ) -> Option<&MouseModifierAction> {
        self.get_context(context)
            .and_then(|ctx_map| ctx_map.get_action(flag))
    }

    /// Set the action for a specific context and modifier flag
    pub fn set_action(
        &mut self,
        context: MouseModifierContext,
        flag: ModifierFlag,
        action: MouseModifierAction,
    ) {
        let ctx_map = self.get_or_create_context(context);
        ctx_map.set_action(flag, action);
    }

    /// Get all contexts that have assignments
    pub fn contexts_with_assignments(&self) -> Vec<&ContextModifierMap> {
        self.contexts
            .values()
            .filter(|ctx_map| ctx_map.has_assignments())
            .collect()
    }

    /// Get total count of assignments across all contexts
    pub fn total_assignments(&self) -> usize {
        self.contexts
            .values()
            .map(|ctx_map| ctx_map.non_empty_assignments().len())
            .sum()
    }

    /// Initialize with all known contexts (empty assignments)
    pub fn initialize_all_contexts(&mut self) {
        for context in MouseModifierContext::all() {
            self.contexts
                .entry(context)
                .or_insert_with(|| ContextModifierMap::new(context));
        }
    }

    /// Save the map to a JSON file
    pub fn save_to_json(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let json = facet_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a map from a JSON file
    pub fn load_from_json(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let json = std::fs::read_to_string(path)?;
        let map: MouseModifierMap = facet_json::from_str(&json)?;
        Ok(map)
    }
}

impl Default for MouseModifierMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Load all current mouse modifiers from REAPER into a typesafe map
pub fn load_all_modifiers(
    medium_reaper: &reaper_medium::Reaper,
) -> Result<MouseModifierMap, Box<dyn std::error::Error>> {
    use crate::input::mouse_modifiers::core::{AllModifierFlags, get_mouse_modifier};

    let low_reaper = medium_reaper.low();

    // Check if GetMouseModifier is available
    if low_reaper.pointers().GetMouseModifier.is_none() {
        return Err("GetMouseModifier is not available in this REAPER version".into());
    }

    let mut map = MouseModifierMap::new();
    map.initialize_all_contexts();

    // Load all modifiers for all contexts
    for context_enum in MouseModifierContext::all() {
        let context_str = context_enum.to_reaper_string();
        let ctx_map = map.get_or_create_context(context_enum);

        // Check all 16 possible modifier flag combinations (0-15)
        for (_flag_val, flag) in AllModifierFlags::new() {
            if let Some(action_string) = get_mouse_modifier(context_str, flag, medium_reaper) {
                let action = MouseModifierAction::from(action_string);
                let modifier_flag = ModifierFlag::from_mouse_modifier_flag(&flag);
                ctx_map.set_action(modifier_flag, action);
            }
        }
    }

    Ok(map)
}

/// Apply a typesafe mouse modifier map to REAPER
pub fn apply_modifier_map(
    map: &MouseModifierMap,
    medium_reaper: &reaper_medium::Reaper,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::input::mouse_modifiers::core::set_mouse_modifier;

    let low_reaper = medium_reaper.low();

    // Check if SetMouseModifier is available
    if low_reaper.pointers().SetMouseModifier.is_none() {
        return Err("SetMouseModifier is not available in this REAPER version".into());
    }

    // Apply all assignments from the map
    for (context_enum, ctx_map) in &map.contexts {
        let context_str = context_enum.to_reaper_string();

        for (modifier_flag, action) in &ctx_map.assignments {
            let mouse_flag = modifier_flag.to_mouse_modifier_flag();

            if action.is_empty() {
                // Reset to default
                set_mouse_modifier(context_str, mouse_flag, "-1", medium_reaper)?;
            } else {
                // Set the action
                set_mouse_modifier(
                    context_str,
                    mouse_flag,
                    &action.action_string,
                    medium_reaper,
                )?;
            }
        }
    }

    Ok(())
}
