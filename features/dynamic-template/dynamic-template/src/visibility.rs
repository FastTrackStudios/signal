//! The visibility manager's groups: which tracks are "Drums", "Bass", …
//!
//! The same classification the template organizes a session by
//! ([`monarchy_sort`] with [`default_config`]), turned into group → track
//! names. Pure, so the REAPER actions (`daw_module`) and the Session app
//! show and hide the same tracks for the same key.

use std::collections::HashMap;

use crate::{default_config, monarchy_sort, ItemMetadata, Structure};

/// Every group the tracks fall into, by [`normalize_key`]ed name
/// (`drums`, `guitars`, `guitars_electric`, …), each with the names of the
/// tracks in it, sorted.
///
/// # Errors
///
/// The sort failed.
pub fn groups(names: Vec<String>) -> eyre::Result<HashMap<String, Vec<String>>> {
    let structure = monarchy_sort(names, &default_config())?;
    let mut cache = HashMap::new();
    collect_group_cache(&structure, &mut Vec::new(), &mut cache);
    for names in cache.values_mut() {
        names.sort();
        names.dedup();
    }
    Ok(cache)
}

fn collect_group_cache(
    structure: &Structure<ItemMetadata>,
    path: &mut Vec<String>,
    cache: &mut HashMap<String, Vec<String>>,
) {
    let pushed = !structure.name.is_empty() && structure.name != "root";
    if pushed {
        path.push(structure.name.clone());
    }

    for item in &structure.items {
        for group in path.iter() {
            cache
                .entry(normalize_key(group))
                .or_default()
                .push(item.original.clone());
        }
        if !path.is_empty() {
            cache
                .entry(normalize_key(&path.join("_")))
                .or_default()
                .push(item.original.clone());
        }
    }

    for child in &structure.children {
        collect_group_cache(child, path, cache);
    }

    if pushed {
        path.pop();
    }
}

/// A group's name as a key: lower case, runs of anything else one `_`.
/// `"Guitars"`, `"GUITARS"` and the action suffix `GUITARS` agree.
#[must_use]
pub fn normalize_key(value: &str) -> String {
    let mut key = String::new();
    let mut last_was_sep = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep && !key.is_empty() {
            key.push('_');
            last_was_sep = true;
        }
    }
    while key.ends_with('_') {
        key.pop();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_ignores_case_and_punctuation() {
        assert_eq!(normalize_key("Guitars"), "guitars");
        assert_eq!(normalize_key("GUITARS"), "guitars");
        assert_eq!(normalize_key("Drums (Live)"), "drums_live");
    }

    #[test]
    fn tracks_fall_into_their_groups() {
        let names = ["Kick", "Snare", "Bass", "Piano"]
            .map(str::to_owned)
            .to_vec();
        let groups = groups(names).expect("the sort runs");
        assert!(
            groups
                .get("drums")
                .is_some_and(|d| d.contains(&"Kick".to_owned())),
            "{groups:?}"
        );
        assert!(
            groups
                .get("bass")
                .is_some_and(|b| b.contains(&"Bass".to_owned())),
            "{groups:?}"
        );
    }
}
