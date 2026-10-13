//! What a face is, and the scaffolding every face uses.

use std::collections::{BTreeMap, HashMap};

use document_proto::{Node, NodeId, NodeKind, Scene};
use surface_proto::{Catalog, Control, ParamInfo};
use surface_widgets::{Builder, Theme};

/// One plugin face: a surface plus the params it binds.
pub struct Face {
    /// File stem, e.g. `"01-1176-black"`.
    pub file: &'static str,
    /// Which plugin's catalog it belongs to (`comp`, `eq`, `delay`, …).
    pub plugin: &'static str,
    pub scene: Scene,
    /// Params this face binds (merged into `<plugin>.catalog.json`).
    pub params: Vec<ParamInfo>,
    pub streams: Vec<surface_proto::StreamInfo>,
}

/// A fresh one-page scene with a builder over it; returns the root frame
/// (unsized, no layout — place panels/components with explicit positions,
/// or give it a layout).
pub fn canvas(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> Scene {
    let mut scene = Scene::new("Surface");
    let page = scene.pages()[0];
    let mut controls: BTreeMap<NodeId, Control> = BTreeMap::new();
    let theme = Theme::stage_dark();
    {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let root = b.add(page, Node::new(NodeKind::Frame, name));
        build(&mut b, root);
    }
    for (id, c) in controls {
        if let Some(n) = scene.node_mut(id) {
            n.control = Some(c);
        }
    }
    let root = scene.node(page).and_then(|p| p.children.first().copied()).expect("root frame");
    let hug = scene.hug_ops(root);
    scene.transact(hug).expect("hug");
    scene
}

/// Merge faces' params/streams into one catalog per plugin.
#[must_use]
pub fn catalogs(faces: &[Face]) -> BTreeMap<&'static str, Catalog> {
    let mut out: BTreeMap<&'static str, Catalog> = BTreeMap::new();
    for f in faces {
        let c = out.entry(f.plugin).or_insert_with(|| Catalog { id: format!("fts.{}", f.plugin), version: 1, ..Catalog::default() });
        for p in &f.params {
            if c.param(&p.addr).is_none() {
                c.params.push(p.clone());
            }
        }
        for s in &f.streams {
            if c.stream(&s.id).is_none() {
                c.streams.push(s.clone());
            }
        }
    }
    out
}

/// Namespace of the faces' stable node ids (uuid v5).
const ID_NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6a1f_3c2e_9d4b_4f7a_8e21_5b0c_d9e3_f471);

/// `scene` with every node id replaced by a stable one: a uuid v5 of
/// `key` and the node's path of names from the document root (a sibling
/// sharing its name with earlier ones gets `#n`). Regenerating a face then
/// rewrites nothing that didn't change, and a live reload keeps the state
/// of every node it can still match. Every reference to an id (children,
/// parents, control slots, links…) follows it.
///
/// # Panics
/// If the scene doesn't round-trip through JSON (it always does).
#[must_use]
pub fn stable_ids(mut scene: Scene, key: &str) -> Scene {
    let mut map: HashMap<NodeId, NodeId> = HashMap::new();
    let mut stack = vec![(scene.root, key.to_string())];
    while let Some((id, path)) = stack.pop() {
        map.insert(id, NodeId(uuid::Uuid::new_v5(&ID_NAMESPACE, path.as_bytes())));
        let Some(node) = scene.node(id) else { continue };
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for c in &node.children {
            let name = scene.node(*c).map_or("", |n| n.name.as_str());
            let n = seen.entry(name).or_insert(0);
            let seg = if *n == 0 { name.to_string() } else { format!("{name}#{n}") };
            *n += 1;
            stack.push((*c, format!("{path}/{seg}")));
        }
    }
    // Nodes off the tree (none today) take an id from their place in it.
    let off: Vec<NodeId> = scene.nodes.keys().filter(|id| !map.contains_key(id)).copied().collect();
    for (i, id) in off.into_iter().enumerate() {
        map.insert(id, NodeId(uuid::Uuid::new_v5(&ID_NAMESPACE, format!("{key}/~{i}").as_bytes())));
    }
    let by_text: HashMap<String, String> = map.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    // Rewrite each id wherever it appears, through the serialized scene
    // (its assets aside: they're content-addressed, and large).
    let assets = std::mem::take(&mut scene.assets);
    let json = serde_json::to_string(&scene).expect("scene to json");
    let mut out = String::with_capacity(json.len());
    let mut rest = json.as_str();
    while let Some(i) = rest.find('-') {
        // A uuid's first dash is its 9th character, of 36.
        if let Some(new) = i.checked_sub(8).and_then(|start| rest.get(start..i + 28)).and_then(|s| by_text.get(s)) {
            out.push_str(&rest[..i - 8]);
            out.push_str(new);
            rest = &rest[i + 28..];
            continue;
        }
        out.push_str(&rest[..=i]);
        rest = &rest[i + 1..];
    }
    out.push_str(rest);
    let mut scene: Scene = serde_json::from_str(&out).expect("scene from json");
    scene.assets = assets;
    scene
}

/// Place `face`'s design inside `parent` of the scene `b` builds, its top
/// frame fitted into `(x, y, w, h)` of `parent` (scaled uniformly, centred)
/// and made the child's own group transform. Every node, control and asset
/// comes along, so an embedded pedal still plays: a rig page is faces
/// arranged, not faces redrawn. Returns the embedded top frame.
///
/// # Panics
/// If `face` has no top frame (every face has one).
pub fn embed(b: &mut Builder<'_>, parent: NodeId, face: &Face, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let src = &face.scene;
    let page = src.pages()[0];
    let top = src.node(page).and_then(|p| p.children.first().copied()).expect("face top frame");
    let size = src.node(top).map_or(document_proto::Size { width: 1.0, height: 1.0 }, |n| n.size);
    let s = (w / size.width.max(1.0)).min(h / size.height.max(1.0));
    let (ox, oy) = (x + (w - size.width * s) / 2.0, y + (h - size.height * s) / 2.0);
    for id in src.walk(top) {
        let Some(mut n) = src.node(id).cloned() else { continue };
        if id == top {
            n.parent = Some(parent);
            n.transform = document_proto::Transform { a: s, b: 0.0, tx: ox, c: 0.0, d: s, ty: oy };
        }
        b.scene.nodes.insert(id, n);
    }
    if let Some(p) = b.scene.node_mut(parent) {
        p.children.push(top);
    }
    for (k, v) in &src.assets {
        b.scene.assets.entry(k.clone()).or_insert_with(|| v.clone());
    }
    top
}

/// The size a face draws at (its top frame).
#[must_use]
pub fn face_size(face: &Face) -> (f64, f64) {
    let page = face.scene.pages()[0];
    face.scene
        .node(page)
        .and_then(|p| p.children.first().copied())
        .and_then(|t| face.scene.node(t))
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}
