//! Electronic snare drum group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;
use monarchy::FieldValueDescriptor;

/// Electronic snare drum group
pub struct Snare;

impl From<Snare> for ItemMetadataGroup {
    fn from(_val: Snare) -> Self {
        // Define multi-mic positions as a Group
        let multi_mic = Self::builder("MultiMic")
            .patterns(["Top", "Bottom", "Side", "OH"])
            .build();

        // Define variant descriptors for drum machine types
        // These are also defined on Electronic Kit parent, but having them here
        // allows isolated testing and ensures they're available at this level
        let variant_descriptors = vec![
            FieldValueDescriptor::builder("808")
                .patterns(["808", "tr-808", "tr808"])
                .build(),
            FieldValueDescriptor::builder("909")
                .patterns(["909", "tr-909", "tr909"])
                .build(),
            FieldValueDescriptor::builder("707")
                .patterns(["707", "tr-707", "tr707"])
                .build(),
        ];

        // Use the convenience method - extension trait is in scope via prelude
        // Require parent match so this only matches when "Electronic Kit" also matches
        Self::builder("Snare")
            .patterns(["snare", "snr", "sn"])
            .multi_mic(multi_mic)
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors)
            .requires_parent_match()
            .build()
    }
}

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{default_config, DynamicTemplateConfig};
    use monarchy::{Config, Parser};

    /// Shared test cases - define input strings and expected metadata once
    /// Note: Electronic Kit snare requires parent match, so inputs must contain
    /// an Electronic Kit pattern (like "808", "electronic", "909") to match.
    mod test_cases {
        use super::*;
        use crate::item_metadata::ItemMetadataBuilder;

        pub fn snare_matches_group_and_has_original_name() -> (&'static str, ItemMetadata) {
            let input = "808 Snare";
            // In integration config, this matches Electronic Kit -> Snare
            // "808" is parsed as the variant
            let expected = ItemMetadataBuilder::new()
                .original_name(input)
                .last_group("Snare")
                .variant("808")
                .build();
            (input, expected)
        }

        pub fn snare_top_parses_multi_mic() -> (&'static str, ItemMetadata) {
            let input = "808 Snare Top";
            let expected = ItemMetadataBuilder::new()
                .original_name(input)
                .last_group("Snare")
                .multi_mic("Top")
                .variant("808")
                .build();
            (input, expected)
        }

        pub fn snare_bottom_parses_multi_mic() -> (&'static str, ItemMetadata) {
            let input = "808 Snare Bottom";
            let expected = ItemMetadataBuilder::new()
                .original_name(input)
                .last_group("Snare")
                .multi_mic("Bottom")
                .variant("808")
                .build();
            (input, expected)
        }
    }

    /// Tests with isolated configuration (only this group)
    mod isolated {
        use super::*;

        /// Create a config with only the Snare group
        fn isolated_config() -> DynamicTemplateConfig {
            Config::builder().group(Snare).build()
        }

        #[test]
        fn snare_matches_group_and_has_original_name() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_matches_group_and_has_original_name();
            let config = isolated_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Isolated config: '808 Snare' should match Snare group and have original_name"
            );
        }

        #[test]
        fn snare_top_parses_multi_mic_field() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_top_parses_multi_mic();
            let config = isolated_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Isolated config: '808 Snare Top' should parse multi_mic field as ['Top']"
            );

            let display_name = item.derive_display_name();
            assert_eq!(
                display_name, "808 Snare Top",
                "Display name should be '808 Snare Top'"
            );
        }

        #[test]
        fn snare_bottom_parses_multi_mic_field() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_bottom_parses_multi_mic();
            let config = isolated_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Isolated config: '808 Snare Bottom' should parse multi_mic field as ['Bottom']"
            );

            let display_name = item.derive_display_name();
            assert_eq!(
                display_name, "808 Snare Bottom",
                "Display name should be '808 Snare Bottom'"
            );
        }
    }

    /// Tests with integration configuration (default config with all groups)
    mod integration {
        use super::*;

        #[test]
        fn snare_matches_group_and_has_original_name() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_matches_group_and_has_original_name();
            let config = default_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Integration config: '808 Snare' should match Electronic Kit -> Snare"
            );

            let display_name = monarchy::to_display_name(&item, &config);
            assert_eq!(
                display_name, "D 808 Snare",
                "Integration config: display name should be 'D 808 Snare'"
            );
        }

        #[test]
        fn snare_top_parses_multi_mic_field() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_top_parses_multi_mic();
            let config = default_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Integration config: '808 Snare Top' should parse multi_mic field as ['Top']"
            );

            let display_name = monarchy::to_display_name(&item, &config);
            assert_eq!(
                display_name, "D 808 Snare Top",
                "Integration config: display name should be 'D 808 Snare Top'"
            );
        }

        #[test]
        fn snare_bottom_parses_multi_mic_field() {
            // -- Setup & Fixtures
            let (input, mut expected) = test_cases::snare_bottom_parses_multi_mic();
            let config = default_config();
            let parser = Parser::new(&config);

            // -- Exec
            let item = parser.parse(input.to_string()).unwrap();

            // -- Check
            expected.group = item.metadata.group.clone();
            assert_eq!(
                item.metadata, expected,
                "Integration config: '808 Snare Bottom' should parse multi_mic field as ['Bottom']"
            );

            let display_name = monarchy::to_display_name(&item, &config);
            assert_eq!(
                display_name, "D 808 Snare Bottom",
                "Integration config: display name should be 'D 808 Snare Bottom'"
            );
        }
    }
}

// endregion: --- Tests
