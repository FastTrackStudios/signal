//! Keymap configuration model and decoding helpers.
//!
//! The loader API is Styx-first (`from_styx_str`) and also supports JSON
//! via the same schema. In this workspace we keep decoding lightweight and
//! self-contained.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::command::ActionId;
use crate::context::WhenExpr;
use crate::event::{MouseAction, MouseButton};
use crate::key::{KeyChord, KeyCode, Modifiers};
use crate::mode::{ModeDefinition, ModeId};
use crate::mouse::MousePattern;
use crate::scroll::{ScrollAxis, ScrollPattern};
use crate::trie::LeafAction;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to decode config: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid key sequence: {0}")]
    InvalidKeySequence(String),
    #[error("invalid key chord: {0}")]
    InvalidKeyChord(String),
    #[error("invalid mouse pattern: {0}")]
    InvalidMousePattern(String),
    #[error("invalid scroll pattern: {0}")]
    InvalidScrollPattern(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeymapConfig {
    #[serde(default)]
    pub modes: HashMap<String, ModeConfig>,
    #[serde(default)]
    pub keymap: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub keymap_context: HashMap<String, Vec<ContextLayerConfig>>,
    #[serde(default)]
    pub mouse: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub scroll: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeConfig {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub passthrough_text: bool,
    #[serde(default)]
    pub sticky: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextLayerConfig {
    pub when: String,
    #[serde(default)]
    pub bindings: HashMap<String, String>,
}

impl KeymapConfig {
    /// Decode Styx source into a config.
    ///
    /// Current implementation accepts JSON-compatible Styx documents.
    pub fn from_styx_str(source: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str(source)?)
    }

    /// Decode JSON source into a config.
    pub fn from_json_str(source: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str(source)?)
    }

    /// Load config from a file path, preferring Styx by extension.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path)?;
        match path.extension().and_then(|e| e.to_str()) {
            Some("styx") => Self::from_styx_str(&source),
            Some("json") => Self::from_json_str(&source),
            _ => Self::from_styx_str(&source),
        }
    }

    /// Merge user config into base config.
    ///
    /// - User mode definitions override base per mode.
    /// - User keybindings override per key sequence.
    /// - `unbind` removes a key from the merged map.
    /// - Inserting a user binding at sequence `K` also removes every base
    ///   binding whose sequence starts with `K ` (i.e. anything *below*
    ///   `K` in the prefix tree). This keeps user leaves from being
    ///   shadowed by leftover base subtrees: e.g. if the base has `f f`,
    ///   `f e r`, …, and the user binds `f → action`, the user's `f`
    ///   resolves as a clean leaf instead of staying pending while the
    ///   trie waits for `f` to disambiguate. Siblings (`g …`, etc.) are
    ///   untouched.
    pub fn merge(base: Self, user: Self) -> Self {
        let mut merged = base;

        for (mode, def) in user.modes {
            merged.modes.insert(mode, def);
        }

        for (mode, user_bindings) in user.keymap {
            let mode_map = merged.keymap.entry(mode.clone()).or_default();

            // First pass: collect every distinct first-chord the user
            // touches in this mode. Anything starting with one of these
            // chords in the base is wiped before we insert — the user's
            // overlay fully owns each prefix namespace they bind into.
            //
            // Without this, an overlay that binds `o s` and `o a`
            // would still inherit every other `o *` entry from the
            // base (e.g. an Options which-key tree), which leaks into
            // the which-key overlay display and the trie's pending
            // continuations. With it, pressing `o` in the overlay
            // shows only `s` and `a`.
            let mut user_roots: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for key_seq in user_bindings.keys() {
                let first = key_seq.split(' ').next().unwrap_or("");
                if !first.is_empty() {
                    user_roots.insert(first.to_string());
                }
            }
            let before = mode_map.len();
            mode_map.retain(|k, _| {
                let first = k.split(' ').next().unwrap_or("");
                !user_roots.contains(first)
            });
            let wiped = before - mode_map.len();
            if wiped > 0 {
                tracing::debug!(
                    mode = %mode,
                    roots = ?user_roots,
                    wiped,
                    "[keymap-merge] Wiped base entries under user-owned prefix roots"
                );
            }

            for (key_seq, value) in user_bindings {
                if value.trim().eq_ignore_ascii_case("unbind") {
                    mode_map.remove(&key_seq);
                    tracing::debug!(
                        mode = %mode,
                        key_seq = %key_seq,
                        "[keymap-merge] Unbind"
                    );
                } else {
                    let replaced = mode_map.insert(key_seq.clone(), value.clone()).is_some();
                    tracing::debug!(
                        mode = %mode,
                        key_seq = %key_seq,
                        action = %value,
                        replaced,
                        "[keymap-merge] Insert"
                    );
                }
            }
        }

        for (mode, layers) in user.keymap_context {
            merged.keymap_context.insert(mode, layers);
        }

        for (mode, bindings) in user.mouse {
            let mode_map = merged.mouse.entry(mode).or_default();
            for (pattern, action) in bindings {
                mode_map.insert(pattern, action);
            }
        }

        for (mode, bindings) in user.scroll {
            let mode_map = merged.scroll.entry(mode).or_default();
            for (pattern, action) in bindings {
                mode_map.insert(pattern, action);
            }
        }

        merged
    }
}

const DEFAULT_KEYMAP_STYX: &str = include_str!("../defaults/keymap.styx");

/// Load the embedded default keymap config.
pub fn load_default_config() -> Result<KeymapConfig, ConfigError> {
    KeymapConfig::from_styx_str(DEFAULT_KEYMAP_STYX)
}

/// Best-effort user keymap path for current OS.
pub fn default_user_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("FastTrackStudio")
                .join("keymap.styx"),
        )
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
            return Some(
                PathBuf::from(xdg)
                    .join("fasttrackstudio")
                    .join("keymap.styx"),
            );
        }
        let home = env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("fasttrackstudio")
                .join("keymap.styx"),
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Load user config from the OS default path.
///
/// Returns `Ok(None)` when no file exists.
pub fn load_user_config() -> Result<Option<KeymapConfig>, ConfigError> {
    let Some(path) = default_user_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    KeymapConfig::from_path(path).map(Some)
}

pub fn mode_definition_from_config(id: &str, cfg: &ModeConfig) -> ModeDefinition {
    ModeDefinition::new(
        ModeId::new(id),
        if cfg.display_name.is_empty() {
            id.to_uppercase()
        } else {
            cfg.display_name.clone()
        },
    )
    .with_passthrough_text(cfg.passthrough_text)
    .with_sticky(cfg.sticky)
}

pub fn parse_key_sequence(input: &str) -> Result<Vec<KeyChord>, ConfigError> {
    let mut chords = Vec::new();
    for part in input.split_whitespace() {
        chords.push(parse_key_chord(part)?);
    }
    if chords.is_empty() {
        return Err(ConfigError::InvalidKeySequence(input.to_string()));
    }
    Ok(chords)
}

pub fn parse_key_chord(input: &str) -> Result<KeyChord, ConfigError> {
    // '+' is also used as a key character (Shift+=), but it's the chord separator,
    // so handle it before splitting.
    if input == "+" {
        return Ok(KeyChord::new(
            KeyCode::Character("+".to_string()),
            Modifiers::NONE,
        ));
    }
    let mut modifiers = Modifiers::NONE;
    let mut parts: Vec<&str> = input.split('+').collect();
    if parts.is_empty() {
        return Err(ConfigError::InvalidKeyChord(input.to_string()));
    }
    let key_token = parts.pop().unwrap_or_default().trim();
    for m in parts {
        match m.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "cmd" | "meta" | "super" => modifiers.meta = true,
            _ => return Err(ConfigError::InvalidKeyChord(input.to_string())),
        }
    }
    let key = parse_key_code(key_token)?;
    Ok(KeyChord::new(key, modifiers))
}

fn parse_key_code(token: &str) -> Result<KeyCode, ConfigError> {
    let t = token.trim();
    if t.len() == 1 {
        let c = t.chars().next().unwrap_or_default();
        return Ok(KeyCode::Character(c.to_ascii_lowercase().to_string()));
    }

    match t.to_ascii_lowercase().as_str() {
        "comma" => Ok(KeyCode::Character(",".to_string())),
        "period" | "dot" => Ok(KeyCode::Character(".".to_string())),
        "space" => Ok(KeyCode::Character(" ".to_string())),
        "equals" | "equal" => Ok(KeyCode::Character("=".to_string())),
        "minus" | "dash" => Ok(KeyCode::Character("-".to_string())),
        "enter" | "return" => Ok(KeyCode::Enter),
        "escape" | "esc" => Ok(KeyCode::Escape),
        "tab" => Ok(KeyCode::Tab),
        "backspace" | "bs" => Ok(KeyCode::Backspace),
        "delete" | "del" => Ok(KeyCode::Delete),
        "up" | "arrowup" => Ok(KeyCode::ArrowUp),
        "down" | "arrowdown" => Ok(KeyCode::ArrowDown),
        "left" | "arrowleft" => Ok(KeyCode::ArrowLeft),
        "right" | "arrowright" => Ok(KeyCode::ArrowRight),
        // Numpad keys
        "kp_enter" => Ok(KeyCode::Enter),
        other if other.starts_with("kp_") => Ok(KeyCode::Character(other.to_string())),
        other if other.starts_with('f') => {
            let n = other[1..]
                .parse::<u8>()
                .map_err(|_| ConfigError::InvalidKeyChord(token.to_string()))?;
            Ok(KeyCode::F(n))
        }
        _ => Err(ConfigError::InvalidKeyChord(token.to_string())),
    }
}

pub fn parse_leaf_action(input: &str) -> LeafAction {
    if let Some(mode) = input.strip_prefix("mode:") {
        return LeafAction::SwitchMode(ModeId::new(mode.trim()));
    }
    if let Some(op) = input.strip_prefix("operator:") {
        return LeafAction::Operator(op.trim().to_string());
    }
    if let Some(motion) = input.strip_prefix("motion:") {
        return LeafAction::Motion(motion.trim().to_string());
    }
    if let Some(text_obj) = input.strip_prefix("textobj:") {
        return LeafAction::TextObject(text_obj.trim().to_string());
    }
    if input.trim().eq_ignore_ascii_case("unbind") {
        return LeafAction::Unbind;
    }
    LeafAction::Action(ActionId::new(input.trim()))
}

pub fn parse_when_expr(input: &str) -> WhenExpr {
    WhenExpr::parse(input)
}

pub fn parse_mouse_pattern(input: &str) -> Result<MousePattern, ConfigError> {
    let (lhs, rhs) = input
        .split_once('.')
        .ok_or_else(|| ConfigError::InvalidMousePattern(input.to_string()))?;

    let action = parse_mouse_action(rhs.trim())?;
    let mut modifiers = Modifiers::NONE;
    let mut lhs_parts: Vec<&str> = lhs.split('+').collect();
    let button_token = lhs_parts
        .pop()
        .ok_or_else(|| ConfigError::InvalidMousePattern(input.to_string()))?;

    for m in lhs_parts {
        match m.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "cmd" | "meta" | "super" => modifiers.meta = true,
            _ => return Err(ConfigError::InvalidMousePattern(input.to_string())),
        }
    }

    let button = parse_mouse_button(button_token.trim())?;
    Ok(MousePattern::new(button, action, modifiers))
}

pub fn parse_scroll_pattern(input: &str) -> Result<ScrollPattern, ConfigError> {
    let mut modifiers = Modifiers::NONE;
    let mut parts: Vec<&str> = input.split('+').collect();
    if parts.is_empty() {
        return Err(ConfigError::InvalidScrollPattern(input.to_string()));
    }

    let axis_token = parts
        .pop()
        .ok_or_else(|| ConfigError::InvalidScrollPattern(input.to_string()))?;

    for m in parts {
        match m.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "cmd" | "meta" | "super" => modifiers.meta = true,
            _ => return Err(ConfigError::InvalidScrollPattern(input.to_string())),
        }
    }

    let axis = match axis_token.trim().to_ascii_lowercase().as_str() {
        "scroll" | "wheel" => ScrollAxis::Any,
        "scrollx" | "wheelx" | "hscroll" | "horizontal" => ScrollAxis::Horizontal,
        "scrolly" | "wheely" | "vscroll" | "vertical" => ScrollAxis::Vertical,
        _ => return Err(ConfigError::InvalidScrollPattern(input.to_string())),
    };

    Ok(ScrollPattern::new(axis, modifiers))
}

fn parse_mouse_button(token: &str) -> Result<MouseButton, ConfigError> {
    match token.to_ascii_lowercase().as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        _ => Err(ConfigError::InvalidMousePattern(token.to_string())),
    }
}

fn parse_mouse_action(token: &str) -> Result<MouseAction, ConfigError> {
    match token.to_ascii_lowercase().as_str() {
        "press" | "down" => Ok(MouseAction::Press),
        "release" | "up" => Ok(MouseAction::Release),
        "click" => Ok(MouseAction::Click),
        "doubleclick" | "double_click" | "double-click" => Ok(MouseAction::DoubleClick),
        _ => Err(ConfigError::InvalidMousePattern(token.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{InputEvent, KeyEvent, MouseEvent};
    use crate::processor::InputProcessor;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn sample_config_json() -> &'static str {
        r#"{
            "modes": {
                "normal": { "display_name": "NORMAL" },
                "insert": { "display_name": "INSERT", "passthrough_text": true }
            },
            "keymap": {
                "normal": {
                    "j": "cursor.down",
                    "g g": "cursor.top",
                    "i": "mode:insert",
                    "d": "operator:delete",
                    "w": "motion:word"
                }
            },
            "keymap_context": {
                "normal": [
                    { "when": "tab:performance", "bindings": { "j": "perf.next_song" } }
                ]
            },
            "mouse": {
                "normal": {
                    "Left.Click": "mouse.primary",
                    "Ctrl+Left.DoubleClick": "mouse.ctrl_double"
                }
            },
            "scroll": {
                "normal": {
                    "Ctrl+Scroll": "view.zoom",
                    "Shift+ScrollX": "view.hscroll"
                }
            }
        }"#
    }

    #[test]
    fn test_decode_styx_json_and_modes() -> Result<()> {
        let cfg = KeymapConfig::from_styx_str(sample_config_json())?;
        assert!(cfg.modes.contains_key("normal"));
        assert!(cfg.modes.contains_key("insert"));
        Ok(())
    }

    #[test]
    fn test_parse_key_sequence_multi_chord() -> Result<()> {
        let seq = parse_key_sequence("g g")?;
        assert_eq!(seq.len(), 2);
        Ok(())
    }

    #[test]
    fn test_parse_mouse_pattern_with_modifiers() -> Result<()> {
        let pat = parse_mouse_pattern("Ctrl+Left.DoubleClick")?;
        assert_eq!(pat.button, MouseButton::Left);
        assert_eq!(pat.action, MouseAction::DoubleClick);
        assert!(pat.modifiers.ctrl);
        Ok(())
    }

    #[test]
    fn test_parse_scroll_pattern_with_modifiers() -> Result<()> {
        let pat = parse_scroll_pattern("Ctrl+ScrollX")?;
        assert_eq!(pat.axis, ScrollAxis::Horizontal);
        assert!(pat.modifiers.ctrl);
        Ok(())
    }

    #[test]
    fn test_round_trip_from_config_process_flow() -> Result<()> {
        let cfg = KeymapConfig::from_styx_str(sample_config_json())?;
        let mut processor = InputProcessor::from_config(cfg)?;
        let mut ctx = crate::context::ActionContext::new();
        ctx.set_var("tab", "performance");

        // Context override should win.
        let cmds = processor.process(
            InputEvent::Key(KeyEvent {
                key: KeyCode::Character("j".to_string()),
                modifiers: Modifiers::NONE,
            }),
            &ctx,
        );
        assert!(matches!(
            &cmds[0],
            crate::command::InputCommand::Action(id) if id.as_str() == "perf.next_song"
                || matches!(&cmds[0], crate::command::InputCommand::ActionWithArgs{action, ..} if action.as_str() == "perf.next_song")
        ));

        // Mouse binding should parse and dispatch.
        let cmds = processor.process(
            InputEvent::Mouse(MouseEvent {
                button: MouseButton::Left,
                action: MouseAction::Click,
                x: 10.0,
                y: 20.0,
                modifiers: Modifiers::NONE,
            }),
            &ctx,
        );
        assert!(
            matches!(&cmds[0], crate::command::InputCommand::ActionWithArgs{action, ..} if action.as_str() == "mouse.primary")
        );

        // Scroll binding should parse and dispatch.
        let cmds = processor.process(
            InputEvent::Scroll(crate::event::ScrollEvent {
                delta_x: 0.0,
                delta_y: -8.0,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                },
            }),
            &ctx,
        );
        assert!(
            matches!(&cmds[0], crate::command::InputCommand::ActionWithArgs{action, ..} if action.as_str() == "view.zoom")
        );
        Ok(())
    }

    #[test]
    fn test_merge_user_override_replaces_binding() -> Result<()> {
        let mut base = KeymapConfig::default();
        base.keymap
            .entry("normal".to_string())
            .or_default()
            .insert("j".to_string(), "cursor.down".to_string());

        let mut user = KeymapConfig::default();
        user.keymap
            .entry("normal".to_string())
            .or_default()
            .insert("j".to_string(), "cursor.custom_down".to_string());

        let merged = KeymapConfig::merge(base, user);
        assert_eq!(merged.keymap["normal"]["j"].as_str(), "cursor.custom_down");
        Ok(())
    }

    #[test]
    fn test_merge_unbind_removes_base_binding() -> Result<()> {
        let mut base = KeymapConfig::default();
        base.keymap
            .entry("normal".to_string())
            .or_default()
            .insert("j".to_string(), "cursor.down".to_string());

        let mut user = KeymapConfig::default();
        user.keymap
            .entry("normal".to_string())
            .or_default()
            .insert("j".to_string(), "unbind".to_string());

        let merged = KeymapConfig::merge(base, user);
        assert!(!merged.keymap["normal"].contains_key("j"));
        Ok(())
    }

    #[test]
    fn test_merge_user_adds_new_binding() -> Result<()> {
        let base = KeymapConfig::default();
        let mut user = KeymapConfig::default();
        user.keymap
            .entry("normal".to_string())
            .or_default()
            .insert("k".to_string(), "cursor.up".to_string());

        let merged = KeymapConfig::merge(base, user);
        assert_eq!(merged.keymap["normal"]["k"].as_str(), "cursor.up");
        Ok(())
    }
}
