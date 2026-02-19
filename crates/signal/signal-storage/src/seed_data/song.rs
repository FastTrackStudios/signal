//! Song seed data — demo songs showcasing section source + overrides.

use signal_proto::metadata::Metadata;
use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
use signal_proto::seed_id;
use signal_proto::song::{Section, Song};

/// All default song collections.
pub fn songs() -> Vec<Song> {
    vec![feature_demo_song(), dummy_song(), guitar_worship_song()]
}

/// Feature-Demo Song
/// - 4 sections total
/// - 3 sections source from profile patches
/// - 1 section sources directly from rig scene preset
/// - Overrides target lower levels (engine/layer/module/block/parameter)
fn feature_demo_song() -> Song {
    let intro = Section::from_patch(
        seed_id("feature-demo-intro"),
        "Intro",
        seed_id("keys-feature-foundation"),
    )
    .with_override(Override {
        // Engine-level preset/scene override
        path: NodePath::engine("synth-engine"),
        op: NodeOverrideOp::ReplaceRef(seed_id("synth-engine-scene-b").to_string()),
    })
    .with_metadata(
        Metadata::new()
            .with_tag("patch")
            .with_tag("engine-override"),
    );

    let verse = Section::from_patch(
        seed_id("feature-demo-verse"),
        "Verse",
        seed_id("keys-feature-wide"),
    )
    .with_override(Override {
        // Layer-level preset override
        path: NodePath::engine("keys-engine").with_layer("keys-layer-space"),
        op: NodeOverrideOp::ReplaceRef(seed_id("keys-layer-space-default").to_string()),
    })
    .with_metadata(Metadata::new().with_tag("patch").with_tag("layer-override"));

    let chorus = Section::from_patch(
        seed_id("feature-demo-chorus"),
        "Chorus",
        seed_id("keys-feature-focus"),
    )
    .with_override(Override {
        // Module-level preset override
        path: NodePath::engine("keys-engine")
            .with_layer("keys-layer-space")
            .with_module("time-parallel"),
        op: NodeOverrideOp::ReplaceRef(seed_id("time-parallel-ambient").to_string()),
    })
    .with_override(Override::set(
        // Parameter-level override
        NodePath::engine("keys-engine")
            .with_layer("keys-layer-space")
            .with_block("keys-space-verb")
            .with_parameter("mix"),
        0.67,
    ))
    .with_metadata(
        Metadata::new()
            .with_tag("patch")
            .with_tag("module-param-override"),
    );

    let bridge = Section::from_rig_scene(
        seed_id("feature-demo-bridge"),
        "Bridge",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-air"),
    )
    .with_override(Override {
        // Block-level preset override
        path: NodePath::engine("synth-engine")
            .with_layer("synth-layer-texture")
            .with_block("texture-verb"),
        op: NodeOverrideOp::ReplaceRef(seed_id("reverb-space-blackhole").to_string()),
    })
    .with_metadata(
        Metadata::new()
            .with_tag("preset")
            .with_tag("block-override"),
    );

    let mut song = Song::new(seed_id("feature-demo-song"), "Feature-Demo Song", intro)
        .with_artist("Signal2")
        .with_metadata(
            Metadata::new()
                .with_tag("setlist")
                .with_tag("keys")
                .with_description(
                    "Demonstrates patch-sourced and preset-sourced sections with deep overrides",
                ),
        );
    song.add_section(verse);
    song.add_section(chorus);
    song.add_section(bridge);
    song
}

fn dummy_song() -> Song {
    let section = Section::from_rig_scene(
        seed_id("dummy-song-main"),
        "Main",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-default"),
    );
    Song::new(seed_id("dummy-song"), "Dummy Song", section)
        .with_artist("Signal2")
        .with_metadata(
            Metadata::new()
                .with_tag("setlist")
                .with_tag("dummy")
                .with_description("Simple seed song for demo setlists"),
        )
}

/// Guitar Worship Song — demonstrates patch-sourced and rig-scene-sourced sections
/// using the Worship profile patches.
fn guitar_worship_song() -> Song {
    let intro = Section::from_patch(
        seed_id("guitar-worship-song-intro"),
        "Intro",
        seed_id("guitar-worship-clean"),
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("patch"));

    let verse = Section::from_patch(
        seed_id("guitar-worship-song-verse"),
        "Verse",
        seed_id("guitar-worship-ambient"),
    )
    .with_override(Override::set(
        NodePath::engine("guitar-engine")
            .with_layer("guitar-layer-archetype-jm")
            .with_block("dream-delay")
            .with_parameter("mix"),
        0.20,
    ))
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("patch"));

    let chorus = Section::from_patch(
        seed_id("guitar-worship-song-chorus"),
        "Chorus",
        seed_id("guitar-worship-drive"),
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("patch"));

    let bridge = Section::from_patch(
        seed_id("guitar-worship-song-bridge"),
        "Bridge",
        seed_id("guitar-worship-delay"),
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("patch"));

    let solo = Section::from_patch(
        seed_id("guitar-worship-song-solo"),
        "Solo",
        seed_id("guitar-worship-solo"),
    )
    .with_override(Override::set(
        NodePath::engine("guitar-engine")
            .with_layer("guitar-layer-archetype-jm")
            .with_block("dream-delay")
            .with_parameter("feedback"),
        0.55,
    ))
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("patch"));

    let outro = Section::from_rig_scene(
        seed_id("guitar-worship-song-outro"),
        "Outro",
        seed_id("guitar-megarig"),
        seed_id("guitar-megarig-default"),
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("preset"));

    let mut song = Song::new(seed_id("guitar-worship-song"), "Worship Set", intro)
        .with_artist("Signal2")
        .with_metadata(
            Metadata::new()
                .with_tag("setlist")
                .with_tag("guitar")
                .with_description(
                    "Worship song using clean/ambient/drive/delay/solo patches from Worship profile",
                ),
        );
    song.add_section(verse);
    song.add_section(chorus);
    song.add_section(bridge);
    song.add_section(solo);
    song.add_section(outro);
    song
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::song::SectionSource;

    #[test]
    fn song_count() {
        assert_eq!(songs().len(), 3);
    }

    #[test]
    fn feature_demo_has_four_sections() {
        let song = &songs()[0];
        assert_eq!(song.name, "Feature-Demo Song");
        assert_eq!(song.sections.len(), 4);
        assert_eq!(
            song.default_section_id.as_str(),
            seed_id("feature-demo-intro").to_string()
        );
    }

    #[test]
    fn feature_demo_section_sources_match_requirements() {
        let song = &songs()[0];
        let patch_count = song
            .sections
            .iter()
            .filter(|s| matches!(s.source, SectionSource::Patch { .. }))
            .count();
        let rig_scene_count = song
            .sections
            .iter()
            .filter(|s| matches!(s.source, SectionSource::RigScene { .. }))
            .count();

        assert_eq!(patch_count, 3);
        assert_eq!(rig_scene_count, 1);
    }

    #[test]
    fn feature_demo_has_overrides_across_levels() {
        let song = &songs()[0];
        let mut saw_engine = false;
        let mut saw_layer = false;
        let mut saw_module = false;
        let mut saw_block = false;
        let mut saw_param = false;

        for section in &song.sections {
            for ov in &section.overrides {
                let path = ov.path.as_str();
                if path == "engine.synth-engine" {
                    saw_engine = true;
                }
                if path.contains(".layer.")
                    && !path.contains(".module.")
                    && !path.contains(".block.")
                {
                    saw_layer = true;
                }
                if path.contains(".module.") {
                    saw_module = true;
                }
                if path.contains(".block.") && !path.contains(".param.") {
                    saw_block = true;
                }
                if path.contains(".param.") {
                    saw_param = true;
                }
            }
        }

        assert!(saw_engine);
        assert!(saw_layer);
        assert!(saw_module);
        assert!(saw_block);
        assert!(saw_param);
    }
}
