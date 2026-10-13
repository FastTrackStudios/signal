//! Manual mapping of REAPER mouse modifier action IDs to display names
//!
//! Since mouse modifier actions are internal REAPER actions that don't have
//! named command IDs, we need to manually map action IDs to their human-readable names.
//!
//! IMPORTANT: Each context+interaction combination has its own unique set of action IDs.
//! For example, "1 m" means different things in:
//! - Arrange View -> Right Drag
//! - Arrange View -> Middle Drag
//! - MIDI Editor -> Note (Left Drag)
//!
//! This mapping is based on REAPER's mouse modifier preferences UI.
//!
//! # Typesafe Actions
//!
//! This module now provides typesafe action enums for better compile-time safety.
//! Instead of working with raw `u32` action IDs, you can use specific enum types
//! that allow pattern matching and type-safe logic.
//!
//! ## Example Usage
//!
//! ```rust
//! use crate::input::mouse_modifiers::action_names::*;
//! use crate::input::mouse_modifiers::types::*;
//!
//! // Get a typesafe action from context and action ID
//! let context = MouseModifierContext::ArrangeView(
//!     ArrangeViewInteraction::Middle(MouseButtonInput::MiddleDrag)
//! );
//! let action = get_typesafe_action(&context, MouseButtonInput::MiddleDrag, 1);
//!
//! // Pattern match on the specific action type
//! match action {
//!     TypesafeMouseModifierAction::ArrangeMiddleDrag(arrange_action) => {
//!         match arrange_action {
//!             ArrangeMiddleDragAction::ScrubAudio => {
//!                 // Type-safe logic for scrub audio action
//!                 println!("User wants to scrub audio");
//!             }
//!             ArrangeMiddleDragAction::HandScroll => {
//!                 // Type-safe logic for hand scroll
//!                 println!("User wants to hand scroll");
//!             }
//!             _ => {}
//!         }
//!     }
//!     _ => {}
//! }
//!
//! // Or use the display name directly
//! println!("Action: {}", action.display_name());
//! ```
//!
//! ## Adding New Action Types
//!
//! To add typesafe actions for other contexts, use the `define_action_enum!` macro:
//!
//! ```rust
//! define_action_enum! {
//!     pub enum MyNewAction {
//!         NoAction = 0 => "No action",
//!         SomeAction = 1 => "Some action",
//!         AnotherAction = 2 => "Another action",
//!     }
//! }
//! // The macro automatically adds Unknown(u32) variant
//! ```
//!
//! Then add it to the `TypesafeMouseModifierAction` enum and update `get_typesafe_action`.

use crate::input::mouse_modifiers::types::{MouseButtonInput, MouseModifierContext};

// ============================================================================
// Typesafe Action Enums
// ============================================================================

/// Macro to generate typesafe action enums with conversion and display name methods
macro_rules! define_action_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $id:expr => $display:literal
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $enum_name {
            $(
                $(#[$variant_meta])*
                $variant,
            )*
            Unknown(u32),
        }

        impl $enum_name {
            /// Convert from action ID to enum variant
            pub fn from_action_id(action_id: u32) -> Self {
                match action_id {
                    $(
                        $id => $enum_name::$variant,
                    )*
                    id => $enum_name::Unknown(id),
                }
            }

            /// Convert enum variant to action ID
            pub fn to_action_id(&self) -> u32 {
                match self {
                    $(
                        $enum_name::$variant => $id,
                    )*
                    $enum_name::Unknown(id) => *id,
                }
            }

            /// Get the display name for this action
            pub fn display_name(&self) -> &'static str {
                match self {
                    $(
                        $enum_name::$variant => $display,
                    )*
                    $enum_name::Unknown(_) => "Unknown action",
                }
            }

            /// Get the action ID string format (e.g., "1 m", "13 m")
            pub fn action_id_string(&self) -> String {
                format!("{} m", self.to_action_id())
            }
        }
    };
}

// ============================================================================
// Arrange View Actions
// ============================================================================

define_action_enum! {
    pub enum ArrangeMiddleDragAction {
        NoAction = 0 => "No action",
        ScrubAudio = 1 => "Scrub audio",
        HandScroll = 2 => "Hand scroll",
        ScrollBrowserStyle = 3 => "Scroll browser-style",
        JogAudio = 4 => "Jog audio",
        ScrubAudioLoopedSegment = 5 => "Scrub audio (looped-segment mode)",
        JogAudioLoopedSegment = 6 => "Jog audio (looped-segment mode)",
        MarqueeZoom = 7 => "Marquee zoom",
        MoveEditCursorWithoutScrubJog = 8 => "Move edit cursor without scrub/jog",
        HandScrollAndHorizontalZoom = 9 => "Hand scroll and horizontal zoom",
        HorizontalZoom = 11 => "Horizontal zoom",
        SetEditCursorAndHorizontalZoom = 13 => "Set edit cursor and horizontal zoom",
        SetEditCursorAndHandScroll = 15 => "Set edit cursor and hand scroll",
        SetEditCursorHandScrollAndHorizontalZoom = 16 => "Set edit cursor, hand scroll and horizontal zoom",
    }
}

define_action_enum! {
    pub enum ArrangeMiddleClickAction {
        NoAction = 0 => "No action",
        RestorePreviousZoomScroll = 1 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 2 => "Restore previous zoom level",
        MoveEditCursorIgnoringSnap = 3 => "Move edit cursor ignoring snap",
    }
}

define_action_enum! {
    pub enum ArrangeRightDragAction {
        NoAction = 1 => "No Action",
        MarqueeSelectItems = 2 => "Marquee Select Items",
        MarqueeAddToItemSelection = 3 => "Marquee Add to Item Selection",
        MarqueeToggleItemSelection = 4 => "Marquee Toggle Item Selection",
        MarqueeSelectItemsAndTime = 5 => "Marquee Select Items and Time",
        MarqueeSelectItemsAndTimeIgnoringSnap = 6 => "Marquee Select Items and Time (ignoring snap)",
    }
}

// ============================================================================
// Wrapper enum for all typesafe actions
// ============================================================================

/// Wrapper enum that allows pattern matching on all typesafe action types
/// This provides a unified way to work with actions from different contexts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypesafeMouseModifierAction {
    // Arrange View actions
    ArrangeMiddleDrag(ArrangeMiddleDragAction),
    ArrangeMiddleClick(ArrangeMiddleClickAction),
    ArrangeRightDrag(ArrangeRightDragAction),

    // Automation Item actions
    AutomationItemLeftDrag(AutomationItemLeftDragAction),
    AutomationItemEdge(AutomationItemEdgeAction),
    AutomationItemDoubleClick(AutomationItemDoubleClickAction),

    // Cursor Handle actions
    CursorHandle(CursorHandleAction),

    // Envelope actions
    EnvelopeControlPanelDoubleClick(EnvelopeControlPanelDoubleClickAction),
    EnvelopeLaneLeftDrag(EnvelopeLaneLeftDragAction),
    EnvelopePointLeftDrag(EnvelopePointLeftDragAction),
    EnvelopePointDoubleClick(EnvelopePointDoubleClickAction),
    EnvelopeSegmentLeftDrag(EnvelopeSegmentLeftDragAction),
    EnvelopeSegmentDoubleClick(EnvelopeSegmentDoubleClickAction),

    // Fixed Lane actions
    FixedLaneHeaderButtonClick(FixedLaneHeaderButtonClickAction),
    FixedLaneHeaderButtonDoubleClick(FixedLaneHeaderButtonDoubleClickAction),
    FixedLaneLinkedLaneLeftDrag(FixedLaneLinkedLaneLeftDragAction),
    FixedLaneLinkedLaneClick(FixedLaneLinkedLaneClickAction),
    FixedLaneLinkedLaneDoubleClick(FixedLaneLinkedLaneDoubleClickAction),

    // Other contexts will be added as we create their enums
    // For now, we use a generic Unknown variant
    Unknown {
        context: MouseModifierContext,
        button_input: MouseButtonInput,
        action_id: u32,
    },
}

impl TypesafeMouseModifierAction {
    /// Get the display name for this action
    pub fn display_name(&self) -> String {
        match self {
            TypesafeMouseModifierAction::ArrangeMiddleDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::ArrangeMiddleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::ArrangeRightDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::AutomationItemLeftDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::AutomationItemEdge(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::AutomationItemDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::CursorHandle(action) => action.display_name().to_string(),
            TypesafeMouseModifierAction::EnvelopeControlPanelDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::EnvelopeLaneLeftDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::EnvelopePointLeftDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::EnvelopePointDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::EnvelopeSegmentLeftDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::EnvelopeSegmentDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneLeftDrag(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneDoubleClick(action) => {
                action.display_name().to_string()
            }
            TypesafeMouseModifierAction::Unknown { action_id, .. } => {
                format!("Action {} m", action_id)
            }
        }
    }

    /// Get the action ID
    pub fn action_id(&self) -> u32 {
        match self {
            TypesafeMouseModifierAction::ArrangeMiddleDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::ArrangeMiddleClick(action) => action.to_action_id(),
            TypesafeMouseModifierAction::ArrangeRightDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::AutomationItemLeftDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::AutomationItemEdge(action) => action.to_action_id(),
            TypesafeMouseModifierAction::AutomationItemDoubleClick(action) => action.to_action_id(),
            TypesafeMouseModifierAction::CursorHandle(action) => action.to_action_id(),
            TypesafeMouseModifierAction::EnvelopeControlPanelDoubleClick(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::EnvelopeLaneLeftDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::EnvelopePointLeftDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::EnvelopePointDoubleClick(action) => action.to_action_id(),
            TypesafeMouseModifierAction::EnvelopeSegmentLeftDrag(action) => action.to_action_id(),
            TypesafeMouseModifierAction::EnvelopeSegmentDoubleClick(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonClick(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonDoubleClick(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneLeftDrag(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneClick(action) => action.to_action_id(),
            TypesafeMouseModifierAction::FixedLaneLinkedLaneDoubleClick(action) => {
                action.to_action_id()
            }
            TypesafeMouseModifierAction::Unknown { action_id, .. } => *action_id,
        }
    }

    /// Get the action ID string format (e.g., "1 m", "13 m")
    pub fn action_id_string(&self) -> String {
        match self {
            TypesafeMouseModifierAction::ArrangeMiddleDrag(action) => action.action_id_string(),
            TypesafeMouseModifierAction::ArrangeMiddleClick(action) => action.action_id_string(),
            TypesafeMouseModifierAction::ArrangeRightDrag(action) => action.action_id_string(),
            TypesafeMouseModifierAction::AutomationItemLeftDrag(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::AutomationItemEdge(action) => action.action_id_string(),
            TypesafeMouseModifierAction::AutomationItemDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::CursorHandle(action) => action.action_id_string(),
            TypesafeMouseModifierAction::EnvelopeControlPanelDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::EnvelopeLaneLeftDrag(action) => action.action_id_string(),
            TypesafeMouseModifierAction::EnvelopePointLeftDrag(action) => action.action_id_string(),
            TypesafeMouseModifierAction::EnvelopePointDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::EnvelopeSegmentLeftDrag(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::EnvelopeSegmentDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::FixedLaneHeaderButtonDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneLeftDrag(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::FixedLaneLinkedLaneDoubleClick(action) => {
                action.action_id_string()
            }
            TypesafeMouseModifierAction::Unknown { action_id, .. } => {
                format!("{} m", action_id)
            }
        }
    }
}

/// Get a typesafe action enum from context, button input, and action ID
/// This is the main entry point for typesafe action handling
pub fn get_typesafe_action(
    context: &MouseModifierContext,
    button_input: MouseButtonInput,
    action_id: u32,
) -> TypesafeMouseModifierAction {
    match context {
        MouseModifierContext::ArrangeView(interaction) => match interaction {
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Middle(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::MiddleDrag => TypesafeMouseModifierAction::ArrangeMiddleDrag(
                        ArrangeMiddleDragAction::from_action_id(action_id),
                    ),
                    MouseButtonInput::MiddleClick => {
                        TypesafeMouseModifierAction::ArrangeMiddleClick(
                            ArrangeMiddleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Right(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::RightDrag => TypesafeMouseModifierAction::ArrangeRightDrag(
                        ArrangeRightDragAction::from_action_id(action_id),
                    ),
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            _ => TypesafeMouseModifierAction::Unknown {
                context: *context,
                button_input,
                action_id,
            },
        },
        // Automation Item contexts
        MouseModifierContext::AutomationItem(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Default(input)
                    if *input == button_input =>
                {
                    match button_input {
                        MouseButtonInput::LeftDrag => {
                            TypesafeMouseModifierAction::AutomationItemLeftDrag(
                                AutomationItemLeftDragAction::from_action_id(action_id),
                            )
                        }
                        _ => TypesafeMouseModifierAction::Unknown {
                            context: *context,
                            button_input,
                            action_id,
                        },
                    }
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Edge => {
                    TypesafeMouseModifierAction::AutomationItemEdge(
                        AutomationItemEdgeAction::from_action_id(action_id),
                    )
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::DoubleClick(
                    input,
                ) if *input == button_input => match button_input {
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::AutomationItemDoubleClick(
                            AutomationItemDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                },
                _ => TypesafeMouseModifierAction::Unknown {
                    context: *context,
                    button_input,
                    action_id,
                },
            }
        }

        // Cursor Handle
        MouseModifierContext::CursorHandle => {
            TypesafeMouseModifierAction::CursorHandle(CursorHandleAction::from_action_id(action_id))
        }

        // Envelope contexts
        MouseModifierContext::Envelope(interaction) => match interaction {
            crate::input::mouse_modifiers::types::EnvelopeInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::EnvelopeControlPanelDoubleClick(
                            EnvelopeControlPanelDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Lane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        TypesafeMouseModifierAction::EnvelopeLaneLeftDrag(
                            EnvelopeLaneLeftDragAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Point(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        TypesafeMouseModifierAction::EnvelopePointLeftDrag(
                            EnvelopePointLeftDragAction::from_action_id(action_id),
                        )
                    }
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::EnvelopePointDoubleClick(
                            EnvelopePointDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Segment(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        TypesafeMouseModifierAction::EnvelopeSegmentLeftDrag(
                            EnvelopeSegmentLeftDragAction::from_action_id(action_id),
                        )
                    }
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::EnvelopeSegmentDoubleClick(
                            EnvelopeSegmentDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            _ => TypesafeMouseModifierAction::Unknown {
                context: *context,
                button_input,
                action_id,
            },
        },

        // Fixed Lane contexts
        MouseModifierContext::FixedLane(interaction) => match interaction {
            crate::input::mouse_modifiers::types::FixedLaneInteraction::HeaderButton(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::Click => {
                        TypesafeMouseModifierAction::FixedLaneHeaderButtonClick(
                            FixedLaneHeaderButtonClickAction::from_action_id(action_id),
                        )
                    }
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::FixedLaneHeaderButtonDoubleClick(
                            FixedLaneHeaderButtonDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            crate::input::mouse_modifiers::types::FixedLaneInteraction::LinkedLane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        TypesafeMouseModifierAction::FixedLaneLinkedLaneLeftDrag(
                            FixedLaneLinkedLaneLeftDragAction::from_action_id(action_id),
                        )
                    }
                    MouseButtonInput::Click => {
                        TypesafeMouseModifierAction::FixedLaneLinkedLaneClick(
                            FixedLaneLinkedLaneClickAction::from_action_id(action_id),
                        )
                    }
                    MouseButtonInput::DoubleClick => {
                        TypesafeMouseModifierAction::FixedLaneLinkedLaneDoubleClick(
                            FixedLaneLinkedLaneDoubleClickAction::from_action_id(action_id),
                        )
                    }
                    _ => TypesafeMouseModifierAction::Unknown {
                        context: *context,
                        button_input,
                        action_id,
                    },
                }
            }
            _ => TypesafeMouseModifierAction::Unknown {
                context: *context,
                button_input,
                action_id,
            },
        },

        _ => {
            // For other contexts, we'll add enums as needed
            // For now, return Unknown variant
            TypesafeMouseModifierAction::Unknown {
                context: *context,
                button_input,
                action_id,
            }
        }
    }
}

/// Get the human-readable display name for a mouse modifier action
///
/// Parameters:
/// - `context`: The mouse modifier context (e.g., ArrangeView, MediaItem, etc.)
/// - `button_input`: The mouse button interaction (e.g., LeftDrag, MiddleDrag, etc.)
/// - `action_id`: The action ID number (e.g., 1, 2, 13, etc.)
///
/// Returns the action ID string if no mapping is found
///
/// This function now uses typesafe action enums internally where available,
/// falling back to the legacy string-based lookup for contexts that haven't
/// been converted yet.
pub fn get_mouse_modifier_name(
    context: &MouseModifierContext,
    button_input: MouseButtonInput,
    action_id: u32,
) -> String {
    // Try to use typesafe action lookup first
    let typesafe_action = get_typesafe_action(context, button_input, action_id);

    // If we got a typesafe action (not Unknown), use it
    match &typesafe_action {
        TypesafeMouseModifierAction::ArrangeMiddleDrag(_)
        | TypesafeMouseModifierAction::ArrangeMiddleClick(_)
        | TypesafeMouseModifierAction::ArrangeRightDrag(_)
        | TypesafeMouseModifierAction::AutomationItemLeftDrag(_)
        | TypesafeMouseModifierAction::AutomationItemEdge(_)
        | TypesafeMouseModifierAction::AutomationItemDoubleClick(_)
        | TypesafeMouseModifierAction::CursorHandle(_)
        | TypesafeMouseModifierAction::EnvelopeControlPanelDoubleClick(_)
        | TypesafeMouseModifierAction::EnvelopeLaneLeftDrag(_)
        | TypesafeMouseModifierAction::EnvelopePointLeftDrag(_)
        | TypesafeMouseModifierAction::EnvelopePointDoubleClick(_)
        | TypesafeMouseModifierAction::EnvelopeSegmentLeftDrag(_)
        | TypesafeMouseModifierAction::EnvelopeSegmentDoubleClick(_)
        | TypesafeMouseModifierAction::FixedLaneHeaderButtonClick(_)
        | TypesafeMouseModifierAction::FixedLaneHeaderButtonDoubleClick(_)
        | TypesafeMouseModifierAction::FixedLaneLinkedLaneLeftDrag(_)
        | TypesafeMouseModifierAction::FixedLaneLinkedLaneClick(_)
        | TypesafeMouseModifierAction::FixedLaneLinkedLaneDoubleClick(_) => {
            return typesafe_action.display_name();
        }
        TypesafeMouseModifierAction::Unknown { .. } => {
            // Fall through to legacy lookup
        }
    }

    // Fall back to legacy string-based lookup for contexts not yet converted
    match context {
        // Arrange View contexts - these should be handled by typesafe actions above,
        // but we keep this as a fallback
        MouseModifierContext::ArrangeView(interaction) => match interaction {
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Middle(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::MiddleDrag => get_arrange_middle_drag_action_name(action_id),
                    MouseButtonInput::MiddleClick => {
                        get_arrange_middle_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::ArrangeViewInteraction::Right(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::RightDrag => get_arrange_right_drag_action_name(action_id),
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // Automation Item contexts
        MouseModifierContext::AutomationItem(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Default(input)
                    if *input == button_input =>
                {
                    match button_input {
                        MouseButtonInput::LeftDrag => {
                            get_automation_item_left_drag_action_name(action_id)
                        }
                        _ => format!("Action {} m", action_id),
                    }
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Edge => {
                    get_automation_item_edge_action_name(action_id)
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::DoubleClick(
                    input,
                ) if *input == button_input => match button_input {
                    MouseButtonInput::DoubleClick => {
                        get_automation_item_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                },
                _ => format!("Action {} m", action_id),
            }
        }

        // Cursor Handle
        MouseModifierContext::CursorHandle => get_cursor_handle_action_name(action_id),

        // Envelope contexts
        MouseModifierContext::Envelope(interaction) => match interaction {
            crate::input::mouse_modifiers::types::EnvelopeInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => {
                        get_envelope_control_panel_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Lane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_envelope_lane_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_envelope_lane_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Point(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_envelope_point_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_envelope_point_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::EnvelopeInteraction::Segment(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_envelope_segment_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_envelope_segment_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // Fixed Lane contexts
        MouseModifierContext::FixedLane(interaction) => match interaction {
            crate::input::mouse_modifiers::types::FixedLaneInteraction::HeaderButton(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::Click => {
                        get_fixed_lane_header_button_click_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_fixed_lane_header_button_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::FixedLaneInteraction::LinkedLane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_fixed_lane_linked_lane_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => {
                        get_fixed_lane_linked_lane_click_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_fixed_lane_linked_lane_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // MIDI contexts
        MouseModifierContext::Midi(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MidiInteraction::CcEvent(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_midi_cc_event_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_midi_cc_event_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::CcLane(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => get_midi_cc_lane_left_drag_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_midi_cc_lane_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::CcSegment(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_midi_cc_segment_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_midi_cc_segment_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::EndPointer => {
                get_midi_end_pointer_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::MidiInteraction::MarkerLanes => {
                get_midi_marker_lanes_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Note(note_interaction) => {
                match note_interaction {
                    crate::input::mouse_modifiers::types::NoteInteraction::Default(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::LeftDrag => {
                                get_midi_note_left_drag_action_name(action_id)
                            }
                            MouseButtonInput::Click => get_midi_note_click_action_name(action_id),
                            MouseButtonInput::DoubleClick => {
                                get_midi_note_double_click_action_name(action_id)
                            }
                            _ => format!("Action {} e", action_id),
                        }
                    }
                    crate::input::mouse_modifiers::types::NoteInteraction::Edge => {
                        get_midi_note_edge_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::PianoRoll(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_midi_piano_roll_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => get_midi_piano_roll_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_midi_piano_roll_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Right(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::RightDrag => get_midi_right_drag_action_name(action_id),
                    _ => format!("Action {} e", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MidiInteraction::Ruler(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => get_midi_ruler_left_drag_action_name(action_id),
                    MouseButtonInput::Click => get_midi_ruler_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_midi_ruler_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} e", action_id),
                }
            }
            _ => format!("Action {} e", action_id),
        },

        // Media Item contexts
        MouseModifierContext::MediaItem(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MediaItemInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => get_media_item_left_drag_action_name(action_id),
                    MouseButtonInput::Click => get_media_item_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_media_item_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Edge(edge_interaction) => {
                match edge_interaction {
                    crate::input::mouse_modifiers::types::EdgeInteraction::Default(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::LeftDrag => {
                                get_media_item_edge_left_drag_action_name(action_id)
                            }
                            _ => format!("Action {} m", action_id),
                        }
                    }
                    crate::input::mouse_modifiers::types::EdgeInteraction::DoubleClick(input)
                        if *input == button_input =>
                    {
                        match button_input {
                            MouseButtonInput::DoubleClick => {
                                get_media_item_edge_double_click_action_name(action_id)
                            }
                            _ => format!("Action {} m", action_id),
                        }
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Fade(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_media_item_fade_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => get_media_item_fade_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_media_item_fade_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Lower(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_media_item_lower_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => get_media_item_lower_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => {
                        get_media_item_lower_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::StretchMarker(
                stretch_interaction,
            ) => {
                match stretch_interaction {
                    crate::input::mouse_modifiers::types::StretchMarkerInteraction::Default(
                        input,
                    ) if *input == button_input => match button_input {
                        MouseButtonInput::LeftDrag => {
                            get_media_item_stretch_marker_left_drag_action_name(action_id)
                        }
                        _ => format!("Action {} m", action_id),
                    },
                    crate::input::mouse_modifiers::types::StretchMarkerInteraction::Rate => {
                        get_media_item_stretch_marker_rate_action_name(action_id)
                    }
                    crate::input::mouse_modifiers::types::StretchMarkerInteraction::DoubleClick(
                        input,
                    ) if *input == button_input => match button_input {
                        MouseButtonInput::DoubleClick => {
                            get_media_item_stretch_marker_double_click_action_name(action_id)
                        }
                        _ => format!("Action {} m", action_id),
                    },
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::MediaItemInteraction::Crossfade(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_media_item_crossfade_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => {
                        get_media_item_crossfade_click_action_name(action_id)
                    }
                    MouseButtonInput::DoubleClick => {
                        get_media_item_crossfade_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // Mixer contexts
        MouseModifierContext::Mixer(interaction) => match interaction {
            crate::input::mouse_modifiers::types::MixerInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => {
                        get_mixer_control_panel_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // Project contexts
        MouseModifierContext::Project(interaction) => match interaction {
            crate::input::mouse_modifiers::types::ProjectInteraction::MarkerLanes => {
                get_project_marker_lanes_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::MarkerRegionEdge => {
                get_project_marker_region_edge_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::Region => {
                get_project_region_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::ProjectInteraction::TempoMarker => {
                get_project_tempo_marker_action_name(action_id)
            }
        },

        // Razor Edit contexts
        MouseModifierContext::RazorEdit(interaction) => match interaction {
            crate::input::mouse_modifiers::types::RazorEditInteraction::Area(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => {
                        get_razor_edit_area_left_drag_action_name(action_id)
                    }
                    MouseButtonInput::Click => get_razor_edit_area_click_action_name(action_id),
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::RazorEditInteraction::Edge => {
                get_razor_edit_edge_action_name(action_id)
            }
            crate::input::mouse_modifiers::types::RazorEditInteraction::EnvelopeArea => {
                get_razor_edit_envelope_area_action_name(action_id)
            }
            _ => format!("Action {} m", action_id),
        },

        // Ruler contexts
        MouseModifierContext::Ruler(interaction) => match interaction {
            crate::input::mouse_modifiers::types::RulerInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => get_ruler_left_drag_action_name(action_id),
                    MouseButtonInput::Click => get_ruler_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => get_ruler_double_click_action_name(action_id),
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },

        // Track contexts
        MouseModifierContext::Track(interaction) => match interaction {
            crate::input::mouse_modifiers::types::TrackInteraction::Default(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::LeftDrag => get_track_left_drag_action_name(action_id),
                    MouseButtonInput::Click => get_track_click_action_name(action_id),
                    MouseButtonInput::DoubleClick => get_track_double_click_action_name(action_id),
                    _ => format!("Action {} m", action_id),
                }
            }
            crate::input::mouse_modifiers::types::TrackInteraction::ControlPanel(input)
                if *input == button_input =>
            {
                match button_input {
                    MouseButtonInput::DoubleClick => {
                        get_track_control_panel_double_click_action_name(action_id)
                    }
                    _ => format!("Action {} m", action_id),
                }
            }
            _ => format!("Action {} m", action_id),
        },
    }
}

// ============================================================================
// Arrange View Action Mappings
// ============================================================================

/// Legacy helper function - now uses typesafe enum internally
pub fn get_arrange_middle_drag_action_name(action_id: u32) -> String {
    ArrangeMiddleDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

/// Legacy helper function - now uses typesafe enum internally
pub fn get_arrange_middle_click_action_name(action_id: u32) -> String {
    ArrangeMiddleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

/// Legacy helper function - now uses typesafe enum internally
fn get_arrange_right_drag_action_name(action_id: u32) -> String {
    ArrangeRightDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// ============================================================================
// Automation Item Action Mappings
// ============================================================================

define_action_enum! {
    pub enum AutomationItemLeftDragAction {
        NoAction = 0 => "No action",
        MoveAutomationItemJustSnap = 1 => "Move automation item - just snap",
        MoveAutomationItemIgnoringSnap = 2 => "Move automation item - ignoring snap",
        CopyAutomationItemAndPoolIt = 3 => "Copy automation item and pool it",
        CopyAutomationItemAndPoolItIgnoringSnap = 4 => "Copy automation item and pool it, ignoring snap",
        CopyAutomationItemJustCopy = 5 => "Copy automation item - just copy",
        CopyAutomationItemIgnoringSnap = 6 => "Copy automation item - ignoring snap",
        CopyAutomationItemAndPoolItIgnoringTimeSelection = 7 => "Copy automation item and pool it, ignoring time selection",
        CopyAutomationItemAndPoolItIgnoringSnapAndTimeSelection = 8 => "Copy automation item and pool it, ignoring snap and time selection",
        CopyAutomationItemIgnoringTimeSelection = 9 => "Copy automation item - ignoring time selection",
    }
}

define_action_enum! {
    pub enum AutomationItemEdgeAction {
        NoAction = 0 => "No action",
        MoveAutomationItemEdgeJustMove = 1 => "Move automation item edge - just move",
        MoveAutomationItemEdgeIgnoringSnap = 2 => "Move automation item edge - ignoring snap",
        StretchAutomationItemEdge = 3 => "Stretch automation item edge",
        StretchAutomationItemEdgeIgnoringSnap = 4 => "Stretch automation item edge - ignoring snap",
        CollectPointsIntoAutomationItem = 5 => "Collect points into automation item",
        CollectPointsIntoAutomationItemIgnoringSnap = 6 => "Collect points into automation item - ignoring snap",
        StretchAutomationItemEdgeRelativeToOtherSelectedItems = 7 => "Stretch automation item edge - relative to other selected items",
        StretchAutomationItemEdgeRelativeToOtherSelectedItemsIgnoringSnap = 8 => "Stretch automation item edge - relative to other selected items ignoring snap",
        MoveAutomationItemEdgeRelativeToOtherSelectedItems = 9 => "Move automation item edge - relative to other selected items",
        MoveAutomationItemEdgeRelativeToOtherSelectedItemsIgnoringSnap = 10 => "Move automation item edge - relative to other selected items ignoring snap",
    }
}

define_action_enum! {
    pub enum AutomationItemDoubleClickAction {
        NoAction = 0 => "No action",
        ShowAutomationItemProperties = 1 => "Show automation item properties",
        SetTimeSelectionToItem = 2 => "Set time selection to item",
        SetLoopPointsToItem = 3 => "Set loop points to item",
        LoadAutomationItem = 4 => "Load automation item",
    }
}

fn get_automation_item_left_drag_action_name(action_id: u32) -> String {
    AutomationItemLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_automation_item_edge_action_name(action_id: u32) -> String {
    AutomationItemEdgeAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_automation_item_double_click_action_name(action_id: u32) -> String {
    AutomationItemDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// ============================================================================
// Cursor Handle Action Mappings
// ============================================================================

define_action_enum! {
    pub enum CursorHandleAction {
        NoAction = 0 => "No action",
        ScrubAudio = 1 => "Scrub audio",
        JogAudio = 2 => "Jog audio",
        ScrubAudioLoopedSegment = 3 => "Scrub audio (looped-segment mode)",
        JogAudioLoopedSegment = 4 => "Jog audio (looped-segment mode)",
    }
}

pub fn get_cursor_handle_action_name(action_id: u32) -> String {
    CursorHandleAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy function - kept for backward compatibility
pub fn get_cursor_handle_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Scrub audio".to_string(),
        2 => "Jog audio".to_string(),
        3 => "Scrub audio (looped-segment mode)".to_string(),
        4 => "Jog audio (looped-segment mode)".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Envelope Action Mappings
// ============================================================================

define_action_enum! {
    pub enum EnvelopeControlPanelDoubleClickAction {
        NoAction = 0 => "No action",
        SelectUnselectAllEnvelopePoints = 1 => "Select/unselect all envelope points",
    }
}

define_action_enum! {
    pub enum EnvelopeLaneLeftDragAction {
        NoAction = 0 => "No action",
        DeselectAllEnvelopePoints = 1 => "Deselect all envelope points",
        DeselectAllEnvelopePointsAndMoveEditCursor = 2 => "Deselect all envelope points and move edit cursor",
    }
}

define_action_enum! {
    pub enum EnvelopePointLeftDragAction {
        NoAction = 0 => "No action",
        MoveEnvelopePoint = 1 => "Move envelope point",
        MoveEnvelopePointIgnoringSnap = 2 => "Move envelope point ignoring snap",
        FreehandDrawEnvelope = 3 => "Freehand draw envelope",
        DeleteEnvelopePoint = 4 => "Delete envelope point",
        MoveEnvelopePointVerticallyFine = 5 => "Move envelope point vertically (fine)",
        MoveEnvelopePointOnOneAxisOnly = 6 => "Move envelope point on one axis only",
        MoveEnvelopePointOnOneAxisOnlyIgnoringSnap = 7 => "Move envelope point on one axis only ignoring snap",
        CopyEnvelopePoint = 8 => "Copy envelope point",
        CopyEnvelopePointIgnoringSnap = 9 => "Copy envelope point ignoring snap",
        MoveEnvelopePointHorizontally = 10 => "Move envelope point horizontally",
        MoveEnvelopePointHorizontallyIgnoringSnap = 11 => "Move envelope point horizontally ignoring snap",
        MoveEnvelopePointVertically = 12 => "Move envelope point vertically",
    }
}

define_action_enum! {
    pub enum EnvelopePointDoubleClickAction {
        NoAction = 0 => "No action",
        ResetPointToDefaultValue = 1 => "Reset point to default value",
        OpenEnvelopePointEditor = 2 => "Open envelope point editor",
    }
}

define_action_enum! {
    pub enum EnvelopeSegmentLeftDragAction {
        NoAction = 0 => "No action",
        MoveEnvelopeSegmentIgnoringTimeSelection = 1 => "Move envelope segment ignoring time selection",
        InsertEnvelopePointDragToMove = 2 => "Insert envelope point, drag to move",
        FreehandDrawEnvelope = 3 => "Freehand draw envelope",
        MoveEnvelopeSegmentFine = 4 => "Move envelope segment (fine)",
        EditEnvelopeSegmentCurvature = 5 => "Edit envelope segment curvature",
        MoveEnvelopeSegment = 6 => "Move envelope segment",
        MoveEnvelopeSegmentPreservingEdgePoints = 7 => "Move envelope segment preserving edge points",
        EditEnvelopeSegmentCurvatureGangSelectedPoints = 8 => "Edit envelope segment curvature (gang selected points)",
    }
}

define_action_enum! {
    pub enum EnvelopeSegmentDoubleClickAction {
        NoAction = 0 => "No action",
        ResetEnvelopeSegmentCurvature = 1 => "Reset envelope segment curvature",
        AddEnvelopePoint = 2 => "Add envelope point",
    }
}

fn get_envelope_control_panel_double_click_action_name(action_id: u32) -> String {
    EnvelopeControlPanelDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_envelope_lane_left_drag_action_name(action_id: u32) -> String {
    EnvelopeLaneLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_envelope_lane_double_click_action_name(action_id: u32) -> String {
    // According to docs, MM_CTX_ENVLANE doesn't have double click, but we'll provide a fallback
    format!("Action {} m", action_id)
}

fn get_envelope_point_left_drag_action_name(action_id: u32) -> String {
    EnvelopePointLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_envelope_point_double_click_action_name(action_id: u32) -> String {
    EnvelopePointDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_envelope_segment_left_drag_action_name(action_id: u32) -> String {
    EnvelopeSegmentLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_envelope_segment_double_click_action_name(action_id: u32) -> String {
    EnvelopeSegmentDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// ============================================================================
// Fixed Lane Action Mappings
// ============================================================================

define_action_enum! {
    pub enum FixedLaneHeaderButtonClickAction {
        NoAction = 0 => "No action",
        SelectItemsInLane = 1 => "Select items in lane",
        ToggleSelectionOfItemsInLane = 2 => "Toggle selection of items in lane",
        PlayOnlyThisLane = 3 => "Play only this lane",
        PlayNoLanes = 4 => "Play no lanes",
        TogglePlayingThisLane = 5 => "Toggle playing this lane",
        PlayOnlyThisLaneWhileMouseButtonIsPressed = 6 => "Play only this lane while mouse button is pressed",
        RecordIntoThisLane = 7 => "Record into this lane",
        PlayAllLanes = 8 => "Play all lanes",
        CompIntoLane = 9 => "Comp into lane",
        CompIntoANewCopyOfLane = 10 => "Comp into a new copy of lane",
        CompIntoANewEmptyLane = 11 => "Comp into a new empty lane",
        InsertLane = 12 => "Insert lane",
        DeleteLaneIncludingMediaItems = 13 => "Delete lane (including media items)",
        CompIntoANewEmptyLaneAutomaticallyCreatingCompAreas = 14 => "Comp into a new empty lane, automatically creating comp areas",
        CopyEditedMediaItemsToNewLaneAndReComp = 15 => "Copy edited media items to new lane and re-comp",
    }
}

define_action_enum! {
    pub enum FixedLaneLinkedLaneLeftDragAction {
        NoAction = 0 => "No action",
        MoveCompAreaVertically = 1 => "Move comp area vertically",
        MoveCompAreaIgnoringSnap = 2 => "Move comp area ignoring snap",
        MoveCompAreaHorizontally = 3 => "Move comp area horizontally",
        MoveCompAreaHorizontallyIgnoringSnap = 4 => "Move comp area horizontally ignoring snap",
        MoveCompAreaVerticallyDuplicate = 5 => "Move comp area vertically", // Duplicate in docs?
        MoveCompAreaOnOneAxisOnly = 6 => "Move comp area on one axis only",
        MoveCompAreaAndMediaItems = 7 => "Move comp area and media items",
        MoveCompAreaAndMediaItemsIgnoringSnap = 8 => "Move comp area and media items ignoring snap",
        MoveCompAreaAndMediaItemsHorizontally = 9 => "Move comp area and media items horizontally",
        MoveCompAreaAndMediaItemsHorizontallyIgnoringSnap = 10 => "Move comp area and media items horizontally ignoring snap",
        MoveCompAreaAndMediaItemsVertically = 11 => "Move comp area and media items vertically",
        MoveCompAreaAndMediaItemsOnOneAxisOnly = 12 => "Move comp area and media items on one axis only",
        AddCompArea = 13 => "Add comp area",
        AddCompAreaIgnoringSnap = 14 => "Add comp area ignoring snap",
        CopyCompAreaAndMediaItemsTogether = 15 => "Copy comp area and media items together",
        CopyCompAreaAndMediaItemsTogetherIgnoringSnap = 16 => "Copy comp area and media items together ignoring snap",
        MoveCompAreaAndMediaItemsOnAllLanes = 17 => "Move comp area and media items on all lanes",
        MoveCompAreaAndMediaItemsOnAllLanesIgnoringSnap = 18 => "Move comp area and media items on all lanes ignoring snap",
        CopyCompAreaAndMediaItemsOnAllLanesTogether = 19 => "Copy comp area and media items on all lanes together",
        CopyCompAreaAndMediaItemsOnAllLanesTogetherIgnoringSnap = 20 => "Copy comp area and media items on all lanes together ignoring snap",
        MoveCompAreaAndAdjacentCompAreaEdges = 21 => "Move comp area and adjacent comp area edges",
        MoveCompAreaAndAdjacentCompAreaEdgesIgnoringSnap = 22 => "Move comp area and adjacent comp area edges ignoring snap",
        MoveCompAreaAndAdjacentCompAreaEdgesAndMediaItems = 23 => "Move comp area and adjacent comp area edges and media items",
        MoveCompAreaAndAdjacentCompAreaEdgesAndMediaItemsIgnoringSnap = 24 => "Move comp area and adjacent comp area edges and media items ignoring snap",
        MoveCompAreaAndAdjacentCompAreaEdgesHorizontally = 25 => "Move comp area and adjacent comp area edges horizontally",
        MoveCompAreaAndAdjacentCompAreaEdgesHorizontallyIgnoringSnap = 26 => "Move comp area and adjacent comp area edges horizontally ignoring snap",
        MoveCompAreaAndAdjacentCompAreaEdgesOnOneAxisOnly = 27 => "Move comp area and adjacent comp area edges on one axis only",
    }
}

define_action_enum! {
    pub enum FixedLaneLinkedLaneClickAction {
        NoAction = 0 => "No action",
        MoveCompAreaUp = 1 => "Move comp area up",
        MoveCompAreaDown = 2 => "Move comp area down",
        DeleteCompAreaButNotMediaItems = 3 => "Delete comp area but not media items",
        SetLoopPointsToCompArea = 4 => "Set loop points to comp area",
        MoveCompAreaToLaneUnderMouse = 5 => "Move comp area to lane under mouse",
        HealCompAreaWithAdjacentCompAreasOnTheSameLane = 6 => "Heal comp area with adjacent comp areas on the same lane",
        SplitMediaItemsAtCompAreaEdges = 7 => "Split media items at comp area edges",
        ExtendCompAreaToNextCompAreaOrEndOfMedia = 8 => "Extend comp area to next comp area or end of media",
        DeleteCompArea = 9 => "Delete comp area",
        SplitCompArea = 10 => "Split comp area",
        SplitCompAreaIgnoringSnap = 11 => "Split comp area ignoring snap",
        SetLoopPointsToCompAreaHalfSecondPrerollPostroll = 12 => "Set loop points to comp area (half second preroll/postroll)",
        SetLoopPointsToCompAreaOneSecondPrerollPostroll = 13 => "Set loop points to comp area (one second preroll/postroll)",
        CopyEditedMediaItemsToNewLaneAndReComp = 14 => "Copy edited media items to new lane and re-comp",
    }
}

define_action_enum! {
    pub enum FixedLaneLinkedLaneDoubleClickAction {
        NoAction = 0 => "No action",
        SetLoopPointsToCompArea = 1 => "Set loop points to comp area",
        ExtendCompAreaToNextCompAreaOrEndOfMedia = 2 => "Extend comp area to next comp area or end of media",
        SplitMediaItemsAtCompAreaEdges = 3 => "Split media items at comp area edges",
        CopyEditedMediaItemsToNewLaneAndReComp = 4 => "Copy edited media items to new lane and re-comp",
        SetLoopPointsToCompAreaHalfSecondPrerollPostroll = 5 => "Set loop points to comp area (half second preroll/postroll)",
        SetLoopPointsToCompAreaOneSecondPrerollPostroll = 6 => "Set loop points to comp area (one second preroll/postroll)",
    }
}

define_action_enum! {
    pub enum FixedLaneHeaderButtonDoubleClickAction {
        NoAction = 0 => "No action",
        SelectItemsInLane = 1 => "Select items in lane",
        ToggleSelectionOfItemsInLane = 2 => "Toggle selection of items in lane",
        PlayOnlyThisLane = 3 => "Play only this lane",
        PlayNoLanes = 4 => "Play no lanes",
        TogglePlayingThisLane = 5 => "Toggle playing this lane",
        RecordIntoThisLane = 6 => "Record into this lane",
        PlayAllLanes = 7 => "Play all lanes",
        CompIntoLane = 8 => "Comp into lane",
        CompIntoANewCopyOfLane = 9 => "Comp into a new copy of lane",
        CompIntoANewEmptyLane = 10 => "Comp into a new empty lane",
        InsertLane = 11 => "Insert lane",
        DeleteLaneIncludingMediaItems = 12 => "Delete lane (including media items)",
        CompIntoANewEmptyLaneAutomaticallyCreatingCompAreas = 13 => "Comp into a new empty lane, automatically creating comp areas",
        CopyEditedMediaItemsWithNoMatchingSourceLaneToNewLaneAndReComp = 14 => "Copy edited media items with no matching source lane to new lane and re-comp",
    }
}

fn get_fixed_lane_header_button_click_action_name(action_id: u32) -> String {
    FixedLaneHeaderButtonClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_fixed_lane_linked_lane_left_drag_action_name(action_id: u32) -> String {
    FixedLaneLinkedLaneLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_fixed_lane_linked_lane_click_action_name(action_id: u32) -> String {
    FixedLaneLinkedLaneClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_fixed_lane_linked_lane_double_click_action_name(action_id: u32) -> String {
    FixedLaneLinkedLaneDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_fixed_lane_header_button_double_click_action_name(action_id: u32) -> String {
    FixedLaneHeaderButtonDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// ============================================================================
// MIDI Action Mappings
// ============================================================================

fn get_midi_cc_event_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move CC event".to_string(),
        2 => "Move CC event ignoring snap".to_string(),
        3 => "Erase CC event".to_string(),
        4 => "Move CC event on one axis only".to_string(),
        5 => "Move CC event on one axis only ignoring snap".to_string(),
        6 => "Copy CC event".to_string(),
        7 => "Copy CC event ignoring snap".to_string(),
        8 => "Move CC horizontally".to_string(),
        9 => "Move CC horizontally ignoring snap".to_string(),
        10 => "Move CC vertically".to_string(),
        11 => "Marquee select CC".to_string(),
        12 => "Marquee toggle CC selection".to_string(),
        13 => "Marquee add to CC selection".to_string(),
        14 => "Marquee select CC and time".to_string(),
        15 => "Marquee select CC and time ignoring snap".to_string(),
        16 => "Select time".to_string(),
        17 => "Select time ignoring snap".to_string(),
        18 => "Draw/edit CC events ignoring selection".to_string(),
        19 => "Linear ramp CC events ignoring selection".to_string(),
        20 => "Draw/edit CC events ignoring snap and selection".to_string(),
        21 => "Edit selected CC events if any, otherwise draw/edit ignoring snap".to_string(),
        22 => "Edit CC events ignoring selection".to_string(),
        23 => "Edit CC events".to_string(),
        24 => "Linear ramp CC events".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_cc_event_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Insert CC event".to_string(),
        2 => "Insert CC event ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_cc_lane_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Draw/edit CC events ignoring selection".to_string(),
        2 => "Edit selected CC events if any, otherwise draw/edit".to_string(),
        3 => "Erase CC events".to_string(),
        4 => "Linear ramp CC events ignoring selection".to_string(),
        5 => "Linear ramp CC events".to_string(),
        6 => "Draw/edit CC events ignoring snap and selection".to_string(),
        7 => "Edit CC events ignoring selection".to_string(),
        8 => "Edit selected CC events if any, otherwise draw/edit ignoring snap".to_string(),
        9 => "Marquee select CC".to_string(),
        10 => "Marquee toggle CC selection".to_string(),
        11 => "Marquee add to CC selection".to_string(),
        12 => "Marquee select CC and time".to_string(),
        13 => "Marquee select CC and time ignoring snap".to_string(),
        14 => "Select time".to_string(),
        15 => "Select time ignoring snap".to_string(),
        16 => "Edit CC events".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_cc_lane_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Insert CC event".to_string(),
        2 => "Insert CC event ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_cc_segment_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move CC segment ignoring time selection".to_string(),
        2 => "Insert CC event".to_string(),
        3 => "Insert CC event ignoring snap".to_string(),
        4 => "Draw/edit CC events ignoring selection".to_string(),
        5 => "Draw/edit CC events ignoring snap and selection".to_string(),
        7 => "Edit CC segment curvature".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_cc_segment_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Reset CC segment curvature".to_string(),
        2 => "Add CC event".to_string(),
        3 => "Add CC event ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_end_pointer_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Edit MIDI source loop length".to_string(),
        2 => "Edit MIDI source loop length ignoring snap".to_string(),
        3 => "Stretch MIDI source loop length".to_string(),
        4 => "Stretch MIDI source loop length ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_marker_lanes_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Hand scroll".to_string(),
        2 => "Hand scroll and horizontal zoom".to_string(),
        4 => "Horizontal zoom".to_string(),
        6 => "Set edit cursor and horizontal zoom".to_string(),
        8 => "Set edit cursor, hand scroll and horizontal zoom".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_note_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move note".to_string(),
        2 => "Move note ignoring snap".to_string(),
        3 => "Erase notes".to_string(),
        4 => "Select time".to_string(),
        5 => "Move note on one axis only".to_string(),
        6 => "Move note on one axis only ignoring snap".to_string(),
        7 => "Copy note".to_string(),
        8 => "Copy note ignoring snap".to_string(),
        9 => "Edit note velocity".to_string(),
        10 => "Edit note velocity (fine)".to_string(),
        11 => "Move note horizontally".to_string(),
        12 => "Move note horizontally ignoring snap".to_string(),
        13 => "Move note vertically".to_string(),
        14 => "Select time ignoring snap".to_string(),
        15 => "Marquee select notes".to_string(),
        16 => "Marquee toggle note selection".to_string(),
        17 => "Marquee add to note selection".to_string(),
        18 => "Marquee select notes and time".to_string(),
        19 => "Marquee select notes and time ignoring snap".to_string(),
        20 => "Stretch note positions ignoring snap (arpeggiate)".to_string(),
        21 => "Stretch note selection vertically (arpeggiate)".to_string(),
        22 => "Stretch note selection vertically (arpeggiate)".to_string(), // Duplicate in docs?
        23 => "Stretch note lengths ignoring snap (arpeggiator legato)".to_string(),
        24 => "Stretch note lengths (arpeggiate legato)".to_string(),
        25 => "Copy note horizontally".to_string(),
        26 => "Copy note horizontally ignoring snap".to_string(),
        27 => "Copy note vertically".to_string(),
        28 => "Select notes touched while dragging".to_string(),
        29 => "Toggle selection for notes touched while dragging".to_string(),
        30 => "Move note ignoring selection".to_string(),
        31 => "Move note ignoring snap and selection".to_string(),
        32 => "Move note vertically ignoring scale/key".to_string(),
        33 => "Move note vertically ignoring scale/key".to_string(), // Duplicate in docs?
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_note_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Select note".to_string(),
        2 => "Select note and move edit cursor".to_string(),
        3 => "Select note and move edit cursor ignoring snap".to_string(),
        4 => "Toggle note selection".to_string(),
        5 => "Add note to selection".to_string(),
        6 => "Erase note".to_string(),
        7 => "Toggle note mute".to_string(),
        8 => "Set note channel higher".to_string(),
        9 => "Set note channel lower".to_string(),
        10 => "Double note length".to_string(),
        11 => "Halve note length".to_string(),
        12 => "Select note and all later notes".to_string(),
        13 => "Add note and all later notes to selection".to_string(),
        14 => "Select note and all later notes of same pitch".to_string(),
        15 => "Add note and all later notes of same pitch to selection".to_string(),
        16 => "Select all notes in measure".to_string(),
        17 => "Add all notes in measure to selection".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_note_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Erase note".to_string(),
        2 => "Activate MIDI item (when clicking a note that is not in the active item)".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_note_edge_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move note edge".to_string(),
        2 => "Move note edge ignoring snap".to_string(),
        3 => "Stretch notes".to_string(),
        4 => "Stretch notes ignoring snap".to_string(),
        5 => "Move note edge ignoring selection".to_string(),
        6 => "Move note edge ignoring snap and selection".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_piano_roll_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Insert note, drag to extend or change pitch".to_string(),
        2 => "Insert note ignoring snap, drag to extend or change pitch".to_string(),
        3 => "Erase notes".to_string(),
        4 => "Select time".to_string(),
        5 => "Paint notes and chords".to_string(),
        6 => "Select time ignoring snap".to_string(),
        7 => "Marquee select notes".to_string(),
        8 => "Marquee toggle note selection".to_string(),
        9 => "Marquee add to note selection".to_string(),
        10 => "Marquee select notes and time".to_string(),
        11 => "Marquee select notes and time ignoring snap".to_string(),
        12 => "Insert note, drag to move".to_string(),
        13 => "Paint a row of notes of the same pitch".to_string(),
        14 => "Insert note, drag to extend".to_string(),
        15 => "Insert note ignoring snap, drag to extend".to_string(),
        16 => "Scrub preview MIDI".to_string(),
        17 => "Insert note ignoring snap, drag to move".to_string(),
        18 => "Insert note ignoring snap, drag to edit velocity".to_string(),
        19 => "Insert note, drag to edit velocity".to_string(),
        20 => "Paint a stack of notes of the same time position".to_string(),
        21 => "Paint notes ignoring snap".to_string(),
        22 => "Paint notes".to_string(),
        23 => "Paint a straight line of notes".to_string(),
        24 => "Paint a straight line of notes ignoring snap".to_string(),
        25 => "Select notes touched while dragging".to_string(),
        26 => "Toggle selection for notes touched while dragging".to_string(),
        27 => "Copy selected notes".to_string(),
        28 => "Copy selected notes ignoring snap".to_string(),
        29 => "Move selected notes".to_string(),
        30 => "Move selected notes ignoring snap".to_string(),
        31 => "Insert note".to_string(),
        32 => "Insert note ignoring snap".to_string(),
        33 => "Insert note ignoring scale/key, drag to move".to_string(),
        34 => "Insert note ignoring snap and scale/key, drag to move".to_string(),
        35 => "Insert note ignoring scale/key, drag to extend or change pitch".to_string(),
        36 => "Insert note ignoring snap and scale/key, drag to extend or change pitch".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_piano_roll_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Deselect all notes and move edit cursor".to_string(),
        2 => "Deselect all notes and move edit cursor ignoring snap".to_string(),
        3 => "Deselect all notes".to_string(),
        4 => "Insert note".to_string(),
        5 => "Insert note ignoring snap".to_string(),
        6 => "Set draw channel higher".to_string(),
        7 => "Set draw channel lower".to_string(),
        8 => "Insert note, leaving other notes selected".to_string(),
        9 => "Insert note ignoring snap, leaving other notes selected".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_piano_roll_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Insert note".to_string(),
        2 => "Insert note ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_right_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Marquee select notes/CC".to_string(),
        2 => "Marquee select notes/CC and time".to_string(),
        3 => "Marquee select notes/CC and time ignoring snap".to_string(),
        4 => "Select time".to_string(),
        5 => "Select time ignoring snap".to_string(),
        6 => "Erase notes/CC".to_string(),
        7 => "Marquee toggle note/CC selection".to_string(),
        8 => "Marquee add to notes/CC selection".to_string(),
        9 => "Hand scroll".to_string(),
        10 => "Erase notes/CC immediately (suppresses right-click context menu)".to_string(),
        11 => "Select notes touched while dragging".to_string(),
        12 => "Toggle selection for notes touched while dragging".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_ruler_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Edit loop point (ruler) or time selection (piano roll)".to_string(),
        2 => "Edit loop point (ruler) or time selection (piano roll) ignoring snap".to_string(),
        3 => "Move loop points (ruler) or time selection (piano roll)".to_string(),
        4 => "Move loop points (ruler) or time selection together".to_string(),
        5 => "Edit loop point and time selection together".to_string(),
        6 => "Edit loop point and time selection together ignoring snap".to_string(),
        7 => "Move loop points and time selection together".to_string(),
        8 => "Move loop points and time selection together ignoring snap".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_ruler_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move edit cursor".to_string(),
        2 => "Move edit cursor ignoring snap".to_string(),
        3 => "Select notes or CC in time selection".to_string(),
        4 => "Clear loop or time selection".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

fn get_midi_ruler_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Zoom to selected notes/CC".to_string(),
        2 => "File: Close window".to_string(),
        _ => format!("Action {} e", action_id),
    }
}

// ============================================================================
// Media Item Action Mappings
// ============================================================================

define_action_enum! {
    pub enum MediaItemLeftDragAction {
        NoAction = 0 => "No action",
        MoveItem = 1 => "Move item",
        CopyItem = 2 => "Copy item",
        MoveItemContents = 3 => "Move item contents",
        MoveItemIgnoringSnap = 4 => "Move item ignoring snap",
        CopyItemIgnoringSnap = 5 => "Copy item ignoring snap",
        MoveItemEdgesButNotContent = 6 => "Move item edges but not content",
        RenderItemToNewFile = 7 => "Render item to new file",
        OpenSourceFileInEditorOrExternalApplication = 8 => "Open source file in editor or external application",
        MoveItemIgnoringSelectionGrouping = 9 => "Move item ignoring selection/grouping",
        MoveItemIgnoringSnapAndSelectionGrouping = 10 => "Move item ignoring snap and selection/grouping",
        AdjustTakePitchSemitones = 11 => "Adjust take pitch (semitones)",
        AdjustTakePitchFine = 12 => "Adjust take pitch (fine)",
        MoveItemIgnoringTimeSelection = 13 => "Move item ignoring time selection",
        MoveItemIgnoringSnapAndTimeSelection = 14 => "Move item ignoring snap and time selection",
        MoveItemIgnoringTimeSelectionAndSelectionGrouping = 15 => "Move item ignoring time selection and selection/grouping",
        MoveItemIgnoringSnapTimeSelectionAndSelectionGrouping = 16 => "Move item ignoring snap, time selection, and selection/grouping",
        CopyItemIgnoringTimeSelection = 17 => "Copy item ignoring time selection",
        CopyItemIgnoringSnapAndTimeSelection = 18 => "Copy item ignoring snap and time selection",
        SelectTime = 19 => "Select time",
        AdjustItemVolume = 20 => "Adjust item volume",
        AdjustItemVolumeFine = 21 => "Adjust item volume (fine)",
        MoveItemAndTimeSelection = 22 => "Move item and time selection",
        MoveItemAndTimeSelectionIgnoringSnap = 23 => "Move item and time selection ignoring snap",
        CopyItemAndMoveTimeSelection = 24 => "Copy item and move time selection",
        CopyItemAndMoveTimeSelectionIgnoringSnap = 25 => "Copy item and move time selection ignoring snap",
        SelectTimeIgnoringSnap = 26 => "Select time ignoring snap",
        MarqueeSelectItems = 27 => "Marquee select items",
        MarqueeSelectItemsAndTime = 28 => "Marquee select items and time",
        MarqueeSelectItemsAndTimeIgnoringSnap = 29 => "Marquee select items and time ignoring snap",
        MarqueeToggleItemSelection = 30 => "Marquee toggle item selection",
        MarqueeAddToItemSelection = 31 => "Marquee add to item selection",
        MoveItemVertically = 32 => "Move item vertically",
        MoveItemVerticallyIgnoringSelectionGrouping = 33 => "Move item vertically ignoring selection/grouping",
        MoveItemVerticallyIgnoringTimeSelection = 34 => "Move item vertically ignoring time selection",
        MoveItemVerticallyIgnoringTimeSelectionAndSelectionGrouping = 35 => "Move item vertically ignoring time selection and selection/grouping",
        CopyItemVertically = 36 => "Copy item vertically",
        CopyItemVerticallyIgnoringTimeSelection = 37 => "Copy item vertically ignoring time selection",
        MoveItemContentsIgnoringSelectionGrouping = 38 => "Move item contents ignoring selection/grouping",
        CopyItemPoolingMidiSourceData = 39 => "Copy item, pooling MIDI source data",
        CopyItemIgnoringSnapPoolingMidiSourceData = 40 => "Copy item ignoring snap, pooling MIDI source data",
        CopyItemIgnoringTimeSelectionPoolingMidiSourceData = 41 => "Copy item ignoring time selection, pooling MIDI source data",
        CopyItemIgnoringSnapAndTimeSelectionPoolingMidiSourceData = 42 => "Copy item ignoring snap and time selection, pooling MIDI source data",
        CopyItemVerticallyPoolingMidiSourceData = 43 => "Copy item vertically, pooling MIDI source data",
        CopyItemVerticallyIgnoringTimeSelectionPoolingMidiSourceData = 44 => "Copy item vertically ignoring time selection, pooling MIDI source data",
        CopyItemAndMoveTimeSelectionPoolingMidiSourceData = 45 => "Copy item and move time selection, pooling MIDI source data",
        CopyItemAndMoveTimeSelectionIgnoringSnapPoolingMidiSourceData = 46 => "Copy item and move time selection ignoring snap, pooling MIDI source data",
        AdjustTakePan = 47 => "Adjust take pan",
        MarqueeZoom = 48 => "Marquee zoom",
        MoveItemIgnoringTimeSelectionDisablingRippleEdit = 49 => "Move item ignoring time selection, disabling ripple edit",
        MoveItemIgnoringTimeSelectionEnablingRippleEditForThisTrack = 50 => "Move item ignoring time selection, enabling ripple edit for this track",
        MoveItemIgnoringTimeSelectionEnablingRippleEditForAllTracks = 51 => "Move item ignoring time selection, enabling ripple edit for all tracks",
        MoveItemIgnoringSnapAndTimeSelectionDisablingRippleEdit = 52 => "Move item ignoring snap and time selection, disabling ripple edit",
        MoveItemIgnoringSnapAndTimeSelectionEnablingRippleEditForThisTrack = 53 => "Move item ignoring snap and time selection, enabling ripple edit for this track",
        MoveItemIgnoringSnapAndTimeSelectionEnablingRippleEditForAllTracks = 54 => "Move item ignoring snap and time selection, enabling ripple edit for all tracks",
        MoveItemContentsRippleAllAdjacentItems = 55 => "Move item contents, ripple all adjacent items",
        MoveItemContentsRippleEarlierAdjacentItems = 56 => "Move item contents, ripple earlier adjacent items",
        MoveItemContentsAndRightEdgeRippleLaterAdjacentItems = 57 => "Move item contents and right edge, ripple later adjacent items",
        SelectRazorEditArea = 62 => "Select razor edit area",
        SelectRazorEditAreaIgnoringSnap = 63 => "Select razor edit area ignoring snap",
        AddToRazorEditArea = 64 => "Add to razor edit area",
        AddToRazorEditAreaIgnoringSnap = 65 => "Add to razor edit area ignoring snap",
        SelectRazorEditAreaAndTime = 66 => "Select razor edit area and time",
        SelectRazorEditAreaAndTimeIgnoringSnap = 67 => "Select razor edit area and time ignoring snap",
    }
}

define_action_enum! {
    pub enum MediaItemClickAction {
        NoAction = 0 => "No action",
        SelectItemAndMoveEditCursor = 1 => "Select item and move edit cursor",
        SelectItemAndMoveEditCursorIgnoringSnap = 2 => "Select item and move edit cursor ignoring snap",
        SelectItem = 3 => "Select item",
        ToggleItemSelection = 4 => "Toggle item selection",
        AddARangeOfItemsToSelection = 5 => "Add a range of items to selection",
        AddARangeOfItemsToSelectionIfAlreadySelectedExtendTimeSelection = 6 => "Add a range of items to selection, if already selected extend time selection",
        AddARangeOfItemsToSelectionIfAlreadySelectedExtendTimeSelectionIgnoringSnap = 7 => "Add a range of items to selection, if already selected extend time selection ignoring snap",
        ExtendTimeSelection = 8 => "Extend time selection",
        ExtendTimeSelectionIgnoringSnap = 9 => "Extend time selection ignoring snap",
        AddARangeOfItemsToSelectionAndExtendTimeSelection = 10 => "Add a range of items to selection and extend time selection",
        AddARangeOfItemsToSelectionAndExtendTimeSelectionIgnoringSnap = 11 => "Add a range of items to selection and extend time selection ignoring snap",
        SelectItemIgnoringGrouping = 12 => "Select item ignoring grouping",
        RestorePreviousZoomScroll = 13 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 14 => "Restore previous zoom level",
        AddStretchMarker = 15 => "Add stretch marker",
        ExtendRazorEditArea = 20 => "Extend razor edit area",
    }
}

define_action_enum! {
    pub enum MediaItemDoubleClickAction {
        NoAction = 0 => "No action",
        OpenMediaItemInExternalEditor = 1 => "Open media item in external editor",
        SetTimeSelectionToItem = 2 => "Set time selection to item",
        SetLoopPointsToItem = 3 => "Set loop points to item",
        ShowMediaItemProperties = 4 => "Show media item properties",
        ShowMediaItemSourceProperties = 5 => "Show media item source properties",
        MidiOpenInEditorSubprojectsOpenProjectAudioShowMediaItemProperties = 6 => "MIDI: open in editor, Subprojects: open project, Audio: show media item properties",
        ShowTakeList = 7 => "Show take list",
        ShowTakePropsList = 8 => "Show take props list",
    }
}

define_action_enum! {
    pub enum MediaItemEdgeLeftDragAction {
        NoAction = 0 => "No action",
        MoveEdge = 1 => "Move edge",
        StretchItem = 2 => "Stretch item",
        MoveEdgeIgnoringSnap = 3 => "Move edge ignoring snap",
        StretchItemIgnoringSnap = 4 => "Stretch item ignoring snap",
        MoveEdgeIgnoringSelectionGrouping = 5 => "Move edge ignoring selection/grouping",
        StretchItemIgnoringSelectionGrouping = 6 => "Stretch item ignoring selection/grouping",
        MoveEdgeIgnoringSnapAndSelectionGrouping = 7 => "Move edge ignoring snap and selection/grouping",
        StretchItemIgnoringSnapAndSelectionGrouping = 8 => "Stretch item ignoring snap and selection/grouping",
        MoveEdgeRelativeEdgeEdit = 9 => "Move edge (relative edge edit)",
        StretchItemRelativeEdgeEdit = 10 => "Stretch item (relative edge edit)",
        MoveEdgeIgnoringSnapRelativeEdgeEdit = 11 => "Move edge ignoring snap (relative edge edit)",
        StretchItemIgnoringSnapRelativeEdgeEdit = 12 => "Stretch item ignoring snap (relative edge edit)",
        MoveEdgeWithoutChangingFadeTime = 13 => "Move edge without changing fade time",
        MoveEdgeIgnoringSnapWithoutChangingFadeTime = 14 => "Move edge ignoring snap without changing fade time",
        MoveEdgeIgnoringSelectionGroupingWithoutChangingFadeTime = 15 => "Move edge ignoring selection/grouping without changing fade time",
        MoveEdgeIgnoringSnapAndSelectionGroupingWithoutChangingFadeTime = 16 => "Move edge ignoring snap and selection/grouping without changing fade time",
        MoveEdgeWithoutChangingFadeTimeRelativeEdgeEdit = 17 => "Move edge without changing fade time (relative edge edit)",
        MoveEdgeIgnoringSnapWithoutChangingFadeTimeRelativeEdgeEdit = 18 => "Move edge ignoring snap without changing fade time (relative edge edit)",
        SelectRazorEditArea = 21 => "Select razor edit area",
        SelectRazorEditAreaIgnoringSnap = 22 => "Select razor edit area ignoring snap",
        AddToRazorEditArea = 23 => "Add to razor edit area",
        AddToRazorEditAreaIgnoringSnap = 24 => "Add to razor edit area ignoring snap",
        SelectRazorEditAreaAndTime = 25 => "Select razor edit area and time",
        SelectRazorEditAreaAndTimeIgnoringSnap = 26 => "Select razor edit area and time ignoring snap",
    }
}

define_action_enum! {
    pub enum MediaItemEdgeDoubleClickAction {
        NoAction = 0 => "No action",
        NoActionDuplicate = 1 => "No action", // According to docs, both 0 and 1 are "no action"
    }
}

fn get_media_item_left_drag_action_name(action_id: u32) -> String {
    MediaItemLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_click_action_name(action_id: u32) -> String {
    MediaItemClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_double_click_action_name(action_id: u32) -> String {
    MediaItemDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_edge_left_drag_action_name(action_id: u32) -> String {
    MediaItemEdgeLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_edge_double_click_action_name(action_id: u32) -> String {
    MediaItemEdgeDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy function - kept for backward compatibility
fn get_media_item_left_drag_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move item".to_string(),
        2 => "Copy item".to_string(),
        3 => "Move item contents".to_string(),
        4 => "Move item ignoring snap".to_string(),
        5 => "Copy item ignoring snap".to_string(),
        6 => "Move item edges but not content".to_string(),
        7 => "Render item to new file".to_string(),
        8 => "Open source file in editor or external application".to_string(),
        9 => "Move item ignoring selection/grouping".to_string(),
        10 => "Move item ignoring snap and selection/grouping".to_string(),
        11 => "Adjust take pitch (semitones)".to_string(),
        12 => "Adjust take pitch (fine)".to_string(),
        13 => "Move item ignoring time selection".to_string(),
        14 => "Move item ignoring snap and time selection".to_string(),
        15 => "Move item ignoring time selection and selection/grouping".to_string(),
        16 => "Move item ignoring snap, time selection, and selection/grouping".to_string(),
        17 => "Copy item ignoring time selection".to_string(),
        18 => "Copy item ignoring snap and time selection".to_string(),
        19 => "Select time".to_string(),
        20 => "Adjust item volume".to_string(),
        21 => "Adjust item volume (fine)".to_string(),
        22 => "Move item and time selection".to_string(),
        23 => "Move item and time selection ignoring snap".to_string(),
        24 => "Copy item and move time selection".to_string(),
        25 => "Copy item and move time selection ignoring snap".to_string(),
        26 => "Select time ignoring snap".to_string(),
        27 => "Marquee select items".to_string(),
        28 => "Marquee select items and time".to_string(),
        29 => "Marquee select items and time ignoring snap".to_string(),
        30 => "Marquee toggle item selection".to_string(),
        31 => "Marquee add to item selection".to_string(),
        32 => "Move item vertically".to_string(),
        33 => "Move item vertically ignoring selection/grouping".to_string(),
        34 => "Move item vertically ignoring time selection".to_string(),
        35 => "Move item vertically ignoring time selection and selection/grouping".to_string(),
        36 => "Copy item vertically".to_string(),
        37 => "Copy item vertically ignoring time selection".to_string(),
        38 => "Move item contents ignoring selection/grouping".to_string(),
        39 => "Copy item, pooling MIDI source data".to_string(),
        40 => "Copy item ignoring snap, pooling MIDI source data".to_string(),
        41 => "Copy item ignoring time selection, pooling MIDI source data".to_string(),
        42 => "Copy item ignoring snap and time selection, pooling MIDI source data".to_string(),
        43 => "Copy item vertically, pooling MIDI source data".to_string(),
        44 => "Copy item vertically ignoring time selection, pooling MIDI source data".to_string(),
        45 => "Copy item and move time selection, pooling MIDI source data".to_string(),
        46 => {
            "Copy item and move time selection ignoring snap, pooling MIDI source data".to_string()
        }
        47 => "Adjust take pan".to_string(),
        48 => "Marquee zoom".to_string(),
        49 => "Move item ignoring time selection, disabling ripple edit".to_string(),
        50 => "Move item ignoring time selection, enabling ripple edit for this track".to_string(),
        51 => "Move item ignoring time selection, enabling ripple edit for all tracks".to_string(),
        52 => "Move item ignoring snap and time selection, disabling ripple edit".to_string(),
        53 => "Move item ignoring snap and time selection, enabling ripple edit for this track"
            .to_string(),
        54 => "Move item ignoring snap and time selection, enabling ripple edit for all tracks"
            .to_string(),
        55 => "Move item contents, ripple all adjacent items".to_string(),
        56 => "Move item contents, ripple earlier adjacent items".to_string(),
        57 => "Move item contents and right edge, ripple later adjacent items".to_string(),
        62 => "Select razor edit area".to_string(),
        63 => "Select razor edit area ignoring snap".to_string(),
        64 => "Add to razor edit area".to_string(),
        65 => "Add to razor edit area ignoring snap".to_string(),
        66 => "Select razor edit area and time".to_string(),
        67 => "Select razor edit area and time ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Select item and move edit cursor".to_string(),
        2 => "Select item and move edit cursor ignoring snap".to_string(),
        3 => "Select item".to_string(),
        4 => "Toggle item selection".to_string(),
        5 => "Add a range of items to selection".to_string(),
        6 => "Add a range of items to selection, if already selected extend time selection".to_string(),
        7 => "Add a range of items to selection, if already selected extend time selection ignoring snap".to_string(),
        8 => "Extend time selection".to_string(),
        9 => "Extend time selection ignoring snap".to_string(),
        10 => "Add a range of items to selection and extend time selection".to_string(),
        11 => "Add a range of items to selection and extend time selection ignoring snap".to_string(),
        12 => "Select item ignoring grouping".to_string(),
        13 => "Restore previous zoom/scroll".to_string(),
        14 => "Restore previous zoom level".to_string(),
        15 => "Add stretch marker".to_string(),
        20 => "Extend razor edit area".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Open media item in external editor".to_string(),
        2 => "Set time selection to item".to_string(),
        3 => "Set loop points to item".to_string(),
        4 => "Show media item properties".to_string(),
        5 => "Show media item source properties".to_string(),
        6 => "MIDI: open in editor, Subprojects: open project, Audio: show media item properties"
            .to_string(),
        7 => "Show take list".to_string(),
        8 => "Show take props list".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_edge_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move edge".to_string(),
        2 => "Stretch item".to_string(),
        3 => "Move edge ignoring snap".to_string(),
        4 => "Stretch item ignoring snap".to_string(),
        5 => "Move edge ignoring selection/grouping".to_string(),
        6 => "Stretch item ignoring selection/grouping".to_string(),
        7 => "Move edge ignoring snap and selection/grouping".to_string(),
        8 => "Stretch item ignoring snap and selection/grouping".to_string(),
        9 => "Move edge (relative edge edit)".to_string(),
        10 => "Stretch item (relative edge edit)".to_string(),
        11 => "Move edge ignoring snap (relative edge edit)".to_string(),
        12 => "Stretch item ignoring snap (relative edge edit)".to_string(),
        13 => "Move edge without changing fade time".to_string(),
        14 => "Move edge ignoring snap without changing fade time".to_string(),
        15 => "Move edge ignoring selection/grouping without changing fade time".to_string(),
        16 => {
            "Move edge ignoring snap and selection/grouping without changing fade time".to_string()
        }
        17 => "Move edge without changing fade time (relative edge edit)".to_string(),
        18 => "Move edge ignoring snap without changing fade time (relative edge edit)".to_string(),
        21 => "Select razor edit area".to_string(),
        22 => "Select razor edit area ignoring snap".to_string(),
        23 => "Add to razor edit area".to_string(),
        24 => "Add to razor edit area ignoring snap".to_string(),
        25 => "Select razor edit area and time".to_string(),
        26 => "Select razor edit area and time ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_edge_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "No action".to_string(), // According to docs, both 0 and 1 are "no action"
        _ => format!("Action {} m", action_id),
    }
}

define_action_enum! {
    pub enum MediaItemFadeLeftDragAction {
        MoveFadeIgnoringSnap = 0 => "Move fade ignoring snap",
        MoveFadeIgnoringSnapDuplicate = 1 => "Move fade ignoring snap",
        MoveCrossfadeIgnoringSnap = 2 => "Move crossfade ignoring snap",
        MoveFadeAndStretchCrossfadedItemsIgnoringSnap = 3 => "Move fade and stretch crossfaded items ignoring snap",
        MoveCrossfadeAndStretchCrossfadedItemsIgnoringSnap = 4 => "Move crossfade and stretch crossfaded items ignoring snap",
        MoveFadeIgnoringSnapAndSelectionGrouping = 5 => "Move fade ignoring snap and selection/grouping",
        MoveCrossfadeIgnoringSnapAndSelectionGrouping = 6 => "Move crossfade ignoring snap and selection/grouping",
        MoveFadeAndStretchCrossfadedItemsIgnoringSnapAndSelectionGrouping = 7 => "Move fade and stretch crossfaded items ignoring snap and selection/grouping",
        MoveCrossfadeAndStretchItemsIgnoringSnapAndSelectionGrouping = 8 => "Move crossfade and stretch items ignoring snap and selection/grouping",
        MoveFadeIgnoringSnapRelativeEdgeEdit = 9 => "Move fade ignoring snap (relative edge edit)",
        MoveCrossfadeIgnoringSnapRelativeEdgeEdit = 10 => "Move crossfade ignoring snap (relative edge edit)",
        MoveFadeAndStretchCrossfadedItemsIgnoringSnapRelativeEdgeEdit = 11 => "Move fade and stretch crossfaded items ignoring snap (relative edge edit)",
        MoveCrossfadeAndStretchItemsIgnoringSnapRelativeEdgeEdit = 12 => "Move crossfade and stretch items ignoring snap (relative edge edit)",
        AdjustFadeCurve = 13 => "Adjust fade curve",
        AdjustFadeCurveIgnoringSelectionGrouping = 14 => "Adjust fade curve ignoring selection/grouping",
        AdjustFadeCurveIgnoringCrossfadedItems = 15 => "Adjust fade curve ignoring crossfaded items",
        AdjustFadeCurveIgnoringCrossfadedItemsAndSelectionGrouping = 16 => "Adjust fade curve ignoring crossfaded items and selection/grouping",
        MoveFadeIgnoringSnapAndCrossfadedItem = 17 => "Move fade ignoring snap and crossfaded item",
        MoveFadeIgnoringSnapSelectionGroupingAndCrossfadedItems = 18 => "Move fade ignoring snap, selection/grouping, and crossfaded items",
        MoveFadeIgnoringSnapAndCrossfadedItemsRelativeEdgeEdit = 19 => "Move fade ignoring snap and crossfaded items (relative edge edit)",
        MoveFade = 20 => "Move fade",
        MoveCrossfade = 21 => "Move crossfade",
        MoveFadeAndStretchCrossfadedItems = 22 => "Move fade and stretch crossfaded items",
        MoveCrossfadeAndStretchItems = 23 => "Move crossfade and stretch items",
        MoveFadeIgnoringSelectionGrouping = 24 => "Move fade ignoring selection/grouping",
        MoveCrossfadeIgnoringSelectionGrouping = 25 => "Move crossfade ignoring selection/grouping",
        MoveFadeAndStretchCrossfadedItemsIgnoringSelectionGrouping = 26 => "Move fade and stretch crossfaded items ignoring selection/grouping",
        MoveCrossfadeAndStretchItemsIgnoringSelectionGrouping = 27 => "Move crossfade and stretch items ignoring selection/grouping",
        MoveFadeRelativeEdgeEdit = 28 => "Move fade (relative edge edit)",
        MoveCrossfadeRelativeEdgeEdit = 29 => "Move crossfade (relative edge edit)",
        MoveFadeAndStretchCrossfadedItemsRelativeEdgeEdit = 30 => "Move fade and stretch crossfaded items (relative edge edit)",
        MoveCrossfadeAndStretchItemsRelativeEdgeEdit = 31 => "Move crossfade and stretch items (relative edge edit)",
        MoveFadeIgnoringCrossfadedItems = 32 => "Move fade ignoring crossfaded items",
        MoveFadeIgnoringCrossfadedItemsAndSelectionGrouping = 33 => "Move fade ignoring crossfaded items and selection/grouping",
        MoveFadeIgnoringCrossfadedItemsRelativeEdgeEdit = 34 => "Move fade ignoring crossfaded items (relative edge edit)",
    }
}

define_action_enum! {
    pub enum MediaItemFadeClickAction {
        NoAction = 0 => "No action",
        DeleteFadeCrossfade = 1 => "Delete fade/crossfade",
        DeleteFadeCrossfadeIgnoringSelection = 2 => "Delete fade/crossfade ignoring selection",
        SetFadeCrossfadeToNextShape = 3 => "Set fade/crossfade to next shape",
        SetFadeCrossfadeToPreviousShape = 4 => "Set fade/crossfade to previous shape",
        SetFadeCrossfadeToPreviousShapeIgnoringSelection = 5 => "Set fade/crossfade to previous shape ignoring selection",
        SetFadeCrossfadeToNextShapeIgnoringSelection = 6 => "Set fade/crossfade to next shape ignoring selection",
        OpenCrossfadeEditor = 7 => "Open crossfade editor",
    }
}

define_action_enum! {
    pub enum MediaItemFadeDoubleClickAction {
        NoAction = 0 => "No action",
        DeleteItemFadeCrossfade = 1 => "Delete item fade/crossfade",
        OpenCrossfadeEditor = 2 => "Open crossfade editor",
    }
}

define_action_enum! {
    pub enum MediaItemLowerLeftDragAction {
        PassThroughToItemDragContext = 0 => "Pass through to item drag context",
        MoveItem = 1 => "Move item",
        CopyItem = 2 => "Copy item",
        MoveItemContents = 3 => "Move item contents",
        MoveItemIgnoringSnap = 4 => "Move item ignoring snap",
        CopyItemIgnoringSnap = 5 => "Copy item ignoring snap",
        MoveItemEdgesButNotContent = 6 => "Move item edges but not content",
        RenderItemToNewFile = 7 => "Render item to new file",
        OpenSourceFileInEditorOrExternalApplication = 8 => "Open source file in editor or external application",
        MoveItemIgnoringSelectionGrouping = 9 => "Move item ignoring selection/grouping",
        MoveItemIgnoringSnapAndSelectionGrouping = 10 => "Move item ignoring snap and selection/grouping",
        AdjustTakePitchSemitones = 11 => "Adjust take pitch (semitones)",
        AdjustTakePitchFine = 12 => "Adjust take pitch (fine)",
        MoveItemIgnoringTimeSelection = 13 => "Move item ignoring time selection",
        MoveItemIgnoringSnapAndTimeSelection = 14 => "Move item ignoring snap and time selection",
        MoveItemIgnoringTimeSelectionAndSelectionGrouping = 15 => "Move item ignoring time selection and selection/grouping",
        MoveItemIgnoringSnapTimeSelectionAndSelectionGrouping = 16 => "Move item ignoring snap, time selection, and selection/grouping",
        CopyItemIgnoringTimeSelection = 17 => "Copy item ignoring time selection",
        CopyItemIgnoringSnapAndTimeSelection = 18 => "Copy item ignoring snap and time selection",
        SelectTime = 19 => "Select time",
        AdjustItemVolume = 20 => "Adjust item volume",
        AdjustItemVolumeFine = 21 => "Adjust item volume (fine)",
        MoveItemAndTimeSelection = 22 => "Move item and time selection",
        MoveItemAndTimeSelectionIgnoringSnap = 23 => "Move item and time selection ignoring snap",
        CopyItemAndMoveTimeSelection = 24 => "Copy item and move time selection",
        CopyItemAndMoveTimeSelectionIgnoringSnap = 25 => "Copy item and move time selection ignoring snap",
        SelectTimeIgnoringSnap = 26 => "Select time ignoring snap",
        MarqueeSelectItems = 27 => "Marquee select items",
        MarqueeSelectItemsAndTime = 28 => "Marquee select items and time",
        MarqueeSelectItemsAndTimeIgnoringSnap = 29 => "Marquee select items and time ignoring snap",
        MarqueeToggleItemSelection = 30 => "Marquee toggle item selection",
        MarqueeAddToItemSelection = 31 => "Marquee add to item selection",
        MoveItemVertically = 32 => "Move item vertically",
        MoveItemVerticallyIgnoringSelectionGrouping = 33 => "Move item vertically ignoring selection/grouping",
        MoveItemVerticallyIgnoringTimeSelection = 34 => "Move item vertically ignoring time selection",
        MoveItemVerticallyIgnoringTimeSelectionAndSelectionGrouping = 35 => "Move item vertically ignoring time selection and selection/grouping",
        CopyItemVertically = 36 => "Copy item vertically",
        CopyItemVerticallyIgnoringTimeSelection = 37 => "Copy item vertically ignoring time selection",
        MoveItemContentsIgnoringSelectionGrouping = 38 => "Move item contents ignoring selection/grouping",
        CopyItemPoolingMidiSourceData = 39 => "Copy item, pooling MIDI source data",
        CopyItemIgnoringSnapPoolingMidiSourceData = 40 => "Copy item ignoring snap, pooling MIDI source data",
        CopyItemIgnoringTimeSelectionPoolingMidiSourceData = 41 => "Copy item ignoring time selection, pooling MIDI source data",
        CopyItemIgnoringSnapAndTimeSelectionPoolingMidiSourceData = 42 => "Copy item ignoring snap and time selection, pooling MIDI source data",
        CopyItemVerticallyPoolingMidiSourceData = 43 => "Copy item vertically, pooling MIDI source data",
        CopyItemVerticallyIgnoringTimeSelectionPoolingMidiSourceData = 44 => "Copy item vertically ignoring time selection, pooling MIDI source data",
        CopyItemAndMoveTimeSelectionPoolingMidiSourceData = 45 => "Copy item and move time selection, pooling MIDI source data",
        CopyItemAndMoveTimeSelectionIgnoringSnapPoolingMidiSourceData = 46 => "Copy item and move time selection ignoring snap, pooling MIDI source data",
        AdjustTakePan = 47 => "Adjust take pan",
        MarqueeZoom = 48 => "Marquee zoom",
        MoveItemIgnoringTimeSelectionDisablingRippleEdit = 49 => "Move item ignoring time selection, disabling ripple edit",
        MoveItemIgnoringTimeSelectionEnablingRippleEditForThisTrack = 50 => "Move item ignoring time selection, enabling ripple edit for this track",
        MoveItemIgnoringTimeSelectionEnablingRippleEditForAllTracks = 51 => "Move item ignoring time selection, enabling ripple edit for all tracks",
        MoveItemIgnoringSnapAndTimeSelectionDisablingRippleEdit = 52 => "Move item ignoring snap and time selection, disabling ripple edit",
        MoveItemIgnoringSnapAndTimeSelectionEnablingRippleEditForThisTrack = 53 => "Move item ignoring snap and time selection, enabling ripple edit for this track",
        MoveItemIgnoringSnapAndTimeSelectionEnablingRippleEditForAllTracks = 54 => "Move item ignoring snap and time selection, enabling ripple edit for all tracks",
        MoveItemContentsRippleAllAdjacentItems = 55 => "Move item contents, ripple all adjacent items",
        MoveItemContentsRippleEarlierAdjacentItems = 56 => "Move item contents, ripple earlier adjacent items",
        MoveItemContentsAndRightEdgeRippleLaterAdjacentItems = 57 => "Move item contents and right edge, ripple later adjacent items",
        SelectRazorEditArea = 62 => "Select razor edit area",
        SelectRazorEditAreaIgnoringSnap = 63 => "Select razor edit area ignoring snap",
        AddToRazorEditArea = 64 => "Add to razor edit area",
        AddToRazorEditAreaIgnoringSnap = 65 => "Add to razor edit area ignoring snap",
        SelectRazorEditAreaAndTime = 66 => "Select razor edit area and time",
        SelectRazorEditAreaAndTimeIgnoringSnap = 67 => "Select razor edit area and time ignoring snap",
    }
}

define_action_enum! {
    pub enum MediaItemLowerClickAction {
        PassThroughToItemClickContext = 0 => "Pass through to item click context",
        SelectItemAndMoveEditCursor = 1 => "Select item and move edit cursor",
        SelectItemAndMoveEditCursorIgnoringSnap = 2 => "Select item and move edit cursor ignoring snap",
        SelectItem = 3 => "Select item",
        ToggleItemSelection = 4 => "Toggle item selection",
        AddARangeOfItemsToSelection = 5 => "Add a range of items to selection",
        AddARangeOfItemsToSelectionIfAlreadySelectedExtendTimeSelection = 6 => "Add a range of items to selection, if already selected extend time selection",
        AddARangeOfItemsToSelectionIfAlreadySelectedExtendTimeSelectionIgnoringSnap = 7 => "Add a range of items to selection, if already selected extend time selection ignoring snap",
        ExtendTimeSelection = 8 => "Extend time selection",
        ExtendTimeSelectionIgnoringSnap = 9 => "Extend time selection ignoring snap",
        AddARangeOfItemsToSelectionAndExtendTimeSelection = 10 => "Add a range of items to selection and extend time selection",
        AddARangeOfItemsToSelectionAndExtendTimeSelectionIgnoringSnap = 11 => "Add a range of items to selection and extend time selection ignoring snap",
        SelectItemIgnoringGrouping = 12 => "Select item ignoring grouping",
        RestorePreviousZoomScroll = 13 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 14 => "Restore previous zoom level",
        AddStretchMarker = 15 => "Add stretch marker",
        SelectRazorEditArea = 19 => "Select razor edit area",
        ExtendRazorEditArea = 20 => "Extend razor edit area",
    }
}

define_action_enum! {
    pub enum MediaItemLowerDoubleClickAction {
        PassThroughToItemDoubleClickContext = 0 => "Pass through to item double-click context",
        OpenMediaItemInExternalEditor = 1 => "Open media item in external editor",
        SetTimeSelectionToItem = 2 => "Set time selection to item",
        SetLoopPointsToItem = 3 => "Set loop points to item",
        ShowMediaItemProperties = 4 => "Show media item properties",
        ShowMediaItemSourceProperties = 5 => "Show media item source properties",
        MidiOpenInEditorSubprojectsOpenProjectAudioShowMediaItemProperties = 6 => "MIDI: open in editor, Subprojects: open project, Audio: show media item properties",
        ShowTakeList = 7 => "Show take list",
        ShowTakePropsList = 8 => "Show take props list",
    }
}

define_action_enum! {
    pub enum MediaItemStretchMarkerLeftDragAction {
        NoAction = 0 => "No action",
        MoveStretchMarker = 1 => "Move stretch marker",
        MoveStretchMarkerIgnoringSnap = 2 => "Move stretch marker ignoring snap",
        MoveStretchMarkerIgnoringSelectionGrouping = 3 => "Move stretch marker ignoring selection/grouping",
        MoveStretchMarkerIgnoringSnapAndSelectionGrouping = 4 => "Move stretch marker ignoring snap and selection/grouping",
        MoveContentsUnderStretchMarker = 5 => "Move contents under stretch marker",
        MoveContentsUnderStretchMarkerIgnoringSelectionGrouping = 6 => "Move contents under stretch marker ignoring selection/grouping",
        MoveStretchMarkerPair = 7 => "Move stretch marker pair",
        MoveStretchMarkerPairIgnoringSnap = 8 => "Move stretch marker pair ignoring snap",
        MoveStretchMarkerPairIgnoringSelectionGrouping = 9 => "Move stretch marker pair ignoring selection/grouping",
        MoveStretchMarkerPairIgnoringSnapAndSelectionGrouping = 10 => "Move stretch marker pair ignoring snap and selection/grouping",
        MoveContentsUnderStretchMarkerPair = 11 => "Move contents under stretch marker pair",
        MoveContentsUnderStretchMarkerPairIgnoringSelectionGrouping = 12 => "Move contents under stretch marker pair ignoring selection/grouping",
        RippleMoveStretchMarkers = 13 => "Ripple move stretch markers",
        RippleMoveStretchMarkersIgnoringSnap = 14 => "Ripple move stretch markers ignoring snap",
        RippleMoveStretchMarkersIgnoringSelectionGrouping = 15 => "Ripple move stretch markers ignoring selection/grouping",
        RippleMoveStretchMarkersIgnoringSnapAndSelectionGrouping = 16 => "Ripple move stretch markers ignoring snap and selection/grouping",
        RippleContentsUnderStretchMarkers = 17 => "Ripple contents under stretch markers",
        RippleContentsUnderStretchMarkersIgnoringSelectionGrouping = 18 => "Ripple contents under stretch markers ignoring selection/grouping",
        MoveStretchMarkerPreservingLeftHandRate = 19 => "Move stretch marker preserving left-hand rate",
        MoveStretchMarkerPreservingLeftHandRateIgnoringSnap = 20 => "Move stretch marker preserving left-hand rate ignoring snap",
        MoveStretchMarkerPreservingLeftHandRateIgnoringSelectionGrouping = 21 => "Move stretch marker preserving left-hand rate ignoring selection/grouping",
        MoveStretchMarkerPreservingLeftHandRateIgnoringSnapAndSelectionGrouping = 22 => "Move stretch marker preserving left-hand rate ignoring snap and selection/grouping",
        MoveStretchMarkerPreservingAllRatesRateEnvelopeMode = 23 => "Move stretch marker preserving all rates (rate envelope mode)",
        MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnap = 24 => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap",
        MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSelectionGrouping = 25 => "Move stretch marker preserving all rates (rate envelope mode) ignoring selection/grouping",
        MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnapAndSelectionGrouping = 26 => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap and selection/grouping",
    }
}

define_action_enum! {
    pub enum MediaItemStretchMarkerRateAction {
        NoAction = 0 => "No action",
        EditStretchMarkerRate = 1 => "Edit stretch marker rate",
        EditStretchMarkerRatesOnBothSides = 2 => "Edit stretch marker rates on both sides",
        EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeMode = 3 => "Edit stretch marker rate preserving marker positions (rate envelope mode)",
        EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeMode = 4 => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode)",
        EditStretchMarkerRateMoveContentsUnderMarkerRippleMarkers = 5 => "Edit stretch marker rate, move contents under marker, ripple markers",
        EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerRippleMarkers = 6 => "Edit stretch marker rates on both sides, move contents under marker, ripple markers",
        EditStretchMarkerRateRippleMarkers = 7 => "Edit stretch marker rate, ripple markers",
        EditStretchMarkerRatesOnBothSidesRippleMarkers = 8 => "Edit stretch marker rates on both sides, ripple markers",
        EditStretchMarkerRateIgnoringSelectionGrouping = 9 => "Edit stretch marker rate ignoring selection/grouping",
        EditStretchMarkerRatesOnBothSidesIgnoringSelectionGrouping = 10 => "Edit stretch marker rates on both sides ignoring selection/grouping",
        EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping = 11 => "Edit stretch marker rate preserving marker positions (rate envelope mode), ignoring selection/grouping",
        EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping = 12 => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode), ignoring selection/grouping",
        EditStretchMarkerRateMoveContentsUnderMarkerIgnoringSelectionGrouping = 13 => "Edit stretch marker rate, move contents under marker, ignoring selection/grouping",
        EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerIgnoringSelectionGroupingRippleMarkers = 14 => "Edit stretch marker rates on both sides, move contents under marker, ignoring selection/grouping, ripple markers",
        EditStretchMarkerRateRippleMarkersIgnoringSelectionGrouping = 15 => "Edit stretch marker rate, ripple markers, ignoring selection/grouping",
        EditStretchMarkerRatesOnBothSidesRippleMarkersIgnoringSelectionGrouping = 16 => "Edit stretch marker rates on both sides, ripple markers, ignoring selection/grouping",
    }
}

define_action_enum! {
    pub enum MediaItemStretchMarkerDoubleClickAction {
        NoAction = 0 => "No action",
        ResetStretchMarkerRateTo10 = 1 => "Reset stretch marker rate to 1.0",
        EditStretchMarkerRate = 2 => "Edit stretch marker rate",
    }
}

define_action_enum! {
    pub enum MediaItemCrossfadeLeftDragAction {
        NoAction = 0 => "No action",
        MoveBothFadesIgnoringSnap = 1 => "Move both fades ignoring snap",
        MoveBothFadesIgnoringSnapAndSelectionGrouping = 2 => "Move both fades ignoring snap and selection/grouping",
        MoveBothFadesIgnoringSnapRelativeEdgeEdit = 3 => "Move both fades ignoring snap (relative edge edit)",
        MoveBothFadesAndStretchItemsIgnoringSnap = 4 => "Move both fades and stretch items ignoring snap",
        MoveBothFadesAndStretchItemsIgnoringSnapAndSelectionGrouping = 5 => "Move both fades and stretch items ignoring snap and selection/grouping",
        MoveBothFadesAndStretchItemsIgnoringSnapRelativeEdgeEdit = 6 => "Move both fades and stretch items ignoring snap (relative edge edit)",
        AdjustBothFadeCurvesHorizontally = 7 => "Adjust both fade curves horizontally",
        AdjustBothFadeCurvesHorizontallyIgnoringSelectionGrouping = 8 => "Adjust both fade curves horizontally ignoring selection/grouping",
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnap = 9 => "Adjust length of both fades preserving intersection ignoring snap",
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnapAndSelectionGrouping = 10 => "Adjust length of both fades preserving intersection ignoring snap and selection/grouping",
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnapRelativeEdgeEdit = 11 => "Adjust length of both fades preserving intersection ignoring snap (relative edge edit)",
        AdjustBothFadeCurvesHorizontallyAndVertically = 12 => "Adjust both fade curves horizontally and vertically",
        AdjustBothFadeCurvesHorizontallyAndVerticallyIgnoringSelectionGrouping = 13 => "Adjust both fade curves horizontally and vertically ignoring selection/grouping",
        MoveBothFades = 14 => "Move both fades",
        MoveBothFadesIgnoringSelectionGrouping = 15 => "Move both fades ignoring selection/grouping",
        MoveBothFadesRelativeEdgeEdit = 16 => "Move both fades (relative edge edit)",
        MoveBothFadesAndStretchItems = 17 => "Move both fades and stretch items",
        MoveBothFadesAndStretchItemsIgnoringSelectionGrouping = 18 => "Move both fades and stretch items ignoring selection/grouping",
        MoveBothFadesAndStretchItemsRelativeEdgeEdit = 19 => "Move both fades and stretch items (relative edge edit)",
        AdjustLengthOfBothFadesPreservingIntersection = 20 => "Adjust length of both fades preserving intersection",
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSelectionGrouping = 21 => "Adjust length of both fades preserving intersection ignoring selection/grouping",
        AdjustLengthOfBothFadesPreservingIntersectionRelativeEdgeEdit = 22 => "Adjust length of both fades preserving intersection (relative edge edit)",
    }
}

define_action_enum! {
    pub enum MediaItemCrossfadeClickAction {
        NoAction = 0 => "No action",
        SetBothFadesToNextShape = 1 => "Set both fades to next shape",
        SetBothFadesToPreviousShape = 2 => "Set both fades to previous shape",
        SetBothFadesToNextShapeIgnoringSelection = 3 => "Set both fades to next shape ignoring selection",
        SetBothFadesToPreviousShapeIgnoringSelection = 4 => "Set both fades to previous shape ignoring selection",
        OpenCrossfadeEditor = 5 => "Open crossfade editor",
    }
}

define_action_enum! {
    pub enum MediaItemCrossfadeDoubleClickAction {
        NoAction = 0 => "No action",
        OpenCrossfadeEditor = 1 => "Open crossfade editor",
        ResetToDefaultCrossfade = 2 => "Reset to default crossfade",
    }
}

fn get_media_item_fade_left_drag_action_name(action_id: u32) -> String {
    MediaItemFadeLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_fade_click_action_name(action_id: u32) -> String {
    MediaItemFadeClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_fade_double_click_action_name(action_id: u32) -> String {
    MediaItemFadeDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_lower_left_drag_action_name(action_id: u32) -> String {
    MediaItemLowerLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_lower_click_action_name(action_id: u32) -> String {
    MediaItemLowerClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_lower_double_click_action_name(action_id: u32) -> String {
    MediaItemLowerDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_stretch_marker_left_drag_action_name(action_id: u32) -> String {
    MediaItemStretchMarkerLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_stretch_marker_rate_action_name(action_id: u32) -> String {
    MediaItemStretchMarkerRateAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_stretch_marker_double_click_action_name(action_id: u32) -> String {
    MediaItemStretchMarkerDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_crossfade_left_drag_action_name(action_id: u32) -> String {
    MediaItemCrossfadeLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_crossfade_click_action_name(action_id: u32) -> String {
    MediaItemCrossfadeClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_media_item_crossfade_double_click_action_name(action_id: u32) -> String {
    MediaItemCrossfadeDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy function - kept for backward compatibility
fn get_media_item_fade_left_drag_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "Move fade ignoring snap".to_string(),
        1 => "Move fade ignoring snap".to_string(),
        2 => "Move crossfade ignoring snap".to_string(),
        3 => "Move fade and stretch crossfaded items ignoring snap".to_string(),
        4 => "Move crossfade and stretch crossfaded items ignoring snap".to_string(),
        5 => "Move fade ignoring snap and selection/grouping".to_string(),
        6 => "Move crossfade ignoring snap and selection/grouping".to_string(),
        7 => "Move fade and stretch crossfaded items ignoring snap and selection/grouping"
            .to_string(),
        8 => "Move crossfade and stretch items ignoring snap and selection/grouping".to_string(),
        9 => "Move fade ignoring snap (relative edge edit)".to_string(),
        10 => "Move crossfade ignoring snap (relative edge edit)".to_string(),
        11 => {
            "Move fade and stretch crossfaded items ignoring snap (relative edge edit)".to_string()
        }
        12 => "Move crossfade and stretch items ignoring snap (relative edge edit)".to_string(),
        13 => "Adjust fade curve".to_string(),
        14 => "Adjust fade curve ignoring selection/grouping".to_string(),
        15 => "Adjust fade curve ignoring crossfaded items".to_string(),
        16 => "Adjust fade curve ignoring crossfaded items and selection/grouping".to_string(),
        17 => "Move fade ignoring snap and crossfaded item".to_string(),
        18 => "Move fade ignoring snap, selection/grouping, and crossfaded items".to_string(),
        19 => "Move fade ignoring snap and crossfaded items (relative edge edit)".to_string(),
        20 => "Move fade".to_string(),
        21 => "Move crossfade".to_string(),
        22 => "Move fade and stretch crossfaded items".to_string(),
        23 => "Move crossfade and stretch items".to_string(),
        24 => "Move fade ignoring selection/grouping".to_string(),
        25 => "Move crossfade ignoring selection/grouping".to_string(),
        26 => "Move fade and stretch crossfaded items ignoring selection/grouping".to_string(),
        27 => "Move crossfade and stretch items ignoring selection/grouping".to_string(),
        28 => "Move fade (relative edge edit)".to_string(),
        29 => "Move crossfade (relative edge edit)".to_string(),
        30 => "Move fade and stretch crossfaded items (relative edge edit)".to_string(),
        31 => "Move crossfade and stretch items (relative edge edit)".to_string(),
        32 => "Move fade ignoring crossfaded items".to_string(),
        33 => "Move fade ignoring crossfaded items and selection/grouping".to_string(),
        34 => "Move fade ignoring crossfaded items (relative edge edit)".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_fade_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Delete fade/crossfade".to_string(),
        2 => "Delete fade/crossfade ignoring selection".to_string(),
        3 => "Set fade/crossfade to next shape".to_string(),
        4 => "Set fade/crossfade to previous shape".to_string(),
        5 => "Set fade/crossfade to previous shape ignoring selection".to_string(),
        6 => "Set fade/crossfade to next shape ignoring selection".to_string(),
        7 => "Open crossfade editor".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_fade_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Delete item fade/crossfade".to_string(),
        2 => "Open crossfade editor".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_lower_left_drag_action_name(action_id: u32) -> String {
    // Same as MM_CTX_ITEM but with "Pass through to item drag context" for 0
    match action_id {
        0 => "Pass through to item drag context".to_string(),
        1 => "Move item".to_string(),
        2 => "Copy item".to_string(),
        3 => "Move item contents".to_string(),
        4 => "Move item ignoring snap".to_string(),
        5 => "Copy item ignoring snap".to_string(),
        6 => "Move item edges but not content".to_string(),
        7 => "Render item to new file".to_string(),
        8 => "Open source file in editor or external application".to_string(),
        9 => "Move item ignoring selection/grouping".to_string(),
        10 => "Move item ignoring snap and selection/grouping".to_string(),
        11 => "Adjust take pitch (semitones)".to_string(),
        12 => "Adjust take pitch (fine)".to_string(),
        13 => "Move item ignoring time selection".to_string(),
        14 => "Move item ignoring snap and time selection".to_string(),
        15 => "Move item ignoring time selection and selection/grouping".to_string(),
        16 => "Move item ignoring snap, time selection, and selection/grouping".to_string(),
        17 => "Copy item ignoring time selection".to_string(),
        18 => "Copy item ignoring snap and time selection".to_string(),
        19 => "Select time".to_string(),
        20 => "Adjust item volume".to_string(),
        21 => "Adjust item volume (fine)".to_string(),
        22 => "Move item and time selection".to_string(),
        23 => "Move item and time selection ignoring snap".to_string(),
        24 => "Copy item and move time selection".to_string(),
        25 => "Copy item and move time selection ignoring snap".to_string(),
        26 => "Select time ignoring snap".to_string(),
        27 => "Marquee select items".to_string(),
        28 => "Marquee select items and time".to_string(),
        29 => "Marquee select items and time ignoring snap".to_string(),
        30 => "Marquee toggle item selection".to_string(),
        31 => "Marquee add to item selection".to_string(),
        32 => "Move item vertically".to_string(),
        33 => "Move item vertically ignoring selection/grouping".to_string(),
        34 => "Move item vertically ignoring time selection".to_string(),
        35 => "Move item vertically ignoring time selection and selection/grouping".to_string(),
        36 => "Copy item vertically".to_string(),
        37 => "Copy item vertically ignoring time selection".to_string(),
        38 => "Move item contents ignoring selection/grouping".to_string(),
        39 => "Copy item, pooling MIDI source data".to_string(),
        40 => "Copy item ignoring snap, pooling MIDI source data".to_string(),
        41 => "Copy item ignoring time selection, pooling MIDI source data".to_string(),
        42 => "Copy item ignoring snap and time selection, pooling MIDI source data".to_string(),
        43 => "Copy item vertically, pooling MIDI source data".to_string(),
        44 => "Copy item vertically ignoring time selection, pooling MIDI source data".to_string(),
        45 => "Copy item and move time selection, pooling MIDI source data".to_string(),
        46 => {
            "Copy item and move time selection ignoring snap, pooling MIDI source data".to_string()
        }
        47 => "Adjust take pan".to_string(),
        48 => "Marquee zoom".to_string(),
        49 => "Move item ignoring time selection, disabling ripple edit".to_string(),
        50 => "Move item ignoring time selection, enabling ripple edit for this track".to_string(),
        51 => "Move item ignoring time selection, enabling ripple edit for all tracks".to_string(),
        52 => "Move item ignoring snap and time selection, disabling ripple edit".to_string(),
        53 => "Move item ignoring snap and time selection, enabling ripple edit for this track"
            .to_string(),
        54 => "Move item ignoring snap and time selection, enabling ripple edit for all tracks"
            .to_string(),
        55 => "Move item contents, ripple all adjacent items".to_string(),
        56 => "Move item contents, ripple earlier adjacent items".to_string(),
        57 => "Move item contents and right edge, ripple later adjacent items".to_string(),
        62 => "Select razor edit area".to_string(),
        63 => "Select razor edit area ignoring snap".to_string(),
        64 => "Add to razor edit area".to_string(),
        65 => "Add to razor edit area ignoring snap".to_string(),
        66 => "Select razor edit area and time".to_string(),
        67 => "Select razor edit area and time ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_lower_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "Pass through to item click context".to_string(),
        1 => "Select item and move edit cursor".to_string(),
        2 => "Select item and move edit cursor ignoring snap".to_string(),
        3 => "Select item".to_string(),
        4 => "Toggle item selection".to_string(),
        5 => "Add a range of items to selection".to_string(),
        6 => "Add a range of items to selection, if already selected extend time selection".to_string(),
        7 => "Add a range of items to selection, if already selected extend time selection ignoring snap".to_string(),
        8 => "Extend time selection".to_string(),
        9 => "Extend time selection ignoring snap".to_string(),
        10 => "Add a range of items to selection and extend time selection".to_string(),
        11 => "Add a range of items to selection and extend time selection ignoring snap".to_string(),
        12 => "Select item ignoring grouping".to_string(),
        13 => "Restore previous zoom/scroll".to_string(),
        14 => "Restore previous zoom level".to_string(),
        15 => "Add stretch marker".to_string(),
        19 => "Select razor edit area".to_string(),
        20 => "Extend razor edit area".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_lower_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "Pass through to item double-click context".to_string(),
        1 => "Open media item in external editor".to_string(),
        2 => "Set time selection to item".to_string(),
        3 => "Set loop points to item".to_string(),
        4 => "Show media item properties".to_string(),
        5 => "Show media item source properties".to_string(),
        6 => "MIDI: open in editor, Subprojects: open project, Audio: show media item properties"
            .to_string(),
        7 => "Show take list".to_string(),
        8 => "Show take props list".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_stretch_marker_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move stretch marker".to_string(),
        2 => "Move stretch marker ignoring snap".to_string(),
        3 => "Move stretch marker ignoring selection/grouping".to_string(),
        4 => "Move stretch marker ignoring snap and selection/grouping".to_string(),
        5 => "Move contents under stretch marker".to_string(),
        6 => "Move contents under stretch marker ignoring selection/grouping".to_string(),
        7 => "Move stretch marker pair".to_string(),
        8 => "Move stretch marker pair ignoring snap".to_string(),
        9 => "Move stretch marker pair ignoring selection/grouping".to_string(),
        10 => "Move stretch marker pair ignoring snap and selection/grouping".to_string(),
        11 => "Move contents under stretch marker pair".to_string(),
        12 => "Move contents under stretch marker pair ignoring selection/grouping".to_string(),
        13 => "Ripple move stretch markers".to_string(),
        14 => "Ripple move stretch markers ignoring snap".to_string(),
        15 => "Ripple move stretch markers ignoring selection/grouping".to_string(),
        16 => "Ripple move stretch markers ignoring snap and selection/grouping".to_string(),
        17 => "Ripple contents under stretch markers".to_string(),
        18 => "Ripple contents under stretch markers ignoring selection/grouping".to_string(),
        19 => "Move stretch marker preserving left-hand rate".to_string(),
        20 => "Move stretch marker preserving left-hand rate ignoring snap".to_string(),
        21 => "Move stretch marker preserving left-hand rate ignoring selection/grouping".to_string(),
        22 => "Move stretch marker preserving left-hand rate ignoring snap and selection/grouping".to_string(),
        23 => "Move stretch marker preserving all rates (rate envelope mode)".to_string(),
        24 => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap".to_string(),
        25 => "Move stretch marker preserving all rates (rate envelope mode) ignoring selection/grouping".to_string(),
        26 => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap and selection/grouping".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_stretch_marker_rate_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Edit stretch marker rate".to_string(),
        2 => "Edit stretch marker rates on both sides".to_string(),
        3 => "Edit stretch marker rate preserving marker positions (rate envelope mode)".to_string(),
        4 => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode)".to_string(),
        5 => "Edit stretch marker rate, move contents under marker, ripple markers".to_string(),
        6 => "Edit stretch marker rates on both sides, move contents under marker, ripple markers".to_string(),
        7 => "Edit stretch marker rate, ripple markers".to_string(),
        8 => "Edit stretch marker rates on both sides, ripple markers".to_string(),
        9 => "Edit stretch marker rate ignoring selection/grouping".to_string(),
        10 => "Edit stretch marker rates on both sides ignoring selection/grouping".to_string(),
        11 => "Edit stretch marker rate preserving marker positions (rate envelope mode), ignoring selection/grouping".to_string(),
        12 => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode), ignoring selection/grouping".to_string(),
        13 => "Edit stretch marker rate, move contents under marker, ignoring selection/grouping".to_string(),
        14 => "Edit stretch marker rates on both sides, move contents under marker, ignoring selection/grouping, ripple markers".to_string(),
        15 => "Edit stretch marker rate, ripple markers, ignoring selection/grouping".to_string(),
        16 => "Edit stretch marker rates on both sides, ripple markers, ignoring selection/grouping".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_stretch_marker_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Reset stretch marker rate to 1.0".to_string(),
        2 => "Edit stretch marker rate".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_crossfade_left_drag_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move both fades ignoring snap".to_string(),
        2 => "Move both fades ignoring snap and selection/grouping".to_string(),
        3 => "Move both fades ignoring snap (relative edge edit)".to_string(),
        4 => "Move both fades and stretch items ignoring snap".to_string(),
        5 => "Move both fades and stretch items ignoring snap and selection/grouping".to_string(),
        6 => "Move both fades and stretch items ignoring snap (relative edge edit)".to_string(),
        7 => "Adjust both fade curves horizontally".to_string(),
        8 => "Adjust both fade curves horizontally ignoring selection/grouping".to_string(),
        9 => "Adjust length of both fades preserving intersection ignoring snap".to_string(),
        10 => "Adjust length of both fades preserving intersection ignoring snap and selection/grouping".to_string(),
        11 => "Adjust length of both fades preserving intersection ignoring snap (relative edge edit)".to_string(),
        12 => "Adjust both fade curves horizontally and vertically".to_string(),
        13 => "Adjust both fade curves horizontally and vertically ignoring selection/grouping".to_string(),
        14 => "Move both fades".to_string(),
        15 => "Move both fades ignoring selection/grouping".to_string(),
        16 => "Move both fades (relative edge edit)".to_string(),
        17 => "Move both fades and stretch items".to_string(),
        18 => "Move both fades and stretch items ignoring selection/grouping".to_string(),
        19 => "Move both fades and stretch items (relative edge edit)".to_string(),
        20 => "Adjust length of both fades preserving intersection".to_string(),
        21 => "Adjust length of both fades preserving intersection ignoring selection/grouping".to_string(),
        22 => "Adjust length of both fades preserving intersection (relative edge edit)".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_crossfade_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Set both fades to next shape".to_string(),
        2 => "Set both fades to previous shape".to_string(),
        3 => "Set both fades to next shape ignoring selection".to_string(),
        4 => "Set both fades to previous shape ignoring selection".to_string(),
        5 => "Open crossfade editor".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_media_item_crossfade_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Open crossfade editor".to_string(),
        2 => "Reset to default crossfade".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Mixer Action Mappings
// ============================================================================

define_action_enum! {
    pub enum MixerControlPanelDoubleClickAction {
        NoAction = 0 => "No action",
        SelectAllMediaItemsOnTrack = 1 => "Select all media items on track",
        ZoomViewToTrack = 2 => "Zoom view to track",
        ToggleSelectionForAllMediaItemsOnTrack = 3 => "Toggle selection for all media items on track",
        AddAllMediaItemsOnTrackToSelection = 4 => "Add all media items on track to selection",
    }
}

fn get_mixer_control_panel_double_click_action_name(action_id: u32) -> String {
    MixerControlPanelDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy function - kept for backward compatibility
fn get_mixer_control_panel_double_click_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Select all media items on track".to_string(),
        2 => "Zoom view to track".to_string(),
        3 => "Toggle selection for all media items on track".to_string(),
        4 => "Add all media items on track to selection".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Project Action Mappings
// ============================================================================

define_action_enum! {
    pub enum ProjectMarkerLanesAction {
        NoAction = 0 => "No action",
        HandScroll = 1 => "Hand scroll",
        HandScrollAndHorizontalZoom = 2 => "Hand scroll and horizontal zoom",
        HorizontalZoom = 4 => "Horizontal zoom",
        SetEditCursorAndHorizontalZoom = 6 => "Set edit cursor and horizontal zoom",
        SetEditCursorAndHandScroll = 8 => "Set edit cursor and hand scroll",
        SetEditCursorHandScrollAndHorizontalZoom = 9 => "Set edit cursor, hand scroll and horizontal zoom",
    }
}

define_action_enum! {
    pub enum ProjectMarkerRegionEdgeAction {
        NoAction = 0 => "No action",
        MoveProjectMarkerRegionEdge = 1 => "Move project marker/region edge",
        MoveProjectMarkerRegionEdgeIgnoringSnap = 2 => "Move project marker/region edge ignoring snap",
    }
}

define_action_enum! {
    pub enum ProjectRegionAction {
        NoAction = 0 => "No action",
        MoveContentsOfProjectRegion = 1 => "Move contents of project region",
        MoveContentsOfProjectRegionIgnoringSnap = 2 => "Move contents of project region ignoring snap",
        CopyContentsOfProjectRegions = 3 => "Copy contents of project regions",
        CopyContentsOfProjectRegionsIgnoringSnap = 4 => "Copy contents of project regions ignoring snap",
        MoveProjectRegionsButNotContents = 5 => "Move project regions but not contents",
        MoveProjectRegionsButNotContentsIgnoringSnap = 6 => "Move project regions but not contents ignoring snap",
        CopyProjectRegionsButNotContents = 7 => "Copy project regions but not contents",
        CopyProjectRegionsButNotContentsIgnoringSnap = 8 => "Copy project regions but not contents ignoring snap",
    }
}

define_action_enum! {
    pub enum ProjectTempoMarkerAction {
        NoAction = 0 => "No action",
        MoveProjectTempoTimeSignatureMarker = 1 => "Move project tempo/time signature marker",
        MoveProjectTempoTimeSignatureMarkerIgnoringSnap = 2 => "Move project tempo/time signature marker ignoring snap",
        MoveProjectTempoTimeSignatureMarkerAdjustingPreviousTempo = 3 => "Move project tempo/time signature marker, adjusting previous tempo",
        MoveProjectTempoTimeSignatureMarkerAdjustingPreviousAndCurrentTempo = 4 => "Move project tempo/time signature marker, adjusting previous and current tempo",
    }
}

fn get_project_marker_lanes_action_name(action_id: u32) -> String {
    ProjectMarkerLanesAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_project_marker_region_edge_action_name(action_id: u32) -> String {
    ProjectMarkerRegionEdgeAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_project_region_action_name(action_id: u32) -> String {
    ProjectRegionAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_project_tempo_marker_action_name(action_id: u32) -> String {
    ProjectTempoMarkerAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy functions - kept for backward compatibility
fn get_project_marker_lanes_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Hand scroll".to_string(),
        2 => "Hand scroll and horizontal zoom".to_string(),
        4 => "Horizontal zoom".to_string(),
        6 => "Set edit cursor and horizontal zoom".to_string(),
        8 => "Set edit cursor and hand scroll".to_string(),
        9 => "Set edit cursor, hand scroll and horizontal zoom".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_project_marker_region_edge_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move project marker/region edge".to_string(),
        2 => "Move project marker/region edge ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_project_region_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move contents of project region".to_string(),
        2 => "Move contents of project region ignoring snap".to_string(),
        3 => "Copy contents of project regions".to_string(),
        4 => "Copy contents of project regions ignoring snap".to_string(),
        5 => "Move project regions but not contents".to_string(),
        6 => "Move project regions but not contents ignoring snap".to_string(),
        7 => "Copy project regions but not contents".to_string(),
        8 => "Copy project regions but not contents ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_project_tempo_marker_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move project tempo/time signature marker".to_string(),
        2 => "Move project tempo/time signature marker ignoring snap".to_string(),
        3 => "Move project tempo/time signature marker, adjusting previous tempo".to_string(),
        4 => "Move project tempo/time signature marker, adjusting previous and current tempo"
            .to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Razor Edit Action Mappings
// ============================================================================

define_action_enum! {
    pub enum RazorEditAreaLeftDragAction {
        NoAction = 0 => "No action",
        MoveAreas = 1 => "Move areas",
        MoveAreasIgnoringSnap = 2 => "Move areas ignoring snap",
        CopyAreas = 3 => "Copy areas",
        CopyAreasIgnoringSnap = 4 => "Copy areas ignoring snap",
        MoveAreasWithoutContents = 5 => "Move areas without contents",
        MoveAreasWithoutContentsIgnoringSnap = 6 => "Move areas without contents ignoring snap",
        MoveAreasVertically = 7 => "Move areas vertically",
        MoveAreasOnOneAxisOnly = 8 => "Move areas on one axis only",
        CopyAreasVertically = 9 => "Copy areas vertically",
        MoveAreasHorizontally = 10 => "Move areas horizontally",
        MoveAreasOnOneAxisOnlyIgnoringSnap = 11 => "Move areas on one axis only ignoring snap",
        CopyAreasHorizontally = 12 => "Copy areas horizontally",
        MoveAreasHorizontallyIgnoringSnap = 13 => "Move areas horizontally ignoring snap",
        CopyAreasHorizontallyIgnoringSnap = 14 => "Copy areas horizontally ignoring snap",
        CopyAreasOnOneAxisOnly = 15 => "Copy areas on one axis only",
        CopyAreasOnOneAxisOnlyIgnoringSnap = 16 => "Copy areas on one axis only ignoring snap",
    }
}

define_action_enum! {
    pub enum RazorEditAreaClickAction {
        NoAction = 0 => "No action",
        RemoveOneArea = 1 => "Remove one area",
        DeleteAreasContents = 2 => "Delete areas contents",
        RemoveAreas = 3 => "Remove areas",
        SplitMediaItemsAtAreaEdges = 4 => "Split media items at area edges",
        MoveAreasBackwards = 5 => "Move areas backwards",
        MoveAreasForwards = 6 => "Move areas forwards",
        MoveAreasUpWithoutContents = 7 => "Move areas up without contents",
        MoveAreasDownWithoutContents = 8 => "Move areas down without contents",
    }
}

define_action_enum! {
    pub enum RazorEditEdgeAction {
        NoAction = 0 => "No action",
        MoveEdges = 1 => "Move edges",
        MoveEdgesIgnoringSnap = 2 => "Move edges ignoring snap",
        StretchAreas = 3 => "Stretch areas",
        StretchAreasIgnoringSnap = 4 => "Stretch areas ignoring snap",
    }
}

define_action_enum! {
    pub enum RazorEditEnvelopeAreaAction {
        NoAction = 0 => "No action",
        MoveOrTiltEnvelopeVertically = 1 => "Move or tilt envelope vertically",
        ExpandOrCompressEnvelopeRange = 2 => "Expand or compress envelope range",
        ExpandOrCompressEnvelopeRangeTowardTopBottom = 3 => "Expand or compress envelope range toward top/bottom",
        MoveOrTiltEnvelopeVerticallyFine = 4 => "Move or tilt envelope vertically (fine)",
    }
}

fn get_razor_edit_area_left_drag_action_name(action_id: u32) -> String {
    RazorEditAreaLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_razor_edit_area_click_action_name(action_id: u32) -> String {
    RazorEditAreaClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_razor_edit_edge_action_name(action_id: u32) -> String {
    RazorEditEdgeAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_razor_edit_envelope_area_action_name(action_id: u32) -> String {
    RazorEditEnvelopeAreaAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy functions - kept for backward compatibility
fn get_razor_edit_area_left_drag_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move areas".to_string(),
        2 => "Move areas ignoring snap".to_string(),
        3 => "Copy areas".to_string(),
        4 => "Copy areas ignoring snap".to_string(),
        5 => "Move areas without contents".to_string(),
        6 => "Move areas without contents ignoring snap".to_string(),
        7 => "Move areas vertically".to_string(),
        8 => "Move areas on one axis only".to_string(),
        9 => "Copy areas vertically".to_string(),
        10 => "Move areas horizontally".to_string(),
        11 => "Move areas on one axis only ignoring snap".to_string(),
        12 => "Copy areas horizontally".to_string(),
        13 => "Move areas horizontally ignoring snap".to_string(),
        14 => "Copy areas horizontally ignoring snap".to_string(),
        15 => "Copy areas on one axis only".to_string(),
        16 => "Copy areas on one axis only ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_razor_edit_area_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Remove one area".to_string(),
        2 => "Delete areas contents".to_string(),
        3 => "Remove areas".to_string(),
        4 => "Split media items at area edges".to_string(),
        5 => "Move areas backwards".to_string(),
        6 => "Move areas forwards".to_string(),
        7 => "Move areas up without contents".to_string(),
        8 => "Move areas down without contents".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_razor_edit_edge_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move edges".to_string(),
        2 => "Move edges ignoring snap".to_string(),
        3 => "Stretch areas".to_string(),
        4 => "Stretch areas ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_razor_edit_envelope_area_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move or tilt envelope vertically".to_string(),
        2 => "Expand or compress envelope range".to_string(),
        3 => "Expand or compress envelope range toward top/bottom".to_string(),
        4 => "Move or tilt envelope vertically (fine)".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Ruler Action Mappings
// ============================================================================

define_action_enum! {
    pub enum RulerLeftDragAction {
        NoAction = 0 => "No action",
        EditLoopPoint = 1 => "Edit loop point",
        MoveLoopPoints = 3 => "Move loop points",
        MoveLoopPointsIgnoringSnap = 4 => "Move loop points ignoring snap",
        EditLoopPointAndTimeSelectionTogether = 5 => "Edit loop point and time selection together",
        MoveLoopPointsAndTimeSelectionTogether = 7 => "Move loop points and time selection together",
        MoveLoopPointsAndTimeSelectionTogetherIgnoringSnap = 8 => "Move loop points and time selection together ignoring snap",
        HandScroll = 9 => "Hand scroll",
        HandScrollAndHorizontalZoom = 10 => "Hand scroll and horizontal zoom",
        HorizontalZoom = 12 => "Horizontal zoom",
        SetEditCursorAndHorizontalZoom = 14 => "Set edit cursor and horizontal zoom",
    }
}

define_action_enum! {
    pub enum RulerClickAction {
        NoAction = 0 => "No action",
        MoveEditCursor = 1 => "Move edit cursor",
        MoveEditCursorIgnoringSnap = 2 => "Move edit cursor ignoring snap",
        ClearLoopPoints = 3 => "Clear loop points",
        ExtendLoopPoints = 4 => "Extend loop points",
        ExtendLoopPointsIgnoringSnap = 5 => "Extend loop points ignoring snap",
        SeekPlaybackWithoutMovingEditCursor = 6 => "Seek playback without moving edit cursor",
        RestorePreviousZoomScroll = 7 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 8 => "Restore previous zoom level",
    }
}

define_action_enum! {
    pub enum RulerDoubleClickAction {
        NoAction = 0 => "No action",
        SetLoopPointsToRegion = 1 => "Set loop points to region",
        SetTimeSelectionToRegion = 2 => "Set time selection to region",
        SetLoopPointsAndTimeSelectionToRegion = 3 => "Set loop points and time selection to region",
        RestorePreviousZoomScroll = 4 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 5 => "Restore previous zoom level",
    }
}

fn get_ruler_left_drag_action_name(action_id: u32) -> String {
    RulerLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_ruler_click_action_name(action_id: u32) -> String {
    RulerClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_ruler_double_click_action_name(action_id: u32) -> String {
    RulerDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy functions - kept for backward compatibility
fn get_ruler_left_drag_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Edit loop point".to_string(),
        3 => "Move loop points".to_string(),
        4 => "Move loop points ignoring snap".to_string(),
        5 => "Edit loop point and time selection together".to_string(),
        7 => "Move loop points and time selection together".to_string(),
        8 => "Move loop points and time selection together ignoring snap".to_string(),
        9 => "Hand scroll".to_string(),
        10 => "Hand scroll and horizontal zoom".to_string(),
        12 => "Horizontal zoom".to_string(),
        14 => "Set edit cursor and horizontal zoom".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_ruler_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Move edit cursor".to_string(),
        2 => "Move edit cursor ignoring snap".to_string(),
        3 => "Clear loop points".to_string(),
        4 => "Extend loop points".to_string(),
        5 => "Extend loop points ignoring snap".to_string(),
        6 => "Seek playback without moving edit cursor".to_string(),
        7 => "Restore previous zoom/scroll".to_string(),
        8 => "Restore previous zoom level".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_ruler_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Set loop points to region".to_string(),
        2 => "Set time selection to region".to_string(),
        3 => "Set loop points and time selection to region".to_string(),
        4 => "Restore previous zoom/scroll".to_string(),
        5 => "Restore previous zoom level".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

// ============================================================================
// Track Action Mappings
// ============================================================================

define_action_enum! {
    pub enum TrackLeftDragAction {
        NoAction = 0 => "No action",
        DrawACopyOfTheSelectedMediaItem = 1 => "Draw a copy of the selected media item",
        DrawACopyOfTheSelectedMediaItemIgnoringSnap = 2 => "Draw a copy of the selected media item ignoring snap",
        DrawACopyOfTheSelectedMediaItemOnTheSameTrack = 3 => "Draw a copy of the selected media item on the same track",
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackIgnoringSnap = 4 => "Draw a copy of the selected media item on the same track ignoring snap",
        DrawAnEmptyMidiItem = 5 => "Draw an empty MIDI item",
        DrawAnEmptyMidiItemIgnoringSnap = 6 => "Draw an empty MIDI item ignoring snap",
        SelectTime = 7 => "Select time",
        SelectTimeIgnoringSnap = 8 => "Select time ignoring snap",
        MarqueeSelectItems = 9 => "Marquee select items",
        MarqueeSelectItemsAndTime = 10 => "Marquee select items and time",
        MarqueeSelectItemsAndTimeIgnoringSnap = 11 => "Marquee select items and time ignoring snap",
        MarqueeToggleItemSelection = 12 => "Marquee toggle item selection",
        MarqueeAddToItemSelection = 13 => "Marquee add to item selection",
        MoveTimeSelection = 14 => "Move time selection",
        MoveTimeSelectionIgnoringSnap = 15 => "Move time selection ignoring snap",
        DrawACopyOfTheSelectedMediaItemPoolingMidiSourceData = 16 => "Draw a copy of the selected media item, pooling MIDI source data",
        DrawACopyOfTheSelectedMediaItemIgnoringSnapPoolingMidiSourceData = 17 => "Draw a copy of the selected media item ignoring snap, pooling MIDI source data",
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackPoolingMidiSourceData = 18 => "Draw a copy of the selected media item on the same track, pooling MIDI source data",
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackIgnoringSnapPoolingMidiSourceData = 19 => "Draw a copy of the selected media item on the same track ignoring snap, pooling MIDI source data",
        EditLoopPoints = 20 => "Edit loop points",
        EditLoopPointsIgnoringSnap = 21 => "Edit loop points ignoring snap",
        MarqueeZoom = 22 => "Marquee zoom",
    }
}

define_action_enum! {
    pub enum TrackClickAction {
        NoAction = 0 => "No action",
        DeselectAllItemsAndMoveEditCursor = 1 => "Deselect all items and move edit cursor",
        DeselectAllItemsAndMoveEditCursorIgnoringSnap = 2 => "Deselect all items and move edit cursor ignoring snap",
        DeselectAllItems = 3 => "Deselect all items",
        ClearTimeSelection = 4 => "Clear time selection",
        ExtendTimeSelection = 5 => "Extend time selection",
        ExtendTimeSelectionIgnoringSnap = 6 => "Extend time selection ignoring snap",
        RestorePreviousZoomScroll = 7 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 8 => "Restore previous zoom level",
        SelectRazorEditArea = 25 => "Select razor edit area",
        SelectRazorEditAreaIgnoringSnap = 26 => "Select razor edit area ignoring snap",
        AddToRazorEditArea = 27 => "Add to razor edit area",
        AddToRazorEditAreaIgnoringSnap = 28 => "Add to razor edit area ignoring snap",
        SelectRazorEditAreaAndTime = 29 => "Select razor edit area and time",
        SelectRazorEditAreaAndTimeIgnoringSnap = 30 => "Select razor edit area and time ignoring snap",
    }
}

define_action_enum! {
    pub enum TrackDoubleClickAction {
        NoAction = 0 => "No action",
    }
}

define_action_enum! {
    pub enum TrackControlPanelDoubleClickAction {
        NoAction = 0 => "No action",
        SelectAllMediaItemsOnTrack = 1 => "Select all media items on track",
        ZoomViewToTrack = 2 => "Zoom view to track",
        ToggleSelectionForAllMediaItemsOnTrack = 3 => "Toggle selection for all media items on track",
        AddAllMediaItemsOnTrackToSelection = 4 => "Add all media items on track to selection",
        RestorePreviousZoomScroll = 5 => "Restore previous zoom/scroll",
        RestorePreviousZoomLevel = 6 => "Restore previous zoom level",
    }
}

fn get_track_left_drag_action_name(action_id: u32) -> String {
    TrackLeftDragAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_track_click_action_name(action_id: u32) -> String {
    TrackClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_track_double_click_action_name(action_id: u32) -> String {
    TrackDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

fn get_track_control_panel_double_click_action_name(action_id: u32) -> String {
    TrackControlPanelDoubleClickAction::from_action_id(action_id)
        .display_name()
        .to_string()
}

// Legacy functions - kept for backward compatibility
fn get_track_left_drag_action_name_old(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Draw a copy of the selected media item".to_string(),
        2 => "Draw a copy of the selected media item ignoring snap".to_string(),
        3 => "Draw a copy of the selected media item on the same track".to_string(),
        4 => "Draw a copy of the selected media item on the same track ignoring snap".to_string(),
        5 => "Draw an empty MIDI item".to_string(),
        6 => "Draw an empty MIDI item ignoring snap".to_string(),
        7 => "Select time".to_string(),
        8 => "Select time ignoring snap".to_string(),
        9 => "Marquee select items".to_string(),
        10 => "Marquee select items and time".to_string(),
        11 => "Marquee select items and time ignoring snap".to_string(),
        12 => "Marquee toggle item selection".to_string(),
        13 => "Marquee add to item selection".to_string(),
        14 => "Move time selection".to_string(),
        15 => "Move time selection ignoring snap".to_string(),
        16 => "Draw a copy of the selected media item, pooling MIDI source data".to_string(),
        17 => "Draw a copy of the selected media item ignoring snap, pooling MIDI source data".to_string(),
        18 => "Draw a copy of the selected media item on the same track, pooling MIDI source data".to_string(),
        19 => "Draw a copy of the selected media item on the same track ignoring snap, pooling MIDI source data".to_string(),
        20 => "Edit loop points".to_string(),
        21 => "Edit loop points ignoring snap".to_string(),
        22 => "Marquee zoom".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_track_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Deselect all items and move edit cursor".to_string(),
        2 => "Deselect all items and move edit cursor ignoring snap".to_string(),
        3 => "Deselect all items".to_string(),
        4 => "Clear time selection".to_string(),
        5 => "Extend time selection".to_string(),
        6 => "Extend time selection ignoring snap".to_string(),
        7 => "Restore previous zoom/scroll".to_string(),
        8 => "Restore previous zoom level".to_string(),
        25 => "Select razor edit area".to_string(),
        26 => "Select razor edit area ignoring snap".to_string(),
        27 => "Add to razor edit area".to_string(),
        28 => "Add to razor edit area ignoring snap".to_string(),
        29 => "Select razor edit area and time".to_string(),
        30 => "Select razor edit area and time ignoring snap".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_track_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        _ => format!("Action {} m", action_id),
    }
}

fn get_track_control_panel_double_click_action_name(action_id: u32) -> String {
    match action_id {
        0 => "No action".to_string(),
        1 => "Select all media items on track".to_string(),
        2 => "Zoom view to track".to_string(),
        3 => "Toggle selection for all media items on track".to_string(),
        4 => "Add all media items on track to selection".to_string(),
        5 => "Restore previous zoom/scroll".to_string(),
        6 => "Restore previous zoom level".to_string(),
        _ => format!("Action {} m", action_id),
    }
}
