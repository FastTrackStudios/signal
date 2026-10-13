//! Parsing logic for REAPER render preset (`reaper-render.ini`) files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing a `reaper-render.ini` file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("not enough fields on {kind} line {line}: {content}")]
    TooFewFields {
        kind: &'static str,
        line: usize,
        content: String,
    },

    #[error("invalid integer on line {line}: {source}")]
    InvalidInt {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("invalid float on line {line}: {source}")]
    InvalidFloat {
        line: usize,
        #[source]
        source: std::num::ParseFloatError,
    },

    #[error("unterminated block <{kind}> starting at line {line}")]
    UnterminatedBlock { kind: String, line: usize },

    #[error("unterminated quoted string on line {line}")]
    UnterminatedQuote { line: usize },

    #[error("unexpected content inside block at line {line}: {content}")]
    UnexpectedBlockContent { line: usize, content: String },
}

// ---------------------------------------------------------------------------
// Token helpers
// ---------------------------------------------------------------------------

/// Parse a preset name token from the front of `s`.
///
/// REAPER quotes names that contain spaces: `"my preset"`. Unquoted names are
/// terminated by the next whitespace character.
///
/// Returns `(name, rest_of_line)`.
fn parse_name(s: &str, line_num: usize) -> Result<(String, &str), ParseError> {
    let s = s.trim_start();
    if let Some(after_open) = s.strip_prefix('"') {
        // Quoted name.
        if let Some(close) = after_open.find('"') {
            let name = after_open[..close].to_string();
            let rest = after_open[close + 1..].trim_start();
            Ok((name, rest))
        } else {
            Err(ParseError::UnterminatedQuote { line: line_num })
        }
    } else {
        // Unquoted: everything up to the next whitespace.
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        let name = s[..end].to_string();
        let rest = s[end..].trim_start();
        Ok((name, rest))
    }
}

/// Parse a quoted-or-bare string token (used for filenames / directories).
///
/// - If the remaining text starts with `"`, read until the matching `"`.
/// - Otherwise consume all remaining text as a single token (no embedded spaces).
///
/// Returns `(value, rest_of_line)`.
fn parse_string_token(s: &str, line_num: usize) -> Result<(String, &str), ParseError> {
    let s = s.trim_start();
    if let Some(after_open) = s.strip_prefix('"') {
        if let Some(close) = after_open.find('"') {
            let value = after_open[..close].to_string();
            let rest = after_open[close + 1..].trim_start();
            Ok((value, rest))
        } else {
            Err(ParseError::UnterminatedQuote { line: line_num })
        }
    } else {
        // Consume until whitespace.
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        let value = s[..end].to_string();
        let rest = s[end..].trim_start();
        Ok((value, rest))
    }
}

/// Split `s` on whitespace, returning the first token and the remainder.
fn next_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    Some((&s[..end], s[end..].trim_start()))
}

/// Parse an integer from the next whitespace-delimited token.
fn parse_u32(s: &str, line_num: usize) -> Result<(u32, &str), ParseError> {
    let (tok, rest) = next_token(s).ok_or_else(|| ParseError::TooFewFields {
        kind: "integer",
        line: line_num,
        content: s.to_string(),
    })?;
    let n = tok.parse::<u32>().map_err(|e| ParseError::InvalidInt {
        line: line_num,
        source: e,
    })?;
    Ok((n, rest))
}

/// Parse a float from the next whitespace-delimited token.
fn parse_f64(s: &str, line_num: usize) -> Result<(f64, &str), ParseError> {
    let (tok, rest) = next_token(s).ok_or_else(|| ParseError::TooFewFields {
        kind: "float",
        line: line_num,
        content: s.to_string(),
    })?;
    let f = tok.parse::<f64>().map_err(|e| ParseError::InvalidFloat {
        line: line_num,
        source: e,
    })?;
    Ok((f, rest))
}

// ---------------------------------------------------------------------------
// Block parsers
// ---------------------------------------------------------------------------

/// Parse a `<RENDERPRESET ...>` block.
///
/// `header` is the text after `<RENDERPRESET ` on the opening line.
/// `body_line` is the single indented line inside the block.
/// `line_num` is the 1-based line number of the opening `<RENDERPRESET` line.
fn parse_renderpreset_block(
    header: &str,
    body_line: &str,
    line_num: usize,
) -> Result<RenderPreset, ParseError> {
    let (name, rest) = parse_name(header, line_num)?;
    let (sample_rate, rest) = parse_u32(rest, line_num)?;
    let (channels, rest) = parse_u32(rest, line_num)?;
    let (offline_mode, rest) = parse_u32(rest, line_num)?;
    let (use_project_sample_rate, rest) = parse_u32(rest, line_num)?;
    let (resample_mode, rest) = parse_u32(rest, line_num)?;
    let (various_checkboxes, rest) = parse_u32(rest, line_num)?;
    let (various_checkboxes2, _rest) = parse_u32(rest, line_num)?;

    let render_cfg = body_line.trim().to_string();

    Ok(RenderPreset {
        name,
        sample_rate,
        channels,
        offline_mode,
        use_project_sample_rate,
        resample_mode,
        various_checkboxes,
        various_checkboxes2,
        render_cfg,
    })
}

/// Parse a `<RENDERPRESET2 ...>` block.
fn parse_renderpreset2_block(
    header: &str,
    body_line: &str,
    line_num: usize,
) -> Result<RenderPreset2, ParseError> {
    let (name, _rest) = parse_name(header, line_num)?;
    let render_cfg = body_line.trim().to_string();
    Ok(RenderPreset2 { name, render_cfg })
}

// ---------------------------------------------------------------------------
// Flat-line parsers
// ---------------------------------------------------------------------------

/// Parse a `RENDERPRESET_OUTPUT ...` line.
///
/// Format:
/// ```text
/// RENDERPRESET_OUTPUT presetname bounds start end source unknown filename tail dir tail_ms
/// ```
fn parse_renderpreset_output(
    rest: &str,
    line_num: usize,
) -> Result<RenderPresetOutput, ParseError> {
    let (name, rest) = parse_name(rest, line_num)?;
    let (bounds, rest) = parse_u32(rest, line_num)?;
    let (start, rest) = parse_f64(rest, line_num)?;
    let (end, rest) = parse_f64(rest, line_num)?;
    let (source, rest) = parse_u32(rest, line_num)?;
    let (unknown, rest) = parse_u32(rest, line_num)?;
    let (filename, rest) = parse_string_token(rest, line_num)?;
    let (tail, rest) = parse_u32(rest, line_num)?;
    let (directory, rest) = parse_string_token(rest, line_num)?;
    let (tail_ms, _rest) = parse_u32(rest, line_num)?;

    Ok(RenderPresetOutput {
        name,
        bounds,
        start,
        end,
        source,
        unknown,
        filename,
        tail,
        directory,
        tail_ms,
    })
}

/// Parse a `RENDERPRESET_EXT ...` line.
///
/// Format:
/// ```text
/// RENDERPRESET_EXT presetname normalize_mode normalize_target brickwall_target
///   fadein_length fadeout_length fadein_shape fadeout_shape
///   trim_lead trim_trail pad_start pad_end
/// ```
fn parse_renderpreset_ext(rest: &str, line_num: usize) -> Result<RenderPresetExt, ParseError> {
    let (name, rest) = parse_name(rest, line_num)?;
    let (normalize_mode, rest) = parse_u32(rest, line_num)?;
    let (normalize_target, rest) = parse_f64(rest, line_num)?;
    let (brickwall_target, rest) = parse_f64(rest, line_num)?;
    let (fadein_length, rest) = parse_f64(rest, line_num)?;
    let (fadeout_length, rest) = parse_f64(rest, line_num)?;
    let (fadein_shape, rest) = parse_u32(rest, line_num)?;
    let (fadeout_shape, rest) = parse_u32(rest, line_num)?;
    let (trim_lead, rest) = parse_f64(rest, line_num)?;
    let (trim_trail, rest) = parse_f64(rest, line_num)?;
    let (pad_start, rest) = parse_f64(rest, line_num)?;
    let (pad_end, _rest) = parse_f64(rest, line_num)?;

    Ok(RenderPresetExt {
        name,
        normalize_mode,
        normalize_target,
        brickwall_target,
        fadein_length,
        fadeout_length,
        fadein_shape,
        fadeout_shape,
        trim_lead,
        trim_trail,
        pad_start,
        pad_end,
    })
}

// ---------------------------------------------------------------------------
// Top-level parser
// ---------------------------------------------------------------------------

/// Parse the full contents of a `reaper-render.ini` file.
pub fn parse(input: &str) -> Result<RenderPresetFile, ParseError> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let raw_line = lines[i];
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        let line_num = i + 1;

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(header) = trimmed.strip_prefix("<RENDERPRESET2 ") {
            // Multi-line block: read body line and closing `>`.
            let block_start = line_num;
            i += 1;
            let body_line = if i < lines.len() {
                lines[i].trim_end_matches('\r')
            } else {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET2".to_string(),
                    line: block_start,
                });
            };
            i += 1;
            // Expect closing `>`.
            if i >= lines.len() || lines[i].trim_end_matches('\r').trim() != ">" {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET2".to_string(),
                    line: block_start,
                });
            }
            let preset2 = parse_renderpreset2_block(header, body_line, block_start)?;
            entries.push(RenderPresetEntry::Preset2(preset2));
        } else if let Some(header) = trimmed.strip_prefix("<RENDERPRESET ") {
            // Multi-line block: read body line and closing `>`.
            let block_start = line_num;
            i += 1;
            let body_line = if i < lines.len() {
                lines[i].trim_end_matches('\r')
            } else {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET".to_string(),
                    line: block_start,
                });
            };
            i += 1;
            // Expect closing `>`.
            if i >= lines.len() || lines[i].trim_end_matches('\r').trim() != ">" {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET".to_string(),
                    line: block_start,
                });
            }
            let preset = parse_renderpreset_block(header, body_line, block_start)?;
            entries.push(RenderPresetEntry::Preset(preset));
        } else if let Some(rest) = trimmed.strip_prefix("RENDERPRESET_OUTPUT ") {
            let output = parse_renderpreset_output(rest, line_num)?;
            entries.push(RenderPresetEntry::Output(output));
        } else if let Some(rest) = trimmed.strip_prefix("RENDERPRESET_EXT ") {
            let ext = parse_renderpreset_ext(rest, line_num)?;
            entries.push(RenderPresetEntry::Ext(ext));
        }
        // Silently skip unrecognized lines.

        i += 1;
    }

    Ok(RenderPresetFile { entries })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- RENDERPRESET block --------------------------------------------------

    #[test]
    fn parse_renderpreset_unquoted_name() {
        let input = "<RENDERPRESET MyPreset 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n>\r\n";
        let f = parse(input).unwrap();
        assert_eq!(f.entries.len(), 1);
        match &f.entries[0] {
            RenderPresetEntry::Preset(p) => {
                assert_eq!(p.name, "MyPreset");
                assert_eq!(p.sample_rate, 44100);
                assert_eq!(p.channels, 2);
                assert_eq!(p.offline_mode, 0);
                assert_eq!(p.use_project_sample_rate, 0);
                assert_eq!(p.resample_mode, 3);
                assert_eq!(p.various_checkboxes, 0);
                assert_eq!(p.various_checkboxes2, 0);
                assert_eq!(p.render_cfg, "SGFsbG8=");
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    #[test]
    fn parse_renderpreset_quoted_name() {
        let input = "<RENDERPRESET \"My Preset\" 48000 1 2 1 0 5 4\r\n  YWJj\r\n>\r\n";
        let f = parse(input).unwrap();
        match &f.entries[0] {
            RenderPresetEntry::Preset(p) => {
                assert_eq!(p.name, "My Preset");
                assert_eq!(p.sample_rate, 48000);
                assert_eq!(p.channels, 1);
                assert_eq!(p.offline_mode, 2);
                assert_eq!(p.use_project_sample_rate, 1);
                assert_eq!(p.resample_mode, 0);
                assert_eq!(p.various_checkboxes, 5);
                assert_eq!(p.various_checkboxes2, 4);
                assert_eq!(p.render_cfg, "YWJj");
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    // --- RENDERPRESET2 block -------------------------------------------------

    #[test]
    fn parse_renderpreset2() {
        let input = "<RENDERPRESET2 Secondary\r\n  bXAz\r\n>\r\n";
        let f = parse(input).unwrap();
        assert_eq!(f.entries.len(), 1);
        match &f.entries[0] {
            RenderPresetEntry::Preset2(p) => {
                assert_eq!(p.name, "Secondary");
                assert_eq!(p.render_cfg, "bXAz");
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    #[test]
    fn parse_renderpreset2_quoted_name() {
        let input = "<RENDERPRESET2 \"My Secondary\"\r\n  bXAz\r\n>\r\n";
        let f = parse(input).unwrap();
        match &f.entries[0] {
            RenderPresetEntry::Preset2(p) => {
                assert_eq!(p.name, "My Secondary");
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    // --- RENDERPRESET_OUTPUT -------------------------------------------------

    #[test]
    fn parse_output_basic() {
        let input = "RENDERPRESET_OUTPUT MyPreset 1 0 0 0 0 $project 0 \"\" 1000\r\n";
        let f = parse(input).unwrap();
        assert_eq!(f.entries.len(), 1);
        match &f.entries[0] {
            RenderPresetEntry::Output(o) => {
                assert_eq!(o.name, "MyPreset");
                assert_eq!(o.bounds, 1);
                assert_eq!(o.start, 0.0);
                assert_eq!(o.end, 0.0);
                assert_eq!(o.source, 0);
                assert_eq!(o.unknown, 0);
                assert_eq!(o.filename, "$project");
                assert_eq!(o.tail, 0);
                assert_eq!(o.directory, "");
                assert_eq!(o.tail_ms, 1000);
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    #[test]
    fn parse_output_quoted_filename_and_dir() {
        let input = "RENDERPRESET_OUTPUT \"My Preset\" 0 1.5 10.25 1 0 \"my output\" 1 \"/home/user/audio\" 500\r\n";
        let f = parse(input).unwrap();
        match &f.entries[0] {
            RenderPresetEntry::Output(o) => {
                assert_eq!(o.name, "My Preset");
                assert_eq!(o.bounds, 0);
                assert!((o.start - 1.5).abs() < 1e-10);
                assert!((o.end - 10.25).abs() < 1e-10);
                assert_eq!(o.source, 1);
                assert_eq!(o.filename, "my output");
                assert_eq!(o.tail, 1);
                assert_eq!(o.directory, "/home/user/audio");
                assert_eq!(o.tail_ms, 500);
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    // --- RENDERPRESET_EXT ----------------------------------------------------

    #[test]
    fn parse_ext_basic() {
        let input = "RENDERPRESET_EXT MyPreset 0 1 1 0 0 0 0 0.031623 0.031623 0 0\r\n";
        let f = parse(input).unwrap();
        assert_eq!(f.entries.len(), 1);
        match &f.entries[0] {
            RenderPresetEntry::Ext(e) => {
                assert_eq!(e.name, "MyPreset");
                assert_eq!(e.normalize_mode, 0);
                assert!((e.normalize_target - 1.0).abs() < 1e-10);
                assert!((e.brickwall_target - 1.0).abs() < 1e-10);
                assert_eq!(e.fadein_length, 0.0);
                assert_eq!(e.fadeout_length, 0.0);
                assert_eq!(e.fadein_shape, 0);
                assert_eq!(e.fadeout_shape, 0);
                assert!((e.trim_lead - 0.031623).abs() < 1e-6);
                assert!((e.trim_trail - 0.031623).abs() < 1e-6);
                assert_eq!(e.pad_start, 0.0);
                assert_eq!(e.pad_end, 0.0);
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    #[test]
    fn parse_ext_quoted_name() {
        let input =
            "RENDERPRESET_EXT \"My Ext\" 65 0.707 0.891 0.01 0.02 2 3 0.001 0.002 0.005 0.01\r\n";
        let f = parse(input).unwrap();
        match &f.entries[0] {
            RenderPresetEntry::Ext(e) => {
                assert_eq!(e.name, "My Ext");
                assert_eq!(e.normalize_mode, 65);
                assert!((e.fadein_length - 0.01).abs() < 1e-10);
                assert_eq!(e.fadein_shape, 2);
                assert_eq!(e.fadeout_shape, 3);
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    // --- Mixed / multi-entry -------------------------------------------------

    #[test]
    fn parse_mixed_entries() {
        let input = concat!(
            "<RENDERPRESET Stereo 44100 2 0 0 3 0 0\r\n",
            "  SGFsbG8=\r\n",
            ">\r\n",
            "<RENDERPRESET2 Stereo\r\n",
            "  bXAz\r\n",
            ">\r\n",
            "RENDERPRESET_OUTPUT Stereo 1 0 0 0 0 $project 0 \"\" 1000\r\n",
            "RENDERPRESET_EXT Stereo 0 1 1 0 0 0 0 0.031623 0.031623 0 0\r\n",
        );
        let f = parse(input).unwrap();
        assert_eq!(f.entries.len(), 4);
        assert!(matches!(f.entries[0], RenderPresetEntry::Preset(_)));
        assert!(matches!(f.entries[1], RenderPresetEntry::Preset2(_)));
        assert!(matches!(f.entries[2], RenderPresetEntry::Output(_)));
        assert!(matches!(f.entries[3], RenderPresetEntry::Ext(_)));
    }

    #[test]
    fn parse_empty_input() {
        let f = parse("").unwrap();
        assert!(f.entries.is_empty());
    }

    #[test]
    fn parse_ignores_unknown_lines() {
        let input = "# This is a comment or unknown line\r\nSomeUnknownKey value\r\n";
        let f = parse(input).unwrap();
        assert!(f.entries.is_empty());
    }

    // --- Error cases ---------------------------------------------------------

    #[test]
    fn parse_unterminated_block() {
        let input = "<RENDERPRESET MyPreset 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n";
        let err = parse(input).unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedBlock { .. }));
    }

    #[test]
    fn parse_unterminated_quote_in_name() {
        let input = "<RENDERPRESET \"Unterminated 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n>\r\n";
        let err = parse(input).unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedQuote { .. }));
    }

    #[test]
    fn parse_invalid_integer() {
        let input = "<RENDERPRESET MyPreset notanint 2 0 0 3 0 0\r\n  SGFsbG8=\r\n>\r\n";
        let err = parse(input).unwrap_err();
        assert!(matches!(err, ParseError::InvalidInt { .. }));
    }
}
