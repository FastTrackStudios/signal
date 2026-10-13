//! Wrapper around [`WindowGeometryService`] — keyboard-bindable nudge /
//! resize / re-position of REAPER windows. The service is most useful via
//! the FTS REAPER actions registered in `daw-bridge`, but is also
//! exposed here so CLI / agents can drive layout changes directly.

use std::sync::Arc;

use daw_proto::{ScreensetRect, WindowGeometryResult, WindowTarget};

use crate::DawClients;

#[derive(Clone)]
pub struct WindowGeometry {
    clients: Arc<DawClients>,
}

impl WindowGeometry {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    pub async fn rect(&self, target: WindowTarget) -> crate::Result<WindowGeometryResult> {
        Ok(self.clients.window_geometry.get_rect(target).await?)
    }

    pub async fn nudge(
        &self,
        target: WindowTarget,
        dx: i32,
        dy: i32,
    ) -> crate::Result<WindowGeometryResult> {
        Ok(self.clients.window_geometry.nudge(target, dx, dy).await?)
    }

    pub async fn grow(
        &self,
        target: WindowTarget,
        dw: i32,
        dh: i32,
    ) -> crate::Result<WindowGeometryResult> {
        Ok(self.clients.window_geometry.grow(target, dw, dh).await?)
    }

    pub async fn set_rect(&self, target: WindowTarget, rect: ScreensetRect) -> crate::Result<()> {
        self.clients
            .window_geometry
            .set_rect(target, rect)
            .await??;
        Ok(())
    }
}
