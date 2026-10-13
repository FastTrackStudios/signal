//! DAW Actions provider.
//!
//! The action registry in the `daw` crate is for registering/executing actions,
//! not enumerating them. For now, we provide a curated set of common actions.
//! When running inside Reaper, the full action list can be read via
//! the Reaper-specific API extension.

use architect_launcher_core::{
    ActionModifier, ActivationResult, Item, ItemAction, Provider, ProviderConfig,
};
use reaper_medium::{CommandId, ProjectContext, SectionContext, SectionId};

pub struct DawActionsProvider {
    config: ProviderConfig,
    actions: Vec<Item>,
}

impl Default for DawActionsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DawActionsProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                name: "actions".into(),
                icon: "A".into(),
                prefix: None,
                default_tags: vec!["reaper/actions".into()],
                max_results: 200,
                ..Default::default()
            },
            actions: Vec::new(),
        }
    }
}

impl Provider for DawActionsProvider {
    fn name(&self) -> &str {
        "actions"
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn config_mut(&mut self) -> &mut ProviderConfig {
        &mut self.config
    }

    fn setup(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.actions = common_actions();
        tracing::info!(count = self.actions.len(), "Loaded cached DAW actions");
        Ok(())
    }

    fn query(
        &self,
        q: &str,
        _exact: bool,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
        let q = q.trim();
        if q.is_empty() {
            return Ok(self.actions.clone());
        }

        Ok(list_reaper_actions(Some(q)).unwrap_or_else(|| self.actions.clone()))
    }

    fn activate(
        &self,
        item: &Item,
        action: &str,
    ) -> Result<ActivationResult, Box<dyn std::error::Error + Send + Sync>> {
        let exec = item
            .actions
            .iter()
            .find(|a| a.name == action)
            .map(|a| a.exec.as_str())
            .unwrap_or("");
        let parts: Vec<&str> = exec.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Ok(ActivationResult::Close);
        }

        let action_id = parts[2];
        if action_id.is_empty() {
            return Ok(ActivationResult::Close);
        }

        let keep_open = item
            .actions
            .iter()
            .find(|a| a.name == action)
            .map(|a| a.keep_open)
            .unwrap_or(false);

        execute_reaper_action(action_id);

        if keep_open {
            Ok(ActivationResult::KeepOpen)
        } else {
            Ok(ActivationResult::Close)
        }
    }
}

fn list_reaper_actions(query: Option<&str>) -> Option<Vec<Item>> {
    let reaper = reaper_high::Reaper::get();
    let medium = reaper.medium_reaper();
    let section = reaper.section_by_id(SectionId::new(0));
    let query = query.map(|q| q.to_ascii_lowercase());
    let mut items = Vec::new();

    let _ = section.with_raw(|raw| {
        let section_context = SectionContext::Sec(raw);
        for i in 0..raw.action_list_cnt() {
            let Some(command) = raw.get_action_by_index(i).map(|action| action.cmd()) else {
                continue;
            };
            let description = unsafe {
                medium.kbd_get_text_from_cmd(command, section_context, |name| name.to_string())
            }
            .unwrap_or_default();
            let description = description.trim().to_string();
            if description.is_empty() {
                continue;
            }
            let command_name =
                medium.reverse_named_command_lookup(command, |name| name.to_string());
            if let Some(query) = query.as_deref() {
                let command_matches = command_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().contains(query));
                if !description.to_ascii_lowercase().contains(query) && !command_matches {
                    continue;
                }
            }
            items.push(action_row_to_item(command.get(), command_name, description));
        }
    });

    if items.is_empty() { None } else { Some(items) }
}

fn action_row_to_item(command_id: u32, command_name: Option<String>, description: String) -> Item {
    let command_name = command_name.unwrap_or_default();
    let provider = action_provider(&command_name, &description);
    let mut tags = vec![
        "reaper/actions".to_string(),
        format!("reaper/actions/{provider}"),
        "reaper/actions/main".to_string(),
    ];
    tags.sort();
    tags.dedup();
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let exec_id = if command_name.is_empty() {
        command_id.to_string()
    } else {
        command_name.clone()
    };

    Item::new(
        format!("action-{command_id}"),
        description.clone(),
        "actions",
    )
    .with_sub(format!("Action: {provider} > Main"))
    .with_icon("A")
    .with_tags(&tag_refs)
    .with_search_fields(vec![
        description,
        command_name,
        provider,
        "main".to_string(),
    ])
    .with_actions(vec![
        ItemAction::new("Run", format!("daw:action:{exec_id}")),
        ItemAction::new("Run (keep open)", format!("daw:action:{exec_id}"))
            .with_modifier(ActionModifier::Shift)
            .with_keep_open(),
    ])
}

fn execute_reaper_action(action_id: &str) {
    let medium = reaper_high::Reaper::get().medium_reaper();
    let command = action_id
        .parse::<u32>()
        .ok()
        .map(CommandId::new)
        .or_else(|| medium.named_command_lookup(action_id))
        .or_else(|| medium.named_command_lookup(format!("_{action_id}")));

    if let Some(command) = command {
        medium.main_on_command_ex(command, 0, ProjectContext::CurrentProject);
        tracing::info!(
            action_id,
            command_id = command.get(),
            "Executed launcher action"
        );
    } else {
        tracing::warn!(action_id, "Could not resolve launcher action");
    }
}

fn action_provider(command_name: &str, description: &str) -> String {
    let name = command_name.to_ascii_lowercase();
    let desc = description.to_ascii_lowercase();
    if name.starts_with("fts_") || name.contains("_fts_") || desc.contains("fasttrackstudio") {
        "fts".to_string()
    } else if name.starts_with("sws") || name.starts_with("_sws") || name.starts_with("_s&m") {
        "sws".to_string()
    } else if name.is_empty() {
        "reaper".to_string()
    } else if name
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == '_' || c == '-')
    {
        "script".to_string()
    } else {
        "extension".to_string()
    }
}

#[allow(dead_code)]
fn normalize_tag(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn common_actions() -> Vec<Item> {
    let actions = [
        ("play", "Play/Stop", "Transport", 1007),
        ("record", "Record", "Transport", 1013),
        ("pause", "Pause", "Transport", 1008),
        ("stop", "Stop", "Transport", 1016),
        ("rewind", "Go to start", "Transport", 40042),
        ("repeat", "Toggle repeat", "Transport", 1068),
        ("save", "Save project", "File", 40026),
        ("saveas", "Save project as...", "File", 40022),
        ("render", "Render project to disk", "File", 40015),
        ("open", "Open project...", "File", 40025),
        ("new", "New project", "File", 40023),
        ("undo", "Undo", "Edit", 40029),
        ("redo", "Redo", "Edit", 40030),
        ("copy", "Copy items", "Edit", 40057),
        ("cut", "Cut items", "Edit", 40059),
        ("paste", "Paste items", "Edit", 40058),
        ("selall", "Select all items", "Edit", 40182),
        ("addtrk", "Insert new track", "Track", 40001),
        ("deltrk", "Remove selected tracks", "Track", 40005),
        ("mute", "Toggle mute", "Track", 40281),
        ("solo", "Toggle solo", "Track", 40282),
        ("arm", "Toggle arm", "Track", 40294),
        ("mixer", "Toggle mixer", "View", 40078),
        ("fxbrow", "Show FX browser", "View", 40271),
        ("actlist", "Show action list", "View", 40605),
    ];

    actions
        .iter()
        .map(|(id, name, section, cmd_id)| {
            let tag = format!("reaper/actions/{}", section.to_lowercase());
            Item::new(format!("action-{id}"), *name, "actions")
                .with_sub(format!("Action: {section} > {name}"))
                .with_icon("A")
                .with_tags(&["reaper/actions", &tag])
                .with_search_fields(vec![name.to_string(), section.to_string()])
                .with_actions(vec![
                    ItemAction::new("Run", format!("daw:action:{cmd_id}")),
                    ItemAction::new("Run (keep open)", format!("daw:action:{cmd_id}"))
                        .with_modifier(ActionModifier::Shift)
                        .with_keep_open(),
                ])
        })
        .collect()
}
