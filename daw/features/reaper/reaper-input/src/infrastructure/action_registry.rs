use once_cell::sync::Lazy;
use reaper_medium::CommandId;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSection {
    Main,
}

pub struct ActionDef {
    pub command_id: &'static str,
    pub display_name: String,
    pub handler: fn(),
    pub appears_in_menu: bool,
    pub section: ActionSection,
    pub toggle_state: Option<fn() -> bool>,
}

/// A dynamically-generated action with owned strings and closure-based handlers.
///
/// Used for actions whose identity (command ID, name) and behavior are only
/// known at runtime — e.g., one toggle action per loaded workflow.
pub struct DynActionDef {
    pub command_id: String,
    pub display_name: String,
    pub handler: std::sync::Arc<dyn Fn() + Send + Sync>,
    pub appears_in_menu: bool,
    pub section: ActionSection,
    pub toggle_state: Option<std::sync::Arc<dyn Fn() -> bool + Send + Sync>>,
}

static COMMAND_IDS: Lazy<RwLock<HashMap<String, CommandId>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_actions(_actions: &[ActionDef], _namespace: &str) {}

pub fn remember_command_id(action_id: impl Into<String>, command_id: CommandId) {
    if let Ok(mut map) = COMMAND_IDS.write() {
        map.insert(action_id.into(), command_id);
    }
}

pub fn get_command_id(action_id: &str) -> Option<CommandId> {
    COMMAND_IDS
        .read()
        .ok()
        .and_then(|map| map.get(action_id).copied())
}

// ─── Late-registration callback ─────────────────────────────────────────────
//
// reaper-input ships as a library inside the host plugin (today:
// fts-extensions). The host owns the REAPER named-command registry +
// the dispatch table that maps command names → handlers. When the
// workflow watcher discovers a brand-new `.styx` file at runtime, we
// need the host to learn about the new toggle action without a
// REAPER restart.
//
// The host installs a callback here once at startup. The workflow
// reload path calls [`fire_action_registrar`] for each newly-added
// workflow; the callback registers the action with REAPER and adds the
// handler to the host's dispatch table.

/// Signature of the callback the host installs to late-register an
/// action with REAPER.
pub type ActionRegistrar = Box<dyn Fn(DynActionDef) + Send + Sync>;

static ACTION_REGISTRAR: Lazy<RwLock<Option<ActionRegistrar>>> = Lazy::new(|| RwLock::new(None));

/// Install (or replace) the action registrar. Call once from the host
/// plugin's startup after its dispatch table is constructed.
pub fn set_action_registrar(f: ActionRegistrar) {
    if let Ok(mut guard) = ACTION_REGISTRAR.write() {
        *guard = Some(f);
    }
}

/// Fire the registrar (if installed) for a newly-discovered action.
/// No-op when nothing has registered a callback yet — keeps the
/// reaper-input crate usable without a host that supports late
/// registration.
pub fn fire_action_registrar(def: DynActionDef) {
    if let Ok(guard) = ACTION_REGISTRAR.read()
        && let Some(f) = guard.as_ref()
    {
        f(def);
    }
}
