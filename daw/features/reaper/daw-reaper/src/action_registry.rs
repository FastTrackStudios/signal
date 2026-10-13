//! REAPER Action Registry Implementation
//!
//! Registers actions with REAPER's action system using `reaper_high::Reaper::register_action`.
//! Tracks registered actions so they can be unregistered when a guest disconnects.
//!
//! When a registered action is triggered, all subscribers receive an
//! `ActionEvent::Triggered` event. Guests handle action logic — the host
//! is domain-agnostic.
//!
//! Actions registered with `show_in_menu: true` are automatically added to the
//! Extensions > FastTrackStudio menu. The menu hierarchy is derived from the
//! command name prefix (e.g., `FTS_SESSION_*` → Session submenu).
#![allow(dead_code)]

use crate::main_thread;
use daw_control::lock::LockExt;
use daw_proto::{
    ActionExecutionResult, ActionInfo, ActionListFilter, ActionListRequest, ActionListResponse,
    ActionOrigin, ActionRegistration, ActionSection, DawError, DawResult,
};
use reaper_high::{ActionKind, Reaper, RegisteredAction};
use reaper_medium::{
    CommandId, Handle, Hmenu, HookCustomMenu, MenuHookFlag, OwnedGaccelRegister, ReaperStr,
    SectionContext, SectionId,
};
use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, CString};
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Owned action keepalive — drops `RegisteredAction` and removes the
/// manually-registered gaccel when unregistered. `Handle<gaccel_register_t>`
/// wraps a NonNull<reaper_low::raw::gaccel_register_t> which is owned by
/// REAPER itself (the medium session keeps the backing storage alive). The
/// pointer is only deref'd on the REAPER main thread through
/// `plugin_register_remove_gaccel`, so we can safely impl Send/Sync for
/// the carrier type.
struct OwnedAction {
    action: RegisteredAction,
    /// `None` when REAPER itself already registered the gaccel (e.g. the
    /// command name appeared in `reaper-menu.ini`'s Main toolbar before our
    /// extension's spawned task ran). In that case the action is in REAPER's
    /// list already and we never minted a handle to remove.
    gaccel_handle: Option<Handle<reaper_low::raw::gaccel_register_t>>,
}

// SAFETY: `OwnedAction` is only mutated / dropped via main_thread::query,
// which serialises on REAPER's main thread. The `Handle` and
// `RegisteredAction` interior pointers are never deref'd from other
// threads — we only need Send so the OnceLock<Mutex<..>> map can hold them.
unsafe impl Send for OwnedAction {}

/// Tracks actions registered through this service.
///
/// Maps command_name → command_id for actions we've registered.
/// Used for unregistration and to avoid double-registering.
static REGISTERED_ACTIONS: std::sync::OnceLock<Mutex<HashMap<String, u32>>> =
    std::sync::OnceLock::new();

/// Live `RegisteredAction` + manual gaccel handle keepalives. Dropping
/// the action removes it from `reaper_high`'s internal command map (so
/// REAPER's hook lookup won't dispatch through it any more), and the
/// gaccel handle lets us call `plugin_register_remove_gaccel` to take
/// the action OUT of REAPER's action list.
static OWNED_ACTIONS: std::sync::OnceLock<Mutex<HashMap<String, OwnedAction>>> =
    std::sync::OnceLock::new();

fn owned_actions() -> &'static Mutex<HashMap<String, OwnedAction>> {
    OWNED_ACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Broadcast channel for action trigger events.
///
/// Each subscriber gets their own `broadcast::Receiver` which forwards
/// events to their vox `Tx<ActionEvent>`.
static ACTION_BROADCASTER: std::sync::OnceLock<broadcast::Sender<String>> =
    std::sync::OnceLock::new();

/// Menu metadata for actions that should appear in the Extensions menu.
static MENU_ACTIONS: std::sync::OnceLock<Mutex<Vec<MenuActionDef>>> = std::sync::OnceLock::new();

/// Toggle state for toggleable actions.
///
/// Maps command_name → current on/off state. REAPER queries toggle state
/// synchronously on the main thread, so we store it here for instant access.
/// Guests update this via `set_toggle_state`.
static TOGGLE_STATES: std::sync::OnceLock<Mutex<HashMap<String, bool>>> =
    std::sync::OnceLock::new();

const SHARED_TOGGLE_EXTSTATE_SECTION: &str = "FastTrackStudio.ActionToggleState";

/// Action metadata stored for menu building.
#[derive(Clone)]
struct MenuActionDef {
    /// REAPER command name (e.g., "FTS_SESSION_TOGGLE_PLAYBACK")
    command_name: String,
    /// Display name shown in menu (the description from registration)
    display_name: String,
    /// Menu group derived from command name (e.g., "Session")
    group: String,
}

pub(crate) fn registered_actions() -> &'static Mutex<HashMap<String, u32>> {
    REGISTERED_ACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Subscribe directly to action trigger broadcasts.
///
/// Returns a receiver that yields command names whenever an action is triggered.
/// Useful for in-process extensions (LocalCaller) that want to avoid the vox
/// streaming round-trip.
pub fn subscribe_action_broadcasts() -> broadcast::Receiver<String> {
    action_broadcaster().subscribe()
}

fn action_broadcaster() -> &'static broadcast::Sender<String> {
    ACTION_BROADCASTER.get_or_init(|| {
        let (tx, _rx) = broadcast::channel::<String>(64);
        tx
    })
}

fn menu_actions() -> &'static Mutex<Vec<MenuActionDef>> {
    MENU_ACTIONS.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn toggle_states() -> &'static Mutex<HashMap<String, bool>> {
    TOGGLE_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Read toggle state for a command. Called from REAPER's main thread
/// via the `ActionKind::Toggleable` closure.
fn read_toggle_state(command_name: &str) -> bool {
    let state = toggle_states()
        .lock()
        .unwrap()
        .get(command_name)
        .copied()
        .unwrap_or(false);
    // Trace every REAPER toggleaction hook query so we can confirm
    // REAPER is actually consulting our state when repainting the
    // action list / toolbars. If this trace never fires, REAPER never
    // queries our hook and the indicator can never update.
    tracing::trace!(target: "toggle_hook", "{} -> {}", command_name, state);
    state
}

/// Derive a menu group name from a REAPER command name.
///
/// `FTS_SESSION_TOGGLE_PLAYBACK` → "Session"
/// `FTS_TRANSPORT_PLAY_STOP` → "Transport"
/// `FTS_MARKERS_REGIONS_INSERT_MARKER` → "Markers Regions"
///
/// Convention: strip "FTS_" prefix, then take segments until we hit
/// a lowercase-starting word (action names are all-caps in the prefix part,
/// but after titlecasing they become mixed). Since the raw command name is
/// ALL_CAPS, we use a heuristic: known domain prefixes get titlecased.
fn derive_menu_group(command_name: &str) -> String {
    let name = command_name.strip_prefix("FTS_").unwrap_or(command_name);

    // Known domain prefixes (order matters — longest match first)
    let known_domains = [
        "MARKERS_REGIONS",
        "DYNAMIC_TEMPLATE",
        "VISIBILITY_MANAGER",
        "AUTO_COLOR",
        "REAPER_EXTENSION",
        "TRANSPORT",
        "SESSION",
        "SIGNAL",
        "SYNC",
        "DAW",
    ];

    for domain in &known_domains {
        if name.starts_with(domain) {
            return titlecase_underscored(domain);
        }
    }

    // Fallback: use first segment
    name.split('_')
        .next()
        .map(titlecase_underscored)
        .unwrap_or_default()
}

/// Titlecase an underscored string: "MARKERS_REGIONS" → "Markers Regions"
fn titlecase_underscored(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    let lower: String = chars.as_str().to_lowercase();
                    format!("{upper}{lower}")
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Notify all subscribers that an action was triggered.
///
/// Called from the REAPER action handler closure on the main thread.
/// Uses a broadcast channel so this is non-blocking.
fn notify_action_triggered(command_name: String) {
    let tx = action_broadcaster();
    // Logged either way: "REAPER never called us" and "REAPER called us and
    // nobody was listening" are different bugs that look identical from the
    // outside — an action that appears to do nothing.
    let receivers = tx.receiver_count();
    debug!(%command_name, receivers, "action triggered");
    if receivers == 0 {
        return;
    }
    let _ = tx.send(command_name);
}

fn flip_toggle_state(command_name: &str) -> bool {
    let mut states = toggle_states().lock_recoverable("action_registry");
    let state = states.entry(command_name.to_string()).or_insert(false);
    *state = !*state;
    *state
}

fn named_command_lookup(command_name: &str) -> Option<CommandId> {
    let medium = Reaper::get().medium_reaper();
    medium.named_command_lookup(command_name).or_else(|| {
        if command_name.starts_with('_') {
            None
        } else {
            medium.named_command_lookup(format!("_{command_name}"))
        }
    })
}

fn normalize_command_name(command_name: &str) -> &str {
    command_name.strip_prefix('_').unwrap_or(command_name)
}

fn read_shared_toggle_state(command_name: &str) -> Option<bool> {
    let section = CString::new(SHARED_TOGGLE_EXTSTATE_SECTION).ok()?;
    let key = CString::new(normalize_command_name(command_name)).ok()?;
    let value = unsafe {
        let ptr = Reaper::get()
            .medium_reaper()
            .low()
            .GetExtState(section.as_ptr(), key.as_ptr());
        if ptr.is_null() {
            return None;
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    };

    match value.as_str() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

fn delete_shared_toggle_state(command_name: &str) {
    let Ok(section) = CString::new(SHARED_TOGGLE_EXTSTATE_SECTION) else {
        return;
    };
    let Ok(key) = CString::new(normalize_command_name(command_name)) else {
        return;
    };
    unsafe {
        Reaper::get()
            .medium_reaper()
            .low()
            .DeleteExtState(section.as_ptr(), key.as_ptr(), false);
    }
}

fn write_shared_toggle_state(command_name: &str, is_on: bool) {
    let Ok(section) = CString::new(SHARED_TOGGLE_EXTSTATE_SECTION) else {
        return;
    };
    let Ok(key) = CString::new(normalize_command_name(command_name)) else {
        return;
    };
    let value = if is_on { c"1".as_ptr() } else { c"0".as_ptr() };
    unsafe {
        Reaper::get().medium_reaper().low().SetExtState(
            section.as_ptr(),
            key.as_ptr(),
            value,
            false,
        );
    }
}

fn read_reaper_toggle_state(
    medium: &reaper_medium::Reaper,
    section_id: u32,
    section_context: SectionContext<'_>,
    command_id: CommandId,
) -> Option<bool> {
    let hook_state = unsafe {
        medium
            .low()
            .GetToggleCommandStateThroughHooks(section_context.to_raw(), command_id.to_raw())
    };
    if hook_state != -1 {
        return Some(hook_state != 0);
    }

    medium.get_toggle_command_state_ex(SectionId::new(section_id), command_id)
}

fn write_reaper_toggle_state(section_id: u32, command_id: CommandId, is_on: bool) {
    let medium = Reaper::get().medium_reaper();
    let state = i32::from(is_on);
    if !medium
        .low()
        .SetToggleCommandState(section_id as i32, command_id.get() as i32, state)
    {
        debug!(
            "SetToggleCommandState({}, {}, {}) returned false",
            section_id,
            command_id.get(),
            state
        );
    }
    medium
        .low()
        .RefreshToolbar2(section_id as i32, command_id.get() as i32);
}

fn execute_main_action(medium: &reaper_medium::Reaper, command_id: CommandId) {
    unsafe {
        medium
            .low()
            .KBD_OnMainActionEx(command_id.to_raw(), 0, -1, 0, null_mut(), null_mut());
    }
}

fn action_toggle_state(
    medium: &reaper_medium::Reaper,
    section_id: u32,
    section_context: SectionContext<'_>,
    command_id: CommandId,
    command_name: Option<&str>,
    toggles: &HashMap<String, bool>,
) -> Option<bool> {
    if let Some(state) = command_name
        .map(normalize_command_name)
        .and_then(|name| toggles.get(name).copied())
    {
        return Some(state);
    }
    if let Some(state) = command_name.and_then(read_shared_toggle_state) {
        return Some(state);
    }

    read_reaper_toggle_state(medium, section_id, section_context, command_id)
}

fn is_sws_action(command_name: Option<&str>, description: &str) -> bool {
    let normalized = command_name.map(normalize_command_name);
    let name_matches = normalized.is_some_and(|name| {
        name.starts_with("SWS")
            || name.starts_with("S&M")
            || name.starts_with("BR_")
            || name.starts_with("FNG_")
            || name.starts_with("NF_")
            || name.starts_with("SN_")
            || name.starts_with("XENAKIOS")
            || name.starts_with("PADRE")
            || name.starts_with("AUTORENDER")
    });
    let desc_matches = description.starts_with("SWS")
        || description.starts_with("S&M")
        || description.starts_with("BR:")
        || description.starts_with("FNG:")
        || description.starts_with("NF:")
        || description.starts_with("SN:")
        || description.starts_with("Xenakios");
    name_matches || desc_matches
}

fn action_provider(command_name: Option<&str>, description: &str) -> (String, Vec<String>) {
    let mut tags = Vec::new();
    let normalized = command_name.map(normalize_command_name);
    if let Some(name) = normalized {
        if name.starts_with("FTS") {
            tags.push("fasttrackstudio".to_string());
            return ("fts".to_string(), tags);
        }
        if name.starts_with("RS") {
            tags.push("script".to_string());
            return ("reascript".to_string(), tags);
        }
        let sws_tags = [
            ("SWS", "sws"),
            ("S&M", "s&m"),
            ("BR_", "br"),
            ("FNG_", "fng"),
            ("NF_", "nf"),
            ("SN_", "sn"),
            ("XENAKIOS", "xenakios"),
            ("PADRE", "padre"),
            ("AUTORENDER", "autorender"),
        ];
        for (prefix, tag) in sws_tags {
            if name.starts_with(prefix) {
                tags.push(tag.to_string());
                return ("sws".to_string(), tags);
            }
        }
    }
    if description.starts_with("SWS") {
        tags.push("sws".to_string());
        return ("sws".to_string(), tags);
    }
    if description.starts_with("S&M") {
        tags.push("s&m".to_string());
        return ("sws".to_string(), tags);
    }
    for (prefix, tag) in [
        ("BR:", "br"),
        ("FNG:", "fng"),
        ("NF:", "nf"),
        ("SN:", "sn"),
        ("Xenakios", "xenakios"),
    ] {
        if description.starts_with(prefix) {
            tags.push(tag.to_string());
            return ("sws".to_string(), tags);
        }
    }
    if command_name.is_some() {
        ("extension".to_string(), tags)
    } else {
        ("reaper".to_string(), tags)
    }
}

fn classify_action(command_name: Option<&str>, description: &str) -> ActionOrigin {
    if let Some(name) = command_name.map(normalize_command_name)
        && name.starts_with("FTS")
    {
        return ActionOrigin::Fts;
    }
    if is_sws_action(command_name, description) {
        return ActionOrigin::Sws;
    }
    if command_name.is_some() {
        ActionOrigin::Extension
    } else {
        ActionOrigin::Reaper
    }
}

fn action_section_label(section: ActionSection) -> (u32, String) {
    (section.unique_id(), section.name())
}

fn section_context_for<'a>(
    _section_id: u32,
    raw: &'a reaper_medium::KbdSectionInfo,
) -> SectionContext<'a> {
    SectionContext::Sec(raw)
}

fn action_matches_filter(info: &ActionInfo, filter: ActionListFilter) -> bool {
    match filter {
        ActionListFilter::All => true,
        ActionListFilter::Reaper => info.origin == ActionOrigin::Reaper,
        ActionListFilter::NonReaper => info.origin != ActionOrigin::Reaper,
        ActionListFilter::Sws => info.origin == ActionOrigin::Sws,
        ActionListFilter::Fts => info.origin == ActionOrigin::Fts,
        ActionListFilter::Registered => {
            info.registered_by_fts
                || (info.origin == ActionOrigin::Fts && info.command_name.is_some())
        }
    }
}

fn action_matches_query(info: &ActionInfo, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    info.description.to_ascii_lowercase().contains(query)
        || info
            .command_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(query))
}

fn find_action_info(command_id: CommandId, section: ActionSection) -> Option<ActionInfo> {
    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();
    let registered = registered_actions()
        .lock_recoverable("action_registry")
        .clone();
    let toggles = toggle_states().lock_recoverable("action_registry").clone();
    let (section_id, section_name) = action_section_label(section);
    let section = reaper.section_by_id(SectionId::new(section_id));

    section
        .with_raw(|s| {
            for i in 0..s.action_list_cnt() {
                let Some(cmd) = s.get_action_by_index(i).map(|a| a.cmd()) else {
                    continue;
                };
                if cmd != command_id {
                    continue;
                }

                let section_context = section_context_for(section_id, s);
                let description = unsafe {
                    medium.kbd_get_text_from_cmd(cmd, section_context, |name| name.to_string())
                }
                .unwrap_or_default();
                let command_name =
                    medium.reverse_named_command_lookup(cmd, |name| name.to_string());
                let normalized = command_name.as_deref().map(normalize_command_name);
                let registered_by_fts = normalized
                    .is_some_and(|name| registered.contains_key(name))
                    || registered.values().any(|id| *id == cmd.get());
                let toggle_state = action_toggle_state(
                    medium,
                    section_id,
                    section_context,
                    cmd,
                    command_name.as_deref(),
                    &toggles,
                );
                let origin = if registered_by_fts {
                    ActionOrigin::Fts
                } else {
                    classify_action(command_name.as_deref(), &description)
                };
                let (provider, provider_tags) = if registered_by_fts {
                    ("fts".to_string(), vec!["fasttrackstudio".to_string()])
                } else {
                    action_provider(command_name.as_deref(), &description)
                };

                return Some(ActionInfo {
                    command_id: cmd.get(),
                    section_id,
                    section_name: section_name.clone(),
                    command_name,
                    description,
                    origin,
                    provider,
                    provider_tags,
                    registered_by_fts,
                    toggle_state,
                });
            }
            None
        })
        .flatten()
}

// ============================================================================
// Extensions Menu
// ============================================================================

/// Register the Extensions menu hook with REAPER.
///
/// Call once during plugin initialization after `ReaperSession` is created.
/// The hook is invoked each time REAPER shows the Extensions menu,
/// dynamically building it from all registered actions with `show_in_menu`.
pub fn register_extension_menu(session: &mut reaper_medium::ReaperSession) {
    Reaper::get().medium_reaper().add_extensions_main_menu();
    if let Err(e) = session.plugin_register_add_hook_custom_menu::<FtsMenuHook>() {
        warn!("Failed to register menu hook: {:?}", e);
    } else {
        info!("Extensions menu hook registered");
    }
}

/// REAPER menu hook implementation for FastTrackStudio.
struct FtsMenuHook;

impl HookCustomMenu for FtsMenuHook {
    fn call(menuidstr: &ReaperStr, hmenu: Hmenu, flag: MenuHookFlag) {
        let result = std::panic::catch_unwind(|| {
            if flag != MenuHookFlag::Init || menuidstr.to_str() != "Main extensions" {
                return;
            }
            build_extension_menu(hmenu);
        });
        if let Err(e) = result {
            warn!("Panic in menu hook: {:?}", e);
        }
    }
}

/// Build the Extensions > FastTrackStudio menu from registered actions.
fn build_extension_menu(hmenu: Hmenu) {
    let actions = menu_actions().lock_recoverable("action_registry").clone();
    if actions.is_empty() {
        return;
    }

    let swell = reaper_low::Swell::get();

    // Group actions by their derived menu group
    let mut groups: BTreeMap<String, Vec<MenuActionDef>> = BTreeMap::new();
    for action in &actions {
        groups
            .entry(action.group.clone())
            .or_default()
            .push(action.clone());
    }

    // Create "FastTrackStudio" submenu
    let fts_menu = swell.CreatePopupMenu();

    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();

    for (group_name, group_actions) in &mut groups {
        // Create submenu for this group
        let submenu = swell.CreatePopupMenu();

        // Sort actions by display name
        group_actions.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        for action in group_actions.iter() {
            let lookup = format!("_{}", action.command_name);
            if let Some(cmd_id) = medium.named_command_lookup(lookup) {
                let mut text_buf: Vec<u8> = action.display_name.as_bytes().to_vec();
                text_buf.push(0);
                let mut mii = reaper_low::raw::MENUITEMINFO {
                    fMask: 0x40 | 0x04, // MIIM_TYPE | MIIM_ID
                    fType: 0,           // MFT_STRING
                    wID: cmd_id.get(),
                    hSubMenu: std::ptr::null_mut(),
                    dwTypeData: text_buf.as_mut_ptr() as *mut _,
                    ..unsafe { std::mem::zeroed() }
                };
                let count = unsafe { swell.GetMenuItemCount(submenu) };
                unsafe {
                    swell.InsertMenuItem(submenu, count, 1, &mut mii);
                }
            }
        }

        // Add the group submenu to the FTS menu
        let mut label_buf: Vec<u8> = group_name.as_bytes().to_vec();
        label_buf.push(0);
        let mut mii = reaper_low::raw::MENUITEMINFO {
            fMask: 0x10 | 0x40 | 0x04, // MIIM_SUBMENU | MIIM_TYPE | MIIM_ID
            fType: 0,
            wID: 0,
            hSubMenu: submenu,
            dwTypeData: label_buf.as_mut_ptr() as *mut _,
            ..unsafe { std::mem::zeroed() }
        };
        let count = unsafe { swell.GetMenuItemCount(fts_menu) };
        unsafe {
            swell.InsertMenuItem(fts_menu, count, 1, &mut mii);
        }
    }

    // Insert "FastTrackStudio" into the Extensions menu
    let parent = hmenu.as_ptr();
    let mut label = b"FastTrackStudio\0".to_vec();

    // Add separator before our menu if there are already items
    let existing = unsafe { swell.GetMenuItemCount(parent) };
    if existing > 0 {
        let mut sep = reaper_low::raw::MENUITEMINFO {
            fMask: 0x40,  // MIIM_TYPE
            fType: 0x800, // MFT_SEPARATOR
            ..unsafe { std::mem::zeroed() }
        };
        unsafe {
            swell.InsertMenuItem(parent, existing, 1, &mut sep);
        }
    }

    let mut mii = reaper_low::raw::MENUITEMINFO {
        fMask: 0x10 | 0x40 | 0x04, // MIIM_SUBMENU | MIIM_TYPE | MIIM_ID
        fType: 0,
        wID: 0,
        hSubMenu: fts_menu,
        dwTypeData: label.as_mut_ptr() as *mut _,
        ..unsafe { std::mem::zeroed() }
    };
    let pos = unsafe { swell.GetMenuItemCount(parent) };
    unsafe {
        swell.InsertMenuItem(parent, pos, 1, &mut mii);
    }

    debug!(
        "Built FastTrackStudio menu ({} actions in {} groups)",
        actions.len(),
        groups.len()
    );
}

fn action_list_contains(command_id: CommandId) -> bool {
    Reaper::get()
        .main_section()
        .with_raw(|s| {
            (0..s.action_list_cnt()).any(|i| {
                s.get_action_by_index(i)
                    .map(|a| a.cmd() == command_id)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

pub fn register_action_main_thread(
    command_name: &str,
    description: &str,
    show_in_menu: bool,
    toggleable: bool,
) -> u32 {
    let command_for_trigger = command_name.to_string();
    register_action_core(
        command_name,
        description,
        show_in_menu,
        toggleable,
        move || {
            // Fake-toggle state is host-managed so REAPER's UI (and
            // execute_action's before/after report) updates synchronously
            // on trigger; consumers handle the domain logic via the
            // broadcast.
            if toggleable {
                let state = flip_toggle_state(&command_for_trigger);
                write_shared_toggle_state(&command_for_trigger, state);
                if let Some(cmd_id) = named_command_lookup(&command_for_trigger) {
                    write_reaper_toggle_state(0, cmd_id, state);
                }
                debug!("Toggle state for '{}' -> {}", command_for_trigger, state);
            }
            notify_action_triggered(command_for_trigger.clone());
        },
    )
}

/// Registers `command_name` with REAPER, invoking `handler` directly when
/// triggered — no broadcast hop. Used by the `architect::action::ActionBackend`
/// adapter (see `impl ActionBackend for crate::Reaper` below), whose handler
/// closures are supplied per-action by `#[architect::actions]`-generated
/// registration code rather than dispatched by matching a broadcast command
/// name downstream.
pub fn register_action_with_handler(
    command_name: &str,
    description: &str,
    show_in_menu: bool,
    toggleable: bool,
    handler: std::sync::Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
) -> u32 {
    let command_for_error = command_name.to_string();
    register_action_core(
        command_name,
        description,
        show_in_menu,
        toggleable,
        move || {
            if let Err(message) = handler() {
                show_action_error(&command_for_error, &message);
            }
        },
    )
}

/// Pop a REAPER message box reporting a failed action — the only place a
/// `#[architect::actions]` method's `Err` reaches the user; everything
/// upstream of this (the macro's generated closure, `ActionBackend`) only
/// carries the message as a `String`. REAPER action triggers always run on
/// the main thread, so `show_message_box` (main-thread-only, blocking) is
/// safe to call directly here.
fn show_action_error(command_name: &str, message: &str) {
    warn!("Action '{command_name}' failed: {message}");
    Reaper::get().medium_reaper().show_message_box(
        message,
        format!("FastTrackStudio — {command_name}"),
        reaper_medium::MessageBoxType::Okay,
    );
}

/// Shared REAPER action-registration mechanics (dedup, gaccel, menu
/// bookkeeping) used by both the broadcast-based `register_action_main_thread`
/// and the direct-handler `register_action_with_handler`. `trigger` is the
/// callback REAPER invokes when the action fires — the only thing that
/// differs between the two callers.
fn register_action_core(
    command_name: &str,
    description: &str,
    show_in_menu: bool,
    toggleable: bool,
    trigger: impl FnMut() + 'static,
) -> u32 {
    if let Some(id) = registered_actions()
        .lock_recoverable("action_registry")
        .get(command_name)
        .copied()
    {
        return id;
    }

    if let Some(command_id) = named_command_lookup(command_name) {
        let id = command_id.get();
        registered_actions()
            .lock_recoverable("action_registry")
            .insert(command_name.to_string(), id);
        if show_in_menu {
            menu_actions()
                .lock_recoverable("action_registry")
                .push(MenuActionDef {
                    command_name: command_name.to_string(),
                    display_name: description.to_string(),
                    group: derive_menu_group(command_name),
                });
        }
        return id;
    }

    let kind = if toggleable {
        let command_for_toggle = command_name.to_string();
        ActionKind::Toggleable(Box::new(move || read_toggle_state(&command_for_toggle)))
    } else {
        ActionKind::NotToggleable
    };
    let action = Reaper::get().register_action(
        command_name.to_string(),
        description.to_string(),
        None,
        trigger,
        kind,
    );
    let command_id = action.command_id();

    let gaccel_handle = if action_list_contains(command_id) {
        None
    } else {
        let gaccel = OwnedGaccelRegister::without_key_binding(command_id, description.to_string());
        match Reaper::get()
            .medium_session()
            .plugin_register_add_gaccel(gaccel)
        {
            Ok(handle) => Some(handle),
            Err(err) => {
                warn!("Failed to register gaccel for '{command_name}': {err:?}");
                None
            }
        }
    };

    let id = command_id.get();
    registered_actions()
        .lock_recoverable("action_registry")
        .insert(command_name.to_string(), id);
    // Toggleable actions start with a recorded OFF state so
    // `get_toggle_state` distinguishes "toggleable, currently off"
    // (Some(false)) from "not toggleable" (None).
    if toggleable {
        toggle_states()
            .lock_recoverable("action_registry")
            .entry(command_name.to_string())
            .or_insert(false);
    }
    owned_actions().lock_recoverable("action_registry").insert(
        command_name.to_string(),
        OwnedAction {
            action,
            gaccel_handle,
        },
    );
    if show_in_menu {
        menu_actions()
            .lock_recoverable("action_registry")
            .push(MenuActionDef {
                command_name: command_name.to_string(),
                display_name: description.to_string(),
                group: derive_menu_group(command_name),
            });
    }

    id
}

/// REAPER's MIDI editor action section.
///
/// Actions live in sections, and the MIDI editor's toolbar and key map
/// only see actions registered in *this* one. An action registered the
/// normal way lands in Main (0) — it shows up in the main action list and
/// works on a global keybind, but a MIDI-toolbar button referencing it
/// silently does nothing, because REAPER cannot resolve the command in
/// the section the toolbar belongs to.
pub const MIDI_EDITOR_SECTION_ID: i32 = 32060;

/// Handlers for actions registered outside the Main section, by command id.
///
/// Needed because REAPER dispatches those through a *different* callback —
/// see [`FtsSectionHook`].
fn section_handlers() -> &'static Mutex<HashMap<u32, Arc<dyn Fn() + Send + Sync>>> {
    static HANDLERS: OnceLock<Mutex<HashMap<u32, Arc<dyn Fn() + Send + Sync>>>> = OnceLock::new();
    HANDLERS.get_or_init(Default::default)
}

/// Dispatches actions that live outside the Main section.
///
/// reaper-high registers `hook_command`, which REAPER only calls for the
/// Main section. Anything registered into another section (the MIDI
/// editor, say) arrives through `hook_command_2` instead, carrying the
/// section it fired in. Without this, a section action registers fine,
/// shows up in that section's action list, accepts a keybind and can be
/// put on that section's toolbar — and then does nothing at all when
/// invoked, because no callback is listening. That is not a hypothetical:
/// it is exactly how this presented.
struct FtsSectionHook;

impl reaper_medium::HookCommand2 for FtsSectionHook {
    fn call(
        _section: reaper_medium::SectionContext,
        command_id: reaper_medium::CommandId,
        _value_change: reaper_medium::ActionValueChange,
        _window: reaper_medium::WindowContext,
    ) -> bool {
        // Command ids come from `plugin_register_add_command_id`, which is
        // global, so the id alone identifies the action — no need to match
        // on the section as well.
        let handler = section_handlers()
            .lock_recoverable("section_handlers")
            .get(&command_id.get())
            .cloned();
        match handler {
            Some(handler) => {
                handler();
                true
            }
            // Not ours. Returning false lets REAPER and other extensions
            // have it.
            None => false,
        }
    }
}

/// Install [`FtsSectionHook`] once.
fn ensure_section_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if let Err(err) = Reaper::get()
            .medium_session()
            .plugin_register_add_hook_command_2::<FtsSectionHook>()
        {
            warn!("Failed to register hook_command_2: {err:?} — section actions will not fire");
        } else {
            info!("Registered hook_command_2 for non-Main section actions");
        }
    });
}

/// Additionally expose an action in `section_id`.
///
/// Call this *after* the normal registration. Two things have to happen
/// for a section action to work, and doing only the first is a silent
/// failure:
///
/// 1. **Registration** — a `custom_action` entry in that section, which is
///    what makes the action appear there, bindable and toolbar-able.
/// 2. **Dispatch** — a `hook_command_2` callback, because REAPER does not
///    route non-Main sections through the ordinary `hook_command` that
///    reaper-high installs.
///
/// Note this gets its *own* command id, distinct from the Main-section
/// registration of the same name — REAPER does not dedupe them — which is
/// why the handler is stored per id rather than per name.
///
/// Must be called from the main thread.
pub fn register_action_in_section_main_thread(
    command_name: &str,
    description: &str,
    section_id: i32,
    trigger: Arc<dyn Fn() + Send + Sync>,
) -> u32 {
    ensure_section_hook();

    let action = Reaper::get().register_action_in_section(
        command_name.to_string(),
        description.to_string(),
        section_id,
        {
            let trigger = trigger.clone();
            move || trigger()
        },
        ActionKind::NotToggleable,
    );
    let id = action.command_id().get();

    section_handlers()
        .lock_recoverable("section_handlers")
        .insert(id, trigger);

    info!(command_name, section_id, id, "Registered action in section");
    // The handle is deliberately not kept: `RegisteredAction` has no
    // `Drop` impl (unregistering is the explicit `unregister()` call),
    // so letting it fall out of scope leaves the action registered, and
    // these live as long as the extension. This used to `mem::forget`
    // it, which did the same thing while implying the opposite.
    id
}

pub fn unregister_action_main_thread(command_name: &str) -> bool {
    let removed = owned_actions()
        .lock_recoverable("action_registry")
        .remove(command_name);
    let had_owned = removed.is_some();
    if let Some(handle) = removed.and_then(|owned| owned.gaccel_handle) {
        Reaper::get()
            .medium_session()
            .plugin_register_remove_gaccel(handle);
    }
    let removed = registered_actions()
        .lock_recoverable("action_registry")
        .remove(command_name)
        .is_some()
        || had_owned;
    if removed {
        // Clear any toggle state so a later re-register starts fresh.
        toggle_states()
            .lock_recoverable("action_registry")
            .remove(command_name);
        delete_shared_toggle_state(command_name);
        menu_actions()
            .lock_recoverable("action_registry")
            .retain(|a| a.command_name != command_name);
    }
    removed
}

fn check_registered(command_id: u32, command_name: &str) -> DawResult<u32> {
    if command_id > 0 {
        Ok(command_id)
    } else {
        Err(DawError::operation_failed(format!(
            "register_action returned 0 for '{command_name}'"
        )))
    }
}

fn register_action_threadsafe(
    command_name: &str,
    description: &str,
    show_in_menu: bool,
    toggleable: bool,
) -> u32 {
    if Reaper::get().is_in_main_thread() {
        return register_action_main_thread(command_name, description, show_in_menu, toggleable);
    }

    let Some(task_support) = main_thread::task_support() else {
        return 0;
    };
    let command_name = command_name.to_string();
    let description = description.to_string();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    if task_support
        .do_later_in_main_thread_asap(move || {
            let id =
                register_action_main_thread(&command_name, &description, show_in_menu, toggleable);
            let _ = tx.send(id);
        })
        .is_err()
    {
        return 0;
    }
    rx.recv().unwrap_or(0)
}

fn unregister_action_threadsafe(command_name: &str) -> bool {
    if Reaper::get().is_in_main_thread() {
        return unregister_action_main_thread(command_name);
    }

    let Some(task_support) = main_thread::task_support() else {
        return false;
    };
    let command_name = command_name.to_string();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    if task_support
        .do_later_in_main_thread_asap(move || {
            let removed = unregister_action_main_thread(&command_name);
            let _ = tx.send(removed);
        })
        .is_err()
    {
        return false;
    }
    rx.recv().unwrap_or(false)
}

// ============================================================================
// Action Registry Service
// ============================================================================

/// REAPER action registry implementation.
#[derive(Clone)]
pub struct ReaperActionRegistry;

impl ReaperActionRegistry {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReaperActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// architect::rpc port —
// ============================================================================
//
// TODO: Wire to the existing helpers above (REGISTERED_ACTIONS, action_broadcaster,
// MENU_ACTIONS, etc.) — currently stubbed with todo!() pending full port.

impl ActionRegistration for crate::Reaper {
    fn register_action(
        &self,
        command_name: &str,
        description: &str,
        show_in_menu: bool,
        toggleable: bool,
    ) -> u32 {
        register_action_threadsafe(command_name, description, show_in_menu, toggleable)
    }

    fn register(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        check_registered(
            register_action_threadsafe(cmd_name, description, false, false),
            cmd_name,
        )
    }

    fn register_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        check_registered(
            register_action_threadsafe(cmd_name, description, true, false),
            cmd_name,
        )
    }

    fn register_toggle(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        check_registered(
            register_action_threadsafe(cmd_name, description, false, true),
            cmd_name,
        )
    }

    fn register_toggle_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        check_registered(
            register_action_threadsafe(cmd_name, description, true, true),
            cmd_name,
        )
    }

    fn unregister(&self, cmd_name: &str) -> DawResult<()> {
        // Unregistering an unknown action is a no-op, not an error —
        // callers use this for idempotent teardown.
        if !unregister_action_threadsafe(cmd_name) {
            debug!("unregister: '{cmd_name}' was not registered by us (no-op)");
        }
        Ok(())
    }

    fn is_registered(&self, command_name: &str) -> bool {
        registered_actions()
            .lock_recoverable("action_registry")
            .contains_key(command_name)
            || named_command_lookup(command_name).is_some()
    }

    fn lookup_command_id(&self, command_name: &str) -> Option<u32> {
        registered_actions()
            .lock_recoverable("action_registry")
            .get(command_name)
            .copied()
            .or_else(|| named_command_lookup(command_name).map(|id| id.get()))
    }

    fn is_in_action_list(&self, command_name: &str) -> bool {
        self.lookup_command_id(command_name)
            .map(CommandId::new)
            .is_some_and(action_list_contains)
    }

    fn list_actions(&self, request: ActionListRequest) -> ActionListResponse {
        // Runs on REAPER's main thread (ReaperMainThreadDispatcher).
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let registered = registered_actions()
            .lock_recoverable("action_registry")
            .clone();
        let toggles = toggle_states().lock_recoverable("action_registry").clone();
        let query = request.query.as_ref().map(|q| q.to_ascii_lowercase());
        let limit = request.limit.unwrap_or(u32::MAX) as usize;
        let (section_id, section_name) = action_section_label(request.section);
        let mut actions = Vec::new();
        let mut total_count = 0_u32;

        let section = reaper.section_by_id(SectionId::new(section_id));
        let _ = section.with_raw(|s| {
            for i in 0..s.action_list_cnt() {
                let Some(cmd) = s.get_action_by_index(i).map(|a| a.cmd()) else {
                    continue;
                };

                let section_context = section_context_for(section_id, s);
                let description = unsafe {
                    medium.kbd_get_text_from_cmd(cmd, section_context, |name| name.to_string())
                }
                .unwrap_or_default();
                let command_name =
                    medium.reverse_named_command_lookup(cmd, |name| name.to_string());

                let normalized = command_name.as_deref().map(normalize_command_name);
                let registered_by_fts = normalized
                    .is_some_and(|name| registered.contains_key(name))
                    || registered.values().any(|id| *id == cmd.get());
                let toggle_state = action_toggle_state(
                    medium,
                    section_id,
                    section_context,
                    cmd,
                    command_name.as_deref(),
                    &toggles,
                );
                let origin = if registered_by_fts {
                    ActionOrigin::Fts
                } else {
                    classify_action(command_name.as_deref(), &description)
                };
                let (provider, provider_tags) = if registered_by_fts {
                    ("fts".to_string(), vec!["fasttrackstudio".to_string()])
                } else {
                    action_provider(command_name.as_deref(), &description)
                };

                let info = ActionInfo {
                    command_id: cmd.get(),
                    section_id,
                    section_name: section_name.clone(),
                    command_name,
                    description,
                    origin,
                    provider,
                    provider_tags,
                    registered_by_fts,
                    toggle_state,
                };

                if action_matches_filter(&info, request.filter)
                    && action_matches_query(&info, query.as_deref())
                {
                    if actions.len() >= limit {
                        total_count += 1;
                        continue;
                    }
                    actions.push(info);
                    total_count += 1;
                }
            }
        });

        ActionListResponse {
            total_count,
            actions,
        }
    }

    fn run_action(&self, command_id: u32) {
        let medium = Reaper::get().medium_reaper();
        medium.main_on_command_ex(
            CommandId::new(command_id),
            0,
            reaper_medium::ProjectContext::CurrentProject,
        );
    }

    fn execute_named_action(&self, command_name: &str) -> bool {
        let Some(command_id) = self.lookup_command_id(command_name) else {
            tracing::debug!("execute_named_action: not found: {command_name}");
            return false;
        };
        let medium = Reaper::get().medium_reaper();
        medium.main_on_command_ex(
            CommandId::new(command_id),
            0,
            reaper_medium::ProjectContext::CurrentProject,
        );
        true
    }

    fn execute_action(&self, action_id: &str) -> ActionExecutionResult {
        // Runs on REAPER's main thread (ReaperMainThreadDispatcher).
        let medium = Reaper::get().medium_reaper();
        let requested_action = action_id.to_string();

        let command_id = if let Ok(id) = action_id.parse::<u32>() {
            if id == 0 {
                None
            } else {
                Some(CommandId::new(id))
            }
        } else {
            named_command_lookup(action_id)
        };

        let Some(command_id) = command_id else {
            return ActionExecutionResult {
                requested_action,
                executed: false,
                command_id: None,
                command_name: None,
                description: None,
                origin: None,
                provider: None,
                provider_tags: Vec::new(),
                registered_by_fts: false,
                toggle_state_before: None,
                toggle_state_after: None,
            };
        };

        let before = find_action_info(command_id, ActionSection::Main);
        execute_main_action(medium, command_id);
        let after = find_action_info(command_id, ActionSection::Main);
        let info = after.as_ref().or(before.as_ref());

        ActionExecutionResult {
            requested_action,
            executed: true,
            command_id: Some(command_id.get()),
            command_name: info.and_then(|info| info.command_name.clone()),
            description: info.map(|info| info.description.clone()),
            origin: info.map(|info| info.origin),
            provider: info.map(|info| info.provider.clone()),
            provider_tags: info
                .map(|info| info.provider_tags.clone())
                .unwrap_or_default(),
            registered_by_fts: info.is_some_and(|info| info.registered_by_fts),
            toggle_state_before: before.and_then(|info| info.toggle_state),
            toggle_state_after: after.and_then(|info| info.toggle_state),
        }
    }

    fn set_toggle_state(&self, command_name: &str, is_on: bool) {
        // Only actions registered as TOGGLEABLE carry a toggle state —
        // setting one on a plain action is a no-op (get_toggle_state
        // keeps returning None). register_toggle seeds the map with
        // false, so presence in the map IS the toggleability signal.
        let normalized = normalize_command_name(command_name).to_string();
        let was_known = {
            let mut states = toggle_states().lock_recoverable("action_registry");
            if let Some(state) = states.get_mut(&normalized) {
                *state = is_on;
                true
            } else {
                false
            }
        };

        if !was_known {
            // Cross-process FTS actions (registered by another extension,
            // visible via the shared toggle-state file) may still be
            // toggled; anything else is ignored.
            let Some(cmd_id) = named_command_lookup(command_name) else {
                debug!("set_toggle_state: '{command_name}' unknown — ignored");
                return;
            };
            let Some(info) = find_action_info(cmd_id, ActionSection::Main) else {
                return;
            };
            if info.origin != ActionOrigin::Fts {
                debug!("set_toggle_state: '{command_name}' not an FTS action — ignored");
                return;
            }
            if info.toggle_state.is_none() && read_shared_toggle_state(command_name).is_none() {
                debug!("set_toggle_state: '{command_name}' not toggleable — ignored");
                return;
            }
            write_shared_toggle_state(command_name, is_on);
            write_reaper_toggle_state(0, cmd_id, is_on);
            return;
        }

        write_shared_toggle_state(command_name, is_on);
        // Nudge REAPER to repaint toolbar/menu toggle indicators — it
        // queries the toggleaction hook lazily on repaint.
        if let Some(command_id) = self.lookup_command_id(command_name) {
            write_reaper_toggle_state(0, CommandId::new(command_id), is_on);
        }
    }

    fn get_toggle_state(&self, command_name: &str) -> Option<bool> {
        toggle_states()
            .lock_recoverable("action_registry")
            .get(normalize_command_name(command_name))
            .copied()
            .or_else(|| read_shared_toggle_state(command_name))
    }
}

// ============================================================================
// architect::action::ActionBackend — registers `#[architect::actions]`-
// declared named commands with REAPER.
//
// Distinct from `ActionRegistration` above: that trait is the read/query
// surface (list REAPER's existing actions, execute by id) exposed to RPC
// clients, mid-port to `#[architect::rpc]` per daw's ARCHITECT_REFACTOR.md
// item #15 — untouched here. This impl is the write/registration seam
// consumed by `#[architect::actions]`-generated `register_<trait>_actions`
// functions (see the FTS action-framework migration), one call per action at
// extension startup. It goes straight to `register_action_with_handler`
// (direct trigger, no `notify_action_triggered` broadcast hop) so the
// handler bound into `ActionMeta` at macro-expansion time is exactly what
// REAPER invokes — no separate string-match dispatch step needed downstream.
// ============================================================================

/// Show in the Extensions > FastTrackStudio menu whenever the action
/// declared a category — an uncategorized action is either internal or not
/// yet worth surfacing in the menu tree. Pure translation logic, split out
/// from the `ActionBackend` impl below so it's unit-testable without a live
/// REAPER instance (everything else `register` does bottoms out in
/// `Reaper::get()`).
fn action_show_in_menu(meta: &architect::action::ActionMeta) -> bool {
    !meta.category.is_empty()
}

/// The undo-history label for an `#[action(undo)]` action, composed from
/// the same metadata that names it in REAPER's action list — e.g.
/// `"Session Track Manager - Add Channel"` for a Session/Track Manager
/// action called "Add Channel". Pure translation, split out from the
/// `ActionBackend` impl so it's unit-testable without a live REAPER.
fn action_undo_label(meta: &architect::action::ActionMeta) -> String {
    let scope = [meta.category, meta.group]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if scope.is_empty() {
        meta.display_name.to_string()
    } else {
        format!("{scope} - {}", meta.display_name)
    }
}

impl architect::action::ActionBackend for crate::Reaper {
    fn register(
        &self,
        meta: &'static architect::action::ActionMeta,
        handler: std::sync::Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    ) {
        // `#[action(undo)]` actions run bracketed in a REAPER undo block
        // labelled after the action, so every mutating action is one
        // atomic undo point without each one hand-rolling
        // begin/end_undo_block. Read-only actions opt out (the default)
        // so they don't litter the undo history.
        let handler = if meta.undo {
            let label = action_undo_label(meta);
            std::sync::Arc::new(move || {
                let project = daw_proto::ProjectContext::Current;
                daw_proto::Projects::begin_undo_block(&crate::Reaper, project.clone(), &label);
                let result = handler();
                daw_proto::Projects::end_undo_block(&crate::Reaper, project, &label, None);
                result
            }) as std::sync::Arc<dyn Fn() -> Result<(), String> + Send + Sync>
        } else {
            handler
        };

        register_action_with_handler(
            meta.id,
            meta.description,
            action_show_in_menu(meta),
            meta.toggleable,
            handler,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_meta(category: &'static str) -> architect::action::ActionMeta {
        architect::action::ActionMeta {
            id: "FTS_TEST_ACTION",
            trait_name: "TestActions",
            method_name: "action",
            display_name: "Test Action",
            description: "A test action",
            category,
            group: "",
            // A REAPER named command is reached from REAPER's own action
            // list, which shows the binding the user gave it — so a hint
            // baked in here would be a second, staler answer.
            shortcut: "",
            toggleable: false,
            undo: false,
        }
    }

    #[test]
    fn action_show_in_menu_true_when_categorized() {
        assert!(action_show_in_menu(&test_meta("Setlist")));
    }

    #[test]
    fn action_show_in_menu_false_when_uncategorized() {
        assert!(!action_show_in_menu(&test_meta("")));
    }

    #[test]
    fn derive_menu_group_session() {
        assert_eq!(derive_menu_group("FTS_SESSION_TOGGLE_PLAYBACK"), "Session");
    }

    #[test]
    fn derive_menu_group_transport() {
        assert_eq!(derive_menu_group("FTS_TRANSPORT_PLAY"), "Transport");
    }

    #[test]
    fn derive_menu_group_markers_regions() {
        assert_eq!(
            derive_menu_group("FTS_MARKERS_REGIONS_INSERT_MARKER"),
            "Markers Regions"
        );
    }

    #[test]
    fn derive_menu_group_signal() {
        assert_eq!(derive_menu_group("FTS_SIGNAL_NEXT_SONG"), "Signal");
    }

    #[test]
    fn derive_menu_group_sync() {
        assert_eq!(derive_menu_group("FTS_SYNC_TOGGLE_LINK"), "Sync");
    }

    #[test]
    fn derive_menu_group_dynamic_template() {
        assert_eq!(
            derive_menu_group("FTS_DYNAMIC_TEMPLATE_SORT_ALL"),
            "Dynamic Template"
        );
    }

    #[test]
    fn derive_menu_group_visibility_manager() {
        assert_eq!(
            derive_menu_group("FTS_VISIBILITY_MANAGER_TOGGLE_DRUMS"),
            "Visibility Manager"
        );
    }

    #[test]
    fn derive_menu_group_auto_color() {
        assert_eq!(derive_menu_group("FTS_AUTO_COLOR_COLOR_ALL"), "Auto Color");
    }

    #[test]
    fn derive_menu_group_daw() {
        assert_eq!(derive_menu_group("FTS_DAW_SOMETHING"), "Daw");
    }

    #[test]
    fn derive_menu_group_unknown_prefix_falls_back_to_first_segment() {
        assert_eq!(derive_menu_group("UNKNOWN_PREFIX"), "Unknown");
    }

    #[test]
    fn titlecase_underscored_multi_word() {
        assert_eq!(titlecase_underscored("MARKERS_REGIONS"), "Markers Regions");
    }

    #[test]
    fn titlecase_underscored_single_word() {
        assert_eq!(titlecase_underscored("TRANSPORT"), "Transport");
    }

    #[test]
    fn titlecase_underscored_two_words() {
        assert_eq!(titlecase_underscored("AUTO_COLOR"), "Auto Color");
    }

    #[test]
    fn titlecase_underscored_empty() {
        assert_eq!(titlecase_underscored(""), "");
    }
}
