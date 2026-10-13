//! Flat binding extraction from trie / table structures for UI display.

use input::context::WhenExpr;
use input::event::{MouseAction, MouseButton};
use input::key::{KeyChord, KeyCode, Modifiers};
use input::mouse::MouseBindingTable;
use input::scroll::{ScrollAxis, ScrollBindingTable};
use input::trie::{KeyTrie, LeafAction};

// ---------------------------------------------------------------------------
// Action sections (color-coded categories)
// ---------------------------------------------------------------------------

/// Semantic section derived from action name prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Classify an action label into a section based on its dotted prefix.
    pub fn from_action_label(label: &str) -> Self {
        // Check LeafActionSummary-style prefixes first (these come from label())
        if label.starts_with("op:") || label.starts_with("mot:") || label.starts_with("obj:") {
            return Self::Vim;
        }
        if label.starts_with("→ ") || label.starts_with("⊕ ") {
            return Self::Mode;
        }

        // Dotted action IDs
        if label.starts_with("fts.session.") {
            return Self::Session;
        }
        if label.starts_with("cursor.") || label.starts_with("chart.cursor.") {
            return Self::Navigation;
        }
        if label.starts_with("motion:") || label.starts_with("operator:") {
            return Self::Vim;
        }
        if label.starts_with("mode:") {
            return Self::Mode;
        }
        if label.starts_with("fts.rig.") {
            return Self::Rig;
        }
        if label.starts_with("fts.standalone.") {
            return Self::App;
        }
        if label.starts_with("input.") {
            return Self::Input;
        }
        if label.starts_with("mouse.") || label.starts_with("view.") {
            return Self::MouseView;
        }

        Self::Other
    }

    /// Short display name for filter chips and table rows.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Navigation => "Nav",
            Self::Vim => "Vim",
            Self::Mode => "Mode",
            Self::Rig => "Rig",
            Self::App => "App",
            Self::Input => "Input",
            Self::MouseView => "Mouse/View",
            Self::Other => "Other",
        }
    }

    /// Tailwind background class for key caps (translucent, dark).
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

    /// Tailwind background class for filter-chip dots (vivid).
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

    /// Tailwind text class for table rows.
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

    /// All defined section variants (for building filter UI).
    pub const ALL: &'static [ActionSection] = &[
        Self::Session,
        Self::Navigation,
        Self::Vim,
        Self::Mode,
        Self::Rig,
        Self::App,
        Self::Input,
        Self::MouseView,
        Self::Other,
    ];
}

// ---------------------------------------------------------------------------
// Key bindings
// ---------------------------------------------------------------------------

/// A single key binding flattened from the trie for display.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyBindingInfo {
    /// The full key sequence (e.g. `[g, g]`).
    pub sequence: Vec<KeyChord>,
    /// Human-readable display string (e.g. `"g g"`).
    pub display: String,
    /// What the binding does.
    pub action: LeafActionSummary,
    /// Color-coded section derived from the action label.
    pub section: ActionSection,
}

/// Summarized leaf action for display purposes.
#[derive(Debug, Clone, PartialEq)]
pub enum LeafActionSummary {
    Action(String),
    SwitchMode(String),
    PushMode(String),
    Operator(String),
    Motion(String),
    TextObject(String),
    Sequence(Vec<String>),
    Unbind,
}

impl LeafActionSummary {
    fn from_leaf(leaf: &LeafAction) -> Self {
        match leaf {
            LeafAction::Action(id) => Self::Action(id.as_str().to_string()),
            LeafAction::SwitchMode(m) => Self::SwitchMode(m.as_str().to_string()),
            LeafAction::PushMode(m) => Self::PushMode(m.as_str().to_string()),
            LeafAction::Operator(s) => Self::Operator(s.clone()),
            LeafAction::Motion(s) => Self::Motion(s.clone()),
            LeafAction::TextObject(s) => Self::TextObject(s.clone()),
            LeafAction::Sequence(ids) => {
                Self::Sequence(ids.iter().map(|id| id.as_str().to_string()).collect())
            }
            LeafAction::Unbind => Self::Unbind,
        }
    }

    /// Primary display label for the action.
    pub fn label(&self) -> String {
        match self {
            Self::Action(s) => s.clone(),
            Self::SwitchMode(s) => format!("→ {s}"),
            Self::PushMode(s) => format!("⊕ {s}"),
            Self::Operator(s) => format!("op:{s}"),
            Self::Motion(s) => format!("mot:{s}"),
            Self::TextObject(s) => format!("obj:{s}"),
            Self::Sequence(ids) => ids.join(" ; "),
            Self::Unbind => "—".to_string(),
        }
    }

    /// The raw action ID used for section classification (before display formatting).
    fn section_key(&self) -> &str {
        match self {
            Self::Action(s) => s,
            Self::SwitchMode(s) | Self::PushMode(s) => s,
            Self::Operator(s) => s,
            Self::Motion(s) => s,
            Self::TextObject(s) => s,
            Self::Sequence(ids) => ids.first().map(|s| s.as_str()).unwrap_or(""),
            Self::Unbind => "",
        }
    }
}

/// Classify a `LeafActionSummary` into a section.
///
/// Uses the raw action ID for dotted-prefix matching, with fallback to the
/// display label for operator/motion/mode prefixes.
fn section_for_summary(summary: &LeafActionSummary) -> ActionSection {
    // First try the raw key for dotted prefixes
    let raw = summary.section_key();
    let section = ActionSection::from_action_label(raw);
    if section != ActionSection::Other {
        return section;
    }
    // Fallback: classify by variant type
    match summary {
        LeafActionSummary::Operator(_)
        | LeafActionSummary::Motion(_)
        | LeafActionSummary::TextObject(_) => ActionSection::Vim,
        LeafActionSummary::SwitchMode(_) | LeafActionSummary::PushMode(_) => ActionSection::Mode,
        _ => ActionSection::from_action_label(&summary.label()),
    }
}

/// Walk a `KeyTrie` and collect all leaf bindings as flat records.
pub fn collect_key_bindings(trie: &KeyTrie) -> Vec<KeyBindingInfo> {
    let mut out = Vec::new();
    walk_trie(trie, &mut Vec::new(), &mut out);
    out.sort_by(|a, b| a.display.cmp(&b.display));
    out
}

fn walk_trie(trie: &KeyTrie, path: &mut Vec<KeyChord>, out: &mut Vec<KeyBindingInfo>) {
    match trie {
        KeyTrie::Leaf(leaf) => {
            let action = LeafActionSummary::from_leaf(leaf);
            let section = section_for_summary(&action);
            out.push(KeyBindingInfo {
                sequence: path.clone(),
                display: format_chord_sequence(path),
                action,
                section,
            });
        }
        KeyTrie::Node(node) => {
            for (chord, child) in &node.children {
                path.push(chord.clone());
                walk_trie(child, path, out);
                path.pop();
            }
            if let Some(default) = &node.default {
                let action = LeafActionSummary::from_leaf(default);
                let section = section_for_summary(&action);
                out.push(KeyBindingInfo {
                    sequence: path.clone(),
                    display: if path.is_empty() {
                        "(default)".to_string()
                    } else {
                        format!("{} (default)", format_chord_sequence(path))
                    },
                    action,
                    section,
                });
            }
        }
    }
}

/// Format a chord sequence as a human-readable string.
pub fn format_chord_sequence(chords: &[KeyChord]) -> String {
    chords
        .iter()
        .map(format_chord)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a single chord as a human-readable string.
pub fn format_chord(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.modifiers.ctrl {
        s.push_str("C-");
    }
    if chord.modifiers.alt {
        s.push_str("A-");
    }
    if chord.modifiers.shift {
        s.push_str("S-");
    }
    if chord.modifiers.meta {
        s.push_str("M-");
    }
    match &chord.key {
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
            s.push('F');
            s.push_str(&n.to_string());
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Context binding groups
// ---------------------------------------------------------------------------

/// A group of key bindings that share the same when-clause context.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBindingGroup {
    /// Human-readable label for the context (e.g. "tab:performance").
    pub when_label: String,
    /// Bindings active within this context layer.
    pub bindings: Vec<KeyBindingInfo>,
}

/// Collect bindings from context keymap layers into labeled groups.
pub fn collect_context_bindings(layers: &[(WhenExpr, KeyTrie)]) -> Vec<ContextBindingGroup> {
    layers
        .iter()
        .map(|(when, trie)| {
            let when_label = format_when_expr(when);
            let bindings = collect_key_bindings(trie);
            ContextBindingGroup {
                when_label,
                bindings,
            }
        })
        .collect()
}

/// Format a `WhenExpr` as a human-readable label.
fn format_when_expr(expr: &WhenExpr) -> String {
    match expr {
        WhenExpr::True => "Global".to_string(),
        WhenExpr::Tag(t) => t.clone(),
        WhenExpr::VarEq { key, value } => format!("{key}:{value}"),
        WhenExpr::And(items) => items
            .iter()
            .map(format_when_expr)
            .collect::<Vec<_>>()
            .join(" && "),
    }
}

/// Extract the short context name from a when-label for display.
///
/// `"tab:performance"` → `"Performance"`, `"tab:chart"` → `"Chart"`.
pub fn context_display_name(when_label: &str) -> String {
    if when_label == "Global" {
        return "Global".to_string();
    }
    if let Some(value) = when_label.strip_prefix("tab:") {
        let mut chars = value.chars();
        match chars.next() {
            Some(c) => {
                let mut s = c.to_uppercase().to_string();
                s.push_str(chars.as_str());
                s
            }
            None => when_label.to_string(),
        }
    } else {
        when_label.to_string()
    }
}

/// Collect all unique `ActionSection` variants present in a set of bindings.
pub fn collect_all_sections(bindings: &[KeyBindingInfo]) -> Vec<ActionSection> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for b in bindings {
        if seen.insert(b.section) {
            out.push(b.section);
        }
    }
    // Return in canonical order
    ActionSection::ALL
        .iter()
        .copied()
        .filter(|s| seen.contains(s))
        .collect()
}

/// Classify a mouse binding action into a section.
pub fn section_for_mouse(action_id: &str) -> ActionSection {
    ActionSection::from_action_label(action_id)
}

/// Classify a scroll binding action into a section.
pub fn section_for_scroll(action_id: &str) -> ActionSection {
    ActionSection::from_action_label(action_id)
}

// ---------------------------------------------------------------------------
// Mouse bindings
// ---------------------------------------------------------------------------

/// A single mouse binding flattened for display.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseBindingInfo {
    /// Human-readable modifier+button+action pattern.
    pub display: String,
    pub button: MouseButton,
    pub mouse_action: MouseAction,
    pub modifiers: Modifiers,
    pub action_id: String,
    /// Color-coded section.
    pub section: ActionSection,
}

/// Extract all mouse bindings from a table.
pub fn collect_mouse_bindings(table: &MouseBindingTable) -> Vec<MouseBindingInfo> {
    table
        .bindings()
        .iter()
        .map(|(pattern, _when, action)| {
            let display = format_mouse_pattern(pattern.button, pattern.action, pattern.modifiers);
            let action_id = action.as_str().to_string();
            let section = section_for_mouse(&action_id);
            MouseBindingInfo {
                display,
                button: pattern.button,
                mouse_action: pattern.action,
                modifiers: pattern.modifiers,
                action_id,
                section,
            }
        })
        .collect()
}

fn format_mouse_pattern(button: MouseButton, action: MouseAction, mods: Modifiers) -> String {
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    if mods.meta {
        s.push_str("Cmd+");
    }
    s.push_str(match button {
        MouseButton::Left => "Left",
        MouseButton::Right => "Right",
        MouseButton::Middle => "Middle",
    });
    s.push('.');
    s.push_str(match action {
        MouseAction::Press => "Press",
        MouseAction::Release => "Release",
        MouseAction::Click => "Click",
        MouseAction::DoubleClick => "DblClick",
    });
    s
}

// ---------------------------------------------------------------------------
// Scroll bindings
// ---------------------------------------------------------------------------

/// A single scroll binding flattened for display.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollBindingInfo {
    /// Human-readable modifier+axis pattern.
    pub display: String,
    pub axis: ScrollAxis,
    pub modifiers: Modifiers,
    pub action_id: String,
    /// Color-coded section.
    pub section: ActionSection,
}

/// Extract all scroll bindings from a table.
pub fn collect_scroll_bindings(table: &ScrollBindingTable) -> Vec<ScrollBindingInfo> {
    table
        .bindings()
        .iter()
        .map(|(pattern, _when, action)| {
            let display = format_scroll_pattern(pattern.axis, pattern.modifiers);
            let action_id = action.as_str().to_string();
            let section = section_for_scroll(&action_id);
            ScrollBindingInfo {
                display,
                axis: pattern.axis,
                modifiers: pattern.modifiers,
                action_id,
                section,
            }
        })
        .collect()
}

fn format_scroll_pattern(axis: ScrollAxis, mods: Modifiers) -> String {
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    if mods.meta {
        s.push_str("Cmd+");
    }
    s.push_str("Scroll.");
    s.push_str(match axis {
        ScrollAxis::Any => "Any",
        ScrollAxis::Horizontal => "H",
        ScrollAxis::Vertical => "V",
    });
    s
}

// ---------------------------------------------------------------------------
// Filtering helpers
// ---------------------------------------------------------------------------

/// Filter key bindings to those whose first chord matches the given modifiers.
pub fn filter_by_modifiers(bindings: &[KeyBindingInfo], mods: Modifiers) -> Vec<&KeyBindingInfo> {
    bindings
        .iter()
        .filter(|b| {
            b.sequence
                .first()
                .is_some_and(|chord| chord.modifiers == mods)
        })
        .collect()
}

/// Find all key bindings whose first chord matches a given key code (ignoring modifiers).
pub fn bindings_for_key<'a>(
    bindings: &'a [KeyBindingInfo],
    key: &KeyCode,
) -> Vec<&'a KeyBindingInfo> {
    bindings
        .iter()
        .filter(|b| b.sequence.first().is_some_and(|chord| &chord.key == key))
        .collect()
}
