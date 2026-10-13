//! Render Queue dialog child window IDs.

use crate::ChildId;

/// Render Queue dialog child window IDs.
pub struct RenderQueue;

impl RenderQueue {
    /// Render all - Class: Button
    pub const RENDER_ALL: ChildId = ChildId(1);
    /// Close - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Remove selected - Class: Button
    pub const REMOVE_SELECTED: ChildId = ChildId(1062);
    /// Render selected - Class: Button
    pub const RENDER_SELECTED: ChildId = ChildId(1063);
    /// Queue list - Class: SysListView32
    pub const QUEUE_LIST: ChildId = ChildId(1106);
}
