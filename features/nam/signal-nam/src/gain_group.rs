use crate::nam_file::{NamFileEntry, NamFileKind};
use regex::Regex;
use serde::{Deserialize, Serialize};
use signal_proto::tagging::TagSet;
use std::collections::HashMap;

/// A group of captures from the same amp/pedal at different gain levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainStageGroup {
    pub id: String,
    pub label: String,
    pub stages: Vec<GainStage>,
    pub tags: TagSet,
    /// Hash of the recommended IR for this group
    pub default_ir: Option<String>,
}

/// One gain level within a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainStage {
    /// 0-based position on the "gain knob"
    pub ordinal: u8,
    /// Human label, e.g. "aggr0", "Gain 03", "Clean"
    pub label: String,
    /// Content hash → NamFileEntry
    pub model_hash: String,
    /// Gain value from .nam metadata, if available
    pub gain_value: Option<f64>,
}

/// Result of auto-grouping: a group key → list of (ordinal, label, hash, gain_value).
type GroupMap = HashMap<String, Vec<(u8, String, String, Option<f64>)>>;

/// Auto-detect gain stage groups from a collection of NAM entries.
///
/// Applies pattern-matching heuristics to filenames to identify families
/// of captures that represent the same amp at different gain settings.
pub fn auto_group(entries: &HashMap<String, NamFileEntry>) -> HashMap<String, GainStageGroup> {
    let amp_entries: Vec<&NamFileEntry> = entries
        .values()
        .filter(|e| e.kind == NamFileKind::AmpModel)
        .collect();

    let mut groups = GroupMap::new();
    let mut grouped_hashes = std::collections::HashSet::new();

    // Pattern 1: `*-Gain-{NN}` or `*_Gain_{NN}` or `* Gain {NN}`
    let gain_num_re = Regex::new(r"^(.+?)[-_ ]Gain[-_ ](\d+)").unwrap();
    for entry in &amp_entries {
        if let Some(caps) = gain_num_re.captures(&entry.filename) {
            let prefix = caps[1].to_string();
            let num: u8 = caps[2].parse().unwrap_or(0);
            let label = format!("Gain {}", &caps[2]);
            groups.entry(slugify(&prefix)).or_default().push((
                num,
                label,
                entry.hash.clone(),
                entry.gain,
            ));
            grouped_hashes.insert(&entry.hash);
        }
    }

    // Pattern 2: `Revv_{channel}_aggr{n}_{eq}_{boost}`
    let revv_re = Regex::new(r"^(Revv_\w+)_aggr(\d+)_(\w+)_(\w+)").unwrap();
    for entry in &amp_entries {
        if grouped_hashes.contains(&entry.hash) {
            continue;
        }
        if let Some(caps) = revv_re.captures(&entry.filename) {
            let group_key = format!("{}_{}_{}", &caps[1], &caps[3], &caps[4]);
            let aggr: u8 = caps[2].parse().unwrap_or(0);
            let label = format!("aggr{}", aggr);
            groups.entry(slugify(&group_key)).or_default().push((
                aggr,
                label,
                entry.hash.clone(),
                entry.gain,
            ));
            grouped_hashes.insert(&entry.hash);
        }
    }

    // Pattern 3: `{NAME} GAIN {N}` (case-insensitive, with "BLUE GAIN 1" etc.)
    let name_gain_re = Regex::new(r"(?i)^(.+?)\s+GAIN\s+(\d+)").unwrap();
    for entry in &amp_entries {
        if grouped_hashes.contains(&entry.hash) {
            continue;
        }
        if let Some(caps) = name_gain_re.captures(&entry.filename) {
            let prefix = caps[1].to_string();
            let num: u8 = caps[2].parse().unwrap_or(0);
            let label = format!("Gain {}", num);
            groups.entry(slugify(&prefix)).or_default().push((
                num,
                label,
                entry.hash.clone(),
                entry.gain,
            ));
            grouped_hashes.insert(&entry.hash);
        }
    }

    // Pattern 4: Fallback — group by directory + similar filename prefix, sort by metadata gain
    // Only considers entries not already grouped, and only groups if 3+ files share a directory
    let mut dir_groups: HashMap<String, Vec<&NamFileEntry>> = HashMap::new();
    for entry in &amp_entries {
        if grouped_hashes.contains(&entry.hash) {
            continue;
        }
        if let Some(dir) = std::path::Path::new(&entry.relative_path).parent() {
            let dir_key = dir.to_string_lossy().to_string();
            if !dir_key.is_empty() {
                dir_groups.entry(dir_key).or_default().push(entry);
            }
        }
    }
    for (dir_key, mut dir_entries) in dir_groups {
        if dir_entries.len() < 3 {
            continue;
        }
        // Sort by metadata gain value if available
        dir_entries.sort_by(|a, b| {
            a.gain
                .unwrap_or(f64::MAX)
                .partial_cmp(&b.gain.unwrap_or(f64::MAX))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let group_key = slugify(&dir_key);
        for (i, entry) in dir_entries.iter().enumerate() {
            let label = entry
                .filename
                .strip_suffix(".nam")
                .unwrap_or(&entry.filename)
                .to_string();
            groups.entry(group_key.clone()).or_default().push((
                i as u8,
                label,
                entry.hash.clone(),
                entry.gain,
            ));
        }
    }

    // Convert GroupMap into GainStageGroups
    let mut result = HashMap::new();
    for (key, mut stages_raw) in groups {
        // Only create groups with 2+ members
        if stages_raw.len() < 2 {
            continue;
        }
        stages_raw.sort_by_key(|(ord, _, _, _)| *ord);

        let stages: Vec<GainStage> = stages_raw
            .into_iter()
            .map(|(ordinal, label, model_hash, gain_value)| GainStage {
                ordinal,
                label,
                model_hash,
                gain_value,
            })
            .collect();

        // Derive a human-readable label from the group key
        let label = key.replace('-', " ");
        let label = titlecase(&label);

        result.insert(
            key.clone(),
            GainStageGroup {
                id: key,
                label,
                stages,
                tags: TagSet::default(),
                default_ir: None,
            },
        );
    }

    result
}

/// Convert a string to a URL-friendly slug.
fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Simple title-case: capitalize first letter of each word.
fn titlecase(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nam_file::NamFileEntry;

    fn make_entry(hash: &str, filename: &str, gain: Option<f64>) -> NamFileEntry {
        NamFileEntry {
            hash: hash.into(),
            kind: NamFileKind::AmpModel,
            relative_path: format!("amps/{}", filename),
            filename: filename.into(),
            nam_version: None,
            architecture: None,
            sample_rate: None,
            gain,
            loudness: None,
            gear_type: None,
            gear_make: None,
            gear_model: None,
            tone_type: None,
            modeled_by: None,
            ir_channels: None,
            ir_sample_rate: None,
            ir_duration_ms: None,
            tags: TagSet::default(),
        }
    }

    #[test]
    fn group_gain_number_pattern() {
        let mut entries = HashMap::new();
        for i in 1..=10 {
            let name = format!("APP-6505Plus-Clean-Gain-{:02}.nam", i);
            let hash = format!("hash_clean_{}", i);
            entries.insert(hash.clone(), make_entry(&hash, &name, Some(i as f64)));
        }

        let groups = auto_group(&entries);
        assert_eq!(groups.len(), 1);
        let group = groups.values().next().unwrap();
        assert_eq!(group.stages.len(), 10);
        // Should be sorted by ordinal
        assert_eq!(group.stages[0].ordinal, 1);
        assert_eq!(group.stages[9].ordinal, 10);
    }

    #[test]
    fn group_revv_pattern() {
        let mut entries = HashMap::new();
        for aggr in 0..=2 {
            let name = format!("Revv_Red_aggr{}_mid_noPedal.nam", aggr);
            let hash = format!("revv_{}", aggr);
            entries.insert(
                hash.clone(),
                make_entry(&hash, &name, Some(aggr as f64 * 3.0)),
            );
        }

        let groups = auto_group(&entries);
        assert_eq!(groups.len(), 1);
        let group = groups.values().next().unwrap();
        assert_eq!(group.stages.len(), 3);
    }

    #[test]
    fn group_blue_gain_pattern() {
        let mut entries = HashMap::new();
        for i in 1..=6 {
            let name = format!("BLUE GAIN {}.nam", i);
            let hash = format!("blue_{}", i);
            entries.insert(hash.clone(), make_entry(&hash, &name, None));
        }

        let groups = auto_group(&entries);
        assert_eq!(groups.len(), 1);
        let group = groups.values().next().unwrap();
        assert_eq!(group.stages.len(), 6);
    }

    #[test]
    fn slugify_works() {
        assert_eq!(slugify("APP-6505Plus-Clean"), "app-6505plus-clean");
        assert_eq!(slugify("Revv Red / Mid"), "revv-red-mid");
        assert_eq!(slugify("  BLUE  "), "blue");
    }
}
