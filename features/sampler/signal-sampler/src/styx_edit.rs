//! Comment/string-aware text surgery for styx specs.
//!
//! Signalpack embedded specs must NEVER be round-tripped through
//! `facet_styx::to_string` (it emits defaulted `Option` variants the parser
//! rejects — packs load silent). All spec mutation is therefore text-level:
//! locate blocks/entries with these helpers, edit the matched spans, keep
//! everything else verbatim. Used by the `fts signal pack` CLI
//! ([`crate::pack_cli`]) and the Cinematic Studio pack builder example.

/// Iterate `text` bytes, calling `f(i, byte, in_code)` where `in_code` is
/// false inside `//` comments and `"…"` strings.
pub fn scan(text: &str, mut f: impl FnMut(usize, u8, bool)) {
    let bytes = text.as_bytes();
    let (mut in_str, mut in_comment) = (false, false);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_comment {
            if b == b'\n' {
                in_comment = false;
            }
            f(i, b, false);
        } else if in_str {
            f(i, b, false);
            if b == b'"' {
                in_str = false;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            in_comment = true;
            f(i, b, false);
        } else if b == b'"' {
            in_str = true;
            f(i, b, false);
        } else {
            f(i, b, true);
        }
        i += 1;
    }
}

/// Find the top-level `key ( … )` list block. Returns
/// `(block_start, inner_start, inner_end, block_end)` byte offsets.
pub fn find_list_block(text: &str, key: &str) -> Option<(usize, usize, usize, usize)> {
    let mut depth = 0i32;
    let mut result = None;
    let mut opened: Option<(usize, usize)> = None; // (block_start, inner_start)
    let mut pending_key_at: Option<usize> = None;
    let bytes = text.as_bytes();
    scan(text, |i, b, in_code| {
        if !in_code || result.is_some() {
            return;
        }
        match b {
            b'(' | b'{' => {
                if depth == 0 && b == b'(' {
                    if let Some(ks) = pending_key_at {
                        opened = Some((ks, i + 1));
                    }
                }
                depth += 1;
            }
            b')' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some((bs, is)) = opened.take() {
                        result = Some((bs, is, i, i + 1));
                    }
                }
            }
            _ => {
                if depth == 0 && !b.is_ascii_whitespace() {
                    let at_word_start = i == 0
                        || bytes[i - 1].is_ascii_whitespace()
                        || bytes[i - 1] == b')'
                        || bytes[i - 1] == b'}';
                    if at_word_start && text[i..].starts_with(key) {
                        let after = i + key.len();
                        let ok = bytes
                            .get(after)
                            .map(|c| c.is_ascii_whitespace() || *c == b'(')
                            .unwrap_or(false);
                        if ok {
                            pending_key_at = Some(i);
                            return;
                        }
                    }
                    if let Some(ks) = pending_key_at {
                        if i >= ks + key.len()
                            && !text[ks + key.len()..i].chars().all(char::is_whitespace)
                        {
                            pending_key_at = None;
                        }
                    }
                }
            }
        }
    });
    result
}

/// Split a list block's inner text into top-level `{ … }` entry spans.
pub fn split_entries(inner: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    scan(inner, |i, b, in_code| {
        if !in_code {
            return;
        }
        match b {
            b'{' | b'(' => {
                if depth == 0 && b == b'{' {
                    start = i;
                }
                depth += 1;
            }
            b'}' | b')' => {
                depth -= 1;
                if depth == 0 && b == b'}' {
                    spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    });
    spans
}

/// First `key <value>` field in an entry (any position — handles both
/// line-per-field and inline `{id Mix, label Mix}` styles). Unquotes.
pub fn entry_field(entry: &str, key: &str) -> Option<String> {
    let bytes = entry.as_bytes();
    let mut depth = 0i32;
    let mut found_at: Option<usize> = None;
    scan(entry, |i, b, in_code| {
        if found_at.is_some() || !in_code {
            return;
        }
        match b {
            b'{' | b'(' => depth += 1,
            b'}' | b')' => depth -= 1,
            _ => {
                // depth 1 = directly inside the entry's braces
                if depth == 1 && has_key_at(entry, bytes, i, key) {
                    found_at = Some(i + key.len());
                }
            }
        }
    });
    let after = found_at?;
    let rest = entry[after..].trim_start();
    let value = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next()?.to_string()
    } else {
        rest.split([',', '}', '\n'])
            .next()?
            .split_whitespace()
            .next()?
            .to_string()
    };
    Some(value)
}

fn has_key_at(text: &str, bytes: &[u8], i: usize, key: &str) -> bool {
    let boundary_before = i == 0
        || bytes[i - 1].is_ascii_whitespace()
        || bytes[i - 1] == b'{'
        || bytes[i - 1] == b',';
    if !boundary_before || !text[i..].starts_with(key) {
        return false;
    }
    bytes
        .get(i + key.len())
        .map(|c| c.is_ascii_whitespace())
        .unwrap_or(false)
}

/// Set (replace or insert) a one-line scalar field in a line-per-field entry
/// block, preserving the block's indentation and `name<pad>value` alignment.
/// Returns the rewritten entry text.
pub fn set_entry_field(entry: &str, key: &str, value: &str) -> String {
    let lines: Vec<&str> = entry.split_inclusive('\n').collect();
    // Field indent: taken from the first field line (fallback 8 spaces).
    let indent = lines
        .iter()
        .find(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('{') && !t.starts_with('}') && !t.starts_with("//")
        })
        .map(|l| &l[..l.len() - l.trim_start().len()])
        .unwrap_or("        ");
    let field_line = format!("{indent}{key:<12} {value}\n");

    let mut out = String::with_capacity(entry.len() + field_line.len());
    let mut replaced = false;
    let mut close_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if !replaced
            && t.starts_with(key)
            && t[key.len()..].starts_with(|c: char| c.is_whitespace())
        {
            out.push_str(&field_line);
            replaced = true;
            continue;
        }
        if t.starts_with('}') {
            close_idx = Some(i);
        }
        out.push_str(line);
    }
    if replaced {
        return out;
    }
    // Insert before the closing brace line.
    let mut out = String::with_capacity(entry.len() + field_line.len());
    for (i, line) in lines.iter().enumerate() {
        if Some(i) == close_idx {
            out.push_str(&field_line);
        }
        out.push_str(line);
    }
    if close_idx.is_none() {
        out.push_str(&field_line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"name "t"
// a comment with { braces } and (parens)
zones (
    {
        file         "Main/x.wav"
        root_key     60
        gain_db      0.000
    }
    {
        file         "Room/y.wav"  // trailing { comment
        root_key     62
    }
)
"#;

    #[test]
    fn finds_and_splits_zones() {
        let (_, is, ie, _) = find_list_block(SPEC, "zones").unwrap();
        let inner = &SPEC[is..ie];
        let entries = split_entries(inner);
        assert_eq!(entries.len(), 2);
        let e0 = &inner[entries[0].0..entries[0].1];
        assert_eq!(entry_field(e0, "file").as_deref(), Some("Main/x.wav"));
        assert_eq!(entry_field(e0, "root_key").as_deref(), Some("60"));
    }

    #[test]
    fn set_field_replaces_and_inserts() {
        let (_, is, ie, _) = find_list_block(SPEC, "zones").unwrap();
        let inner = &SPEC[is..ie];
        let spans = split_entries(inner);
        let e0 = &inner[spans[0].0..spans[0].1];

        let replaced = set_entry_field(e0, "gain_db", "-3.0");
        assert!(replaced.contains("gain_db      -3.0"));
        assert!(!replaced.contains("0.000"));

        let inserted = set_entry_field(e0, "loop_start", "72000");
        assert!(inserted.contains("loop_start   72000"));
        // still well-formed: closing brace after the new field
        assert!(inserted.trim_end().ends_with('}'));
    }
}
