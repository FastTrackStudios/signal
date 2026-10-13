use once_cell::sync::Lazy;
use reaper_high::Reaper;
use reaper_medium::{CommandId, MenuOrToolbarItem, PositionDescriptor, UiRefreshBehavior};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolbarTarget {
    #[default]
    Main,
    Floating(u8),
}

impl ToolbarTarget {
    pub fn as_str(&self) -> String {
        match self {
            Self::Main => "Main toolbar".to_string(),
            Self::Floating(n) => format!("Floating toolbar {}", (*n).clamp(1, 32)),
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        if value == "Main toolbar" {
            return Some(Self::Main);
        }

        if let Some(num) = value.strip_prefix("Floating toolbar ") {
            let n = num.parse::<u8>().ok()?;
            if (1..=32).contains(&n) {
                return Some(Self::Floating(n));
            }
        }

        None
    }
}

pub mod flags {
    pub const NORMAL: u32 = 0;
}

#[derive(Debug, Clone)]
pub struct ToolbarButton {
    pub command_name: String,
    pub label: String,
    pub icon: Option<String>,
    pub toolbar: ToolbarTarget,
    pub toolbar_flags: u32,
}

#[derive(Default)]
struct ToolbarState {
    added_buttons: HashMap<(String, String), String>,
}

static STATE: Lazy<Mutex<ToolbarState>> = Lazy::new(|| Mutex::new(ToolbarState::default()));

pub fn is_available() -> bool {
    Reaper::get()
        .medium_reaper()
        .low()
        .pointers()
        .GetCustomMenuOrToolbarItem
        .is_some()
}

pub fn add_button(button: &ToolbarButton, workflow_id: &str) -> Result<CommandId, String> {
    if !is_available() {
        return Err("Dynamic toolbar API not available".to_string());
    }

    let command_id = resolve_command_id(&button.command_name)?;
    let toolbar_name = button.toolbar.as_str();

    if scan_toolbar_for_command(&toolbar_name, command_id).is_none() {
        let icon_path = button.icon.as_deref().map(camino::Utf8Path::new);
        Reaper::get()
            .medium_reaper()
            .add_custom_menu_or_toolbar_item_command(
                toolbar_name.as_str(),
                PositionDescriptor::Append,
                command_id,
                button.toolbar_flags,
                button.label.as_str(),
                icon_path,
                UiRefreshBehavior::Refresh,
            )
            .map_err(|e| format!("Failed to add toolbar item: {e}"))?;
    }

    if let Ok(mut state) = STATE.lock() {
        state.added_buttons.insert(
            (toolbar_name, button.command_name.clone()),
            workflow_id.to_string(),
        );
    }

    Ok(command_id)
}

pub fn remove_workflow_buttons(workflow_id: &str) -> Result<(), String> {
    if !is_available() {
        return Ok(());
    }

    let buttons = STATE
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
        let target = ToolbarTarget::from_str(&toolbar_name).unwrap_or_default();
        remove_button(&target, &command_name)?;
    }

    Ok(())
}

fn remove_button(target: &ToolbarTarget, command_name: &str) -> Result<(), String> {
    let command_id = resolve_command_id(command_name)?;
    let toolbar_name = target.as_str();

    if let Some(position) = scan_toolbar_for_command(&toolbar_name, command_id) {
        Reaper::get()
            .medium_reaper()
            .delete_custom_menu_or_toolbar_item(
                toolbar_name.as_str(),
                position,
                UiRefreshBehavior::Refresh,
            )
            .map_err(|e| format!("Failed to delete toolbar item: {e}"))?;
    }

    if let Ok(mut state) = STATE.lock() {
        state
            .added_buttons
            .remove(&(toolbar_name, command_name.to_string()));
    }

    Ok(())
}

fn resolve_command_id(command_name: &str) -> Result<CommandId, String> {
    Reaper::get()
        .action_by_command_name(command_name)
        .command_id()
        .map_err(|e| format!("Command not found: {command_name} - {e}"))
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
