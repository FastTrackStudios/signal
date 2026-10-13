use super::*;
use crate::document::proto::{Node, NodeKind, Op, Paint};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("document-io-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scene with an image-filled frame and a document font.
fn scene_with(png: &[u8]) -> Scene {
    let mut scene = Scene::new("Surface");
    let page = scene.pages()[0];
    let image = scene.add_asset("image/png", png.to_vec());
    scene.add_asset("font/ttf", b"not really a font".to_vec());
    let mut n = Node::new(NodeKind::Frame, "Panel");
    n.fills = vec![Paint {
        kind: crate::document::proto::PaintKind::Image {
            asset: image,
            source_ref: None,
            scale_mode: crate::document::proto::ImageScaleMode::Stretch,
            transform: None,
        },
        opacity: 1.0,
        blend: crate::document::proto::BlendMode::Normal,
        visible: true,
    }];
    scene
        .apply(Op::Insert {
            node: n,
            parent: page,
            index: u32::MAX,
        })
        .unwrap();
    scene
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn markup_round_trips_with_its_assets() {
    let dir = TempDir::new("rt");
    let path = dir.0.join("s.frame");
    let scene = scene_with(b"png one");
    save_surface(&path, &scene).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        crate::document::text::print(&scene),
        "the markup itself is unchanged: ids only, no bytes"
    );
    let side = assets_dir(&path);
    assert_eq!(side, dir.0.join("s.frame.assets"));
    let ids: Vec<String> = scene
        .assets
        .values()
        .map(|a| format!("{}.{}", a.id, extension_for(&a.mime)))
        .collect();
    let mut want: Vec<String> = ids.into_iter().chain([MANIFEST.to_owned()]).collect();
    want.sort();
    assert_eq!(files(&side), want);

    assert_eq!(load_surface(&path).unwrap(), scene);
}

#[test]
fn stale_assets_are_pruned_and_an_assetless_scene_drops_the_sidecar() {
    let dir = TempDir::new("prune");
    let path = dir.0.join("s.frame");
    save_surface(&path, &scene_with(b"png one")).unwrap();
    let second = scene_with(b"png two");
    save_surface(&path, &second).unwrap();
    assert_eq!(
        files(&assets_dir(&path)).len(),
        3,
        "manifest, font, the new png only"
    );
    assert_eq!(load_surface(&path).unwrap(), second);

    // A stranger's file survives pruning (and so does the folder).
    std::fs::write(assets_dir(&path).join("notes.txt"), "mine").unwrap();
    let mut bare = second.clone();
    bare.assets.clear();
    save_surface(&path, &bare).unwrap();
    assert_eq!(files(&assets_dir(&path)), ["notes.txt"]);
    std::fs::remove_file(assets_dir(&path).join("notes.txt")).unwrap();
    save_surface(&path, &bare).unwrap();
    assert!(!assets_dir(&path).exists());
    assert_eq!(load_surface(&path).unwrap(), bare);
}

#[test]
fn a_missing_or_tampered_asset_is_an_error() {
    let dir = TempDir::new("bad");
    let path = dir.0.join("s.frame");
    let scene = scene_with(b"png one");
    save_surface(&path, &scene).unwrap();
    let png = scene
        .assets
        .values()
        .find(|a| a.mime == "image/png")
        .unwrap();
    let file = assets_dir(&path).join(format!("{}.png", png.id));
    std::fs::write(&file, b"flipped").unwrap();
    assert!(matches!(
        load_surface(&path),
        Err(Error::CorruptAsset { .. })
    ));
    std::fs::remove_file(&file).unwrap();
    let err = load_surface(&path).unwrap_err();
    assert!(matches!(err, Error::MissingAsset { .. }), "{err}");
    assert!(err.to_string().contains(&png.id.0), "{err}");
}

#[test]
fn manifest_files_must_stay_in_the_folder() {
    let dir = TempDir::new("escape");
    let path = dir.0.join("s.frame");
    std::fs::write(&path, "<Document><Page /></Document>").unwrap();
    std::fs::create_dir_all(assets_dir(&path)).unwrap();
    std::fs::write(
        assets_dir(&path).join(MANIFEST),
        r#"{"format":1,"assets":[{"id":"00","mime":"image/png","file":"../s.frame"}]}"#,
    )
    .unwrap();
    assert!(matches!(read_assets(&path), Err(Error::Manifest { .. })));
}

#[test]
fn parse_errors_carry_path_line_and_col() {
    let dir = TempDir::new("parse");
    let path = dir.0.join("bad.frame");
    std::fs::write(&path, "<Document>\n  <Page>\n</Document>").unwrap();
    let err = load_surface(&path).unwrap_err().to_string();
    let rest = err.strip_prefix(&format!("{}:", path.display())).unwrap();
    assert!(
        rest.split(':').next().unwrap().parse::<usize>().is_ok(),
        "{err}"
    );
}

#[test]
fn fingerprint_follows_the_markup_and_its_assets() {
    let dir = TempDir::new("fp");
    let path = dir.0.join("s.frame");
    assert_eq!(fingerprint(&path), None);
    let scene = scene_with(b"png one");
    save_surface(&path, &scene).unwrap();
    let before = fingerprint(&path).unwrap();
    assert_eq!(
        fingerprint(&path),
        Some(before),
        "stable while nothing changes"
    );
    let png = scene
        .assets
        .values()
        .find(|x| x.mime == "image/png")
        .unwrap();
    std::fs::write(
        assets_dir(&path).join(format!("{}.png", png.id)),
        b"png one, edited",
    )
    .unwrap();
    assert_ne!(
        fingerprint(&path),
        Some(before),
        "an asset edit changes the fingerprint"
    );
}

#[test]
fn bundles_load_and_save_through_the_same_api() {
    let dir = TempDir::new("bundle");
    let path = dir.0.join("s.frame");
    let scene = scene_with(b"png one");
    assert_eq!(
        Format::of(&path),
        Some(Format::Markup),
        "a new .frame defaults to markup"
    );
    save_surface_as(&path, &scene, Format::Bundle).unwrap();
    assert_eq!(
        Format::of(&path),
        Some(Format::Bundle),
        "detected by content"
    );
    assert!(
        !assets_dir(&path).exists(),
        "a bundle carries its own assets"
    );
    assert_eq!(load_surface(&path).unwrap(), scene);
    let (first, _) = crate::document::bundle::load(&path).unwrap();
    save_surface(&path, &scene).unwrap();
    let (again, _) = crate::document::bundle::load(&path).unwrap();
    assert_eq!(
        first.doc_id, again.doc_id,
        "a re-save keeps the document's identity"
    );
    assert_eq!(
        Format::of(&path),
        Some(Format::Bundle),
        "a plain re-save stays a bundle"
    );
    assert!(matches!(
        save_surface(dir.0.join("s.txt"), &scene),
        Err(Error::UnknownFormat { .. })
    ));
}

#[test]
fn remove_takes_the_sidecar_too() {
    let dir = TempDir::new("rm");
    let path = dir.0.join("s.frame");
    save_surface(&path, &scene_with(b"png one")).unwrap();
    remove_surface(&path).unwrap();
    assert!(!path.exists());
    assert!(!assets_dir(&path).exists());
    remove_surface(&path).unwrap();
}

#[test]
fn assets_dirs_are_recognised() {
    assert!(is_assets_dir(Path::new("a/b.frame.assets")));
    assert!(!is_assets_dir(Path::new("a/assets")));
    assert!(
        is_assets_dir(Path::new("a/b.fm.assets")),
        "the legacy name keeps working"
    );
}

#[test]
fn legacy_fm_markup_still_loads_and_saves() {
    let dir = TempDir::new("legacy-fm");
    let path = dir.0.join("old.fm");
    let scene = scene_with(b"png one");
    save_surface(&path, &scene).unwrap();
    assert!(assets_dir(&path).ends_with("old.fm.assets"));
    assert_eq!(load_surface(&path).unwrap(), scene);
    assert_eq!(Format::of(&path), Some(Format::Markup));
}

#[test]
fn a_frame_is_markup_or_a_bundle_by_content() {
    let dir = TempDir::new("sniff");
    let scene = scene_with(b"png one");
    let text = dir.0.join("text.frame");
    save_surface(&text, &scene).unwrap();
    assert_eq!(Format::of(&text), Some(Format::Markup));
    let zip = dir.0.join("zip.frame");
    save_surface_as(&zip, &scene, Format::Bundle).unwrap();
    assert!(std::fs::read(&zip).unwrap().starts_with(b"PK\x03\x04"));
    assert_eq!(Format::of(&zip), Some(Format::Bundle));
    assert_eq!(load_surface(&zip).unwrap(), scene);
    assert_eq!(
        Format::of(&dir.0.join("new.frame")),
        Some(Format::Markup),
        "a new .frame is markup"
    );
    assert_eq!(Format::of(&dir.0.join("new.fm")), Some(Format::Markup));
    assert_eq!(Format::of(&dir.0.join("new.txt")), None);
}

#[test]
fn saving_in_the_other_format_rewrites_the_same_path() {
    let dir = TempDir::new("reformat");
    let path = dir.0.join("s.frame");
    let scene = scene_with(b"png one");
    save_surface(&path, &scene).unwrap();
    assert!(assets_dir(&path).exists());
    save_surface_as(&path, &scene, Format::Bundle).unwrap();
    assert_eq!(Format::of(&path), Some(Format::Bundle));
    assert!(!assets_dir(&path).exists(), "the markup's sidecar is gone");
    save_surface_as(&path, &scene, Format::Markup).unwrap();
    assert_eq!(Format::of(&path), Some(Format::Markup));
    assert_eq!(load_surface(&path).unwrap(), scene);
}

#[test]
fn frame_documents_are_recognised_by_extension() {
    for name in ["a.frame", "a.fm"] {
        assert!(is_frame_document(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "a", "a.frame.assets"] {
        assert!(!is_frame_document(Path::new(name)), "{name}");
    }
}
