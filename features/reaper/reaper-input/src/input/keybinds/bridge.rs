//! Bridge between input-reaper keybind definitions and the `input` crate.
//!
//! Converts:
//! - `KeybindPreset` / `KeybindOverride` → `KeymapConfig`
//! - `WhichKeyEntry` trees → keymap entries in the trie
//! - REAPER VK codes → `InputEvent`
//! - input-reaper key notation (`<C-s>`, `gg`) → input crate notation (`Ctrl+s`, `g g`)

use std::collections::HashMap;

use enumflags2::BitFlags;
use input::config::KeymapConfig;
use input::event::{InputEvent, KeyEvent};
use input::key::{KeyChord, KeyCode, Modifiers};
use reaper_medium::{AcceleratorBehavior, AcceleratorKeyCode};

use super::which_key::WhichKeyEntry;
use super::{KeybindContext, KeybindOverride, KeybindPreset};

// ---------------------------------------------------------------------------
// VK Code → InputEvent
// ---------------------------------------------------------------------------

/// Convert REAPER's VK code + AcceleratorBehavior into an `InputEvent`.
///
/// Returns `None` for pure modifier-only keypresses (Shift, Ctrl, Alt, Cmd).
pub fn vk_to_input_event(
    key: AcceleratorKeyCode,
    behavior: &BitFlags<AcceleratorBehavior>,
) -> Option<InputEvent> {
    vk_to_input_event_with_flags(key, behavior, 0)
}

/// SWELL's FLWIN bit (0x20). reaper-rs's `AcceleratorBehavior` only
/// exposes Shift/Control/Alt/VirtKey and truncates FLWIN away, so
/// callers that want the physical macOS Ctrl key to be reachable have
/// to pass the raw lParam low byte through this overload.
pub const SWELL_FLWIN: u8 = 0x20;

/// Like [`vk_to_input_event`], but accepts the raw SWELL lParam low
/// byte so the macOS physical Control key (FLWIN bit) is exposed as a
/// real Ctrl modifier instead of getting truncated.
pub fn vk_to_input_event_with_flags(
    key: AcceleratorKeyCode,
    behavior: &BitFlags<AcceleratorBehavior>,
    raw_flags: u8,
) -> Option<InputEvent> {
    let key_code = key.get() as u32;
    let rawctrl = (raw_flags & SWELL_FLWIN) != 0;

    // Skip pure modifier keys — they don't produce input events.
    match key_code {
        16 | 160 | 161 => return None,     // VK_SHIFT, VK_LSHIFT, VK_RSHIFT
        17 | 162 | 163 => return None,     // VK_CONTROL, VK_LCONTROL, VK_RCONTROL
        18 | 164 | 165 => return None,     // VK_MENU (Alt), VK_LMENU, VK_RMENU
        91 | 92 if rawctrl => return None, // macOS Ctrl press alone (FLWIN, code = VK_LWIN/RWIN)
        91 | 92 => {
            // No FLWIN: this is genuinely '[' / '\' on macOS, or
            // VK_LWIN on Windows (we don't bind that).
        }
        _ => {}
    }

    let ctrl = behavior.contains(AcceleratorBehavior::Control);
    let alt = behavior.contains(AcceleratorBehavior::Alt);
    let mut shift = behavior.contains(AcceleratorBehavior::Shift);

    // On macOS: SWELL maps Cmd (⌘) → FCONTROL and physical Ctrl (⌃) →
    // FLWIN (truncated by reaper-rs, surfaced via `raw_flags`). Map
    // Cmd → meta and physical Ctrl → ctrl so `<C-x>` / `<M-x>` /
    // `<CC-x>` bindings all reach the right physical key.
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers {
        ctrl: rawctrl,
        alt,
        shift,
        meta: ctrl,
    };
    #[cfg(not(target_os = "macos"))]
    let modifiers = Modifiers {
        ctrl,
        alt,
        shift,
        meta: false,
    };

    // Convert VK code to KeyCode.
    //
    // SWELL on macOS strips the FSHIFT flag when delivering already-shifted
    // punctuation (see swell-kb.mm line 200), so a press of Shift+/ arrives
    // as ASCII '?' with no modifier flag. Bindings written as `keys "?"`
    // should match directly, so emit the literal character instead of
    // base-key + inferred-shift.
    #[cfg(target_os = "macos")]
    let key_code_result = {
        if let Some(literal) = mac_punctuation_key(key_code) {
            Some(KeyCode::Character(literal.to_string()))
        } else {
            vk_to_key_code(key_code)
        }
    };
    #[cfg(not(target_os = "macos"))]
    let key_code_result = {
        if let Some(actual) = ascii_punctuation_key(key_code) {
            Some(KeyCode::Character(actual.to_string()))
        } else {
            vk_to_key_code(key_code)
        }
    };
    let _ = &mut shift; // suppress unused-mut warning when macOS path doesn't reassign

    let key = key_code_result?;

    // Rebuild modifiers with possibly updated shift.
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers { shift, ..modifiers };

    Some(InputEvent::Key(KeyEvent { key, modifiers }))
}

/// Map a Windows VK code to an `input::KeyCode`.
fn vk_to_key_code(key_code: u32) -> Option<KeyCode> {
    match key_code {
        // Letters (A-Z) → lowercase character
        65..=90 => {
            let c = char::from_u32(key_code + 32)?;
            Some(KeyCode::Character(c.to_string()))
        }
        // Numbers (0-9)
        48..=57 => {
            let c = char::from_u32(key_code)?;
            Some(KeyCode::Character(c.to_string()))
        }
        // Special keys
        8 => Some(KeyCode::Backspace),
        9 => Some(KeyCode::Tab),
        13 => Some(KeyCode::Enter),
        27 => Some(KeyCode::Escape),
        32 => Some(KeyCode::Character(" ".to_string())), // Space
        // Arrow keys
        0x25 => Some(KeyCode::ArrowLeft),
        0x26 => Some(KeyCode::ArrowUp),
        0x27 => Some(KeyCode::ArrowRight),
        0x28 => Some(KeyCode::ArrowDown),
        // Function keys (F1-F12)
        0x70..=0x7B => Some(KeyCode::F((key_code - 0x70 + 1) as u8)),
        // Navigation
        0x21 => Some(KeyCode::Character("pageup".to_string())),
        0x22 => Some(KeyCode::Character("pagedown".to_string())),
        0x23 => Some(KeyCode::Character("end".to_string())),
        0x24 => Some(KeyCode::Character("home".to_string())),
        0x2D => Some(KeyCode::Character("insert".to_string())),
        0x2E => Some(KeyCode::Delete),
        // OEM keys (US layout)
        // OEM keys (US layout)
        0xBA => Some(KeyCode::Character(";".to_string())),
        0xBB => Some(KeyCode::Character("=".to_string())),
        0xBC => Some(KeyCode::Character(",".to_string())),
        0xBD => Some(KeyCode::Character("-".to_string())),
        0xBE => Some(KeyCode::Character(".".to_string())),
        0xBF => Some(KeyCode::Character("/".to_string())),
        0xC0 => Some(KeyCode::Character("`".to_string())),
        0xDB => Some(KeyCode::Character("[".to_string())),
        0xDC => Some(KeyCode::Character("\\".to_string())),
        0xDD => Some(KeyCode::Character("]".to_string())),
        0xDE => Some(KeyCode::Character("'".to_string())),
        // Numpad keys
        0x60 => Some(KeyCode::Character("kp_0".to_string())),
        0x61 => Some(KeyCode::Character("kp_1".to_string())),
        0x62 => Some(KeyCode::Character("kp_2".to_string())),
        0x63 => Some(KeyCode::Character("kp_3".to_string())),
        0x64 => Some(KeyCode::Character("kp_4".to_string())),
        0x65 => Some(KeyCode::Character("kp_5".to_string())),
        0x66 => Some(KeyCode::Character("kp_6".to_string())),
        0x67 => Some(KeyCode::Character("kp_7".to_string())),
        0x68 => Some(KeyCode::Character("kp_8".to_string())),
        0x69 => Some(KeyCode::Character("kp_9".to_string())),
        0x6A => Some(KeyCode::Character("kp_multiply".to_string())),
        0x6B => Some(KeyCode::Character("kp_plus".to_string())),
        0x6D => Some(KeyCode::Character("kp_minus".to_string())),
        0x6E => Some(KeyCode::Character("kp_period".to_string())),
        0x6F => Some(KeyCode::Character("kp_divide".to_string())),
        _ => None, // Unknown VK code
    }
}

/// SWELL on Linux can emit ASCII punctuation codes instead of Windows VK OEM
/// codes. These are actual typed characters, so keep shifted symbols like `?`
/// as their literal key.
#[cfg(not(target_os = "macos"))]
fn ascii_punctuation_key(key_code: u32) -> Option<&'static str> {
    match key_code {
        33 => Some("!"),
        34 => Some("\""),
        35 => Some("#"),
        36 => Some("$"),
        37 => Some("%"),
        38 => Some("&"),
        39 => Some("'"),
        40 => Some("("),
        41 => Some(")"),
        42 => Some("*"),
        43 => Some("+"),
        44 => Some(","),
        45 => Some("-"),
        46 => Some("."),
        47 => Some("/"),
        58 => Some(":"),
        59 => Some(";"),
        60 => Some("<"),
        61 => Some("="),
        62 => Some(">"),
        63 => Some("?"),
        64 => Some("@"),
        91 => Some("["),
        92 => Some("\\"),
        93 => Some("]"),
        94 => Some("^"),
        95 => Some("_"),
        96 => Some("`"),
        123 => Some("{"),
        124 => Some("|"),
        125 => Some("}"),
        126 => Some("~"),
        _ => None,
    }
}

/// macOS SWELL emits the literal character ASCII code for printable keys
/// (Shift is consumed: `Shift+/` arrives as `?`, code 63). Return the
/// literal char so bindings written as `keys "?"` match.
#[cfg(target_os = "macos")]
fn mac_punctuation_key(key_code: u32) -> Option<&'static str> {
    match key_code {
        33 => Some("!"),
        34 => Some("\""),
        35 => Some("#"),
        36 => Some("$"),
        37 => Some("%"),
        38 => Some("&"),
        39 => Some("'"),
        40 => Some("("),
        41 => Some(")"),
        42 => Some("*"),
        43 => Some("+"),
        44 => Some(","),
        45 => Some("-"),
        46 => Some("."),
        47 => Some("/"),
        58 => Some(":"),
        59 => Some(";"),
        60 => Some("<"),
        61 => Some("="),
        62 => Some(">"),
        63 => Some("?"),
        64 => Some("@"),
        91 => Some("["),
        92 => Some("\\"),
        93 => Some("]"),
        94 => Some("^"),
        95 => Some("_"),
        96 => Some("`"),
        123 => Some("{"),
        124 => Some("|"),
        125 => Some("}"),
        126 => Some("~"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Preset → KeymapConfig
// ---------------------------------------------------------------------------

/// Convert a `KeybindPreset` and its which-key trees into a `KeymapConfig`.
///
/// All keybinds go into the "normal" mode keymap (REAPER doesn't use modal
/// editing). Context-specific bindings (`KeybindContext::Main`, `Midi`, etc.)
/// are placed in `keymap_context` with `WhenExpr` conditions.
pub fn preset_to_keymap_config(
    preset: &KeybindPreset,
    trees: &[super::WhichKeyTree],
) -> KeymapConfig {
    let mut global_bindings: HashMap<String, String> = HashMap::new();
    let mut context_bindings: HashMap<KeybindContext, HashMap<String, String>> = HashMap::new();

    // Convert keybindings
    for binding in &preset.bindings {
        let input_seq = translate_sequence(&binding.keys);
        let ctx = binding.effective_context();
        // Tag the action with its routing (MIDI editor section / forced main /
        // plain) so execution follows the *binding's* intent, not whichever
        // window has focus when the key fires.
        let action =
            crate::input::handler::mark_action(&binding.action, ctx.clone(), binding.passthrough);

        if ctx == KeybindContext::Global {
            global_bindings.insert(input_seq, action);
        } else {
            context_bindings
                .entry(ctx)
                .or_default()
                .insert(input_seq, action);
        }
    }

    // Convert which-key trees into flat key sequences
    let tree_entries = which_key_trees_to_keymap_entries(trees);
    for (seq, action) in tree_entries {
        global_bindings.insert(seq, action);
    }

    // Build keymap
    let mut keymap: HashMap<String, HashMap<String, String>> = HashMap::new();
    keymap.insert("normal".to_string(), global_bindings);

    // Build keymap_context
    let mut keymap_context: HashMap<String, Vec<input::config::ContextLayerConfig>> =
        HashMap::new();
    for (ctx, bindings) in context_bindings {
        let when = context_to_when_expr(ctx);
        let layer = input::config::ContextLayerConfig { when, bindings };
        keymap_context
            .entry("normal".to_string())
            .or_default()
            .push(layer);
    }

    // Build scroll bindings from wheel bindings
    let scroll = convert_wheel_bindings(preset);

    KeymapConfig {
        modes: HashMap::new(), // Use defaults (normal mode only)
        keymap,
        keymap_context,
        mouse: HashMap::new(), // Mouse modifiers handled separately by REAPER's native system
        scroll,
    }
}

/// Convert an override layer into a `KeymapConfig` for merging.
pub fn override_to_keymap_config(override_layer: &KeybindOverride) -> KeymapConfig {
    // Build a temporary preset to reuse the conversion logic
    let pseudo_preset = KeybindPreset {
        name: override_layer.name.clone(),
        display_name: override_layer.name.clone(),
        description: override_layer.description.clone(),
        version: "1.0.0".to_string(),
        bindings: override_layer.bindings.clone(),
        which_key_trees: Vec::new(),
        wheel_bindings: override_layer.wheel_bindings.clone(),
        mouse_modifiers: Vec::new(),
    };
    preset_to_keymap_config(&pseudo_preset, &[])
}

// ---------------------------------------------------------------------------
// Which-Key Trees → Keymap Entries
// ---------------------------------------------------------------------------

/// Convert `WhichKeyEntry` trees into flat keymap entries.
///
/// Each tree prefix + leaf path becomes a space-separated key sequence.
/// For example: tree prefix="v", leaf key="d" → sequence "v d", action="..."
fn which_key_trees_to_keymap_entries(trees: &[super::WhichKeyTree]) -> HashMap<String, String> {
    let mut entries = HashMap::new();

    for tree in trees {
        let translated_prefix = translate_sequence(&tree.prefix);
        let anchor_mods = anchor_modifiers(&translated_prefix);
        flatten_which_key_entries(
            &mut entries,
            &translated_prefix,
            &tree.entries,
            tree.case_insensitive,
            &anchor_mods,
        );
    }

    entries
}

/// Recursively flatten which-key entries into space-separated key sequences.
///
/// When `case_insensitive` is set, each bare-letter child key is also bound
/// with the anchor's own modifier set applied — so the user can keep the
/// anchor's modifiers held while picking children (e.g. hold `<S-m>` and
/// tap `c`, or hold `<A-m>` and tap `r`).
fn flatten_which_key_entries(
    out: &mut HashMap<String, String>,
    prefix: &str,
    entries: &[WhichKeyEntry],
    case_insensitive: bool,
    anchor_mods: &[&str],
) {
    for entry in entries {
        match entry {
            WhichKeyEntry::Leaf { key, action, .. } => {
                for variant in key_variants(key, case_insensitive, anchor_mods) {
                    let seq = format!("{prefix} {variant}");
                    out.insert(seq, action.clone());
                }
            }
            WhichKeyEntry::Branch { key, children, .. } => {
                for variant in key_variants(key, case_insensitive, anchor_mods) {
                    let new_prefix = format!("{prefix} {variant}");
                    flatten_which_key_entries(
                        out,
                        &new_prefix,
                        children,
                        case_insensitive,
                        anchor_mods,
                    );
                }
            }
        }
    }
}

/// Return the trie-key variants to emit for a single styx key token.
///
/// When `case_insensitive` is enabled and the key is a bare ASCII letter,
/// each variant in `anchor_mods` is prepended to give the user the option
/// of keeping the anchor's modifiers held while picking children.
///
/// `anchor_mods` are pre-extracted from the prefix (e.g. `["Shift"]` for
/// `<S-m>`, `["Alt"]` for `<A-m>`, `["Alt", "Shift"]` for `<S-A-m>`).
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

    // Anchor-with-shift case: when the anchor already includes Shift, the
    // user often keeps Shift held while picking children — bind the
    // capital-letter form too.
    if anchor_mods.contains(&"Shift") {
        out.push(format!("Shift+{lower}"));
    }

    // Other anchor modifiers (Alt / Ctrl / Meta) — bind a variant where
    // those modifiers are still pressed on the child. Combined with shift
    // if shift is also part of the anchor.
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

/// Extract the modifier set of a translated prefix. Returns the canonical
/// order used elsewhere: `["Ctrl", "Alt", "Meta", "Shift"]` filtered to
/// whichever are present. Looks only at the FIRST chord of the prefix —
/// multi-chord prefixes (rare) inherit modifiers from the leading chord.
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
// Key Notation Translation
// ---------------------------------------------------------------------------

/// Key-notation translation is app-agnostic and now lives in the shared
/// `input-keybinds` crate; re-exported here so callers keep using
/// `bridge::translate_sequence`.
pub use input_keybinds::translate_sequence;

// ---------------------------------------------------------------------------
// Context → WhenExpr string
// ---------------------------------------------------------------------------

/// Convert a `KeybindContext` to a when-expression string for `keymap_context`.
fn context_to_when_expr(ctx: KeybindContext) -> String {
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

/// Convert wheel bindings from a preset into scroll config entries.
fn convert_wheel_bindings(preset: &KeybindPreset) -> HashMap<String, HashMap<String, String>> {
    if preset.wheel_bindings.is_empty() {
        return HashMap::new();
    }

    let mut scroll_map: HashMap<String, String> = HashMap::new();

    for wheel in &preset.wheel_bindings {
        // Build the scroll pattern string
        let axis = if wheel.horizontal {
            "ScrollX"
        } else {
            "Scroll"
        };

        // Parse modifiers from the wheel binding's modifier string
        let mods = parse_reaper_modifier_string(&wheel.modifiers);
        let pattern = if mods.is_empty() {
            axis.to_string()
        } else {
            format!("{}+{}", mods, axis)
        };

        scroll_map.insert(pattern, wheel.action.clone());
    }

    let mut result = HashMap::new();
    if !scroll_map.is_empty() {
        result.insert("normal".to_string(), scroll_map);
    }
    result
}

/// Parse input-reaper modifier string (e.g., `"<C->"`, `"<C-S->"`, `""`) into
/// input crate modifier prefix (e.g., `"Ctrl"`, `"Ctrl+Shift"`, `""`).
fn parse_reaper_modifier_string(mods: &str) -> String {
    let trimmed = mods.trim();
    if trimmed.is_empty() || trimmed == "<>" {
        return String::new();
    }

    // Strip angle brackets
    let inner = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed);

    // Split by '-' and collect modifier names. See `translate_bracketed`
    // for the rationale on the macOS `<C-…>` → Cmd mapping.
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
            "" => {} // trailing hyphen
            _ => {}  // unknown, skip
        }
    }

    parts.join("+")
}

// ---------------------------------------------------------------------------
// Introspection helpers (for which-key overlay)
// ---------------------------------------------------------------------------

/// Get the continuations (child entries) at a given key sequence prefix
/// from the InputProcessor's keytrie.
///
/// Returns `(key_display, label, is_branch)` tuples.
pub fn trie_continuations_at(
    trie: &input::trie::KeyTrie,
    prefix: &[KeyChord],
) -> Vec<(String, String, bool)> {
    trie_continuations_at_detailed(trie, prefix)
        .into_iter()
        .map(|(key, label, is_branch, _action)| (key, label, is_branch))
        .collect()
}

/// Detailed variant — also returns the action id string for leaves
/// (None for branch nodes). Used by the which-key overlay so labels
/// can fall back to humanizing the leaf action when no tree metadata
/// is available (e.g. a workflow-only binding with no `desc`).
pub fn trie_continuations_at_detailed(
    trie: &input::trie::KeyTrie,
    prefix: &[KeyChord],
) -> Vec<(String, String, bool, Option<String>)> {
    use input::trie::{KeyTrie, LeafAction};

    let node = walk_trie(trie, prefix);
    match node {
        Some(KeyTrie::Node(node)) => {
            let mut result: Vec<(String, String, bool, Option<String>)> = node
                .children
                .iter()
                .map(|(chord, child)| {
                    let key_display = chord_to_display(chord);
                    match child {
                        KeyTrie::Node(n) => (key_display, n.name.clone(), true, None),
                        KeyTrie::Leaf(leaf) => {
                            let action = match leaf {
                                LeafAction::Action(s) => Some(s.to_string()),
                                _ => None,
                            };
                            (key_display, String::new(), false, action)
                        }
                    }
                })
                .collect();
            result.sort_by(|a, b| a.0.cmp(&b.0));
            result
        }
        _ => Vec::new(),
    }
}

/// Walk a KeyTrie along a sequence of chords.
fn walk_trie<'a>(
    trie: &'a input::trie::KeyTrie,
    path: &[KeyChord],
) -> Option<&'a input::trie::KeyTrie> {
    use input::trie::KeyTrie;

    if path.is_empty() {
        return Some(trie);
    }

    let mut current = trie;
    for chord in path {
        match current {
            KeyTrie::Node(node) => {
                current = node.children.get(chord)?;
            }
            KeyTrie::Leaf(_) => return None,
        }
    }
    Some(current)
}

/// Canonical display form of a `keys` sequence: chords joined by spaces in
/// `chord_to_display` notation, e.g. `"<S-t> 2 4"` → `"S-t 2 4"`. Used as the
/// lookup key between binding descriptions and which-key continuations.
pub fn canonical_sequence(reaper_seq: &str) -> String {
    let translated = translate_sequence(reaper_seq);
    let chords = crate::input::handler::pending_display_to_chords(&translated);
    chords
        .iter()
        .map(chord_to_display)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a KeyChord for display in the which-key overlay.
pub(crate) fn chord_to_display(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.modifiers.ctrl {
        s.push_str("C-");
    }
    if chord.modifiers.meta {
        s.push_str("M-");
    }
    if chord.modifiers.shift {
        s.push_str("S-");
    }
    if chord.modifiers.alt {
        s.push_str("A-");
    }
    match &chord.key {
        KeyCode::Character(c) if c == " " => s.push_str("SPC"),
        KeyCode::Character(c) => s.push_str(c),
        KeyCode::Escape => s.push_str("Esc"),
        KeyCode::Enter => s.push_str("Enter"),
        KeyCode::Tab => s.push_str("Tab"),
        KeyCode::Backspace => s.push_str("BS"),
        KeyCode::Delete => s.push_str("Del"),
        KeyCode::ArrowUp => s.push_str("Up"),
        KeyCode::ArrowDown => s.push_str("Down"),
        KeyCode::ArrowLeft => s.push_str("Left"),
        KeyCode::ArrowRight => s.push_str("Right"),
        KeyCode::F(n) => {
            s.push('F');
            s.push_str(&n.to_string());
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::keybinds::Keybind;

    #[test]
    fn test_translate_simple_key() {
        assert_eq!(translate_sequence("a"), "a");
    }

    #[test]
    fn test_translate_multi_key_sequence() {
        assert_eq!(translate_sequence("gg"), "g g");
    }

    #[test]
    fn test_translate_ctrl_key() {
        assert_eq!(translate_sequence("<C-s>"), "Ctrl+s");
    }

    #[test]
    fn test_translate_shift_tab() {
        assert_eq!(translate_sequence("<S-Tab>"), "Shift+Tab");
    }

    #[test]
    fn test_translate_meta_key() {
        assert_eq!(translate_sequence("<M-w>"), "Meta+w");
    }

    #[test]
    fn test_translate_multi_modifier() {
        assert_eq!(translate_sequence("<C-S-a>"), "Ctrl+Shift+a");
    }

    #[test]
    fn test_translate_mixed_sequence() {
        assert_eq!(translate_sequence("g<C-a>"), "g Ctrl+a");
    }

    #[test]
    fn test_translate_space() {
        assert_eq!(translate_sequence("<space>"), "Space");
    }

    #[test]
    fn test_translate_escape() {
        assert_eq!(translate_sequence("<esc>"), "Escape");
    }

    #[test]
    fn test_which_key_trees_to_entries() {
        let trees = vec![super::super::WhichKeyTree {
            prefix: "v".to_string(),
            label: "Visibility".to_string(),
            entries: vec![
                WhichKeyEntry::leaf("d", "Drums", "vis:drums"),
                WhichKeyEntry::branch(
                    "e",
                    "EQ",
                    vec![WhichKeyEntry::leaf("r", "Rescue", "fx:rescue")],
                ),
            ],
            case_insensitive: false,
        }];

        let entries = which_key_trees_to_keymap_entries(&trees);

        assert_eq!(entries.get("v d"), Some(&"vis:drums".to_string()));
        assert_eq!(entries.get("v e r"), Some(&"fx:rescue".to_string()));
        assert_eq!(entries.get("v"), None); // Prefix, not a leaf
    }

    #[test]
    fn test_case_insensitive_emits_shift_variants() {
        let trees = vec![super::super::WhichKeyTree {
            prefix: "<S-m>".to_string(),
            label: "Insert Marker".to_string(),
            entries: vec![
                WhichKeyEntry::leaf("c", "Chorus", "marker:chorus"),
                WhichKeyEntry::leaf("v", "Verse", "marker:verse"),
            ],
            case_insensitive: true,
        }];

        let entries = which_key_trees_to_keymap_entries(&trees);

        // Both shifted and unshifted leaf variants should resolve.
        assert_eq!(entries.get("Shift+m c"), Some(&"marker:chorus".to_string()),);
        assert_eq!(
            entries.get("Shift+m Shift+c"),
            Some(&"marker:chorus".to_string()),
        );
        assert_eq!(
            entries.get("Shift+m Shift+v"),
            Some(&"marker:verse".to_string()),
        );
    }

    #[test]
    fn test_case_insensitive_inherits_alt_anchor_modifier() {
        // For an Alt-anchored prefix the user typically keeps Alt held while
        // picking children. `case_insensitive` should emit both the bare and
        // `Alt+letter` variants so either chord resolves to the same action.
        let trees = vec![super::super::WhichKeyTree {
            prefix: "<A-m>".to_string(),
            label: "Modes".to_string(),
            entries: vec![
                WhichKeyEntry::leaf("r", "Record", "session:record"),
                WhichKeyEntry::leaf("e", "Edit", "session:edit"),
            ],
            case_insensitive: true,
        }];

        let entries = which_key_trees_to_keymap_entries(&trees);

        assert_eq!(entries.get("Alt+m r"), Some(&"session:record".to_string()));
        assert_eq!(
            entries.get("Alt+m Alt+r"),
            Some(&"session:record".to_string())
        );
        assert_eq!(entries.get("Alt+m e"), Some(&"session:edit".to_string()));
        assert_eq!(
            entries.get("Alt+m Alt+e"),
            Some(&"session:edit".to_string())
        );
        // No Shift variants when the anchor doesn't include Shift.
        assert!(!entries.contains_key("Alt+m Shift+r"));
    }

    #[test]
    fn test_parse_reaper_modifier_string() {
        assert_eq!(parse_reaper_modifier_string(""), "");
        assert_eq!(parse_reaper_modifier_string("<C->"), "Ctrl");
        assert_eq!(parse_reaper_modifier_string("<C-S->"), "Ctrl+Shift");
        assert_eq!(parse_reaper_modifier_string("<M->"), "Meta");
    }

    #[test]
    fn test_preset_to_keymap_config_basic() {
        let preset = KeybindPreset::new("test", "Test").with_bindings(vec![
            Keybind::new("a", "action_a"),
            Keybind::new("<C-s>", "save"),
            Keybind::new("gg", "goto_top"),
        ]);

        let config = preset_to_keymap_config(&preset, &[]);

        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("a"), Some(&"action_a".to_string()));
        assert_eq!(normal.get("Ctrl+s"), Some(&"save".to_string()));
        assert_eq!(normal.get("g g"), Some(&"goto_top".to_string()));
    }

    #[test]
    fn test_preset_to_keymap_config_preserves_custom_command_ids() {
        let preset = KeybindPreset::new("test", "Test")
            .with_bindings(vec![Keybind::new("z", "_SWS_TOGZOOMIMIN")]);

        let config = preset_to_keymap_config(&preset, &[]);

        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("z"), Some(&"_SWS_TOGZOOMIMIN".to_string()));
    }

    #[test]
    fn test_preset_with_context_bindings() {
        let preset = KeybindPreset::new("test", "Test").with_bindings(vec![
            Keybind::new("j", "nav_down").with_context(KeybindContext::Main),
            Keybind::new("k", "nav_up"),
        ]);

        let config = preset_to_keymap_config(&preset, &[]);

        // Global binding in keymap
        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("k"), Some(&"nav_up".to_string()));
        assert!(!normal.contains_key("j")); // Main-only, not in global

        // Context binding in keymap_context
        let layers = config.keymap_context.get("normal").unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].when, "context:main");
        assert_eq!(layers[0].bindings.get("j"), Some(&"nav_down".to_string()));
    }
}
