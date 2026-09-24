//! A switch's part menu, in a song: make the patch the switch is on into a
//! part of the song, or — when it already is one — go to it, rename it,
//! remove it. The same menu from the footswitch grid's right-click and the
//! setlist sidebar's switch rows.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PerfStack, PerformanceModel};

use crate::kit::{MenuItem, Picked};

/// The song's parts as `(name, patch)`, in order.
#[must_use]
pub fn parts_of(model: &PerformanceModel) -> Vec<(String, String)> {
    model
        .parts
        .iter()
        .map(|p| (p.name.clone(), p.patch.clone()))
        .collect()
}

/// The patch a stack switch is on (its rotation at the cursor).
#[must_use]
pub fn stack_patch(st: &PerfStack) -> String {
    st.patches
        .get(st.position as usize)
        .cloned()
        .unwrap_or_default()
}

/// The part that recalls `patch`, with its index.
fn part_for<'a>(parts: &'a [(String, String)], patch: &str) -> Option<(usize, &'a str)> {
    parts
        .iter()
        .position(|(_, p)| !patch.is_empty() && p.eq_ignore_ascii_case(patch))
        .map(|i| (i, parts[i].0.as_str()))
}

/// The song's changes to `patch`, as the menu's own items: save them back
/// to the profile (its new default), or drop them.
#[must_use]
pub fn change_items(changes: &[(String, u32)], patch: &str) -> Vec<MenuItem> {
    match changes.iter().find(|(p, _)| p.eq_ignore_ascii_case(patch)) {
        Some((_, n)) => vec![
            MenuItem::head(format!("Song changes · {n}")),
            MenuItem::run("changes-save", format!("Save back to profile ({patch})")),
            MenuItem::delete("changes-discard", "Discard the song's changes", None),
        ],
        None => Vec::new(),
    }
}

/// The song's changes, `(patch, count)`.
#[must_use]
pub fn changes_of(model: &PerformanceModel) -> Vec<(String, u32)> {
    model.song_changes.iter().map(|c| (c.patch.clone(), c.count)).collect()
}

/// The items for a switch on `patch`: its part, then the song's changes to
/// it.
#[must_use]
pub fn items(parts: &[(String, String)], patch: &str) -> Vec<MenuItem> {
    items_with_changes(parts, &[], patch)
}

/// [`items`] with the song's changes to the patch.
#[must_use]
pub fn items_with_changes(parts: &[(String, String)], changes: &[(String, u32)], patch: &str) -> Vec<MenuItem> {
    let mut out = part_items(parts, patch);
    let ch = change_items(changes, patch);
    if !ch.is_empty() {
        if !out.is_empty() {
            out.push(MenuItem::sep());
        }
        out.extend(ch);
    }
    out
}

fn part_items(parts: &[(String, String)], patch: &str) -> Vec<MenuItem> {
    if patch.is_empty() {
        return Vec::new();
    }
    let taken: Vec<String> = parts.iter().map(|(n, _)| n.clone()).collect();
    match part_for(parts, patch) {
        Some((_, name)) => {
            let others: Vec<String> = taken.iter().filter(|n| !n.eq_ignore_ascii_case(name)).cloned().collect();
            vec![
                MenuItem::head(format!("Part · {name}")),
                MenuItem::run("part-go", format!("Go to {name}")),
                MenuItem::name("part-rename", "Rename part…", "Rename", name.to_string(), others),
                MenuItem::delete("part-remove", format!("Remove part {name}"), None),
            ]
        }
        None => vec![
            MenuItem::head("Part"),
            MenuItem::name(
                "part-make",
                format!("Make a part from {patch}…"),
                "Add part",
                patch.to_string(),
                taken,
            ),
        ],
    }
}

/// Do what was picked from [`items`].
pub fn act(rig: &Option<RigClient>, parts: &[(String, String)], patch: &str, p: Picked) {
    let Some(r) = rig.clone() else { return };
    let existing = part_for(parts, patch).map(|(i, n)| (i, n.to_string()));
    let patch = patch.to_string();
    spawn(async move {
        match (p.id, existing) {
            ("changes-save", _) => drop(r.save_song_changes(patch).await),
            ("changes-discard", _) => drop(r.discard_song_changes(patch).await),
            ("part-go", Some((i, _))) => drop(r.select_part(i as u32).await),
            ("part-rename", Some((_, old))) => {
                let new = p.text.trim().to_string();
                if !new.is_empty() && new != old {
                    drop(r.rename_part(old, new).await);
                }
            }
            ("part-remove", Some((_, name))) => drop(r.remove_part(name).await),
            ("part-make", None) => {
                let name = p.text.trim().to_string();
                if !name.is_empty() {
                    let _ = r.add_part(name.clone()).await;
                    let _ = r.set_part_patch(name, patch).await;
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_not_yet_a_part_offers_to_make_one() {
        let parts = vec![("Verse 1".to_string(), "Dry Chorus Clean".to_string())];
        let it = items(&parts, "Drive");
        assert!(it.iter().any(|i| i.id == "part-make"));
        let it = items(&parts, "Dry Chorus Clean");
        assert!(it.iter().any(|i| i.id == "part-rename"));
        assert!(it.iter().any(|i| i.id == "part-remove"));
        assert!(items(&parts, "").is_empty());
    }
}
