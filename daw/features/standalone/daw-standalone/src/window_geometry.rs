//! `impl WindowGeometry for Standalone` — stubbed.
//!
//! The standalone backend has no real windows, so every op returns
//! `applied: false` with an explanatory error. Tests that need synthetic
//! window state can extend this later by promoting the (existing) sync
//! `windows` HashMap onto `StandaloneState`.

use daw_proto::window_geometry::{WindowGeometry, WindowGeometryResult, WindowTarget};
use daw_proto::{DawError, DawResult, ScreensetRect};

use crate::sync::Standalone;

fn stub_result() -> WindowGeometryResult {
    WindowGeometryResult {
        applied: false,
        rect: None,
        error: "standalone".to_string(),
    }
}

impl WindowGeometry for Standalone {
    fn get_rect(&self, _target: WindowTarget) -> WindowGeometryResult {
        stub_result()
    }

    fn nudge(&self, _target: WindowTarget, _dx: i32, _dy: i32) -> WindowGeometryResult {
        stub_result()
    }

    fn grow(&self, _target: WindowTarget, _dw: i32, _dh: i32) -> WindowGeometryResult {
        stub_result()
    }

    fn set_rect(&self, _target: WindowTarget, _rect: ScreensetRect) -> DawResult<()> {
        Err(DawError::operation_failed(
            "standalone: window geometry not supported",
        ))
    }
}
