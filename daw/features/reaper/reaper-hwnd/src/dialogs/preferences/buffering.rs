//! Preferences -> Audio -> Buffering page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Buffering page child window IDs.
pub struct BufferingPrefs;

impl BufferingPrefs {
    /// Media buffer size inputbox - Class: Edit
    pub const MEDIA_BUFFER_SIZE: ChildId = ChildId(1000);
    /// Media buffer size label - Class: Static
    pub const MEDIA_BUFFER_SIZE_LABEL: ChildId = ChildId(1001);
    /// Media buffer size (when project is stopped) inputbox - Class: Edit
    pub const MEDIA_BUFFER_STOPPED: ChildId = ChildId(1002);
    /// Buffer stopped label - Class: Static
    pub const BUFFER_STOPPED_LABEL: ChildId = ChildId(1003);
    /// Prebuffer media on playback start - Class: Button
    pub const PREBUFFER_ON_PLAYBACK: ChildId = ChildId(1004);
    /// Allow anticipative FX processing - Class: Button
    pub const ANTICIPATIVE_FX: ChildId = ChildId(1005);
    /// Disk read buffer inputbox - Class: Edit
    pub const DISK_READ_BUFFER: ChildId = ChildId(1006);
    /// Disk read buffer label - Class: Static
    pub const DISK_READ_BUFFER_LABEL: ChildId = ChildId(1007);
    /// Disk write buffer inputbox - Class: Edit
    pub const DISK_WRITE_BUFFER: ChildId = ChildId(1008);
    /// Disk write buffer label - Class: Static
    pub const DISK_WRITE_BUFFER_LABEL: ChildId = ChildId(1009);
}
