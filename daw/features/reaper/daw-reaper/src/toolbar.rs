//! REAPER Toolbar Service Implementation
//!
//! Manages dynamic toolbar buttons using REAPER's GetCustomMenuOrToolbarItem /
//! AddCustomMenuOrToolbarItem / DeleteCustomMenuOrToolbarItem API.
//!
//! Operations are deferred and applied from the timer callback to avoid
//! re-entrancy issues inside REAPER callbacks.

use daw_proto::toolbar::{
    Toolbar, ToolbarButton, ToolbarIcon, ToolbarItemInfo, ToolbarPlacement, ToolbarResult,
    ToolbarSnapshot, ToolbarSnapshotSource, ToolbarTarget, TrackedButton,
};
use reaper_high::Reaper;
use reaper_medium::{CommandId, MenuOrToolbarItem, PositionDescriptor, UiRefreshBehavior};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

/// REAPER implementation of the ToolbarService.
#[derive(Clone)]
pub struct ReaperToolbar {
    // State is shared via statics because toolbar operations must be deferred
    // to the timer callback (main thread).
}

impl ReaperToolbar {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ReaperToolbar {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct ToolbarState {
    added_buttons: HashMap<(String, String), String>,
}

static STATE: std::sync::OnceLock<Mutex<ToolbarState>> = std::sync::OnceLock::new();

fn state() -> &'static Mutex<ToolbarState> {
    STATE.get_or_init(|| Mutex::new(ToolbarState::default()))
}

// ============================================================================
// Public API for timer callback
// ============================================================================

/// Retained for bridge timer compatibility. Toolbar operations now use
/// `main_thread::query` so callers receive the actual success/error result.
pub fn process_deferred_ops() {}

// ============================================================================
// ToolbarTarget helpers
// ============================================================================

fn target_to_str(target: &ToolbarTarget) -> String {
    match target {
        ToolbarTarget::Main => "Main toolbar".to_string(),
        ToolbarTarget::Floating(n) => format!("Floating toolbar {}", (*n).clamp(1, 32)),
        ToolbarTarget::Midi(n) => format!("Floating MIDI toolbar {}", (*n).clamp(1, 8)),
    }
}

fn target_menu_names(target: &ToolbarTarget) -> Vec<String> {
    match target {
        ToolbarTarget::Main => vec![target_to_str(target)],
        ToolbarTarget::Floating(n) => {
            let n = (*n).clamp(1, 32);
            vec![
                format!("Floating toolbar {n}"),
                format!("Floating toolbar {n} (Toolbar {n})"),
                format!("Toolbar {n}"),
            ]
        }
        ToolbarTarget::Midi(n) => vec![target_to_str(&ToolbarTarget::Midi(*n))],
    }
}

fn target_from_str(value: &str) -> ToolbarTarget {
    if value == "Main toolbar" {
        return ToolbarTarget::Main;
    }
    if let Some(num) = value.strip_prefix("Floating toolbar ")
        && let Ok(n) = num.parse::<u8>()
        && (1..=32).contains(&n)
    {
        return ToolbarTarget::Floating(n);
    }
    if let Some(num) = value.strip_prefix("Floating MIDI toolbar ")
        && let Ok(n) = num.parse::<u8>()
        && (1..=8).contains(&n)
    {
        return ToolbarTarget::Midi(n);
    }
    ToolbarTarget::Main
}

fn toolbar_targets() -> impl Iterator<Item = ToolbarTarget> {
    std::iter::once(ToolbarTarget::Main)
        .chain((1..=32).map(ToolbarTarget::Floating))
        .chain((1..=8).map(ToolbarTarget::Midi))
}

// ============================================================================
// Immediate operations (run on main thread from timer callback)
// ============================================================================

fn is_api_available() -> bool {
    Reaper::get()
        .medium_reaper()
        .low()
        .pointers()
        .GetCustomMenuOrToolbarItem
        .is_some()
}

fn resolve_command_id(command_name: &str) -> Result<CommandId, String> {
    if let Ok(command_id) = command_name.parse::<u32>() {
        return Ok(CommandId::new(command_id));
    }

    Reaper::get()
        .action_by_command_name(command_name)
        .command_id()
        .map_err(|e| format!("Command not found: {command_name} - {e}"))
}

fn position_descriptor(placement: &ToolbarPlacement) -> PositionDescriptor {
    match placement {
        ToolbarPlacement::Append => PositionDescriptor::Append,
        ToolbarPlacement::Position(position) => PositionDescriptor::AtPos(*position),
    }
}

fn icon_path(icon: Option<&ToolbarIcon>) -> Option<&camino::Utf8Path> {
    icon.map(|icon| camino::Utf8Path::new(icon.value.as_str()))
}

fn add_toolbar_command_item(
    toolbar_name: &str,
    position: PositionDescriptor,
    command_id: CommandId,
    flags: u32,
    label: &str,
    icon: Option<&ToolbarIcon>,
    refresh: UiRefreshBehavior,
) -> Result<(), String> {
    Reaper::get()
        .medium_reaper()
        .add_custom_menu_or_toolbar_item_command(
            toolbar_name,
            position,
            command_id,
            flags,
            label,
            icon_path(icon),
            refresh,
        )
        .map_err(|e| format!("Failed to add toolbar item: {e}"))
}

fn add_button_immediate(button: &ToolbarButton, workflow_id: &str) -> Result<CommandId, String> {
    let command_id = resolve_command_id(&button.command_name)?;
    let toolbar_name = resolve_toolbar_name_for_add(&button.target, command_id, |toolbar_name| {
        add_toolbar_command_item(
            toolbar_name,
            position_descriptor(&button.placement),
            command_id,
            button.flags,
            button.label.as_str(),
            button.icon.as_ref(),
            UiRefreshBehavior::Refresh,
        )
    })?;

    info!(
        command = %button.command_name,
        toolbar = %toolbar_name,
        "added toolbar button"
    );

    if let Ok(mut s) = state().lock() {
        s.added_buttons.insert(
            (toolbar_name, button.command_name.clone()),
            workflow_id.to_string(),
        );
    }

    Ok(command_id)
}

fn update_button_immediate(button: &ToolbarButton, workflow_id: &str) -> Result<CommandId, String> {
    let command_id = resolve_command_id(&button.command_name)?;
    let toolbar_name = resolve_toolbar_name_for_command(&button.target, command_id)
        .unwrap_or_else(|| target_to_str(&button.target));

    if let Some(position) = scan_toolbar_for_command(&toolbar_name, command_id) {
        let medium = Reaper::get().medium_reaper();
        medium
            .delete_custom_menu_or_toolbar_item(
                toolbar_name.as_str(),
                position,
                UiRefreshBehavior::NoRefresh,
            )
            .map_err(|e| format!("Failed to remove toolbar item: {e}"))?;

        let placement = match button.placement {
            ToolbarPlacement::Append => PositionDescriptor::AtPos(position),
            ToolbarPlacement::Position(position) => PositionDescriptor::AtPos(position),
        };
        add_toolbar_command_item(
            toolbar_name.as_str(),
            placement,
            command_id,
            button.flags,
            button.label.as_str(),
            button.icon.as_ref(),
            UiRefreshBehavior::Refresh,
        )?;

        debug!(
            command = %button.command_name,
            toolbar = %toolbar_name,
            position,
            "updated toolbar button"
        );
    } else {
        return add_button_immediate(button, workflow_id);
    }

    if let Ok(mut s) = state().lock() {
        s.added_buttons.insert(
            (toolbar_name, button.command_name.clone()),
            workflow_id.to_string(),
        );
    }

    Ok(command_id)
}

struct ToolbarCommandDetails {
    position: u32,
    command_id: CommandId,
    flags: u32,
    label: String,
    icon: Option<ToolbarIcon>,
}

fn get_toolbar_command_details(
    toolbar_name: &str,
    command_id: CommandId,
) -> Option<ToolbarCommandDetails> {
    let medium = Reaper::get().medium_reaper();
    let mut pos = 0;

    loop {
        let result =
            medium.get_custom_menu_or_toolbar_item(toolbar_name, pos, |item| match item? {
                MenuOrToolbarItem::Command(cmd) if cmd.command_id == command_id => {
                    Some(Some(ToolbarCommandDetails {
                        position: pos,
                        command_id: cmd.command_id,
                        flags: cmd.toolbar_flags,
                        label: cmd.label.to_string(),
                        icon: cmd.icon_file_name.map(|value| ToolbarIcon {
                            value: value.to_string(),
                            ..Default::default()
                        }),
                    }))
                }
                _ => Some(None),
            });

        match result {
            Some(Some(details)) => return Some(details),
            Some(None) => pos += 1,
            None => return None,
        }
    }
}

fn move_button_immediate(
    target: &ToolbarTarget,
    command_name: &str,
    position: u32,
) -> Result<CommandId, String> {
    let command_id = resolve_command_id(command_name)?;
    let toolbar_name = resolve_toolbar_name_for_command(target, command_id)
        .ok_or_else(|| format!("Toolbar button not found: {command_name}"))?;
    let details = get_toolbar_command_details(toolbar_name.as_str(), command_id)
        .ok_or_else(|| format!("Toolbar button not found: {command_name}"))?;
    let medium = Reaper::get().medium_reaper();
    medium
        .delete_custom_menu_or_toolbar_item(
            toolbar_name.as_str(),
            details.position,
            UiRefreshBehavior::NoRefresh,
        )
        .map_err(|e| format!("Failed to remove toolbar item: {e}"))?;
    add_toolbar_command_item(
        toolbar_name.as_str(),
        PositionDescriptor::AtPos(position),
        details.command_id,
        details.flags,
        details.label.as_str(),
        details.icon.as_ref(),
        UiRefreshBehavior::Refresh,
    )?;
    Ok(command_id)
}

fn set_button_icon_immediate(
    target: &ToolbarTarget,
    command_name: &str,
    icon: Option<ToolbarIcon>,
) -> Result<CommandId, String> {
    let command_id = resolve_command_id(command_name)?;
    let toolbar_name = resolve_toolbar_name_for_command(target, command_id)
        .ok_or_else(|| format!("Toolbar button not found: {command_name}"))?;
    let details = get_toolbar_command_details(toolbar_name.as_str(), command_id)
        .ok_or_else(|| format!("Toolbar button not found: {command_name}"))?;
    let medium = Reaper::get().medium_reaper();
    medium
        .delete_custom_menu_or_toolbar_item(
            toolbar_name.as_str(),
            details.position,
            UiRefreshBehavior::NoRefresh,
        )
        .map_err(|e| format!("Failed to remove toolbar item: {e}"))?;
    add_toolbar_command_item(
        toolbar_name.as_str(),
        PositionDescriptor::AtPos(details.position),
        details.command_id,
        details.flags,
        details.label.as_str(),
        icon.as_ref(),
        UiRefreshBehavior::Refresh,
    )?;
    Ok(command_id)
}

fn remove_button_immediate(target: &ToolbarTarget, command_name: &str) -> Result<(), String> {
    let command_id = resolve_command_id(command_name)?;
    let mut removed = Vec::new();

    for toolbar_name in target_menu_names(target) {
        let Some(position) = scan_toolbar_for_command(&toolbar_name, command_id) else {
            continue;
        };
        Reaper::get()
            .medium_reaper()
            .delete_custom_menu_or_toolbar_item(
                toolbar_name.as_str(),
                position,
                UiRefreshBehavior::Refresh,
            )
            .map_err(|e| format!("Failed to delete toolbar item: {e}"))?;

        info!(
            command = %command_name,
            toolbar = %toolbar_name,
            position,
            "removed toolbar button"
        );
        removed.push(toolbar_name);
    }

    if let Ok(mut s) = state().lock() {
        for toolbar_name in removed {
            s.added_buttons
                .remove(&(toolbar_name, command_name.to_string()));
        }
    }

    Ok(())
}

fn remove_workflow_buttons_immediate(workflow_id: &str) -> Result<(), String> {
    let buttons = state()
        .lock()
        .ok()
        .map(|s| {
            s.added_buttons
                .iter()
                .filter(|(_, owner)| owner.as_str() == workflow_id)
                .map(|((toolbar_name, command_name), _)| {
                    (toolbar_name.clone(), command_name.clone())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for (toolbar_name, command_name) in buttons {
        let target = target_from_str(&toolbar_name);
        remove_button_immediate(&target, &command_name)?;
    }

    Ok(())
}

fn scan_toolbar_for_command(toolbar_name: &str, command_id: CommandId) -> Option<u32> {
    let medium = Reaper::get().medium_reaper();
    let mut pos = 0;

    loop {
        let result =
            medium.get_custom_menu_or_toolbar_item(toolbar_name, pos, |item| match item? {
                MenuOrToolbarItem::Command(cmd) if cmd.command_id == command_id => Some(Some(pos)),
                _ => Some(None),
            });

        match result {
            Some(Some(found)) => return Some(found),
            Some(None) => pos += 1,
            None => return None,
        }
    }
}

fn resolve_toolbar_name_for_command(
    target: &ToolbarTarget,
    command_id: CommandId,
) -> Option<String> {
    target_menu_names(target)
        .into_iter()
        .find(|toolbar_name| scan_toolbar_for_command(toolbar_name, command_id).is_some())
}

fn resolve_toolbar_name_for_add(
    target: &ToolbarTarget,
    command_id: CommandId,
    add: impl Fn(&str) -> Result<(), String>,
) -> Result<String, String> {
    if let Some(toolbar_name) = resolve_toolbar_name_for_command(target, command_id) {
        return Ok(toolbar_name);
    }

    let mut errors = Vec::new();
    for toolbar_name in target_menu_names(target) {
        if toolbar_has_edit_me_placeholder(toolbar_name.as_str()) {
            let _ = Reaper::get()
                .medium_reaper()
                .delete_custom_menu_or_toolbar_item(
                    toolbar_name.as_str(),
                    0,
                    UiRefreshBehavior::NoRefresh,
                );
        }
        match add(toolbar_name.as_str()) {
            Ok(()) if scan_toolbar_for_command(toolbar_name.as_str(), command_id).is_some() => {
                return Ok(toolbar_name);
            }
            Ok(()) => errors.push(format!(
                "{toolbar_name}: add returned ok but item was not readable"
            )),
            Err(error) => errors.push(format!("{toolbar_name}: {error}")),
        }
    }

    Err(format!(
        "Failed to add readable toolbar item for {}: {}",
        target_to_str(target),
        errors.join("; ")
    ))
}

fn toolbar_has_edit_me_placeholder(toolbar_name: &str) -> bool {
    let medium = Reaper::get().medium_reaper();
    let first = medium.get_custom_menu_or_toolbar_item(toolbar_name, 0, |item| match item {
        Some(MenuOrToolbarItem::Command(command)) => {
            command.command_id.get() == 41101 && command.label.to_string() == "Edit me"
        }
        _ => false,
    });
    if !first {
        return false;
    }
    !medium.get_custom_menu_or_toolbar_item(toolbar_name, 1, |item| item.is_some())
}

fn command_name_for(command_id: CommandId) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .reverse_named_command_lookup(command_id, |name| format!("_{name}"))
}

fn snapshot_live_toolbar(target: ToolbarTarget) -> ToolbarSnapshot {
    let medium = Reaper::get().medium_reaper();
    let canonical_name = target_to_str(&target);
    let mut best_items = Vec::new();

    for toolbar_name in target_menu_names(&target) {
        let mut items = Vec::new();
        let mut pos = 0;

        loop {
            let item = medium.get_custom_menu_or_toolbar_item(toolbar_name.as_str(), pos, |item| {
                item.map(|item| match item {
                    MenuOrToolbarItem::Separator => ToolbarItemInfo {
                        position: pos,
                        kind: "separator".to_string(),
                        raw: Some("-1".to_string()),
                        ..Default::default()
                    },
                    MenuOrToolbarItem::SubMenuStart(submenu) => ToolbarItemInfo {
                        position: pos,
                        kind: "submenu-start".to_string(),
                        label: submenu.label.to_string(),
                        raw: Some("-2".to_string()),
                        ..Default::default()
                    },
                    MenuOrToolbarItem::SubMenuEnd => ToolbarItemInfo {
                        position: pos,
                        kind: "submenu-end".to_string(),
                        raw: Some("-3".to_string()),
                        ..Default::default()
                    },
                    MenuOrToolbarItem::Command(command) => {
                        let command_id = command.command_id;
                        ToolbarItemInfo {
                            position: pos,
                            kind: "command".to_string(),
                            command_id: Some(command_id.get()),
                            command_name: command_name_for(command_id),
                            label: command.label.to_string(),
                            flags: command.toolbar_flags,
                            icon: command.icon_file_name.map(|name| name.to_string()),
                            raw: None,
                        }
                    }
                })
            });

            match item {
                Some(item) => {
                    items.push(item);
                    pos += 1;
                }
                None => break,
            }
        }

        if has_non_placeholder_items(&items) {
            best_items = items;
            break;
        }
        if best_items.is_empty() {
            best_items = items;
        }
    }

    ToolbarSnapshot {
        toolbar_name: canonical_name,
        source: ToolbarSnapshotSource::Live,
        items: best_items,
    }
}

fn has_non_placeholder_items(items: &[ToolbarItemInfo]) -> bool {
    items.iter().any(|item| {
        item.command_name.is_some() || item.command_id != Some(41101) || item.label != "Edit me"
    })
}

fn command_result(result: Result<CommandId, String>) -> ToolbarResult {
    match result {
        Ok(command_id) => ToolbarResult::ok(command_id.get()),
        Err(error) => ToolbarResult::error(error),
    }
}

fn unit_result(result: Result<(), String>) -> ToolbarResult {
    match result {
        Ok(()) => ToolbarResult::ok(0),
        Err(error) => ToolbarResult::error(error),
    }
}

// ============================================================================
// ToolbarService trait implementation
// ============================================================================

impl Toolbar for crate::Reaper {
    fn add_button(&self, button: ToolbarButton, workflow_id: &str) -> ToolbarResult {
        debug!(command = %button.command_name, "toolbar add_button");
        if !is_api_available() {
            return ToolbarResult::error("Dynamic toolbar API not available");
        }
        command_result(add_button_immediate(&button, workflow_id))
    }

    fn update_button(&self, button: ToolbarButton, workflow_id: &str) -> ToolbarResult {
        if !is_api_available() {
            return ToolbarResult::error("Dynamic toolbar API not available");
        }
        command_result(update_button_immediate(&button, workflow_id))
    }

    fn remove_button(&self, target: ToolbarTarget, command_name: &str) -> ToolbarResult {
        if !is_api_available() {
            return ToolbarResult::ok(0);
        }
        unit_result(remove_button_immediate(&target, command_name))
    }

    fn move_button(
        &self,
        target: ToolbarTarget,
        command_name: &str,
        position: u32,
    ) -> ToolbarResult {
        debug!(%command_name, position, "toolbar move_button");
        if !is_api_available() {
            return ToolbarResult::error("Dynamic toolbar API not available");
        }
        command_result(move_button_immediate(&target, command_name, position))
    }

    fn set_button_icon(
        &self,
        target: ToolbarTarget,
        command_name: &str,
        icon: Option<ToolbarIcon>,
    ) -> ToolbarResult {
        if !is_api_available() {
            return ToolbarResult::error("Dynamic toolbar API not available");
        }
        command_result(set_button_icon_immediate(&target, command_name, icon))
    }

    fn remove_workflow_buttons(&self, workflow_id: &str) -> ToolbarResult {
        if !is_api_available() {
            return ToolbarResult::ok(0);
        }
        unit_result(remove_workflow_buttons_immediate(workflow_id))
    }

    fn is_available(&self) -> bool {
        is_api_available()
    }

    fn tracked_buttons(&self) -> Vec<TrackedButton> {
        toolbar_targets()
            .map(snapshot_live_toolbar)
            .flat_map(|snapshot| {
                let toolbar_name = snapshot.toolbar_name;
                snapshot.items.into_iter().map(move |item| TrackedButton {
                    toolbar_name: toolbar_name.clone(),
                    command_name: facet_json::to_string(&item).unwrap_or_default(),
                    workflow_id: "__fts_live_toolbar_item".to_string(),
                })
            })
            .collect()
    }
}
