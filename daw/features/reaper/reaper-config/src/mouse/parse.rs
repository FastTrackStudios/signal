//! INI parser for `reaper-mouse.ini` and `.ReaperMouseMap` files.

use super::types::{
    ActionValue, ContextKind, HasImportedEntry, ModifierIndex, MouseBinding, MouseConfig,
    MouseContext,
};
use thiserror::Error;

/// Errors that can occur while parsing a mouse config file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid integer on line {line}: {source}")]
    InvalidInt {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("missing '=' on line {line}: {content:?}")]
    MissingEquals { line: usize, content: String },

    #[error("invalid mm_N key on line {line}: {content:?}")]
    InvalidModifierKey { line: usize, content: String },

    #[error("invalid binding value on line {line}: {content:?}")]
    InvalidBindingValue { line: usize, content: String },
}

/// Parse an `ActionValue` from the right-hand side of a `mm_N=…` entry.
fn parse_action_value(rhs: &str, line: usize) -> Result<ActionValue, ParseError> {
    let rhs = rhs.trim();

    // Named action command ID.
    if rhs.starts_with('_') {
        return Ok(ActionValue::Named(rhs.to_string()));
    }

    // Split on whitespace.
    let mut parts = rhs.splitn(2, ' ');
    let num_str = parts.next().unwrap_or("").trim();
    let suffix = parts.next().map(|s| s.trim());

    let n = num_str
        .parse::<i32>()
        .map_err(|e| ParseError::InvalidInt { line, source: e })?;

    match suffix {
        Some("m") | None => Ok(ActionValue::MouseBehavior(n)),
        Some("c") => Ok(ActionValue::CommandId(n)),
        Some(_) => Err(ParseError::InvalidBindingValue {
            line,
            content: rhs.to_string(),
        }),
    }
}

/// Try to parse a `mm_N` key into its modifier index.
fn parse_modifier_key(key: &str, line: usize) -> Result<Option<u8>, ParseError> {
    let lower = key.to_ascii_lowercase();
    if !lower.starts_with("mm_") {
        return Ok(None);
    }
    let index_str = &lower[3..];
    let idx = index_str
        .parse::<u8>()
        .map_err(|e| ParseError::InvalidInt { line, source: e })?;
    Ok(Some(idx))
}

/// Parse the contents of a `reaper-mouse.ini` or `.ReaperMouseMap` file.
pub fn parse(input: &str) -> Result<MouseConfig, ParseError> {
    let mut config = MouseConfig::new_mouse_map();

    enum Section {
        None,
        HasImported,
        Context(MouseContext),
    }

    let mut state = Section::None;

    let lines: Vec<&str> = input.lines().collect();

    let flush = |state: Section, config: &mut MouseConfig| {
        if let Section::Context(ctx) = state {
            config.contexts.push(ctx);
        }
    };

    for (idx, raw_line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim_end_matches('\r').trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_name = &line[1..line.len() - 1];

            let prev = std::mem::replace(&mut state, Section::None);
            flush(prev, &mut config);

            if section_name.eq_ignore_ascii_case("hasimported") {
                config.has_imported = Some(Vec::new());
                state = Section::HasImported;
            } else {
                let kind = ContextKind::from_reaper_str(section_name);
                state = Section::Context(MouseContext::new(kind));
            }
            continue;
        }

        let eq_pos = line.find('=').ok_or_else(|| ParseError::MissingEquals {
            line: line_num,
            content: line.to_string(),
        })?;

        let key = line[..eq_pos].trim();
        let value = line[eq_pos + 1..].trim();

        match &mut state {
            Section::None => {}

            Section::HasImported => {
                let n = value.parse::<i32>().map_err(|e| ParseError::InvalidInt {
                    line: line_num,
                    source: e,
                })?;
                if let Some(ref mut entries) = config.has_imported {
                    entries.push(HasImportedEntry {
                        key: key.to_string(),
                        value: n,
                    });
                }
            }

            Section::Context(ctx) => {
                if key.eq_ignore_ascii_case("name") {
                    ctx.name = Some(value.to_string());
                } else {
                    if let Some(idx) = parse_modifier_key(key, line_num)? {
                        let action = parse_action_value(value, line_num)?;
                        ctx.bindings.push(MouseBinding {
                            modifier: ModifierIndex(idx),
                            action,
                        });
                    }
                }
            }
        }
    }

    flush(state, &mut config);

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::super::types::{ActionValue, ContextKind, ModifierIndex};
    use super::*;

    #[test]
    fn parse_empty_input() {
        let cfg = parse("").unwrap();
        assert!(cfg.has_imported.is_none());
        assert!(cfg.contexts.is_empty());
    }

    #[test]
    fn parse_hasimported_section() {
        let input = "[hasimported]\r\nMM_CTX_ITEM=1\r\nMM_CTX_ITEM_CLK=1\r\n";
        let cfg = parse(input).unwrap();
        let hi = cfg.has_imported.as_ref().unwrap();
        assert_eq!(hi.len(), 2);
        assert_eq!(hi[0].key, "MM_CTX_ITEM");
        assert_eq!(hi[0].value, 1);
        assert_eq!(hi[1].key, "MM_CTX_ITEM_CLK");
    }

    #[test]
    fn parse_context_section_mouse_behavior() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\nmm_2=1 m\r\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.contexts.len(), 1);
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.kind, ContextKind::ItemClick);
        assert_eq!(ctx.bindings.len(), 2);
        assert_eq!(ctx.bindings[0].modifier, ModifierIndex::NONE);
        assert_eq!(ctx.bindings[0].action, ActionValue::MouseBehavior(3));
        assert_eq!(ctx.bindings[1].modifier, ModifierIndex::CTRL);
        assert_eq!(ctx.bindings[1].action, ActionValue::MouseBehavior(1));
    }

    #[test]
    fn parse_context_section_command_id() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=40001 c\r\n";
        let cfg = parse(input).unwrap();
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.bindings[0].action, ActionValue::CommandId(40001));
    }

    #[test]
    fn parse_context_section_named_action() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_4=_MY_ACTION\r\n";
        let cfg = parse(input).unwrap();
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.bindings[0].modifier, ModifierIndex::ALT);
        assert_eq!(
            ctx.bindings[0].action,
            ActionValue::Named("_MY_ACTION".to_string())
        );
    }

    #[test]
    fn parse_name_entry() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\nname=My Tooltip\r\n";
        let cfg = parse(input).unwrap();
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.name.as_deref(), Some("My Tooltip"));
    }

    #[test]
    fn parse_without_suffix_treated_as_mouse_behavior() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3\r\n";
        let cfg = parse(input).unwrap();
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.bindings[0].action, ActionValue::MouseBehavior(3));
    }

    #[test]
    fn parse_multiple_contexts() {
        let input = "\
[MM_CTX_ITEM_CLK]\r\n\
mm_0=3 m\r\n\
[MM_CTX_ITEM]\r\n\
mm_2=4 m\r\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.contexts.len(), 2);
        assert_eq!(cfg.contexts[0].kind, ContextKind::ItemClick);
        assert_eq!(cfg.contexts[1].kind, ContextKind::Item);
    }

    #[test]
    fn parse_hasimported_then_contexts() {
        let input = "\
[hasimported]\r\n\
MM_CTX_ITEM=1\r\n\
[MM_CTX_ITEM_CLK]\r\n\
mm_0=3 m\r\n";
        let cfg = parse(input).unwrap();
        assert!(cfg.has_imported.is_some());
        assert_eq!(cfg.contexts.len(), 1);
        assert!(cfg.is_mouse_ini());
    }

    #[test]
    fn parse_negative_value_in_hasimported() {
        let input = "[hasimported]\r\nMM_CTX_ITEM=-1\r\n";
        let cfg = parse(input).unwrap();
        let hi = cfg.has_imported.as_ref().unwrap();
        assert_eq!(hi[0].value, -1);
    }

    #[test]
    fn parse_unknown_context_name_preserved() {
        let input = "[MM_CTX_FUTURE_THING]\r\nmm_0=5 m\r\n";
        let cfg = parse(input).unwrap();
        assert_eq!(
            cfg.contexts[0].kind,
            ContextKind::Other("MM_CTX_FUTURE_THING".to_string())
        );
    }

    #[test]
    fn parse_crlf_and_lf_mixed() {
        let input = "[MM_CTX_ITEM_CLK]\nmm_0=3 m\r\nmm_1=1 m\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.contexts[0].bindings.len(), 2);
    }

    #[test]
    fn parse_all_modifier_indices() {
        let mut input = String::from("[MM_CTX_ITEM_CLK]\r\n");
        for i in 0u8..=15 {
            input.push_str(&format!("mm_{i}={i} m\r\n"));
        }
        let cfg = parse(&input).unwrap();
        let ctx = &cfg.contexts[0];
        assert_eq!(ctx.bindings.len(), 16);
        for (i, b) in ctx.bindings.iter().enumerate() {
            assert_eq!(b.modifier, ModifierIndex(i as u8));
        }
    }
}
