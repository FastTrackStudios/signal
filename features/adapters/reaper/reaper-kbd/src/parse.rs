//! Parsing logic for REAPER keyboard mapping files.

use crate::types::*;

use thiserror::Error;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid line prefix on line {line}: {content}")]
    InvalidPrefix { line: usize, content: String },

    #[error("not enough fields on KEY line {line}: {content}")]
    KeyTooFewFields { line: usize, content: String },

    #[error("invalid integer on line {line}: {source}")]
    InvalidInt {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("not enough fields on ACT line {line}: {content}")]
    ActTooFewFields { line: usize, content: String },

    #[error("not enough fields on SCR line {line}: {content}")]
    ScrTooFewFields { line: usize, content: String },

    #[error("unterminated quoted string on line {line}")]
    UnterminatedQuote { line: usize },
}

/// Parse a command ID from a string token: numeric => Native, _-prefixed => Named.
fn parse_command_id(s: &str) -> CommandId {
    if s.starts_with('_') {
        CommandId::Named(s.to_string())
    } else {
        match s.parse::<u32>() {
            Ok(n) => CommandId::Native(n),
            Err(_) => CommandId::Named(s.to_string()),
        }
    }
}

/// Extract a quoted string starting at `pos` in `fields_str`, returning the
/// unquoted content and the position after the closing quote.
fn extract_quoted(s: &str, start: usize, line_num: usize) -> Result<(String, usize), ParseError> {
    // `start` should point at the opening `"`.
    if s.as_bytes().get(start) != Some(&b'"') {
        return Err(ParseError::UnterminatedQuote { line: line_num });
    }
    let after_open = start + 1;
    if let Some(close) = s[after_open..].find('"') {
        let content = &s[after_open..after_open + close];
        Ok((content.to_string(), after_open + close + 1))
    } else {
        Err(ParseError::UnterminatedQuote { line: line_num })
    }
}

/// Determine if a section value indicates a global-flag line (102 or 103).
fn is_global_section(section_raw: u32) -> bool {
    section_raw == 102 || section_raw == 103
}

/// Parse a single KEY line into its raw fields.
struct RawKey {
    modifier: u16,
    key: u16,
    command_or_scope: String,
    section: u32,
}

fn parse_raw_key(fields: &[&str], line_num: usize) -> Result<RawKey, ParseError> {
    if fields.len() < 5 {
        return Err(ParseError::KeyTooFewFields {
            line: line_num,
            content: fields.join(" "),
        });
    }
    let modifier = fields[1]
        .parse::<u16>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;
    let key = fields[2]
        .parse::<u16>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;
    let command_or_scope = fields[3].to_string();
    let section = fields[4]
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;
    Ok(RawKey {
        modifier,
        key,
        command_or_scope,
        section,
    })
}

/// Parse an ACT line. Format:
/// `ACT flags section "ActionCommandID" "Description" ActionCommandID [ActionCommandID]...`
fn parse_act_line(line: &str, line_num: usize) -> Result<CustomAction, ParseError> {
    // Split only the first three tokens (ACT, flags, section), rest parsed manually for quotes.
    let rest = &line[3..].trim_start(); // skip "ACT"
    let mut parts_iter = rest.splitn(3, ' ');
    let flags_str = parts_iter
        .next()
        .ok_or_else(|| ParseError::ActTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;
    let section_str = parts_iter
        .next()
        .ok_or_else(|| ParseError::ActTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;
    let remainder = parts_iter
        .next()
        .ok_or_else(|| ParseError::ActTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;

    let flags_val = flags_str
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;
    let section_val = section_str
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;

    // Parse first quoted string: ActionCommandID
    let (command_id, pos) = extract_quoted(remainder, 0, line_num)?;
    // Skip whitespace
    let pos = skip_ws(remainder, pos);
    // Parse second quoted string: Description
    let (description, pos) = extract_quoted(remainder, pos, line_num)?;
    // Remaining tokens are component action IDs
    let tail = remainder[pos..].trim();
    let actions: Vec<CommandId> = if tail.is_empty() {
        Vec::new()
    } else {
        tail.split_whitespace().map(parse_command_id).collect()
    };

    Ok(CustomAction {
        flags: ActionFlags::from_bits_truncate(flags_val),
        section: Section::from_raw(section_val),
        command_id,
        description,
        actions,
    })
}

/// Parse a SCR line. Format:
/// `SCR flags section ActionCommandID "ScriptDescription" Scriptname`
fn parse_scr_line(line: &str, line_num: usize) -> Result<ScriptBinding, ParseError> {
    let rest = line[3..].trim_start(); // skip "SCR"
    let mut parts_iter = rest.splitn(4, ' ');
    let flags_str = parts_iter
        .next()
        .ok_or_else(|| ParseError::ScrTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;
    let section_str = parts_iter
        .next()
        .ok_or_else(|| ParseError::ScrTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;
    let command_id = parts_iter
        .next()
        .ok_or_else(|| ParseError::ScrTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;
    let remainder = parts_iter
        .next()
        .ok_or_else(|| ParseError::ScrTooFewFields {
            line: line_num,
            content: line.to_string(),
        })?;

    let flags_val = flags_str
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;
    let section_val = section_str
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            line: line_num,
            source: e,
        })?;

    // Parse quoted description
    let (description, pos) = extract_quoted(remainder, 0, line_num)?;
    // Rest is the script filename
    let script_file = remainder[pos..].trim().to_string();

    Ok(ScriptBinding {
        flags: ScriptFlags::from_bits_truncate(flags_val),
        section: Section::from_raw(section_val),
        command_id: command_id.to_string(),
        description,
        script_file,
    })
}

fn skip_ws(s: &str, pos: usize) -> usize {
    let bytes = s.as_bytes();
    let mut p = pos;
    while p < bytes.len() && bytes[p] == b' ' {
        p += 1;
    }
    p
}

/// Parse the full contents of a `reaper-kb.ini` file.
pub fn parse(input: &str) -> Result<KeyMap, ParseError> {
    let mut entries = Vec::new();
    // Collect raw KEY lines so we can pair consecutive global-flag lines.
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end_matches('\r');
        if line.is_empty() {
            i += 1;
            continue;
        }

        if line.starts_with("KEY ") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let raw = parse_raw_key(&fields, i + 1)?;

            // Peek ahead to see if the next line is a global-flag KEY line.
            let mut global = None;
            if i + 1 < lines.len() {
                let next_line = lines[i + 1].trim_end_matches('\r');
                if next_line.starts_with("KEY ") {
                    let next_fields: Vec<&str> = next_line.split_whitespace().collect();
                    if let Ok(next_raw) = parse_raw_key(&next_fields, i + 2)
                        && is_global_section(next_raw.section)
                    {
                        // This is a global-flag line.
                        if let Ok(scope_val) = next_raw.command_or_scope.parse::<u32>() {
                            global = GlobalScope::from_raw(scope_val);
                        }
                        // Skip the global-flag line.
                        i += 1;
                    }
                }
            }

            entries.push(Entry::Key(KeyBinding {
                modifier: raw.modifier,
                key: raw.key,
                command: parse_command_id(&raw.command_or_scope),
                section: Section::from_raw(raw.section),
                global,
            }));
        } else if line.starts_with("ACT ") {
            entries.push(Entry::Action(parse_act_line(line, i + 1)?));
        } else if line.starts_with("SCR ") {
            entries.push(Entry::Script(parse_scr_line(line, i + 1)?));
        }
        // Silently skip unrecognized lines.

        i += 1;
    }

    Ok(KeyMap { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_native_command() {
        let input = "KEY 1 65 40001 0\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 1);
        match &km.entries[0] {
            Entry::Key(kb) => {
                assert_eq!(kb.modifier, 1);
                assert_eq!(kb.key, 65);
                assert!(matches!(&kb.command, CommandId::Native(40001)));
                assert_eq!(kb.section, Section::Main);
                assert!(kb.global.is_none());
            }
            _ => panic!("expected Key entry"),
        }
    }

    #[test]
    fn parse_key_named_command() {
        let input = "KEY 0 83 _SWS_SAVE 0\r\n";
        let km = parse(input).unwrap();
        match &km.entries[0] {
            Entry::Key(kb) => {
                assert!(matches!(&kb.command, CommandId::Named(s) if s == "_SWS_SAVE"));
            }
            _ => panic!("expected Key entry"),
        }
    }

    #[test]
    fn parse_key_global_binding() {
        let input = "KEY 1 65 40001 0\r\nKEY 1 65 1 102\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 1);
        match &km.entries[0] {
            Entry::Key(kb) => {
                assert_eq!(kb.global, Some(GlobalScope::Global));
            }
            _ => panic!("expected Key entry"),
        }
    }

    #[test]
    fn parse_key_global_text_fields() {
        let input = "KEY 4 66 40002 100\r\nKEY 4 66 101 103\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 1);
        match &km.entries[0] {
            Entry::Key(kb) => {
                assert_eq!(kb.global, Some(GlobalScope::GlobalTextFields));
                assert_eq!(kb.section, Section::MainAlt);
            }
            _ => panic!("expected Key entry"),
        }
    }

    #[test]
    fn parse_act_line() {
        let input = "ACT 3 0 \"_custom_action_1\" \"My Custom Action\" 40001 _SWS_SAVE 40002\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 1);
        match &km.entries[0] {
            Entry::Action(act) => {
                assert_eq!(
                    act.flags,
                    ActionFlags::CONSOLIDATE_UNDO | ActionFlags::SHOW_IN_MENU
                );
                assert_eq!(act.section, Section::Main);
                assert_eq!(act.command_id, "_custom_action_1");
                assert_eq!(act.description, "My Custom Action");
                assert_eq!(act.actions.len(), 3);
                assert!(matches!(&act.actions[0], CommandId::Native(40001)));
                assert!(matches!(&act.actions[1], CommandId::Named(s) if s == "_SWS_SAVE"));
                assert!(matches!(&act.actions[2], CommandId::Native(40002)));
            }
            _ => panic!("expected Action entry"),
        }
    }

    #[test]
    fn parse_scr_line() {
        let input = "SCR 4 0 my_script_id \"Custom: My Script\" myscript.lua\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 1);
        match &km.entries[0] {
            Entry::Script(scr) => {
                assert_eq!(scr.flags, ScriptFlags::DIALOG_IF_RUNNING);
                assert_eq!(scr.section, Section::Main);
                assert_eq!(scr.command_id, "my_script_id");
                assert_eq!(scr.description, "Custom: My Script");
                assert_eq!(scr.script_file, "myscript.lua");
            }
            _ => panic!("expected Script entry"),
        }
    }

    #[test]
    fn parse_mixed_entries() {
        let input = "\
KEY 0 65 40001 0\r\n\
ACT 2 32060 \"_my_act\" \"Do Thing\" 40001\r\n\
SCR 4 0 my_scr \"Custom: Test\" test.py\r\n\
KEY 8 32801 40005 0\r\n";
        let km = parse(input).unwrap();
        assert_eq!(km.entries.len(), 4);
        assert!(matches!(&km.entries[0], Entry::Key(_)));
        assert!(matches!(&km.entries[1], Entry::Action(_)));
        assert!(matches!(&km.entries[2], Entry::Script(_)));
        assert!(matches!(&km.entries[3], Entry::Key(_)));
    }

    #[test]
    fn parse_empty_input() {
        let km = parse("").unwrap();
        assert!(km.entries.is_empty());
    }

    #[test]
    fn parse_midi_section() {
        let input = "KEY 0 60 40001 32060\r\n";
        let km = parse(input).unwrap();
        match &km.entries[0] {
            Entry::Key(kb) => {
                assert_eq!(kb.section, Section::MidiEditor);
            }
            _ => panic!("expected Key entry"),
        }
    }
}
