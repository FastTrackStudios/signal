use dawfile_standalone::{DawProject, EntityId};

#[test]
fn added_items_and_takes_keep_file_and_opaque_sources() {
    for source in [
        "<SOURCE WAVE\nFILE \"Media/drum take.wav\"\n>",
        "<SOURCE MIDI\nHASDATA 1 960 QN\nE 0 90 24 7f\n>",
    ] {
        let text = format!(
            "<REAPER_PROJECT 0.1 7 1\n<TRACK {{11111111-1111-1111-1111-111111111111}}\nNAME Drums\n<ITEM\nPOSITION 0\nLENGTH 2\nIGUID {{22222222-2222-2222-2222-222222222222}}\nNAME Hit\nGUID {{33333333-3333-3333-3333-333333333333}}\n{source}\n>\n>\n>\n"
        );
        let (mut project, _) = DawProject::import_rpp(&text, "fixture").unwrap();
        let expected = project.document().tracks[0].items[0].takes[0]
            .source
            .clone();
        project.edit(|doc| {
            let track = &mut doc.tracks[0];
            let mut added = track.items[0].clone();
            added.id = EntityId::adopt("{44444444-4444-4444-4444-444444444444}");
            added.item.guid = added.id.as_str().into();
            added.takes[0].id = EntityId::adopt("{55555555-5555-5555-5555-555555555555}");
            added.takes[0].take.guid = added.takes[0].id.as_str().into();
            track.items.push(added);
            let mut take = track.items[0].takes[0].clone();
            take.id = EntityId::adopt("{66666666-6666-6666-6666-666666666666}");
            take.take.guid = take.id.as_str().into();
            track.items[0].takes.push(take);
        });
        let exported = project.to_rpp().unwrap();
        let (reopened, _) = DawProject::import_rpp(&exported, "reopened").unwrap();
        let items = &reopened.document().tracks[0].items;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].takes.len(), 2);
        assert_eq!(items[1].takes.len(), 1);
        for take in items.iter().flat_map(|item| &item.takes) {
            assert_eq!(take.source, expected);
        }
    }
}

#[test]
fn empty_take_slots_do_not_change_active_source_offsets() {
    let text = "<REAPER_PROJECT 0.1 7 1
<TRACK {11111111-1111-1111-1111-111111111111}
NAME Drums
TRACKID {11111111-1111-1111-1111-111111111111}
<ITEM
POSITION 0
LENGTH 2
IGUID {22222222-2222-2222-2222-222222222222}
VOLPAN 1
TAKE SEL
NAME Hit
SOFFS 2.8576875
GUID {33333333-3333-3333-3333-333333333333}
<SOURCE WAVE
FILE drum.wav
>
TAKE NULL
TAKE NULL
>
>
>
";
    let (project, _) = DawProject::import_rpp(text, "fixture").unwrap();
    let exported = project.to_rpp_patched().unwrap().0;
    let (reopened, _) = DawProject::import_rpp(&exported, "reopened").unwrap();
    let before = &project.document().tracks[0].items[0].takes;
    let after = &reopened.document().tracks[0].items[0].takes;
    assert_eq!(before.len(), after.len(), "{exported}");
    for (before, after) in before.iter().zip(after) {
        assert_eq!(before.source, after.source);
        assert_eq!(before.take.start_offset, after.take.start_offset);
        assert_eq!(before.take.name, after.take.name);
    }
}

#[test]
fn duplicated_item_saves_the_selected_take() {
    use daw::service::{
        ItemRef, Items, ProjectContext, Projects, TakeRef, Takes, TrackRef, Tracks,
    };
    use daw::standalone::sync::Standalone;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("takes.rpp");
    std::fs::write(
        &path,
        "<REAPER_PROJECT 0.1 7 1
<TRACK {11111111-1111-1111-1111-111111111111}
NAME Drums
TRACKID {11111111-1111-1111-1111-111111111111}
<ITEM
POSITION 0
LENGTH 2
IGUID {22222222-2222-2222-2222-222222222222}
NAME First
GUID {33333333-3333-3333-3333-333333333333}
<SOURCE WAVE
FILE first.wav
>
TAKE SEL
NAME Selected
GUID {44444444-4444-4444-4444-444444444444}
<SOURCE WAVE
FILE selected.wav
>
>
>
>
",
    )
    .unwrap();
    let daw = Standalone::new();
    let project = daw.open(path.to_str().unwrap()).unwrap();
    let ctx = ProjectContext::Project(project.guid.clone());
    let track = Tracks::all(&daw, ctx.clone()).remove(0);
    let item = daw
        .get_items(ctx.clone(), TrackRef::Guid(track.guid))
        .remove(0);
    daw.duplicate_item(ctx, ItemRef::Guid(item.guid)).unwrap();
    let saved = daw::standalone::save::save_project_as(&daw, &project.guid).unwrap();
    let reopened = Standalone::new();
    let project = reopened.open(saved.to_str().unwrap()).unwrap();
    let ctx = ProjectContext::Project(project.guid);
    let track = Tracks::all(&reopened, ctx.clone()).remove(0);
    let items = reopened.get_items(ctx.clone(), TrackRef::Guid(track.guid));
    assert_eq!(items.len(), 2);
    for item in items {
        let take = reopened
            .get_take(ctx.clone(), ItemRef::Guid(item.guid), TakeRef::Active)
            .unwrap();
        assert_eq!(take.name, "Selected");
        assert_eq!(take.source_file_path.as_deref(), Some("selected.wav"));
    }
}
