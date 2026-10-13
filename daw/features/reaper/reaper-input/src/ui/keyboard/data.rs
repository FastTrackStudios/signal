//! Data extraction from the input processor for keyboard visualization.

use input::{KeyCode, Modifiers};
use std::collections::HashMap;

use crate::input::processor;

// ---------------------------------------------------------------------------
// Action sections (color categories)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionSection {
    Session,
    Navigation,
    Vim,
    Mode,
    Rig,
    App,
    Input,
    MouseView,
    Other,
}

impl ActionSection {
    /// Background class for key caps.
    pub fn bg_class(self) -> &'static str {
        match self {
            Self::Session => "bg-blue-800/60",
            Self::Navigation => "bg-green-800/60",
            Self::Vim => "bg-amber-800/60",
            Self::Mode => "bg-violet-800/60",
            Self::Rig => "bg-purple-800/60",
            Self::App => "bg-cyan-800/60",
            Self::Input => "bg-zinc-700/60",
            Self::MouseView => "bg-rose-800/60",
            Self::Other => "bg-zinc-700/60",
        }
    }

    /// Dot color for filter chips.
    pub fn dot_class(self) -> &'static str {
        match self {
            Self::Session => "bg-blue-500",
            Self::Navigation => "bg-green-500",
            Self::Vim => "bg-amber-500",
            Self::Mode => "bg-violet-500",
            Self::Rig => "bg-purple-500",
            Self::App => "bg-cyan-500",
            Self::Input => "bg-zinc-400",
            Self::MouseView => "bg-rose-500",
            Self::Other => "bg-zinc-500",
        }
    }

    /// Text color for labels.
    pub fn text_class(self) -> &'static str {
        match self {
            Self::Session => "text-blue-400",
            Self::Navigation => "text-green-400",
            Self::Vim => "text-amber-400",
            Self::Mode => "text-violet-400",
            Self::Rig => "text-purple-400",
            Self::App => "text-cyan-400",
            Self::Input => "text-zinc-400",
            Self::MouseView => "text-rose-400",
            Self::Other => "text-zinc-500",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Navigation => "Navigation",
            Self::Vim => "Vim",
            Self::Mode => "Mode",
            Self::Rig => "Rig",
            Self::App => "App",
            Self::Input => "Input",
            Self::MouseView => "Mouse/View",
            Self::Other => "Other",
        }
    }

    pub fn all() -> &'static [ActionSection] {
        &[
            Self::Session,
            Self::Navigation,
            Self::Vim,
            Self::Mode,
            Self::Rig,
            Self::App,
            Self::Input,
            Self::MouseView,
            Self::Other,
        ]
    }

    /// Classify an action ID into a section.
    pub fn from_action_id(action_id: &str) -> Self {
        let id = action_id.to_lowercase();
        if id.starts_with("fts.session") || id.contains("session") {
            Self::Session
        } else if id.contains("cursor")
            || id.contains("navigate")
            || id.contains("goto")
            || id.contains("scroll")
            || id.contains("zoom")
        {
            Self::Navigation
        } else if id.contains("mode:") || id.contains("mode.") {
            Self::Mode
        } else if id.contains("rig") || id.contains("instrument") {
            Self::Rig
        } else if id.contains("mouse") || id.contains("view") {
            Self::MouseView
        } else if id.starts_with("fts.app")
            || id.contains("preference")
            || id.contains("action_list")
        {
            Self::App
        } else if id.starts_with("fts_input") || id.starts_with("fts.input") {
            Self::Input
        } else {
            // REAPER numeric actions are typically navigation/transport
            if id.parse::<u32>().is_ok() {
                Self::Navigation
            } else {
                Self::Other
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Binding info
// ---------------------------------------------------------------------------

/// Display-friendly context label.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BindingContext {
    Global,
    Main,
    Midi,
    MidiInline,
    MediaExplorer,
}

impl BindingContext {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Main => "Main",
            Self::Midi => "MIDI Editor",
            Self::MidiInline => "MIDI Inline",
            Self::MediaExplorer => "Media Explorer",
        }
    }

    pub fn all() -> &'static [BindingContext] {
        &[
            Self::Global,
            Self::Main,
            Self::Midi,
            Self::MidiInline,
            Self::MediaExplorer,
        ]
    }
}

/// A resolved key binding for display.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyBindingInfo {
    /// The key sequence (e.g., [KeyChord { key: 'g', .. }, KeyChord { key: 'g', .. }])
    pub sequence_display: String,
    /// The first key in the sequence (for keyboard coloring).
    pub first_key: Option<KeyCode>,
    /// Modifier state for the first key.
    pub modifiers: Modifiers,
    /// The action description.
    pub description: String,
    /// The action ID.
    pub action_id: String,
    /// Color category.
    pub section: ActionSection,
    /// Whether this is a prefix (has children) rather than a leaf action.
    pub is_prefix: bool,
    /// Which context this binding applies in.
    pub context: BindingContext,
}

/// Collect all key bindings from the current processor state.
/// Walks the entire keytrie to get both root prefixes and all leaf bindings.
pub fn collect_current_bindings() -> Vec<KeyBindingInfo> {
    let proc = processor::get_processor();
    let proc = proc.read().unwrap();
    let mut bindings = Vec::new();

    // Get root prefixes (for keyboard coloring)
    let roots = proc.all_root_prefixes();
    for (key_str, label, is_branch) in &roots {
        let first_key = if key_str.len() == 1 {
            Some(KeyCode::Character(key_str.to_lowercase()))
        } else {
            None
        };

        let section = if *is_branch {
            ActionSection::Navigation
        } else {
            ActionSection::from_action_id(label)
        };

        bindings.push(KeyBindingInfo {
            sequence_display: key_str.clone(),
            first_key,
            modifiers: Modifiers::default(),
            description: label.clone(),
            action_id: label.clone(),
            section,
            is_prefix: *is_branch,
            context: BindingContext::Global,
        });
    }

    bindings
}

/// Collect ALL leaf bindings by walking the entire keytrie.
/// Returns a flat list of every complete key sequence → action mapping.
pub fn collect_all_leaf_bindings() -> Vec<KeyBindingInfo> {
    use input::trie::KeyTrie;

    let proc = processor::get_processor();
    let proc = proc.read().unwrap();
    let mut results = Vec::new();

    let Some(trie) = proc.normal_keytrie() else {
        return results;
    };

    // Recursively walk the trie
    fn walk(
        node: &KeyTrie,
        path: &mut Vec<(KeyCode, Modifiers)>,
        results: &mut Vec<KeyBindingInfo>,
    ) {
        match node {
            KeyTrie::Leaf(leaf) => {
                let action_id = match leaf {
                    input::trie::LeafAction::Action(id) => id.0.clone(),
                    input::trie::LeafAction::SwitchMode(m) => format!("mode:{}", m.0),
                    input::trie::LeafAction::PushMode(m) => format!("push:{}", m.0),
                    input::trie::LeafAction::Operator(s) => format!("op:{s}"),
                    input::trie::LeafAction::Motion(s) => format!("motion:{s}"),
                    input::trie::LeafAction::TextObject(s) => format!("textobj:{s}"),
                    input::trie::LeafAction::Sequence(ids) => ids
                        .iter()
                        .map(|id| id.0.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    input::trie::LeafAction::Unbind => "unbind".into(),
                };

                let sequence_display = path
                    .iter()
                    .map(|(k, m)| format_chord_display(k, m))
                    .collect::<Vec<_>>()
                    .join(" ");

                let first_key = path.first().map(|(k, _)| k.clone());
                let first_mods = path.first().map(|(_, m)| *m).unwrap_or_default();
                let section = ActionSection::from_action_id(&action_id);

                results.push(KeyBindingInfo {
                    sequence_display,
                    first_key,
                    modifiers: first_mods,
                    description: action_id.clone(),
                    action_id,
                    context: BindingContext::Global,
                    section,
                    is_prefix: false,
                });
            }
            KeyTrie::Node(node) => {
                for (chord, child) in &node.children {
                    path.push((chord.key.clone(), chord.modifiers));
                    walk(child, path, results);
                    path.pop();
                }
            }
        }
    }

    let mut path = Vec::new();
    walk(trie, &mut path, &mut results);
    results
}

/// Load descriptions from the styx config files for action enrichment.
pub fn load_action_descriptions() -> HashMap<String, String> {
    use crate::keybind_config::editor::ProfileEditor;

    let mut descs = HashMap::new();

    // Try to load from the active profile's config directory
    let config_dir = super::source::config_dir();

    // Scan each profile directory for section files with descriptions
    if let Ok(entries) = std::fs::read_dir(config_dir.join("keybinds")) {
        for entry in entries.flatten() {
            if entry.path().is_dir()
                && let Ok(profile) = ProfileEditor::open(entry.path())
            {
                for (_, binding) in profile.all_bindings() {
                    if let Some(desc) = &binding.desc {
                        descs.insert(binding.action.clone(), desc.clone());
                    }
                    // Also map by keys for lookup
                    if let Some(desc) = &binding.desc {
                        descs
                            .entry(binding.keys.clone())
                            .or_insert_with(|| desc.clone());
                    }
                }
            }
        }
    }

    descs
}

/// Group bindings by action ID, creating an action-centric view.
/// Each action has a list of all key sequences that trigger it.
pub fn group_by_action(bindings: &[KeyBindingInfo]) -> Vec<ActionEntry> {
    let mut map: HashMap<String, ActionEntry> = HashMap::new();

    for binding in bindings {
        let entry = map
            .entry(binding.action_id.clone())
            .or_insert_with(|| ActionEntry {
                action_id: binding.action_id.clone(),
                description: binding.description.clone(),
                section: binding.section,
                bindings: Vec::new(),
            });
        entry.bindings.push(BindingEntry {
            sequence_display: binding.sequence_display.clone(),
            first_key: binding.first_key.clone(),
            modifiers: binding.modifiers,
            source_file: None,
            raw_keys: Some(binding.sequence_display.clone()),
        });
    }

    let mut actions: Vec<ActionEntry> = map.into_values().collect();
    actions.sort_by(|a, b| a.action_id.cmp(&b.action_id));
    actions
}

/// An action with all its key bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionEntry {
    pub action_id: String,
    pub description: String,
    pub section: ActionSection,
    pub bindings: Vec<BindingEntry>,
}

/// A single key binding for an action.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingEntry {
    pub sequence_display: String,
    pub first_key: Option<KeyCode>,
    pub modifiers: Modifiers,
    /// Which .styx file this binding comes from (for editing).
    pub source_file: Option<String>,
    /// The raw keys string from the styx file (e.g. `<C-s>`, `h`).
    pub raw_keys: Option<String>,
}

/// A REAPER action from the native action list.
#[derive(Clone, Debug, PartialEq)]
pub struct ReaperAction {
    pub command_id: u32,
    pub command_name: String,
    pub description: String,
}

/// Enumerate all REAPER native actions from the main section.
///
/// Returns an empty list in standalone mode — REAPER isn't loaded, so its
/// action table is unavailable.
pub fn enumerate_reaper_actions() -> Vec<ReaperAction> {
    use reaper_high::Reaper;

    if super::source::is_standalone() {
        return Vec::new();
    }

    let mut actions = Vec::new();
    let reaper = Reaper::get();
    let section = reaper.main_section();
    let count = section.action_count();

    for i in 0..count {
        let action = section.action_by_index(i);
        let cmd_id = action.command_id().unwrap_or_default();
        let name = action.name().map(|n| n.to_string()).unwrap_or_default();
        let command_name = action
            .command_name()
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("{}", cmd_id.get()));

        actions.push(ReaperAction {
            command_id: cmd_id.get(),
            command_name,
            description: name,
        });
    }

    actions
}

fn normalize_action_id(action_id: &str) -> String {
    action_id.strip_prefix('_').unwrap_or(action_id).to_string()
}

/// Collect the full action list by merging the live REAPER registry with
/// current key bindings.
pub fn collect_action_entries() -> Vec<ActionEntry> {
    // In-REAPER we walk the live keytrie; standalone we read the `.styx` files
    // directly (the processor isn't running).
    let all_bindings = if super::source::is_standalone() {
        collect_bindings_with_sources()
    } else {
        collect_all_leaf_bindings()
    };
    let descriptions = load_action_descriptions();
    let mut entries: HashMap<String, ActionEntry> = HashMap::new();

    for def in crate::input::actions::get_input_action_defs() {
        entries
            .entry(def.command_id.to_string())
            .or_insert_with(|| ActionEntry {
                action_id: def.command_id.to_string(),
                description: def.display_name,
                section: ActionSection::from_action_id(def.command_id),
                bindings: Vec::new(),
            });
    }

    for def in crate::input::actions::get_dynamic_preset_action_defs() {
        entries
            .entry(def.command_id.clone())
            .or_insert_with(|| ActionEntry {
                action_id: def.command_id.clone(),
                description: def.display_name.clone(),
                section: ActionSection::from_action_id(&def.command_id),
                bindings: Vec::new(),
            });
    }

    for def in crate::input::actions::get_dynamic_workflow_action_defs() {
        entries
            .entry(def.command_id.clone())
            .or_insert_with(|| ActionEntry {
                action_id: def.command_id.clone(),
                description: def.display_name.clone(),
                section: ActionSection::from_action_id(&def.command_id),
                bindings: Vec::new(),
            });
    }

    for action in enumerate_reaper_actions() {
        let action_id = normalize_action_id(&action.command_name);
        let section = ActionSection::from_action_id(&action_id);
        entries
            .entry(action_id.clone())
            .or_insert_with(|| ActionEntry {
                action_id,
                description: action.description,
                section,
                bindings: Vec::new(),
            });
    }

    for binding in all_bindings {
        let sequence_display = binding.sequence_display.clone();
        let entry = entries
            .entry(binding.action_id.clone())
            .or_insert_with(|| ActionEntry {
                action_id: binding.action_id.clone(),
                description: descriptions
                    .get(&binding.action_id)
                    .cloned()
                    .unwrap_or_default(),
                section: binding.section,
                bindings: Vec::new(),
            });
        entry.bindings.push(BindingEntry {
            sequence_display,
            first_key: binding.first_key,
            modifiers: binding.modifiers,
            source_file: None,
            raw_keys: Some(binding.sequence_display),
        });
    }

    for entry in entries.values_mut() {
        if entry.description.is_empty()
            && let Some(desc) = descriptions.get(&entry.action_id)
        {
            entry.description = desc.clone();
        }
        if matches!(entry.section, ActionSection::Other) {
            entry.section = ActionSection::from_action_id(&entry.action_id);
        }
    }

    let mut actions: Vec<ActionEntry> = entries.into_values().collect();
    actions.sort_by(|a, b| a.action_id.cmp(&b.action_id));
    actions
}

/// Collect bindings with source file tracking from the styx config.
pub fn collect_bindings_with_sources() -> Vec<KeyBindingInfo> {
    use crate::keybind_config::editor::ProfileEditor;

    let mut bindings = Vec::new();

    // Scope to the focused profile when one is selected; otherwise merge every
    // profile directory under `keybinds/`.
    let profile_dirs: Vec<std::path::PathBuf> =
        if let Some(dir) = super::source::active_profile_dir() {
            vec![dir]
        } else {
            std::fs::read_dir(super::source::keybinds_dir())
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        };

    for profile_path in profile_dirs {
        if let Ok(profile) = ProfileEditor::open(&profile_path) {
            for (_section_name, binding_def) in profile.all_bindings() {
                let section = ActionSection::from_action_id(&binding_def.action);
                let ctx = match &binding_def.context {
                    Some(c) => match c {
                        crate::input::keybinds::KeybindContext::Global => BindingContext::Global,
                        crate::input::keybinds::KeybindContext::Main => BindingContext::Main,
                        crate::input::keybinds::KeybindContext::Midi => BindingContext::Midi,
                        crate::input::keybinds::KeybindContext::MidiInline => {
                            BindingContext::MidiInline
                        }
                        crate::input::keybinds::KeybindContext::MediaExplorer => {
                            BindingContext::MediaExplorer
                        }
                        crate::input::keybinds::KeybindContext::Custom(_) => BindingContext::Global,
                    },
                    None => BindingContext::Global,
                };
                bindings.push(KeyBindingInfo {
                    sequence_display: binding_def.keys.clone(),
                    first_key: parse_first_key(&binding_def.keys),
                    modifiers: Modifiers::default(),
                    description: binding_def.desc.clone().unwrap_or_default(),
                    action_id: binding_def.action.clone(),
                    section,
                    is_prefix: false,
                    context: ctx,
                });
            }
        }
    }

    bindings
}

/// List available modes (session-mode workflows) from `workflows/`.
/// Returns `(filename, display_name)` pairs, sorted, `mode-*` first.
pub fn list_modes() -> Vec<(String, String)> {
    use crate::keybind_config::types::{WorkflowConfig, kebab_to_title};

    let dir = super::source::config_dir().join("workflows");
    let mut modes: Vec<(String, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut files: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "styx").unwrap_or(false))
            .collect();
        files.sort();
        for p in files {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let file = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let display = std::fs::read_to_string(&p)
                .ok()
                .and_then(|c| facet_styx::from_str::<WorkflowConfig>(&c).ok())
                .and_then(|w| w.name)
                .unwrap_or_else(|| kebab_to_title(stem.trim_start_matches("mode-")));
            modes.push((file, display));
        }
    }
    // mode-* first, then the rest, each group already alphabetical.
    modes.sort_by_key(|(f, _)| (!f.starts_with("mode-"), f.clone()));
    modes
}

/// Collect the inline keyboard bindings of a workflow/mode file as
/// [`KeyBindingInfo`], tagged with the `Mode` section colour.
pub fn collect_workflow_bindings(file: &str) -> Vec<KeyBindingInfo> {
    use crate::keybind_config::types::WorkflowConfig;

    let path = super::source::config_dir().join("workflows").join(file);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(wf) = facet_styx::from_str::<WorkflowConfig>(&content) else {
        return Vec::new();
    };
    wf.bindings()
        .iter()
        .map(|b| KeyBindingInfo {
            sequence_display: b.keys.clone(),
            first_key: parse_first_key(&b.keys),
            modifiers: Modifiers::default(),
            description: b.desc.clone().unwrap_or_default(),
            action_id: b.action.clone(),
            section: ActionSection::Mode,
            is_prefix: false,
            context: BindingContext::Global,
        })
        .collect()
}

/// Parse the first key from a key sequence string.
fn parse_first_key(keys: &str) -> Option<KeyCode> {
    let s = keys.trim();
    if s.len() == 1 {
        Some(KeyCode::Character(s.to_lowercase()))
    } else if s.starts_with('<') && s.ends_with('>') {
        // Parse <C-s>, <S-Tab>, etc. — extract the key part after modifiers
        let inner = &s[1..s.len() - 1];
        let key_part = inner.split('-').next_back().unwrap_or(inner);
        match key_part.to_lowercase().as_str() {
            "space" => Some(KeyCode::Character(" ".into())),
            "tab" => Some(KeyCode::Tab),
            "enter" | "cr" => Some(KeyCode::Enter),
            "esc" => Some(KeyCode::Escape),
            "bs" => Some(KeyCode::Backspace),
            "del" => Some(KeyCode::Delete),
            s if s.len() == 1 => Some(KeyCode::Character(s.into())),
            _ => None,
        }
    } else {
        None
    }
}

/// Build a map from key label → `Vec<KeyBindingInfo>` for key cap coloring.
pub fn bindings_by_key(bindings: &[KeyBindingInfo]) -> HashMap<String, Vec<KeyBindingInfo>> {
    let mut map: HashMap<String, Vec<KeyBindingInfo>> = HashMap::new();
    for binding in bindings {
        if let Some(ref key) = binding.first_key {
            let key_str = match key {
                KeyCode::Character(c) => c.to_lowercase(),
                KeyCode::Escape => "esc".to_string(),
                KeyCode::Enter => "enter".to_string(),
                KeyCode::Tab => "tab".to_string(),
                KeyCode::Backspace => "bksp".to_string(),
                KeyCode::Delete => "del".to_string(),
                KeyCode::ArrowUp => "↑".to_string(),
                KeyCode::ArrowDown => "↓".to_string(),
                KeyCode::ArrowLeft => "←".to_string(),
                KeyCode::ArrowRight => "→".to_string(),
                KeyCode::F(n) => format!("f{n}"),
            };
            map.entry(key_str).or_default().push(binding.clone());
        }
    }
    map
}

/// Format a key chord for display (e.g., "C-s", "S-Tab").
pub fn format_chord_display(key: &KeyCode, mods: &Modifiers) -> String {
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("C-");
    }
    if mods.alt {
        s.push_str("A-");
    }
    if mods.shift {
        s.push_str("S-");
    }
    if mods.meta {
        s.push_str("M-");
    }
    match key {
        KeyCode::Character(c) => s.push_str(c),
        KeyCode::Escape => s.push_str("Esc"),
        KeyCode::Enter => s.push_str("Enter"),
        KeyCode::Tab => s.push_str("Tab"),
        KeyCode::Backspace => s.push_str("BS"),
        KeyCode::Delete => s.push_str("Del"),
        KeyCode::ArrowUp => s.push('↑'),
        KeyCode::ArrowDown => s.push('↓'),
        KeyCode::ArrowLeft => s.push('←'),
        KeyCode::ArrowRight => s.push('→'),
        KeyCode::F(n) => {
            s.push_str(&format!("F{n}"));
        }
    }
    s
}
