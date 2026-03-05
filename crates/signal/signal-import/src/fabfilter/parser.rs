//! Parser for FabFilter `.ffp` text-format presets (Pro-Q 4, Pro-C 3).
//!
//! Text-format `.ffp` files use an INI-like structure:
//! ```text
//! [Preset]
//! Signature=FFPQ
//! Author=FabFilter
//! Description=A warm EQ curve
//! Tags=Drums,Bright,Bus
//! [Parameters]
//! Band 1 Frequency=9.96
//! ...
//! ```
//! Binary `.ffp` files start with a 4-byte signature (no `[Preset]` header).

/// Parsed metadata from a text-format `.ffp` file.
#[derive(Debug, Clone, PartialEq)]
pub struct FfpPreset {
    pub signature: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Check whether raw bytes represent a text-format `.ffp` file.
///
/// Text presets start with `[Preset]` (possibly with a UTF-8 BOM).
pub fn is_text_format(bytes: &[u8]) -> bool {
    let s = std::str::from_utf8(bytes).unwrap_or("");
    let trimmed = s.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with("[Preset]")
}

/// Parse metadata from a text-format `.ffp` file.
///
/// Extracts Signature, Author, Description, and Tags from the `[Preset]` section.
/// Parameters are intentionally ignored — we store the raw bytes as `state_data`.
pub fn parse_ffp_text(content: &str) -> eyre::Result<FfpPreset> {
    let content = content.trim_start_matches('\u{feff}');
    let mut signature = String::new();
    let mut author = None;
    let mut description = None;
    let mut tags = Vec::new();
    let mut in_preset_section = false;

    for line in content.lines() {
        let line = line.trim();

        if line.eq_ignore_ascii_case("[Preset]") {
            in_preset_section = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            // Entering a different section — stop reading [Preset] keys
            in_preset_section = false;
            continue;
        }

        if !in_preset_section {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "Signature" => signature = value.to_string(),
                "Author" => {
                    if !value.is_empty() {
                        author = Some(value.to_string());
                    }
                }
                "Description" => {
                    if !value.is_empty() {
                        description = Some(value.to_string());
                    }
                }
                "Tags" => {
                    tags = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    if signature.is_empty() {
        eyre::bail!("Missing Signature in [Preset] section");
    }

    Ok(FfpPreset {
        signature,
        author,
        description,
        tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_preset_extracts_metadata() {
        let content = "\
[Preset]
Signature=FFPQ
Author=FabFilter
Description=A bright EQ for drums
Tags=Drums,Bright,Bus
[Parameters]
Band 1 Frequency=9.96
Band 1 Gain=3.0
";
        let preset = parse_ffp_text(content).unwrap();
        assert_eq!(preset.signature, "FFPQ");
        assert_eq!(preset.author.as_deref(), Some("FabFilter"));
        assert_eq!(preset.description.as_deref(), Some("A bright EQ for drums"));
        assert_eq!(preset.tags, vec!["Drums", "Bright", "Bus"]);
    }

    #[test]
    fn parse_text_preset_handles_missing_optional_fields() {
        let content = "\
[Preset]
Signature=FFPQ
[Parameters]
Band 1 Frequency=1.0
";
        let preset = parse_ffp_text(content).unwrap();
        assert_eq!(preset.signature, "FFPQ");
        assert!(preset.author.is_none());
        assert!(preset.description.is_none());
        assert!(preset.tags.is_empty());
    }

    #[test]
    fn parse_text_preset_fails_without_signature() {
        let content = "\
[Preset]
Author=FabFilter
[Parameters]
";
        assert!(parse_ffp_text(content).is_err());
    }

    #[test]
    fn is_text_format_detects_ini_header() {
        assert!(is_text_format(b"[Preset]\nSignature=FFPQ"));
        assert!(is_text_format(b"\xef\xbb\xbf[Preset]\nSignature=FFPQ")); // UTF-8 BOM
        assert!(!is_text_format(b"FFPQ\x00\x00\x00\x01")); // binary
        assert!(!is_text_format(b"")); // empty
    }

    #[test]
    fn parse_handles_whitespace_in_tags() {
        let content = "\
[Preset]
Signature=FFPC
Tags= Drums , Bright , Bus
[Parameters]
";
        let preset = parse_ffp_text(content).unwrap();
        assert_eq!(preset.tags, vec!["Drums", "Bright", "Bus"]);
    }
}
