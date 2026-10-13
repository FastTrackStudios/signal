//! Chunk text extraction and insertion helpers for REAPER state chunks.
//!
//! These functions operate on raw RPP chunk text (strings), enabling:
//! - Extracting FXCHAIN blocks from track state chunks
//! - Extracting named CONTAINER blocks from FX chains
//! - Inserting blocks into FX chains
//! - Wrapping FX chain content in a CONTAINER block
//!
//! Used by the module preset save/load flow to manipulate chunk text atomically.

/// Extract the FXCHAIN block from a track state chunk.
///
/// Returns the complete `<FXCHAIN ...>...</FXCHAIN>` text including delimiters,
/// or `None` if no FXCHAIN section is found.
pub fn extract_fxchain_block(track_chunk: &str) -> Option<&str> {
    extract_block_by_tag(track_chunk, "FXCHAIN")
}

/// Extract a named CONTAINER block from FX chain text.
///
/// Searches for `<CONTAINER Container "name"` (case-insensitive name match)
/// and returns the complete container block text including delimiters.
///
/// Returns `None` if no container with that name is found.
pub fn extract_container_block<'a>(fxchain_text: &'a str, container_name: &str) -> Option<&'a str> {
    let name_lower = container_name.to_lowercase();

    // Scan for <CONTAINER lines
    let bytes = fxchain_text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Find next '<CONTAINER'
        if let Some(pos) = fxchain_text[i..].find("<CONTAINER") {
            let abs_pos = i + pos;

            // Check if this container's name matches
            let header_end = fxchain_text[abs_pos..]
                .find('\n')
                .map(|p| abs_pos + p)
                .unwrap_or(fxchain_text.len());
            let header = &fxchain_text[abs_pos..header_end];

            if container_name_matches(header, &name_lower) {
                // Found matching container — extract the full block
                if let Some(end) = find_block_end(fxchain_text, abs_pos) {
                    return Some(&fxchain_text[abs_pos..=end]);
                }
            }

            // Move past this <CONTAINER to look for the next one
            i = abs_pos + "<CONTAINER".len();
        } else {
            break;
        }
    }

    None
}

/// Insert a raw RPP chunk block into an FX chain.
///
/// The block (e.g., a `<CONTAINER ...>...>` block) is appended at the end
/// of the FX chain, before the closing `>` of the FXCHAIN section.
///
/// `track_chunk` is the complete track state chunk text.
/// Returns the modified track chunk text.
pub fn insert_into_fxchain(track_chunk: &str, block_text: &str) -> Result<String, String> {
    // Find the FXCHAIN block
    let fxchain_start = track_chunk
        .find("<FXCHAIN")
        .ok_or_else(|| "No FXCHAIN section found in track chunk".to_string())?;

    // Find the closing > of the FXCHAIN block
    let fxchain_end = find_block_end(track_chunk, fxchain_start)
        .ok_or_else(|| "FXCHAIN block is not properly closed".to_string())?;

    // The closing > is at fxchain_end. Insert the block text before it.
    // Find the line start of the closing >
    let close_line_start = track_chunk[..fxchain_end]
        .rfind('\n')
        .map(|p| p + 1)
        .unwrap_or(fxchain_end);

    // Get the indentation of the closing >
    let close_line = &track_chunk[close_line_start..=fxchain_end];
    let indent = &close_line[..close_line.len() - close_line.trim_start().len()];

    // Build the insertion: indent the block text to match the FX chain level
    let indented_block = indent_block(block_text.trim(), indent);

    let mut result = String::with_capacity(track_chunk.len() + indented_block.len() + 2);
    result.push_str(&track_chunk[..close_line_start]);
    result.push_str(&indented_block);
    result.push('\n');
    result.push_str(&track_chunk[close_line_start..]);

    Ok(result)
}

/// Wrap FX chain content in a CONTAINER block.
///
/// Given raw FX chain content (the inner content of an FXCHAIN, not the
/// FXCHAIN wrapper itself), wraps it in a named container block.
///
/// Returns the complete `<CONTAINER Container "name" ""\n...\n>` text.
pub fn wrap_in_container(fx_content: &str, container_name: &str) -> String {
    let mut result = String::new();
    result.push_str(&format!(
        "<CONTAINER Container \"{}\" \"\"\n",
        container_name
    ));
    result.push_str("  CONTAINER_CFG 2 2 2 0\n");
    result.push_str("  SHOW 0\n");
    result.push_str("  LASTSEL 0\n");
    result.push_str("  DOCKED 0\n");

    // Indent the FX content by 2 spaces
    for line in fx_content.lines() {
        if line.trim().is_empty() {
            result.push('\n');
        } else {
            result.push_str("  ");
            result.push_str(line);
            result.push('\n');
        }
    }

    result.push('>');
    result
}

/// Extract an FXCHAIN_REC (input FX chain) block from a track state chunk.
pub fn extract_input_fxchain_block(track_chunk: &str) -> Option<&str> {
    extract_block_by_tag(track_chunk, "FXCHAIN_REC")
}

/// Get the inner content of an FXCHAIN block (everything between the header line and closing >).
///
/// Strips the `<FXCHAIN ...` header line and the closing `>` line.
pub fn fxchain_inner_content(fxchain_block: &str) -> Option<&str> {
    let trimmed = fxchain_block.trim();

    // Find end of first line
    let first_newline = trimmed.find('\n')?;
    let after_header = &trimmed[first_newline + 1..];

    // Find the last > (closing the block)
    let last_close = after_header.rfind('>')?;

    // Return everything between header and closing >
    let inner = &after_header[..last_close];

    // Trim trailing whitespace/newlines before the >
    Some(inner.trim_end())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract a block by its tag name from RPP text.
/// Returns the full block text from `<TAG` through the matching `>`.
fn extract_block_by_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let pattern = format!("<{}", tag);
    let start = text.find(&pattern)?;
    let end = find_block_end(text, start)?;
    Some(&text[start..=end])
}

/// Find the position of the closing `>` for a block starting at `start_pos`.
/// Handles nested blocks correctly by tracking depth.
///
/// Uses line-based parsing: only `<TAG` at the start of a trimmed line opens a block,
/// and only `>` as the sole content of a trimmed line closes a block.
/// This avoids false positives from `<` and `>` inside base64 data or hex strings.
fn find_block_end(text: &str, start_pos: usize) -> Option<usize> {
    let mut depth = 0;
    let mut pos = start_pos;

    for line in text[start_pos..].lines() {
        let trimmed = line.trim();
        let line_end = pos + line.len(); // position after this line's content

        if trimmed.starts_with('<') && !trimmed.starts_with("<<") {
            depth += 1;
        } else if trimmed == ">" {
            depth -= 1;
            if depth == 0 {
                // Return position of the > character itself
                let gt_pos = text[pos..line_end].rfind('>').unwrap() + pos;
                return Some(gt_pos);
            }
        }

        // +1 for the newline that .lines() strips
        pos = line_end + 1;
    }

    None
}

/// Check if a CONTAINER header line matches a given name (case-insensitive).
///
/// Header formats:
/// - `<CONTAINER Container "DRIVE" ""`
/// - `<CONTAINER Container DRIVE`
fn container_name_matches(header: &str, name_lower: &str) -> bool {
    // Try quoted name
    let mut in_quote = false;
    let mut quote_count = 0;
    let mut current_quoted = String::new();

    for ch in header.chars() {
        if ch == '"' {
            if in_quote {
                // Closing quote
                quote_count += 1;
                if quote_count == 1 {
                    // First quoted string — this is the container name in REAPER format
                    // (but the header is actually: Container "NAME" "")
                    // The first quoted string after "Container" is the name
                    if current_quoted.to_lowercase() == *name_lower {
                        return true;
                    }
                }
                current_quoted.clear();
                in_quote = false;
            } else {
                in_quote = true;
            }
        } else if in_quote {
            current_quoted.push(ch);
        }
    }

    // Fallback: check unquoted tokens
    let parts: Vec<&str> = header.split_whitespace().collect();
    // Typical: ["<CONTAINER", "Container", "NAME"] or ["<CONTAINER", "Container", "\"NAME\"", "\"\""]
    for part in parts.iter().skip(2) {
        let cleaned = part.trim_matches('"');
        if !cleaned.is_empty() && cleaned.to_lowercase() == *name_lower {
            return true;
        }
    }

    false
}

/// Indent all lines of a block by the given prefix.
fn indent_block(block: &str, indent: &str) -> String {
    block
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{}{}", indent, line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TRACK_CHUNK: &str = r#"<TRACK
  NAME "Guitar"
  VOLPAN 1.0 0.0
  <FXCHAIN
    WNDRECT 0 0 800 600
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
      ZXE=
    >
    FXID {EQ-GUID-1234}
    BYPASS 0 0 0
    <CONTAINER Container "DRIVE" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: TubeScreamer (Analog)" ts808.dylib 0 "" 0<00> ""
        dGVzdA==
      >
      FXID {TS-GUID-5678}
    >
    FXID {DRIVE-CONTAINER-GUID}
  >
>"#;

    #[test]
    fn test_extract_fxchain_block() {
        let fxchain = extract_fxchain_block(SAMPLE_TRACK_CHUNK);
        assert!(fxchain.is_some());

        let block = fxchain.unwrap();
        assert!(block.starts_with("<FXCHAIN"));
        assert!(block.ends_with('>'));
        assert!(block.contains("ReaEQ"));
        assert!(block.contains("DRIVE"));
    }

    #[test]
    fn test_extract_container_block() {
        let fxchain = extract_fxchain_block(SAMPLE_TRACK_CHUNK).unwrap();
        let container = extract_container_block(fxchain, "DRIVE");

        assert!(container.is_some());
        let block = container.unwrap();
        assert!(block.starts_with("<CONTAINER"));
        assert!(block.contains("TubeScreamer"));
        assert!(block.contains("DRIVE"));
        assert!(block.ends_with('>'));
    }

    #[test]
    fn test_extract_container_case_insensitive() {
        let fxchain = extract_fxchain_block(SAMPLE_TRACK_CHUNK).unwrap();

        // Should match case-insensitively
        assert!(extract_container_block(fxchain, "drive").is_some());
        assert!(extract_container_block(fxchain, "Drive").is_some());
        assert!(extract_container_block(fxchain, "DRIVE").is_some());

        // Should not find nonexistent containers
        assert!(extract_container_block(fxchain, "AMP").is_none());
    }

    #[test]
    fn test_insert_into_fxchain() {
        let new_block = r#"<CONTAINER Container "AMP" ""
  CONTAINER_CFG 2 2 2 0
  SHOW 0
  LASTSEL 0
  DOCKED 0
>"#;

        let result = insert_into_fxchain(SAMPLE_TRACK_CHUNK, new_block).unwrap();

        // The result should still be valid — FXCHAIN should contain both the original
        // content and the new container
        assert!(result.contains("ReaEQ"));
        assert!(result.contains("DRIVE"));
        assert!(result.contains("AMP"));

        // The FXCHAIN should still be properly closed
        let fxchain = extract_fxchain_block(&result).unwrap();
        assert!(fxchain.starts_with("<FXCHAIN"));
        assert!(fxchain.ends_with('>'));
    }

    #[test]
    fn test_wrap_in_container() {
        let fx_content = r#"BYPASS 0 0 0
<VST "VST: Plugin (Test)" test.dylib 0 "" 0<00> ""
  dGVzdA==
>
FXID {TEST-GUID}"#;

        let wrapped = wrap_in_container(fx_content, "TIME");

        assert!(wrapped.starts_with("<CONTAINER Container \"TIME\""));
        assert!(wrapped.contains("CONTAINER_CFG 2 2 2 0"));
        assert!(wrapped.contains("Plugin (Test)"));
        assert!(wrapped.ends_with('>'));
    }

    #[test]
    fn test_fxchain_inner_content() {
        let fxchain = extract_fxchain_block(SAMPLE_TRACK_CHUNK).unwrap();
        let inner = fxchain_inner_content(fxchain);

        assert!(inner.is_some());
        let content = inner.unwrap();

        // Should NOT contain the <FXCHAIN header or closing >
        assert!(!content.starts_with("<FXCHAIN"));
        // Should contain the FX chain content
        assert!(content.contains("WNDRECT"));
        assert!(content.contains("ReaEQ"));
        assert!(content.contains("DRIVE"));
    }

    #[test]
    fn test_extract_no_fxchain() {
        let chunk = r#"<TRACK
  NAME "Empty Track"
  VOLPAN 1.0 0.0
>"#;
        assert!(extract_fxchain_block(chunk).is_none());
    }

    #[test]
    fn test_insert_no_fxchain() {
        let chunk = r#"<TRACK
  NAME "Empty Track"
>"#;
        let result = insert_into_fxchain(chunk, "<CONTAINER>");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrap_empty_content() {
        let wrapped = wrap_in_container("", "EMPTY");
        assert!(wrapped.starts_with("<CONTAINER Container \"EMPTY\""));
        assert!(wrapped.contains("CONTAINER_CFG"));
        assert!(wrapped.ends_with('>'));
    }

    #[test]
    fn test_multiple_containers_extract_correct_one() {
        let fxchain = r#"<FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <CONTAINER Container "INPUT" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
    >
    FXID {INPUT-GUID}
    BYPASS 0 0 0
    <CONTAINER Container "DRIVE" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: OD (Test)" od.dylib 0 "" 0<00> ""
        b2Q=
      >
      FXID {OD-GUID}
    >
    FXID {DRIVE-GUID}
    BYPASS 0 0 0
    <CONTAINER Container "AMP" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
    >
    FXID {AMP-GUID}
  >"#;

        let input = extract_container_block(fxchain, "INPUT");
        assert!(input.is_some());
        assert!(input.unwrap().contains("INPUT"));
        assert!(!input.unwrap().contains("DRIVE"));

        let drive = extract_container_block(fxchain, "DRIVE");
        assert!(drive.is_some());
        assert!(drive.unwrap().contains("OD (Test)"));

        let amp = extract_container_block(fxchain, "AMP");
        assert!(amp.is_some());
        assert!(amp.unwrap().contains("AMP"));
    }
}
