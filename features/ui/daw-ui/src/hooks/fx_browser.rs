//! Hook for the FX Parameter Browser panel.
//!
//! Manages the lifecycle of DAW FX data: poll-waits for DAW connection,
//! fetches tracks, loads FX chains when a track is selected, loads
//! parameters when an FX is selected, and subscribes to live parameter updates.

use crate::prelude::*;
use crate::signals::{
    FX_CHAIN, FX_DAW_CONNECTED, FX_LOADING, FX_PARAMETERS, FX_SELECTED_FX, FX_SELECTED_TRACK,
    FX_TRACKS,
};
use daw_proto::FxEvent;

/// Hook that initializes the FX browser and reacts to selection changes.
///
/// 1. Poll-waits for DAW connection, then fetches track list
/// 2. Watches `FX_SELECTED_TRACK` — loads FX chain when it changes
/// 3. Watches `FX_SELECTED_FX` — loads parameters and subscribes to live events
pub fn use_fx_browser_subscription() {
    // Effect 1: Poll-wait for DAW connection, then fetch tracks
    use_effect(move || {
        spawn(async move {
            tracing::info!("FX browser: waiting for DAW connection...");

            // Poll until DAW is initialized (same pattern as latency_task in main.rs)
            loop {
                if daw_control::Daw::try_get().is_some() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }

            let daw = daw_control::Daw::get();
            *FX_DAW_CONNECTED.write() = true;
            tracing::info!("FX browser: DAW connected");

            match daw.current_project().await {
                Ok(project) => match project.tracks().all().await {
                    Ok(tracks) => {
                        tracing::info!("FX browser: loaded {} tracks", tracks.len());
                        *FX_TRACKS.write() = tracks;
                    }
                    Err(e) => {
                        tracing::warn!("FX browser: failed to fetch tracks: {:?}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("FX browser: failed to get current project: {:?}", e);
                }
            }
        });
    });

    // Effect 2: When selected track changes, load its FX chain
    use_effect(move || {
        let track_guid = FX_SELECTED_TRACK.read().clone();
        spawn(async move {
            let Some(guid) = track_guid else {
                *FX_CHAIN.write() = Vec::new();
                *FX_SELECTED_FX.write() = None;
                *FX_PARAMETERS.write() = Vec::new();
                return;
            };

            let Some(daw) = daw_control::Daw::try_get() else {
                tracing::warn!("FX browser: DAW not available for track load");
                return;
            };

            tracing::info!("FX browser: loading FX chain for track {}", guid);
            *FX_LOADING.write() = true;

            let result: Result<_, eyre::Report> = async {
                let project = daw.current_project().await?;
                let track = project
                    .tracks()
                    .by_guid(&guid)
                    .await?
                    .ok_or_else(|| eyre::eyre!("Track not found: {}", guid))?;
                let fx_list = track.fx_chain().all().await?;
                Ok(fx_list)
            }
            .await;

            match result {
                Ok(fx_list) => {
                    tracing::info!("FX browser: loaded {} FX for track", fx_list.len());
                    *FX_CHAIN.write() = fx_list;
                }
                Err(e) => {
                    tracing::warn!("FX browser: failed to load FX chain: {:?}", e);
                    *FX_CHAIN.write() = Vec::new();
                }
            }

            // Clear FX selection when track changes
            *FX_SELECTED_FX.write() = None;
            *FX_PARAMETERS.write() = Vec::new();
            *FX_LOADING.write() = false;
        });
    });

    // Effect 3: When selected FX changes, load parameters and subscribe to events
    use_effect(move || {
        let fx_guid = FX_SELECTED_FX.read().clone();
        let track_guid = FX_SELECTED_TRACK.read().clone();
        spawn(async move {
            let Some(fx_guid) = fx_guid else {
                *FX_PARAMETERS.write() = Vec::new();
                return;
            };
            let Some(track_guid) = track_guid else {
                return;
            };
            let Some(daw) = daw_control::Daw::try_get() else {
                tracing::warn!("FX browser: DAW not available for parameter load");
                return;
            };

            tracing::info!("FX browser: loading parameters for FX {}", fx_guid);
            *FX_LOADING.write() = true;

            let result: Result<(), eyre::Report> = async {
                let project = daw.current_project().await?;
                let track = project
                    .tracks()
                    .by_guid(&track_guid)
                    .await?
                    .ok_or_else(|| eyre::eyre!("Track not found"))?;
                let chain = track.fx_chain();
                let fx = chain
                    .by_guid(&fx_guid)
                    .await?
                    .ok_or_else(|| eyre::eyre!("FX not found"))?;

                // Fetch all parameters
                let params = fx.parameters().await?;
                tracing::info!("FX browser: loaded {} parameters", params.len());
                *FX_PARAMETERS.write() = params;
                *FX_LOADING.write() = false;

                // FX event streaming retired with the architect::rpc
                // port (sibling-trait territory). Live updates pause
                // until the sibling trait lands — until then the
                // parameter view is a one-shot snapshot.
                let _ = chain;
                let _ = fx_guid;
                let _: &FxEvent;

                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::warn!("FX browser: parameter subscription failed: {:?}", e);
                *FX_LOADING.write() = false;
            }
        });
    });
}
