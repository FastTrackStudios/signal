//! End-to-end tests for the project Media/FX Bay.

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{Effects, FxChainContext, ItemRef, ProjectInfo, Takes, TrackRef, Tracks};
use daw_standalone::media_bay::{BayView, InMemoryResolver, ReplaceScope};
use daw_standalone::sync::Standalone;

fn seeded() -> (Standalone, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    (daw, guid)
}

/// Create an audio item on `track_guid` whose active take's
/// `source_file_path` equals `source`.
fn add_audio_item(
    daw: &Standalone,
    project_guid: &str,
    track_guid: &str,
    start: f64,
    length: f64,
    source: &str,
) -> String {
    let ctx = ProjectContext::Project(project_guid.to_string());
    let loc = Midi::create_midi_item(
        daw,
        ctx.clone(),
        TrackRef::Guid(track_guid.to_string()),
        start,
        start + length,
    )
    .unwrap();
    let item_guid = match loc.item {
        ItemRef::Guid(g) => g,
        _ => panic!(),
    };
    let active = Takes::get_active_take(daw, ctx, ItemRef::Guid(item_guid.clone())).unwrap();
    daw.write_project(project_guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.source_file_path = Some(source.to_string());
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                }
            }
        }
    });
    item_guid
}

#[test]
fn source_media_dedupes_by_path() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();

    // Three items pointing at two unique files.
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "kick.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "kick.wav");
    add_audio_item(&daw, &guid, &t, 2.0, 1.0, "snare.wav");

    let bay = daw.media_bay();
    let entries = bay.list(ctx.clone(), BayView::SourceMedia, "");
    assert_eq!(entries.len(), 2, "expected 2 unique sources");
    let kick = entries.iter().find(|e| e.name == "kick.wav").unwrap();
    assert_eq!(kick.usage_count, 2);
    let snare = entries.iter().find(|e| e.name == "snare.wav").unwrap();
    assert_eq!(snare.usage_count, 1);
}

#[test]
fn media_items_lists_all_items() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "a.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "b.wav");

    let entries = daw.media_bay().list(ctx, BayView::MediaItems, "");
    assert_eq!(entries.len(), 2);
}

#[test]
fn list_filter_is_case_insensitive_substring() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "PIANO_loop.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "kick.wav");

    let bay = daw.media_bay();
    let filtered = bay.list(ctx, BayView::SourceMedia, "piano");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "PIANO_loop.wav");
}

#[test]
fn usages_returns_ordinals_in_stable_order() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let _ = add_audio_item(&daw, &guid, &t, 0.0, 1.0, "k.wav");
    let _ = add_audio_item(&daw, &guid, &t, 1.0, 1.0, "k.wav");
    let _ = add_audio_item(&daw, &guid, &t, 2.0, 1.0, "k.wav");

    let usages = daw.media_bay().usages(ctx, "k.wav");
    assert_eq!(usages.len(), 3);
    assert_eq!(usages[0].ordinal, 0);
    assert_eq!(usages[1].ordinal, 1);
    assert_eq!(usages[2].ordinal, 2);
}

#[test]
fn rename_source_updates_all_takes() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "old.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "old.wav");

    daw.media_bay()
        .rename_source(ctx.clone(), "old.wav", "new.wav")
        .unwrap();

    assert_eq!(daw.media_bay().usage_count(ctx.clone(), "old.wav"), 0);
    assert_eq!(daw.media_bay().usage_count(ctx, "new.wav"), 2);
}

#[test]
fn replace_in_project_single_instance_picks_by_ordinal() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "src.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "src.wav");
    add_audio_item(&daw, &guid, &t, 2.0, 1.0, "src.wav");

    // Replace only the 2nd instance (ordinal 1).
    daw.media_bay()
        .replace_in_project(
            ctx.clone(),
            "src.wav",
            "alt.wav",
            ReplaceScope::SingleInstance { ordinal: 1 },
        )
        .unwrap();

    assert_eq!(daw.media_bay().usage_count(ctx.clone(), "src.wav"), 2);
    assert_eq!(daw.media_bay().usage_count(ctx, "alt.wav"), 1);
}

#[test]
fn replace_in_project_all_instances() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "a.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "a.wav");

    daw.media_bay()
        .replace_in_project(ctx.clone(), "a.wav", "b.wav", ReplaceScope::AllInstances)
        .unwrap();
    assert_eq!(daw.media_bay().usage_count(ctx.clone(), "a.wav"), 0);
    assert_eq!(daw.media_bay().usage_count(ctx, "b.wav"), 2);
}

#[test]
fn mute_all_uses_propagates_to_items() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let i1 = add_audio_item(&daw, &guid, &t, 0.0, 1.0, "s.wav");
    let i2 = add_audio_item(&daw, &guid, &t, 1.0, 1.0, "s.wav");

    daw.media_bay()
        .set_muted_all_uses(ctx.clone(), "s.wav", true)
        .unwrap();
    // Both items now muted.
    let items = daw.media_bay().list(ctx.clone(), BayView::MediaItems, "");
    for item in items.iter() {
        if item.id == i1 || item.id == i2 {
            assert_eq!(
                item.all_muted,
                Some(true),
                "item {} should be muted",
                item.id
            );
        }
    }
    // Source entry reports all_muted = true.
    let src = daw
        .media_bay()
        .get(ctx, BayView::SourceMedia, "s.wav")
        .unwrap();
    assert_eq!(src.all_muted, Some(true));
}

#[test]
fn retained_keeps_source_after_all_usages_removed() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "keep.wav");

    let bay = daw.media_bay();
    bay.set_retained(ctx.clone(), "keep.wav", true).unwrap();

    // Wipe all takes — usage_count drops to 0 but retained keeps it.
    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                t.source_file_path = None;
            }
        }
    });

    let entries = bay.list(ctx, BayView::SourceMedia, "");
    let kept = entries
        .iter()
        .find(|e| e.name == "keep.wav")
        .expect("retained source survived");
    assert_eq!(kept.usage_count, 0);
    assert!(kept.retained);
}

#[test]
fn bay_folders_organize_entries() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "kick.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "snare.wav");

    let bay = daw.media_bay();
    bay.create_bay_folder(ctx.clone(), BayView::SourceMedia, "drums")
        .unwrap();
    bay.move_to_bay_folder(ctx.clone(), BayView::SourceMedia, "kick.wav", Some("drums"))
        .unwrap();

    let entries = bay.list(ctx.clone(), BayView::SourceMedia, "");
    let kick = entries.iter().find(|e| e.name == "kick.wav").unwrap();
    assert_eq!(kick.bay_folder.as_deref(), Some("drums"));
    let snare = entries.iter().find(|e| e.name == "snare.wav").unwrap();
    assert!(snare.bay_folder.is_none());

    // Move out → folder cleared.
    bay.move_to_bay_folder(ctx.clone(), BayView::SourceMedia, "kick.wav", None)
        .unwrap();
    let entries = bay.list(ctx, BayView::SourceMedia, "");
    let kick = entries.iter().find(|e| e.name == "kick.wav").unwrap();
    assert!(kick.bay_folder.is_none());
}

#[test]
fn save_load_bay_round_trips_retained_and_folders() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "drums.wav");

    let bay = daw.media_bay();
    bay.set_retained(ctx.clone(), "drums.wav", true).unwrap();
    bay.create_bay_folder(ctx.clone(), BayView::SourceMedia, "loops")
        .unwrap();
    bay.move_to_bay_folder(
        ctx.clone(),
        BayView::SourceMedia,
        "drums.wav",
        Some("loops"),
    )
    .unwrap();

    let snapshot = bay.save_bay(ctx.clone());
    assert!(!snapshot.is_empty(), "snapshot should be non-empty");

    // Fresh project: load_bay merges retained + folders.
    let daw2 = Standalone::new();
    let guid2 = daw2.seed_project(ProjectInfo {
        guid: "p2".into(),
        name: "p2".into(),
        path: String::new(),
    });
    let ctx2 = ProjectContext::Project(guid2.clone());
    let bay2 = daw2.media_bay();
    bay2.load_bay(ctx2.clone(), snapshot).unwrap();

    assert_eq!(bay2.retained(ctx2.clone()), vec!["drums.wav".to_string()]);
    let folders = bay2.bay_folders(ctx2, BayView::SourceMedia);
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "loops");
    assert_eq!(folders[0].entries, vec!["drums.wav".to_string()]);
}

#[test]
fn effects_view_lists_fx_chain_entries() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(t.clone()),
        "ReaComp",
    )
    .unwrap();
    Effects::add(&daw, ctx.clone(), FxChainContext::Track(t), "ReaEQ").unwrap();

    let entries = daw.media_bay().list(ctx, BayView::Effects, "");
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"ReaComp"));
    assert!(names.contains(&"ReaEQ"));
}

#[cfg(feature = "decode")]
#[test]
fn materialize_via_bay_uses_installed_resolver() {
    use daw_standalone::audio_engine::materialize::materialize_via_bay;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "tone.wav");

    // Without a resolver installed, materialize_via_bay surfaces an
    // error from the bay rather than silently no-op'ing.
    assert!(materialize_via_bay(&daw, &guid).is_ok_and(|r| !r.failed.is_empty()));

    // With an in-memory resolver, the take's bytes resolve cleanly.
    let mem = InMemoryResolver::new();
    mem.insert("tone.wav", build_wav_440hz_1s());
    daw.media_bay().set_file_resolver(Box::new(mem));

    let report = materialize_via_bay(&daw, &guid).unwrap();
    assert_eq!(report.loaded, 1, "expected 1 source loaded: {report:?}");
    assert!(report.failed.is_empty(), "failed: {:?}", report.failed);
    let count = daw.audio_source_count(&guid);
    assert_eq!(count, 1);
}

#[cfg(feature = "decode")]
fn build_wav_440hz_1s() -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let n = sample_rate as usize;
    let mut data = Vec::with_capacity(44 + n * 2);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    data.extend_from_slice(b"WAVE");
    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&sample_rate.to_le_bytes());
    data.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&((n as u32) * 2).to_le_bytes());
    for i in 0..n {
        let t = i as f64 / sample_rate as f64;
        let s = (t * 440.0 * 2.0 * std::f64::consts::PI).sin();
        data.extend_from_slice(&((s * i16::MAX as f64) as i16).to_le_bytes());
    }
    data
}

#[test]
fn paths_to_resolve_enumerates_unique_sources_sorted() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    add_audio_item(&daw, &guid, &t, 0.0, 1.0, "drums/snare.wav");
    add_audio_item(&daw, &guid, &t, 1.0, 1.0, "drums/kick.wav");
    add_audio_item(&daw, &guid, &t, 2.0, 1.0, "drums/snare.wav"); // dup
    add_audio_item(&daw, &guid, &t, 3.0, 1.0, "piano.wav");

    let paths = daw.media_bay().paths_to_resolve(ctx);
    // Deduped + sorted lexicographically.
    assert_eq!(
        paths,
        vec![
            "drums/kick.wav".to_string(),
            "drums/snare.wav".to_string(),
            "piano.wav".to_string(),
        ]
    );
}

#[test]
fn file_resolver_is_proxied_through_bay() {
    let (daw, _guid) = seeded();
    let bay = daw.media_bay();

    // No resolver installed → error.
    assert!(bay.resolve_file("anything.wav").is_err());

    // Install an in-memory resolver.
    let mem = InMemoryResolver::new();
    mem.insert("kick.wav", vec![1, 2, 3, 4]);
    bay.set_file_resolver(Box::new(mem));

    let bytes = bay.resolve_file("kick.wav").expect("resolved");
    assert_eq!(bytes, vec![1, 2, 3, 4]);

    // Path that wasn't registered.
    assert!(bay.resolve_file("missing.wav").is_err());
}
