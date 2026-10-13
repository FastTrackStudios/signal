use std::path::PathBuf;

use frame::document::loro::{AssetStorage, Error, LoroScene};
use frame::document::proto::*;

fn rect(name: &str) -> Node {
    let mut n = Node::new(NodeKind::Rectangle, name);
    n.size = Size {
        width: 10.0,
        height: 20.0,
    };
    n.fills = vec![Paint::solid(Color::rgba(1.0, 0.0, 0.0, 1.0))];
    n
}

/// The same sample as document-proto's model tests, plus a few awkward
/// floats. Returns (scene, page, frame, knob, label).
fn sample() -> (Scene, NodeId, NodeId, NodeId, NodeId) {
    let mut scene = Scene::new("Page 1");
    let page = scene.pages()[0];
    let mut frame = Node::new(NodeKind::Frame, "Panel");
    frame.layout = Some(AutoLayout {
        direction: LayoutDirection::Horizontal,
        gap: 8.0,
        padding_left: 4.0,
        padding_right: 4.0,
        padding_top: 4.0,
        padding_bottom: 4.0,
        wrap: false,
        main_align: Some(MainAxisAlign::Center),
        cross_align: None,
        hug_main: false,
        hug_cross: false,
    });
    frame.transform = Transform {
        a: 0.1 + 0.2,
        b: 1.0 / 3.0,
        tx: -0.0,
        c: 1e-300,
        d: 1.0,
        ty: 12_345.678_901_234_5,
    };
    let frame_id = frame.id;
    scene
        .apply(Op::Insert {
            node: frame,
            parent: page,
            index: u32::MAX,
        })
        .unwrap();
    let r = rect("Knob");
    let r_id = r.id;
    scene
        .apply(Op::Insert {
            node: r,
            parent: frame_id,
            index: u32::MAX,
        })
        .unwrap();
    let mut t = Node::new(
        NodeKind::Text(TextData {
            characters: "Gain".into(),
            ..TextData::default()
        }),
        "Label",
    );
    t.fills = vec![Paint::solid(Color::rgba(1.0, 1.0, 1.0, 1.0))];
    let t_id = t.id;
    scene
        .apply(Op::Insert {
            node: t,
            parent: frame_id,
            index: u32::MAX,
        })
        .unwrap();
    let png = scene.add_asset("image/png", vec![1, 2, 3]);
    let mut img = rect("Image");
    img.fills = vec![Paint {
        kind: PaintKind::Image {
            asset: png,
            source_ref: Some("abc".into()),
            scale_mode: ImageScaleMode::Fit,
            transform: None,
        },
        opacity: 0.5,
        blend: BlendMode::Multiply,
        visible: true,
    }];
    scene
        .apply(Op::Insert {
            node: img,
            parent: page,
            index: 0,
        })
        .unwrap();
    (scene, page, frame_id, r_id, t_id)
}

/// Two replicas of one doc (B starts from A's snapshot).
fn pair(scene: &Scene) -> (LoroScene, LoroScene) {
    let a = LoroScene::from_scene_with(scene, AssetStorage::Inline);
    let b = LoroScene::from_snapshot(&a.export_snapshot(), AssetStorage::Inline).unwrap();
    assert_eq!(b.scene(), *scene);
    (a, b)
}

fn sync(a: &LoroScene, b: &LoroScene) {
    let (va, vb) = (a.version(), b.version());
    b.import(&a.export_updates(&vb)).unwrap();
    a.import(&b.export_updates(&va)).unwrap();
}

/// Every node is listed exactly once by its parent and reachable from the root.
fn assert_well_formed(scene: &Scene) {
    let walked = scene.walk(scene.root);
    assert_eq!(
        walked.len(),
        scene.nodes.len(),
        "every node reachable exactly once"
    );
    for node in scene.nodes.values() {
        if let Some(p) = node.parent {
            let siblings = &scene.node(p).unwrap().children;
            assert_eq!(siblings.iter().filter(|c| **c == node.id).count(), 1);
        } else {
            assert_eq!(node.id, scene.root);
        }
    }
}

#[test]
fn from_scene_round_trips() {
    let (scene, ..) = sample();
    for storage in [AssetStorage::External, AssetStorage::Inline] {
        let live = LoroScene::from_scene_with(&scene, storage);
        assert_eq!(live.scene(), scene);
    }
}

/// Each edit is checked three ways: same inverse as `Scene::apply`, same
/// resulting scene (both the cache and a cold re-read of the doc), and the
/// inverse restores the prior state.
#[test]
fn apply_matches_the_model() {
    let (mut model, page, frame, knob, label) = sample();
    let live = LoroScene::from_scene(&model);
    let extra = rect("Extra");
    let extra_id = extra.id;
    let ops = vec![
        Op::Set {
            id: knob,
            prop: Prop::Opacity(0.25),
        },
        Op::Set {
            id: frame,
            prop: Prop::Name("Renamed".into()),
        },
        Op::Set {
            id: label,
            prop: Prop::Kind(NodeKind::Text(TextData {
                characters: "Vol".into(),
                ..TextData::default()
            })),
        },
        Op::Set {
            id: knob,
            prop: Prop::Stroke(Stroke {
                weight: 2.5,
                dash_pattern: vec![1.0, 2.0],
                ..Stroke::default()
            }),
        },
        Op::Set {
            id: knob,
            prop: Prop::Mask(Some(MaskKind::Alpha)),
        },
        Op::Insert {
            node: extra,
            parent: frame,
            index: 1,
        },
        Op::Move {
            id: knob,
            parent: frame,
            index: u32::MAX,
        },
        Op::Move {
            id: knob,
            parent: frame,
            index: 0,
        },
        Op::Move {
            id: label,
            parent: page,
            index: 0,
        },
        Op::Move {
            id: extra_id,
            parent: label,
            index: 0,
        },
        Op::PutAsset {
            asset: Asset::new("image/svg+xml", b"<svg/>".to_vec()),
        },
        Op::Set {
            id: extra_id,
            prop: Prop::SvgSnapshot(Some(AssetId::of(b"<svg/>"))),
        },
        Op::Remove { id: frame },
        Op::DropAsset {
            id: AssetId::of(&[1, 2, 3]),
        },
    ];
    for op in ops {
        let before = model.clone();
        let want = model.apply(op.clone()).unwrap();
        let got = live.apply(op.clone()).unwrap();
        assert_eq!(got, want, "inverse of {op:?}");
        assert_eq!(live.scene(), model, "after {op:?}");
        let cold = LoroScene::from_snapshot(&live.export_snapshot(), AssetStorage::Inline).unwrap();
        for (id, bytes) in model
            .assets
            .values()
            .map(|a| (a.id.clone(), a.bytes.clone()))
        {
            assert_eq!(cold.put_blob(bytes), id);
        }
        assert_eq!(cold.scene(), model, "cold read after {op:?}");
        // The inverse restores the prior state, then redo.
        live.transact(got).unwrap();
        assert_eq!(live.scene(), before, "undo of {op:?}");
        live.apply(op).unwrap();
        assert_eq!(live.scene(), model);
    }
}

#[test]
fn transact_matches_the_model_and_is_atomic() {
    let (mut model, page, frame, knob, _) = sample();
    let live = LoroScene::from_scene(&model);
    let tx = [
        Op::Move {
            id: knob,
            parent: page,
            index: 0,
        },
        Op::Set {
            id: knob,
            prop: Prop::Opacity(0.5),
        },
        Op::Set {
            id: frame,
            prop: Prop::Name("Renamed".into()),
        },
    ];
    let before = model.clone();
    let want = model.transact(tx.clone()).unwrap();
    let got = live.transact(tx).unwrap();
    assert_eq!(got, want);
    assert_eq!(live.scene(), model);
    live.transact(got).unwrap();
    assert_eq!(live.scene(), before);

    // A failing transaction writes nothing.
    let version = live.version();
    let err = live
        .transact([
            Op::Set {
                id: knob,
                prop: Prop::Visible(false),
            },
            Op::Move {
                id: frame,
                parent: knob,
                index: 0,
            },
        ])
        .unwrap_err();
    // (knob is back under frame after the undo above, so this is a cycle.)
    assert!(
        matches!(err, Error::Op(OpError::Cycle(id)) if id == frame),
        "{err:?}"
    );
    assert_eq!(live.version(), version);
    assert_eq!(live.scene(), before);
    assert!(matches!(
        live.apply(Op::Remove { id: before.root }),
        Err(Error::Op(OpError::Root))
    ));
}

#[test]
fn history_runs_on_the_replica() {
    let (scene, _, _, knob, _) = sample();
    let live = LoroScene::from_scene(&scene);
    let undo = live
        .transact([Op::Set {
            id: knob,
            prop: Prop::Size(Size {
                width: 64.0,
                height: 64.0,
            }),
        }])
        .unwrap();
    let edited = live.scene();
    let redo = live.transact(undo).unwrap();
    assert_eq!(live.scene(), scene);
    live.transact(redo).unwrap();
    assert_eq!(live.scene(), edited);
}

#[test]
fn concurrent_prop_edits_on_one_node_merge() {
    let (scene, _, _, knob, _) = sample();
    let (a, b) = pair(&scene);
    a.apply(Op::Set {
        id: knob,
        prop: Prop::Opacity(0.3),
    })
    .unwrap();
    b.apply(Op::Set {
        id: knob,
        prop: Prop::Name("Dial".into()),
    })
    .unwrap();
    b.apply(Op::Set {
        id: knob,
        prop: Prop::Size(Size {
            width: 1.5,
            height: 2.5,
        }),
    })
    .unwrap();
    sync(&a, &b);
    let (sa, sb) = (a.scene(), b.scene());
    assert_eq!(sa, sb);
    let n = sa.node(knob).unwrap();
    assert_eq!(n.opacity, 0.3);
    assert_eq!(n.name, "Dial");
    assert_eq!(
        n.size,
        Size {
            width: 1.5,
            height: 2.5
        }
    );
}

#[test]
fn concurrent_moves_converge_without_duplicates_or_cycles() {
    let (scene, page, frame, knob, label) = sample();
    let (a, b) = pair(&scene);
    // Same node to two different parents.
    a.apply(Op::Move {
        id: knob,
        parent: page,
        index: 0,
    })
    .unwrap();
    b.apply(Op::Move {
        id: knob,
        parent: label,
        index: 0,
    })
    .unwrap();
    // And a would-be cycle: two siblings, each moved under the other.
    let x = rect("X");
    let y = rect("Y");
    let (x_id, y_id) = (x.id, y.id);
    a.transact([
        Op::Insert {
            node: x,
            parent: page,
            index: u32::MAX,
        },
        Op::Insert {
            node: y,
            parent: page,
            index: u32::MAX,
        },
    ])
    .unwrap();
    sync(&a, &b);
    a.apply(Op::Move {
        id: x_id,
        parent: y_id,
        index: 0,
    })
    .unwrap();
    b.apply(Op::Move {
        id: y_id,
        parent: x_id,
        index: 0,
    })
    .unwrap();
    sync(&a, &b);

    let (sa, sb) = (a.scene(), b.scene());
    assert_eq!(sa, sb);
    assert_well_formed(&sa);
    let knob_parent = sa.node(knob).unwrap().parent.unwrap();
    assert!(knob_parent == page || knob_parent == label);
    // Exactly one of the two cyclic moves won.
    let (xp, yp) = (
        sa.node(x_id).unwrap().parent.unwrap(),
        sa.node(y_id).unwrap().parent.unwrap(),
    );
    assert!((xp == y_id) ^ (yp == x_id), "x under {xp}, y under {yp}");
    assert!(sa.node(frame).is_some());
}

#[test]
fn concurrent_insert_and_remove_converge() {
    let (scene, _, frame, knob, _) = sample();
    let (a, b) = pair(&scene);
    a.apply(Op::Remove { id: frame }).unwrap();
    b.apply(Op::Insert {
        node: rect("Late"),
        parent: knob,
        index: 0,
    })
    .unwrap();
    b.apply(Op::Set {
        id: knob,
        prop: Prop::Visible(false),
    })
    .unwrap();
    sync(&a, &b);
    let (sa, sb) = (a.scene(), b.scene());
    assert_eq!(sa, sb);
    assert_well_formed(&sa);
    assert!(sa.node(frame).is_none() && sa.node(knob).is_none());
}

#[test]
fn snapshot_round_trip() {
    let (scene, _, _, knob, _) = sample();
    let live = LoroScene::from_scene(&scene);
    live.apply(Op::Set {
        id: knob,
        prop: Prop::Locked(true),
    })
    .unwrap();
    let want = live.scene();

    // External storage: the snapshot has no bytes; the doc still lists the asset.
    let back = LoroScene::from_snapshot(&live.export_snapshot(), AssetStorage::External).unwrap();
    assert_eq!(back.missing_assets(), vec![AssetId::of(&[1, 2, 3])]);
    assert!(back.scene().assets.is_empty());
    back.put_blob(vec![1, 2, 3]);
    assert!(back.missing_assets().is_empty());
    assert_eq!(back.scene(), want);

    // Inline storage carries the bytes in the doc.
    let inline = LoroScene::from_scene_with(&want, AssetStorage::Inline);
    let back = LoroScene::from_snapshot(&inline.export_snapshot(), AssetStorage::External).unwrap();
    assert_eq!(back.scene(), want);
}

#[test]
fn subscribers_hear_local_and_remote_changes() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let (scene, _, _, knob, _) = sample();
    let (a, b) = pair(&scene);
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let _sub = b.subscribe(move || {
        h.fetch_add(1, Ordering::SeqCst);
    });
    b.apply(Op::Set {
        id: knob,
        prop: Prop::Opacity(0.1),
    })
    .unwrap();
    let local = hits.load(Ordering::SeqCst);
    assert!(local >= 1);
    a.apply(Op::Set {
        id: knob,
        prop: Prop::Name("R".into()),
    })
    .unwrap();
    sync(&a, &b);
    assert!(hits.load(Ordering::SeqCst) > local);
}

fn kitchen_sink() -> Scene {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("render/tests/parity/scenarios/regression.kitchen-sink");
    let path = [
        dir.join("latest-bridge-export.json"),
        dir.join("export/latest-bridge-export.json"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .expect("kitchen-sink bridge export");
    frame::figma::import::import_bytes(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn figma_kitchen_sink_survives_the_replica() {
    let scene = kitchen_sink();
    assert!(scene.nodes.len() > 10);
    let live = LoroScene::from_scene(&scene);
    assert_eq!(live.scene(), scene);
    // Cold, from bytes, with the blobs supplied separately.
    let back = LoroScene::from_snapshot(&live.export_snapshot(), AssetStorage::External).unwrap();
    for asset in scene.assets.values() {
        back.put_blob(asset.bytes.clone());
    }
    assert_eq!(back.scene(), scene);
}
