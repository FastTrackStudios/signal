//! Templates as project fragments (#175).

use dawfile_standalone::document::{DawDocument, TrackNode};
use dawfile_standalone::id::{EntityId, ObjectId};
use dawfile_standalone::objects::ObjectStore;
use dawfile_standalone::project::DawProject;
use dawfile_standalone::template::DawTemplate;

fn track(name: &str, fx: Option<ObjectId>) -> TrackNode {
    let id = EntityId::new();
    let mut t = daw_proto::track::Track::default();
    t.guid = id.to_string();
    t.name = name.into();
    TrackNode {
        id,
        track: t,
        parent: None,
        envelopes: Vec::new(),
        items: Vec::new(),
        fx_chain: fx,
        input_fx_chain: None,
        comping: daw_proto::track::LaneComping::default(),
        receives: Vec::new(),
    }
}

fn project() -> DawProject {
    DawProject::new(DawDocument::new("Belief"), ObjectStore::new())
}

#[test]
fn a_template_is_captured_from_a_selection() {
    let mut p = project();
    p.edit(|d| {
        d.tracks.push(track("Lead Vox", None));
        d.tracks.push(track("Kit", None));
        d.tracks.push(track("Bass", None));
    });
    let wanted = vec![p.document().tracks[1].id.clone()];

    let t = DawTemplate::from_tracks(&p, "Kit", &wanted);
    assert_eq!(t.document().tracks.len(), 1);
    assert_eq!(t.document().tracks[0].track.name, "Kit");
}

#[test]
fn a_template_carries_only_the_objects_its_selection_references() {
    // A one-track template must not drag the other thirty-nine tracks'
    // FX chunks along with it.
    let mut p = project();
    let mine = p.put_object(b"my fx".to_vec());
    let theirs = p.put_object(b"someone else's fx".to_vec());
    p.edit(|d| {
        d.tracks.push(track("Kit", Some(mine.clone())));
        d.tracks.push(track("Bass", Some(theirs.clone())));
    });
    let wanted = vec![p.document().tracks[0].id.clone()];

    let t = DawTemplate::from_tracks(&p, "Kit", &wanted);
    assert!(t.objects().contains(&mine));
    assert!(!t.objects().contains(&theirs), "it took the wrong chunk");
    assert_eq!(t.objects().len(), 1);
}

#[test]
fn loading_a_template_brings_its_objects_so_the_project_stays_self_contained() {
    let mut source = project();
    let fx = source.put_object(b"chunk".to_vec());
    source.edit(|d| d.tracks.push(track("Kit", Some(fx.clone()))));
    let wanted = vec![source.document().tracks[0].id.clone()];
    let t = DawTemplate::from_tracks(&source, "Kit", &wanted);

    let mut target = project();
    assert!(!target.objects().contains(&fx));
    t.instantiate(&mut target).unwrap();

    assert!(
        target.objects().contains(&fx),
        "the receiving project would point at bytes it does not hold"
    );

    // And it can therefore save without the missing-object guard firing.
    let dir = std::env::temp_dir().join(format!("fts-tmpl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    target.save(&dir).expect("a self-contained project saves");
}

#[test]
fn loading_the_same_template_twice_produces_two_distinct_sets() {
    let mut source = project();
    source.edit(|d| d.tracks.push(track("Kit", None)));
    let wanted = vec![source.document().tracks[0].id.clone()];
    let t = DawTemplate::from_tracks(&source, "Kit", &wanted);

    let mut target = project();
    t.instantiate(&mut target).unwrap();
    t.instantiate(&mut target).unwrap();

    assert_eq!(target.document().tracks.len(), 2);
    assert_ne!(
        target.document().tracks[0].id,
        target.document().tracks[1].id,
        "a second load collided with the first"
    );
}

#[test]
fn instantiating_gives_fresh_ids_and_keeps_the_guid_in_step() {
    let mut source = project();
    source.edit(|d| d.tracks.push(track("Kit", None)));
    let original = source.document().tracks[0].id.clone();
    let t = DawTemplate::from_tracks(&source, "Kit", std::slice::from_ref(&original));

    let mut target = project();
    let remap = t.instantiate(&mut target).unwrap();

    let new_id = &target.document().tracks[0].id;
    assert_ne!(new_id, &original);
    assert_eq!(remap.get(&original), Some(new_id));
    assert_eq!(
        target.document().tracks[0].track.guid,
        new_id.to_string(),
        "the facade guid follows the node id"
    );
}

#[test]
fn a_group_template_keeps_its_shape() {
    // Parents are remapped, so the copy points at the *copy's* parent
    // rather than back at the source project's track.
    let mut source = project();
    source.edit(|d| {
        let parent = track("Drums", None);
        let mut child = track("Snare", None);
        child.parent = Some(parent.id.clone());
        d.tracks.push(parent);
        d.tracks.push(child);
    });
    let ids: Vec<EntityId> = source
        .document()
        .tracks
        .iter()
        .map(|t| t.id.clone())
        .collect();
    let t = DawTemplate::from_tracks(&source, "Drums", &ids);

    let mut target = project();
    t.instantiate(&mut target).unwrap();

    let parent_id = target.document().tracks[0].id.clone();
    assert_eq!(
        target.document().tracks[1].parent.as_ref(),
        Some(&parent_id),
        "the child points at the copied parent"
    );
    assert!(!ids.contains(&parent_id), "and not at the source's");
}

#[test]
fn a_template_is_inspectable_exactly_like_a_project() {
    // The point of storing it in the identical format: one serializer,
    // one parser, one test suite.
    let mut source = project();
    source.edit(|d| d.tracks.push(track("Kit", None)));
    let ids = vec![source.document().tracks[0].id.clone()];
    let t = DawTemplate::from_tracks(&source, "Kit", &ids);

    let as_project = DawProject::new(t.document().clone(), t.objects().clone());
    let text = as_project
        .to_text()
        .expect("a template serializes as a project");
    assert!(text.contains("Kit"));
}

#[test]
fn an_empty_selection_makes_an_empty_template() {
    let mut p = project();
    p.edit(|d| d.tracks.push(track("Kit", None)));
    let t = DawTemplate::from_tracks(&p, "Nothing", &[]);
    assert!(t.document().tracks.is_empty());
    assert!(t.objects().is_empty());
}
