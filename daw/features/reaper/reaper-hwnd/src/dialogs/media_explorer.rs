//! Media Explorer dialog child window IDs.

use crate::ChildId;

/// Media Explorer dialog child window IDs.
pub struct MediaExplorer;

impl MediaExplorer {
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Path inputbox - Class: Edit
    pub const PATH: ChildId = ChildId(1000);
    /// Path label - Class: Static
    pub const PATH_LABEL: ChildId = ChildId(1001);
    /// File list - Class: SysListView32
    pub const FILE_LIST: ChildId = ChildId(1002);
    /// Search/filter inputbox - Class: Edit
    pub const SEARCH_FILTER: ChildId = ChildId(1003);
    /// Search label - Class: Static
    pub const SEARCH_LABEL: ChildId = ChildId(1004);
    /// Preview volume fader - Class: REAPERhfader
    pub const PREVIEW_VOLUME: ChildId = ChildId(1005);
    /// Tempo match checkbox - Class: Button
    pub const TEMPO_MATCH: ChildId = ChildId(1006);
    /// Pitch shift inputbox - Class: Edit
    pub const PITCH_SHIFT: ChildId = ChildId(1007);
    /// Start preview on select - Class: Button
    pub const START_ON_SELECT: ChildId = ChildId(1008);
    /// Peaks display - Class: MEpeaks
    pub const PEAKS: ChildId = ChildId(1009);
    /// Preview start/stop button - Class: Button
    pub const PREVIEW_TOGGLE: ChildId = ChildId(1010);
    /// Insert into project button - Class: Button
    pub const INSERT: ChildId = ChildId(1011);
    /// BPM inputbox - Class: Edit
    pub const BPM: ChildId = ChildId(1012);
    /// BPM label - Class: Static
    pub const BPM_LABEL: ChildId = ChildId(1013);
    /// Up one directory button - Class: Button
    pub const UP_DIRECTORY: ChildId = ChildId(1014);
    /// Favorites/shortcuts dropdown - Class: ComboBox
    pub const FAVORITES: ChildId = ChildId(1015);
    /// Database search dropdown - Class: ComboBox
    pub const DATABASE: ChildId = ChildId(1016);
    /// Metadata display - Class: Static
    pub const METADATA: ChildId = ChildId(1100);
    /// Options button - Class: Button
    pub const OPTIONS: ChildId = ChildId(1200);
    /// File info label - Class: Static
    pub const FILE_INFO: ChildId = ChildId(1201);
}
