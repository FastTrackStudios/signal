//! Highlighter implementation that walks parsed tokens and AST.
//!
//! Provides line-by-line and full-chart highlighting using the parser's
//! tokenizer and chord parsing infrastructure.

use super::{HighlightKind, HighlightSpan};
use crate::parsing::{Lexer, Token, TokenType};

/// Highlighter for Keyflow notation.
///
/// Provides methods for highlighting individual lines or tokens,
/// producing spans that can be rendered with themes.
pub struct Highlighter;

impl Highlighter {
    /// Highlight a single line of Keyflow notation.
    ///
    /// This is the primary method for real-time editor highlighting.
    /// It analyzes the line content and produces highlight spans for
    /// all recognized elements.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spans = Highlighter::highlight_line("Gmaj7_8 | Am | D_4 G");
    /// assert!(!spans.is_empty());
    /// ```
    #[must_use]
    pub fn highlight_line(line: &str) -> Vec<HighlightSpan> {
        let line_trimmed = line.trim();

        if line_trimmed.is_empty() {
            return Vec::new();
        }

        // Check for comment lines first
        if line_trimmed.starts_with(';') {
            return Self::highlight_comment_line(line);
        }

        // Check for config directive lines (e.g., /push = triplet)
        if line_trimmed.starts_with('/') {
            return Self::highlight_config_directive(line);
        }

        // Check for metadata lines (title, tempo, key, time signature)
        if let Some(spans) = Self::try_highlight_metadata_line(line) {
            return spans;
        }

        // Check for section marker lines
        if let Some(spans) = Self::try_highlight_section_line(line) {
            return spans;
        }

        // Check for track marker lines
        if let Some(spans) = Self::try_highlight_track_marker(line) {
            return spans;
        }

        // Default: treat as chord line
        Self::highlight_chord_line(line)
    }

    /// Highlight a comment line.
    fn highlight_comment_line(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        // Find the semicolon
        if let Some(semi_pos) = line.find(';') {
            // Highlight the semicolon
            spans.push(HighlightSpan::from_range(
                semi_pos,
                1,
                HighlightKind::CommentMarker,
            ));

            // Highlight the rest as comment
            let comment_start = semi_pos + 1;
            if comment_start < line.len() {
                spans.push(HighlightSpan::from_range(
                    comment_start,
                    line.len() - comment_start,
                    HighlightKind::Comment,
                ));
            }
        }

        spans
    }

    /// Highlight a config directive line (e.g., /push = triplet).
    ///
    /// These are parser configuration lines that set rendering options.
    /// We highlight the entire line as a command/comment style.
    fn highlight_config_directive(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        // Find the leading whitespace
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if !trimmed.is_empty() {
            // Highlight the entire directive as a command
            spans.push(HighlightSpan::from_range(
                leading_ws,
                trimmed.len(),
                HighlightKind::Command,
            ));
        }

        spans
    }

    /// Try to highlight a metadata line.
    fn try_highlight_metadata_line(line: &str) -> Option<Vec<HighlightSpan>> {
        let trimmed = line.trim();

        // Title line: starts with a quote or contains " - " for artist
        if trimmed.starts_with('"') || trimmed.contains(" - ") {
            return Some(Self::highlight_title_line(line));
        }

        // Tempo line: contains "bpm" or starts with number followed by "bpm"
        let lower = trimmed.to_lowercase();
        if lower.contains("bpm") || lower.ends_with("bpm") {
            return Some(Self::highlight_tempo_line(line));
        }

        // Key signature: starts with # or b followed by uppercase letter
        if (trimmed.starts_with('#') || trimmed.starts_with('b'))
            && trimmed.len() >= 2
            && trimmed
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_uppercase())
        {
            return Some(Self::highlight_key_line(line));
        }

        // Time signature: matches pattern like "4/4", "6/8", "3/4"
        if Self::is_time_signature(trimmed) {
            return Some(Self::highlight_time_signature_line(line));
        }

        None
    }

    /// Check if a string is a time signature.
    fn is_time_signature(s: &str) -> bool {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return false;
        }
        parts[0].parse::<u8>().is_ok() && parts[1].parse::<u8>().is_ok()
    }

    /// Highlight a title line.
    fn highlight_title_line(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let trimmed = line.trim();

        // Find leading whitespace
        let leading_ws = line.len() - line.trim_start().len();

        // Check for "Title - Artist" format
        if let Some(dash_pos) = trimmed.find(" - ") {
            let title_end = leading_ws + dash_pos;
            let artist_start = leading_ws + dash_pos + 3;

            // Title
            spans.push(HighlightSpan::from_range(
                leading_ws,
                dash_pos,
                HighlightKind::Title,
            ));

            // Dash (as part of title formatting)
            spans.push(HighlightSpan::from_range(
                title_end,
                3,
                HighlightKind::Title,
            ));

            // Artist
            if artist_start < line.len() {
                spans.push(HighlightSpan::from_range(
                    artist_start,
                    line.trim_end().len() - (artist_start - leading_ws),
                    HighlightKind::Artist,
                ));
            }
        } else {
            // Just a title
            spans.push(HighlightSpan::from_range(
                leading_ws,
                trimmed.len(),
                HighlightKind::Title,
            ));
        }

        spans
    }

    /// Highlight a tempo line.
    fn highlight_tempo_line(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        // Check for tempo change arrow
        if trimmed.starts_with("->") {
            spans.push(HighlightSpan::from_range(
                leading_ws,
                2,
                HighlightKind::TempoArrow,
            ));
            spans.push(HighlightSpan::from_range(
                leading_ws + 2,
                trimmed.len() - 2,
                HighlightKind::Tempo,
            ));
        } else {
            spans.push(HighlightSpan::from_range(
                leading_ws,
                trimmed.len(),
                HighlightKind::Tempo,
            ));
        }

        spans
    }

    /// Highlight a key signature line.
    fn highlight_key_line(line: &str) -> Vec<HighlightSpan> {
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        vec![HighlightSpan::from_range(
            leading_ws,
            trimmed.len(),
            HighlightKind::Key,
        )]
    }

    /// Highlight a time signature line.
    fn highlight_time_signature_line(line: &str) -> Vec<HighlightSpan> {
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        vec![HighlightSpan::from_range(
            leading_ws,
            trimmed.len(),
            HighlightKind::TimeSignature,
        )]
    }

    /// Try to highlight a section marker line.
    fn try_highlight_section_line(line: &str) -> Option<Vec<HighlightSpan>> {
        let trimmed = line.trim();

        // Custom section with brackets
        if trimmed.starts_with('[') && trimmed.contains(']') {
            return Some(Self::highlight_custom_section(line));
        }

        // Standard section markers
        let first_word = trimmed.split_whitespace().next()?;
        let first_word_lower = first_word.to_lowercase();

        // Check if it's a section keyword
        let is_section = matches!(
            first_word_lower.as_str(),
            "intro"
                | "in"
                | "verse"
                | "vs"
                | "v"
                | "chorus"
                | "ch"
                | "c"
                | "bridge"
                | "br"
                | "b"
                | "outro"
                | "out"
                | "o"
                | "instrumental"
                | "inst"
                | "count"
                | "countin"
                | "count-in"
                | "hits"
                | "hit"
                | "interlude"
                | "inter"
                | "int"
                | "breakdown"
                | "bd"
        );

        // Also check for preset modifiers (Down, Build, etc.)
        let is_preset_modifier = matches!(
            first_word_lower.as_str(),
            "down"
                | "build"
                | "half-time"
                | "halftime"
                | "double-time"
                | "doubletime"
                | "soft"
                | "loud"
                | "quiet"
                | "big"
                | "small"
                | "sparse"
                | "full"
                | "stripped"
        );

        if is_section {
            return Some(Self::highlight_standard_section(line));
        }

        if is_preset_modifier {
            // Check if next word is a section
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            if words.len() >= 2 {
                let second_lower = words[1].to_lowercase();
                if is_section_keyword(&second_lower) {
                    return Some(Self::highlight_section_with_modifier(line));
                }
            }
        }

        None
    }

    /// Highlight a custom section marker (e.g., [Hits], [SOLO Keys]).
    fn highlight_custom_section(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let _leading_ws = line.len() - line.trim_start().len();

        // Find brackets
        if let (Some(open), Some(close)) = (line.find('['), line.find(']')) {
            // Opening bracket
            spans.push(HighlightSpan::from_range(
                open,
                1,
                HighlightKind::SectionBracket,
            ));

            // Section name
            if close > open + 1 {
                spans.push(HighlightSpan::from_range(
                    open + 1,
                    close - open - 1,
                    HighlightKind::Section,
                ));
            }

            // Closing bracket
            spans.push(HighlightSpan::from_range(
                close,
                1,
                HighlightKind::SectionBracket,
            ));

            // Check for measure count after bracket
            let after_bracket = &line[close + 1..];
            if let Some(count_start) = after_bracket.find(|c: char| c.is_ascii_digit()) {
                let count_pos = close + 1 + count_start;
                let count_str: String = after_bracket[count_start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '+' || *c == '-' || *c == 'x')
                    .collect();
                if !count_str.is_empty() {
                    spans.push(HighlightSpan::from_range(
                        count_pos,
                        count_str.len(),
                        HighlightKind::MeasureCount,
                    ));
                }
            }
        }

        spans
    }

    /// Highlight a standard section marker (e.g., vs 8, ch 4).
    fn highlight_standard_section(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        let mut pos = leading_ws;
        let _chars = trimmed.char_indices().peekable();

        // Section keyword
        let keyword_len = trimmed
            .split_whitespace()
            .next()
            .map(|s| s.len())
            .unwrap_or(0);
        spans.push(HighlightSpan::from_range(
            pos,
            keyword_len,
            HighlightKind::Section,
        ));
        pos += keyword_len;

        // Skip whitespace
        while pos < line.len() && line[pos..].starts_with(' ') {
            pos += 1;
        }

        // Measure count or expression (if present)
        let remaining = &line[pos..];
        let count_end = remaining.find(['"', ';']).unwrap_or(remaining.len());
        let count_str = remaining[..count_end].trim();

        if !count_str.is_empty() {
            // Check if it's a number or expression
            let is_measure_expr = count_str
                .chars()
                .all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == 'x' || c == ' ');
            if is_measure_expr {
                spans.push(HighlightSpan::from_range(
                    pos,
                    count_str.len(),
                    HighlightKind::MeasureCount,
                ));
                pos += count_str.len();
            }
        }

        // Check for quoted comment
        if let Some(quote_start) = line[pos..].find('"') {
            let abs_quote_start = pos + quote_start;
            if let Some(quote_end) = line[abs_quote_start + 1..].find('"') {
                spans.push(HighlightSpan::from_range(
                    abs_quote_start,
                    quote_end + 2,
                    HighlightKind::SectionComment,
                ));
            }
        }

        spans
    }

    /// Highlight a section with a preset modifier (e.g., "Down ch 4").
    fn highlight_section_with_modifier(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let leading_ws = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        let words: Vec<&str> = trimmed.split_whitespace().collect();
        let mut pos = leading_ws;

        // First word is the modifier (treated as section comment)
        if !words.is_empty() {
            spans.push(HighlightSpan::from_range(
                pos,
                words[0].len(),
                HighlightKind::SectionComment,
            ));
            pos += words[0].len();

            // Skip whitespace
            while pos < line.len() && line[pos..].starts_with(' ') {
                pos += 1;
            }
        }

        // Second word is the section keyword
        if words.len() > 1 {
            spans.push(HighlightSpan::from_range(
                pos,
                words[1].len(),
                HighlightKind::Section,
            ));
            pos += words[1].len();

            // Skip whitespace
            while pos < line.len() && line[pos..].starts_with(' ') {
                pos += 1;
            }
        }

        // Third word is the measure count (if present)
        if words.len() > 2 {
            spans.push(HighlightSpan::from_range(
                pos,
                words[2].len(),
                HighlightKind::MeasureCount,
            ));
        }

        spans
    }

    /// Try to highlight a track marker line.
    fn try_highlight_track_marker(line: &str) -> Option<Vec<HighlightSpan>> {
        let trimmed = line.trim();

        if !trimmed.starts_with('[') {
            return None;
        }

        // Check for track markers like [Chords], [Melody], [Rhythm], [Lyrics]
        let name = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .or_else(|| {
                trimmed
                    .strip_prefix('[')
                    .and_then(|s| s.find(']').map(|i| &s[..i]))
            })?;

        let name_lower = name.to_lowercase();
        let first_word = name_lower.split_whitespace().next().unwrap_or("");

        if ["chords", "melody", "rhythm", "lyrics"].contains(&first_word) {
            let _leading_ws = line.len() - line.trim_start().len();
            let open = line.find('[')?;
            let close = line.find(']')?;

            return Some(vec![HighlightSpan::from_range(
                open,
                close - open + 1,
                HighlightKind::TrackMarker,
            )]);
        }

        None
    }

    /// Highlight a chord line (the most common case).
    fn highlight_chord_line(line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let mut lexer = Lexer::new(line.to_string());
        let tokens = lexer.tokenize();

        let mut i = 0;
        while i < tokens.len() {
            let token = &tokens[i];

            match &token.token_type {
                // Skip EOF
                TokenType::Eof => {}

                // Measure separator
                TokenType::Slash => {
                    // Check if this is a slash rhythm (multiple slashes or single)
                    let mut slash_count = 1;
                    let start_pos = token.pos;
                    let mut end_idx = i;

                    // Look ahead for more slashes
                    while end_idx + 1 < tokens.len() {
                        if matches!(tokens[end_idx + 1].token_type, TokenType::Slash) {
                            slash_count += 1;
                            end_idx += 1;
                        } else {
                            break;
                        }
                    }

                    let kind = if slash_count == 1 {
                        // Check context - is this a measure separator or bass note slash?
                        // If preceded by a letter, it's likely a bass slash
                        if i > 0
                            && matches!(
                                tokens[i - 1].token_type,
                                TokenType::Letter(_) | TokenType::Number(_)
                            )
                        {
                            HighlightKind::BassSlash
                        } else {
                            HighlightKind::MeasureSeparator
                        }
                    } else {
                        HighlightKind::SlashRhythm
                    };

                    spans.push(HighlightSpan::from_range(start_pos, slash_count, kind));
                    i = end_idx;
                }

                // Underscore - start of duration notation
                TokenType::Underscore => {
                    let start_pos = token.pos;
                    let mut len = 1;

                    // Look ahead for duration number and modifiers
                    if i + 1 < tokens.len()
                        && let TokenType::Number(num) = &tokens[i + 1].token_type
                    {
                        len += num.len();
                        i += 1;

                        // Check for dot or triplet marker
                        while i + 1 < tokens.len() {
                            match &tokens[i + 1].token_type {
                                TokenType::Dot => {
                                    len += 1;
                                    i += 1;
                                }
                                TokenType::Letter('t') => {
                                    len += 1;
                                    i += 1;
                                }
                                _ => break,
                            }
                        }
                    }

                    spans.push(HighlightSpan::from_range(
                        start_pos,
                        len,
                        HighlightKind::Duration,
                    ));
                }

                // Apostrophe - push or pull notation
                TokenType::Apostrophe => {
                    let start_pos = token.pos;
                    let mut len = 1;

                    // Count consecutive apostrophes
                    while i + 1 < tokens.len()
                        && matches!(tokens[i + 1].token_type, TokenType::Apostrophe)
                    {
                        len += 1;
                        i += 1;
                    }

                    // Determine if push or pull based on context
                    // Push: apostrophe before chord
                    // Pull: apostrophe after chord
                    let kind = if i > 0
                        && matches!(
                            tokens[i - len].token_type,
                            TokenType::Letter(_) | TokenType::Number(_)
                        ) {
                        HighlightKind::Pull
                    } else {
                        HighlightKind::Push
                    };

                    spans.push(HighlightSpan::from_range(start_pos, len, kind));
                }

                // Letter - could be chord root, quality, or other
                TokenType::Letter(c) => {
                    let start_pos = token.pos;

                    // Check if this could be a chord root (A-G, uppercase or lowercase)
                    // IMPORTANT: Only match at word boundaries to avoid highlighting
                    // letters inside words like "Midnight" or "Dreams"
                    let is_potential_root = c.to_ascii_uppercase().is_ascii_alphabetic()
                        && matches!(
                            c.to_ascii_uppercase(),
                            'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G'
                        );

                    // Must be at chord position AND not followed by word continuation
                    // This prevents "Dreams" from having "D" highlighted as a chord root
                    if is_potential_root
                        && Self::is_chord_position(&tokens, i)
                        && !Self::is_followed_by_word_continuation(&tokens, i)
                    {
                        // This looks like a chord root
                        spans.push(HighlightSpan::from_range(start_pos, 1, HighlightKind::Root));

                        // Look ahead for a root accidental. `#`/`b`
                        // here belong to the root (F# is one note), so
                        // they take the ROOT color, not the standalone
                        // accidental color — the accidental inherits
                        // the color of what it modifies.
                        if i + 1 < tokens.len() {
                            match &tokens[i + 1].token_type {
                                TokenType::Sharp | TokenType::Flat | TokenType::Letter('b') => {
                                    spans.push(HighlightSpan::from_range(
                                        tokens[i + 1].pos,
                                        1,
                                        HighlightKind::Root,
                                    ));
                                    i += 1;
                                }
                                _ => {}
                            }
                        }

                        // Look for quality markers (m, maj, dim, aug, sus)
                        i = Self::highlight_chord_quality(&tokens, i, &mut spans);

                        // Look for extensions (7, 9, 11, 13)
                        i = Self::highlight_chord_extension(&tokens, i, &mut spans);
                    } else if (*c == 'r' || *c == 's') && Self::is_chord_position(&tokens, i) {
                        // Rest or space notation (only at word boundaries)
                        let kind = if *c == 'r' {
                            HighlightKind::Rest
                        } else {
                            HighlightKind::Space
                        };

                        // Look for duration after r/s
                        let mut rest_len = 1;
                        if i + 1 < tokens.len()
                            && let TokenType::Number(num) = &tokens[i + 1].token_type
                        {
                            rest_len += num.len();
                            i += 1;

                            // Check for triplet marker
                            if i + 1 < tokens.len()
                                && let TokenType::Letter('t') = &tokens[i + 1].token_type
                            {
                                rest_len += 1;
                                i += 1;
                            }
                        }

                        spans.push(HighlightSpan::from_range(start_pos, rest_len, kind));
                    } else if *c == 'm' && Self::is_after_root(&tokens, i) {
                        // Quality marker (minor) - only after a chord root
                        spans.push(HighlightSpan::from_range(
                            start_pos,
                            1,
                            HighlightKind::Quality,
                        ));
                    } else if *c == 't' && Self::is_after_number(&tokens, i) {
                        // Triplet marker - only after a number (like 8t, 4t)
                        spans.push(HighlightSpan::from_range(
                            start_pos,
                            1,
                            HighlightKind::Triplet,
                        ));
                    } else if ['I', 'V', 'i', 'v'].contains(c)
                        && Self::is_chord_position(&tokens, i)
                    {
                        // Roman numeral - highlight the whole numeral (only at word boundaries)
                        let (len, kind) = Self::parse_roman_numeral(&tokens, i);
                        spans.push(HighlightSpan::from_range(start_pos, len, kind));
                        i += len.saturating_sub(1);
                    } else {
                        // Other letter - might be part of quality or other notation
                        // Let it be handled by subsequent passes
                    }
                }

                // Number - could be scale degree, extension, or measure count
                TokenType::Number(num) => {
                    let start_pos = token.pos;
                    let num_val: u8 = num.parse().unwrap_or(0);

                    // Check if this is a scale degree (1-7 at start of chord position)
                    if (1..=7).contains(&num_val) && Self::is_chord_position(&tokens, i) {
                        spans.push(HighlightSpan::from_range(
                            start_pos,
                            num.len(),
                            HighlightKind::ScaleDegree,
                        ));
                    } else if [7, 9, 11, 13, 6].contains(&num_val) {
                        // Extension number
                        spans.push(HighlightSpan::from_range(
                            start_pos,
                            num.len(),
                            HighlightKind::Extension,
                        ));
                    } else {
                        // Generic number (might be measure count or part of modifier)
                        spans.push(HighlightSpan::from_range(
                            start_pos,
                            num.len(),
                            HighlightKind::MeasureCount,
                        ));
                    }
                }

                // Sharp and flat symbols
                TokenType::Sharp => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Accidental,
                    ));
                }
                TokenType::Flat => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Accidental,
                    ));
                }

                // Special chord symbols
                TokenType::Triangle => {
                    // Major 7 symbol (△)
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        token.len,
                        HighlightKind::Quality,
                    ));
                }
                TokenType::Circle => {
                    // Diminished symbol (°)
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        token.len,
                        HighlightKind::Quality,
                    ));
                }
                TokenType::HalfDiminished => {
                    // Half-diminished symbol (ø)
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        token.len,
                        HighlightKind::Quality,
                    ));
                }

                // Plus and minus (augmented, minor, or modifiers)
                TokenType::Plus => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Quality,
                    ));
                }
                TokenType::Minus => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Quality,
                    ));
                }

                // Parentheses (for modifiers)
                TokenType::LParen | TokenType::RParen => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Modifier,
                    ));
                }

                // At sign (command marker)
                TokenType::At => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Command,
                    ));
                }

                // Semicolon (comment start)
                TokenType::Semicolon => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::CommentMarker,
                    ));
                    // Rest of line is comment
                    let comment_start = token.pos + 1;
                    if comment_start < line.len() {
                        spans.push(HighlightSpan::from_range(
                            comment_start,
                            line.len() - comment_start,
                            HighlightKind::Comment,
                        ));
                        break; // Don't process more tokens
                    }
                }

                // Greater than (tempo change arrow start)
                TokenType::GreaterThan => {
                    // Check if this is part of -> tempo change
                    if i > 0 && matches!(tokens[i - 1].token_type, TokenType::Minus) {
                        // Already highlighted as part of tempo change
                    } else {
                        spans.push(HighlightSpan::from_range(
                            token.pos,
                            1,
                            HighlightKind::Unknown,
                        ));
                    }
                }

                // Dot (duration modifier or chord separator)
                TokenType::Dot => {
                    spans.push(HighlightSpan::from_range(token.pos, 1, HighlightKind::Dot));
                }

                // Tilde (memory recall)
                TokenType::Tilde => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::MemoryRecall,
                    ));
                }

                // Asterisk (repeat marker or multiplication)
                TokenType::Asterisk => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Repeat,
                    ));
                }

                // Space - skip
                TokenType::Space => {}

                // Comma - modifier separator
                TokenType::Comma => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Modifier,
                    ));
                }

                // Colon - optional root/quality separator (`1:7`)
                TokenType::Colon => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        1,
                        HighlightKind::Modifier,
                    ));
                }

                // Illegal token
                TokenType::Illegal => {
                    spans.push(HighlightSpan::from_range(
                        token.pos,
                        token.len,
                        HighlightKind::Unknown,
                    ));
                }
            }

            i += 1;
        }

        // Sort spans by position
        spans.sort_by_key(|s| s.span.start);

        spans
    }

    /// Check if current position is where a chord or notation would start.
    /// This is used to avoid highlighting letters inside regular words.
    fn is_chord_position(tokens: &[Token], idx: usize) -> bool {
        if idx == 0 {
            return true;
        }

        // Chord/notation starts after: space, measure separator, push/accent, or line start
        // This prevents highlighting letters like 'D' in "Midnight" or 'r' in "verse"
        matches!(
            tokens[idx - 1].token_type,
            TokenType::Space
                | TokenType::Slash
                | TokenType::Eof
                | TokenType::Apostrophe  // Push notation: 'G, 'Cm
                | TokenType::GreaterThan // Accent notation: >Cm
        )
    }

    /// Check if the previous token is a chord root (A-G).
    fn is_after_root(tokens: &[Token], idx: usize) -> bool {
        if idx == 0 {
            return false;
        }

        matches!(
            tokens[idx - 1].token_type,
            TokenType::Letter('A')
                | TokenType::Letter('B')
                | TokenType::Letter('C')
                | TokenType::Letter('D')
                | TokenType::Letter('E')
                | TokenType::Letter('F')
                | TokenType::Letter('G')
        )
    }

    /// Check if the previous token is a number (for triplet markers like 8t, 4t).
    fn is_after_number(tokens: &[Token], idx: usize) -> bool {
        if idx == 0 {
            return false;
        }

        matches!(tokens[idx - 1].token_type, TokenType::Number(_))
    }

    /// Check if what follows looks like a regular word (not a chord).
    /// Returns true if the next token is a letter that is NOT a valid chord modifier.
    /// For example: "Dreams" - after 'D' comes 'r', which is not a valid chord modifier,
    /// so 'D' should not be highlighted as a chord root.
    fn is_followed_by_word_continuation(tokens: &[Token], idx: usize) -> bool {
        if idx + 1 >= tokens.len() {
            return false; // End of tokens, not a word continuation
        }

        match &tokens[idx + 1].token_type {
            // These letters are valid after a chord root
            TokenType::Letter('m') => false, // minor
            TokenType::Letter('b') => false, // flat
            TokenType::Letter('a') => false, // aug, add
            TokenType::Letter('d') => false, // dim
            TokenType::Letter('s') => false, // sus
            TokenType::Letter('M') => false, // Maj
            // These are NOT valid chord modifiers - it's a regular word
            TokenType::Letter(_) => true,
            // Anything else (numbers, symbols, space, etc.) is fine for chord
            _ => false,
        }
    }

    /// Highlight chord quality markers and advance the token index.
    fn highlight_chord_quality(
        tokens: &[Token],
        start_idx: usize,
        spans: &mut Vec<HighlightSpan>,
    ) -> usize {
        let mut idx = start_idx;

        // Look for quality markers starting after the root
        while idx + 1 < tokens.len() {
            let next = &tokens[idx + 1];
            match &next.token_type {
                // Minor: m, min, -
                TokenType::Letter('m') => {
                    // Check for "maj" (major, not minor)
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('a'), TokenType::Letter('j')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            3,
                            HighlightKind::Quality,
                        ));
                        idx += 3;
                        continue;
                    }
                    // Check for "min"
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('i'), TokenType::Letter('n')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            3,
                            HighlightKind::Quality,
                        ));
                        idx += 3;
                        continue;
                    }
                    // Just 'm' for minor
                    spans.push(HighlightSpan::from_range(
                        next.pos,
                        1,
                        HighlightKind::Quality,
                    ));
                    idx += 1;
                }
                // Diminished: dim, o
                TokenType::Letter('d') => {
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('i'), TokenType::Letter('m')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            3,
                            HighlightKind::Quality,
                        ));
                        idx += 3;
                        continue;
                    }
                    break;
                }
                // Augmented: aug
                TokenType::Letter('a') => {
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('u'), TokenType::Letter('g')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            3,
                            HighlightKind::Quality,
                        ));
                        idx += 3;
                        continue;
                    }
                    break;
                }
                // Suspended: sus, sus2, sus4
                TokenType::Letter('s') => {
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('u'), TokenType::Letter('s')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        // Check for sus2 or sus4
                        let sus_len = if idx + 4 < tokens.len() {
                            if let TokenType::Number(n) = &tokens[idx + 4].token_type {
                                if n == "2" || n == "4" { 4 } else { 3 }
                            } else {
                                3
                            }
                        } else {
                            3
                        };
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            sus_len,
                            HighlightKind::Quality,
                        ));
                        idx += sus_len;
                        continue;
                    }
                    break;
                }
                // Major: M, Maj
                TokenType::Letter('M') => {
                    if idx + 3 < tokens.len()
                        && let (TokenType::Letter('a'), TokenType::Letter('j')) =
                            (&tokens[idx + 2].token_type, &tokens[idx + 3].token_type)
                    {
                        spans.push(HighlightSpan::from_range(
                            next.pos,
                            3,
                            HighlightKind::Quality,
                        ));
                        idx += 3;
                        continue;
                    }
                    // Just 'M' for major
                    spans.push(HighlightSpan::from_range(
                        next.pos,
                        1,
                        HighlightKind::Quality,
                    ));
                    idx += 1;
                }
                // Special symbols
                TokenType::Triangle | TokenType::Circle | TokenType::HalfDiminished => {
                    spans.push(HighlightSpan::from_range(
                        next.pos,
                        next.len,
                        HighlightKind::Quality,
                    ));
                    idx += 1;
                }
                TokenType::Plus | TokenType::Minus => {
                    spans.push(HighlightSpan::from_range(
                        next.pos,
                        1,
                        HighlightKind::Quality,
                    ));
                    idx += 1;
                }
                _ => break,
            }
        }

        idx
    }

    /// Highlight chord extension numbers and advance the token index.
    fn highlight_chord_extension(
        tokens: &[Token],
        start_idx: usize,
        spans: &mut Vec<HighlightSpan>,
    ) -> usize {
        let mut idx = start_idx;

        while idx + 1 < tokens.len() {
            let next = &tokens[idx + 1];
            match &next.token_type {
                // A plain extension number (6, 7, 9, 11, 13).
                TokenType::Number(num) if [6, 7, 9, 11, 13].contains(&num.parse().unwrap_or(0)) => {
                    spans.push(HighlightSpan::from_range(
                        next.pos,
                        num.len(),
                        HighlightKind::Extension,
                    ));
                    idx += 1;
                }
                // An ALTERATION: `b5`, `#11`, `b9`, `#5`, … — the
                // accidental modifies the degree that follows, so both
                // take the Extension color (the accidental inherits the
                // color of what it's connected to). Requires a number
                // immediately after the accidental.
                TokenType::Sharp | TokenType::Flat | TokenType::Letter('b')
                    if idx + 2 < tokens.len()
                        && matches!(tokens[idx + 2].token_type, TokenType::Number(_)) =>
                {
                    let acc = &tokens[idx + 1];
                    spans.push(HighlightSpan::from_range(
                        acc.pos,
                        1,
                        HighlightKind::Extension,
                    ));
                    if let TokenType::Number(num) = &tokens[idx + 2].token_type {
                        spans.push(HighlightSpan::from_range(
                            tokens[idx + 2].pos,
                            num.len(),
                            HighlightKind::Extension,
                        ));
                    }
                    idx += 2;
                }
                _ => break,
            }
        }

        idx
    }

    /// Parse a Roman numeral and return its length and kind.
    fn parse_roman_numeral(tokens: &[Token], start_idx: usize) -> (usize, HighlightKind) {
        let mut len = 0;

        // Count Roman numeral characters (I, V, i, v)
        let mut idx = start_idx;
        while idx < tokens.len() {
            if let TokenType::Letter(c) = &tokens[idx].token_type {
                if ['I', 'V', 'i', 'v'].contains(c) {
                    len += 1;
                    idx += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        (len.max(1), HighlightKind::RomanNumeral)
    }

    /// Highlight a single chord token and return spans.
    ///
    /// This is useful for highlighting chords in isolation,
    /// such as in a chord picker or reference display.
    #[must_use]
    pub fn highlight_chord(chord: &str) -> Vec<HighlightSpan> {
        Self::highlight_chord_line(chord)
    }
}

fn is_section_keyword(word: &str) -> bool {
    matches!(
        word.trim().to_ascii_lowercase().as_str(),
        "verse"
            | "vs"
            | "v"
            | "chorus"
            | "ch"
            | "c"
            | "bridge"
            | "br"
            | "b"
            | "intro"
            | "in"
            | "i"
            | "outro"
            | "out"
            | "o"
            | "instrumental"
            | "inst"
            | "instrument"
            | "solo"
            | "count"
            | "countin"
            | "count-in"
            | "hits"
            | "hit"
            | "interlude"
            | "inter"
            | "int"
            | "breakdown"
            | "bd"
            | "vamp"
            | "vmp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_simple_chord() {
        let spans = Highlighter::highlight_line("G");
        assert!(!spans.is_empty());
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Root));
    }

    #[test]
    fn test_highlight_chord_with_quality() {
        let spans = Highlighter::highlight_line("Gmaj7");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Root));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Quality));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Extension));
    }

    #[test]
    fn test_highlight_chord_with_duration() {
        let spans = Highlighter::highlight_line("G_8");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Root));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Duration));
    }

    #[test]
    fn test_highlight_section() {
        let spans = Highlighter::highlight_line("vs 8");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Section));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::MeasureCount));
    }

    #[test]
    fn test_highlight_comment() {
        let spans = Highlighter::highlight_line("; This is a comment");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::CommentMarker));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Comment));
    }

    #[test]
    fn test_highlight_title() {
        let spans = Highlighter::highlight_line("Song Title - Artist Name");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Title));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Artist));
    }

    #[test]
    fn test_highlight_tempo() {
        let spans = Highlighter::highlight_line("120bpm");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Tempo));
    }

    #[test]
    fn test_highlight_push_pull() {
        // Push notation
        let spans = Highlighter::highlight_line("'G");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Push));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Root));
    }

    #[test]
    fn test_highlight_rest() {
        let spans = Highlighter::highlight_line("r4");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Rest));
    }

    #[test]
    fn test_highlight_multiple_chords() {
        // In Keyflow, multiple chords on a line are separated by spaces
        let spans = Highlighter::highlight_line("G Am D");
        // Should have at least 3 root highlights (G, A, D)
        let root_count = spans
            .iter()
            .filter(|s| s.kind == HighlightKind::Root)
            .count();
        assert!(
            root_count >= 3,
            "Expected at least 3 roots, got {}",
            root_count
        );
    }

    #[test]
    fn test_highlight_slash_rhythm() {
        let spans = Highlighter::highlight_line("G////");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Root));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::SlashRhythm));
    }

    #[test]
    fn test_highlight_custom_section() {
        let spans = Highlighter::highlight_line("[SOLO Keys] 8");
        assert!(
            spans
                .iter()
                .any(|s| s.kind == HighlightKind::SectionBracket)
        );
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Section));
        assert!(spans.iter().any(|s| s.kind == HighlightKind::MeasureCount));
    }

    #[test]
    fn test_no_chord_roots_in_words() {
        // Letters A-G inside words should NOT be highlighted as chord roots
        // "Midnight Dreams" has D in both words, but neither should be highlighted
        let spans = Highlighter::highlight_line("Midnight Dreams");
        let root_count = spans
            .iter()
            .filter(|s| s.kind == HighlightKind::Root)
            .count();
        assert_eq!(
            root_count, 0,
            "Should not highlight letters inside words as roots, found {} roots",
            root_count
        );
    }

    #[test]
    fn test_chord_roots_at_word_start() {
        // Standalone letters A-G at word boundaries SHOULD be highlighted.
        // A root accidental takes the ROOT color, so `F#` contributes
        // two Root spans (the F and the #): A, D, F, # → 4.
        let spans = Highlighter::highlight_line("A D F#m");
        let root_count = spans
            .iter()
            .filter(|s| s.kind == HighlightKind::Root)
            .count();
        assert_eq!(
            root_count, 4,
            "A, D, F and F's accidental should all be root-colored, found {root_count}"
        );
    }

    #[test]
    fn root_accidental_and_alteration_inherit_color() {
        // `F#m7b5`: F# is one root (# takes Root color); the b5
        // alteration is one extension (both b and 5 take Extension).
        let spans = Highlighter::highlight_chord("F#m7b5");
        let root = spans
            .iter()
            .filter(|s| s.kind == HighlightKind::Root)
            .count();
        assert_eq!(root, 2, "F and # are both root-colored");
        let ext = spans
            .iter()
            .filter(|s| s.kind == HighlightKind::Extension)
            .count();
        assert_eq!(ext, 3, "7, b and 5 are all extension-colored");
        // Nothing falls through to the gray MeasureCount kind mid-chord.
        assert!(
            !spans.iter().any(|s| s.kind == HighlightKind::MeasureCount),
            "no uncategorized gray tokens in the chord"
        );
    }
}
