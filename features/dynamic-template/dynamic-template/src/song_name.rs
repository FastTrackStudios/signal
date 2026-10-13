//! Song/project name detection and stripping
//!
//! Detects common strings across all track names that are likely song/project names
//! and strips them from display names.

use std::collections::{HashMap, HashSet};

/// Minimum token length to consider as a potential song name
const MIN_TOKEN_LENGTH: usize = 3;

/// Configuration for song name detection
#[derive(Clone, Debug)]
pub struct SongNameConfig {
    /// Minimum percentage of files that must contain the token (0.0 to 1.0)
    /// Default: 1.0 (100% - token must appear in ALL files)
    pub threshold: f32,

    /// Enable case-insensitive fuzzy matching
    /// When true, "`TheTrooper`" and "Trooper" are considered equivalent
    /// Default: true
    pub fuzzy_match: bool,
}

impl Default for SongNameConfig {
    fn default() -> Self {
        Self {
            threshold: 1.0,    // 100% - must appear in all files
            fuzzy_match: true, // Case-insensitive fuzzy matching enabled
        }
    }
}

impl SongNameConfig {
    /// Create config with custom threshold (0.0 to 1.0)
    #[must_use]
    pub const fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Enable or disable fuzzy matching
    #[must_use]
    pub const fn with_fuzzy_match(mut self, enabled: bool) -> Self {
        self.fuzzy_match = enabled;
        self
    }
}

/// Patterns that should NOT be considered song names even if they appear in all files
/// These are common audio production terms, instrument names, and group names
const EXCLUDED_PATTERNS: &[&str] = &[
    // File extensions and formats
    "wav",
    "aiff",
    "flac",
    "mp3",
    "ogg",
    "aif",
    // Common track numbering
    "01",
    "02",
    "03",
    "04",
    "05",
    "06",
    "07",
    "08",
    "09",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
    "17",
    "18",
    "19",
    "20",
    // Common audio terms
    "mix",
    "master",
    "stem",
    "track",
    "audio",
    "final",
    "bounce",
    // Recording identifiers
    "take",
    "pass",
    "rec",
    "recording",
    // Channel identifiers
    "mono",
    "stereo",
    "left",
    "right",
    // Version markers
    "v1",
    "v2",
    "v3",
    "ver",
    "version",
    // Instrument/group names - these are NOT song names
    "drums",
    "drum",
    "kick",
    "snare",
    "hat",
    "hihat",
    "tom",
    "toms",
    "cymbal",
    "cymbals",
    "overhead",
    "room",
    "rooms",
    "bass",
    "guitar",
    "guitars",
    "gtr",
    "acoustic",
    "electric",
    "amp",
    "di",
    "clean",
    "dirty",
    "crunch",
    "distorted",
    "overdrive",
    "rhythm",
    "rhy",
    "solo",
    "keys",
    "keyboard",
    "piano",
    "organ",
    "synth",
    "synths",
    "vocal",
    "vocals",
    "vox",
    "voc",
    "lead",
    "bgv",
    "bgvs",
    "backing",
    "background",
    "choir",
    "strings",
    "brass",
    "horns",
    "orchestra",
    "percussion",
    "perc",
    "sfx",
    "fx",
    "effects",
    // Common metadata terms
    "main",
    "dbl",
    "double",
    "triple",
    "chorus",
    "verse",
    "intro",
    "outro",
    "bridge",
    "harmony",
    "harm",
    "unison",
    // Layer/position terms
    "top",
    "bottom",
    "close",
    "far",
    "near",
    "mid",
    "side",
    // Common performer names should NOT be excluded - they could be song names
];

/// Detect potential song/project names from a collection of track names
///
/// Returns a set of strings that appear in the configured percentage of input names
/// and are likely song titles. Uses default config (100% threshold, fuzzy matching enabled).
///
/// # Example
///
/// ```
/// use dynamic_template::song_name::detect_song_names;
///
/// let inputs: Vec<String> = vec![
///     "08-Kick-TheTrooper.wav".to_string(),
///     "09-Snare-TheTrooper.wav".to_string(),
///     "15-Rhy Gtr L-TheTrooper.wav".to_string(),
/// ];
/// let song_names = detect_song_names(&inputs);
/// assert!(song_names.contains("TheTrooper") || song_names.contains("thetrooper"));
/// ```
#[must_use]
pub fn detect_song_names(inputs: &[String]) -> HashSet<String> {
    detect_song_names_with_config(inputs, &SongNameConfig::default())
}

/// Detect potential song/project names with custom configuration
///
/// # Arguments
/// * `inputs` - Collection of track/file names to analyze
/// * `config` - Configuration for detection behavior
///
/// # Returns
/// Set of detected song name tokens (lowercase if fuzzy matching enabled)
#[must_use]
pub fn detect_song_names_with_config(
    inputs: &[String],
    config: &SongNameConfig,
) -> HashSet<String> {
    if inputs.is_empty() {
        return HashSet::new();
    }

    let total_inputs = inputs.len();
    // Safe: threshold is clamped to [0.0, 1.0], so result is guaranteed in [0, total_inputs]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        clippy::cast_precision_loss
    )]
    let min_count = (total_inputs as f32 * config.threshold).ceil() as usize;

    if config.fuzzy_match {
        detect_with_fuzzy_matching(inputs, min_count)
    } else {
        detect_exact_matching(inputs, min_count)
    }
}

/// Detect song names with exact token matching
fn detect_exact_matching(inputs: &[String], min_count: usize) -> HashSet<String> {
    let mut token_counts: HashMap<String, usize> = HashMap::new();

    for input in inputs {
        let tokens: HashSet<String> = tokenize(input).into_iter().collect();
        for token in tokens {
            token_counts
                .entry(token)
                .and_modify(|count: &mut usize| *count = count.saturating_add(1))
                .or_insert(1);
        }
    }

    filter_song_names(token_counts, min_count, false)
}

/// Detect song names with case-insensitive fuzzy matching
///
/// This normalizes all tokens to lowercase and also checks for containment
/// (e.g., "`TheTrooper`" contains "Trooper", so they're grouped together)
fn detect_with_fuzzy_matching(inputs: &[String], min_count: usize) -> HashSet<String> {
    // First pass: collect all normalized tokens per input
    let mut all_normalized_tokens: Vec<HashSet<String>> = Vec::new();

    for input in inputs {
        let tokens: HashSet<String> = tokenize(input)
            .into_iter()
            .map(|t| t.to_lowercase())
            .collect();
        all_normalized_tokens.push(tokens);
    }

    // Count occurrences of each normalized token
    let mut token_counts: HashMap<String, usize> = HashMap::new();
    for tokens in &all_normalized_tokens {
        for token in tokens {
            token_counts
                .entry(token.clone())
                .and_modify(|count: &mut usize| *count = count.saturating_add(1))
                .or_insert(1);
        }
    }

    // Also check for containment relationships
    // If "thetrooper" appears in some files and "trooper" in others,
    // count them together under the shorter/base form
    let all_tokens: Vec<String> = token_counts.keys().cloned().collect();
    let mut containment_groups: HashMap<String, HashSet<String>> = HashMap::new();

    for token in &all_tokens {
        // Find tokens that contain this one or that this one contains
        for other in &all_tokens {
            if token != other
                && token.len() >= MIN_TOKEN_LENGTH
                && other.len() >= MIN_TOKEN_LENGTH
                && (other.contains(token.as_str()) || token.contains(other.as_str()))
            {
                // Group under the shorter token (base form)
                let base = if token.len() <= other.len() {
                    token
                } else {
                    other
                };
                containment_groups
                    .entry(base.clone())
                    .or_default()
                    .insert(token.clone());
                containment_groups
                    .entry(base.clone())
                    .or_default()
                    .insert(other.clone());
            }
        }
    }

    // For each containment group, count inputs that have ANY of the group members
    for (base, group) in &containment_groups {
        let inputs_with_group = all_normalized_tokens
            .iter()
            .filter(|tokens| group.iter().any(|t| tokens.contains(t)))
            .count();
        // Update the base token count to reflect the group
        token_counts.insert(base.clone(), inputs_with_group);
    }

    filter_song_names(token_counts, min_count, true)
}

/// Filter token counts to find likely song names
fn filter_song_names(
    token_counts: HashMap<String, usize>,
    min_count: usize,
    is_lowercase: bool,
) -> HashSet<String> {
    let mut song_names = HashSet::new();

    for (token, count) in token_counts {
        // Must meet threshold
        if count < min_count {
            continue;
        }

        // Must be long enough
        if token.len() < MIN_TOKEN_LENGTH {
            continue;
        }

        // Must not be an excluded pattern
        let lower = if is_lowercase {
            token.clone()
        } else {
            token.to_lowercase()
        };
        if EXCLUDED_PATTERNS.contains(&lower.as_str()) {
            continue;
        }

        // Must not be purely numeric
        if token.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        song_names.insert(token);
    }

    song_names
}

/// Tokenize a string into words/tokens
///
/// Splits on common delimiters: space, dash, underscore, dot, slash
fn tokenize(input: &str) -> Vec<String> {
    input
        .split([' ', '-', '_', '.', '/', '\\'])
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

/// Strip detected song names from a string
///
/// Removes the song name tokens while preserving the rest of the string structure.
/// Handles case-insensitive matching and containment (e.g., "trooper" matches "`TheTrooper`").
#[must_use]
pub fn strip_song_names<S: std::hash::BuildHasher>(
    input: &str,
    song_names: &HashSet<String, S>,
) -> String {
    if song_names.is_empty() {
        return input.to_string();
    }

    let mut result = input.to_string();

    for song_name in song_names {
        // Case-insensitive matching
        let _lower_result = result.to_lowercase();
        let lower_song = song_name.to_lowercase();

        // Find any token in the input that contains the song name (for fuzzy matching)
        // This handles "TheTrooper" when we detected "trooper"
        let tokens = tokenize(&result);
        for token in tokens {
            let lower_token = token.to_lowercase();
            // Check if this token matches or contains the song name
            if lower_token == lower_song || lower_token.contains(&lower_song) {
                // Remove this token from the result
                if let Some(idx) = result.find(&token) {
                    let before = result.get(..idx).unwrap_or_default();
                    let after = idx
                        .checked_add(token.len())
                        .and_then(|end| result.get(end..))
                        .unwrap_or_default();

                    // Clean up separators around the removed token
                    let before = before.trim_end_matches(['-', '_', '.', ' ']);
                    let after = after.trim_start_matches(['-', '_', '.', ' ']);

                    result = if before.is_empty() {
                        after.to_string()
                    } else if after.is_empty() {
                        before.to_string()
                    } else {
                        format!("{before} {after}")
                    };
                    break; // Only remove first occurrence per song name
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_song_name_in_all_tracks() {
        let inputs: Vec<String> = vec![
            "08-Kick-TheTrooper.wav".to_string(),
            "09-Snare-TheTrooper.wav".to_string(),
            "10-OH-TheTrooper.wav".to_string(),
            "15-Rhy Gtr L-TheTrooper.wav".to_string(),
        ];

        let song_names = detect_song_names(&inputs);
        // With fuzzy matching enabled (default), returns lowercase
        assert!(
            song_names.contains("thetrooper"),
            "Should detect 'thetrooper' as song name"
        );
        assert!(
            !song_names.contains("wav"),
            "Should not include file extension"
        );
    }

    #[test]
    fn detect_with_exact_matching() {
        let inputs: Vec<String> = vec![
            "08-Kick-TheTrooper.wav".to_string(),
            "09-Snare-TheTrooper.wav".to_string(),
        ];

        let config = SongNameConfig::default().with_fuzzy_match(false);
        let song_names = detect_song_names_with_config(&inputs, &config);
        // Without fuzzy matching, preserves original case
        assert!(
            song_names.contains("TheTrooper"),
            "Should detect 'TheTrooper' with original case"
        );
    }

    #[test]
    fn fuzzy_match_groups_variants() {
        // "TheTrooper" in most files, "Trooper" in one file
        let inputs: Vec<String> = vec![
            "08-Kick-TheTrooper.wav".to_string(),
            "09-Snare-TheTrooper.wav".to_string(),
            "10-OH-TheTrooper.wav".to_string(),
            "Trooper-mix1.wav".to_string(), // Different variant
        ];

        let song_names = detect_song_names(&inputs);
        // Fuzzy matching should detect "trooper" as appearing in all files
        // (because "thetrooper" contains "trooper")
        assert!(
            song_names.contains("trooper"),
            "Should detect 'trooper' via fuzzy matching"
        );
    }

    #[test]
    fn no_false_positives_for_audio_terms() {
        let inputs: Vec<String> = vec![
            "01.Kick.wav".to_string(),
            "02.Snare.wav".to_string(),
            "03.Bass.wav".to_string(),
        ];

        let song_names = detect_song_names(&inputs);
        assert!(
            !song_names.contains("wav"),
            "Should not detect 'wav' as song name"
        );
        assert!(song_names.is_empty(), "Should not detect any song names");
    }

    #[test]
    fn strip_song_name_from_string() {
        // With fuzzy matching, song names are lowercase
        let song_names: HashSet<String> = std::iter::once("thetrooper".to_string()).collect();

        assert_eq!(
            strip_song_names("15-Rhy Gtr L-TheTrooper", &song_names),
            "15-Rhy Gtr L"
        );
        assert_eq!(strip_song_names("TheTrooper-Kick", &song_names), "Kick");
    }

    #[test]
    fn strip_handles_containment() {
        // When we detect "trooper" (shorter form), it should strip "TheTrooper" too
        let song_names: HashSet<String> = std::iter::once("trooper".to_string()).collect();

        assert_eq!(
            strip_song_names("15-Rhy Gtr L-TheTrooper", &song_names),
            "15-Rhy Gtr L"
        );
        assert_eq!(strip_song_names("Trooper-mix1", &song_names), "mix1");
    }

    #[test]
    fn handles_camelcase_song_names() {
        let inputs: Vec<String> = vec![
            "01.LV BECCA.TimeAfterTime.126bpm".to_string(),
            "02.BV LUCA.TimeAfterTime.126bpm".to_string(),
            "03.Kick.TimeAfterTime.126bpm".to_string(),
        ];

        let song_names = detect_song_names(&inputs);
        // With fuzzy matching (default), returns lowercase
        assert!(
            song_names.contains("timeaftertime"),
            "Should detect 'timeaftertime' as song name"
        );
    }

    #[test]
    fn empty_inputs() {
        let inputs: Vec<String> = vec![];
        let song_names = detect_song_names(&inputs);
        assert!(song_names.is_empty());
    }

    #[test]
    fn single_input_no_detection() {
        // With a single input, everything would "appear in all inputs"
        // But common terms should still be filtered
        let inputs: Vec<String> = vec!["Kick-MySong.wav".to_string()];
        let song_names = detect_song_names(&inputs);
        // MySong might be detected, but it's a reasonable behavior for single files
        assert!(!song_names.contains("wav"));
        assert!(!song_names.contains("Kick"));
    }
}
