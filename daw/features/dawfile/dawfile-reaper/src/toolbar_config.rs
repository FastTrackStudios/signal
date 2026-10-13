//! REAPER toolbar configuration parsing.
//!
//! Parses toolbar sections from `reaper-menu.ini`. This module intentionally
//! lives in `dawfile-reaper` so callers can inspect saved REAPER configuration
//! without requiring a running REAPER instance.

use daw_proto::{ToolbarItemInfo, ToolbarSnapshot, ToolbarSnapshotSource};
use std::collections::BTreeMap;
use std::path::Path;

/// Parse all toolbar sections from a `reaper-menu.ini` file.
pub fn parse_toolbar_config_file(path: impl AsRef<Path>) -> std::io::Result<Vec<ToolbarSnapshot>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_toolbar_config_text(&content))
}

/// Parse all toolbar sections from `reaper-menu.ini` text.
pub fn parse_toolbar_config_text(content: &str) -> Vec<ToolbarSnapshot> {
    let mut sections = BTreeMap::<String, BTreeMap<u32, String>>::new();
    let mut flags = BTreeMap::<String, BTreeMap<u32, u32>>::new();
    let mut current = None::<String>;

    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = section
                .to_ascii_lowercase()
                .contains("toolbar")
                .then(|| section.to_string());
            continue;
        }

        let Some(section) = current.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if let Some(index) = key
            .strip_prefix("item_")
            .and_then(|n| n.parse::<u32>().ok())
        {
            sections
                .entry(section.clone())
                .or_default()
                .insert(index, value.to_string());
        } else if let Some(index) = key.strip_prefix("tbf_").and_then(|n| n.parse::<u32>().ok())
            && let Ok(value) = value.parse::<u32>()
        {
            flags
                .entry(section.clone())
                .or_default()
                .insert(index, value);
        }
    }

    sections
        .into_iter()
        .map(|(toolbar_name, items)| {
            let toolbar_flags = flags.remove(&toolbar_name).unwrap_or_default();
            ToolbarSnapshot {
                toolbar_name,
                source: ToolbarSnapshotSource::Config,
                items: items
                    .into_iter()
                    .map(|(position, raw)| {
                        let flags = toolbar_flags.get(&position).copied().unwrap_or_default();
                        parse_toolbar_config_item(position, &raw, flags)
                    })
                    .collect(),
            }
        })
        .collect()
}

fn parse_toolbar_config_item(position: u32, raw: &str, flags: u32) -> ToolbarItemInfo {
    let trimmed = raw.trim();
    if matches!(trimmed, "-1" | "-2" | "-3") {
        let kind = match trimmed {
            "-1" => "separator",
            "-2" => "submenu-start",
            "-3" => "submenu-end",
            _ => "unknown",
        };
        return ToolbarItemInfo {
            position,
            kind: kind.to_string(),
            raw: Some(trimmed.to_string()),
            ..Default::default()
        };
    }

    let (id, label) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    ToolbarItemInfo {
        position,
        kind: "command".to_string(),
        command_id: id.parse::<u32>().ok(),
        command_name: id.starts_with('_').then(|| id.to_string()),
        label: label.to_string(),
        flags,
        icon: None,
        raw: Some(trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_toolbar_items_and_flags() {
        let snapshots = parse_toolbar_config_text(
            r#"
[Main toolbar]
item_0=40023 New project...
item_1=-1
item_2=_FTS_GUEST_TOGGLE_EXAMPLE FTS: Integrated Toggle Example
tbf_2=1
"#,
        );

        assert_eq!(snapshots.len(), 1);
        let toolbar = &snapshots[0];
        assert_eq!(toolbar.toolbar_name, "Main toolbar");
        assert_eq!(toolbar.items.len(), 3);
        assert_eq!(toolbar.items[0].command_id, Some(40023));
        assert_eq!(toolbar.items[1].kind, "separator");
        assert_eq!(
            toolbar.items[2].command_name.as_deref(),
            Some("_FTS_GUEST_TOGGLE_EXAMPLE")
        );
        assert_eq!(toolbar.items[2].flags, 1);
    }
}
