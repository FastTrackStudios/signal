//! Parsing logic for REAPER menu/toolbar configuration files.
//!
//! ## File structure recap
//!
//! ```text
//! [Section Header]
//! item_0=<cmd> [title]
//! item_1=-1
//! item_2=-2 Submenu Name
//! item_3=40001 "Do Something"
//! item_4=-3
//! icon_0=play.png          ; toolbar-only
//! title=Display Name
//!
//! [Next Section]
//! ...
//! ```
//!
//! ### Special `item_N=` values
//!
//! | Value            | Meaning                            |
//! |------------------|------------------------------------|
//! | `-1`             | Separator                          |
//! | `-2 <text>`      | Sub-menu start (menu variant)      |
//! | `sub "<text>"`   | Sub-menu start (toolbar variant)   |
//! | `-3`             | Sub-menu end (menu variant)        |
//! | `endsub`         | Sub-menu end (toolbar variant)     |
//! | `-4 <text>`      | Non-clickable label                |
//! | `<cmd> [title]`  | Clickable action                   |

use std::collections::HashMap;

use thiserror::Error;

use super::types::*;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing a `reaper-menu.ini`-style file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("malformed section header on line {line}: {content}")]
    BadSectionHeader { line: usize, content: String },

    #[error("malformed item line on line {line}: {content}")]
    BadItemLine { line: usize, content: String },

    #[error("unterminated quoted string on line {line}")]
    UnterminatedQuote { line: usize },
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Strip a trailing `\r` if present (handles both LF and CRLF input).
#[inline]
fn strip_cr(s: &str) -> &str {
    s.strip_suffix('\r').unwrap_or(s)
}

/// Extract a quoted string starting at byte `start` within `s`.
/// Returns `(contents_without_quotes, byte_position_after_closing_quote)`.
fn extract_quoted(s: &str, start: usize, line_num: usize) -> Result<(String, usize), ParseError> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return Err(ParseError::UnterminatedQuote { line: line_num });
    }
    let after_open = start + 1;
    match s[after_open..].find('"') {
        Some(rel) => {
            let content = s[after_open..after_open + rel].to_string();
            Ok((content, after_open + rel + 1))
        }
        None => Err(ParseError::UnterminatedQuote { line: line_num }),
    }
}

/// Parse a title that may optionally be wrapped in double-quotes.
///
/// - `"Some Title"` → `Some Title`
/// - `Some Title`   → `Some Title`
fn parse_optional_title(s: &str, line_num: usize) -> Result<String, ParseError> {
    let s = s.trim();
    if s.starts_with('"') {
        let (content, _) = extract_quoted(s, 0, line_num)?;
        Ok(content)
    } else {
        Ok(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Per-section raw accumulator
// ---------------------------------------------------------------------------

/// Collects raw `item_N` and `icon_N` key-value pairs while building a section.
#[derive(Default)]
struct RawSection {
    header: String,
    title: Option<String>,
    /// item index → raw value string
    items: HashMap<u32, String>,
    /// icon index → raw value string
    icons: HashMap<u32, String>,
}

impl RawSection {
    fn new(header: &str) -> Self {
        RawSection {
            header: header.to_string(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Public parse entry point
// ---------------------------------------------------------------------------

/// Parse the full contents of a `reaper-menu.ini` (or `.ReaperMenu` /
/// `.ReaperMenuSet`) file.
pub fn parse(input: &str) -> Result<MenuConfig, ParseError> {
    let lines: Vec<&str> = input.lines().collect();
    let mut sections: Vec<MenuSection> = Vec::new();
    let mut current: Option<RawSection> = None;

    for (idx, raw_line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let line = strip_cr(raw_line).trim();

        if line.is_empty() {
            continue;
        }

        // Section header: `[Title]`
        if line.starts_with('[') {
            // Flush previous section
            if let Some(sec) = current.take() {
                sections.push(build_section(sec, line_num)?);
            }
            // Parse new header
            let end = line.find(']').ok_or_else(|| ParseError::BadSectionHeader {
                line: line_num,
                content: line.to_string(),
            })?;
            let header = &line[1..end];
            current = Some(RawSection::new(header));
            continue;
        }

        // Key=value lines belong to the current section (ignore if none yet)
        let Some(sec) = current.as_mut() else {
            continue;
        };

        if let Some(rest) = line.strip_prefix("title=") {
            sec.title = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("item_") {
            // `item_N=value`
            let eq = rest.find('=').ok_or_else(|| ParseError::BadItemLine {
                line: line_num,
                content: line.to_string(),
            })?;
            let idx_str = &rest[..eq];
            let value = &rest[eq + 1..];
            let item_idx = idx_str
                .parse::<u32>()
                .map_err(|_| ParseError::BadItemLine {
                    line: line_num,
                    content: line.to_string(),
                })?;
            sec.items.insert(item_idx, value.to_string());
        } else if let Some(rest) = line.strip_prefix("icon_") {
            // `icon_N=value`
            let eq = rest.find('=').ok_or_else(|| ParseError::BadItemLine {
                line: line_num,
                content: line.to_string(),
            })?;
            let idx_str = &rest[..eq];
            let value = &rest[eq + 1..];
            let icon_idx = idx_str
                .parse::<u32>()
                .map_err(|_| ParseError::BadItemLine {
                    line: line_num,
                    content: line.to_string(),
                })?;
            sec.icons.insert(icon_idx, value.trim().to_string());
        }
        // Any other keys (unknown) are silently ignored for forward-compatibility.
    }

    // Flush the last section
    if let Some(sec) = current.take() {
        sections.push(build_section(sec, lines.len())?);
    }

    Ok(MenuConfig { sections })
}

// ---------------------------------------------------------------------------
// Build a MenuSection from accumulated raw data
// ---------------------------------------------------------------------------

fn build_section(raw: RawSection, line_hint: usize) -> Result<MenuSection, ParseError> {
    let id = MenuSectionId::from_header(&raw.header);

    // Determine the maximum sequential item index present.
    let max_idx = raw.items.keys().copied().max().unwrap_or(0);

    let mut items = Vec::with_capacity(raw.items.len());

    // Items must be emitted in sequential index order (0, 1, 2, …).
    // REAPER stops reading a section once a gap appears, so we honour that
    // by iterating 0..=max_idx and stopping on the first missing index.
    let mut n = 0u32;
    loop {
        if n > max_idx {
            break;
        }
        match raw.items.get(&n) {
            None => break, // gap — stop
            Some(value) => {
                let icon = raw.icons.get(&n).map(|s| IconSpec::from_token(s));
                let item = parse_item_value(value.trim(), icon, line_hint)?;
                items.push(item);
            }
        }
        n += 1;
    }

    Ok(MenuSection {
        id,
        title: raw.title,
        items,
    })
}

// ---------------------------------------------------------------------------
// Parse an individual item_N value
// ---------------------------------------------------------------------------

/// Parse the right-hand side of an `item_N=` assignment into a `MenuItem`.
///
/// `icon` is the corresponding `icon_N=` value if present (toolbar sections).
fn parse_item_value(
    value: &str,
    icon: Option<IconSpec>,
    line_hint: usize,
) -> Result<MenuItem, ParseError> {
    if value.is_empty() {
        return Err(ParseError::BadItemLine {
            line: line_hint,
            content: value.to_string(),
        });
    }

    // ---- Separator ----
    if value == "-1" {
        return Ok(MenuItem::Separator);
    }

    // ---- Sub-menu end (menu variant: -3) ----
    if value == "-3" {
        return Ok(MenuItem::SubMenuEnd);
    }

    // ---- Sub-menu end (toolbar variant: endsub) ----
    if value.eq_ignore_ascii_case("endsub") {
        return Ok(MenuItem::SubMenuEnd);
    }

    // ---- Sub-menu start (menu variant: -2 <title>) ----
    if let Some(rest) = value.strip_prefix("-2") {
        let title = parse_optional_title(rest, line_hint)?;
        return Ok(MenuItem::SubMenuStart { title });
    }

    // ---- Sub-menu start (toolbar variant: sub "<title>") ----
    if let Some(rest) = value.strip_prefix("sub ").or_else(|| {
        // Case-insensitive match for "sub"
        if value.len() >= 4 && value[..3].eq_ignore_ascii_case("sub") && value.as_bytes()[3] == b' '
        {
            Some(&value[4..])
        } else {
            None
        }
    }) {
        let title = parse_optional_title(rest, line_hint)?;
        return Ok(MenuItem::SubMenuStart { title });
    }

    // ---- Non-clickable label (-4 <text>) ----
    if let Some(rest) = value.strip_prefix("-4") {
        let text = rest.trim().to_string();
        return Ok(MenuItem::Label { text });
    }

    // ---- Clickable action ----
    parse_action_item(value, icon, line_hint)
}

/// Parse a regular action item line value: `[flags] <cmd> [title]`
///
/// REAPER writes toolbar items as `<flags> <cmd> [title]` (e.g. `0 40001 Play`)
/// and menu items as `<cmd> [title]` (e.g. `40001 "Play"` or `_SWS_SAVE`).
///
/// To disambiguate: if the first token is a pure decimal integer *and* is not
/// a valid action ID by itself (i.e. there is a second non-title token that
/// looks like a command ID), we treat the first as flags.  In practice REAPER
/// always writes `0` flags for most items, so a leading `0` is the hint.
fn parse_action_item(
    value: &str,
    icon: Option<IconSpec>,
    line_hint: usize,
) -> Result<MenuItem, ParseError> {
    // Split into up to three whitespace-separated tokens:
    //   token0  [token1  [rest]]
    let (token0, after0) = next_token(value);
    if token0.is_empty() {
        return Err(ParseError::BadItemLine {
            line: line_hint,
            content: value.to_string(),
        });
    }

    // Check if token0 is a plain decimal integer (flags field) vs a command ID.
    // Heuristic: if token0 is a number that is NOT a known big command ID and
    // the next token also looks like a number or _-name, token0 is flags.
    let (flags, cmd_token, title_rest) = if looks_like_flags(token0, after0.trim_start()) {
        // toolbar format: `<flags> <cmd> [title]`
        let (cmd, after_cmd) = next_token(after0.trim_start());
        (
            token0.parse::<u32>().unwrap_or(0),
            cmd,
            after_cmd.trim_start(),
        )
    } else {
        // menu format: `<cmd> [title]`
        (0u32, token0, after0.trim_start())
    };

    if cmd_token.is_empty() {
        return Err(ParseError::BadItemLine {
            line: line_hint,
            content: value.to_string(),
        });
    }

    let command = CommandId::from_token(cmd_token);

    let title = if title_rest.is_empty() {
        None
    } else {
        Some(parse_optional_title(title_rest, line_hint)?)
    };

    Ok(MenuItem::Action(ActionItem {
        command,
        title,
        icon,
        flags,
    }))
}

// ---------------------------------------------------------------------------
// Token helpers
// ---------------------------------------------------------------------------

/// Return the first whitespace-delimited token and the remainder of the string.
fn next_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&s[..pos], &s[pos..]),
        None => (s, ""),
    }
}

/// Heuristic: `token0` is a flags field (rather than a command ID) when:
///  1. It parses as a small non-negative decimal integer (≤ 255), AND
///  2. The immediately following token also looks like a command ID (number or
///     `_`-prefixed name).
fn looks_like_flags(token0: &str, next: &str) -> bool {
    let Ok(v) = token0.parse::<u32>() else {
        return false;
    };
    if v > 255 {
        return false;
    }
    if next.is_empty() {
        return false;
    }
    let (next_tok, _) = next_token(next);
    // The next token should itself be a command ID (number or _-name), not a
    // quoted title string.
    !next_tok.starts_with('"') && !next_tok.is_empty()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let cfg = parse("").unwrap();
        assert!(cfg.sections.is_empty());
    }

    #[test]
    fn parse_single_section_header_only() {
        let input = "[Main toolbar]\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.sections.len(), 1);
        assert_eq!(cfg.sections[0].id, MenuSectionId::MainToolbar);
        assert!(cfg.sections[0].items.is_empty());
    }

    #[test]
    fn parse_separator() {
        let input = "[Main toolbar]\nitem_0=-1\n";
        let cfg = parse(input).unwrap();
        assert!(matches!(cfg.sections[0].items[0], MenuItem::Separator));
    }

    #[test]
    fn parse_submenu_menu_style() {
        let input = "[Main edit]\nitem_0=-2 Submenu\nitem_1=40001 \"Do It\"\nitem_2=-3\n";
        let cfg = parse(input).unwrap();
        assert!(matches!(
            &cfg.sections[0].items[0],
            MenuItem::SubMenuStart { title } if title == "Submenu"
        ));
        assert!(matches!(&cfg.sections[0].items[2], MenuItem::SubMenuEnd));
    }

    #[test]
    fn parse_submenu_toolbar_style() {
        let input = "[Main toolbar]\nitem_0=sub \"My Sub\"\nitem_1=40001\nitem_2=endsub\n";
        let cfg = parse(input).unwrap();
        assert!(matches!(
            &cfg.sections[0].items[0],
            MenuItem::SubMenuStart { title } if title == "My Sub"
        ));
        assert!(matches!(&cfg.sections[0].items[2], MenuItem::SubMenuEnd));
    }

    #[test]
    fn parse_label() {
        let input = "[Main edit]\nitem_0=-4 Section Label\n";
        let cfg = parse(input).unwrap();
        assert!(matches!(
            &cfg.sections[0].items[0],
            MenuItem::Label { text } if text == "Section Label"
        ));
    }

    #[test]
    fn parse_action_menu_style() {
        let input = "[Main file]\nitem_0=40001 \"New Project\"\n";
        let cfg = parse(input).unwrap();
        match &cfg.sections[0].items[0] {
            MenuItem::Action(a) => {
                assert!(matches!(a.command, CommandId::Native(40001)));
                assert_eq!(a.title.as_deref(), Some("New Project"));
                assert_eq!(a.flags, 0);
                assert!(a.icon.is_none());
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn parse_action_named_command() {
        let input = "[Main file]\nitem_0=_SWS_SAVE \"SWS Save\"\n";
        let cfg = parse(input).unwrap();
        match &cfg.sections[0].items[0] {
            MenuItem::Action(a) => {
                assert!(matches!(&a.command, CommandId::Named(s) if s == "_SWS_SAVE"));
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn parse_action_toolbar_with_icon() {
        let input = "[Main toolbar]\nicon_0=play.png\nitem_0=0 40044 Play\n";
        let cfg = parse(input).unwrap();
        match &cfg.sections[0].items[0] {
            MenuItem::Action(a) => {
                assert!(matches!(a.command, CommandId::Native(40044)));
                assert_eq!(a.flags, 0);
                assert_eq!(a.icon, Some(IconSpec::File("play.png".to_string())));
                assert_eq!(a.title.as_deref(), Some("Play"));
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn parse_action_toolbar_text_icon() {
        let input = "[Main toolbar]\nicon_0=text\nitem_0=0 40001 Record\n";
        let cfg = parse(input).unwrap();
        match &cfg.sections[0].items[0] {
            MenuItem::Action(a) => {
                assert_eq!(a.icon, Some(IconSpec::Text));
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn parse_multiple_sections() {
        let input = "[Main toolbar]\nitem_0=-1\n\n[Main file]\nitem_0=40001\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.sections.len(), 2);
        assert_eq!(cfg.sections[0].id, MenuSectionId::MainToolbar);
        assert_eq!(cfg.sections[1].id, MenuSectionId::MainFile);
    }

    #[test]
    fn parse_title_line() {
        let input = "[Main toolbar]\ntitle=My Custom Toolbar\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.sections[0].title.as_deref(), Some("My Custom Toolbar"));
    }

    #[test]
    fn parse_item_gap_stops_at_gap() {
        // item_0 and item_2 present, item_1 missing — only item_0 should appear
        let input = "[Main toolbar]\nitem_0=40001\nitem_2=40002\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.sections[0].items.len(), 1);
    }

    #[test]
    fn parse_unknown_section() {
        let input = "[Custom section]\nitem_0=40001\n";
        let cfg = parse(input).unwrap();
        assert!(matches!(
            &cfg.sections[0].id,
            MenuSectionId::Other(s) if s == "Custom section"
        ));
    }

    #[test]
    fn parse_crlf_line_endings() {
        let input = "[Main toolbar]\r\nitem_0=-1\r\n";
        let cfg = parse(input).unwrap();
        assert_eq!(cfg.sections.len(), 1);
        assert!(matches!(cfg.sections[0].items[0], MenuItem::Separator));
    }

    #[test]
    fn parse_floating_toolbars() {
        for n in 1u8..=16 {
            let input = format!("[Floating toolbar {}]\nitem_0=-1\n", n);
            let cfg = parse(&input).unwrap();
            assert_ne!(cfg.sections[0].id, MenuSectionId::Other(String::new()));
        }
    }
}
