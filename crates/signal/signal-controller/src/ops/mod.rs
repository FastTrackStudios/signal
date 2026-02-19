//! Namespace handles for the signal controller API.
//!
//! Each handle groups related operations on a single entity type.
//! Obtain a handle via the corresponding accessor on [`SignalController`]:
//!
//! ```ignore
//! let profiles = ctrl.profiles();
//! profiles.list().await;
//! profiles.create("Worship", "Clean", target).await;
//! ```

mod block_presets;
mod blocks;
mod engines;
mod layers;
mod module_presets;
mod profiles;
mod racks;
mod rigs;
mod scene_templates;
mod setlists;
mod songs;

/// Reorder items in a `Vec` by a list of IDs.
///
/// Items matching `ordered_ids` are placed first (in that order),
/// followed by any remaining items not in the list.
pub(crate) fn reorder_by_id<T, Id: PartialEq>(
    items: &mut Vec<T>,
    ordered_ids: &[Id],
    get_id: impl Fn(&T) -> &Id,
) {
    let mut reordered = Vec::with_capacity(items.len());
    for id in ordered_ids {
        if let Some(pos) = items.iter().position(|x| get_id(x) == id) {
            reordered.push(items.remove(pos));
        }
    }
    reordered.append(items);
    *items = reordered;
}

pub use block_presets::BlockPresetOps;
pub use blocks::BlockOps;
pub use engines::EngineOps;
pub use layers::LayerOps;
pub use module_presets::ModulePresetOps;
pub use profiles::ProfileOps;
pub use racks::RackOps;
pub use rigs::RigOps;
pub use scene_templates::SceneTemplateOps;
pub use setlists::SetlistOps;
pub use songs::SongOps;
