//! Section Types
//!
//! Defines different types of song sections

use super::measure_expr::MeasureExpression;
use facet::Facet;

/// Represents different types of song sections
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub enum SectionType {
    Intro,
    Verse,
    Chorus,
    Bridge,
    Outro,
    Instrumental,
    Solo,                   // Solo section (instrument specified via comment, e.g. Solo "Keys")
    CountIn,                // Count-in measures (rendered small, with whole rests)
    Opening, // Pre-Intro setup: groove-builds, fade-ins, etc. Falls before the song's INTRO marker.
    End,     // End section (from SONGEND to =END, for ring-out/fade)
    Hits,    // Hits section (rhythmic accents)
    Interlude, // Interlude section
    Breakdown, // Breakdown section
    Vamp,    // Vamp section (repeated section for improvisation/transitions)
    Refrain, // Refrain section (recurring hook, common in worship charts)
    Tag,     // Tag — a short repeat of a closing line, often several in a row (worship charts)
    Turnaround, // Turnaround — short instrumental link/transition back into a section
    Pre(Box<SectionType>), // Pre-Chorus, Pre-Verse, etc.
    Post(Box<SectionType>), // Post-Chorus, Post-Verse, etc.
    Custom(String), // Custom section types
}

/// Result of parsing a section marker line.
///
/// Contains the section type, optional measure expression, and optional comment/annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSection {
    /// The section type (Verse, Chorus, etc.)
    pub section_type: SectionType,
    /// Optional measure expression (e.g., "8", "+1", "4x4")
    pub measure_expr: Option<MeasureExpression>,
    /// Optional comment/annotation (e.g., "Down", "Build", "Horns", "Half-time")
    pub comment: Option<String>,
    /// Optional key-change token on the header line (e.g. `#G` in `BR 8 #G`),
    /// applied at the start of the section. Stored verbatim for the caller to
    /// parse with `Key::parse`.
    pub key_change: Option<String>,
}

impl ParsedSection {
    /// Create a new parsed section with just a section type.
    pub fn new(section_type: SectionType) -> Self {
        Self {
            section_type,
            measure_expr: None,
            comment: None,
            key_change: None,
        }
    }

    /// Create a parsed section with section type and measure expression.
    pub fn with_measures(
        section_type: SectionType,
        measure_expr: Option<MeasureExpression>,
    ) -> Self {
        Self {
            section_type,
            measure_expr,
            comment: None,
            key_change: None,
        }
    }

    /// Create a full parsed section with all fields.
    pub fn full(
        section_type: SectionType,
        measure_expr: Option<MeasureExpression>,
        comment: Option<String>,
    ) -> Self {
        Self {
            section_type,
            measure_expr,
            comment,
            key_change: None,
        }
    }

    /// Attach a key-change token (builder).
    #[must_use]
    pub fn with_key_change(mut self, key_change: Option<String>) -> Self {
        self.key_change = key_change;
        self
    }
}

/// Preset comment modifiers that can appear before section types.
/// e.g., "Down CH 4" -> Chorus with comment "Down"
const COMMENT_PRESETS: &[(&str, &str)] = &[
    ("down", "Down"),
    ("build", "Build"),
    ("half-time", "Half-time"),
    ("halftime", "Half-time"),
    ("double-time", "Double-time"),
    ("doubletime", "Double-time"),
    ("soft", "Soft"),
    ("loud", "Loud"),
    ("quiet", "Quiet"),
    ("big", "Big"),
    ("small", "Small"),
    ("sparse", "Sparse"),
    ("full", "Full"),
    ("stripped", "Stripped"),
    ("breakdown", "Breakdown"), // Can be both a section type and a modifier
];

impl SectionType {
    /// Get a lowercase key for this section type.
    ///
    /// Useful for caching, config lookup, or CSS class names.
    /// Returns lowercase strings like "intro", "verse", "pre_chorus".
    pub fn key(&self) -> String {
        match self {
            SectionType::Intro => "intro".to_string(),
            SectionType::Verse => "verse".to_string(),
            SectionType::Chorus => "chorus".to_string(),
            SectionType::Bridge => "bridge".to_string(),
            SectionType::Outro => "outro".to_string(),
            SectionType::Instrumental => "instrumental".to_string(),
            SectionType::Solo => "solo".to_string(),
            SectionType::CountIn => "count_in".to_string(),
            SectionType::Opening => "opening".to_string(),
            SectionType::End => "end".to_string(),
            SectionType::Hits => "hits".to_string(),
            SectionType::Interlude => "interlude".to_string(),
            SectionType::Breakdown => "breakdown".to_string(),
            SectionType::Vamp => "vamp".to_string(),
            SectionType::Refrain => "refrain".to_string(),
            SectionType::Tag => "tag".to_string(),
            SectionType::Turnaround => "turnaround".to_string(),
            SectionType::Pre(inner) => format!("pre_{}", inner.key()),
            SectionType::Post(inner) => format!("post_{}", inner.key()),
            SectionType::Custom(name) => {
                // Convert custom name to lowercase with underscores
                name.to_lowercase().replace(' ', "_")
            }
        }
    }

    /// Check if this section type should be rendered in charts.
    ///
    /// Returns false for End sections, which are typically silent/fade-out
    /// and don't need visual representation in the chart.
    pub fn should_render(&self) -> bool {
        !matches!(self, SectionType::End)
    }

    /// Get the full name of the section
    pub fn full_name(&self) -> String {
        match self {
            SectionType::Intro => "Intro".to_string(),
            SectionType::Verse => "Verse".to_string(),
            SectionType::Chorus => "Chorus".to_string(),
            SectionType::Bridge => "Bridge".to_string(),
            SectionType::Outro => "Outro".to_string(),
            SectionType::Instrumental => "Instrumental".to_string(),
            SectionType::Solo => "Solo".to_string(),
            SectionType::CountIn => "Count-In".to_string(),
            SectionType::Opening => "Opening".to_string(),
            SectionType::End => "End".to_string(),
            SectionType::Hits => "Hits".to_string(),
            SectionType::Interlude => "Interlude".to_string(),
            SectionType::Breakdown => "Breakdown".to_string(),
            SectionType::Vamp => "Vamp".to_string(),
            SectionType::Refrain => "Refrain".to_string(),
            SectionType::Tag => "Tag".to_string(),
            SectionType::Turnaround => "Turnaround".to_string(),
            SectionType::Pre(inner) => format!("Pre-{}", inner.full_name()),
            SectionType::Post(inner) => format!("Post-{}", inner.full_name()),
            SectionType::Custom(name) => name.clone(),
        }
    }

    /// Get the abbreviated name of the section
    pub fn abbreviation(&self) -> String {
        match self {
            SectionType::Intro => "IN".to_string(),
            SectionType::Verse => "VS".to_string(),
            SectionType::Chorus => "CH".to_string(),
            SectionType::Bridge => "BR".to_string(),
            SectionType::Outro => "OUT".to_string(),
            SectionType::Instrumental => "INST".to_string(),
            SectionType::Solo => "SOLO".to_string(),
            SectionType::CountIn => "COUNT".to_string(),
            SectionType::Opening => "OPEN".to_string(),
            SectionType::End => "END".to_string(),
            SectionType::Hits => "HITS".to_string(),
            SectionType::Interlude => "INT".to_string(),
            SectionType::Breakdown => "BD".to_string(),
            SectionType::Vamp => "VMP".to_string(),
            SectionType::Refrain => "REF".to_string(),
            SectionType::Tag => "TAG".to_string(),
            SectionType::Turnaround => "TURN".to_string(),
            SectionType::Pre(inner) => format!("PRE-{}", inner.abbreviation()),
            SectionType::Post(inner) => format!("POST-{}", inner.abbreviation()),
            SectionType::Custom(name) => name.clone(), // Custom sections use their full name
        }
    }

    /// Check if this section type should be numbered in charts
    pub fn should_number(&self) -> bool {
        match self {
            SectionType::Intro
            | SectionType::Outro
            | SectionType::Instrumental
            | SectionType::CountIn
            | SectionType::End => false,
            SectionType::Solo
            | SectionType::Hits
            | SectionType::Interlude
            | SectionType::Breakdown
            | SectionType::Vamp
            | SectionType::Refrain
            | SectionType::Tag
            | SectionType::Turnaround => false,
            SectionType::Pre(_) | SectionType::Post(_) => false,
            SectionType::Custom(_) => false, // Custom sections don't get numbered
            _ => true,
        }
    }

    /// Check if this section type should show a section header/label in charts
    /// CountIn and End are hidden from charts but visible in progress bars
    pub fn should_show_header(&self) -> bool {
        !matches!(self, SectionType::CountIn | SectionType::End)
    }

    /// Check if this section should use compact/small measure rendering
    pub fn is_compact(&self) -> bool {
        matches!(self, SectionType::CountIn)
    }

    /// Parse a section type from a string (name or abbreviation)
    ///
    /// Handles case-insensitive matching and common typos/variations:
    /// - "verse", "Verse", "VERSE", "vs", "VS", "vErSe", "vrse" -> Verse
    /// - "chorus", "Chorus", "CHORUS", "ch", "CH", "chorous", "corus" -> Chorus
    /// - etc.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s_lower = s.to_lowercase();
        let s_lower = s_lower.trim();

        // Try exact matches first (case-insensitive)
        match s_lower {
            // Single-letter abbreviations are intentionally NOT accepted: they
            // would shadow chord roots (C, B) and Roman numerals (I, V) on a
            // content line. Section abbreviations are two or more letters (CH,
            // VS, BR, IN, OUT, …).
            "verse" | "vs" => return Ok(SectionType::Verse),
            "chorus" | "ch" => return Ok(SectionType::Chorus),
            "bridge" | "br" => return Ok(SectionType::Bridge),
            "intro" | "in" => return Ok(SectionType::Intro),
            "opening" | "open" => return Ok(SectionType::Opening),
            "outro" | "out" => return Ok(SectionType::Outro),
            "instrumental" | "inst" | "instrument" => return Ok(SectionType::Instrumental),
            "solo" => return Ok(SectionType::Solo),
            "count" | "countin" | "count-in" => return Ok(SectionType::CountIn),
            "tag" | "tags" => return Ok(SectionType::Tag),
            "hits" | "hit" => return Ok(SectionType::Hits),
            "interlude" | "inter" | "int" => return Ok(SectionType::Interlude),
            "breakdown" | "bd" => return Ok(SectionType::Breakdown),
            "vamp" | "vmp" => return Ok(SectionType::Vamp),
            "refrain" | "ref" => return Ok(SectionType::Refrain),
            "turnaround" | "turn" => return Ok(SectionType::Turnaround),
            _ => {}
        }

        // Try fuzzy matching for common typos and variations
        // Verse variations
        if Self::fuzzy_match(s_lower, "verse", &["vrse", "verce", "vers", "versa"]) {
            return Ok(SectionType::Verse);
        }

        // Chorus variations
        if Self::fuzzy_match(
            s_lower,
            "chorus",
            &["chorous", "corus", "chrous", "chors", "chor"],
        ) {
            return Ok(SectionType::Chorus);
        }

        // Bridge variations
        if Self::fuzzy_match(s_lower, "bridge", &["bridg", "brige", "brid"]) {
            return Ok(SectionType::Bridge);
        }

        // Intro variations - handle "introduction", "intro", etc.
        // Note: "int" is NOT an intro variant - it maps to Interlude (see exact matches above)
        if Self::fuzzy_match(s_lower, "intro", &["intr", "introo", "introduction"]) {
            return Ok(SectionType::Intro);
        }
        // Also check if it starts with "introduction"
        if s_lower.starts_with("introduction") {
            return Ok(SectionType::Intro);
        }

        // Outro variations - handle "outroduction", "outro", etc.
        if Self::fuzzy_match(s_lower, "outro", &["outr", "out", "outroo", "outroduction"]) {
            return Ok(SectionType::Outro);
        }
        // Also check if it starts with "outroduction"
        if s_lower.starts_with("outroduction") {
            return Ok(SectionType::Outro);
        }

        // Instrumental variations
        if Self::fuzzy_match(
            s_lower,
            "instrumental",
            &["instumental", "instrumantal", "instrument"],
        ) {
            return Ok(SectionType::Instrumental);
        }

        // Count-in variations
        if Self::fuzzy_match(
            s_lower,
            "count",
            &["countin", "count-in", "countinn", "cnt"],
        ) {
            return Ok(SectionType::CountIn);
        }

        // Hits variations
        if Self::fuzzy_match(s_lower, "hits", &["hit", "hts"]) {
            return Ok(SectionType::Hits);
        }

        // Interlude variations
        if Self::fuzzy_match(s_lower, "interlude", &["inter", "interlud", "intrlude"]) {
            return Ok(SectionType::Interlude);
        }

        // Breakdown variations
        if Self::fuzzy_match(s_lower, "breakdown", &["brkdown", "breakdwn", "bdown"]) {
            return Ok(SectionType::Breakdown);
        }

        // Vamp variations
        if Self::fuzzy_match(s_lower, "vamp", &["vmp", "vampp"]) {
            return Ok(SectionType::Vamp);
        }

        // Try to parse Pre/Post
        if let Some(rest) = s_lower.strip_prefix("pre-")
            && let Ok(inner) = Self::parse(rest)
        {
            return Ok(SectionType::Pre(Box::new(inner)));
        }
        if let Some(rest) = s_lower.strip_prefix("post-")
            && let Ok(inner) = Self::parse(rest)
        {
            return Ok(SectionType::Post(Box::new(inner)));
        }

        Err(format!(
            "Unknown section type: '{}' - supported types: verse, chorus, bridge, intro, outro, instrumental, solo, count, hits, interlude, breakdown, vamp, pre-*, post-*",
            s
        ))
    }

    /// Fuzzy matching helper - checks if the input matches the target or any variations
    fn fuzzy_match(input: &str, target: &str, variations: &[&str]) -> bool {
        // Exact match with target
        if input == target {
            return true;
        }

        // Check if input starts with target (allows for trailing characters like numbers)
        if input.starts_with(target) {
            return true;
        }

        // Check variations
        for variation in variations {
            if input == *variation || input.starts_with(variation) {
                return true;
            }
        }

        // Check if input is close enough to target (simple edit distance check)
        // For very short strings, just check if first few chars match
        if input.len() >= 3 && target.len() >= 3 {
            let input_prefix = &input[..input.len().min(3)];
            let target_prefix = &target[..target.len().min(3)];
            if input_prefix == target_prefix {
                return true;
            }
        }

        false
    }

    /// Parse a section marker from input (for chart parsing)
    ///
    /// Supports:
    /// - Standard sections: "VS 16", "Intro 4", etc.
    /// - Custom sections with brackets: "[Hits]", "[SOLO Keys] 8", etc.
    /// - Expressions: "VS 8+1", "VS 4x4", "VS +1", "VS -1"
    /// - Quoted comments: `CH 4 "Down"`, `VS 8 "Build"`, `Interlude "Horns"`
    /// - Preset modifiers: `Down CH 4`, `Build VS 8`
    ///
    /// Returns a `ParsedSection` containing the section type, measure expression, and optional comment.
    pub fn parse_with_measure_count(input: &str) -> Option<ParsedSection> {
        let input = input.trim();

        // Peel a trailing key-change token (`BR 8 #G`) so the section header is
        // still recognized; the caller applies it at the section start.
        let (input, key_change) = extract_trailing_key_change(input);
        let input = input.trim();

        // First, extract any quoted comment at the end: CH 4 "Down"
        let (input_without_quote, quoted_comment) = extract_quoted_comment(input);
        let input = input_without_quote.trim();

        // Extract parenthetical comment: In (Drums), CH (Build), etc.
        let (input_owned, paren_comment) = extract_paren_comment(input);
        let input = input_owned.trim();

        // Check for preset modifier at the start: "Down CH 4"
        let (preset_comment, remaining_input) = extract_preset_modifier(input);

        // Priority: quoted > parenthetical > preset
        let comment = quoted_comment.or(paren_comment).or(preset_comment);
        let input = remaining_input;

        // Check for custom section with brackets: [Hits] or [SOLO Keys] 8
        if input.starts_with('[') && input.contains(']') {
            // Find the closing bracket
            if let Some(close_bracket_idx) = input[1..].find(']') {
                let name = &input[1..close_bracket_idx + 1]; // Extract name between brackets

                // Exclude track markers - these are not sections
                let name_lower = name.to_lowercase();
                let first_word = name_lower.split_whitespace().next().unwrap_or("");
                if ["chords", "melody", "rhythm", "lyrics"].contains(&first_word) {
                    return None; // This is a track marker, not a section
                }

                let remaining = input[close_bracket_idx + 2..].trim();

                // Parse measure expression if present
                let measure_expr = if remaining.is_empty() {
                    None
                } else {
                    // Must be a valid expression, otherwise it's not a section marker
                    Some(MeasureExpression::parse(remaining)?)
                };

                return Some(
                    ParsedSection::full(
                        SectionType::Custom(name.to_string()),
                        measure_expr,
                        comment,
                    )
                    .with_key_change(key_change),
                );
            }
        }

        // Parse standard sections (case-insensitive)
        let input_lower = input.to_lowercase();
        let parts: Vec<&str> = input_lower.split_whitespace().collect();

        if parts.is_empty() {
            return None;
        }

        // Handle Solo sections with instrument names.
        // Supports both orderings:
        //   "SOLO Keys"      → Solo with comment "Keys"
        //   "SOLO Keys 8"    → Solo with comment "Keys", 8 measures
        //   "Keys SOLO"      → Solo with comment "Keys"
        //   "Keys SOLO 8"    → Solo with comment "Keys", 8 measures
        //   "GUITAR SOLO"    → Solo with comment "Guitar"
        //   "GUITAR SOLO 8"  → Solo with comment "Guitar", 8 measures
        //   "SOLO 8"         → Solo with 8 measures (no instrument)
        //   "SOLO"           → Solo (no instrument, no measures)
        // Note: "SOLO \"Keys\"" is handled by the quoted comment extraction above.
        if let Some(solo_result) = parse_solo_section(&parts, comment.clone()) {
            return Some(solo_result.with_key_change(key_change));
        }

        let section_str = parts[0];

        // Section markers should be alone or followed by a sub-label and/or measure count.
        // This prevents lines like "c d g" from being parsed as a section marker.
        //
        // Supported patterns:
        //   1 token:  "CH"           → section only
        //   2 tokens: "CH 10"        → section + measure count
        //             "CH 3A"        → section + sub-label (no measure count)
        //   3 tokens: "CH 3A 10"     → section + sub-label + measure count
        //             "Interlude A 8" → section + sub-label + measure count
        if parts.len() > 3 {
            return None; // Too many tokens, not a section marker
        }

        let measure_expr;
        let sub_label_comment;

        if parts.len() == 3 {
            // Three tokens: first must be section type, second is sub-label, third is measure expr
            if !is_sub_label(parts[1]) && parts[1].parse::<u32>().is_err() {
                return None; // Second token is not a valid sub-label
            }
            // Third token must be a valid measure expression
            measure_expr = Some(MeasureExpression::parse(parts[2])?);
            sub_label_comment = Some(parts[1].to_string());
        } else if parts.len() == 2 {
            // Two tokens: second is either a measure expression or a sub-label
            if let Some(expr) = MeasureExpression::parse(parts[1]) {
                measure_expr = Some(expr);
                sub_label_comment = None;
            } else if is_sub_label(parts[1]) {
                // Not a valid measure expression — treat as sub-label
                measure_expr = None;
                sub_label_comment = Some(parts[1].to_string());
            } else {
                return None; // Invalid second token
            }
        } else {
            measure_expr = None;
            sub_label_comment = None;
        };

        // Merge sub-label into comment (prefer quoted/preset comment if present)
        let comment = comment.or(sub_label_comment);

        // Resolve the base type, then a `pre-`/`post-` prefix wrapping it, so
        // `Pre-Chorus`, `PRE-CH`, `post-verse`, … all parse (not just bare `pre`).
        let section_type = base_section_type(section_str)
            .or_else(|| {
                section_str
                    .strip_prefix("pre-")
                    .and_then(base_section_type)
                    .map(|inner| SectionType::Pre(Box::new(inner)))
            })
            .or_else(|| {
                section_str
                    .strip_prefix("post-")
                    .and_then(base_section_type)
                    .map(|inner| SectionType::Post(Box::new(inner)))
            });

        section_type
            .map(|st| ParsedSection::full(st, measure_expr, comment).with_key_change(key_change))
    }
}

/// Resolve a single section-marker token (already lowercased) to its base
/// `SectionType`, with no `pre-`/`post-` wrapping. Single-letter abbreviations
/// are intentionally rejected (see the note in [`SectionType::parse`]) so they
/// don't shadow chord roots or Roman numerals on a content line.
fn base_section_type(token: &str) -> Option<SectionType> {
    match token {
        "intro" | "in" => Some(SectionType::Intro),
        "opening" | "open" => Some(SectionType::Opening),
        "verse" | "vs" => Some(SectionType::Verse),
        "chorus" | "ch" => Some(SectionType::Chorus),
        "bridge" | "br" => Some(SectionType::Bridge),
        "outro" | "out" => Some(SectionType::Outro),
        "instrumental" | "inst" => Some(SectionType::Instrumental),
        "count" | "countin" | "count-in" => Some(SectionType::CountIn),
        "tag" | "tags" => Some(SectionType::Tag),
        "hits" | "hit" => Some(SectionType::Hits),
        "interlude" | "inter" | "int" => Some(SectionType::Interlude),
        "breakdown" | "bd" => Some(SectionType::Breakdown),
        "vamp" | "vmp" => Some(SectionType::Vamp),
        "refrain" | "ref" => Some(SectionType::Refrain),
        "turnaround" | "turn" => Some(SectionType::Turnaround),
        _ => None,
    }
}

/// Peel a trailing key-change token (`#G`, `bBb`) off a section-header line.
/// Only accidental-prefixed tokens that parse as a key are stripped, so
/// measure counts and sub-labels are never mistaken for a key.
fn extract_trailing_key_change(input: &str) -> (&str, Option<String>) {
    if let Some((head, last)) = input.rsplit_once(char::is_whitespace)
        && (last.starts_with('#') || last.starts_with('b'))
        && last.len() >= 2
        && crate::key::Key::parse(last).is_ok()
    {
        return (head.trim_end(), Some(last.to_string()));
    }
    (input, None)
}

/// Parse a Solo section from whitespace-split parts.
///
/// Handles both "SOLO <instrument>" and "<instrument> SOLO" orderings,
/// with optional measure count. The `existing_comment` is a pre-extracted
/// quoted or preset comment that takes precedence over instrument names.
///
/// Returns `Some(ParsedSection)` if this is a Solo section, `None` otherwise.
fn parse_solo_section(parts: &[&str], existing_comment: Option<String>) -> Option<ParsedSection> {
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }

    let solo_idx = parts.iter().position(|&p| p == "solo")?;

    // Determine instrument name and measure expression based on "solo" position
    let (instrument, measure_token) = match (solo_idx, parts.len()) {
        // "solo" — just Solo, no instrument
        (0, 1) => (None, None),
        // "solo 8" — Solo with measure count, OR "solo keys" — Solo with instrument
        (0, 2) => {
            if MeasureExpression::parse(parts[1]).is_some() {
                (None, Some(parts[1]))
            } else {
                (Some(parts[1]), None)
            }
        }
        // "solo keys 8" — Solo + instrument + measure count
        (0, 3) => (Some(parts[1]), Some(parts[2])),
        // "keys solo" — reversed, instrument first
        (1, 2) => (Some(parts[0]), None),
        // "keys solo 8" — reversed + measure count
        (1, 3) => (Some(parts[0]), Some(parts[2])),
        // "8 keys solo" — doesn't make sense
        _ => return None,
    };

    let measure_expr = match measure_token {
        Some(token) => Some(MeasureExpression::parse(token)?),
        None => None,
    };

    // Use existing comment (from quoted string or preset) if available,
    // otherwise use instrument name with title case
    let comment = existing_comment.or_else(|| {
        instrument.map(|name| {
            let mut chars = name.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
                None => String::new(),
            }
        })
    });

    Some(ParsedSection::full(
        SectionType::Solo,
        measure_expr,
        comment,
    ))
}

/// Check if a token is a valid section sub-label.
///
/// Sub-labels are short identifiers like "a", "b", "3a", "3b" used for split sections.
/// They must be 1-3 characters long and end with a single letter (a-z).
/// Examples: "a", "b", "3a", "3b", "2c"
/// Non-examples: "abc", "10", "3ab", "hello"
fn is_sub_label(s: &str) -> bool {
    if s.is_empty() || s.len() > 3 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    // Last character must be a single letter
    let last = *chars.last().unwrap();
    if !last.is_ascii_alphabetic() {
        return false;
    }
    // All preceding characters must be digits
    chars[..chars.len() - 1].iter().all(|c| c.is_ascii_digit())
}

/// Extract a quoted comment from the end of the input.
/// Returns (input without quote, optional comment)
/// Example: `CH 4 "Down"` -> (`CH 4`, Some("Down"))
/// Example: `Interlude "Horns"` -> (`Interlude`, Some("Horns"))
/// Extract a parenthetical comment from the input.
///
/// Examples:
/// - `In (Drums)` → (`In`, Some("Drums"))
/// - `CH (Build) 8` → (`CH  8`, Some("Build"))
/// - `In (Band) x2` → (`In  x2`, Some("Band"))
fn extract_paren_comment(input: &str) -> (String, Option<String>) {
    if let Some(open) = input.find('(')
        && let Some(close) = input[open..].find(')')
    {
        let close = open + close;
        let comment = input[open + 1..close].trim().to_string();
        let remaining = format!("{}{}", &input[..open], &input[close + 1..]);
        if !comment.is_empty() {
            return (remaining, Some(comment));
        }
    }
    (input.to_string(), None)
}

fn extract_quoted_comment(input: &str) -> (String, Option<String>) {
    // Look for a quoted string anywhere (the LAST quoted run).
    if let Some(last_quote) = input.rfind('"') {
        // Find the opening quote
        let before_last = &input[..last_quote];
        if let Some(open_quote) = before_last.rfind('"') {
            let comment = input[open_quote + 1..last_quote].trim().to_string();
            // Keep BOTH the text before the opening quote AND after the closing
            // quote — a quoted label can sit in the MIDDLE of a marker, e.g.
            // `Interlude "Breakdown" 8`, and the trailing measure count `8`
            // must survive (it was previously dropped).
            let before = input[..open_quote].trim();
            let after = input[last_quote + 1..].trim();
            let remaining = match (before.is_empty(), after.is_empty()) {
                (true, _) => after.to_string(),
                (_, true) => before.to_string(),
                _ => format!("{before} {after}"),
            };
            if !comment.is_empty() {
                return (remaining, Some(comment));
            }
        }
    }
    (input.to_string(), None)
}

/// Extract a preset modifier from the start of the input.
/// Returns (optional comment, remaining input)
/// Example: `Down CH 4` -> (Some("Down"), `CH 4`)
fn extract_preset_modifier(input: &str) -> (Option<String>, &str) {
    let input_lower = input.to_lowercase();

    for (preset_lower, preset_display) in COMMENT_PRESETS {
        // Check if input starts with this preset followed by a space
        if input_lower.starts_with(preset_lower) {
            let after_preset = &input[preset_lower.len()..];
            if after_preset.starts_with(' ') || after_preset.starts_with('\t') {
                let remaining = after_preset.trim_start();
                // Make sure the remaining part could be a section
                // (has at least one more token that looks like a section type)
                let first_word = remaining.split_whitespace().next().unwrap_or("");
                let first_word_lower = first_word.to_lowercase();
                let could_be_section = matches!(
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
                        | "i"
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
                ) || first_word.starts_with('[');

                if could_be_section {
                    return (Some((*preset_display).to_string()), remaining);
                }
            }
        }
    }
    (None, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_type_names() {
        assert_eq!(SectionType::Verse.full_name(), "Verse");
        assert_eq!(SectionType::Chorus.full_name(), "Chorus");
        assert_eq!(SectionType::Bridge.full_name(), "Bridge");
        assert_eq!(SectionType::Intro.full_name(), "Intro");
        assert_eq!(SectionType::Outro.full_name(), "Outro");
    }

    #[test]
    fn test_section_type_abbreviations() {
        assert_eq!(SectionType::Verse.abbreviation(), "VS");
        assert_eq!(SectionType::Chorus.abbreviation(), "CH");
        assert_eq!(SectionType::Bridge.abbreviation(), "BR");
        assert_eq!(SectionType::Intro.abbreviation(), "IN");
        assert_eq!(SectionType::Outro.abbreviation(), "OUT");
        assert_eq!(SectionType::Instrumental.abbreviation(), "INST");
    }

    #[test]
    fn test_pre_post_sections() {
        let pre_chorus = SectionType::Pre(Box::new(SectionType::Chorus));
        assert_eq!(pre_chorus.full_name(), "Pre-Chorus");
        assert_eq!(pre_chorus.abbreviation(), "PRE-CH");

        let post_chorus = SectionType::Post(Box::new(SectionType::Chorus));
        assert_eq!(post_chorus.full_name(), "Post-Chorus");
        assert_eq!(post_chorus.abbreviation(), "POST-CH");
    }

    #[test]
    fn test_should_number() {
        assert!(SectionType::Verse.should_number());
        assert!(SectionType::Chorus.should_number());
        assert!(SectionType::Bridge.should_number());

        assert!(!SectionType::Intro.should_number());
        assert!(!SectionType::Outro.should_number());
        assert!(!SectionType::Instrumental.should_number());
        assert!(!SectionType::Pre(Box::new(SectionType::Chorus)).should_number());
        assert!(!SectionType::Post(Box::new(SectionType::Chorus)).should_number());
    }

    #[test]
    fn test_parse_section_markers() {
        assert_eq!(
            SectionType::parse_with_measure_count("vs 4"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Absolute(4))
            ))
        );
        assert_eq!(
            SectionType::parse_with_measure_count("ch 8"),
            Some(ParsedSection::with_measures(
                SectionType::Chorus,
                Some(MeasureExpression::Absolute(8))
            ))
        );
        assert_eq!(
            SectionType::parse_with_measure_count("intro 2"),
            Some(ParsedSection::with_measures(
                SectionType::Intro,
                Some(MeasureExpression::Absolute(2))
            ))
        );
        assert_eq!(
            SectionType::parse_with_measure_count("br"),
            Some(ParsedSection::with_measures(SectionType::Bridge, None))
        );
        // Turnaround: both "turn" and "turnaround" parse, with a measure count.
        assert_eq!(
            SectionType::parse_with_measure_count("Turn 2"),
            Some(ParsedSection::with_measures(
                SectionType::Turnaround,
                Some(MeasureExpression::Absolute(2))
            ))
        );
        assert_eq!(
            SectionType::parse_with_measure_count("turnaround 4"),
            Some(ParsedSection::with_measures(
                SectionType::Turnaround,
                Some(MeasureExpression::Absolute(4))
            ))
        );
    }

    #[test]
    fn test_parse_expressions() {
        // Addition expression
        assert_eq!(
            SectionType::parse_with_measure_count("vs 8+1"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Absolute(9))
            ))
        );

        // Subtraction expression
        assert_eq!(
            SectionType::parse_with_measure_count("vs 8-1"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Absolute(7))
            ))
        );

        // Multiplication expression
        assert_eq!(
            SectionType::parse_with_measure_count("vs 4x4"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Absolute(16))
            ))
        );

        // Relative add
        assert_eq!(
            SectionType::parse_with_measure_count("vs +1"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Add(1))
            ))
        );

        // Relative subtract
        assert_eq!(
            SectionType::parse_with_measure_count("vs -1"),
            Some(ParsedSection::with_measures(
                SectionType::Verse,
                Some(MeasureExpression::Subtract(1))
            ))
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert_eq!(SectionType::parse_with_measure_count("invalid"), None);
        assert_eq!(SectionType::parse_with_measure_count(""), None);
        // Invalid expression should cause parse to fail
        assert_eq!(SectionType::parse_with_measure_count("vs abc"), None);
    }

    #[test]
    fn test_single_letter_abbrevs_are_not_sections() {
        // Single-letter section abbreviations are intentionally rejected so they
        // don't shadow chord roots (C, B) or Roman numerals (I, V) on a content
        // line. A line like `C G` must parse as chords, not "Chorus / g".
        for s in ["c", "C", "b", "B", "v", "V", "i", "I", "o", "O"] {
            assert_eq!(
                SectionType::parse(s).ok(),
                None,
                "single letter {s:?} should not parse as a section type"
            );
            assert_eq!(
                SectionType::parse_with_measure_count(s),
                None,
                "single letter {s:?} should not parse as a section marker"
            );
        }
        // `C G` (chord-root + chord-like token) must not be grabbed as a section.
        assert_eq!(SectionType::parse_with_measure_count("C G"), None);
    }

    #[test]
    fn test_pre_post_section_headers() {
        let pre_chorus = SectionType::Pre(Box::new(SectionType::Chorus));
        for s in ["Pre-Chorus 2", "PRE-CH 2", "pre-ch 2", "pre-chorus 2"] {
            let parsed = SectionType::parse_with_measure_count(s)
                .unwrap_or_else(|| panic!("{s:?} should parse"));
            assert_eq!(parsed.section_type, pre_chorus, "{s}");
            assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(2)));
        }
        assert_eq!(
            SectionType::parse_with_measure_count("pre-verse 4").map(|p| p.section_type),
            Some(SectionType::Pre(Box::new(SectionType::Verse)))
        );
        assert_eq!(
            SectionType::parse_with_measure_count("Post-Chorus 8").map(|p| p.section_type),
            Some(SectionType::Post(Box::new(SectionType::Chorus)))
        );

        // Two-letter abbreviations still work.
        assert_eq!(
            SectionType::parse_with_measure_count("ch").map(|p| p.section_type),
            Some(SectionType::Chorus)
        );
        assert_eq!(
            SectionType::parse_with_measure_count("vs 8").map(|p| p.section_type),
            Some(SectionType::Verse)
        );
        assert_eq!(SectionType::parse("br").ok(), Some(SectionType::Bridge));
    }

    #[test]
    fn test_parse_sub_labels() {
        // Sub-label with measure count: "CH 3A 10" → Chorus, comment "3a", measure count 10
        let parsed = SectionType::parse_with_measure_count("CH 3A 10").unwrap();
        assert_eq!(parsed.section_type, SectionType::Chorus);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(10)));
        assert_eq!(parsed.comment, Some("3a".to_string()));

        // Sub-label with measure count: "CH 3B 8"
        let parsed = SectionType::parse_with_measure_count("CH 3B 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Chorus);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("3b".to_string()));

        // Interlude with single-letter sub-label: "Interlude A 8"
        let parsed = SectionType::parse_with_measure_count("Interlude A 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Interlude);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("a".to_string()));

        // Interlude with sub-label only (no measure count): "Interlude B"
        let parsed = SectionType::parse_with_measure_count("Interlude B").unwrap();
        assert_eq!(parsed.section_type, SectionType::Interlude);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("b".to_string()));

        // Sub-label should NOT match long strings like "abc"
        assert_eq!(SectionType::parse_with_measure_count("vs abc"), None);

        // Four tokens should still be rejected
        assert_eq!(
            SectionType::parse_with_measure_count("ch 3a 10 extra"),
            None
        );
    }

    #[test]
    fn test_parse_sub_labels_with_quoted_comment() {
        // Sub-label + quoted comment: 'Interlude B 8 "HORNS"'
        let parsed = SectionType::parse_with_measure_count(r#"Interlude B 8 "HORNS""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Interlude);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        // Quoted comment takes precedence over sub-label
        assert_eq!(parsed.comment, Some("HORNS".to_string()));
    }

    #[test]
    fn test_parse_custom_sections() {
        // Custom section with brackets
        assert_eq!(
            SectionType::parse_with_measure_count("[Hits]"),
            Some(ParsedSection::with_measures(
                SectionType::Custom("Hits".to_string()),
                None
            ))
        );

        // Custom section with brackets and measure count
        assert_eq!(
            SectionType::parse_with_measure_count("[SOLO Keys] 8"),
            Some(ParsedSection::with_measures(
                SectionType::Custom("SOLO Keys".to_string()),
                Some(MeasureExpression::Absolute(8))
            ))
        );

        // Custom section with brackets, no measure count
        assert_eq!(
            SectionType::parse_with_measure_count("[Bridge Out]"),
            Some(ParsedSection::with_measures(
                SectionType::Custom("Bridge Out".to_string()),
                None
            ))
        );

        // Custom section with expression
        assert_eq!(
            SectionType::parse_with_measure_count("[SOLO Keys] 4x2"),
            Some(ParsedSection::with_measures(
                SectionType::Custom("SOLO Keys".to_string()),
                Some(MeasureExpression::Absolute(8))
            ))
        );
    }

    #[test]
    fn test_parse_quoted_comments() {
        // Section with quoted comment
        let parsed = SectionType::parse_with_measure_count(r#"ch 4 "Down""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Chorus);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(4)));
        assert_eq!(parsed.comment, Some("Down".to_string()));

        // Section with quoted comment and no measure count
        let parsed = SectionType::parse_with_measure_count(r#"vs "Build""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Verse);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Build".to_string()));

        // Interlude with quoted comment (like from REAPER region)
        let parsed = SectionType::parse_with_measure_count(r#"interlude "Horns""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Interlude);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Horns".to_string()));
    }

    #[test]
    fn test_parse_preset_modifiers() {
        // Down chorus
        let parsed = SectionType::parse_with_measure_count("Down ch 4").unwrap();
        assert_eq!(parsed.section_type, SectionType::Chorus);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(4)));
        assert_eq!(parsed.comment, Some("Down".to_string()));

        // Build verse
        let parsed = SectionType::parse_with_measure_count("Build vs 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Verse);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("Build".to_string()));

        // Half-time bridge
        let parsed = SectionType::parse_with_measure_count("Half-time br 4").unwrap();
        assert_eq!(parsed.section_type, SectionType::Bridge);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(4)));
        assert_eq!(parsed.comment, Some("Half-time".to_string()));
    }

    #[test]
    fn test_custom_section_names() {
        let hits = SectionType::Custom("Hits".to_string());
        assert_eq!(hits.full_name(), "Hits");
        assert_eq!(hits.abbreviation(), "Hits");
        assert!(!hits.should_number());
    }

    #[test]
    fn test_section_type_keys() {
        assert_eq!(SectionType::Intro.key(), "intro");
        assert_eq!(SectionType::Verse.key(), "verse");
        assert_eq!(SectionType::Chorus.key(), "chorus");
        assert_eq!(SectionType::Bridge.key(), "bridge");
        assert_eq!(SectionType::Outro.key(), "outro");
        assert_eq!(SectionType::Instrumental.key(), "instrumental");
        assert_eq!(SectionType::Solo.key(), "solo");
        assert_eq!(SectionType::CountIn.key(), "count_in");
        assert_eq!(SectionType::End.key(), "end");

        // Pre/Post sections
        let pre_chorus = SectionType::Pre(Box::new(SectionType::Chorus));
        assert_eq!(pre_chorus.key(), "pre_chorus");

        let post_verse = SectionType::Post(Box::new(SectionType::Verse));
        assert_eq!(post_verse.key(), "post_verse");

        // Custom sections
        let custom = SectionType::Custom("SOLO Keys".to_string());
        assert_eq!(custom.key(), "solo_keys");
    }

    #[test]
    fn test_parse_solo_sections() {
        // "SOLO" alone
        let parsed = SectionType::parse_with_measure_count("SOLO").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, None);

        // "SOLO 8" — Solo with measure count
        let parsed = SectionType::parse_with_measure_count("SOLO 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, None);

        // "SOLO Keys" — Solo with instrument name
        let parsed = SectionType::parse_with_measure_count("SOLO Keys").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Keys".to_string()));

        // "SOLO Keys 8" — Solo with instrument + measure count
        let parsed = SectionType::parse_with_measure_count("SOLO Keys 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("Keys".to_string()));

        // "Keys SOLO" — reversed syntax
        let parsed = SectionType::parse_with_measure_count("Keys SOLO").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Keys".to_string()));

        // "GUITAR SOLO" — reversed with different instrument
        let parsed = SectionType::parse_with_measure_count("GUITAR SOLO").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Guitar".to_string()));

        // "Guitar SOLO 8" — reversed with measure count
        let parsed = SectionType::parse_with_measure_count("Guitar SOLO 8").unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("Guitar".to_string()));

        // 'SOLO "Keys"' — quoted instrument name
        let parsed = SectionType::parse_with_measure_count(r#"SOLO "Keys""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, None);
        assert_eq!(parsed.comment, Some("Keys".to_string()));

        // 'SOLO 8 "Keys"' — measure count + quoted instrument (standard ordering)
        let parsed = SectionType::parse_with_measure_count(r#"SOLO 8 "Keys""#).unwrap();
        assert_eq!(parsed.section_type, SectionType::Solo);
        assert_eq!(parsed.measure_expr, Some(MeasureExpression::Absolute(8)));
        assert_eq!(parsed.comment, Some("Keys".to_string()));
    }

    #[test]
    fn test_solo_section_properties() {
        assert_eq!(SectionType::Solo.key(), "solo");
        assert_eq!(SectionType::Solo.full_name(), "Solo");
        assert_eq!(SectionType::Solo.abbreviation(), "SOLO");
        assert!(!SectionType::Solo.should_number());
        assert!(SectionType::Solo.should_render());
        assert!(SectionType::Solo.should_show_header());
    }

    #[test]
    fn test_should_render() {
        assert!(SectionType::Verse.should_render());
        assert!(SectionType::Chorus.should_render());
        assert!(SectionType::CountIn.should_render()); // CountIn is rendered (as compact)
        assert!(!SectionType::End.should_render()); // End is not rendered
    }
}
