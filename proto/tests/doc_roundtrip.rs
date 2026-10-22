//! The codec's round trip, as properties: whatever a Rig, Patch, Song or
//! Setlist holds, `get(put(x)) == x` — through a fresh doc, a snapshot, and
//! a doc that held something else before. Writing the same thing twice
//! records nothing, and two replicas that write different entities at once
//! converge.
//!
//! Run with: `cargo test -p signal-proto --features doc --test doc_roundtrip`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use serde_json::{Map, Number, Value};
use signal_proto::default_profile::LibrarySettings;
use signal_proto::doc::library::{PatchRef, ProfileEntry};
use signal_proto::doc::{DocEntity, SignalDoc, codec};
use signal_proto::engine::EngineSceneId;
use signal_proto::metadata::{Metadata, Tags};
use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};
use signal_proto::profile::{Patch, PatchTarget, Profile, ProfileId};
use signal_proto::rig::{EngineSelection, Rig, RigScene, RigType};
use signal_proto::setlist::{Setlist, SetlistEntry};
use signal_proto::song::{Scene, SceneSource, Song};
use signal_proto::{ParameterValue, PresetId, SnapshotId};
use uuid::Uuid;

// ── strategies ──────────────────────────────────────────────────────────

fn uuid() -> impl Strategy<Value = Uuid> {
    any::<u128>().prop_map(Uuid::from_u128)
}

fn text() -> impl Strategy<Value = String> {
    "\\PC{0,12}"
}

fn word() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}"
}

fn metadata() -> impl Strategy<Value = Metadata> {
    (
        prop::collection::vec(word(), 0..4),
        prop::option::of(text()),
        prop::option::of(text()),
        prop::option::of(text()),
        prop::option::of(word()),
    )
        .prop_map(|(tags, description, notes, folder, base)| Metadata {
            tags: Tags::from_vec(tags),
            description,
            notes,
            folder,
            base_profile_id: base,
        })
}

fn path() -> impl Strategy<Value = NodePath> {
    prop::collection::vec(
        prop_oneof![
            word().prop_map(|id| NodePathSegment::Engine { id }),
            word().prop_map(|id| NodePathSegment::Layer { id }),
            word().prop_map(|id| NodePathSegment::Module { id }),
            word().prop_map(|id| NodePathSegment::Block { id }),
            word().prop_map(|id| NodePathSegment::Parameter { id }),
            text().prop_map(|text| NodePathSegment::Raw { text }),
        ],
        1..5,
    )
    .prop_map(NodePath::new)
}

fn override_() -> impl Strategy<Value = Override> {
    let op = prop_oneof![
        (0.0f32..=1.0).prop_map(|v| NodeOverrideOp::Set {
            value: ParameterValue::new(v)
        }),
        any::<bool>().prop_map(|bypassed| NodeOverrideOp::Bypass { bypassed }),
        word().prop_map(|id| NodeOverrideOp::ReplaceRef { id }),
        Just(NodeOverrideOp::Remove),
        any::<bool>().prop_map(|enabled| NodeOverrideOp::Enable { enabled }),
    ];
    (path(), op).prop_map(|(path, op)| Override { path, op })
}

fn overrides() -> impl Strategy<Value = Vec<Override>> {
    prop::collection::vec(override_(), 0..5)
}

fn patch_target() -> impl Strategy<Value = PatchTarget> {
    prop_oneof![
        (uuid(), uuid()).prop_map(|(r, s)| PatchTarget::RigScene {
            rig_id: r.into(),
            scene_id: s.into(),
        }),
        (uuid(), uuid()).prop_map(|(p, s)| PatchTarget::BlockSnapshot {
            preset_id: PresetId::from(p),
            snapshot_id: SnapshotId::from(s),
        }),
        (uuid(), uuid()).prop_map(|(l, s)| PatchTarget::LayerSnapshot {
            layer_id: l.into(),
            snapshot_id: s.into(),
        }),
        uuid().prop_map(|p| PatchTarget::Patch { patch_id: p.into() }),
    ]
}

prop_compose! {
    fn patch()(
        id in uuid(),
        name in text(),
        slug in prop_oneof![word(), Just(String::new())],
        target in patch_target(),
        overrides in overrides(),
        metadata in metadata(),
    ) -> Patch {
        Patch { id: id.into(), name, slug, target, overrides, metadata }
    }
}

prop_compose! {
    fn profile_entry()(
        id in uuid(),
        name in text(),
        refs in prop::collection::btree_set(word(), 0..8),
        default in word(),
        metadata in metadata(),
    ) -> ProfileEntry {
        ProfileEntry {
            id: id.into(),
            name,
            default_patch: format!("patch:{default}"),
            patches: refs.into_iter().map(|r| PatchRef { id: format!("patch:{r}") }).collect(),
            metadata,
        }
    }
}

prop_compose! {
    fn scene()(
        id in uuid(),
        name in text(),
        source in prop_oneof![
            uuid().prop_map(|p| SceneSource::Patch { patch_id: p.into() }),
            (uuid(), uuid()).prop_map(|(r, s)| SceneSource::RigScene {
                rig_id: r.into(),
                scene_id: s.into(),
            }),
            Just(SceneSource::DefaultProfile),
        ],
        overrides in overrides(),
        metadata in metadata(),
    ) -> Scene {
        Scene { id: id.into(), name, source, overrides, metadata }
    }
}

prop_compose! {
    fn song()(
        id in uuid(),
        name in text(),
        artist in prop::option::of(text()),
        scenes in prop::collection::vec(scene(), 1..8),
        pick in any::<prop::sample::Index>(),
        metadata in metadata(),
    ) -> Song {
        let default_scene_id = scenes[pick.index(scenes.len())].id.clone();
        Song { id: id.into(), name, artist, default_scene_id, scenes, metadata }
    }
}

prop_compose! {
    fn setlist()(
        id in uuid(),
        name in text(),
        entries in prop::collection::vec(
            (uuid(), text(), uuid(), metadata()).prop_map(|(id, name, song, metadata)| {
                SetlistEntry::new(id, name, song).with_metadata(metadata)
            }),
            1..12,
        ),
        pick in any::<prop::sample::Index>(),
        metadata in metadata(),
    ) -> Setlist {
        let default_entry_id = entries[pick.index(entries.len())].id.clone();
        Setlist { id: id.into(), name, default_entry_id, entries, metadata }
    }
}

prop_compose! {
    fn rig_scene()(
        id in uuid(),
        name in text(),
        engines in prop::collection::vec((uuid(), uuid()), 0..3),
        overrides in overrides(),
        metadata in metadata(),
    ) -> RigScene {
        let mut scene = RigScene::new(id, name).with_metadata(metadata);
        for (engine, variant) in engines {
            scene = scene.with_engine(EngineSelection::new(engine, EngineSceneId::from(variant)));
        }
        scene.overrides = overrides;
        scene
    }
}

prop_compose! {
    fn rig()(
        id in uuid(),
        name in text(),
        rig_type in prop::option::of(prop_oneof![
            Just(RigType::Guitar), Just(RigType::Bass), Just(RigType::Keys),
            Just(RigType::Drums), Just(RigType::Vocals),
        ]),
        engines in prop::collection::vec(uuid(), 0..3),
        scenes in prop::collection::vec(rig_scene(), 1..5),
        pick in any::<prop::sample::Index>(),
        input in prop::option::of(text()),
        metadata in metadata(),
    ) -> Rig {
        let mut scenes = scenes.into_iter();
        let first = scenes.next().unwrap();
        let mut rig = Rig::new(id, name, engines.into_iter().map(Into::into).collect(), first);
        for scene in scenes {
            rig.add_variant(scene);
        }
        rig.default_variant_id = rig.variants[pick.index(rig.variants.len())].id.clone();
        rig.rig_type = rig_type;
        rig.input_track_ref = input;
        rig.metadata = metadata;
        rig
    }
}

/// Any JSON, with objects whose keys never start with `$` (the codec's own
/// marks) and arrays that are sometimes keyed lists.
fn json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|i| Value::Number(i.into())),
        any::<f64>()
            .prop_filter_map("finite", Number::from_f64)
            .prop_map(Value::Number),
        text().prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::btree_map("[a-z_]{1,6}", inner.clone(), 0..6)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
            // A keyed list: objects with unique string ids.
            prop::collection::btree_map(
                word(),
                prop::collection::btree_map("[a-z_]{1,6}", inner, 0..4),
                1..6
            )
            .prop_map(|items| {
                Value::Array(
                    items
                        .into_iter()
                        .map(|(id, fields)| {
                            let mut object: Map<String, Value> = fields.into_iter().collect();
                            object.insert("id".to_owned(), Value::String(id));
                            Value::Object(object)
                        })
                        .collect(),
                )
            }),
        ]
    })
}

// ── the properties ──────────────────────────────────────────────────────

/// `entity` survives a fresh doc, a snapshot of it, and a doc that held
/// `before` under the same key.
fn round_trips<E: DocEntity + PartialEq + std::fmt::Debug>(before: &E, entity: &E) {
    let doc = SignalDoc::new();
    doc.put(entity).unwrap();
    doc.commit("test");
    let key = entity.doc_key();
    assert_eq!(
        doc.get::<E>(&key).unwrap().as_ref(),
        Some(entity),
        "fresh doc"
    );

    let restored = SignalDoc::from_bytes(&doc.snapshot().unwrap()).unwrap();
    assert_eq!(
        restored.get::<E>(&key).unwrap().as_ref(),
        Some(entity),
        "snapshot"
    );

    // Over an earlier version: write `before` under the entity's key, then
    // reconcile to `entity`.
    let reused = SignalDoc::new();
    let earlier = serde_json::to_value(before).unwrap();
    codec::put(&reused.loro().get_map(E::KIND.as_str()), &key, &earlier).unwrap();
    reused.commit("test");
    reused.put(entity).unwrap();
    reused.commit("test");
    assert_eq!(
        reused.get::<E>(&key).unwrap().as_ref(),
        Some(entity),
        "over an earlier version"
    );

    // Writing it again records nothing.
    let version = reused.version();
    reused.put(entity).unwrap();
    reused.commit("test");
    assert_eq!(
        reused.version(),
        version,
        "a second write of the same thing is a no-op"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_patch_round_trips(before in patch(), after in patch()) {
        round_trips(&before, &after);
    }

    #[test]
    fn a_song_round_trips(before in song(), after in song()) {
        round_trips(&before, &after);
    }

    #[test]
    fn a_setlist_round_trips(before in setlist(), after in setlist()) {
        round_trips(&before, &after);
    }

    #[test]
    fn a_rig_round_trips(before in rig(), after in rig()) {
        round_trips(&before, &after);
    }

    #[test]
    fn a_profile_round_trips(before in profile_entry(), after in profile_entry()) {
        round_trips(&before, &after);
    }

    #[test]
    fn the_default_profile_pointer_round_trips(
        before in prop::option::of(uuid()),
        after in prop::option::of(uuid()),
    ) {
        let settings = |id: Option<Uuid>| LibrarySettings {
            default_profile_id: id.map(ProfileId::from),
        };
        round_trips(&settings(before), &settings(after));
    }

    /// A profile split into its entry and its patches, written to a doc and
    /// read back, assembles to the same profile.
    #[test]
    fn a_profile_and_its_patches_reassemble(
        patches in prop::collection::btree_map(word(), patch(), 1..6),
        id in uuid(),
        name in text(),
        pick in any::<prop::sample::Index>(),
    ) {
        let patches: Vec<Patch> = patches
            .into_iter()
            .map(|(slug, p)| p.with_slug(slug))
            .collect();
        let default = patches[pick.index(patches.len())].clone();
        let mut profile = Profile::new(id, name, default.clone());
        for p in patches.iter().filter(|p| p.id != default.id) {
            profile.add_patch(p.clone());
        }
        let doc = SignalDoc::new();
        doc.put(&ProfileEntry::of(&profile)).unwrap();
        for p in &profile.patches {
            doc.put(p).unwrap();
        }
        doc.commit("test");
        let read = doc.read_library().unwrap();
        prop_assert_eq!(read.assembled_profiles(), vec![profile]);
    }

    /// The codec under the entities: any JSON, over any JSON.
    #[test]
    fn any_json_round_trips(before in json(), value in json()) {
        let doc = loro::LoroDoc::new();
        let map = doc.get_map("root");
        codec::put(&map, "x", &before).unwrap();
        doc.commit();
        codec::put(&map, "x", &value).unwrap();
        doc.commit();
        prop_assert_eq!(codec::get(&map, "x"), Some(value.clone()));

        let version = doc.oplog_vv();
        codec::put(&map, "x", &value).unwrap();
        doc.commit();
        prop_assert_eq!(doc.oplog_vv(), version);
    }

    /// A setlist edited in steps reads as the last step, and a replica that
    /// only ever saw the updates reads the same.
    #[test]
    fn a_setlist_edited_in_steps_reads_as_the_last(steps in prop::collection::vec(setlist(), 1..6)) {
        let doc = SignalDoc::new();
        let replica = SignalDoc::new();
        let mut last = None;
        for mut step in steps {
            // One setlist, edited: keep the id the first step had.
            if let Some(id) = last.as_ref().map(|s: &Setlist| s.id.clone()) {
                step.id = id;
            }
            let since = replica.version();
            doc.put(&step).unwrap();
            doc.commit("test");
            replica.import(&doc.updates_since(&since).unwrap()).unwrap();
            prop_assert_eq!(doc.get::<Setlist>(&step.doc_key()).unwrap(), Some(step.clone()));
            prop_assert_eq!(replica.get::<Setlist>(&step.doc_key()).unwrap(), Some(step.clone()));
            last = Some(step);
        }
    }

    /// Two replicas writing different patches at once converge, both kept.
    #[test]
    fn concurrent_writes_converge(a in patch(), b in patch()) {
        prop_assume!(a.doc_key() != b.doc_key());
        let left = SignalDoc::new();
        let right = left.fork();
        left.put(&a).unwrap();
        left.commit("left");
        right.put(&b).unwrap();
        right.commit("right");
        let (lv, rv) = (left.version(), right.version());
        left.import(&right.updates_since(&lv).unwrap()).unwrap();
        right.import(&left.updates_since(&rv).unwrap()).unwrap();
        prop_assert_eq!(left.dump(), right.dump());
        prop_assert_eq!(left.get::<Patch>(&a.doc_key()).unwrap(), Some(a.clone()));
        prop_assert_eq!(left.get::<Patch>(&b.doc_key()).unwrap(), Some(b.clone()));
    }
}

/// Two people edit different fields and different entries of one setlist at
/// once — one reorders and renames an entry, the other adds a song and
/// renames the set — and every edit lands.
#[test]
fn concurrent_edits_to_one_setlist_all_land() {
    let mut set = Setlist::new(
        Uuid::from_u128(1),
        "Sunday",
        SetlistEntry::new(Uuid::from_u128(10), "One", Uuid::from_u128(100)),
    );
    set.add_entry(SetlistEntry::new(
        Uuid::from_u128(11),
        "Two",
        Uuid::from_u128(101),
    ));
    set.add_entry(SetlistEntry::new(
        Uuid::from_u128(12),
        "Three",
        Uuid::from_u128(102),
    ));

    let agent = SignalDoc::new();
    agent.put(&set).unwrap();
    agent.commit("seed");
    let captain = agent.fork();

    let mut by_agent = set.clone();
    let three = by_agent.entries.remove(2);
    by_agent.entries.insert(0, three);
    by_agent.entries[1].name = "One (acoustic)".to_owned();
    agent.put(&by_agent).unwrap();
    agent.commit("agent");

    let mut by_captain = set.clone();
    by_captain.name = "Sunday AM".to_owned();
    by_captain.add_entry(SetlistEntry::new(
        Uuid::from_u128(13),
        "Four",
        Uuid::from_u128(103),
    ));
    captain.put(&by_captain).unwrap();
    captain.commit("captain");

    let (av, cv) = (agent.version(), captain.version());
    agent.import(&captain.updates_since(&av).unwrap()).unwrap();
    captain.import(&agent.updates_since(&cv).unwrap()).unwrap();

    let merged: Setlist = agent.get(&set.doc_key()).unwrap().unwrap();
    assert_eq!(
        merged,
        captain.get::<Setlist>(&set.doc_key()).unwrap().unwrap()
    );
    assert_eq!(merged.name, "Sunday AM");
    let names: Vec<&str> = merged.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Three", "One (acoustic)", "Two", "Four"]);
}

/// A patch saved before slugs existed has no slug; the doc keys it by id
/// and reads it back unchanged.
#[test]
fn a_patch_without_a_slug_is_keyed_by_id() {
    let mut patch = Patch::from_rig_scene(
        Uuid::from_u128(5),
        "Clean",
        Uuid::from_u128(6),
        Uuid::from_u128(7),
    );
    patch.slug.clear();
    let doc = SignalDoc::new();
    doc.put(&patch).unwrap();
    doc.commit("test");
    let key = format!("patch:{}", patch.id);
    assert_eq!(
        doc.keys(signal_proto::doc::EntityKind::Patch),
        [key.clone()]
    );
    assert_eq!(doc.get::<Patch>(&key).unwrap(), Some(patch));
}
