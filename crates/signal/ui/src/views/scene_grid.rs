//! Rig scene grid -- domain-aware scene grid wrapping SceneTileGrid.
//!
//! Fetches rig data from the controller and maps [`RigScene`] variants
//! to [`TileData`] for the dumb [`SceneTileGrid`] component.

use dioxus::prelude::*;
use signal::rig::Rig;

use crate::components::{SceneTileGrid, TileData};

// region: --- RigSceneGrid

/// A domain-aware scene grid for a Rig.
///
/// Fetches the rig from the controller, maps its variants to tile data,
/// and renders them using the dumb `SceneTileGrid` component.
#[component]
pub fn RigSceneGrid(
    /// Rig collection ID to display scenes for.
    rig_id: String,
    /// Currently active scene ID, if any.
    #[props(default)]
    active_scene_id: Option<String>,
    /// Callback when a scene tile is selected.
    on_scene_select: EventHandler<String>,
) -> Element {
    let signal = crate::use_signal_service();
    let mut rig = use_signal(|| None::<Rig>);

    // Fetch rig when rig_id changes.
    {
        let signal = signal.clone();
        let rig_id = rig_id.clone();
        use_effect(move || {
            let signal = signal.clone();
            let rig_id = rig_id.clone();
            spawn(async move {
                rig.set(signal.rigs().load(rig_id.as_str()).await.ok().flatten());
            });
        });
    }

    let current_rig = rig();

    match current_rig {
        None => rsx! {
            div { class: "flex items-center justify-center h-full text-sm text-zinc-500",
                "Loading rig..."
            }
        },
        Some(r) => {
            let scene_ids: Vec<String> = r.variants.iter().map(|v| v.id.to_string()).collect();

            let tiles: Vec<TileData> = r
                .variants
                .iter()
                .map(|v| TileData {
                    name: v.name.clone(),
                    subtitle: None,
                    active: active_scene_id
                        .as_ref()
                        .map_or(false, |aid| aid == &v.id.to_string()),
                    empty: false,
                    preloaded: true,
                    loading: false,
                    disabled: false,
                })
                .collect();

            let slot_count = tiles.len().max(8);

            rsx! {
                SceneTileGrid {
                    tiles,
                    slot_count,
                    on_tile_click: move |idx: usize| {
                        if let Some(scene_id) = scene_ids.get(idx) {
                            on_scene_select.call(scene_id.clone());
                        }
                    },
                }
            }
        }
    }
}

// endregion: --- RigSceneGrid
