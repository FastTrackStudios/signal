//! Profile patch grid -- domain-aware grid wrapping SceneTileGrid.
//!
//! Fetches a profile from the controller and maps its [`Patch`] entries
//! to [`TileData`] for the dumb [`SceneTileGrid`] component.
//! Clicking a tile activates that patch via `profiles().activate()`.

use dioxus::prelude::*;
use signal::profile::Profile;

use crate::components::{SceneTileGrid, TileData};

/// A domain-aware patch grid for a Profile.
///
/// Loads the profile, maps its patches to colored tiles, and calls
/// `profiles().activate(profile_id, Some(patch_id))` on tile click.
#[component]
pub fn ProfilePatchGrid(
    /// Profile collection ID to display patches for.
    profile_id: String,
    /// Currently active patch ID, if any.
    #[props(default)]
    active_patch_id: Option<String>,
    /// Callback when a patch tile is selected. Receives `(profile_id, patch_id)`.
    on_patch_select: EventHandler<(String, String)>,
) -> Element {
    let signal = crate::use_signal_service();
    let mut profile = use_signal(|| None::<Profile>);

    // Fetch profile when profile_id changes.
    {
        let signal = signal.clone();
        let profile_id = profile_id.clone();
        use_effect(move || {
            let signal = signal.clone();
            let profile_id = profile_id.clone();
            spawn(async move {
                profile.set(
                    signal
                        .profiles()
                        .load(profile_id.as_str())
                        .await
                        .ok()
                        .flatten(),
                );
            });
        });
    }

    let current_profile = profile();

    match current_profile {
        None => rsx! {
            div { class: "flex items-center justify-center h-full text-sm text-zinc-500",
                "Loading profile..."
            }
        },
        Some(p) => {
            let pid = p.id.to_string();
            let patch_ids: Vec<String> =
                p.patches.iter().map(|patch| patch.id.to_string()).collect();

            let tiles: Vec<TileData> = p
                .patches
                .iter()
                .map(|patch| TileData {
                    name: patch.name.clone(),
                    subtitle: None,
                    active: active_patch_id
                        .as_ref()
                        .map_or(false, |aid| aid == &patch.id.to_string()),
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
                        if let Some(patch_id) = patch_ids.get(idx) {
                            on_patch_select.call((pid.clone(), patch_id.clone()));
                        }
                    },
                }
            }
        }
    }
}
