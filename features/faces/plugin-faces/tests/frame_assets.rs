//! A kit face saved as `.frame` keeps its material textures and fonts: they
//! ride in the `.frame.assets/` sidecar, come back byte-identical, and the
//! loaded face renders like the one built in memory.

use std::path::PathBuf;

use frame::document::proto::Scene;
use frame::surface::proto::Catalog;
use plugin_faces::Face;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("plugin-faces-{tag}-{}", std::process::id()));
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

fn textured() -> Vec<Face> {
    let faces: Vec<Face> = plugin_faces::suite_studio_blues::faces()
        .into_iter()
        .filter(|f| f.scene.assets.values().any(|a| a.mime == "image/png"))
        .collect();
    assert!(faces.len() >= 3, "the studio-blues suite is textured");
    faces
}

#[test]
fn textured_faces_round_trip_through_markup_on_disk() {
    let dir = TempDir::new("rt");
    for f in &textured() {
        let path = dir.0.join(format!("{}.frame", f.file));
        frame::document::io::save_surface(&path, &f.scene).unwrap();
        let back = frame::document::io::load_surface(&path).unwrap();
        assert_eq!(back.assets, f.scene.assets, "{}: assets", f.file);
        assert_eq!(back, f.scene, "{}", f.file);
        // The markup stays text: ids, not base64.
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, frame::document::text::print(&f.scene), "{}", f.file);
    }
}

/// The page's first top-level node's size.
fn size(scene: &Scene) -> (u32, u32) {
    let page = scene.pages()[0];
    let top = scene.node(page).unwrap().children[0];
    let s = scene.node(top).unwrap().size;
    (s.width.ceil() as u32, s.height.ceil() as u32)
}

/// `scene` played the way hosts play it (a runtime frame against its
/// catalog, as `render_surface` and the player draw) on a fresh GPU
/// context. Shaders are skipped (`Mode::Fallback`): once compiled (from
/// the second frame on) a material's shader can paint over its texture
/// tiles, and it's the textures this compares. `None` without a GPU.
fn render(scene: &Scene, catalog: &Catalog) -> Option<Vec<u8>> {
    let mut gpu = frame::render::offscreen::Offscreen::new().ok()?;
    let rt = frame::surface::runtime::Runtime::new(
        frame::surface::runtime::Surface::from_scene(scene.clone()),
        catalog.clone(),
    );
    let scene = rt.frame(0.0);
    let (w, h) = size(&scene);
    let page = scene.pages()[0];
    let values: std::collections::HashMap<_, f64> = rt
        .surface()
        .controls
        .iter()
        .map(|(id, c)| (*id, rt.position(*id, c)))
        .collect();
    let inputs = rt.shader_inputs();
    let frame = frame::render::fx::Frame {
        fonts: Some(frame::render::FontBook::bundled()),
        transform: kurbo::Affine::IDENTITY,
        values: Some(&values),
        inputs: Some(&inputs),
        mode: frame::render::fx::Mode::Fallback,
        ..frame::render::fx::Frame::new(&scene, page, w, h)
    };
    Some(gpu.render_with_effects(&frame).expect("render").rgba)
}

/// Pixels whose channels differ by more than `tol`.
fn differing(a: &[u8], b: &[u8], tol: u8) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > tol))
        .count()
}

#[test]
fn a_loaded_face_renders_like_the_one_in_memory() {
    let dir = TempDir::new("render");
    let faces = textured();
    let catalogs = plugin_faces::catalogs(&faces);
    let face = faces
        .iter()
        .find(|f| f.file.contains("cab-room"))
        .expect("the room cab");
    let catalog = &catalogs[face.plugin];
    let path = dir.0.join("face.frame");
    frame::document::io::save_surface(&path, &face.scene).unwrap();
    let loaded = frame::document::io::load_surface(&path).unwrap();

    // The renderer caches decoded images by content, so the loaded face
    // goes first, from a cold cache: had it lost its textures it would
    // draw flat here.
    let Some(disk) = render(&loaded, catalog) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let memory = render(&face.scene, catalog).expect("a GPU");
    let n = memory.len() / 4;
    let off = differing(&memory, &disk, 2);
    assert!(
        off * 1000 <= n,
        "{off} of {n} px differ between the in-memory and loaded face"
    );

    // …and the textures are really in the picture: pointed at assets that
    // aren't there (what loading `.frame` used to amount to), it draws flat.
    let mut text = frame::document::text::print(&face.scene);
    for id in face.scene.assets.keys() {
        text = text.replace(id.0.as_str(), &"0".repeat(64));
    }
    let flat = frame::document::text::parse(&text, std::collections::BTreeMap::new()).unwrap();
    let bare = render(&flat, catalog).expect("a GPU");
    let lost = differing(&memory, &bare, 2);
    assert!(
        lost * 50 > n,
        "dropping the textures changed only {lost} of {n} px"
    );
}
