//! Mapping from FabFilter's flat tag vocabulary to Signal's structured tags.
//!
//! FabFilter presets use simple comma-separated tags like "Drums,Bright,Bus".
//! We map these into Signal's `TagCategory` taxonomy so they integrate with
//! the browser's semantic search.

use signal_proto::tagging::{StructuredTag, TagCategory, TagSource};

/// Map a single FabFilter tag string to a Signal `StructuredTag`.
///
/// Returns a tag with `TagSource::Imported` and the appropriate category.
/// Unmapped tags are preserved as `custom:<tag>`.
pub fn map_fabfilter_tag(raw: &str) -> StructuredTag {
    let lower = raw.trim().to_ascii_lowercase();

    let (category, value) = match lower.as_str() {
        // ─── Instrument ──────────────────────────────────────────
        "drums" | "drum" => (TagCategory::Instrument, "drums"),
        "guitar" => (TagCategory::Instrument, "guitar"),
        "bass" => (TagCategory::Instrument, "bass"),
        "vocals" | "vocal" | "voice" => (TagCategory::Instrument, "vocals"),
        "keys" | "keyboard" | "piano" | "synth" => (TagCategory::Instrument, "keys"),
        "strings" => (TagCategory::Instrument, "strings"),
        "brass" | "horns" => (TagCategory::Instrument, "brass"),
        "woodwinds" => (TagCategory::Instrument, "woodwinds"),
        "percussion" => (TagCategory::Instrument, "percussion"),

        // ─── Character ───────────────────────────────────────────
        "bright" => (TagCategory::Character, "bright"),
        "warm" => (TagCategory::Character, "warm"),
        "dark" => (TagCategory::Character, "dark"),
        "smooth" => (TagCategory::Character, "smooth"),
        "aggressive" => (TagCategory::Character, "aggressive"),
        "subtle" => (TagCategory::Character, "subtle"),
        "natural" => (TagCategory::Character, "natural"),
        "transparent" => (TagCategory::Character, "transparent"),
        "punchy" => (TagCategory::Character, "punchy"),
        "crisp" => (TagCategory::Character, "crisp"),
        "thick" => (TagCategory::Character, "thick"),
        "tight" => (TagCategory::Character, "tight"),
        "airy" => (TagCategory::Character, "airy"),

        // ─── Context ─────────────────────────────────────────────
        "bus" | "mixbus" | "mix bus" => (TagCategory::Context, "mixbus"),
        "master" | "mastering" => (TagCategory::Context, "mastering"),
        "recording" | "tracking" => (TagCategory::Context, "tracking"),
        "live" => (TagCategory::Context, "live"),
        "sidechain" => (TagCategory::Context, "sidechain"),
        "parallel" => (TagCategory::Context, "parallel"),

        // ─── Tone ────────────────────────────────────────────────
        "clean" => (TagCategory::Tone, "clean"),
        "lo-fi" | "lofi" => (TagCategory::Tone, "lo-fi"),
        "vintage" => (TagCategory::Tone, "vintage"),

        // ─── Genre ───────────────────────────────────────────────
        "rock" => (TagCategory::Genre, "rock"),
        "pop" => (TagCategory::Genre, "pop"),
        "hiphop" | "hip-hop" | "hip hop" => (TagCategory::Genre, "hiphop"),
        "electronic" | "edm" => (TagCategory::Genre, "electronic"),
        "jazz" => (TagCategory::Genre, "jazz"),
        "country" => (TagCategory::Genre, "country"),
        "metal" => (TagCategory::Genre, "metal"),

        // ─── Unmapped → custom ───────────────────────────────────
        _ => {
            return StructuredTag::new(TagCategory::Custom, raw.trim())
                .with_source(TagSource::Imported);
        }
    };

    StructuredTag::new(category, value).with_source(TagSource::Imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_instrument_tags() {
        let tag = map_fabfilter_tag("Drums");
        assert_eq!(tag.category, TagCategory::Instrument);
        assert_eq!(tag.value, "drums");
        assert_eq!(tag.source, TagSource::Imported);
    }

    #[test]
    fn maps_character_tags() {
        let tag = map_fabfilter_tag("Bright");
        assert_eq!(tag.category, TagCategory::Character);
        assert_eq!(tag.value, "bright");
    }

    #[test]
    fn maps_context_tags() {
        let tag = map_fabfilter_tag("Bus");
        assert_eq!(tag.category, TagCategory::Context);
        assert_eq!(tag.value, "mixbus");
    }

    #[test]
    fn preserves_unknown_as_custom() {
        let tag = map_fabfilter_tag("MyCustomTag");
        assert_eq!(tag.category, TagCategory::Custom);
        assert_eq!(tag.value, "mycustomtag");
    }

    #[test]
    fn handles_case_insensitive() {
        let tag = map_fabfilter_tag("GUITAR");
        assert_eq!(tag.category, TagCategory::Instrument);
        assert_eq!(tag.value, "guitar");
    }
}
