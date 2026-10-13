//! Dock context for dependency injection.
//!
//! Provides the panel renderer callback to the component tree.
//! The consuming app registers a callback that maps PanelId -> Element.

use crate::prelude::*;
use crate::signals::DOCK_PANEL_RENDERER;
use dock_proto::PanelId;
use std::rc::Rc;

/// Wrapper around the panel renderer function with `PartialEq` via `Rc::ptr_eq`.
///
/// Dioxus `#[component]` requires all props to implement `PartialEq`.
/// Since `Rc<dyn Fn>` doesn't, we wrap it and compare by pointer identity.
#[derive(Clone)]
pub struct PanelRenderer(Rc<dyn Fn(PanelId) -> Element>);

impl PanelRenderer {
    /// Create a new panel renderer from a closure.
    pub fn new(f: impl Fn(PanelId) -> Element + 'static) -> Self {
        Self(Rc::new(f))
    }

    /// Render a panel by its ID.
    pub fn render(&self, panel_id: PanelId) -> Element {
        (self.0)(panel_id)
    }
}

impl PartialEq for PanelRenderer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Context holding the panel renderer.
#[derive(Clone)]
pub struct DockContext {
    pub render_panel: PanelRenderer,
}

/// Hook to access the dock context.
pub fn use_dock_context() -> DockContext {
    use_context::<DockContext>()
}

/// Provider component that injects the panel renderer into context.
///
/// # Usage
///
/// ```rust,ignore
/// DockProvider {
///     render_panel: PanelRenderer::new(|panel_id| match panel_id {
///         PanelId::Performance => rsx! { PerformanceLayout {} },
///         PanelId::ChartEditor => rsx! { ChartView {} },
///         _ => rsx! { div { "Panel: {panel_id:?}" } },
///     }),
///     // children...
/// }
/// ```
#[component]
pub fn DockProvider(render_panel: PanelRenderer, children: Element) -> Element {
    let renderer_for_signal = render_panel.clone();
    use_effect(move || {
        *DOCK_PANEL_RENDERER.write() = Some(renderer_for_signal.clone());
    });

    use_context_provider(move || DockContext {
        render_panel: render_panel.clone(),
    });
    children
}
