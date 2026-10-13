//! Module system — standard interface for DAW extension modules.
//!
//! Any library that wants to be loadable by a DAW extension host implements
//! [`DawModule`]. The host collects modules and calls the standard methods
//! to register actions and subscribe to events.
//!
//! # For library authors
//!
//! ```rust,ignore
//! use daw::module::{DawModule, ActionDef, ModuleContext};
//!
//! pub struct MyModule;
//!
//! impl DawModule for MyModule {
//!     fn name(&self) -> &str { "my-module" }
//!     fn display_name(&self) -> &str { "My Module" }
//!
//!     fn actions(&self) -> Vec<ActionDef> {
//!         vec![
//!             ActionDef::new("FTS_MY_ACTION", "Do something", || {
//!                 tracing::info!("Action executed!");
//!             }),
//!         ]
//!     }
//! }
//!
//! // Convention: export a `module()` function
//! pub fn module() -> Box<dyn DawModule> {
//!     Box::new(MyModule)
//! }
//! ```
//!
//! # For the host extension
//!
//! ```rust,ignore
//! use daw::module::{self, DawModule, ModuleContext};
//!
//! let modules: Vec<Box<dyn DawModule>> = vec![
//!     session::module(),
//!     sync::module(),
//!     dynamic_template::module(),
//! ];
//!
//! let ctx = ModuleContext::new(tokio_runtime.clone());
//! module::init_all(&modules, &ctx);
//! let action_defs = module::collect_actions(&modules);
//! ```

use std::sync::Arc;

/// Tuple consumed by extension action registration.
pub type ActionRegistrationTuple = (String, String, Arc<dyn Fn() + Send + Sync>, bool, bool);

/// An action that a module registers with the DAW.
pub struct ActionDef {
    /// Unique command ID registered with the DAW (e.g. "FTS_SESSION_NEXT_SONG").
    pub command_id: String,
    /// Human-readable name shown in the action list.
    pub display_name: String,
    /// Handler called on the main thread when the action is triggered.
    pub handler: Arc<dyn Fn() + Send + Sync>,
    /// If true, the action appears in the Extensions > FastTrackStudio menu.
    pub show_in_menu: bool,
    /// If true, the host should register the action as toggleable.
    pub toggleable: bool,
}

impl ActionDef {
    /// Create a new action definition (not shown in menu by default).
    pub fn new(
        command_id: impl Into<String>,
        display_name: impl Into<String>,
        handler: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            command_id: command_id.into(),
            display_name: display_name.into(),
            handler: Arc::new(handler),
            show_in_menu: false,
            toggleable: false,
        }
    }

    /// Mark this action to appear in the Extensions > FastTrackStudio menu.
    pub fn in_menu(mut self) -> Self {
        self.show_in_menu = true;
        self
    }

    /// Mark this action as toggleable in REAPER.
    pub fn toggleable(mut self) -> Self {
        self.toggleable = true;
        self
    }

    /// Convert to the tuple format used by extension action registration.
    pub fn into_tuple(self) -> ActionRegistrationTuple {
        (
            self.command_id,
            self.display_name,
            self.handler,
            self.show_in_menu,
            self.toggleable,
        )
    }
}

/// Context provided to modules during initialization.
///
/// Gives modules access to the async runtime for spawning tasks
/// (event subscriptions, background work, etc.)
pub struct ModuleContext {
    /// The shared tokio runtime.
    pub runtime: Arc<tokio::runtime::Runtime>,
}

impl ModuleContext {
    pub fn new(runtime: Arc<tokio::runtime::Runtime>) -> Self {
        Self { runtime }
    }

    /// Spawn an async task on the runtime.
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }
}

/// Standard interface for a DAW extension module.
///
/// Libraries implement this trait and export a `pub fn module() -> Box<dyn DawModule>`
/// function. The host extension collects modules and calls these methods during startup.
///
/// # Lifecycle
///
/// 1. Host creates all modules: `let modules = vec![foo::module(), bar::module()]`
/// 2. Host calls `init_all(&modules, &ctx)` — each module initializes
/// 3. Host calls `collect_actions(&modules)` — gathers all action defs for registration
/// 4. Host calls `collect_panels(&modules)` — gathers all panel defs for registration
/// 5. Host registers actions and panels with the DAW
/// 6. When an action is triggered, host dispatches to the handler
pub trait DawModule: Send + Sync {
    /// Unique module identifier (e.g. "session", "sync", "input").
    fn name(&self) -> &str;

    /// Human-readable display name (e.g. "Session Control", "Transport Sync").
    fn display_name(&self) -> &str;

    /// Return all actions this module provides.
    /// Called once at startup.
    fn actions(&self) -> Vec<ActionDef>;

    /// Return all UI panels this module provides.
    /// Called once at startup. The host creates dock windows for each panel
    /// using the DAW's native panel system (e.g. reaper-dioxus).
    fn panels(&self) -> Vec<PanelDef> {
        vec![]
    }

    /// Initialize the module. Called once after DAW is available.
    /// Use for one-time setup: loading config, building caches, etc.
    fn init(&self, _ctx: &ModuleContext) {}

    /// Subscribe to DAW events (track changes, transport, etc.)
    /// Called once after init. Use `ctx.spawn()` for async event listeners.
    fn subscribe(&self, _ctx: &ModuleContext) {}
}

// ── Panel Definitions ──────────────────────────────────────────────────────

/// A UI panel that a module registers with the host.
///
/// The host creates a dockable window for each panel and renders the
/// Dioxus component inside it. Panels are toggled via their associated
/// action (the host auto-generates a toggle action if `toggle_action` is set).
pub struct PanelDef {
    /// Unique panel ID (e.g. "FTS_LAUNCHER", "FTS_INPUT_STATUS").
    /// Used as the REAPER docker ID for state persistence.
    pub id: &'static str,

    /// Display title shown in the panel's title bar.
    pub title: &'static str,

    /// The Dioxus root component for this panel.
    /// Must be a `fn() -> Element` — the component manages its own state
    /// via signals, context, or module-level singletons.
    pub component: PanelComponent,

    /// Where the panel docks by default.
    pub default_dock: DockPosition,

    /// Which renderer backend should host this panel.
    pub renderer: PanelRenderer,

    /// Default panel size (width, height) in logical pixels.
    pub default_size: (f64, f64),

    /// If set, the host auto-generates a toggle action with this command ID.
    /// The action shows/hides the panel.
    pub toggle_action: Option<&'static str>,
}

/// Renderer backend for a Dioxus panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRenderer {
    /// Existing native renderer backed by Blitz/Vello.
    Native,
    /// Browser-backed Dioxus desktop renderer hosted in an embedded webview.
    Desktop,
}

/// A panel's root component — opaque function pointer.
///
/// This is `fn() -> dioxus::prelude::Element` but we store it as a type-erased
/// pointer since the `daw` crate doesn't depend on Dioxus. The host (which does
/// depend on Dioxus) casts it back.
///
/// # Safety
///
/// The function pointer must be a valid Dioxus component: `fn() -> Element`.
/// The host verifies this at registration time.
#[derive(Clone, Copy)]
pub struct PanelComponent {
    /// Type-erased function pointer. The host casts this to `fn() -> Element`.
    ptr: *const (),
}

unsafe impl Send for PanelComponent {}
unsafe impl Sync for PanelComponent {}

impl PanelComponent {
    /// Create from a Dioxus component function.
    ///
    /// # Example
    /// ```rust,ignore
    /// use dioxus::prelude::*;
    /// fn my_panel() -> Element { rsx! { div { "Hello" } } }
    /// PanelComponent::new(my_panel)
    /// ```
    pub fn new<F>(component: F) -> Self
    where
        F: Fn() + 'static,
    {
        Self {
            ptr: &component as *const F as *const (),
        }
    }

    /// Create from a raw function pointer.
    /// Use this when you have a `fn() -> Element` directly.
    pub fn from_fn_ptr(ptr: *const ()) -> Self {
        Self { ptr }
    }

    /// Get the raw pointer (for the host to cast back).
    pub fn as_ptr(&self) -> *const () {
        self.ptr
    }
}

impl std::fmt::Debug for PanelComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PanelComponent({:p})", self.ptr)
    }
}

/// Where a panel docks by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    /// Floating window (not docked).
    Floating,
    /// Docked at the bottom of the arrange view.
    Bottom,
    /// Docked on the left side.
    Left,
    /// Docked on the right side.
    Right,
    /// Docked at the top.
    Top,
}

/// Collect all action tuples from a list of modules.
pub fn collect_actions(modules: &[Box<dyn DawModule>]) -> Vec<ActionRegistrationTuple> {
    let mut all = Vec::new();
    for m in modules {
        let actions = m.actions();
        tracing::info!(
            module = m.name(),
            actions = actions.len(),
            "Collected actions from {}",
            m.display_name()
        );
        all.extend(actions.into_iter().map(|a| a.into_tuple()));
    }
    all
}

/// Collect all panel definitions from a list of modules.
pub fn collect_panels(modules: &[Box<dyn DawModule>]) -> Vec<PanelDef> {
    let mut all = Vec::new();
    for m in modules {
        let panels = m.panels();
        if !panels.is_empty() {
            tracing::info!(
                module = m.name(),
                panels = panels.len(),
                "Collected panels from {}",
                m.display_name()
            );
        }
        all.extend(panels);
    }
    all
}

/// Initialize all modules and subscribe to events.
pub fn init_all(modules: &[Box<dyn DawModule>], ctx: &ModuleContext) {
    for m in modules {
        tracing::info!(module = m.name(), "Initializing {}", m.display_name());
        m.init(ctx);
    }
    for m in modules {
        m.subscribe(ctx);
    }
    tracing::info!(modules = modules.len(), "All modules initialized");
}
