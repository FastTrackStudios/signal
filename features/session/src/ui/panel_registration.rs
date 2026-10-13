//! Panel registration for session domain.
//!
//! Registers session-ui panels with the dock renderer registry,
//! decoupling panel definitions from the central app binary.

use dock_dioxus::PanelRendererRegistry;
use dock_proto::PanelId;

use crate::ui::layouts::PerformanceLayout;
use crate::ui::layouts::PerformanceSidebar;
use crate::ui::layouts::TransportPanel;

use crate::ui::prelude::*;

/// Register all session panels with the renderer registry.
///
/// Panels whose components live in the app binary (e.g. `ChartView`,
/// `ChartPreviewPanel`, `SetlistView`, `SettingsView`) are not registered here
/// -- the app registers those directly.
pub fn register_panels(registry: &mut PanelRendererRegistry) {
    registry.register(PanelId::Performance, || {
        rsx! {
            div {
                class: "relative h-full w-full bg-background",
                PerformanceLayout {}
            }
        }
    });
    registry.register(PanelId::Navigator, || {
        rsx! { PerformanceSidebar {} }
    });
    registry.register(PanelId::Transport, || {
        rsx! { TransportPanel {} }
    });
}
