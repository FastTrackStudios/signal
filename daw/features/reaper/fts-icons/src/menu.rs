//! Patch reaper-menu.ini: point toolbar buttons at generated icons.
//!
//! Toolbar sections hold `item_N=<command> <label>` plus optional
//! `icon_N=<file>.png`. We match buttons by command id (first token of the
//! item value) so assignments survive button reordering, and set/replace the
//! matching `icon_N` line. Applies to every toolbar containing the command.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// (command id, icon file stem) pairs → set `icon_N=<stem>.png`.
/// Returns number of buttons updated.
pub fn apply_assignments(resource: &Path, assigns: &[(String, String)]) -> Result<usize> {
    let path = resource.join("reaper-menu.ini");
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut changed = 0;

    for (cmd, file) in assigns {
        let png = format!("{file}.png");
        let mut i = 0;
        let mut section_start = 0;
        while i < lines.len() {
            if lines[i].starts_with('[') {
                section_start = i;
            } else {
                let matched = lines[i].split_once('=').and_then(|(key, val)| {
                    let n = key.strip_prefix("item_")?;
                    (val.split_whitespace().next() == Some(cmd.as_str())).then(|| n.to_string())
                });
                if let Some(n) = matched {
                    if set_icon(&mut lines, section_start, i, &n, &png) {
                        changed += 1;
                    }
                }
            }
            i += 1;
        }
    }

    if changed > 0 {
        let backup = path.with_extension("ini.fts-icons.bak");
        fs::copy(&path, &backup).with_context(|| format!("backup {}", backup.display()))?;
        fs::write(&path, lines.join("\n") + "\n")
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(changed)
}

/// Replace `icon_N=` within the section, or insert it after the item line.
/// Returns false if the icon was already set to this file.
fn set_icon(
    lines: &mut Vec<String>,
    section_start: usize,
    item_idx: usize,
    n: &str,
    png: &str,
) -> bool {
    let key = format!("icon_{n}=");
    let new_line = format!("{key}{png}");
    let mut j = section_start + 1;
    while j < lines.len() && !lines[j].starts_with('[') {
        if lines[j].starts_with(&key) {
            if lines[j] == new_line {
                return false;
            }
            lines[j] = new_line;
            return true;
        }
        j += 1;
    }
    lines.insert(item_idx + 1, new_line);
    true
}
