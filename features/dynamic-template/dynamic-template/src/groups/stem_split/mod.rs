//! Stem Split group definitions
//!
//! This group captures outputs from AI stem separation tools:
//! - Demucs: `drums.wav`, `bass.wav`, `vocals.wav`, `other.wav`, `piano.wav`
//! - LALAL.ai: `*_vocals.*`, `*_instrumental.*`, `*_drums.*`, `*_bass.*`
//! - Generic stem names when found as a cohesive set

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Stem Split group for AI-separated stems (Demucs, LALAL.ai, etc.)
pub struct StemSplit;

impl From<StemSplit> for Group<ItemMetadata> {
    fn from(_val: StemSplit) -> Self {
        Self::builder("Stem Split")
            .prefix("SS")
            .patterns(vec![
                // Demucs output patterns
                "htdemucs",
                "demucs",
                "mdx",
                "mdx_extra",
                // LALAL.ai suffix patterns
                "stem split",
                "stem-split",
                "separated",
                "isolation",
                "isolated",
                // Generic stem patterns (these match when items are
                // clearly stem-split outputs rather than live recordings)
                "other",     // Demucs "other" stem
                "no_vocals", // Vocal-removed version
                "no vocals",
                "instrumental", // Instrumental stem
                "accompaniment",
            ])
            .build()
    }
}

/// Standard stem categories recognized by stem separation tools.
const STEM_CATEGORIES: &[&str] = &[
    "drums",
    "bass",
    "vocals",
    "other",
    "piano",
    "guitar",
    "no_vocals",
    "instrumental",
    "accompaniment",
];

/// The stem category a name carries, if any — `"Song_drums"` → `"drums"`.
///
/// Used to tell which members of a folder are stems once
/// [`is_stem_split_set`] has decided the folder holds a separation. Without
/// this the whole folder gets reclassified, sweeping in anything that merely
/// sat alongside the stems.
#[must_use]
pub fn stem_category(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let stem = lower
        .rsplit_once('.')
        .map_or(lower.as_str(), |(name, _ext)| name);
    let basename = stem.rsplit_once('/').map_or(stem, |(_, name)| name);

    STEM_CATEGORIES.iter().copied().find(|cat| {
        basename == *cat
            || basename.ends_with(&format!("_{cat}"))
            || basename.ends_with(&format!("-{cat}"))
    })
}

/// The part of `name` before its stem category — the apparent source.
///
/// `"02 LORD OF THE FIGHT_Vocals"` → `"02 lord of the fight"`. Stems from one
/// separation share this; unrelated tracks that merely happen to be called
/// `Drums` and `Bass` do not.
#[must_use]
pub fn stem_source(name: &str) -> Option<String> {
    let cat = stem_category(name)?;
    let lower = name.to_lowercase();
    let stem = lower.rsplit_once('.').map_or(lower.as_str(), |(n, _)| n);
    let cut = stem.len().checked_sub(cat.len())?;
    Some(
        stem.get(..cut)?
            .trim_end_matches(['_', '-', ' '])
            .to_string(),
    )
}

/// Check if a set of item names looks like stem-split outputs.
///
/// Returns `true` if 3+ items match standard stem category names from the
/// same apparent source. This helps distinguish AI-separated stems from
/// live-recorded tracks that happen to be named "drums", "bass", etc.
#[must_use]
pub fn is_stem_split_set(items: &[String]) -> bool {
    if items.len() < 3 {
        return false;
    }

    let mut matches: usize = 0;
    for item in items {
        let lower = item.to_lowercase();
        // Check bare name (e.g., "drums.wav" → "drums")
        let stem = lower
            .rsplit_once('.')
            .map_or(lower.as_str(), |(name, _ext)| name);
        // Also check the last path component
        let basename = stem.rsplit_once('/').map_or(stem, |(_, name)| name);

        if STEM_CATEGORIES.iter().any(|cat| {
            basename == *cat
                || basename.ends_with(&format!("_{cat}"))
                || basename.ends_with(&format!("-{cat}"))
        }) {
            matches = matches.saturating_add(1);
        }
    }

    matches >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_category_reads_the_suffix() {
        assert_eq!(stem_category("Song_drums.wav"), Some("drums"));
        assert_eq!(stem_category("drums.wav"), Some("drums"));
        assert_eq!(stem_category("Kick In"), None);
        assert_eq!(stem_category("BAND RECORD VCA"), None);
    }

    #[test]
    fn stem_source_is_what_the_stems_share() {
        assert_eq!(
            stem_source("02 LORD OF THE FIGHT_Vocals"),
            Some("02 lord of the fight".to_string())
        );
        assert_eq!(
            stem_source("02 LORD OF THE FIGHT_Drums"),
            Some("02 lord of the fight".to_string())
        );
        // A bare live-tracked name shares nothing with anything.
        assert_eq!(stem_source("Drums"), Some(String::new()));
    }

    #[test]
    fn demucs_output_detected() {
        let items = vec![
            "drums.wav".to_string(),
            "bass.wav".to_string(),
            "vocals.wav".to_string(),
            "other.wav".to_string(),
        ];
        assert!(is_stem_split_set(&items));
    }

    #[test]
    fn lalal_ai_output_detected() {
        let items = vec![
            "Song_drums.wav".to_string(),
            "Song_bass.wav".to_string(),
            "Song_vocals.wav".to_string(),
            "Song_instrumental.wav".to_string(),
        ];
        assert!(is_stem_split_set(&items));
    }

    #[test]
    fn too_few_items_not_detected() {
        let items = vec!["drums.wav".to_string(), "bass.wav".to_string()];
        assert!(!is_stem_split_set(&items));
    }

    #[test]
    fn unrelated_items_not_detected() {
        let items = vec![
            "Lead Vocal.wav".to_string(),
            "Harmony BG.wav".to_string(),
            "Rhythm Guitar.wav".to_string(),
            "Pad.wav".to_string(),
        ];
        assert!(!is_stem_split_set(&items));
    }
}
