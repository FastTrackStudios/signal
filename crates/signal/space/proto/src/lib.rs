//! Wire contract for the sample space (`docs/spec/sample-space.md`, #77):
//! browse built `.space` maps, query similarity, audition items, kick off
//! (re)builds with a progress stream. Wasm-clean — the browser remote's map
//! view consumes exactly this surface.

use facet::Facet;

/// One space (a built map) the engine knows about.
#[derive(Facet, Clone, Debug, Default)]
pub struct SpaceInfo {
    pub name: String,
    /// Library root the space was built over.
    pub root: String,
    pub item_count: u32,
}

/// One node on the map. `idx` is the stable in-space index used by
/// [`SampleSpace::similar`] / [`SampleSpace::audition`].
#[derive(Facet, Clone, Debug, Default)]
pub struct MapItem {
    pub idx: u32,
    /// Piece key or relative sample path.
    pub path: String,
    pub class: String,
    /// Normalized map coords (0..1).
    pub x: f32,
    pub y: f32,
    pub duration_s: f32,
    pub centroid_hz: f32,
    pub percussiveness: f32,
    pub favorite: bool,
}

/// Filters — the XO rule: they re-scope the map AND every similarity list.
#[derive(Facet, Clone, Debug, Default)]
pub struct SpaceFilter {
    /// Empty = all classes.
    pub classes: Vec<String>,
    /// Case-insensitive substring on the item path.
    pub text: String,
    pub favorites_only: bool,
    /// 0 = no limit (seconds).
    pub max_duration_s: f32,
}

#[derive(Facet, Clone, Debug, Default)]
pub struct SimilarHit {
    pub idx: u32,
    pub path: String,
    pub class: String,
    pub score: f32,
}

/// Build/rebuild progress + map invalidation events.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SpaceEvent {
    /// (space, analyzed, total)
    Progress(String, u32, u32),
    /// Space list or a map changed — refetch.
    Changed,
}

pub mod space {
    //! `SampleSpace` → `SampleSpaceClient` / `SampleSpaceService`.
    use super::{MapItem, SimilarHit, SpaceEvent, SpaceFilter, SpaceInfo};

    #[architect::rpc]
    pub trait SampleSpace {
        /// All built spaces discovered under the configured library roots.
        fn spaces(&self) -> Vec<SpaceInfo>;
        /// Every mappable item of a space (already filtered server-side).
        fn map(&self, space: String, filter: SpaceFilter) -> Vec<MapItem>;
        /// Top-k most similar items to `idx`, scoped by the same filter.
        fn similar(&self, space: String, idx: u32, filter: SpaceFilter) -> Vec<SimilarHit>;
        /// Preview an item on the engine's audio output.
        fn audition(&self, space: String, idx: u32);
        fn set_favorite(&self, space: String, idx: u32, favorite: bool);
        /// (Re)build the space over its root (or a new root for a new name).
        fn build(&self, name: String, root: String, pieces: bool);
        #[subscribe]
        fn events(&self) -> SpaceEvent;
    }
}
