//! Parsing logic for `reaper-fxfolders.ini` files.

use std::collections::HashMap;

use thiserror::Error;

use crate::ini::IniFile;

use super::types::{FxFolder, FxFolderItem, FxFolders, PluginType};

/// The magic type code that marks a folder as a smart folder.
const SMART_FOLDER_TYPE: u32 = 1048576;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid integer value '{value}' on line {line}: {source}")]
    InvalidInt {
        line: usize,
        value: String,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("duplicate key '{key}' in section '[{section}]' on line {line}")]
    DuplicateKey {
        line: usize,
        section: String,
        key: String,
    },

    #[error("missing 'NbFolders' entry in [Folders] section")]
    MissingNbFolders,

    #[error(
        "folder index {index} has a 'NameN' entry but no corresponding 'IdN' entry in [Folders]"
    )]
    MissingFolderId { index: u32 },
}

// ---------------------------------------------------------------------------
// Public parser
// ---------------------------------------------------------------------------

/// Parse the full contents of a `reaper-fxfolders.ini` file.
pub fn parse(input: &str) -> Result<FxFolders, ParseError> {
    let ini = IniFile::parse(input);

    // -----------------------------------------------------------------------
    // 1.  Parse [Folders] section.
    // -----------------------------------------------------------------------
    let folders_entries = ini.section_entries("Folders");

    // Build a first-occurrence map for simple scalar lookups.
    let mut folders_kv: HashMap<&str, &str> = HashMap::new();
    for (key, value) in &folders_entries {
        folders_kv.entry(key).or_insert(value);
    }

    // NbFolders
    let nb_folders: u32 = if let Some(v) = folders_kv.get("NbFolders") {
        v.trim()
            .parse::<u32>()
            .map_err(|e| ParseError::InvalidInt {
                line: 0,
                value: v.to_string(),
                source: e,
            })?
    } else {
        // An empty / absent [Folders] section is valid — no folders.
        return Ok(FxFolders::default());
    };

    // Collect NameN and IdN pairs.  The key variants seen in the wild are:
    //   NameN  (always)
    //   IDN    (uppercase, used in some examples)
    //   IdN    (mixed-case, used in other examples)
    // We normalise to lowercase for lookup.
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut ids: HashMap<u32, u32> = HashMap::new();

    for (key, value) in &folders_entries {
        let key_lower = key.to_ascii_lowercase();
        if let Some(rest) = key_lower.strip_prefix("name") {
            if let Ok(idx) = rest.parse::<u32>() {
                names.entry(idx).or_insert_with(|| value.to_string());
            }
        } else if let Some(rest) = key_lower.strip_prefix("id")
            && let Ok(idx) = rest.parse::<u32>()
        {
            let ref_id = value
                .trim()
                .parse::<u32>()
                .map_err(|e| ParseError::InvalidInt {
                    line: 0,
                    value: value.to_string(),
                    source: e,
                })?;
            ids.entry(idx).or_insert(ref_id);
        }
    }

    // -----------------------------------------------------------------------
    // 2.  Parse [FolderN] sections and assemble FxFolder list.
    // -----------------------------------------------------------------------
    let mut folders: Vec<FxFolder> = Vec::with_capacity(nb_folders as usize);

    for folder_idx in 0..nb_folders {
        let name = match names.get(&folder_idx) {
            Some(n) if !n.is_empty() => n.clone(),
            // Folders with empty names are ignored by REAPER; we skip them too.
            _ => continue,
        };

        let folder_ref_id = match ids.get(&folder_idx) {
            Some(&id) => id,
            None => return Err(ParseError::MissingFolderId { index: folder_idx }),
        };

        // The corresponding section is named "Folder{folder_ref_id}".
        let section_name = format!("Folder{}", folder_ref_id);
        let folder_entries = ini.section_entries(&section_name);

        // Deduplicate: only first occurrence per key.
        let mut folder_kv: HashMap<&str, &str> = HashMap::new();
        for (key, value) in &folder_entries {
            folder_kv.entry(key).or_insert(value);
        }

        // Nb — number of items (items with index >= Nb are ignored).
        let nb: u32 = if let Some(v) = folder_kv.get("Nb") {
            v.trim()
                .parse::<u32>()
                .map_err(|e| ParseError::InvalidInt {
                    line: 0,
                    value: v.to_string(),
                    source: e,
                })?
        } else {
            0
        };

        let items = parse_folder_items(&folder_kv, nb)?;

        folders.push(FxFolder {
            name,
            folder_ref_id,
            items,
        });
    }

    Ok(FxFolders { folders })
}

/// Parse the `TypeN`/`ItemN` pairs from a folder section's key-value map.
fn parse_folder_items(kv: &HashMap<&str, &str>, nb: u32) -> Result<Vec<FxFolderItem>, ParseError> {
    let mut items: Vec<FxFolderItem> = Vec::with_capacity(nb as usize);

    for item_idx in 0..nb {
        let type_key = format!("Type{}", item_idx);
        let item_key = format!("Item{}", item_idx);

        let type_val = match kv.get(type_key.as_str()) {
            Some(v) => *v,
            // A TypeN without a matching ItemN is ignored per the spec.
            None => continue,
        };
        let item_val = match kv.get(item_key.as_str()) {
            Some(v) => *v,
            // A TypeN without a matching ItemN is ignored per the spec.
            None => continue,
        };

        let type_code = type_val
            .trim()
            .parse::<u32>()
            .map_err(|e| ParseError::InvalidInt {
                line: 0,
                value: type_val.to_string(),
                source: e,
            })?;

        let item = if type_code == SMART_FOLDER_TYPE {
            FxFolderItem::SmartFilter {
                filter: item_val.to_string(),
            }
        } else {
            FxFolderItem::Plugin {
                plugin_type: PluginType::from_raw(type_code),
                name: item_val.to_string(),
            }
        };

        items.push(item);
    }

    Ok(items)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_input() {
        let result = parse("").unwrap();
        assert!(result.folders.is_empty());
    }

    #[test]
    fn parse_no_folders_section() {
        let input = "[Folder0]\r\nNb=1\r\nType0=3\r\nItem0=MyVST\r\n";
        let result = parse(input).unwrap();
        assert!(result.folders.is_empty());
    }

    #[test]
    fn parse_single_vst_folder() {
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=Favorites\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=VST: ReaComp (Cockos)\r\n";
        let result = parse(input).unwrap();
        assert_eq!(result.folders.len(), 1);
        let folder = &result.folders[0];
        assert_eq!(folder.name, "Favorites");
        assert_eq!(folder.folder_ref_id, 0);
        assert_eq!(folder.items.len(), 1);
        assert_eq!(
            folder.items[0],
            FxFolderItem::Plugin {
                plugin_type: PluginType::VST,
                name: "VST: ReaComp (Cockos)".to_string(),
            }
        );
    }

    #[test]
    fn parse_mixed_plugin_types() {
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=AllTypes\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=4\r\n\
Type0=0\r\n\
Item0=DXPlugin\r\n\
Type1=2\r\n\
Item1=JS/delay\r\n\
Type2=3\r\n\
Item2=ReaEQ\r\n\
Type3=5\r\n\
Item3=AUPlugin\r\n";
        let result = parse(input).unwrap();
        let items = &result.folders[0].items;
        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0],
            FxFolderItem::Plugin {
                plugin_type: PluginType::DX,
                name: "DXPlugin".to_string(),
            }
        );
        assert_eq!(
            items[1],
            FxFolderItem::Plugin {
                plugin_type: PluginType::JS,
                name: "JS/delay".to_string(),
            }
        );
        assert_eq!(
            items[2],
            FxFolderItem::Plugin {
                plugin_type: PluginType::VST,
                name: "ReaEQ".to_string(),
            }
        );
        assert_eq!(
            items[3],
            FxFolderItem::Plugin {
                plugin_type: PluginType::AU,
                name: "AUPlugin".to_string(),
            }
        );
    }

    #[test]
    fn parse_smart_folder() {
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=SmartEQ\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=1048576\r\n\
Item0=EQ OR equalizer OR NOT dynamics\r\n";
        let result = parse(input).unwrap();
        let folder = &result.folders[0];
        assert_eq!(folder.items.len(), 1);
        assert_eq!(
            folder.items[0],
            FxFolderItem::SmartFilter {
                filter: "EQ OR equalizer OR NOT dynamics".to_string(),
            }
        );
    }

    #[test]
    fn parse_shared_folder_ref() {
        // Two named folders referencing the same [Folder0] section.
        let input = "\
[Folders]\r\n\
NbFolders=2\r\n\
Name0=Favorites\r\n\
ID0=0\r\n\
Name1=Also Favorites\r\n\
Id1=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=ReaComp\r\n";
        let result = parse(input).unwrap();
        assert_eq!(result.folders.len(), 2);
        assert_eq!(result.folders[0].folder_ref_id, 0);
        assert_eq!(result.folders[1].folder_ref_id, 0);
        assert_eq!(result.folders[0].items, result.folders[1].items);
    }

    #[test]
    fn parse_nb_larger_than_actual_items() {
        // Nb=3 but only 2 Type/Item pairs — extra slots silently absent.
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=Partial\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=3\r\n\
Type0=3\r\n\
Item0=ReaComp\r\n\
Type1=2\r\n\
Item1=JS/delay\r\n";
        let result = parse(input).unwrap();
        assert_eq!(result.folders[0].items.len(), 2);
    }

    #[test]
    fn parse_type_without_item_is_skipped() {
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=Test\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=2\r\n\
Type0=3\r\n\
Item0=ReaComp\r\n\
Type1=3\r\n";
        // Type1 has no Item1 — should be skipped.
        let result = parse(input).unwrap();
        assert_eq!(result.folders[0].items.len(), 1);
    }

    #[test]
    fn parse_duplicate_keys_uses_first() {
        // REAPER uses first occurrence of duplicate keys.
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=First\r\n\
Name0=Second\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=ReaComp\r\n";
        let result = parse(input).unwrap();
        assert_eq!(result.folders[0].name, "First");
    }

    #[test]
    fn parse_unknown_plugin_type_round_trips() {
        let input = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=Test\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=99\r\n\
Item0=WeirdPlugin\r\n";
        let result = parse(input).unwrap();
        assert_eq!(
            result.folders[0].items[0],
            FxFolderItem::Plugin {
                plugin_type: PluginType::Unknown(99),
                name: "WeirdPlugin".to_string(),
            }
        );
    }

    #[test]
    fn parse_multiple_folders() {
        let input = "\
[Folders]\r\n\
NbFolders=2\r\n\
Name0=EQ\r\n\
ID0=0\r\n\
Name1=Dynamics\r\n\
ID1=1\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=ReaEQ\r\n\
[Folder1]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=ReaComp\r\n";
        let result = parse(input).unwrap();
        assert_eq!(result.folders.len(), 2);
        assert_eq!(result.folders[0].name, "EQ");
        assert_eq!(result.folders[1].name, "Dynamics");
    }
}
