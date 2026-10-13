//! The live, collaborative scene store: a [`Scene`] held as a Loro CRDT.
//!
//! A [`LoroScene`] is one replica of a document. Local edits are the same
//! typed [`Op`]s the in-memory model takes ([`LoroScene::apply`],
//! [`LoroScene::transact`]) and return the same inverse ops; remote edits
//! arrive as Loro update bytes ([`LoroScene::import`]) and merge. Reads
//! materialize back into a plain [`Scene`] ([`LoroScene::scene`]), so
//! everything downstream (render, tools, the `.frame` bundle) keeps reading
//! only the model.
//!
//! # Schema (inside the `LoroDoc`)
//!
//! ```text
//! "nodes"  LoroTree   one tree node per scene node, fractional-index ordered
//!   └─ meta LoroMap   "id" (NodeId uuid string), "name", "kind", and one key
//!                     per Prop field: visible, locked, transform, size,
//!                     opacity, blend, corner_radii, clips_content, mask,
//!                     fills, stroke, effects, fill_geometry, stroke_geometry,
//!                     svg_snapshot, layout, layout_child, control, source
//! "scene"  LoroMap    "root" → the root node's id
//! "assets" LoroMap    asset id → { mime, size, bytes? (Binary) }
//! ```
//!
//! Each meta value is the [`LoroValue`](crdt::loro::LoroValue) image of the
//! field's serde JSON form (see [`value`]), written whole: a prop is the
//! unit of last-writer-wins, so concurrent edits to *different* props of
//! one node merge, and edits to the *same* prop resolve to one of them.
//! Parent/child structure and sibling order are the tree's own — Loro's
//! movable tree guarantees concurrent moves converge without cycles or
//! duplicated nodes.
//!
//! # Assets
//!
//! Asset *metadata* always lives in the doc. Where the *bytes* live is the
//! replica's [`AssetStorage`]:
//!
//! - [`AssetStorage::External`] (the default) keeps bytes in a local,
//!   content-addressed blob store next to the doc. Assets are immutable
//!   and named by their blake3, so they need no merging — keeping them out
//!   of the CRDT keeps its history (and every snapshot) small. The `.frame`
//!   bundle stores them as separate files. A replica that learns of an
//!   asset whose bytes it doesn't hold lists it in
//!   [`LoroScene::missing_assets`] and leaves it out of [`LoroScene::scene`]
//!   until [`LoroScene::put_blob`] supplies them.
//! - [`AssetStorage::Inline`] also writes the bytes into the doc (a
//!   `Binary` value), so plain Loro sync carries them — convenient for small
//!   documents without a blob channel, at the cost of snapshot size.
//!
//! Reads accept both shapes regardless of the replica's own mode.

pub mod value;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crdt::CrdtDoc;
use crdt::loro::{
    ExportMode, Frontiers, LoroDoc, LoroError, LoroMap, LoroTree, LoroValue, Subscription, TreeID,
    ValueOrContainer, VersionVector,
};
use document_proto::{Asset, AssetId, Node, NodeId, Op, OpError, Prop, Scene};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// The tree of scene nodes.
pub const NODES: &str = "nodes";
/// Scene-level fields (`"root"`).
pub const SCENE: &str = "scene";
/// Asset metadata (and inline bytes).
pub const ASSETS: &str = "assets";

/// Where a replica keeps asset bytes. See the crate docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetStorage {
    /// Bytes in a local blob store beside the doc; only metadata in the doc.
    #[default]
    External,
    /// Bytes inside the doc as well, synced like any other value.
    Inline,
}

/// Why a [`LoroScene`] edit failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The ops don't fit the scene; nothing was written.
    #[error(transparent)]
    Op(#[from] OpError),
    /// Loro rejected a write the model accepted (the doc was changed
    /// underneath this replica mid-edit). The replica re-reads the doc on
    /// its next access.
    #[error("loro: {0}")]
    Loro(#[from] LoroError),
}

/// One replica of a scene. Cheap reads (a cached materialization that is
/// refreshed whenever the doc's version moves), `&self` edits.
pub struct LoroScene {
    doc: CrdtDoc,
    storage: AssetStorage,
    state: Mutex<State>,
}

struct State {
    /// The doc version `scene`/`index` reflect; `None` = stale.
    frontiers: Option<Frontiers>,
    scene: Scene,
    index: HashMap<NodeId, TreeID>,
    blobs: BTreeMap<AssetId, Vec<u8>>,
}

impl LoroScene {
    /// Wrap a doc (empty, or holding a scene written by any replica), with
    /// external asset storage.
    #[must_use]
    pub fn new(doc: CrdtDoc) -> Self {
        Self::with_storage(doc, AssetStorage::External)
    }

    /// [`Self::new`] with an explicit [`AssetStorage`].
    #[must_use]
    pub fn with_storage(doc: CrdtDoc, storage: AssetStorage) -> Self {
        doc.loro().get_tree(NODES).enable_fractional_index(0);
        let state = State {
            frontiers: None,
            scene: empty_scene(),
            index: HashMap::new(),
            blobs: BTreeMap::new(),
        };
        Self {
            doc,
            storage,
            state: Mutex::new(state),
        }
    }

    /// A fresh ephemeral replica holding `scene` (external asset storage).
    #[must_use]
    pub fn from_scene(scene: &Scene) -> Self {
        Self::from_scene_with(scene, AssetStorage::External)
    }

    /// [`Self::from_scene`] with an explicit [`AssetStorage`].
    #[must_use]
    pub fn from_scene_with(scene: &Scene, storage: AssetStorage) -> Self {
        let this = Self::with_storage(CrdtDoc::ephemeral(), storage);
        // A fresh doc accepts every write below; a failure here is a bug.
        this.write_scene(scene)
            .expect("writing a scene into a fresh doc");
        this
    }

    /// A replica restored from snapshot/update bytes (e.g. a bundle's
    /// `doc.loro`). With external storage, supply the bytes through
    /// [`Self::put_blob`].
    ///
    /// # Errors
    /// The bytes aren't a Loro export.
    pub fn from_snapshot(bytes: &[u8], storage: AssetStorage) -> Result<Self, LoroError> {
        let this = Self::with_storage(CrdtDoc::ephemeral(), storage);
        this.import(bytes)?;
        Ok(this)
    }

    /// The underlying doc (sync transport, persistence, time travel).
    #[must_use]
    pub fn doc(&self) -> &CrdtDoc {
        &self.doc
    }

    #[must_use]
    pub fn storage(&self) -> AssetStorage {
        self.storage
    }

    /// The current scene, materialized.
    #[must_use]
    pub fn scene(&self) -> Scene {
        self.lock_fresh().scene.clone()
    }

    /// Read the current scene without cloning it.
    pub fn with_scene<R>(&self, f: impl FnOnce(&Scene) -> R) -> R {
        f(&self.lock_fresh().scene)
    }

    /// Apply one op; returns the ops that revert it (as [`Scene::apply`]).
    ///
    /// # Errors
    /// [`Error::Op`] leaves the doc untouched.
    pub fn apply(&self, op: Op) -> Result<Vec<Op>, Error> {
        let mut st = self.lock_fresh();
        let inverse = st.scene.apply(op.clone())?;
        self.write_ops(&mut st, std::slice::from_ref(&op))?;
        Ok(inverse)
    }

    /// Apply ops atomically — validated against the model first, then
    /// written to the doc as a single commit. Returns the inverse
    /// transaction (as [`Scene::transact`]).
    ///
    /// # Errors
    /// [`Error::Op`] (the first failing op) leaves the doc untouched.
    pub fn transact(&self, ops: impl IntoIterator<Item = Op>) -> Result<Vec<Op>, Error> {
        let ops: Vec<Op> = ops.into_iter().collect();
        let mut st = self.lock_fresh();
        let mut trial = st.scene.clone();
        let inverse = trial.transact(ops.clone())?;
        st.scene = trial;
        self.write_ops(&mut st, &ops)?;
        Ok(inverse)
    }

    /// Merge remote snapshot/update bytes.
    ///
    /// # Errors
    /// The bytes aren't a Loro export (or don't apply to this doc).
    pub fn import(&self, bytes: &[u8]) -> Result<(), LoroError> {
        self.doc.loro().import(bytes)?;
        Ok(())
    }

    /// A full snapshot of the doc (history included; asset bytes only with
    /// [`AssetStorage::Inline`]).
    ///
    /// # Panics
    /// Never in practice: a snapshot export of an attached doc can't fail.
    #[must_use]
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.doc
            .loro()
            .export(ExportMode::Snapshot)
            .expect("snapshot export")
    }

    /// The updates a peer at `since` is missing.
    ///
    /// # Panics
    /// Never in practice: an update export of an attached doc can't fail.
    #[must_use]
    pub fn export_updates(&self, since: &VersionVector) -> Vec<u8> {
        self.doc
            .loro()
            .export(ExportMode::updates(since))
            .expect("updates export")
    }

    /// This replica's version vector (what to send a peer to get a delta).
    #[must_use]
    pub fn version(&self) -> VersionVector {
        self.doc.loro().oplog_vv()
    }

    /// Call `f` after every change to the doc — local commits and imports.
    /// Keep the returned subscription alive for as long as you listen.
    pub fn subscribe(&self, f: impl Fn() + Send + Sync + 'static) -> Subscription {
        self.doc.loro().subscribe_root(Arc::new(move |_| f()))
    }

    /// Store asset bytes in the local blob store (external storage) and
    /// return their id. Doesn't add the asset to the scene — that's
    /// [`Op::PutAsset`]; this supplies bytes the doc already references.
    pub fn put_blob(&self, bytes: Vec<u8>) -> AssetId {
        let id = AssetId::of(&bytes);
        let mut st = self.lock();
        st.blobs.insert(id.clone(), bytes);
        st.frontiers = None;
        id
    }

    /// Every asset the doc references, with its mime type — whether or not
    /// this replica holds its bytes.
    #[must_use]
    pub fn asset_entries(&self) -> Vec<(AssetId, String)> {
        let assets = self.doc.loro().get_map(ASSETS);
        let mut out = Vec::new();
        assets.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::Map(m)) = v {
                let mime = match m.get("mime") {
                    Some(LoroValue::String(s)) => s.to_string(),
                    _ => String::new(),
                };
                out.push((AssetId(k.to_string()), mime));
            }
        });
        out.sort();
        out
    }

    /// Assets the doc references whose bytes this replica doesn't have.
    #[must_use]
    pub fn missing_assets(&self) -> Vec<AssetId> {
        self.with_scene(|scene| {
            self.asset_entries()
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| !scene.assets.contains_key(id))
                .collect()
        })
    }

    // ── internals ────────────────────────────────────────────────────────

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state, rematerialized if the doc moved since it was built.
    fn lock_fresh(&self) -> MutexGuard<'_, State> {
        let mut st = self.lock();
        let now = self.doc.loro().state_frontiers();
        if st.frontiers.as_ref() != Some(&now) {
            let (scene, index) = materialize(self.doc.loro(), &st.blobs);
            st.scene = scene;
            st.index = index;
            st.frontiers = Some(now);
        }
        st
    }

    /// Write validated ops (already applied to `st.scene`) and commit once.
    fn write_ops(&self, st: &mut State, ops: &[Op]) -> Result<(), Error> {
        let doc = self.doc.loro();
        let result = ops.iter().try_for_each(|op| self.write_op(doc, st, op));
        doc.commit();
        match result {
            Ok(()) => {
                st.frontiers = Some(doc.state_frontiers());
                Ok(())
            }
            Err(e) => {
                st.frontiers = None;
                Err(e.into())
            }
        }
    }

    fn write_op(&self, doc: &LoroDoc, st: &mut State, op: &Op) -> Result<(), LoroError> {
        let tree = doc.get_tree(NODES);
        let tid = |st: &State, id: NodeId| {
            st.index
                .get(&id)
                .copied()
                .ok_or(LoroError::NotFoundError(id.to_string().into()))
        };
        match op {
            Op::Insert {
                node,
                parent,
                index,
            } => {
                let parent = tid(st, *parent)?;
                let len = tree.children_num(parent).unwrap_or(0);
                let at = (*index as usize).min(len);
                let created = tree.create_at(parent, at)?;
                write_node(&tree.get_meta(created)?, node)?;
                st.index.insert(node.id, created);
            }
            Op::Remove { id } => tree.delete(tid(st, *id)?)?,
            Op::Move { id, parent, index } => {
                let target = tid(st, *id)?;
                let parent = tid(st, *parent)?;
                // `Scene` semantics: take the node out, then insert at `index`
                // (clamped) — i.e. `index` is the final position.
                let others = tree
                    .children(parent)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| *c != target)
                    .count();
                tree.mov_to(target, parent, (*index as usize).min(others))?;
            }
            Op::Set { id, prop } => {
                let meta = tree.get_meta(tid(st, *id)?)?;
                let (key, value) = prop_entry(prop);
                meta.insert(key, value::to_loro(&value))?;
            }
            Op::PutAsset { asset } => self.write_asset(doc, st, asset)?,
            Op::DropAsset { id } => {
                doc.get_map(ASSETS).delete(&id.0)?;
                st.blobs.remove(id);
            }
        }
        Ok(())
    }

    fn write_asset(&self, doc: &LoroDoc, st: &mut State, asset: &Asset) -> Result<(), LoroError> {
        let mut entry: HashMap<String, LoroValue> = HashMap::new();
        entry.insert("mime".into(), asset.mime.as_str().into());
        entry.insert(
            "size".into(),
            LoroValue::I64(i64::try_from(asset.bytes.len()).unwrap_or(i64::MAX)),
        );
        match self.storage {
            AssetStorage::Inline => {
                entry.insert("bytes".into(), asset.bytes.clone().into());
            }
            AssetStorage::External => {
                st.blobs.insert(asset.id.clone(), asset.bytes.clone());
            }
        }
        doc.get_map(ASSETS)
            .insert(&asset.id.0, LoroValue::Map(entry.into()))
    }

    /// Write a whole scene into this (empty) replica, as one commit.
    fn write_scene(&self, scene: &Scene) -> Result<(), LoroError> {
        let doc = self.doc.loro();
        let tree = doc.get_tree(NODES);
        let mut st = self.lock();
        doc.get_map(SCENE).insert("root", scene.root.to_string())?;
        let mut stack: Vec<(NodeId, Option<TreeID>)> = vec![(scene.root, None)];
        while let Some((id, parent)) = stack.pop() {
            let Some(node) = scene.node(id) else { continue };
            let created = tree.create(parent)?;
            write_node(&tree.get_meta(created)?, node)?;
            st.index.insert(id, created);
            // Reverse push → children are created (appended) in order.
            stack.extend(node.children.iter().rev().map(|c| (*c, Some(created))));
        }
        for asset in scene.assets.values() {
            self.write_asset(doc, &mut st, asset)?;
        }
        doc.commit();
        st.frontiers = None;
        Ok(())
    }
}

impl std::fmt::Debug for LoroScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoroScene")
            .field("storage", &self.storage)
            .finish_non_exhaustive()
    }
}

// ── encoding ─────────────────────────────────────────────────────────────

/// The fields of `Node` that are structure, not meta.
const STRUCTURAL: [&str; 2] = ["parent", "children"];

/// Serialize a model value. The model's types are plain data (string map
/// keys, no custom serializers), so this can't fail.
fn json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Write every meta key of `node` (all fields but parent/children).
fn write_node(meta: &LoroMap, node: &Node) -> Result<(), LoroError> {
    let Value::Object(fields) = json(node) else {
        return Ok(());
    };
    for (key, value) in &fields {
        if !STRUCTURAL.contains(&key.as_str()) {
            meta.insert(key, value::to_loro(value))?;
        }
    }
    Ok(())
}

/// Read a node (children empty, parent unset) back out of its meta.
fn read_node(meta: &LoroMap) -> Option<Node> {
    let LoroValue::Map(map) = meta.get_value() else {
        return None;
    };
    let Value::Object(mut fields) = value::from_loro(&LoroValue::Map(map)) else {
        return None;
    };
    fields.insert("parent".into(), Value::Null);
    fields.insert("children".into(), Value::Array(Vec::new()));
    serde_json::from_value(Value::Object(fields)).ok()
}

fn meta_id(meta: &LoroMap) -> Option<NodeId> {
    match meta.get("id") {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Uuid::parse_str(&s).ok().map(NodeId),
        _ => None,
    }
}

/// The meta key and serialized value a `Prop` assigns — the key is the
/// `Node` field the prop sets.
fn prop_entry(prop: &Prop) -> (&'static str, Value) {
    match prop {
        Prop::Name(v) => ("name", json(v)),
        Prop::Kind(v) => ("kind", json(v)),
        Prop::Visible(v) => ("visible", json(v)),
        Prop::Locked(v) => ("locked", json(v)),
        Prop::Transform(v) => ("transform", json(v)),
        Prop::Size(v) => ("size", json(v)),
        Prop::Opacity(v) => ("opacity", json(v)),
        Prop::Blend(v) => ("blend", json(v)),
        Prop::CornerRadii(v) => ("corner_radii", json(v)),
        Prop::ClipsContent(v) => ("clips_content", json(v)),
        Prop::Mask(v) => ("mask", json(v)),
        Prop::Fills(v) => ("fills", json(v)),
        Prop::Stroke(v) => ("stroke", json(v)),
        Prop::Effects(v) => ("effects", json(v)),
        Prop::FillGeometry(v) => ("fill_geometry", json(v)),
        Prop::StrokeGeometry(v) => ("stroke_geometry", json(v)),
        Prop::SvgSnapshot(v) => ("svg_snapshot", json(v)),
        Prop::Layout(v) => ("layout", json(v)),
        Prop::LayoutChild(v) => ("layout_child", json(v)),
        Prop::Control(v) => ("control", json(v)),
        Prop::Emission(v) => ("emission", json(v)),
        Prop::Shader(v) => ("shader", json(v)),
        Prop::Motion(v) => ("motion", json(v)),
    }
}

// ── materialization ──────────────────────────────────────────────────────

/// A placeholder for a doc that holds no scene yet.
fn empty_scene() -> Scene {
    Scene {
        root: NodeId(Uuid::nil()),
        nodes: BTreeMap::new(),
        assets: BTreeMap::new(),
        variables: document_proto::Variables::default(),
    }
}

/// Read the doc back into a `Scene`, plus the `NodeId → TreeID` index.
///
/// Lenient by design — a replica must render whatever peers sent: tree
/// nodes that don't decode are skipped with their subtrees, and if two
/// peers concurrently inserted the same `NodeId`, the lowest `TreeID` wins
/// (on every replica alike) and the other copy is ignored.
fn materialize(
    doc: &LoroDoc,
    blobs: &BTreeMap<AssetId, Vec<u8>>,
) -> (Scene, HashMap<NodeId, TreeID>) {
    let tree: LoroTree = doc.get_tree(NODES);
    let mut index: HashMap<NodeId, TreeID> = HashMap::new();
    for tid in tree.nodes() {
        if tree.is_node_deleted(&tid).unwrap_or(true) {
            continue;
        }
        let Some(id) = tree.get_meta(tid).ok().as_ref().and_then(meta_id) else {
            continue;
        };
        index
            .entry(id)
            .and_modify(|t| *t = (*t).min(tid))
            .or_insert(tid);
    }

    let mut scene = empty_scene();
    scene.assets = read_assets(doc, blobs);
    let root = match doc.get_map(SCENE).get("root") {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Uuid::parse_str(&s).ok().map(NodeId),
        _ => None,
    };
    let Some(root) = root else {
        return (scene, index);
    };
    scene.root = root;
    let Some(root_tid) = index.get(&root).copied() else {
        return (scene, index);
    };
    let Some(root_node) = tree.get_meta(root_tid).ok().as_ref().and_then(read_node) else {
        return (scene, index);
    };

    let mut stack = vec![(root_tid, root_node)];
    while let Some((tid, mut node)) = stack.pop() {
        for child_tid in tree.children(tid).unwrap_or_default() {
            let Some(mut child) = tree.get_meta(child_tid).ok().as_ref().and_then(read_node) else {
                continue;
            };
            if index.get(&child.id) != Some(&child_tid) {
                continue;
            }
            child.parent = Some(node.id);
            node.children.push(child.id);
            stack.push((child_tid, child));
        }
        scene.nodes.insert(node.id, node);
    }
    (scene, index)
}

fn read_assets(doc: &LoroDoc, blobs: &BTreeMap<AssetId, Vec<u8>>) -> BTreeMap<AssetId, Asset> {
    let mut out = BTreeMap::new();
    doc.get_map(ASSETS).for_each(|key, v| {
        let ValueOrContainer::Value(LoroValue::Map(entry)) = v else {
            return;
        };
        let id = AssetId(key.to_string());
        let mime = match entry.get("mime") {
            Some(LoroValue::String(s)) => s.to_string(),
            _ => String::new(),
        };
        let bytes = match entry.get("bytes") {
            Some(LoroValue::Binary(b)) => Some(b.to_vec()),
            _ => blobs.get(&id).cloned(),
        };
        if let Some(bytes) = bytes {
            out.insert(id.clone(), Asset { id, mime, bytes });
        }
    });
    out
}
