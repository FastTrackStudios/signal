//! Media Item stretch marker behaviors

use crate::input::mouse_modifiers::behaviors::shared::traits::{BehaviorDisplay, BehaviorId};

/// Media Item stretch marker left drag behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemStretchMarkerLeftDragBehavior {
    NoAction,
    MoveStretchMarker,
    MoveStretchMarkerIgnoringSnap,
    MoveStretchMarkerIgnoringSelectionGrouping,
    MoveStretchMarkerIgnoringSnapAndSelectionGrouping,
    MoveContentsUnderStretchMarker,
    MoveContentsUnderStretchMarkerIgnoringSelectionGrouping,
    MoveStretchMarkerPair,
    MoveStretchMarkerPairIgnoringSnap,
    MoveStretchMarkerPairIgnoringSelectionGrouping,
    MoveStretchMarkerPairIgnoringSnapAndSelectionGrouping,
    MoveContentsUnderStretchMarkerPair,
    MoveContentsUnderStretchMarkerPairIgnoringSelectionGrouping,
    RippleMoveStretchMarkers,
    RippleMoveStretchMarkersIgnoringSnap,
    RippleMoveStretchMarkersIgnoringSelectionGrouping,
    RippleMoveStretchMarkersIgnoringSnapAndSelectionGrouping,
    RippleContentsUnderStretchMarkers,
    RippleContentsUnderStretchMarkersIgnoringSelectionGrouping,
    MoveStretchMarkerPreservingLeftHandRate,
    MoveStretchMarkerPreservingLeftHandRateIgnoringSnap,
    MoveStretchMarkerPreservingLeftHandRateIgnoringSelectionGrouping,
    MoveStretchMarkerPreservingLeftHandRateIgnoringSnapAndSelectionGrouping,
    MoveStretchMarkerPreservingAllRatesRateEnvelopeMode,
    MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnap,
    MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSelectionGrouping,
    MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnapAndSelectionGrouping,
    Unknown(u32),
}

impl BehaviorId for MediaItemStretchMarkerLeftDragBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemStretchMarkerLeftDragBehavior::NoAction,
            1 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarker,
            2 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnap,
            3 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSelectionGrouping,
            4 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnapAndSelectionGrouping,
            5 => MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarker,
            6 => MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerIgnoringSelectionGrouping,
            7 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPair,
            8 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnap,
            9 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSelectionGrouping,
            10 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnapAndSelectionGrouping,
            11 => MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPair,
            12 => MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPairIgnoringSelectionGrouping,
            13 => MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkers,
            14 => MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnap,
            15 => MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSelectionGrouping,
            16 => MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnapAndSelectionGrouping,
            17 => MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkers,
            18 => MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkersIgnoringSelectionGrouping,
            19 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRate,
            20 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnap,
            21 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSelectionGrouping,
            22 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnapAndSelectionGrouping,
            23 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeMode,
            24 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnap,
            25 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSelectionGrouping,
            26 => MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnapAndSelectionGrouping,
            id => MediaItemStretchMarkerLeftDragBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemStretchMarkerLeftDragBehavior::NoAction => 0,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarker => 1,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnap => 2,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSelectionGrouping => 3,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnapAndSelectionGrouping => 4,
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarker => 5,
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerIgnoringSelectionGrouping => 6,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPair => 7,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnap => 8,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSelectionGrouping => 9,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnapAndSelectionGrouping => 10,
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPair => 11,
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPairIgnoringSelectionGrouping => 12,
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkers => 13,
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnap => 14,
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSelectionGrouping => 15,
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnapAndSelectionGrouping => 16,
            MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkers => 17,
            MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkersIgnoringSelectionGrouping => 18,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRate => 19,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnap => 20,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSelectionGrouping => 21,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnapAndSelectionGrouping => 22,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeMode => 23,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnap => 24,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSelectionGrouping => 25,
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnapAndSelectionGrouping => 26,
            MediaItemStretchMarkerLeftDragBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemStretchMarkerLeftDragBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemStretchMarkerLeftDragBehavior::NoAction => "No action",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarker => "Move stretch marker",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnap => "Move stretch marker ignoring snap",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSelectionGrouping => "Move stretch marker ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerIgnoringSnapAndSelectionGrouping => "Move stretch marker ignoring snap and selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarker => "Move contents under stretch marker",
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerIgnoringSelectionGrouping => "Move contents under stretch marker ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPair => "Move stretch marker pair",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnap => "Move stretch marker pair ignoring snap",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSelectionGrouping => "Move stretch marker pair ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPairIgnoringSnapAndSelectionGrouping => "Move stretch marker pair ignoring snap and selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPair => "Move contents under stretch marker pair",
            MediaItemStretchMarkerLeftDragBehavior::MoveContentsUnderStretchMarkerPairIgnoringSelectionGrouping => "Move contents under stretch marker pair ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkers => "Ripple move stretch markers",
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnap => "Ripple move stretch markers ignoring snap",
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSelectionGrouping => "Ripple move stretch markers ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::RippleMoveStretchMarkersIgnoringSnapAndSelectionGrouping => "Ripple move stretch markers ignoring snap and selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkers => "Ripple contents under stretch markers",
            MediaItemStretchMarkerLeftDragBehavior::RippleContentsUnderStretchMarkersIgnoringSelectionGrouping => "Ripple contents under stretch markers ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRate => "Move stretch marker preserving left-hand rate",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnap => "Move stretch marker preserving left-hand rate ignoring snap",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSelectionGrouping => "Move stretch marker preserving left-hand rate ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingLeftHandRateIgnoringSnapAndSelectionGrouping => "Move stretch marker preserving left-hand rate ignoring snap and selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeMode => "Move stretch marker preserving all rates (rate envelope mode)",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnap => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSelectionGrouping => "Move stretch marker preserving all rates (rate envelope mode) ignoring selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::MoveStretchMarkerPreservingAllRatesRateEnvelopeModeIgnoringSnapAndSelectionGrouping => "Move stretch marker preserving all rates (rate envelope mode) ignoring snap and selection/grouping",
            MediaItemStretchMarkerLeftDragBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}

/// Media Item stretch marker rate behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemStretchMarkerRateBehavior {
    NoAction,
    EditStretchMarkerRate,
    EditStretchMarkerRatesOnBothSides,
    EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeMode,
    EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeMode,
    EditStretchMarkerRateMoveContentsUnderMarkerRippleMarkers,
    EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerRippleMarkers,
    EditStretchMarkerRateRippleMarkers,
    EditStretchMarkerRatesOnBothSidesRippleMarkers,
    EditStretchMarkerRateIgnoringSelectionGrouping,
    EditStretchMarkerRatesOnBothSidesIgnoringSelectionGrouping,
    EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping,
    EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping,
    EditStretchMarkerRateMoveContentsUnderMarkerIgnoringSelectionGrouping,
    EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerIgnoringSelectionGroupingRippleMarkers,
    EditStretchMarkerRateRippleMarkersIgnoringSelectionGrouping,
    EditStretchMarkerRatesOnBothSidesRippleMarkersIgnoringSelectionGrouping,
    Unknown(u32),
}

impl BehaviorId for MediaItemStretchMarkerRateBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemStretchMarkerRateBehavior::NoAction,
            1 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRate,
            2 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSides,
            3 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeMode,
            4 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeMode,
            5 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerRippleMarkers,
            6 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerRippleMarkers,
            7 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkers,
            8 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkers,
            9 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateIgnoringSelectionGrouping,
            10 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesIgnoringSelectionGrouping,
            11 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping,
            12 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping,
            13 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerIgnoringSelectionGrouping,
            14 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerIgnoringSelectionGroupingRippleMarkers,
            15 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkersIgnoringSelectionGrouping,
            16 => MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkersIgnoringSelectionGrouping,
            id => MediaItemStretchMarkerRateBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemStretchMarkerRateBehavior::NoAction => 0,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRate => 1,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSides => 2,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeMode => 3,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeMode => 4,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerRippleMarkers => 5,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerRippleMarkers => 6,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkers => 7,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkers => 8,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateIgnoringSelectionGrouping => 9,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesIgnoringSelectionGrouping => 10,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping => 11,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping => 12,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerIgnoringSelectionGrouping => 13,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerIgnoringSelectionGroupingRippleMarkers => 14,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkersIgnoringSelectionGrouping => 15,
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkersIgnoringSelectionGrouping => 16,
            MediaItemStretchMarkerRateBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemStretchMarkerRateBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemStretchMarkerRateBehavior::NoAction => "No action",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRate => "Edit stretch marker rate",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSides => "Edit stretch marker rates on both sides",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeMode => "Edit stretch marker rate preserving marker positions (rate envelope mode)",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeMode => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode)",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerRippleMarkers => "Edit stretch marker rate, move contents under marker, ripple markers",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerRippleMarkers => "Edit stretch marker rates on both sides, move contents under marker, ripple markers",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkers => "Edit stretch marker rate, ripple markers",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkers => "Edit stretch marker rates on both sides, ripple markers",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateIgnoringSelectionGrouping => "Edit stretch marker rate ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesIgnoringSelectionGrouping => "Edit stretch marker rates on both sides ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatePreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping => "Edit stretch marker rate preserving marker positions (rate envelope mode), ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesPreservingMarkerPositionsRateEnvelopeModeIgnoringSelectionGrouping => "Edit stretch marker rates on both sides preserving marker positions (rate envelope mode), ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateMoveContentsUnderMarkerIgnoringSelectionGrouping => "Edit stretch marker rate, move contents under marker, ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesMoveContentsUnderMarkerIgnoringSelectionGroupingRippleMarkers => "Edit stretch marker rates on both sides, move contents under marker, ignoring selection/grouping, ripple markers",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRateRippleMarkersIgnoringSelectionGrouping => "Edit stretch marker rate, ripple markers, ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::EditStretchMarkerRatesOnBothSidesRippleMarkersIgnoringSelectionGrouping => "Edit stretch marker rates on both sides, ripple markers, ignoring selection/grouping",
            MediaItemStretchMarkerRateBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}

/// Media Item stretch marker double click behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemStretchMarkerDoubleClickBehavior {
    NoAction,
    ResetStretchMarkerRateTo10,
    EditStretchMarkerRate,
    Unknown(u32),
}

impl BehaviorId for MediaItemStretchMarkerDoubleClickBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemStretchMarkerDoubleClickBehavior::NoAction,
            1 => MediaItemStretchMarkerDoubleClickBehavior::ResetStretchMarkerRateTo10,
            2 => MediaItemStretchMarkerDoubleClickBehavior::EditStretchMarkerRate,
            id => MediaItemStretchMarkerDoubleClickBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemStretchMarkerDoubleClickBehavior::NoAction => 0,
            MediaItemStretchMarkerDoubleClickBehavior::ResetStretchMarkerRateTo10 => 1,
            MediaItemStretchMarkerDoubleClickBehavior::EditStretchMarkerRate => 2,
            MediaItemStretchMarkerDoubleClickBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemStretchMarkerDoubleClickBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemStretchMarkerDoubleClickBehavior::NoAction => "No action",
            MediaItemStretchMarkerDoubleClickBehavior::ResetStretchMarkerRateTo10 => {
                "Reset stretch marker rate to 1.0"
            }
            MediaItemStretchMarkerDoubleClickBehavior::EditStretchMarkerRate => {
                "Edit stretch marker rate"
            }
            MediaItemStretchMarkerDoubleClickBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}
