//! Song seed data — demo songs showcasing section source + overrides.

use signal_proto::metadata::Metadata;
use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
use signal_proto::seed_id;
use signal_proto::song::{Section, Song};

/// All default song collections.
pub fn songs() -> Vec<Song> {
    vec![
        feature_demo_song(),
        dummy_song(),
        guitar_worship_song(),
        // Commercial Music setlist songs
        four_am_song(),
        thriller_song(),
        movin_out_song(),
        girl_goodbye_song(),
        bennie_and_the_jets_song(),
        for_cryin_out_loud_song(),
        we_were_never_really_friends_song(),
        i_dont_trust_myself_song(),
        // Night of Entertainment setlist songs
        come_and_see_song(),
        holy_one_song(),
        separate_ways_song(),
    ]
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

// ── Commercial Music setlist songs ────────────────────────────────────

/// 4 A.M. — no base profile; three standalone sections from various profiles.
fn four_am_song() -> Song {
    let lead = Section::from_patch(
        seed_id("four-am-lead"),
        "Lead",
        seed_id("guitar-worship-lead"),
    );

    let ambient_b = Section::from_patch(
        seed_id("four-am-ambient-b"),
        "Ambient B-Section",
        seed_id("guitar-worship-ambient"),
    );

    let jazz_dry = Section::from_patch(
        seed_id("four-am-jazz-dry"),
        "Jazz Dry",
        seed_id("guitar-worship-clean"),
    );

    let mut song = Song::new(seed_id("four-am-song"), "4 A.M.", lead);
    song.add_section(ambient_b);
    song.add_section(jazz_dry);
    song
}

/// Thriller — base profile: All-Around (8 sections, one per slot).
/// Overrides: Slot 4 → "Lead" using All-Around Solo patch, Slot 5 → Funk* (override).
/// All-Around order: Clean, Crunch, Drive, Lead, Funk, Ambient, Q-Tron, Solo
fn thriller_song() -> Song {
    // Slot 1: Clean (inherited)
    let s1 = Section::from_patch(
        seed_id("thriller-s1"),
        "Clean",
        seed_id("guitar-allaround-clean"),
    );
    // Slot 2: Crunch (inherited)
    let s2 = Section::from_patch(
        seed_id("thriller-s2"),
        "Crunch",
        seed_id("guitar-allaround-crunch"),
    );
    // Slot 3: Drive (inherited)
    let s3 = Section::from_patch(
        seed_id("thriller-s3"),
        "Drive",
        seed_id("guitar-allaround-drive"),
    );
    // Slot 4: "Lead" — overridden to All-Around "Solo" patch
    let s4 = Section::from_patch(
        seed_id("thriller-s4"),
        "Lead",
        seed_id("guitar-allaround-solo"),
    );
    // Slot 5: Funk* — slot override (still uses All-Around Funk patch)
    let s5 = Section::from_patch(
        seed_id("thriller-s5"),
        "Funk",
        seed_id("guitar-allaround-funk"),
    );
    // Slot 6: Ambient (inherited)
    let s6 = Section::from_patch(
        seed_id("thriller-s6"),
        "Ambient",
        seed_id("guitar-allaround-ambient"),
    );
    // Slot 7: Q-Tron (inherited)
    let s7 = Section::from_patch(
        seed_id("thriller-s7"),
        "Q-Tron",
        seed_id("guitar-allaround-qtron"),
    );
    // Slot 8: Solo (inherited)
    let s8 = Section::from_patch(
        seed_id("thriller-s8"),
        "Solo",
        seed_id("guitar-allaround-solo"),
    );

    let mut song = Song::new(seed_id("thriller-song"), "Thriller", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-allaround-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

/// Movin' Out — base profile: All-Around (8 sections, one per slot).
/// User listed Slot 1 (Clean) and Slot 5 (Funk) — rest inherit from All-Around.
/// All-Around order: Clean, Crunch, Drive, Lead, Funk, Ambient, Q-Tron, Solo
fn movin_out_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("movin-out-s1"),
        "Clean",
        seed_id("guitar-allaround-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("movin-out-s2"),
        "Crunch",
        seed_id("guitar-allaround-crunch"),
    );
    let s3 = Section::from_patch(
        seed_id("movin-out-s3"),
        "Drive",
        seed_id("guitar-allaround-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("movin-out-s4"),
        "Lead",
        seed_id("guitar-allaround-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("movin-out-s5"),
        "Funk",
        seed_id("guitar-allaround-funk"),
    );
    let s6 = Section::from_patch(
        seed_id("movin-out-s6"),
        "Ambient",
        seed_id("guitar-allaround-ambient"),
    );
    let s7 = Section::from_patch(
        seed_id("movin-out-s7"),
        "Q-Tron",
        seed_id("guitar-allaround-qtron"),
    );
    let s8 = Section::from_patch(
        seed_id("movin-out-s8"),
        "Solo",
        seed_id("guitar-allaround-solo"),
    );

    let mut song = Song::new(seed_id("movin-out-song"), "Movin' Out", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-allaround-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

/// Girl Goodbye — base profile: Rock (8 sections, one per slot).
/// Overrides: Slot 3 → Drive* (override).
/// Rock order: Clean, Crunch, Drive, Lead, Ambient, Phaser, DLY Lead, Solo
fn girl_goodbye_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("girl-goodbye-s1"),
        "Clean",
        seed_id("guitar-rock-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("girl-goodbye-s2"),
        "Crunch",
        seed_id("guitar-rock-crunch"),
    );
    // Slot 3: Drive* — slot override (still Rock Drive patch, but marked as overridden)
    let s3 = Section::from_patch(
        seed_id("girl-goodbye-s3"),
        "Drive",
        seed_id("guitar-rock-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("girl-goodbye-s4"),
        "Lead",
        seed_id("guitar-rock-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("girl-goodbye-s5"),
        "Ambient",
        seed_id("guitar-rock-ambient"),
    );
    let s6 = Section::from_patch(
        seed_id("girl-goodbye-s6"),
        "Phaser",
        seed_id("guitar-rock-phaser"),
    );
    let s7 = Section::from_patch(
        seed_id("girl-goodbye-s7"),
        "DLY Lead",
        seed_id("guitar-rock-dly-lead"),
    );
    let s8 = Section::from_patch(
        seed_id("girl-goodbye-s8"),
        "Solo",
        seed_id("guitar-rock-solo"),
    );

    let mut song = Song::new(seed_id("girl-goodbye-song"), "Girl Goodbye", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-rock-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

/// Bennie And The Jets — base profile: All-Around.
/// Uses all 8 patches as-is (no overrides, no subset).
fn bennie_and_the_jets_song() -> Song {
    let clean = Section::from_patch(
        seed_id("bennie-jets-clean"),
        "Clean",
        seed_id("guitar-allaround-clean"),
    );
    let crunch = Section::from_patch(
        seed_id("bennie-jets-crunch"),
        "Crunch",
        seed_id("guitar-allaround-crunch"),
    );
    let drive = Section::from_patch(
        seed_id("bennie-jets-drive"),
        "Drive",
        seed_id("guitar-allaround-drive"),
    );
    let lead = Section::from_patch(
        seed_id("bennie-jets-lead"),
        "Lead",
        seed_id("guitar-allaround-lead"),
    );
    let funk = Section::from_patch(
        seed_id("bennie-jets-funk"),
        "Funk",
        seed_id("guitar-allaround-funk"),
    );
    let ambient = Section::from_patch(
        seed_id("bennie-jets-ambient"),
        "Ambient",
        seed_id("guitar-allaround-ambient"),
    );
    let qtron = Section::from_patch(
        seed_id("bennie-jets-qtron"),
        "Q-Tron",
        seed_id("guitar-allaround-qtron"),
    );
    let solo = Section::from_patch(
        seed_id("bennie-jets-solo"),
        "Solo",
        seed_id("guitar-allaround-solo"),
    );

    let mut song = Song::new(seed_id("bennie-jets-song"), "Bennie And The Jets", clean)
        .with_metadata(
            Metadata::new().with_base_profile_id(seed_id("guitar-allaround-profile").to_string()),
        );
    song.add_section(crunch);
    song.add_section(drive);
    song.add_section(lead);
    song.add_section(funk);
    song.add_section(ambient);
    song.add_section(qtron);
    song.add_section(solo);
    song
}

/// For Cryin' Out Loud — no base profile.
/// 4 standalone sections referencing various profile patches.
fn for_cryin_out_loud_song() -> Song {
    // Slot references are just descriptive — no base profile, so these are
    // hand-picked patches from various profiles.
    let clean = Section::from_patch(
        seed_id("cryin-out-loud-clean"),
        "Clean",
        seed_id("guitar-worship-clean"),
    );

    let ambient_chords = Section::from_patch(
        seed_id("cryin-out-loud-ambient"),
        "Ambient Chords",
        seed_id("guitar-worship-ambient"),
    );

    let crunch = Section::from_patch(
        seed_id("cryin-out-loud-crunch"),
        "Crunch",
        seed_id("guitar-worship-crunch"),
    );

    let solo = Section::from_patch(
        seed_id("cryin-out-loud-solo"),
        "Solo",
        seed_id("guitar-worship-solo"),
    );

    let mut song = Song::new(seed_id("cryin-out-loud-song"), "For Cryin' Out Loud", clean);
    song.add_section(ambient_chords);
    song.add_section(crunch);
    song.add_section(solo);
    song
}

/// We Were Never Really Friends — no base profile.
/// 3 standalone sections: Clean, Drive, Solo.
fn we_were_never_really_friends_song() -> Song {
    let clean = Section::from_patch(
        seed_id("never-friends-clean"),
        "Clean",
        seed_id("guitar-worship-clean"),
    );

    let drive = Section::from_patch(
        seed_id("never-friends-drive"),
        "Drive",
        seed_id("guitar-worship-drive"),
    );

    let solo = Section::from_patch(
        seed_id("never-friends-solo"),
        "Solo",
        seed_id("guitar-worship-solo"),
    );

    let mut song = Song::new(
        seed_id("never-friends-song"),
        "We Were Never Really Friends",
        clean,
    );
    song.add_section(drive);
    song.add_section(solo);
    song
}

/// I Don't Trust Myself — base profile: Blues (8 sections, one per slot).
/// Notable slots: Slot 6 → Q-Tron, Slot 7 → Q-Tron Lead (renamed from Roomy),
/// Slot 8 → Solo, plus extra sections beyond the 8 base slots.
/// Blues order: Clean, Crunch, Drive, Lead, Funk, Q-Tron, Roomy, Solo
fn i_dont_trust_myself_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("dont-trust-s1"),
        "Clean",
        seed_id("guitar-blues-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("dont-trust-s2"),
        "Crunch",
        seed_id("guitar-blues-crunch"),
    );
    let s3 = Section::from_patch(
        seed_id("dont-trust-s3"),
        "Drive",
        seed_id("guitar-blues-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("dont-trust-s4"),
        "Lead",
        seed_id("guitar-blues-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("dont-trust-s5"),
        "Funk",
        seed_id("guitar-blues-funk"),
    );
    // Slot 6: Q-Tron (inherited from Blues)
    let s6 = Section::from_patch(
        seed_id("dont-trust-s6"),
        "Q-Tron",
        seed_id("guitar-blues-qtron"),
    );
    // Slot 7: "Q-Tron Lead" — overridden from Blues "Roomy" to Blues Q-Tron patch
    let s7 = Section::from_patch(
        seed_id("dont-trust-s7"),
        "Q-Tron Lead",
        seed_id("guitar-blues-qtron"),
    );
    // Slot 8: Solo (inherited from Blues)
    let s8 = Section::from_patch(
        seed_id("dont-trust-s8"),
        "Solo",
        seed_id("guitar-blues-solo"),
    );

    let mut song = Song::new(seed_id("dont-trust-song"), "I Don't Trust Myself", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-blues-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

// ── Night of Entertainment setlist songs ──────────────────────────────

/// Come and See — base profile: Worship (8 sections, one per slot).
/// Worship order: Clean, Crunch, Drive, Lead, Ambient, Tremolo, Delay, Solo
/// Default section: Ambient (Come and See RfxChain — main sound).
fn come_and_see_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("come-see-s1"),
        "Clean",
        seed_id("guitar-worship-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("come-see-s2"),
        "Crunch",
        seed_id("guitar-worship-crunch"),
    );
    let s3 = Section::from_patch(
        seed_id("come-see-s3"),
        "Drive",
        seed_id("guitar-worship-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("come-see-s4"),
        "Lead",
        seed_id("guitar-worship-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("come-see-s5"),
        "Ambient",
        seed_id("guitar-worship-ambient"),
    );
    let s6 = Section::from_patch(
        seed_id("come-see-s6"),
        "Tremolo",
        seed_id("guitar-worship-tremolo"),
    );
    let s7 = Section::from_patch(
        seed_id("come-see-s7"),
        "Delay",
        seed_id("guitar-worship-delay"),
    );
    let s8 = Section::from_patch(
        seed_id("come-see-s8"),
        "Solo",
        seed_id("guitar-worship-solo"),
    );

    // Ambient is the default/main section — the Come and See RfxChain preset.
    let mut song = Song::new(seed_id("come-see-song"), "Come and See", s5).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-worship-profile").to_string()),
    );
    song.add_section(s1);
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

/// Holy One — base profile: Worship (8 sections, one per slot).
/// Worship order: Clean, Crunch, Drive, Lead, Ambient, Tremolo, Delay, Solo
fn holy_one_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("holy-one-s1"),
        "Clean",
        seed_id("guitar-worship-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("holy-one-s2"),
        "Crunch",
        seed_id("guitar-worship-crunch"),
    );
    let s3 = Section::from_patch(
        seed_id("holy-one-s3"),
        "Drive",
        seed_id("guitar-worship-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("holy-one-s4"),
        "Lead",
        seed_id("guitar-worship-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("holy-one-s5"),
        "Ambient",
        seed_id("guitar-worship-ambient"),
    );
    let s6 = Section::from_patch(
        seed_id("holy-one-s6"),
        "Tremolo",
        seed_id("guitar-worship-tremolo"),
    );
    let s7 = Section::from_patch(
        seed_id("holy-one-s7"),
        "Delay",
        seed_id("guitar-worship-delay"),
    );
    let s8 = Section::from_patch(
        seed_id("holy-one-s8"),
        "Solo",
        seed_id("guitar-worship-solo"),
    );

    let mut song = Song::new(seed_id("holy-one-song"), "Holy One", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-worship-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

/// Separate Ways — base profile: Rock (8 sections, one per slot).
/// Rock order: Clean, Crunch, Drive, Lead, Ambient, Phaser, DLY Lead, Solo
fn separate_ways_song() -> Song {
    let s1 = Section::from_patch(
        seed_id("separate-ways-s1"),
        "Clean",
        seed_id("guitar-rock-clean"),
    );
    let s2 = Section::from_patch(
        seed_id("separate-ways-s2"),
        "Crunch",
        seed_id("guitar-rock-crunch"),
    );
    let s3 = Section::from_patch(
        seed_id("separate-ways-s3"),
        "Drive",
        seed_id("guitar-rock-drive"),
    );
    let s4 = Section::from_patch(
        seed_id("separate-ways-s4"),
        "Lead",
        seed_id("guitar-rock-lead"),
    );
    let s5 = Section::from_patch(
        seed_id("separate-ways-s5"),
        "Ambient",
        seed_id("guitar-rock-ambient"),
    );
    let s6 = Section::from_patch(
        seed_id("separate-ways-s6"),
        "Phaser",
        seed_id("guitar-rock-phaser"),
    );
    let s7 = Section::from_patch(
        seed_id("separate-ways-s7"),
        "DLY Lead",
        seed_id("guitar-rock-dly-lead"),
    );
    let s8 = Section::from_patch(
        seed_id("separate-ways-s8"),
        "Solo",
        seed_id("guitar-rock-solo"),
    );

    let mut song = Song::new(seed_id("separate-ways-song"), "Separate Ways", s1).with_metadata(
        Metadata::new().with_base_profile_id(seed_id("guitar-rock-profile").to_string()),
    );
    song.add_section(s2);
    song.add_section(s3);
    song.add_section(s4);
    song.add_section(s5);
    song.add_section(s6);
    song.add_section(s7);
    song.add_section(s8);
    song
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::song::SectionSource;

    #[test]
    fn song_count() {
        assert_eq!(songs().len(), 14);
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
    fn commercial_songs_section_counts() {
        let all = songs();
        let find = |name: &str| all.iter().find(|s| s.name == name).unwrap();

        // Songs without base profiles — only the listed sections
        assert_eq!(find("4 A.M.").sections.len(), 3);
        assert_eq!(find("For Cryin' Out Loud").sections.len(), 4);
        assert_eq!(find("We Were Never Really Friends").sections.len(), 3);

        // Songs with base profiles — always 8 sections (one per slot)
        assert_eq!(find("Thriller").sections.len(), 8);
        assert_eq!(find("Movin' Out").sections.len(), 8);
        assert_eq!(find("Girl Goodbye").sections.len(), 8);
        assert_eq!(find("Bennie And The Jets").sections.len(), 8);
        assert_eq!(find("I Don't Trust Myself").sections.len(), 8);
    }

    #[test]
    fn commercial_songs_base_profiles() {
        let all = songs();
        let find = |name: &str| all.iter().find(|s| s.name == name).unwrap();

        // Songs with base profiles
        assert_eq!(
            find("Thriller").metadata.base_profile_id.as_deref(),
            Some(seed_id("guitar-allaround-profile").to_string()).as_deref()
        );
        assert_eq!(
            find("Movin' Out").metadata.base_profile_id.as_deref(),
            Some(seed_id("guitar-allaround-profile").to_string()).as_deref()
        );
        assert_eq!(
            find("Girl Goodbye").metadata.base_profile_id.as_deref(),
            Some(seed_id("guitar-rock-profile").to_string()).as_deref()
        );
        assert_eq!(
            find("Bennie And The Jets")
                .metadata
                .base_profile_id
                .as_deref(),
            Some(seed_id("guitar-allaround-profile").to_string()).as_deref()
        );
        assert_eq!(
            find("I Don't Trust Myself")
                .metadata
                .base_profile_id
                .as_deref(),
            Some(seed_id("guitar-blues-profile").to_string()).as_deref()
        );

        // Songs without base profiles
        assert!(find("4 A.M.").metadata.base_profile_id.is_none());
        assert!(find("For Cryin' Out Loud")
            .metadata
            .base_profile_id
            .is_none());
        assert!(find("We Were Never Really Friends")
            .metadata
            .base_profile_id
            .is_none());
    }

    #[test]
    fn bennie_jets_mirrors_all_around_patches() {
        let song = songs()
            .into_iter()
            .find(|s| s.name == "Bennie And The Jets")
            .unwrap();
        let section_names: Vec<&str> = song.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            section_names,
            ["Clean", "Crunch", "Drive", "Lead", "Funk", "Ambient", "Q-Tron", "Solo"]
        );
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
