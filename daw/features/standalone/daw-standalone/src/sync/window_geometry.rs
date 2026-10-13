//! `StandaloneWindowGeometry` — sync window-geometry sub-handle.
//!
//! The standalone backend has no real windows. Tests seed a synthetic rect
//! per `WindowTarget` via `Standalone::seed_window_target` and drive
//! nudge/grow/set_rect against those entries.

use std::collections::HashMap;

use daw_proto::{DawError, DawResult, ScreensetRect, WindowTarget, sync::WindowGeometry};

use super::daw::Standalone;

const MIN_DIM: u32 = 80;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum TargetKey {
    Focused,
    Main,
}

impl From<WindowTarget> for TargetKey {
    fn from(t: WindowTarget) -> Self {
        match t {
            WindowTarget::Focused => Self::Focused,
            WindowTarget::Main => Self::Main,
        }
    }
}

pub(crate) type WindowMap = HashMap<TargetKey, ScreensetRect>;

pub struct StandaloneWindowGeometry<'a> {
    daw: &'a Standalone,
}

impl<'a> StandaloneWindowGeometry<'a> {
    pub(crate) fn new(daw: &'a Standalone) -> Self {
        Self { daw }
    }
}

impl<'a> WindowGeometry for StandaloneWindowGeometry<'a> {
    fn rect(&self, target: WindowTarget) -> Option<ScreensetRect> {
        let s = self.daw.state.lock().expect("standalone state poisoned");
        s.windows.get(&target.into()).cloned()
    }

    fn nudge(&self, target: WindowTarget, dx: i32, dy: i32) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        let key: TargetKey = target.into();
        let r = s
            .windows
            .get_mut(&key)
            .ok_or_else(|| DawError::not_found("Window", &format!("{:?}", target)))?;
        r.x += dx;
        r.y += dy;
        Ok(())
    }

    fn grow(&self, target: WindowTarget, dw: i32, dh: i32) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        let key: TargetKey = target.into();
        let r = s
            .windows
            .get_mut(&key)
            .ok_or_else(|| DawError::not_found("Window", &format!("{:?}", target)))?;
        r.width = ((r.width as i32) + dw).max(MIN_DIM as i32) as u32;
        r.height = ((r.height as i32) + dh).max(MIN_DIM as i32) as u32;
        Ok(())
    }

    fn set_rect(&self, target: WindowTarget, rect: ScreensetRect) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        s.windows.insert(target.into(), rect);
        Ok(())
    }
}
