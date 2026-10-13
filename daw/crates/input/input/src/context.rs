//! Action context for when-clause evaluation.
//!
//! A lightweight context that keybindings can check against to decide
//! whether a binding is active (e.g., "only in insert mode" or
//! "only when panel is focused").

use std::collections::{HashMap, HashSet};

/// Execution context carrying tags (boolean flags) and variables (key-value pairs).
///
/// This is a local version of the `ActionContext` from `actions-proto`,
/// kept self-contained to avoid the upstream `vox-session` breakage.
#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    tags: HashSet<String>,
    vars: HashMap<String, String>,
}

/// Simple when-clause expression used by context-aware layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenExpr {
    /// Always true.
    True,
    /// Match a context tag exactly.
    Tag(String),
    /// Match a context variable value, e.g. `tab:performance`.
    VarEq { key: String, value: String },
    /// All sub-expressions must evaluate true.
    And(Vec<WhenExpr>),
}

impl ActionContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a boolean tag (e.g., "panel:focused").
    pub fn set_tag(&mut self, tag: impl Into<String>) {
        self.tags.insert(tag.into());
    }

    /// Remove a boolean tag.
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.remove(tag);
    }

    /// Check whether a tag is present.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    /// Set a variable (e.g., "mode" → "normal").
    pub fn set_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    /// Get a variable value.
    pub fn get_var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }

    /// Convenience: set mode variable (for modal editing).
    pub fn set_mode(&mut self, mode: &str) {
        self.set_var("mode", mode);
    }

    /// Convenience: set active tab (updates both var and tag form `tab:<name>`).
    pub fn set_tab(&mut self, tab: &str) {
        self.tags.retain(|t| !t.starts_with("tab:"));
        self.set_var("tab", tab);
        self.set_tag(format!("tab:{tab}"));
    }
}

impl WhenExpr {
    /// Parse a tiny expression language:
    /// - `key:value` => variable equality (or matching tag)
    /// - `foo && bar:baz` => logical AND
    /// - empty/`true` => true
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("true") {
            return Self::True;
        }

        let parts: Vec<_> = trimmed
            .split("&&")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() > 1 {
            return Self::And(parts.into_iter().map(Self::parse_atom).collect());
        }
        Self::parse_atom(trimmed)
    }

    fn parse_atom(input: &str) -> Self {
        if let Some((key, value)) = input.split_once(':') {
            return Self::VarEq {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            };
        }
        Self::Tag(input.trim().to_string())
    }

    pub fn evaluate(&self, ctx: &ActionContext) -> bool {
        match self {
            Self::True => true,
            Self::Tag(tag) => ctx.has_tag(tag),
            Self::VarEq { key, value } => {
                ctx.get_var(key).is_some_and(|v| v == value)
                    || ctx.has_tag(&format!("{key}:{value}"))
            }
            Self::And(items) => items.iter().all(|expr| expr.evaluate(ctx)),
        }
    }
}
