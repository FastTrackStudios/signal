//! Guide service — click track, count-in, and section guide event generation.

use facet::Facet;

/// Request for generating guide events during a count-in region.
#[derive(Clone, Facet)]
pub struct GuideRequest {
    /// Section start position in quarter notes.
    pub section_start_quarters: f64,
    /// Count-in start position in quarter notes.
    pub count_in_start_quarters: f64,
    /// Time signature for the section.
    pub time_signature: crate::TimeSignature,
    /// Tempo in BPM.
    pub tempo: f64,
    /// The section type being counted into.
    pub section_type: crate::SectionType,
    /// Optional section number (e.g., Verse 1, Verse 2).
    pub section_number: Option<u32>,
    /// Click track configuration.
    pub click_config: crate::ClickConfig,
    /// Count-in configuration.
    pub count_in_config: crate::CountInConfig,
    /// Section guide cue configuration.
    pub guide_config: crate::GuideConfig,
}

#[architect::rpc]
pub trait Guides {
    /// Generate all guide events for a section's count-in region.
    ///
    /// Returns a list of `GuideEvent`s sorted by position.
    fn generate_count_in_events(&self, request: GuideRequest) -> Vec<crate::GuideEvent>;
}
