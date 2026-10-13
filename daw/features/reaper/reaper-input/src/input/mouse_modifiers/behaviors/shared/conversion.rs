//! Conversion functions for mouse modifier behaviors

use super::behavior::MouseModifierBehavior;
use super::traits::{BehaviorId, MouseBehavior};
use crate::input::mouse_modifiers::types::{MouseButtonInput, MouseModifierContext};

// Import all behavior enums
use crate::input::mouse_modifiers::behaviors::arrange_view::*;
use crate::input::mouse_modifiers::behaviors::automation_item::*;
use crate::input::mouse_modifiers::behaviors::cursor_handle::*;
use crate::input::mouse_modifiers::behaviors::envelope::*;
use crate::input::mouse_modifiers::behaviors::fixed_lane::*;
use crate::input::mouse_modifiers::behaviors::media_item::*;
use crate::input::mouse_modifiers::behaviors::midi::*;
use crate::input::mouse_modifiers::behaviors::mixer::*;
use crate::input::mouse_modifiers::behaviors::project::*;
use crate::input::mouse_modifiers::behaviors::razor_edit::*;
use crate::input::mouse_modifiers::behaviors::ruler::*;
use crate::input::mouse_modifiers::behaviors::track::*;

/// Get a behavior from context, button input, and behavior ID
/// This is the main entry point for behavior handling
pub fn get_behavior(
    context: &MouseModifierContext,
    button_input: MouseButtonInput,
    behavior_id: u32,
) -> MouseModifierBehavior {
    // Helper to box a behavior - all our behavior enums implement MouseBehavior via the blanket impl
    fn box_behavior<T: MouseBehavior + 'static>(behavior: T) -> MouseModifierBehavior {
        MouseModifierBehavior::Known(Box::new(behavior))
    }

    // Helper to create unknown behavior
    fn unknown_behavior(
        context: &MouseModifierContext,
        button_input: MouseButtonInput,
        behavior_id: u32,
    ) -> MouseModifierBehavior {
        MouseModifierBehavior::Unknown {
            context: *context,
            button_input,
            behavior_id,
        }
    }

    match context {
        MouseModifierContext::ArrangeView(interaction) => match interaction {
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Middle(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::MiddleDrag => {
                        box_behavior(ArrangeMiddleDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::MiddleClick => {
                        box_behavior(ArrangeMiddleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Right(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::RightDrag => {
                        box_behavior(ArrangeRightDragBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::AutomationItem(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Default(input)
                    if *input == button_input =>
                {
                    match button_input {
                        MouseButtonInput::LeftDrag => box_behavior(
                            AutomationItemLeftDragBehavior::from_behavior_id(behavior_id),
                        ),
                        _ => unknown_behavior(context, button_input, behavior_id),
                    }
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Edge => {
                    box_behavior(AutomationItemEdgeBehavior::from_behavior_id(behavior_id))
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::DoubleClick(
                    input,
                ) if *input == button_input => match button_input {
                    MouseButtonInput::DoubleClick => box_behavior(
                        AutomationItemDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                },
                _ => unknown_behavior(context, button_input, behavior_id),
            }
        }

        MouseModifierContext::CursorHandle => {
            box_behavior(CursorHandleBehavior::from_behavior_id(behavior_id))
        }

        MouseModifierContext::Envelope(interaction) => match interaction {
            crate::input::mouse_modifiers::types::EnvelopeInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => box_behavior(
                        EnvelopeControlPanelDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Lane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(EnvelopeLaneLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Point(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(EnvelopePointLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        EnvelopePointDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Segment(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => box_behavior(
                        EnvelopeSegmentLeftDragBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::DoubleClick => box_behavior(
                        EnvelopeSegmentDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::FixedLane(interaction) => match interaction {
            crate::input::mouse_modifiers::types::FixedLaneInteraction::HeaderButton(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::Click => box_behavior(
                        FixedLaneHeaderButtonClickBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::DoubleClick => box_behavior(
                        FixedLaneHeaderButtonDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::FixedLaneInteraction::LinkedLane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => box_behavior(
                        FixedLaneLinkedLaneLeftDragBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::Click => box_behavior(
                        FixedLaneLinkedLaneClickBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::DoubleClick => box_behavior(
                        FixedLaneLinkedLaneDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::MediaItem(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MediaItemInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MediaItemLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(MediaItemClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => {
                        box_behavior(MediaItemDoubleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Edge(edge_interaction) => {
                match edge_interaction {
                    crate::input::mouse_modifiers::types::EdgeInteraction::Default(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::LeftDrag => box_behavior(
                                MediaItemEdgeLeftDragBehavior::from_behavior_id(behavior_id),
                            ),
                            _ => unknown_behavior(context, button_input, behavior_id),
                        }
                    }
                    crate::input::mouse_modifiers::types::EdgeInteraction::DoubleClick(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::DoubleClick => box_behavior(
                                MediaItemEdgeDoubleClickBehavior::from_behavior_id(behavior_id),
                            ),
                            _ => unknown_behavior(context, button_input, behavior_id),
                        }
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Fade(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MediaItemFadeLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(MediaItemFadeClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        MediaItemFadeDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Lower(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => box_behavior(
                        MediaItemLowerLeftDragBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::Click => {
                        box_behavior(MediaItemLowerClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        MediaItemLowerDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::StretchMarker(
                stretch_interaction,
            ) => match stretch_interaction {
                crate::input::mouse_modifiers::types::StretchMarkerInteraction::Default(input)
                    if *input == button_input =>
                {
                    match button_input {
                        MouseButtonInput::LeftDrag => box_behavior(
                            MediaItemStretchMarkerLeftDragBehavior::from_behavior_id(behavior_id),
                        ),
                        _ => unknown_behavior(context, button_input, behavior_id),
                    }
                }
                crate::input::mouse_modifiers::types::StretchMarkerInteraction::Rate => {
                    box_behavior(MediaItemStretchMarkerRateBehavior::from_behavior_id(
                        behavior_id,
                    ))
                }
                crate::input::mouse_modifiers::types::StretchMarkerInteraction::DoubleClick(
                    input,
                ) if *input == button_input => match button_input {
                    MouseButtonInput::DoubleClick => box_behavior(
                        MediaItemStretchMarkerDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                },
                _ => unknown_behavior(context, button_input, behavior_id),
            },
            crate::input::mouse_modifiers::types::MediaItemInteraction::Crossfade(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => box_behavior(
                        MediaItemCrossfadeLeftDragBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::Click => box_behavior(
                        MediaItemCrossfadeClickBehavior::from_behavior_id(behavior_id),
                    ),
                    MouseButtonInput::DoubleClick => box_behavior(
                        MediaItemCrossfadeDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::Midi(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MidiInteraction::CcEvent(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MidiCcEventLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        MidiCcEventDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::CcLane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MidiCcLaneLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => {
                        box_behavior(MidiCcLaneDoubleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::CcSegment(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MidiCcSegmentLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        MidiCcSegmentDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::EndPointer => {
                box_behavior(MidiEndPointerBehavior::from_behavior_id(behavior_id))
            }
            crate::input::mouse_modifiers::types::MidiInteraction::MarkerLanes => {
                box_behavior(MidiMarkerLanesBehavior::from_behavior_id(behavior_id))
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Note(note_interaction) => {
                match note_interaction {
                    crate::input::mouse_modifiers::types::NoteInteraction::Default(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::LeftDrag => box_behavior(
                                MidiNoteLeftDragBehavior::from_behavior_id(behavior_id),
                            ),
                            MouseButtonInput::Click => {
                                box_behavior(MidiNoteClickBehavior::from_behavior_id(behavior_id))
                            }
                            MouseButtonInput::DoubleClick => box_behavior(
                                MidiNoteDoubleClickBehavior::from_behavior_id(behavior_id),
                            ),
                            _ => unknown_behavior(context, button_input, behavior_id),
                        }
                    }
                    crate::input::mouse_modifiers::types::NoteInteraction::Edge => {
                        box_behavior(MidiNoteEdgeBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::PianoRoll(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MidiPianoRollLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(MidiPianoRollClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => box_behavior(
                        MidiPianoRollDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Right(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::RightDrag => {
                        box_behavior(MidiRightDragBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Ruler(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(MidiRulerLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(MidiRulerClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => {
                        box_behavior(MidiRulerDoubleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::Mixer(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MixerInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => box_behavior(
                        MixerControlPanelDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::Project(interaction) => match interaction {
            crate::input::mouse_modifiers::types::ProjectInteraction::MarkerLanes => {
                box_behavior(ProjectMarkerLanesBehavior::from_behavior_id(behavior_id))
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::MarkerRegionEdge => {
                box_behavior(ProjectMarkerRegionEdgeBehavior::from_behavior_id(
                    behavior_id,
                ))
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::Region => {
                box_behavior(ProjectRegionBehavior::from_behavior_id(behavior_id))
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::TempoMarker => {
                box_behavior(ProjectTempoMarkerBehavior::from_behavior_id(behavior_id))
            }
        },

        MouseModifierContext::RazorEdit(interaction) => match interaction {
            crate::input::mouse_modifiers::types::RazorEditInteraction::Area(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(RazorEditAreaLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(RazorEditAreaClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::RazorEditInteraction::Edge => {
                box_behavior(RazorEditEdgeBehavior::from_behavior_id(behavior_id))
            }
            crate::input::mouse_modifiers::types::RazorEditInteraction::EnvelopeArea => {
                box_behavior(RazorEditEnvelopeAreaBehavior::from_behavior_id(behavior_id))
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::Ruler(interaction) => match interaction {
            crate::input::mouse_modifiers::types::RulerInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(RulerLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(RulerClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => {
                        box_behavior(RulerDoubleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },

        MouseModifierContext::Track(interaction) => match interaction {
            crate::input::mouse_modifiers::types::TrackInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        box_behavior(TrackLeftDragBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::Click => {
                        box_behavior(TrackClickBehavior::from_behavior_id(behavior_id))
                    }
                    MouseButtonInput::DoubleClick => {
                        box_behavior(TrackDoubleClickBehavior::from_behavior_id(behavior_id))
                    }
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            crate::input::mouse_modifiers::types::TrackInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => box_behavior(
                        TrackControlPanelDoubleClickBehavior::from_behavior_id(behavior_id),
                    ),
                    _ => unknown_behavior(context, button_input, behavior_id),
                }
            }
            _ => unknown_behavior(context, button_input, behavior_id),
        },
    }
}

/// Get the human-readable display name for a mouse modifier behavior
///
/// Parameters:
/// - `context`: The mouse modifier context (e.g., ArrangeView, MediaItem, etc.)
/// - `button_input`: The mouse button interaction (e.g., LeftDrag, MiddleDrag, etc.)
/// - `behavior_id`: The behavior ID number (e.g., 1, 2, 13, etc.)
///
/// Returns the behavior ID string if no mapping is found
///
/// This function now uses behavior enums internally where available,
/// falling back to the legacy string-based lookup for contexts that haven't
/// been converted yet.
pub fn get_mouse_modifier_name(
    context: &MouseModifierContext,
    button_input: MouseButtonInput,
    behavior_id: u32,
) -> String {
    let behavior = get_behavior(context, button_input, behavior_id);
    behavior.display_name()
}
