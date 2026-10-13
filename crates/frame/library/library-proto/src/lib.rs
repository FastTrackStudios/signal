//! The `library` contract: storing, finding and live-following surfaces.
//!
//! - [`Signature`]: the engine addresses a surface binds (what it *needs*)
//!   or an engine offers (what it *has*). A surface fits an engine when its
//!   signature is a subset of the engine's, so it only offers itself where
//!   every binding resolves.
//! - [`SurfaceMeta`], [`Content`], [`Surface`]: what the library holds.
//! - [`store::SurfaceStore`]: the service any library backend implements
//!   (`library-fs`, a shared community server). Hosts list, fetch, publish,
//!   and subscribe to a surface's updates for live reload.
//!
//! ## Updates are whole snapshots, not Loro deltas
//!
//! [`SurfaceUpdate`] carries the surface's full content with its version.
//! A surface is small (tens of KB of markup or a zipped Loro snapshot), the
//! main writers are external (an agent saving a `.frame`, a person's editor)
//! and share no CRDT history with the subscribers, so a delta would first
//! need a canonical replica per surface on the server to diff against. A
//! snapshot is idempotent, needs no ordering or catch-up protocol (a late
//! or reconnecting subscriber just takes the latest), and a host can
//! validate the whole document before swapping it in. When several editors
//! share one live Loro document, deltas can ride a second variant of
//! [`Content`] without changing the rpc.

use std::collections::BTreeSet;

use document_proto::{Asset, Scene};
use facet::Facet;
use serde::{Deserialize, Serialize};
use surface_proto::{Binding, Catalog, Control, ControlKind, InputSource, Source};

/// A surface's id in a library: its path under the library root without
/// the extension, `/`-separated (`comp/07-vari-mu`).
pub type SurfaceId = String;

/// The engine addresses a surface binds, or an engine offers. Each list is
/// sorted and deduplicated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet, Serialize, Deserialize)]
pub struct Signature {
    pub params: Vec<String>,
    pub actions: Vec<String>,
    pub streams: Vec<String>,
    pub collections: Vec<String>,
}

impl Signature {
    /// What `scene`'s controls bind: params (incl. XY second axes and meter
    /// switches), actions, streams (incl. switched meter taps), collections.
    #[must_use]
    pub fn of_scene(scene: &Scene) -> Self {
        let mut sets: [BTreeSet<String>; 4] = Default::default();
        for node in scene.nodes.values() {
            let Some(control) = &node.control else {
                continue;
            };
            // A templated control (`{n}`) binds every item of its repeat —
            // the enclosing template's, else the one its context names.
            if control.is_templated() {
                let (first, count) = repeat_of(scene, node.id, control).unwrap_or((1, 1));
                for n in first..first.saturating_add(count) {
                    collect(&control.instantiate(n, |s| s), &mut sets);
                }
            } else {
                collect(control, &mut sets);
            }
        }
        Self::from_sets(sets)
    }

    /// Everything `catalog` offers.
    #[must_use]
    pub fn of_catalog(catalog: &Catalog) -> Self {
        Self::from_sets([
            catalog.params.iter().map(|p| p.addr.0.clone()).collect(),
            catalog.actions.iter().map(|a| a.id.0.clone()).collect(),
            catalog.streams.iter().map(|s| s.id.0.clone()).collect(),
            catalog.collections.iter().map(|c| c.id.0.clone()).collect(),
        ])
    }

    /// Both signatures' addresses (an engine serving several catalogs).
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let join = |a: &[String], b: &[String]| a.iter().chain(b).cloned().collect::<BTreeSet<_>>();
        Self::from_sets([
            join(&self.params, &other.params),
            join(&self.actions, &other.actions),
            join(&self.streams, &other.streams),
            join(&self.collections, &other.collections),
        ])
    }

    /// Whether every address this signature needs is in `engine`.
    #[must_use]
    pub fn fits(&self, engine: &Self) -> bool {
        let sub =
            |need: &[String], have: &[String]| need.iter().all(|n| have.binary_search(n).is_ok());
        sub(&self.params, &engine.params)
            && sub(&self.actions, &engine.actions)
            && sub(&self.streams, &engine.streams)
            && sub(&self.collections, &engine.collections)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
            && self.actions.is_empty()
            && self.streams.is_empty()
            && self.collections.is_empty()
    }

    fn from_sets([p, a, s, c]: [BTreeSet<String>; 4]) -> Self {
        Self {
            params: p.into_iter().collect(),
            actions: a.into_iter().collect(),
            streams: s.into_iter().collect(),
            collections: c.into_iter().collect(),
        }
    }
}

/// The `(first, count)` of the repeat a node's `{n}` refers to: its
/// enclosing template, or the template its context popover / menu names.
fn repeat_of(scene: &Scene, id: document_proto::NodeId, control: &Control) -> Option<(u32, u32)> {
    let range = |c: &Control| match &c.kind {
        ControlKind::Repeat { first, count, .. } => Some((*first, *count)),
        _ => None,
    };
    let mut cur = Some(id);
    while let Some(n) = cur.and_then(|c| scene.node(c)) {
        if let Some(r) = n.control.as_ref().and_then(range) {
            return Some(r);
        }
        if let Some(
            ControlKind::Popover {
                context: Some(t), ..
            }
            | ControlKind::Menu {
                context: Some(t), ..
            },
        ) = n.control.as_ref().map(|c| &c.kind)
        {
            return scene
                .node(document_proto::NodeId(*t))
                .and_then(|t| t.control.as_ref())
                .and_then(range);
        }
        cur = n.parent;
    }
    let _ = control;
    None
}

fn collect(control: &Control, [params, actions, streams, collections]: &mut [BTreeSet<String>; 4]) {
    // A curve's inputs and a handle's axes bind too (an EQ graph needs
    // every band's params).
    let mut extra: Vec<&Binding> = Vec::new();
    // Rules, commands, conditions, zooms name params too.
    if !matches!(
        control.kind,
        ControlKind::Curve { .. } | ControlKind::Handle { .. }
    ) {
        for addr in control.extra_params() {
            params.insert(addr.0.clone());
        }
    }
    match &control.kind {
        ControlKind::Curve { inputs, .. } => {
            for input in inputs {
                match &input.source {
                    InputSource::Param { addr } => {
                        params.insert(addr.0.clone());
                    }
                    InputSource::Stream { id, .. } => {
                        streams.insert(id.0.clone());
                    }
                    InputSource::Const { .. } => {}
                }
            }
        }
        ControlKind::Handle {
            x,
            y,
            wheel,
            x_map,
            y_map,
        } => {
            extra.extend(x.iter().chain(y).chain(wheel));
            for addr in x_map.after.iter().chain(&y_map.after) {
                params.insert(addr.0.clone());
            }
        }
        _ => {}
    }
    for binding in control
        .binding
        .iter()
        .chain(&control.y_binding)
        .chain(extra)
    {
        // `if let` rather than a `match`: new source kinds (feeds) are
        // additive and needn't break this.
        if let Source::Param { addr } = &binding.source {
            params.insert(addr.0.clone());
        } else if let Source::Action { id } = &binding.source {
            actions.insert(id.0.clone());
        } else if let Source::Stream { id, .. } = &binding.source {
            streams.insert(id.0.clone());
        } else if let Source::Collection { id } = &binding.source {
            collections.insert(id.0.clone());
        }
    }
    if let Some(select) = &control.select {
        params.insert(select.param.0.clone());
        for tap in select.taps.iter().flatten() {
            streams.insert(tap.stream.0.clone());
        }
    }
}

/// How a surface's bytes are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum Format {
    /// A `.frame` bundle (zip: manifest, Loro snapshot, assets).
    Frame,
    /// frame markup text (`.frame`, or the legacy `.fm`), UTF-8.
    Markup,
}

impl Format {
    /// The file extension, without the dot: `frame` for both formats (a
    /// bundle is told from markup by its content, not its name).
    #[must_use]
    pub fn extension(self) -> &'static str {
        "frame"
    }
}

/// A surface document, as stored.
///
/// Markup names its images and fonts by id only, so a markup surface
/// carries their bytes alongside (what `<name>.frame.assets/` holds on disk):
/// a surface shared through a library brings its textures and fonts. A
/// bundle holds its own; its `assets` stay empty.
#[derive(Debug, Clone, PartialEq, Eq, Facet, Serialize, Deserialize)]
pub struct Content {
    pub format: Format,
    pub bytes: Vec<u8>,
    /// The assets markup `bytes` reference (empty for a bundle).
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// What the library knows about one surface.
#[derive(Debug, Clone, Default, PartialEq, Facet, Serialize, Deserialize)]
pub struct SurfaceMeta {
    pub id: SurfaceId,
    pub name: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    /// What the surface binds; it fits engines offering all of it.
    pub fits: Signature,
    /// A PNG thumbnail (empty when the backend can't render one).
    pub preview_png: Vec<u8>,
    /// Bumped on every change (publish, or an edit on disk).
    pub version: u64,
    /// SPDX id or free text.
    pub license: String,
    /// `id@version` this one was forked from.
    pub forked_from: Option<String>,
}

/// A surface: its metadata and its content.
#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct Surface {
    pub meta: SurfaceMeta,
    pub content: Content,
}

/// A change to one surface, pushed to its subscribers: the whole new
/// snapshot (see the crate docs for why), or `None` when it was removed.
#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct SurfaceUpdate {
    pub id: SurfaceId,
    pub version: u64,
    pub surface: Option<Surface>,
}

/// A change to the library's index (for browsers): the new metadata, or
/// `None` when the surface was removed.
#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct IndexChange {
    pub id: SurfaceId,
    pub meta: Option<SurfaceMeta>,
}

/// What to list. Empty fields match everything.
#[derive(Debug, Clone, Default, PartialEq, Facet, Serialize, Deserialize)]
pub struct ListQuery {
    /// Case-insensitive substring of id, name, description or a tag.
    pub text: String,
    /// Every one of these tags must be present.
    pub tags: Vec<String>,
    /// Only surfaces that fit this engine signature.
    pub fits: Option<Signature>,
    /// Include each surface's preview PNG. Off by default: a list of a big
    /// library with previews inline outgrows one message — fetch them with
    /// `preview` as they're shown.
    #[serde(default)]
    #[facet(default)]
    pub previews: bool,
}

impl ListQuery {
    /// Whether `meta` matches.
    #[must_use]
    pub fn matches(&self, meta: &SurfaceMeta) -> bool {
        let text = self.text.to_lowercase();
        let has_text = text.is_empty()
            || [&meta.id, &meta.name, &meta.description]
                .iter()
                .any(|s| s.to_lowercase().contains(&text))
            || meta.tags.iter().any(|t| t.to_lowercase().contains(&text));
        let has_tags = self
            .tags
            .iter()
            .all(|t| meta.tags.iter().any(|m| m.eq_ignore_ascii_case(t)));
        let fits = self
            .fits
            .as_ref()
            .is_none_or(|engine| meta.fits.fits(engine));
        has_text && has_tags && fits
    }
}

/// What a publisher says about a surface. The library computes `fits`,
/// the version and (when it can render) the preview itself.
#[derive(Debug, Clone, Default, PartialEq, Facet, Serialize, Deserialize)]
pub struct PublishMeta {
    /// The id to publish under (a new version when it exists); empty = a
    /// fresh id from the name.
    pub id: SurfaceId,
    pub name: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub license: String,
    pub forked_from: Option<String>,
    /// A thumbnail to use instead of rendering one (empty = render).
    pub preview_png: Vec<u8>,
}

/// Where a publish landed.
#[derive(Debug, Clone, PartialEq, Eq, Facet, Serialize, Deserialize)]
pub struct Published {
    pub id: SurfaceId,
    pub version: u64,
}

/// Why a library refused a call.
#[derive(Debug, Clone, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum LibraryError {
    /// No such surface (or version).
    NotFound(String),
    /// The content doesn't decode (markup parse error, broken bundle).
    BadContent(String),
    /// The id isn't a valid relative path.
    BadId(String),
    /// Storage failed.
    Io(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(what) => write!(f, "not found: {what}"),
            Self::BadContent(e) => write!(f, "bad content: {e}"),
            Self::BadId(id) => write!(f, "bad surface id `{id}`"),
            Self::Io(e) => write!(f, "storage: {e}"),
        }
    }
}

impl std::error::Error for LibraryError {}

pub mod store {
    //! The library service. `SurfaceStore` → `SurfaceStoreClient` /
    //! `SurfaceStoreService`, plus the stream sibling
    //! (`SurfaceStoreStreamClient`, `SurfaceStoreStreamService`, the
    //! `SurfaceStoreStreamSource` backend contract).
    use super::{
        Catalog, Content, IndexChange, LibraryError, ListQuery, PublishMeta, Published, Surface,
        SurfaceMeta, SurfaceUpdate,
    };

    /// A store of surfaces.
    #[architect::rpc]
    pub trait SurfaceStore {
        /// Every surface matching `query`, by id.
        fn list(&self, query: ListQuery) -> Vec<SurfaceMeta>;
        /// A surface at `version` (`None` = the latest).
        fn get(&self, id: String, version: Option<u64>) -> Result<Surface, LibraryError>;
        /// A surface's preview PNG (empty when it has none).
        fn preview(&self, id: String) -> Vec<u8>;
        /// Store `content` as a new surface, or a new version of `meta.id`.
        fn publish(&self, content: Content, meta: PublishMeta) -> Result<Published, LibraryError>;
        /// The engine catalogs the library knows (`*.catalog.json` next to
        /// its surfaces) — so a host without an engine can mock them.
        fn catalogs(&self) -> Vec<Catalog>;
        /// One surface's changes: its current snapshot on attach, then every
        /// new version (live reload).
        #[subscribe]
        fn updates(&self, id: String) -> SurfaceUpdate;
        /// Every index change (surfaces added, changed, removed).
        #[subscribe]
        fn changes(&self) -> IndexChange;
    }
}

pub use store::*;

#[cfg(feature = "codec")]
mod codec;
#[cfg(feature = "codec")]
pub use codec::*;

#[cfg(test)]
mod tests {
    use super::*;
    use surface_proto::{Binding, ControlKind};

    fn sig(params: &[&str], streams: &[&str]) -> Signature {
        Signature {
            params: params.iter().map(|s| (*s).to_owned()).collect(),
            streams: streams.iter().map(|s| (*s).to_owned()).collect(),
            ..Signature::default()
        }
    }

    #[test]
    fn subset_fits() {
        let engine = sig(&["a", "b", "c"], &["lvl"]);
        assert!(sig(&["a", "c"], &["lvl"]).fits(&engine));
        assert!(Signature::default().fits(&engine));
        assert!(!sig(&["a", "z"], &[]).fits(&engine));
        assert!(!sig(&[], &["gr"]).fits(&engine));
    }

    #[test]
    fn scene_signature_reads_bindings() {
        let mut scene = Scene::new("S");
        let page = scene.pages()[0];
        for (name, binding) in [
            ("k", Binding::param("amp/gain")),
            ("m", Binding::stream("out_level")),
        ] {
            let mut n = document_proto::Node::new(document_proto::NodeKind::Frame, name);
            let mut c = Control::new(ControlKind::knob());
            c.binding = Some(binding);
            n.control = Some(c);
            scene
                .apply(document_proto::Op::Insert {
                    node: n,
                    parent: page,
                    index: u32::MAX,
                })
                .unwrap();
        }
        assert_eq!(
            Signature::of_scene(&scene),
            sig(&["amp/gain"], &["out_level"])
        );
    }

    #[test]
    fn query_matching() {
        let meta = SurfaceMeta {
            id: "comp/07-vari-mu".into(),
            name: "Vari-Mu".into(),
            tags: vec!["comp".into()],
            fits: sig(&["a"], &[]),
            ..SurfaceMeta::default()
        };
        assert!(ListQuery::default().matches(&meta));
        assert!(
            ListQuery {
                text: "VARI".into(),
                ..ListQuery::default()
            }
            .matches(&meta)
        );
        assert!(
            ListQuery {
                tags: vec!["Comp".into()],
                ..ListQuery::default()
            }
            .matches(&meta)
        );
        assert!(
            !ListQuery {
                tags: vec!["eq".into()],
                ..ListQuery::default()
            }
            .matches(&meta)
        );
        assert!(
            ListQuery {
                fits: Some(sig(&["a", "b"], &[])),
                ..ListQuery::default()
            }
            .matches(&meta)
        );
        assert!(
            !ListQuery {
                fits: Some(sig(&["b"], &[])),
                ..ListQuery::default()
            }
            .matches(&meta)
        );
    }
}

#[cfg(test)]
mod fits_tests {
    use super::*;
    use surface_proto::{Axis, CurveInput, HandleAxis, ParamAddr, PlotRange, PlotStyle};

    #[test]
    fn curve_inputs_and_handle_axes_are_part_of_the_signature() {
        let mut sets: [BTreeSet<String>; 4] = Default::default();
        let curve = Control::new(ControlKind::Curve {
            provider: "eq.response".into(),
            inputs: vec![CurveInput {
                name: "b1_freq".into(),
                source: InputSource::Param {
                    addr: ParamAddr::new("eq/b1_freq"),
                },
            }],
            series: None,
            style: PlotStyle::Line,
            x: Axis::Log {
                f_min: 20.0,
                f_max: 20_000.0,
            },
            y: PlotRange {
                min: -18.0,
                max: 18.0,
                curve: surface_proto::Curve::Linear,
            },
        });
        collect(&curve, &mut sets);
        let axis = |after: Vec<ParamAddr>| HandleAxis {
            axis: Axis::Linear { min: 0.0, max: 1.0 },
            after,
            shift: 0.0,
        };
        let handle = Control::new(ControlKind::Handle {
            x: Some(Binding::param("adsr/decay")),
            y: None,
            wheel: Some(Binding::param("eq/b1_q")),
            x_map: axis(vec![ParamAddr::new("adsr/attack")]),
            y_map: axis(Vec::new()),
        });
        collect(&handle, &mut sets);
        let params: Vec<_> = sets[0].iter().cloned().collect();
        assert_eq!(
            params,
            ["adsr/attack", "adsr/decay", "eq/b1_freq", "eq/b1_q"]
        );
    }
}
