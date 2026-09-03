//! **What is selected** — the one piece of state the browser reads.
//!
//! The rig is a tree (engine → layer → module) and the browser is the same
//! browser at every level of it; what changes is the *scope* it lists and the
//! engine it filters to. So selection is not "which panel is open", it is a
//! path into the rig, and clicking anywhere in the mixer sets it:
//!
//! ```text
//! click the engine card    → Selection::Engine("Keys")   → engine programs
//! click a lane             → Selection::Layer("Keys 1")  → layer presets
//! pick a module in the zoom→ Selection::Module{…}        → soundsources
//! ```

use dioxus::prelude::*;

/// The rig node the browser is pointed at.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Selection {
    /// Nothing picked yet — the browser lists everything.
    #[default]
    None,
    /// A whole engine, by name.
    Engine(String),
    /// One lane, by name (its engine comes from the mixer).
    Layer { engine: String, layer: String },
    /// One module inside a lane.
    Module {
        engine: String,
        layer: String,
        module: u32,
    },
}

impl Selection {
    /// The engine this selection lives under, if any — the browser's tag
    /// filter.
    #[must_use]
    pub fn engine(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Engine(e) => Some(e),
            Self::Layer { engine, .. } | Self::Module { engine, .. } => Some(engine),
        }
    }

    /// The lane a preset would load into, if the selection names one.
    #[must_use]
    pub fn layer(&self) -> Option<&str> {
        match self {
            Self::Layer { layer, .. } | Self::Module { layer, .. } => Some(layer),
            _ => None,
        }
    }

    /// Which module a preset would land in. A layer selection loads into its
    /// first module — a lane's preset is what module A is playing.
    #[must_use]
    pub fn module(&self) -> u32 {
        match self {
            Self::Module { module, .. } => *module,
            _ => 0,
        }
    }

    /// What to call this level in the browser's header.
    #[must_use]
    pub fn level(&self) -> &'static str {
        match self {
            Self::None => "Library",
            Self::Engine(_) => "Engine presets",
            Self::Layer { .. } => "Layer presets",
            Self::Module { .. } => "Module presets",
        }
    }

    /// Whether a preset of `scope` belongs at this level. A level shows its
    /// own scope and everything beneath it: a soundsource is a fine thing to
    /// put in a lane, it just fills one of its modules.
    #[must_use]
    pub fn accepts(&self, scope: &str) -> bool {
        match self {
            Self::None | Self::Engine(_) => true,
            Self::Layer { .. } => scope == "layer" || scope == "module",
            Self::Module { .. } => scope == "module" || scope == "layer",
        }
    }
}

/// The shared selection. Both hooks run unconditionally — a hook behind a
/// fallback would shift hook order between renders and panic.
pub fn use_selection() -> Signal<Selection> {
    let local = use_signal(Selection::default);
    let shared = use_hook(try_consume_context::<Signal<Selection>>);
    shared.unwrap_or(local)
}
