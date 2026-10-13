//! Mixer mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Mixer control panel double click behaviors
    pub enum MixerControlPanelDoubleClickBehavior {
        NoAction => (0, "No action"),
        SelectAllMediaItemsOnTrack => (1, "Select all media items on track"),
        ZoomViewToTrack => (2, "Zoom view to track"),
        ToggleSelectionForAllMediaItemsOnTrack => (3, "Toggle selection for all media items on track"),
        AddAllMediaItemsOnTrackToSelection => (4, "Add all media items on track to selection"),
    }
}
