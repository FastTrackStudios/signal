//! Panel registration for DAW domain.
//!
//! Registers daw-ui panels with the dock renderer registry,
//! decoupling panel definitions from the central app binary.

use dock_dioxus::PanelRendererRegistry;
use dock_proto::PanelId;

use crate::components::arrangement_view::ArrangementView;
use crate::components::fx_chain_tree::FxChainTree;
use crate::components::mixer::MixerPanel;
use crate::components::track_control_panel::TrackControlPanel;

use crate::prelude::*;

/// Register all DAW panels with the renderer registry.
pub fn register_panels(registry: &mut PanelRendererRegistry) {
    registry.register(PanelId::Mixer, || {
        rsx! { MixerPanel {} }
    });
    registry.register(PanelId::FxChainTree, || {
        rsx! { FxChainTree {} }
    });
    registry.register(PanelId::TrackControlPanel, || {
        rsx! { TrackControlPanel {} }
    });
    registry.register(PanelId::ArrangementView, || {
        rsx! { ArrangementView {} }
    });
}
