//! Minimal action context and when-expression evaluation for dock visibility.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    tags: HashSet<String>,
    vars: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenExpr {
    True,
    Tag(String),
    VarEq { key: String, value: String },
    And(Vec<WhenExpr>),
}

impl ActionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_tag(&mut self, tag: impl Into<String>) {
        self.tags.insert(tag.into());
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    pub fn set_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    pub fn get_var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }

    pub fn set_tab(&mut self, tab: &str) {
        self.tags.retain(|t| !t.starts_with("tab:"));
        self.set_var("tab", tab);
        self.set_tag(format!("tab:{tab}"));
    }
}

impl WhenExpr {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_tab_context() {
        let mut ctx = ActionContext::new();
        ctx.set_tab("performance");
        assert!(WhenExpr::parse("tab:performance").evaluate(&ctx));
        assert!(!WhenExpr::parse("tab:chart").evaluate(&ctx));
    }
}
