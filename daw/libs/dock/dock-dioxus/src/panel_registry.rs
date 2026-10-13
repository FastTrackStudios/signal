//! Panel renderer registry — maps PanelId to UI render functions.
//!
//! This lives in dock-dioxus (not dock-proto) because it depends on
//! Dioxus `Element` types. Domain crates register their panels at startup
//! via `register()`, and the dock system calls `render()` to display them.

use crate::prelude::*;
use dock_proto::PanelId;
use std::collections::HashMap;
use std::rc::Rc;

/// Type alias for a panel render function.
///
/// Each registered panel provides a closure that returns an `Element`.
type RenderFn = Rc<dyn Fn() -> Element>;

/// Registry that maps `PanelId` to render functions.
///
/// Domain crates (signal-ui, session-ui, daw-ui) register their panels
/// at startup, and the dock system uses `render()` to display them.
/// Unregistered panels get a "Coming soon" fallback.
pub struct PanelRendererRegistry {
    renderers: HashMap<PanelId, RenderFn>,
}

impl PanelRendererRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            renderers: HashMap::new(),
        }
    }

    /// Register a render function for a panel.
    ///
    /// If a renderer was already registered for this panel, it is replaced.
    pub fn register(&mut self, panel_id: PanelId, renderer: impl Fn() -> Element + 'static) {
        self.renderers.insert(panel_id, Rc::new(renderer));
    }

    /// Render a panel by its ID.
    ///
    /// Returns the panel's element if registered, or a "Coming soon" fallback.
    pub fn render(&self, panel_id: PanelId) -> Element {
        match self.renderers.get(&panel_id) {
            Some(render_fn) => render_fn(),
            None => {
                let name = panel_id.display_name();
                rsx! {
                    div {
                        class: "flex items-center justify-center h-full text-zinc-500",
                        "{name} -- Coming soon"
                    }
                }
            }
        }
    }

    /// Check whether a panel has a registered renderer.
    pub fn has(&self, panel_id: PanelId) -> bool {
        self.renderers.contains_key(&panel_id)
    }

    /// Number of registered panel renderers.
    pub fn len(&self) -> usize {
        self.renderers.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.renderers.is_empty()
    }
}

impl Default for PanelRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}
