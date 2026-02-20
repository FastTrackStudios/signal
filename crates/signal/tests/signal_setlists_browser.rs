//! Tests for setlists, scene templates, browser/search, reorder APIs,
//! delete, and the set_section_source / set_patch_preset mutation helpers.
//!
//!   cargo test -p signal --test signal_setlists_browser -- --nocapture

mod fixtures;

use fixtures::*;
use signal::{
    engine::EngineId,
    overrides::{NodePath, Override},
    profile::{Patch, PatchId, PatchTarget, Profile, ProfileId},
    rig::{EngineSelection, RigId, RigSceneId},
    scene_template::SceneTemplate,
    seed_id,
    setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId},
    song::{Section, SectionId, SectionSource, Song, SongId},
    tagging::{BrowserMode, BrowserQuery},
};

// ─────────────────────────────────────────────────────────────
//  Setlist tests
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn seed_setlists_are_loaded() {
    let signal = controller().await;
    let setlists = signal.setlists().list().await.unwrap();
    println!("Seeded setlists:");
    for s in &setlists {
        println!("  {} — {} ({} entries)", s.id, s.name, s.entries.len());
    }
    assert!(!setlists.is_empty());
    let worship = setlists
        .iter()
        .find(|s| s.name == "Worship Set")
        .expect("Worship Set not found");
    assert_eq!(worship.entries.len(), 2);
}

#[tokio::test]
async fn create_and_reload_custom_setlist() {
    let signal = controller().await;

    let entry1_id = SetlistEntryId::new();
    let entry2_id = SetlistEntryId::new();
    let setlist_id = SetlistId::new();

    let entry1 = SetlistEntry::new(entry1_id.clone(), "Song A", seed_id("guitar-worship-song"));
    let entry2 = SetlistEntry::new(entry2_id.clone(), "Song B", seed_id("feature-demo-song"));

    let mut setlist = Setlist::new(setlist_id.clone(), "Custom Gig", entry1);
    setlist.add_entry(entry2);
    signal.setlists().save(setlist).await.unwrap();

    let reloaded = signal
        .setlists()
        .load(setlist_id.clone())
        .await
        .unwrap()
        .expect("setlist not found");
    println!(
        "Reloaded setlist '{}': {} entries",
        reloaded.name,
        reloaded.entries.len()
    );
    assert_eq!(reloaded.entries.len(), 2);
    assert_eq!(reloaded.entries[0].name, "Song A");
    assert_eq!(reloaded.entries[1].name, "Song B");
}

#[tokio::test]
async fn load_setlist_entry_by_id() {
    let signal = controller().await;

    let entry = signal
        .setlists()
        .load_entry(seed_id("worship-set"), seed_id("worship-set-worship-song"))
        .await
        .unwrap()
        .expect("setlist entry not found");

    println!("Entry '{}' → song_id={}", entry.name, entry.song_id);
    assert_eq!(entry.name, "Worship Set");
}

#[tokio::test]
async fn reorder_setlist_entries() {
    let signal = controller().await;

    let e1 = SetlistEntryId::new();
    let e2 = SetlistEntryId::new();
    let e3 = SetlistEntryId::new();
    let sid = SetlistId::new();

    let mut setlist = Setlist::new(
        sid.clone(),
        "Reorder Test",
        SetlistEntry::new(e1.clone(), "A", seed_id("dummy-song")),
    );
    setlist.add_entry(SetlistEntry::new(e2.clone(), "B", seed_id("dummy-song")));
    setlist.add_entry(SetlistEntry::new(e3.clone(), "C", seed_id("dummy-song")));
    signal.setlists().save(setlist).await.unwrap();

    // Use the controller's reorder method instead of duplicating the logic.
    signal
        .setlists()
        .reorder_entries(sid.clone(), &[e3.clone(), e1.clone(), e2.clone()])
        .await
        .unwrap();

    let setlist = signal
        .setlists()
        .load(sid.clone())
        .await
        .unwrap()
        .expect("setlist not found");

    println!(
        "After reorder: {:?}",
        setlist.entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(setlist.entries[0].name, "C");
    assert_eq!(setlist.entries[1].name, "A");
    assert_eq!(setlist.entries[2].name, "B");
}

#[tokio::test]
async fn delete_setlist() {
    let signal = controller().await;

    let sid = SetlistId::new();
    let setlist = Setlist::new(
        sid.clone(),
        "Temp Setlist",
        SetlistEntry::new(SetlistEntryId::new(), "A", seed_id("dummy-song")),
    );
    signal.setlists().save(setlist).await.unwrap();

    assert!(
        signal.setlists().load(sid.clone()).await.unwrap().is_some(),
        "should exist before delete"
    );
    signal.setlists().delete(sid.clone()).await.unwrap();
    assert!(
        signal.setlists().load(sid).await.unwrap().is_none(),
        "should be gone after delete"
    );
    println!("✓ Setlist deleted");
}

// ─────────────────────────────────────────────────────────────
//  Reorder APIs
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn reorder_profile_patches() {
    let signal = controller().await;

    let worship = signal
        .profiles()
        .load(seed_id("guitar-worship-profile"))
        .await
        .unwrap()
        .expect("worship not found");
    let original_order: Vec<String> = worship.patches.iter().map(|p| p.name.clone()).collect();

    // Move last patch to front
    let ids: Vec<_> = worship.patches.iter().map(|p| p.id.clone()).collect();
    let mut new_order = ids.clone();
    new_order.rotate_right(1);

    // Use the controller's reorder method.
    signal
        .profiles()
        .reorder_patches(seed_id("guitar-worship-profile"), &new_order)
        .await
        .unwrap();

    let worship = signal
        .profiles()
        .load(seed_id("guitar-worship-profile"))
        .await
        .unwrap()
        .expect("worship not found after reorder");
    let new_names: Vec<String> = worship.patches.iter().map(|p| p.name.clone()).collect();

    println!("Patch order: {:?} → {:?}", original_order, new_names);
    assert_ne!(original_order, new_names, "order should have changed");
    // First element after rotate should be what was last
    assert_eq!(new_names[0], original_order[original_order.len() - 1]);
}

#[tokio::test]
async fn reorder_song_sections() {
    let signal = controller().await;

    let s1 = SectionId::new();
    let s2 = SectionId::new();
    let s3 = SectionId::new();
    let song_id = SongId::new();

    let mut song = Song::new(
        song_id.clone(),
        "Reorder Song",
        Section::from_rig_scene(
            s1.clone(),
            "Verse",
            guitar_megarig_id(),
            guitar_megarig_default_scene(),
        ),
    );
    song.add_section(Section::from_rig_scene(
        s2.clone(),
        "Chorus",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    ));
    song.add_section(Section::from_rig_scene(
        s3.clone(),
        "Bridge",
        guitar_megarig_id(),
        guitar_megarig_lead_scene(),
    ));
    signal.songs().save(song).await.unwrap();

    // Use the controller's reorder method.
    signal
        .songs()
        .reorder_sections(song_id.clone(), &[s3.clone(), s1.clone(), s2.clone()])
        .await
        .unwrap();

    let song = signal
        .songs()
        .load(song_id)
        .await
        .unwrap()
        .expect("song not found");
    let names: Vec<&str> = song.sections.iter().map(|s| s.name.as_str()).collect();
    println!("Sections after reorder: {:?}", names);
    assert_eq!(names, ["Bridge", "Verse", "Chorus"]);
}

// ─────────────────────────────────────────────────────────────
//  Mutation helpers
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn set_section_source_switches_from_patch_to_rig_scene() {
    let signal = controller().await;

    let section_id = SectionId::new();
    let song_id = SongId::new();

    // Start with a Patch source
    let section = Section::from_patch(section_id.clone(), "Intro", seed_id("guitar-worship-clean"));
    let song = Song::new(song_id.clone(), "Mutation Test Song", section);
    signal.songs().save(song).await.unwrap();

    {
        let loaded = signal.songs().load(song_id.clone()).await.unwrap().unwrap();
        let s = loaded.section(&section_id).unwrap();
        assert!(
            matches!(s.source, SectionSource::Patch { .. }),
            "should start as Patch"
        );
        println!("Before: {:?}", s.source);
    }

    // Switch to RigScene source
    signal
        .songs()
        .set_section_source(
            song_id.clone(),
            section_id.clone(),
            SectionSource::RigScene {
                rig_id: guitar_megarig_id(),
                scene_id: guitar_megarig_lead_scene(),
            },
        )
        .await
        .unwrap();

    let reloaded = signal.songs().load(song_id).await.unwrap().unwrap();
    let updated = reloaded.section(&section_id).unwrap();
    println!("After: {:?}", updated.source);
    assert!(
        matches!(updated.source, SectionSource::RigScene { .. }),
        "should now be RigScene"
    );
}

#[tokio::test]
async fn set_patch_preset_retargets_rig_scene() {
    let signal = controller().await;

    // Load rock profile, retarget Clean patch from default → lead scene
    // (Blues patches are now BlockSnapshot targets, so use Rock which still uses RigScene)
    let before = signal
        .profiles()
        .load_patch(seed_id("guitar-rock-profile"), seed_id("guitar-rock-clean"))
        .await
        .unwrap()
        .expect("rock clean not found");

    match &before.target {
        PatchTarget::RigScene { scene_id, .. } => {
            println!("Before retarget: scene={}", scene_id);
            assert_eq!(
                scene_id.to_string(),
                seed_id("guitar-megarig-default").to_string()
            );
        }
        _ => panic!("expected RigScene target"),
    }

    signal
        .profiles()
        .set_patch_preset(
            seed_id("guitar-rock-profile"),
            seed_id("guitar-rock-clean"),
            guitar_megarig_id(),
            guitar_megarig_lead_scene(),
        )
        .await
        .unwrap();

    let after = signal
        .profiles()
        .load_patch(seed_id("guitar-rock-profile"), seed_id("guitar-rock-clean"))
        .await
        .unwrap()
        .expect("rock clean not found after retarget");

    match &after.target {
        PatchTarget::RigScene { scene_id, .. } => {
            println!("After retarget: scene={}", scene_id);
            assert_eq!(
                scene_id.to_string(),
                seed_id("guitar-megarig-lead").to_string(),
                "patch should now target lead scene"
            );
        }
        _ => panic!("expected RigScene target"),
    }
}

// ─────────────────────────────────────────────────────────────
//  Delete operations
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_profile_and_verify_gone() {
    let signal = controller().await;

    let id = ProfileId::new();
    let patch = Patch::from_rig_scene(
        PatchId::new(),
        "P",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    );
    signal
        .profiles()
        .save(Profile::new(id.clone(), "Temp Profile", patch))
        .await
        .unwrap();
    assert!(signal.profiles().load(id.clone()).await.unwrap().is_some());

    signal.profiles().delete(id.clone()).await.unwrap();
    assert!(
        signal.profiles().load(id).await.unwrap().is_none(),
        "profile should be deleted"
    );
    println!("✓ Profile deleted");
}

#[tokio::test]
async fn delete_song_and_verify_gone() {
    let signal = controller().await;

    let id = SongId::new();
    let section = Section::from_rig_scene(
        SectionId::new(),
        "S",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    );
    signal
        .songs()
        .save(Song::new(id.clone(), "Temp Song", section))
        .await
        .unwrap();
    assert!(signal.songs().load(id.clone()).await.unwrap().is_some());

    signal.songs().delete(id.clone()).await.unwrap();
    assert!(
        signal.songs().load(id).await.unwrap().is_none(),
        "song should be deleted"
    );
    println!("✓ Song deleted");
}

// ─────────────────────────────────────────────────────────────
//  Scene Template tests
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn save_and_reload_scene_template() {
    let signal = controller().await;

    let tmpl = SceneTemplate::new(
        signal::scene_template::SceneTemplateId::new(),
        "JM Clean Template",
    )
    .with_engine(EngineSelection::new(
        EngineId::from(seed_id("guitar-engine")),
        seed_id("guitar-engine-default"),
    ))
    .with_override(Override::set(
        NodePath::engine("guitar-engine")
            .with_layer("guitar-layer-archetype-jm")
            .with_block("amp")
            .with_parameter("gain"),
        0.20,
    ));

    let tmpl_id = tmpl.id.clone();
    signal.scene_templates().save(tmpl).await.unwrap();

    let reloaded = signal
        .scene_templates()
        .load(tmpl_id.clone())
        .await
        .unwrap()
        .expect("template not found");
    println!(
        "Template '{}': {} overrides",
        reloaded.name,
        reloaded.overrides.len()
    );
    assert_eq!(reloaded.name, "JM Clean Template");
    assert_eq!(reloaded.overrides.len(), 1);

    // Convert template to a RigScene and verify it carries the override
    let scene = reloaded.to_rig_scene(RigSceneId::new());
    assert_eq!(scene.overrides.len(), 1);
    assert!(scene.overrides[0].path.as_str().contains("gain"));
    println!(
        "✓ Template → RigScene override path: {}",
        scene.overrides[0].path.as_str()
    );
}

#[tokio::test]
async fn list_and_delete_scene_templates() {
    let signal = controller().await;

    let before = signal.scene_templates().list().await.unwrap().len();

    let id1 = signal::scene_template::SceneTemplateId::new();
    let id2 = signal::scene_template::SceneTemplateId::new();
    signal
        .scene_templates()
        .save(SceneTemplate::new(id1.clone(), "T1"))
        .await
        .unwrap();
    signal
        .scene_templates()
        .save(SceneTemplate::new(id2.clone(), "T2"))
        .await
        .unwrap();

    assert_eq!(
        signal.scene_templates().list().await.unwrap().len(),
        before + 2
    );

    signal.scene_templates().delete(id1).await.unwrap();
    assert_eq!(
        signal.scene_templates().list().await.unwrap().len(),
        before + 1
    );

    signal.scene_templates().delete(id2).await.unwrap();
    assert_eq!(signal.scene_templates().list().await.unwrap().len(), before);
    println!("✓ Scene templates created and deleted");
}

#[tokio::test]
async fn reorder_scene_templates() {
    let signal = controller().await;

    let t1 = signal::scene_template::SceneTemplateId::new();
    let t2 = signal::scene_template::SceneTemplateId::new();
    let t3 = signal::scene_template::SceneTemplateId::new();

    signal
        .scene_templates()
        .save(SceneTemplate::new(t1.clone(), "Alpha"))
        .await
        .unwrap();
    signal
        .scene_templates()
        .save(SceneTemplate::new(t2.clone(), "Beta"))
        .await
        .unwrap();
    signal
        .scene_templates()
        .save(SceneTemplate::new(t3.clone(), "Gamma"))
        .await
        .unwrap();

    // Reorder: Gamma, Alpha, Beta
    signal
        .scene_templates()
        .reorder(vec![t3.clone(), t1.clone(), t2.clone()])
        .await
        .unwrap();

    let templates = signal.scene_templates().list().await.unwrap();
    // Find our three and check relative order
    let positions: Vec<(usize, &str)> = templates
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            if t.id == t1 {
                Some((i, "Alpha"))
            } else if t.id == t2 {
                Some((i, "Beta"))
            } else if t.id == t3 {
                Some((i, "Gamma"))
            } else {
                None
            }
        })
        .collect();

    println!("Template positions after reorder: {:?}", positions);
    assert_eq!(positions.len(), 3);
    // Gamma should come before Alpha, Alpha before Beta
    let gamma_pos = positions.iter().find(|(_, n)| *n == "Gamma").unwrap().0;
    let alpha_pos = positions.iter().find(|(_, n)| *n == "Alpha").unwrap().0;
    let beta_pos = positions.iter().find(|(_, n)| *n == "Beta").unwrap().0;
    assert!(
        gamma_pos < alpha_pos && alpha_pos < beta_pos,
        "order should be Gamma < Alpha < Beta"
    );
}

// ─────────────────────────────────────────────────────────────
//  Browser / search tests
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn browser_index_covers_all_domain_levels() {
    let signal = controller().await;

    let index = signal.browser_index().await.unwrap();
    let entries = index.entries();
    println!("Browser index: {} entries", entries.len());

    assert!(
        entries.len() > 50,
        "index should have many entries across all domain levels"
    );

    // Spot-check: JM block presets should appear
    let jm_entries: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.name.contains("JM") || e.name.contains("Justa") || e.name.contains("Archetype")
        })
        .collect();
    println!("JM-related entries: {}", jm_entries.len());
    assert!(
        !jm_entries.is_empty(),
        "JM blocks/modules should appear in browser index"
    );
}

fn text_query(text: &str) -> BrowserQuery {
    BrowserQuery {
        text: Some(text.to_string()),
        ..BrowserQuery::default()
    }
}

#[tokio::test]
async fn browse_by_text_finds_jm_amp() {
    let signal = controller().await;
    let index = signal.browser_index().await.unwrap();

    let hits = signal.browse(text_query("JM Amp")).await.unwrap();
    println!("Browse 'JM Amp': {} hits", hits.len());
    for h in hits.iter().take(5) {
        // Look up name in the index
        let name = index
            .entries()
            .iter()
            .find(|e| e.node == h.node)
            .map(|e| e.name.as_str())
            .unwrap_or("?");
        println!("  {:?} — {} (score={:.2})", h.node.kind, name, h.score);
    }
    assert!(!hits.is_empty(), "should find hits for 'JM Amp'");
    // Semantic scoring matches by tag overlap, not name prefix — verify that
    // some entry in the top results references a JM/Amp-related entity.
    let hit_names: Vec<&str> = hits
        .iter()
        .take(10)
        .filter_map(|h| index.entries().iter().find(|e| e.node == h.node))
        .map(|e| e.name.as_str())
        .collect();
    println!("Top 10 hit names: {:?}", hit_names);
    assert!(
        hit_names
            .iter()
            .any(|n| n.contains("JM") || n.contains("Amp")),
        "some top hits should reference JM or Amp, got: {:?}",
        hit_names
    );
}

#[tokio::test]
async fn browse_by_text_finds_guitar_worship() {
    let signal = controller().await;

    let hits = signal.browse(text_query("Worship")).await.unwrap();
    println!("Browse 'Worship': {} hits", hits.len());
    assert!(!hits.is_empty(), "should find worship-related hits");
    for h in hits.iter().take(3) {
        println!("  {:?} score={:.2}", h.node.kind, h.score);
    }
}

#[tokio::test]
async fn browse_returns_empty_for_nonsense_query() {
    let signal = controller().await;
    let hits = signal
        .browse(text_query("xyzzy_definitely_not_a_preset_zzz"))
        .await
        .unwrap();
    println!("Browse nonsense: {} hits", hits.len());
    assert!(
        hits.is_empty() || hits.iter().all(|h| h.score < 0.1),
        "nonsense query should return no high-confidence hits"
    );
}

#[tokio::test]
async fn list_rig_collections_by_guitar_tag() {
    let signal = controller().await;
    let rigs = signal.rigs().by_tag("guitar").await.unwrap();
    println!("Guitar-tagged rigs: {}", rigs.len());
    for r in &rigs {
        println!("  {} — {}", r.id, r.name);
    }
    assert!(!rigs.is_empty(), "should find guitar-tagged rigs");
}

#[tokio::test]
async fn list_profiles_by_worship_tag() {
    let signal = controller().await;
    let profiles = signal.profiles().by_tag("worship").await.unwrap();
    println!("Worship-tagged profiles: {}", profiles.len());
    assert!(!profiles.is_empty(), "should find worship-tagged profiles");
    assert!(
        profiles.iter().any(|p| p.name == "Worship"),
        "Worship profile should be found"
    );
}

// ─────────────────────────────────────────────────────────────
//  Resolve: all JM presets via every path
// ─────────────────────────────────────────────────────────────

/// Walk every patch in every guitar profile and resolve it — verifying the
/// full pipeline works for all 24 guitar patches.
#[tokio::test]
async fn resolve_all_guitar_profile_patches() {
    use signal::resolve::ResolveTarget;

    let signal = controller().await;
    let profiles = signal.profiles().by_tag("guitar").await.unwrap();

    let mut total = 0;
    let mut errors = vec![];

    for profile in &profiles {
        for patch in &profile.patches {
            let result = signal
                .resolve_target(ResolveTarget::ProfilePatch {
                    profile_id: profile.id.clone().into(),
                    patch_id: patch.id.clone().into(),
                })
                .await;

            match result {
                Ok(graph) => {
                    total += 1;
                    let block_count: usize = graph
                        .engines
                        .iter()
                        .flat_map(|e| &e.layers)
                        .flat_map(|l| &l.modules)
                        .map(|m| m.blocks.len())
                        .sum();
                    println!(
                        "  ✓ {}/{}: {} engines, ~{} blocks, {} overrides",
                        profile.name,
                        patch.name,
                        graph.engines.len(),
                        block_count,
                        graph.effective_overrides.len()
                    );
                }
                Err(e) => {
                    errors.push(format!("{}/{}: {:?}", profile.name, patch.name, e));
                    println!("  ✗ {}/{}: {:?}", profile.name, patch.name, e);
                }
            }
        }
    }

    println!("\nResolved {total} patches, {} errors", errors.len());
    assert!(
        errors.is_empty(),
        "some patches failed to resolve:\n{}",
        errors.join("\n")
    );
}

/// Walk every section in every seeded song and resolve it.
#[tokio::test]
async fn resolve_all_seeded_song_sections() {
    use signal::resolve::ResolveTarget;

    let signal = controller().await;
    let songs = signal.songs().list().await.unwrap();

    let mut total = 0;
    let mut errors = vec![];

    for song in &songs {
        for section in &song.sections {
            let result = signal
                .resolve_target(ResolveTarget::SongSection {
                    song_id: song.id.clone().into(),
                    section_id: section.id.clone().into(),
                })
                .await;

            match result {
                Ok(graph) => {
                    total += 1;
                    println!(
                        "  ✓ {}/{}: {} engines, {} overrides",
                        song.name,
                        section.name,
                        graph.engines.len(),
                        graph.effective_overrides.len()
                    );
                }
                Err(e) => {
                    errors.push(format!("{}/{}: {:?}", song.name, section.name, e));
                    println!("  ✗ {}/{}: {:?}", song.name, section.name, e);
                }
            }
        }
    }

    println!("\nResolved {total} sections, {} errors", errors.len());
    assert!(
        errors.is_empty(),
        "some sections failed to resolve:\n{}",
        errors.join("\n")
    );
}
