//! Serialization logic for writing `reaper-fxfolders.ini` files.

use super::types::{FxFolder, FxFolderItem, FxFolders, PluginType};

/// The magic type code that marks a smart folder in the INI file.
const SMART_FOLDER_TYPE: u32 = 1048576;

impl FxFolders {
    /// Serialize this `FxFolders` back to the `reaper-fxfolders.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        // -----------------------------------------------------------------------
        // [Folders] section
        // -----------------------------------------------------------------------
        out.push_str("[Folders]\r\n");
        out.push_str(&format!("NbFolders={}\r\n", self.folders.len()));

        for (idx, folder) in self.folders.iter().enumerate() {
            out.push_str(&format!("Name{}={}\r\n", idx, folder.name));
            out.push_str(&format!("ID{}={}\r\n", idx, folder.folder_ref_id));
        }

        // -----------------------------------------------------------------------
        // One [FolderN] section per unique folder_ref_id.
        // We emit a section for every folder in order.  When multiple named
        // folders share the same folder_ref_id the section is only written once
        // (on its first encounter) to avoid duplicates.
        // -----------------------------------------------------------------------
        let mut emitted_ids: Vec<u32> = Vec::new();

        for folder in &self.folders {
            if emitted_ids.contains(&folder.folder_ref_id) {
                continue;
            }
            emitted_ids.push(folder.folder_ref_id);
            serialize_folder_section(folder, &mut out);
        }

        out
    }
}

fn serialize_folder_section(folder: &FxFolder, out: &mut String) {
    out.push_str(&format!("[Folder{}]\r\n", folder.folder_ref_id));
    out.push_str(&format!("Nb={}\r\n", folder.items.len()));

    for (idx, item) in folder.items.iter().enumerate() {
        match item {
            FxFolderItem::Plugin { plugin_type, name } => {
                out.push_str(&format!("Type{}={}\r\n", idx, plugin_type.to_raw()));
                out.push_str(&format!("Item{}={}\r\n", idx, name));
            }
            FxFolderItem::SmartFilter { filter } => {
                out.push_str(&format!("Type{}={}\r\n", idx, SMART_FOLDER_TYPE));
                out.push_str(&format!("Item{}={}\r\n", idx, filter));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_empty() {
        let fx = FxFolders::default();
        let out = fx.serialize();
        assert_eq!(out, "[Folders]\r\nNbFolders=0\r\n");
    }

    #[test]
    fn serialize_single_vst_folder() {
        let fx = FxFolders {
            folders: vec![FxFolder {
                name: "Favorites".to_string(),
                folder_ref_id: 0,
                items: vec![FxFolderItem::Plugin {
                    plugin_type: PluginType::VST,
                    name: "VST: ReaComp (Cockos)".to_string(),
                }],
            }],
        };
        let expected = "\
[Folders]\r\n\
NbFolders=1\r\n\
Name0=Favorites\r\n\
ID0=0\r\n\
[Folder0]\r\n\
Nb=1\r\n\
Type0=3\r\n\
Item0=VST: ReaComp (Cockos)\r\n";
        assert_eq!(fx.serialize(), expected);
    }

    #[test]
    fn serialize_smart_folder() {
        let fx = FxFolders {
            folders: vec![FxFolder {
                name: "SmartEQ".to_string(),
                folder_ref_id: 0,
                items: vec![FxFolderItem::SmartFilter {
                    filter: "EQ OR equalizer OR NOT dynamics".to_string(),
                }],
            }],
        };
        let out = fx.serialize();
        assert!(out.contains("Type0=1048576\r\n"));
        assert!(out.contains("Item0=EQ OR equalizer OR NOT dynamics\r\n"));
    }

    #[test]
    fn serialize_all_plugin_types() {
        let fx = FxFolders {
            folders: vec![FxFolder {
                name: "All".to_string(),
                folder_ref_id: 0,
                items: vec![
                    FxFolderItem::Plugin {
                        plugin_type: PluginType::DX,
                        name: "DXPlugin".to_string(),
                    },
                    FxFolderItem::Plugin {
                        plugin_type: PluginType::JS,
                        name: "JS/delay".to_string(),
                    },
                    FxFolderItem::Plugin {
                        plugin_type: PluginType::VST,
                        name: "ReaEQ".to_string(),
                    },
                    FxFolderItem::Plugin {
                        plugin_type: PluginType::AU,
                        name: "AUPlugin".to_string(),
                    },
                ],
            }],
        };
        let out = fx.serialize();
        assert!(out.contains("Type0=0\r\n"));
        assert!(out.contains("Type1=2\r\n"));
        assert!(out.contains("Type2=3\r\n"));
        assert!(out.contains("Type3=5\r\n"));
    }

    #[test]
    fn serialize_shared_folder_ref_emits_section_once() {
        // Two folders referencing the same section — the [Folder0] block
        // should only appear once in the output.
        let fx = FxFolders {
            folders: vec![
                FxFolder {
                    name: "Favorites".to_string(),
                    folder_ref_id: 0,
                    items: vec![FxFolderItem::Plugin {
                        plugin_type: PluginType::VST,
                        name: "ReaComp".to_string(),
                    }],
                },
                FxFolder {
                    name: "Also Favorites".to_string(),
                    folder_ref_id: 0,
                    items: vec![FxFolderItem::Plugin {
                        plugin_type: PluginType::VST,
                        name: "ReaComp".to_string(),
                    }],
                },
            ],
        };
        let out = fx.serialize();
        // [Folder0] should appear exactly once.
        assert_eq!(out.matches("[Folder0]").count(), 1);
        // Both folder names should be in [Folders].
        assert!(out.contains("Name0=Favorites\r\n"));
        assert!(out.contains("Name1=Also Favorites\r\n"));
    }

    #[test]
    fn serialize_multiple_folders() {
        let fx = FxFolders {
            folders: vec![
                FxFolder {
                    name: "EQ".to_string(),
                    folder_ref_id: 0,
                    items: vec![FxFolderItem::Plugin {
                        plugin_type: PluginType::VST,
                        name: "ReaEQ".to_string(),
                    }],
                },
                FxFolder {
                    name: "Dynamics".to_string(),
                    folder_ref_id: 1,
                    items: vec![FxFolderItem::Plugin {
                        plugin_type: PluginType::VST,
                        name: "ReaComp".to_string(),
                    }],
                },
            ],
        };
        let out = fx.serialize();
        assert!(out.contains("[Folder0]\r\n"));
        assert!(out.contains("[Folder1]\r\n"));
        assert!(out.contains("Name0=EQ\r\n"));
        assert!(out.contains("Name1=Dynamics\r\n"));
    }

    #[test]
    fn serialize_unknown_plugin_type() {
        let fx = FxFolders {
            folders: vec![FxFolder {
                name: "Weird".to_string(),
                folder_ref_id: 0,
                items: vec![FxFolderItem::Plugin {
                    plugin_type: PluginType::Unknown(99),
                    name: "WeirdPlugin".to_string(),
                }],
            }],
        };
        let out = fx.serialize();
        assert!(out.contains("Type0=99\r\n"));
        assert!(out.contains("Item0=WeirdPlugin\r\n"));
    }
}
