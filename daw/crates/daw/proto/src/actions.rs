//! DAW action grouping roots.
//!
//! The group names the DAW's action sets sort under. There used to be a
//! `define_actions!` block here too, existing only to give those sets a
//! title — and the only sets that asked for it, `fts.transport` and
//! `fts.markers_regions`, were declared and never referenced by
//! anything. All three went with the move to `#[architect::actions]`,
//! where a group is a plain string on the action rather than a root that
//! has to be declared before it can be named.

pub const GROUP_TRANSPORT: &str = "Transport";
pub const GROUP_MARKERS_REGIONS: &str = "Markers/Regions";
