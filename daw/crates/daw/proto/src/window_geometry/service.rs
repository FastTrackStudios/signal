//! Window geometry — nudge/resize/position against host windows.

use super::{WindowGeometryResult, WindowTarget};
use crate::{DawResult, ScreensetRect};

#[architect::rpc]
pub trait WindowGeometry {
    /// Read the current outer-rect of `target`.
    fn get_rect(&self, target: WindowTarget) -> WindowGeometryResult;

    /// Nudge `target` by `dx`/`dy` pixels.
    fn nudge(&self, target: WindowTarget, dx: i32, dy: i32) -> WindowGeometryResult;

    /// Grow / shrink `target` by `dw`/`dh` pixels (negative shrinks).
    fn grow(&self, target: WindowTarget, dw: i32, dh: i32) -> WindowGeometryResult;

    /// Move + resize to an explicit rect.
    fn set_rect(&self, target: WindowTarget, rect: ScreensetRect) -> DawResult<()>;
}
