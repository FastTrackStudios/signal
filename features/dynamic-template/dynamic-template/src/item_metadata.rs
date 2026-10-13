use monarchy::{Group, Metadata, ToDisplayName};
use serde::{Deserialize, Serialize};

/// Represents a parsed or constructed track name with all its components.
/// This is stored in `Track.ext_state` as JSON and represents the full FTS naming convention.
///
/// Note: `Metadata` derive automatically includes `MetadataBuilder` functionality,
/// so you only need to derive `Metadata` to get both the trait implementation
/// and the `GroupBuilder` extension methods (like `.multi_mic()`, `.layers()`, etc.).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Metadata)]
pub struct ItemMetadata {
    /// Recording tag (e.g., "PASS-01", "TAKE-02", "REC-01")
    pub rec_tag: Option<String>,

    /// Full hierarchy of groups that matched, from top-level to most specific
    /// Example: [`Drums`, `Drum_Kit`, `Kick`]
    /// The last element is the most specific group that matched
    pub group: Option<Vec<String>>,

    /// Performer name (e.g., "Cody", "Joshua", "Sarah")
    pub performer: Option<String>,

    /// Arrangement style (e.g., "Rhythm", "Solo", "Amb", "Crunch")
    pub arrangement: Option<String>,

    /// Section of the song (e.g., "Intro", "Verse", "Chorus", "Bridge")
    pub section: Option<String>,

    /// Layer information (e.g., "DBL", "OCT", "L", "R", "Stereo")
    pub layers: Option<String>,

    /// Multi-mic positions (e.g., [`Top`, `Bottom`], [`In`, `Out`], [`Close`, `Room`])
    pub multi_mic: Option<Vec<String>>,

    /// Effect/send indicators (e.g., [`Verb`], [`Delay`], [`Chorus`, `Verb`])
    pub effect: Option<Vec<String>>,

    /// Increment number for numbered instances (e.g., "1", "2" for Tom 1, Tom 2)
    pub increment: Option<String>,

    /// Channel information (e.g., "L", "R", "C", "Left", "Right")
    pub channel: Option<String>,

    /// Playlist identifier (e.g., ".1", ".2", ".3", ".A", ".B")
    pub playlist: Option<String>,

    /// Track type indicator (e.g., "BUS", "SUM", "MIDI", "DI", "NOFX")
    pub track_type: Option<String>,

    /// Variant/model of the instrument (e.g., "808", "909", "Gibson", "Telecaster", "P-Bass", "Rhodes")
    /// This identifies the specific type or make of an instrument
    pub variant: Option<String>,

    /// Tagged collections that this item matches (e.g., [`SUM`] if it matches a tagged collection's patterns)
    /// An item can match multiple tagged collections
    pub tagged_collection: Option<Vec<String>>,

    /// Words that didn't match any known patterns (for validation/debugging)
    pub unparsed_words: Option<Vec<String>>,

    /// Original input string before parsing (preserves full context for later analysis)
    /// This allows removing context based on matched group, but still checking the original context later
    pub original_name: Option<String>,

    /// File extension if parsed from a filename (e.g., ".wav", ".aiff", ".flac")
    /// This is typically not included in string formatting output
    pub file_extension: Option<String>,

    /// Tempo in BPM extracted from the input (e.g., 126.0 from "126bpm", 83.5 from "83.5BPM")
    /// This preserves tempo information that would otherwise be stripped from display names
    /// Note: Skipped in Metadata derive since f32 isn't a string type - extracted manually
    #[metadata(skip)]
    pub tempo: Option<f32>,
}

// Type alias for convenience
/// Type alias for `Group<ItemMetadata>`
pub type ItemMetadataGroup = Group<ItemMetadata>;

/// Builder for `ItemMetadata` with flexible input types
///
/// Allows passing strings, vectors, or arrays directly without explicit conversions.
#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct ItemMetadataBuilder {
    rec_tag: Option<String>,
    group: Option<Vec<String>>,
    performer: Option<String>,
    arrangement: Option<String>,
    section: Option<String>,
    layers: Option<String>,
    multi_mic: Option<Vec<String>>,
    effect: Option<Vec<String>>,
    increment: Option<String>,
    channel: Option<String>,
    playlist: Option<String>,
    track_type: Option<String>,
    variant: Option<String>,
    tagged_collection: Option<Vec<String>>,
    unparsed_words: Option<Vec<String>>,
    original_name: Option<String>,
    file_extension: Option<String>,
    tempo: Option<f32>,
}

#[allow(dead_code)]
impl ItemMetadataBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the recording tag
    pub fn rec_tag(mut self, value: impl Into<String>) -> Self {
        self.rec_tag = Some(value.into());
        self
    }

    /// Set the group trail (accepts single string, Vec<String>, array, etc.)
    /// Single string will be converted to a single-element vector
    pub fn group<T>(mut self, value: T) -> Self
    where
        T: IntoVec<String>,
    {
        self.group = Some(value.into_vec());
        self
    }

    /// Set the last group in the trail (most specific group)
    /// This is a convenience method for tests where you only care about the final group,
    /// not the full hierarchy. The group trail will be set to a single-element vector.
    pub fn last_group(mut self, group_name: impl Into<String>) -> Self {
        self.group = Some(vec![group_name.into()]);
        self
    }

    /// Set the performer name
    pub fn performer(mut self, value: impl Into<String>) -> Self {
        self.performer = Some(value.into());
        self
    }

    /// Set the arrangement style
    pub fn arrangement(mut self, value: impl Into<String>) -> Self {
        self.arrangement = Some(value.into());
        self
    }

    /// Set the section
    pub fn section(mut self, value: impl Into<String>) -> Self {
        self.section = Some(value.into());
        self
    }

    /// Set the layers
    pub fn layers(mut self, value: impl Into<String>) -> Self {
        self.layers = Some(value.into());
        self
    }

    /// Set multi-mic positions (accepts single string, Vec<String>, array, etc.)
    pub fn multi_mic<T>(mut self, value: T) -> Self
    where
        T: IntoVec<String>,
    {
        self.multi_mic = Some(value.into_vec());
        self
    }

    /// Set effect indicators (accepts single string, Vec<String>, array, etc.)
    pub fn effect<T>(mut self, value: T) -> Self
    where
        T: IntoVec<String>,
    {
        self.effect = Some(value.into_vec());
        self
    }

    /// Set the increment number
    pub fn increment(mut self, value: impl Into<String>) -> Self {
        self.increment = Some(value.into());
        self
    }

    /// Set the channel
    pub fn channel(mut self, value: impl Into<String>) -> Self {
        self.channel = Some(value.into());
        self
    }

    /// Set the playlist identifier
    pub fn playlist(mut self, value: impl Into<String>) -> Self {
        self.playlist = Some(value.into());
        self
    }

    /// Set the track type
    pub fn track_type(mut self, value: impl Into<String>) -> Self {
        self.track_type = Some(value.into());
        self
    }

    /// Set the variant/model of the instrument (e.g., "808", "Gibson", "Rhodes")
    pub fn variant(mut self, value: impl Into<String>) -> Self {
        self.variant = Some(value.into());
        self
    }

    /// Set tagged collection names (accepts single string, Vec<String>, array, etc.)
    /// An item can match multiple tagged collections
    pub fn tagged_collection<T>(mut self, value: T) -> Self
    where
        T: IntoVec<String>,
    {
        self.tagged_collection = Some(value.into_vec());
        self
    }

    /// Set unparsed words (accepts single string, Vec<String>, array, etc.)
    pub fn unparsed_words<T>(mut self, value: T) -> Self
    where
        T: IntoVec<String>,
    {
        self.unparsed_words = Some(value.into_vec());
        self
    }

    /// Set the original name
    pub fn original_name(mut self, value: impl Into<String>) -> Self {
        self.original_name = Some(value.into());
        self
    }

    /// Set the file extension
    pub fn file_extension(mut self, value: impl Into<String>) -> Self {
        self.file_extension = Some(value.into());
        self
    }

    /// Set the tempo in BPM
    pub const fn tempo(mut self, value: f32) -> Self {
        self.tempo = Some(value);
        self
    }

    /// Build the `ItemMetadata`
    pub fn build(self) -> ItemMetadata {
        ItemMetadata {
            rec_tag: self.rec_tag,
            group: self.group,
            performer: self.performer,
            arrangement: self.arrangement,
            section: self.section,
            layers: self.layers,
            multi_mic: self.multi_mic,
            effect: self.effect,
            increment: self.increment,
            channel: self.channel,
            playlist: self.playlist,
            track_type: self.track_type,
            variant: self.variant,
            tagged_collection: self.tagged_collection,
            unparsed_words: self.unparsed_words,
            original_name: self.original_name,
            file_extension: self.file_extension,
            tempo: self.tempo,
        }
    }
}

// Re-export IntoVec from monarchy for convenience
pub use monarchy::IntoVec;

/// Prelude module that exports everything needed to work with `ItemMetadata` groups
///
/// Import this to get all types and the extension trait in scope:
/// ```ignore
/// use dynamic_template::item_metadata::prelude::*;
/// ```
pub mod prelude {
    pub use super::ItemMetadata;

    pub use super::ItemMetadataGroup;
    pub use monarchy::Group;
    // The extension trait is generated by the Metadata derive macro
    // Bringing it into scope makes methods like .multi_mic() available
    pub use super::ItemMetadataGroupExt;
}

impl ToDisplayName for ItemMetadata {
    /// Generate a full canonical display name from metadata
    ///
    /// Build order: group → performer → section → arrangement → layers → increment → channel → `multi_mic` → `track_type`
    ///
    /// If we have meaningful metadata (beyond just group name), use that.
    /// If we only have group name and no other metadata, return empty string to trigger
    /// fallback to item.original in `derive_display_name()`.
    ///
    /// "Meaningful metadata" means metadata that describes WHAT the item is, not just
    /// version/layer information. Increment alone (".2") or playlist alone are not
    /// considered meaningful - they only make sense when combined with identifying info.
    ///
    /// The prefixes and `group_names` from `matched_groups` provide context that can be used
    /// to strip redundant information in a later cleanup pass.
    ///
    /// # Examples
    /// - Kick with `multi_mic`=[`In`] → "D Kick In" (with D prefix from Drums)
    /// - Electric Guitar with performer="Ed", arrangement="Crunch" → "GTR Ed Crunch"
    /// - Snare with increment="1" → "D Snare 1" (but only if has mic info too)
    /// - Acoustic Guitar → "GTR Acoustic" (not "GTR AG Acoustic" - skip redundant prefix)
    /// - DX7 .`2_03.wav` → "" (triggers fallback to original)
    fn to_display_name(&self, prefixes: &[String], group_names: &[String]) -> String {
        // Helper to capitalize first letter of a string
        fn title_case(s: &str) -> String {
            let mut chars = s.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        }

        let mut parts = Vec::new();

        // We'll add prefixes later, after we know if we're including the group name
        // This lets us skip the last prefix if it would be redundant with the group name

        // Track if we have any PRIMARY metadata (identifies what the item is)
        // vs SECONDARY metadata (version/layer info that only makes sense with primary).
        // Every branch below that pushes onto `parts` is primary metadata, so
        // whether any of them ran is exactly whether `parts` grew.
        let before_primary = parts.len();

        // Add variant (e.g., "808", "Gibson", "Rhodes")
        // This comes first as it modifies the instrument type
        if let Some(ref variant) = self.variant {
            parts.push(variant.clone());
        }

        // Add performer (e.g., "Ed", "Johny")
        if let Some(ref performer) = self.performer {
            parts.push(performer.clone());
        }

        // Add section (e.g., "Verse", "Chorus") - title case
        if let Some(ref section) = self.section {
            parts.push(title_case(section));
        }

        // Add arrangement (e.g., "Crunch", "Clean", "Lead") - title case
        if let Some(ref arrangement) = self.arrangement {
            parts.push(title_case(arrangement));
        }

        // Add multi-mic positions (e.g., "In", "Out", "Top", "Bottom") - title case
        // This is primary because it identifies mic placement
        if let Some(ref multi_mic) = self.multi_mic {
            for mic in multi_mic {
                parts.push(title_case(mic));
            }
        }

        // Add track type (e.g., "BUS", "SUM", "MIDI", "DI") - uppercase for acronyms
        // This is primary because it identifies the track's role
        if let Some(ref track_type) = self.track_type {
            parts.push(track_type.to_uppercase());
        }

        // Add effect (e.g., "Verb", "Comp", "Delay") - title case
        // This is primary because it identifies the track's processing
        if let Some(ref effect) = self.effect {
            for fx in effect {
                parts.push(title_case(fx));
            }
        }

        let has_primary_metadata = parts.len() > before_primary;

        // SECONDARY metadata fields - only include if we have primary metadata
        // These provide additional context but don't identify the item alone

        if has_primary_metadata {
            // Add layers (e.g., "DBL", "OCT", "L", "R") - title case (preserves uppercase)
            if let Some(ref layers) = self.layers {
                parts.push(title_case(layers));
            }

            // Add increment (e.g., "1", "2" for Tom 1, Tom 2)
            if let Some(ref increment) = self.increment {
                parts.push(increment.clone());
            }

            // Add channel (e.g., "L", "R", "C") - uppercase
            if let Some(ref channel) = self.channel {
                parts.push(channel.to_uppercase());
            }
        }

        // Include group name in display if:
        // 1. We have primary metadata (the group provides context), OR
        // 2. The original_name contains the group name (case-insensitive), OR
        // 3. We matched a specific subgroup (more than one group in the trail)
        //    This handles "Acc Guitar" → Guitars > Acoustic where we want "GTR Acoustic"
        //
        // If we only matched a top-level group by pattern without the group name in original,
        // return empty to use the original name (e.g., "Robot Voice" stays as "Robot Voice")
        let should_include_group = group_names.last().map_or(false, |last_group| {
            if has_primary_metadata {
                true
            } else if group_names.len() > 1 && !has_primary_metadata {
                // Matched a subgroup without primary metadata
                // e.g., "Acc Guitar" matched Guitars > Acoustic
                // We want to show the subgroup name
                true
            } else if let Some(ref original) = self.original_name {
                // Check if original contains the group name (case-insensitive)
                original.to_lowercase().contains(&last_group.to_lowercase())
            } else {
                false
            }
        });

        if should_include_group {
            if let Some(last_group) = group_names.last() {
                // Build prefixes, but skip the last one if we're including the group name
                // This avoids redundancy like "GTR AG Acoustic" → "GTR Acoustic"
                // The last prefix corresponds to the last group, so skip it
                let prefixes_to_use: Vec<&String> = if prefixes.len() == group_names.len() {
                    // Each group has a prefix, skip the last one
                    prefixes
                        .iter()
                        .take(prefixes.len().saturating_sub(1))
                        .collect()
                } else {
                    // Not a 1:1 mapping, use all prefixes
                    prefixes.iter().collect()
                };

                // Build final parts with adjusted prefixes
                let mut final_parts = Vec::new();
                if !prefixes_to_use.is_empty() {
                    final_parts.push(
                        prefixes_to_use
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                }

                // Add variant if present
                if let Some(ref variant) = self.variant {
                    final_parts.push(variant.clone());
                }

                // Add group name
                final_parts.push(last_group.clone());

                // Add remaining parts (everything after variant, which is first in parts if present)
                // Parts order: variant?, performer?, section?, arrangement?, multi_mic?, track_type?, effect?
                let skip_count = usize::from(self.variant.is_some());
                for part in parts.iter().skip(skip_count) {
                    final_parts.push(part.clone());
                }

                return final_parts.join(" ");
            }
        }

        // No meaningful metadata and original doesn't contain group name
        // Return empty string to trigger fallback to item.original
        String::new()
    }
}
