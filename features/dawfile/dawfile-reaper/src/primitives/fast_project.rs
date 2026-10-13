//! Fast line-scanned project parser for large RPP files.
//!
//! This parser keeps token semantics from `parse_token_line` while replacing
//! recursive nom block walking with a single pass over lines and a block stack.

use super::block::{BlockType, RppBlock, RppBlockContent};
use super::project::RppProject;
use super::token::{Token, parse_token_line};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

fn parse_hex_u8(s: &str) -> Option<u8> {
    u8::from_str_radix(s, 16).ok()
}

fn numeric_shape_flags(raw: &str) -> (bool, bool) {
    // (has_float_marker, has_comma_decimal)
    let mut has_float_marker = false;
    let mut has_comma = false;
    for b in raw.as_bytes() {
        match *b {
            b'.' => has_float_marker = true,
            b',' => {
                has_float_marker = true;
                has_comma = true;
            }
            b'e' | b'E' => has_float_marker = true,
            _ => {}
        }
    }
    (has_float_marker, has_comma)
}

fn fast_classify_token(raw: &str) -> Token {
    if let Some(hex) = raw.strip_prefix("0x")
        && let Ok(v) = u64::from_str_radix(hex, 16)
    {
        return Token::HexInteger(v);
    }

    let (has_float_marker, has_comma_decimal) = numeric_shape_flags(raw);
    if !has_float_marker && let Ok(v) = raw.parse::<i64>() {
        return Token::Integer(v);
    }

    if has_comma_decimal {
        let normalized = raw.replace(',', ".");
        if let Ok(v) = normalized.parse::<f64>() {
            return Token::Float(v);
        }
    } else if let Ok(v) = raw.parse::<f64>() {
        return Token::Float(v);
    }

    Token::Identifier(raw.to_string())
}

fn tokenize_line(line: &str) -> Result<Vec<Token>, String> {
    if line.contains('"')
        || line.contains('\'')
        || line.contains('`')
        || line.contains('#')
        || line.contains(';')
    {
        return parse_token_line(line)
            .map(|(_, t)| t)
            .map_err(|e| format!("{e:?}"));
    }

    let mut parts = line.split_whitespace();
    let Some(first) = parts.next() else {
        return Err("empty token line".to_string());
    };

    // Fast MIDI event classification: E/e <time> <hex> <hex> <hex> [extra...]
    if first == "E" || first == "e" {
        let p1 = parts.next();
        let p2 = parts.next();
        let p3 = parts.next();
        let p4 = parts.next();
        if let (Ok(time), Some(b1), Some(b2), Some(b3)) = (
            p1.unwrap_or_default().parse::<i64>(),
            parse_hex_u8(p2.unwrap_or_default()),
            parse_hex_u8(p3.unwrap_or_default()),
            parse_hex_u8(p4.unwrap_or_default()),
        ) {
            let mut out = vec![Token::MidiEvent {
                time,
                bytes: [b1, b2, b3],
            }];
            for part in parts {
                out.push(fast_classify_token(part));
            }
            return Ok(out);
        }
    }

    let mut out = Vec::with_capacity(8);
    out.push(fast_classify_token(first));
    out.extend(parts.map(fast_classify_token));
    Ok(out)
}

fn parse_block_header(block_line: &str) -> Result<(String, Vec<Token>), String> {
    if block_line.is_empty() {
        return Err("empty block header".to_string());
    }

    if !block_line.contains(' ') && !block_line.contains('\t') {
        return Ok((block_line.to_string(), Vec::new()));
    }

    if block_line.contains('"')
        || block_line.contains('\'')
        || block_line.contains('`')
        || block_line.contains('#')
        || block_line.contains(';')
    {
        let tokens = tokenize_line(block_line)?;
        if tokens.is_empty() {
            return Err("empty block header".to_string());
        }
        let mut iter = tokens.into_iter();
        let first = iter
            .next()
            .ok_or_else(|| "empty block header".to_string())?;
        let name = first.to_string();
        let params: Vec<Token> = iter.collect();
        return Ok((name, params));
    }

    let mut parts = block_line.split_whitespace();
    let Some(name) = parts.next() else {
        return Err("empty block header".to_string());
    };
    let mut params = Vec::with_capacity(4);
    params.extend(parts.map(fast_classify_token));
    Ok((name.to_string(), params))
}

fn requires_structured_tokens(block_name: &str) -> bool {
    matches!(
        block_name,
        "REAPER_PROJECT"
            | "TEMPOENVEX"
            | "VOLENV"
            | "VOLENV2"
            | "VOLENV3"
            | "PANENV"
            | "PANENV2"
            | "MUTEENV"
            | "MUTEENV2"
            | "WIDTHENV"
            | "WIDTHENV2"
            | "AUXVOLENV"
            | "AUXPANENV"
            | "AUXMUTEENV"
            | "PARMENV"
    )
}

#[inline]
fn is_skippable_trimmed(line: &str) -> bool {
    line.is_empty() || line.starts_with("//") || line.starts_with('#') || line.starts_with(';')
}

fn parse_single_block_lines(lines: &[&str], base_line_no: usize) -> Result<RppBlock, String> {
    let mut idx = 0usize;
    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if !is_skippable_trimmed(trimmed) {
            break;
        }
        idx += 1;
    }
    if idx >= lines.len() {
        return Err("empty block".to_string());
    }
    let first = lines[idx].trim();
    if !first.starts_with('<') {
        return Err(format!(
            "line {}: expected block header, got: {first}",
            base_line_no + idx
        ));
    }

    let (name, params) = parse_block_header(&first[1..])
        .map_err(|e| format!("line {}: invalid block header: {e}", base_line_no + idx))?;
    let mut stack = vec![RppBlock {
        block_type: BlockType::parse(&name),
        name,
        params,
        children: Vec::new(),
    }];
    idx += 1;

    while idx < lines.len() {
        let line_no = base_line_no + idx;
        let line = lines[idx].trim();
        idx += 1;

        if is_skippable_trimmed(line) {
            continue;
        }

        if line == ">" {
            let Some(block) = stack.pop() else {
                return Err(format!("line {line_no}: unexpected block close"));
            };
            if let Some(parent) = stack.last_mut() {
                parent.children.push(RppBlockContent::Block(block));
            } else {
                return Ok(block);
            }
            continue;
        }

        if let Some(block_line) = line.strip_prefix('<') {
            let (name, params) = parse_block_header(block_line)
                .map_err(|e| format!("line {line_no}: invalid block header: {e}"))?;
            stack.push(RppBlock {
                block_type: BlockType::parse(&name),
                name,
                params,
                children: Vec::new(),
            });
            continue;
        }

        let parse_structured = stack
            .last()
            .map(|b| requires_structured_tokens(&b.name))
            .unwrap_or(true);
        let tokens = if parse_structured {
            tokenize_line(line).map_err(|e| format!("line {line_no}: invalid content line: {e}"))?
        } else {
            Vec::new()
        };
        if let Some(parent) = stack.last_mut() {
            if parse_structured {
                parent.children.push(RppBlockContent::Content(tokens));
            } else {
                parent
                    .children
                    .push(RppBlockContent::RawLine(line.to_string().into_boxed_str()));
            }
        } else {
            return Err(format!("line {line_no}: missing parent block"));
        }
    }

    Err("unclosed block at EOF".to_string())
}

/// Parse a complete RPP project using a fast line scanner.
///
/// Parallel top-level block parsing is available behind:
/// `RPP_FAST_TOP_BLOCK_PARALLEL` with values:
/// - `off`/`0` (default): single-pass parser
/// - `on`/`1`: force top-level parallel parser
/// - `auto`: parallel parser with conservative enable criteria
pub fn parse_rpp_fast(content: &str) -> Result<RppProject, String> {
    match std::env::var("RPP_FAST_TOP_BLOCK_PARALLEL")
        .ok()
        .as_deref()
        .map(|v| v.to_ascii_lowercase())
    {
        Some(v) if v == "1" || v == "on" || v == "true" => parse_rpp_fast_parallel(content, true),
        // Keep auto conservative until fixture-class wins are established.
        Some(v) if v == "auto" => parse_rpp_fast_single(content),
        _ => parse_rpp_fast_single(content),
    }
}

fn parse_rpp_fast_single(content: &str) -> Result<RppProject, String> {
    let mut lines = content.lines();
    let header_line = lines
        .by_ref()
        .find_map(|line| {
            let t = line.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.trim_start_matches('\u{feff}').to_string())
            }
        })
        .ok_or("empty input".to_string())?;

    if !header_line.starts_with("<REAPER_PROJECT") {
        return Err(format!(
            "expected <REAPER_PROJECT header, got: {}",
            header_line
        ));
    }

    let header_tail = header_line["<REAPER_PROJECT".len()..].trim();
    let header_tokens = tokenize_line(header_tail)
        .map_err(|e| format!("failed to parse project header tokens: {e}"))?;

    let version = header_tokens
        .first()
        .and_then(|t| t.as_number())
        .unwrap_or(0.1);
    let version_string = header_tokens
        .get(1)
        .and_then(|t| t.as_string())
        .unwrap_or("")
        .to_string();
    let timestamp = header_tokens
        .get(2)
        .and_then(|t| t.as_number())
        .unwrap_or(0.0) as i64;

    let mut top_blocks: Vec<RppBlock> = Vec::new();
    let mut project_props = RppBlock {
        block_type: BlockType::Project,
        name: "REAPER_PROJECT".to_string(),
        params: vec![],
        children: vec![],
    };
    let mut stack: Vec<RppBlock> = Vec::new();
    let mut project_closed = false;

    for (line_idx0, raw_line) in lines.enumerate() {
        let line_no = line_idx0 + 2;
        let line = raw_line.trim();
        if is_skippable_trimmed(line) {
            continue;
        }

        if line == ">" {
            if let Some(block) = stack.pop() {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(RppBlockContent::Block(block));
                } else {
                    top_blocks.push(block);
                }
            } else {
                project_closed = true;
                break;
            }
            continue;
        }

        if let Some(block_line) = line.strip_prefix('<') {
            let (name, params) = parse_block_header(block_line)
                .map_err(|e| format!("line {line_no}: invalid block header: {e}"))?;
            stack.push(RppBlock {
                block_type: BlockType::parse(&name),
                name,
                params,
                children: vec![],
            });
            continue;
        }

        let parse_structured = stack
            .last()
            .map(|b| requires_structured_tokens(&b.name))
            .unwrap_or(true);
        let tokens = if parse_structured {
            tokenize_line(line).map_err(|e| format!("line {line_no}: invalid content line: {e}"))?
        } else {
            Vec::new()
        };
        if let Some(parent) = stack.last_mut() {
            if parse_structured {
                parent.children.push(RppBlockContent::Content(tokens));
            } else {
                parent
                    .children
                    .push(RppBlockContent::RawLine(line.to_string().into_boxed_str()));
            }
        } else {
            project_props
                .children
                .push(RppBlockContent::Content(tokens));
        }
    }

    if !project_closed {
        return Err("missing closing > for REAPER_PROJECT".to_string());
    }
    if !stack.is_empty() {
        return Err(format!("unclosed blocks at EOF: {}", stack.len()));
    }

    if !project_props.children.is_empty() {
        top_blocks.insert(0, project_props);
    }

    Ok(RppProject {
        version,
        version_string,
        timestamp,
        blocks: top_blocks,
    })
}

fn parse_rpp_fast_parallel(content: &str, force_parallel: bool) -> Result<RppProject, String> {
    let mut lines = content.lines();
    let header_line = lines
        .by_ref()
        .find_map(|line| {
            let t = line.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.trim_start_matches('\u{feff}').to_string())
            }
        })
        .ok_or("empty input".to_string())?;

    if !header_line.starts_with("<REAPER_PROJECT") {
        return Err(format!(
            "expected <REAPER_PROJECT header, got: {}",
            header_line
        ));
    }

    let header_tail = header_line["<REAPER_PROJECT".len()..].trim();
    let header_tokens = tokenize_line(header_tail)
        .map_err(|e| format!("failed to parse project header tokens: {e}"))?;

    let version = header_tokens
        .first()
        .and_then(|t| t.as_number())
        .unwrap_or(0.1);
    let version_string = header_tokens
        .get(1)
        .and_then(|t| t.as_string())
        .unwrap_or("")
        .to_string();
    let timestamp = header_tokens
        .get(2)
        .and_then(|t| t.as_number())
        .unwrap_or(0.0) as i64;

    let all_lines: Vec<&str> = lines.collect();
    let mut top_blocks: Vec<RppBlock> = Vec::new();
    let mut project_props = RppBlock {
        block_type: BlockType::Project,
        name: "REAPER_PROJECT".to_string(),
        params: vec![],
        children: vec![],
    };
    let mut project_closed = false;
    let mut project_depth = 0i32;
    let mut block_start: Option<usize> = None;
    let mut block_ranges: Vec<(usize, usize)> = Vec::new();

    for (idx, raw_line) in all_lines.iter().enumerate() {
        let line_no = idx + 2;
        let line = raw_line.trim();
        if is_skippable_trimmed(line) {
            continue;
        }

        if project_depth == 0 {
            if line == ">" {
                project_closed = true;
                break;
            }
            if line.starts_with('<') {
                project_depth = 1;
                block_start = Some(idx);
            } else {
                let tokens = tokenize_line(line)
                    .map_err(|e| format!("line {line_no}: invalid content line: {e}"))?;
                project_props
                    .children
                    .push(RppBlockContent::Content(tokens));
            }
            continue;
        }

        if line.starts_with('<') {
            project_depth += 1;
        } else if line == ">" {
            project_depth -= 1;
            if project_depth == 0 {
                let start = block_start.ok_or_else(|| {
                    format!("line {line_no}: internal error, missing block start")
                })?;
                block_ranges.push((start, idx));
                block_start = None;
            } else if project_depth < 0 {
                return Err(format!("line {line_no}: invalid project depth"));
            }
        }
    }

    if !project_closed {
        return Err("missing closing > for REAPER_PROJECT".to_string());
    }
    if project_depth != 0 {
        return Err("unclosed top-level block at EOF".to_string());
    }

    // Conservative activation criteria for optional parallel mode.
    // Keep single-thread processing unless we have a top-block-heavy file
    // and enough worker threads to amortize rayon overhead.
    // Parallel parsing is opt-in via the `parallel` feature. When
    // disabled (e.g. WASM builds), this branch is dead code.
    #[cfg(feature = "parallel")]
    let parse_parallel = if force_parallel {
        true
    } else {
        block_ranges.len() >= 1024
            && content.len() >= (32 * 1024 * 1024)
            && rayon::current_num_threads() > 1
    };
    #[cfg(not(feature = "parallel"))]
    let parse_parallel = false;
    #[cfg(not(feature = "parallel"))]
    let _ = force_parallel; // suppress unused warning when parallel is off

    let parsed_blocks: Vec<(usize, Result<RppBlock, String>)> = if parse_parallel {
        #[cfg(feature = "parallel")]
        {
            block_ranges
                .par_iter()
                .enumerate()
                .map(|(order, (start, end))| {
                    let slice = &all_lines[*start..=*end];
                    let base = *start + 2;
                    (order, parse_single_block_lines(slice, base))
                })
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            unreachable!("parse_parallel is false when feature is off")
        }
    } else {
        block_ranges
            .iter()
            .enumerate()
            .map(|(order, (start, end))| {
                let slice = &all_lines[*start..=*end];
                let base = *start + 2;
                (order, parse_single_block_lines(slice, base))
            })
            .collect()
    };

    let mut parsed_blocks = parsed_blocks;
    parsed_blocks.sort_by_key(|(order, _)| *order);
    for (_order, block) in parsed_blocks {
        top_blocks.push(block?);
    }

    if !project_props.children.is_empty() {
        top_blocks.insert(0, project_props);
    }

    Ok(RppProject {
        version,
        version_string,
        timestamp,
        blocks: top_blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_rpp_fast;
    use crate::primitives::project::parse_rpp;
    use crate::types::ReaperProject;

    #[test]
    fn fast_parser_matches_nom_small_project() {
        let input = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369
  RIPPLE 0 0
  <TRACK
    NAME "Track 1"
    <ITEM
      POSITION 0
      LENGTH 1
    >
  >
>"#;

        let fast = parse_rpp_fast(input).expect("fast parse failed");
        let (rem, nom) = parse_rpp(input).expect("nom parse failed");
        assert!(rem.trim().is_empty(), "nom parser had trailing input");

        // Fast parser can keep some deep blocks opaque for allocation savings.
        // Validate semantic equivalence through typed conversion.
        let mut fast_typed =
            ReaperProject::from_rpp_project(&fast).expect("fast typed conversion failed");
        let mut nom_typed =
            ReaperProject::from_rpp_project(&nom).expect("nom typed conversion failed");
        // custom_properties can diverge for non-domain lines and is not part of
        // typed domain parity guarantees.
        fast_typed.properties.custom_properties.clear();
        nom_typed.properties.custom_properties.clear();
        assert_eq!(fast_typed, nom_typed);
    }
}
