//! Main Window (GetMainHWND) child window IDs.

use crate::ChildId;

/// Main Window (GetMainHWND) child window IDs.
pub struct MainWindow;

impl MainWindow {
    /// Transport (Windows) / MainHWND (Mac) - Class: REAPERVirtWndDlgHost
    pub const TRANSPORT: ChildId = ChildId(0);
    /// Project tabs (if tabs exist) - Class: WDLTabCtrl
    pub const PROJECT_TABS: ChildId = ChildId(999);
    /// Trackview - Class: REAPERTrackListWindow
    pub const TRACKVIEW: ChildId = ChildId(1000);
    /// Timeline - Class: REAPERTimeDisplay
    pub const TIMELINE: ChildId = ChildId(1005);
    /// Mouse editing help beneath track control panels - Class: Static
    pub const MOUSE_EDITING_HELP: ChildId = ChildId(1259);
}

/// Transport bar child window IDs (child 0 of MainWindow, Windows only).
pub struct Transport;

impl Transport {
    /// Status display - Class: REAPERstatusdisp
    pub const STATUS: ChildId = ChildId(1010);
}
