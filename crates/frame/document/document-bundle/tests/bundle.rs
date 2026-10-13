use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use document_bundle::{Error, FORMAT_VERSION, Manifest};
use document_loro::{AssetStorage, LoroScene};
use document_proto::*;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

fn sample() -> (Scene, NodeId) {
    let mut scene = Scene::new("Page 1");
    let page = scene.pages()[0];
    let png = scene.add_asset("image/png", vec![137, 80, 78, 71, 1, 2, 3]);
    let svg = scene.add_asset(
        "image/svg+xml",
        b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
    );
    let mut img = Node::new(NodeKind::Rectangle, "Image");
    img.size = Size {
        width: 100.0,
        height: 0.1 + 0.2,
    };
    img.fills = vec![Paint {
        kind: PaintKind::Image {
            asset: png,
            source_ref: None,
            scale_mode: ImageScaleMode::Fill,
            transform: None,
        },
        opacity: 1.0,
        blend: BlendMode::Normal,
        visible: true,
    }];
    img.svg_snapshot = Some(svg);
    let id = img.id;
    scene
        .apply(Op::Insert {
            node: img,
            parent: page,
            index: u32::MAX,
        })
        .unwrap();
    (scene, id)
}

fn kitchen_sink() -> Scene {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../render/tests/parity/scenarios/regression.kitchen-sink");
    let path = [
        dir.join("latest-bridge-export.json"),
        dir.join("export/latest-bridge-export.json"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .expect("kitchen-sink bridge export");
    figma_import::import_bytes(&std::fs::read(path).unwrap()).unwrap()
}

fn entries(bytes: &[u8]) -> Vec<String> {
    let zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    zip.file_names().map(str::to_owned).collect()
}

#[test]
fn manifest_round_trips_through_styx() {
    let mut m = Manifest::new("Mixer \"panel\"\nv2");
    m.catalogs = vec!["reaper".into(), "fts.eq".into()];
    let text = facet_styx::to_string(&m).unwrap();
    let back: Manifest = facet_styx::from_str(&text).unwrap();
    assert_eq!(back, m, "{text}");
}

#[test]
fn sample_round_trips_through_a_file() {
    let (scene, _) = sample();
    let manifest = Manifest::new("Sample");
    let dir = std::env::temp_dir().join(format!("frame-bundle-{}", manifest.doc_id));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.frame");

    document_bundle::save(&path, &scene, &manifest).unwrap();
    let (m, live) = document_bundle::load(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(m, manifest);
    assert_eq!(m.format, FORMAT_VERSION);
    assert_eq!(live.scene(), scene);
    assert_eq!(live.scene().assets.len(), 2);
}

#[test]
fn live_replica_keeps_history_and_keeps_bytes_out_of_the_doc() {
    let (scene, img) = sample();
    let live = LoroScene::from_scene(&scene);
    live.apply(Op::Set {
        id: img,
        prop: Prop::Opacity(0.5),
    })
    .unwrap();
    let bytes = document_bundle::to_bytes(&live, &Manifest::new("Live")).unwrap();

    let mut names = entries(&bytes);
    names.sort();
    let mut want: Vec<String> = scene
        .assets
        .keys()
        .map(|id| format!("assets/{id}"))
        .collect();
    want.extend(["doc.loro".to_owned(), "manifest.styx".to_owned()]);
    want.sort();
    assert_eq!(names, want);

    // doc.loro is exactly the replica's snapshot: no asset bytes in it.
    let mut zip = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
    let mut doc = Vec::new();
    zip.by_name("doc.loro")
        .unwrap()
        .read_to_end(&mut doc)
        .unwrap();
    let bare = LoroScene::from_snapshot(&doc, AssetStorage::External).unwrap();
    assert_eq!(bare.missing_assets().len(), 2);

    let (_, back) = document_bundle::from_bytes(&bytes).unwrap();
    assert_eq!(back.scene(), live.scene());
    assert_eq!(back.version(), live.version(), "history survives");

    // An inline-storage replica is re-based, so its bytes stay out too.
    let inline = LoroScene::from_scene_with(&live.scene(), AssetStorage::Inline);
    let bytes = document_bundle::to_bytes(&inline, &Manifest::new("Inline")).unwrap();
    let (_, back) = document_bundle::from_bytes(&bytes).unwrap();
    assert_eq!(back.scene(), live.scene());
}

#[test]
fn kitchen_sink_round_trips() {
    let scene = kitchen_sink();
    let bytes = document_bundle::to_bytes(&scene, &Manifest::new("Kitchen sink")).unwrap();
    let (m, live) = document_bundle::from_bytes(&bytes).unwrap();
    assert_eq!(m.title, "Kitchen sink");
    assert_eq!(live.scene(), scene);
}

/// Rewrite a bundle, passing each entry through `edit` (None drops it).
fn rewrite(bytes: &[u8], edit: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>) -> Vec<u8> {
    let mut src = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut entry = src.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if let Some(data) = edit(&name, data) {
            out.start_file(name, SimpleFileOptions::default()).unwrap();
            out.write_all(&data).unwrap();
        }
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn corrupt_asset_is_detected() {
    let (scene, _) = sample();
    let bytes = document_bundle::to_bytes(&scene, &Manifest::new("Sample")).unwrap();
    let png = AssetId::of(&[137, 80, 78, 71, 1, 2, 3]);
    let target = format!("assets/{png}");
    let corrupt = rewrite(&bytes, |name, mut data| {
        if name == target {
            data[0] ^= 1;
        }
        Some(data)
    });
    assert!(
        matches!(document_bundle::from_bytes(&corrupt), Err(Error::CorruptAsset(id)) if id == png)
    );

    let missing = rewrite(&bytes, |name, data| (name != target).then_some(data));
    assert!(
        matches!(document_bundle::from_bytes(&missing), Err(Error::MissingAsset(id)) if id == png)
    );
}

#[test]
fn newer_format_is_refused() {
    let (scene, _) = sample();
    let mut manifest = Manifest::new("Future");
    manifest.format = FORMAT_VERSION + 1;
    let bytes = document_bundle::to_bytes(&scene, &manifest).unwrap();
    assert!(
        matches!(document_bundle::from_bytes(&bytes), Err(Error::UnsupportedFormat(v)) if v == FORMAT_VERSION + 1)
    );
}
