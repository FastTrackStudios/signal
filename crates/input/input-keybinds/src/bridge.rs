//! Bridge between the `input-config-proto` keybind data model and the `input`
//! crate's `KeymapConfig`.
//!
//! Converts:
//! - `KeybindDef` lists → `KeymapConfig` sequence bindings
//! - `WhichKeyTreeDef` trees → flat space-separated key sequences in the trie
//! - `WheelBindDef` lists → scroll bindings
//! - key notation (`<C-s>`, `gg`) → input crate notation (`Ctrl+s`, `g g`)
//!
//! This is the app-agnostic core: it carries no REAPER coupling (no action
//! "routing marks", no VK-code conversion — those stay in reaper-input). A
//! plain Dioxus app can call [`section_to_keymap_config`] on a parsed styx
//! section and hand the result straight to an `input::InputProcessor`.

use std::collections::HashMap;

use input::config::{ContextLayerConfig, KeymapConfig};
use input_config_proto::{
    KeybindContext, KeybindDef, SectionConfig, WheelBindDef, WhichKeyEntryDef, WhichKeyTreeDef,
};

// ---------------------------------------------------------------------------
// Config → KeymapConfig
// ---------------------------------------------------------------------------

/// Convert a parsed [`SectionConfig`] (bindings + which-key trees + wheel
/// bindings) into a [`KeymapConfig`].
pub fn section_to_keymap_config(section: &SectionConfig) -> KeymapConfig {
    keymap_config_from_defs(section.bindings(), section.which_key(), section.wheel())
}

/// Convert raw def slices into a [`KeymapConfig`].
///
/// All keybinds go into the "normal" mode keymap. Context-specific bindings
/// (`KeybindContext::Main`, `Midi`, `Custom(..)`, …) are placed in
/// `keymap_context` with `WhenExpr` conditions from [`context_to_when_expr`].
pub fn keymap_config_from_defs(
    bindings: &[KeybindDef],
    trees: &[WhichKeyTreeDef],
    wheel: &[WheelBindDef],
) -> KeymapConfig {
    let mut global_bindings: HashMap<String, String> = HashMap::new();
    let mut context_bindings: HashMap<KeybindContext, HashMap<String, String>> = HashMap::new();

    for binding in bindings {
        let input_seq = translate_sequence(&binding.keys);
        let ctx = binding.context.clone().unwrap_or(KeybindContext::Global);
        if ctx == KeybindContext::Global {
            global_bindings.insert(input_seq, binding.action.clone());
        } else {
            context_bindings
                .entry(ctx)
                .or_default()
                .insert(input_seq, binding.action.clone());
        }
    }

    // Which-key trees flatten into global sequence bindings.
    for (seq, action) in which_key_trees_to_keymap_entries(trees) {
        global_bindings.insert(seq, action);
    }

    let mut keymap: HashMap<String, HashMap<String, String>> = HashMap::new();
    keymap.insert("normal".to_string(), global_bindings);

    let mut keymap_context: HashMap<String, Vec<ContextLayerConfig>> = HashMap::new();
    for (ctx, bindings) in context_bindings {
        let when = context_to_when_expr(&ctx);
        keymap_context
            .entry("normal".to_string())
            .or_default()
            .push(ContextLayerConfig { when, bindings });
    }

    let scroll = convert_wheel_bindings(wheel);

    KeymapConfig {
        modes: HashMap::new(),
        keymap,
        keymap_context,
        mouse: HashMap::new(),
        scroll,
    }
}

// ---------------------------------------------------------------------------
// Which-key trees → keymap entries
// ---------------------------------------------------------------------------

/// Flatten `WhichKeyTreeDef` trees into `sequence → action` entries.
fn which_key_trees_to_keymap_entries(trees: &[WhichKeyTreeDef]) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    for tree in trees {
        let translated_prefix = translate_sequence(&tree.prefix);
        let anchor_mods = anchor_modifiers(&translated_prefix);
        flatten_which_key_entries(
            &mut entries,
            &translated_prefix,
            &tree.entries,
            tree.case_insensitive.unwrap_or(false),
            &anchor_mods,
        );
    }
    entries
}

/// The labels which-key trees give their prefixes, groups and leaves,
/// keyed by the key sequence exactly as the keymap binds it.
///
/// [`keymap_config_from_defs`] flattens the trees into `sequence →
/// action` and keeps no names, so a popup walking the processor's trie
/// has nothing to call each key. This is the same walk (the same
/// translation, the same case-insensitive variants), keeping the names:
/// `z` → "Zoom", `z t` → "Toggle tracks/time zoom".
#[must_use]
pub fn which_key_labels(trees: &[WhichKeyTreeDef]) -> HashMap<Vec<input::KeyChord>, String> {
    let mut labels = HashMap::new();
    let mut put = |seq: &str, label: &str| {
        if let Ok(chords) = input::parse_key_sequence(seq) {
            labels.insert(chords, label.to_owned());
        }
    };
    fn walk(
        put: &mut dyn FnMut(&str, &str),
        prefix: &str,
        entries: &[WhichKeyEntryDef],
        case_insensitive: bool,
        anchor_mods: &[&str],
    ) {
        for entry in entries {
            for variant in key_variants(&entry.key, case_insensitive, anchor_mods) {
                let seq = format!("{prefix} {variant}");
                put(&seq, &entry.label);
                if let Some(children) = entry.children.as_deref() {
                    walk(put, &seq, children, case_insensitive, anchor_mods);
                }
            }
        }
    }
    for tree in trees {
        let prefix = translate_sequence(&tree.prefix);
        let anchor_mods = anchor_modifiers(&prefix);
        put(&prefix, &tree.label);
        walk(
            &mut put,
            &prefix,
            &tree.entries,
            tree.case_insensitive.unwrap_or(false),
            &anchor_mods,
        );
    }
    labels
}

/// Recursively flatten which-key entries into space-separated key sequences.
///
/// An entry with children is a branch (recurse); an entry with an `action`
/// and no children is a leaf (bind). Entries with neither are skipped.
fn flatten_which_key_entries(
    out: &mut HashMap<String, String>,
    prefix: &str,
    entries: &[WhichKeyEntryDef],
    case_insensitive: bool,
    anchor_mods: &[&str],
) {
    for entry in entries {
        let has_children = entry.children.as_deref().is_some_and(|c| !c.is_empty());
        if has_children {
            let children = entry.children.as_deref().unwrap_or(&[]);
            for variant in key_variants(&entry.key, case_insensitive, anchor_mods) {
                let new_prefix = format!("{prefix} {variant}");
                flatten_which_key_entries(
                    out,
                    &new_prefix,
                    children,
                    case_insensitive,
                    anchor_mods,
                );
            }
        } else if let Some(action) = &entry.action {
            for variant in key_variants(&entry.key, case_insensitive, anchor_mods) {
                out.insert(format!("{prefix} {variant}"), action.clone());
            }
        }
    }
}

/// Return the trie-key variants to emit for a single key token.
///
/// When `case_insensitive` is enabled and the key is a bare ASCII letter,
/// each anchor modifier is prepended so the user can keep the anchor's
/// modifiers held while picking children.
fn key_variants(key: &str, case_insensitive: bool, anchor_mods: &[&str]) -> Vec<String> {
    let translated = translate_sequence(key);
    if !case_insensitive {
        return vec![translated];
    }
    let is_bare_letter =
        key.chars().count() == 1 && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if !is_bare_letter {
        return vec![translated];
    }

    let lower = translated.to_lowercase();
    let mut out = vec![lower.clone()];

    if anchor_mods.contains(&"Shift") {
        out.push(format!("Shift+{lower}"));
    }

    let non_shift: Vec<&str> = anchor_mods
        .iter()
        .copied()
        .filter(|m| *m != "Shift")
        .collect();
    if !non_shift.is_empty() {
        out.push(format!("{}+{lower}", non_shift.join("+")));
        if anchor_mods.contains(&"Shift") {
            let mut combined = non_shift.clone();
            combined.push("Shift");
            out.push(format!("{}+{lower}", combined.join("+")));
        }
    }

    out
}

/// Extract the modifier set of a translated prefix (first chord only).
fn anchor_modifiers(translated_prefix: &str) -> Vec<&'static str> {
    let first_chord = translated_prefix.split(' ').next().unwrap_or("");
    let mut mods = Vec::new();
    for tag in ["Ctrl", "Alt", "Meta", "Shift"] {
        if first_chord.contains(&format!("{tag}+")) {
            mods.push(tag);
        }
    }
    mods
}

// ---------------------------------------------------------------------------
// Key notation translation
// ---------------------------------------------------------------------------

/// Convert a keybind-config key sequence string to input crate notation.
///
/// # Examples
/// - `"a"` → `"a"`
/// - `"gg"` → `"g g"`
/// - `"<C-s>"` → `"Ctrl+s"`
/// - `"<S-Tab>"` → `"Shift+Tab"`
/// - `"<M-w>"` → `"Meta+w"`
/// - `"<C-S-a>"` → `"Ctrl+Shift+a"`
/// - `"g<C-a>"` → `"g Ctrl+a"`
pub fn translate_sequence(seq: &str) -> String {
    let mut chords = Vec::new();
    let mut chars = seq.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut bracket_content = String::new();
            for ch in chars.by_ref() {
                if ch == '>' {
                    break;
                }
                bracket_content.push(ch);
            }
            chords.push(translate_bracketed(&bracket_content));
        } else {
            chords.push(c.to_lowercase().to_string());
        }
    }

    chords.join(" ")
}

/// Translate a bracketed key expression (without angle brackets).
fn translate_bracketed(content: &str) -> String {
    let lower = content.to_lowercase();
    if let Some(special) = translate_special_key(&lower) {
        return special;
    }

    let parts: Vec<&str> = content.split('-').collect();
    if parts.is_empty() {
        return String::new();
    }

    let key_str = parts.last().unwrap();
    let lower_key = key_str.to_lowercase();
    let key = translate_special_key(&lower_key).unwrap_or(lower_key);

    let mut modifiers: Vec<String> = Vec::new();
    for part in &parts[..parts.len() - 1] {
        let upper = part.to_uppercase();
        let resolved: Option<&'static str> = match upper.as_str() {
            #[cfg(target_os = "macos")]
            "C" | "CTRL" | "CONTROL" => {
                if modifiers.iter().any(|m| m == "Meta") {
                    Some("Ctrl")
                } else {
                    Some("Meta")
                }
            }
            #[cfg(not(target_os = "macos"))]
            "C" | "CTRL" | "CONTROL" => Some("Ctrl"),
            "CC" | "RAWCTRL" => Some("Ctrl"),
            "S" | "SHIFT" => Some("Shift"),
            "A" | "ALT" | "OPT" | "OPTION" => Some("Alt"),
            #[cfg(target_os = "macos")]
            "M" | "META" | "CMD" | "COMMAND" | "WIN" | "SUPER" | "D" => {
                if modifiers.iter().any(|m| m == "Meta") {
                    Some("Ctrl")
                } else {
                    Some("Meta")
                }
            }
            #[cfg(not(target_os = "macos"))]
            "M" | "META" | "CMD" | "COMMAND" | "WIN" | "SUPER" | "D" => Some("Meta"),
            _ => None,
        };
        if let Some(m) = resolved
            && !modifiers.iter().any(|existing| existing == m)
        {
            modifiers.push(m.to_string());
        }
    }

    if modifiers.is_empty() {
        key
    } else {
        modifiers.push(key);
        modifiers.join("+")
    }
}

/// Translate special key names to input crate names.
fn translate_special_key(name: &str) -> Option<String> {
    match name {
        "space" | "spc" => Some("Space".to_string()),
        "tab" => Some("Tab".to_string()),
        "enter" | "return" | "ret" | "cr" => Some("Enter".to_string()),
        "esc" | "escape" => Some("Escape".to_string()),
        "backspace" | "bs" => Some("Backspace".to_string()),
        "delete" | "del" => Some("Delete".to_string()),
        "up" => Some("Up".to_string()),
        "down" => Some("Down".to_string()),
        "left" => Some("Left".to_string()),
        "right" => Some("Right".to_string()),
        "home" => Some("home".to_string()),
        "end" => Some("end".to_string()),
        "pageup" | "pgup" => Some("pageup".to_string()),
        "pagedown" | "pgdn" => Some("pagedown".to_string()),
        "insert" | "ins" => Some("insert".to_string()),
        "plus" => Some("equals".to_string()),
        "minus" => Some("minus".to_string()),
        "equals" | "equal" => Some("equals".to_string()),
        "kp_+" | "kp_plus" => Some("kp_plus".to_string()),
        "kp_-" | "kp_minus" => Some("kp_minus".to_string()),
        "kp_*" | "kp_multiply" => Some("kp_multiply".to_string()),
        "kp_/" | "kp_divide" => Some("kp_divide".to_string()),
        "kp_." | "kp_period" => Some("kp_period".to_string()),
        "kp_enter" => Some("kp_enter".to_string()),
        "kp_0" => Some("kp_0".to_string()),
        "kp_1" => Some("kp_1".to_string()),
        "kp_2" => Some("kp_2".to_string()),
        "kp_3" => Some("kp_3".to_string()),
        "kp_4" => Some("kp_4".to_string()),
        "kp_5" => Some("kp_5".to_string()),
        "kp_6" => Some("kp_6".to_string()),
        "kp_7" => Some("kp_7".to_string()),
        "kp_8" => Some("kp_8".to_string()),
        "kp_9" => Some("kp_9".to_string()),
        "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
            Some(name.to_string())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Context → WhenExpr string
// ---------------------------------------------------------------------------

/// Convert a [`KeybindContext`] to a when-expression string for
/// `keymap_context`. `Custom(name)` maps to `context:<name>`.
pub fn context_to_when_expr(ctx: &KeybindContext) -> String {
    match ctx {
        KeybindContext::Global => "true".to_string(),
        KeybindContext::Main => "context:main".to_string(),
        KeybindContext::Midi => "context:midi".to_string(),
        KeybindContext::MidiInline => "context:midi_inline".to_string(),
        KeybindContext::MediaExplorer => "context:media_explorer".to_string(),
        KeybindContext::Custom(name) => format!("context:{name}"),
    }
}

// ---------------------------------------------------------------------------
// Wheel bindings → scroll config
// ---------------------------------------------------------------------------

/// Convert wheel bindings into scroll config entries (mode "normal").
fn convert_wheel_bindings(wheel: &[WheelBindDef]) -> HashMap<String, HashMap<String, String>> {
    if wheel.is_empty() {
        return HashMap::new();
    }

    let mut scroll_map: HashMap<String, String> = HashMap::new();
    for w in wheel {
        let axis = if w.horizontal.unwrap_or(false) {
            "ScrollX"
        } else {
            "Scroll"
        };
        let mods = parse_reaper_modifier_string(&w.modifiers);
        let pattern = if mods.is_empty() {
            axis.to_string()
        } else {
            format!("{mods}+{axis}")
        };
        scroll_map.insert(pattern, w.action.clone());
    }

    let mut result = HashMap::new();
    if !scroll_map.is_empty() {
        result.insert("normal".to_string(), scroll_map);
    }
    result
}

/// Parse a modifier string (`""`, `<C->`, `<C-S->`) into an input crate
/// modifier prefix (`""`, `"Ctrl"`, `"Ctrl+Shift"`).
pub fn parse_reaper_modifier_string(mods: &str) -> String {
    let trimmed = mods.trim();
    if trimmed.is_empty() || trimmed == "<>" {
        return String::new();
    }

    let inner = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed);

    let mut parts = Vec::new();
    for part in inner.split('-') {
        match part.to_uppercase().as_str() {
            #[cfg(target_os = "macos")]
            "C" | "CTRL" => parts.push("Meta"),
            #[cfg(not(target_os = "macos"))]
            "C" | "CTRL" => parts.push("Ctrl"),
            "CC" | "RAWCTRL" => parts.push("Ctrl"),
            "S" | "SHIFT" => parts.push("Shift"),
            "A" | "ALT" => parts.push("Alt"),
            "M" | "META" | "CMD" | "D" => parts.push("Meta"),
            "" => {}
            _ => {}
        }
    }

    parts.join("+")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_simple_and_multi() {
        assert_eq!(translate_sequence("a"), "a");
        assert_eq!(translate_sequence("gg"), "g g");
    }

    #[test]
    fn translate_modifiers() {
        assert_eq!(translate_sequence("<C-s>"), "Ctrl+s");
        assert_eq!(translate_sequence("<S-Tab>"), "Shift+Tab");
        assert_eq!(translate_sequence("<C-S-a>"), "Ctrl+Shift+a");
        assert_eq!(translate_sequence("g<C-a>"), "g Ctrl+a");
    }

    #[test]
    fn custom_context_when_expr() {
        assert_eq!(
            context_to_when_expr(&KeybindContext::Custom("editor".into())),
            "context:editor"
        );
    }

    #[test]
    fn defs_bridge_basic() {
        let bindings = vec![
            KeybindDef {
                keys: "a".into(),
                action: "action_a".into(),
                desc: None,
                context: None,
                passthrough: None,
                mnemonic: None,
                why: None,
            },
            KeybindDef {
                keys: "<C-s>".into(),
                action: "save".into(),
                desc: None,
                context: None,
                passthrough: None,
                mnemonic: None,
                why: None,
            },
        ];
        let config = keymap_config_from_defs(&bindings, &[], &[]);
        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("a"), Some(&"action_a".to_string()));
        assert_eq!(normal.get("Ctrl+s"), Some(&"save".to_string()));
    }

    #[test]
    fn which_key_tree_flattens() {
        let trees = vec![WhichKeyTreeDef {
            prefix: "v".into(),
            label: "Visibility".into(),
            case_insensitive: None,
            entries: vec![
                WhichKeyEntryDef {
                    key: "d".into(),
                    label: "Drums".into(),
                    action: Some("vis:drums".into()),
                    children: None,
                },
                WhichKeyEntryDef {
                    key: "e".into(),
                    label: "EQ".into(),
                    action: None,
                    children: Some(vec![WhichKeyEntryDef {
                        key: "r".into(),
                        label: "Rescue".into(),
                        action: Some("fx:rescue".into()),
                        children: None,
                    }]),
                },
            ],
        }];
        let config = keymap_config_from_defs(&[], &trees, &[]);
        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("v d"), Some(&"vis:drums".to_string()));
        assert_eq!(normal.get("v e r"), Some(&"fx:rescue".to_string()));
        assert_eq!(normal.get("v"), None);
    }

    #[test]
    fn context_binding_goes_to_layer() {
        let bindings = vec![KeybindDef {
            keys: "j".into(),
            action: "nav_down".into(),
            desc: None,
            context: Some(KeybindContext::Custom("editor".into())),
            passthrough: None,
            mnemonic: None,
            why: None,
        }];
        let config = keymap_config_from_defs(&bindings, &[], &[]);
        assert!(!config.keymap.get("normal").unwrap().contains_key("j"));
        let layers = config.keymap_context.get("normal").unwrap();
        assert_eq!(layers[0].when, "context:editor");
        assert_eq!(layers[0].bindings.get("j"), Some(&"nav_down".to_string()));
    }
}
