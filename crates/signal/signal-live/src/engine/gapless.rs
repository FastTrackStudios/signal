//! Gapless FX switching engine.
//!
//! Orchestrates the load-silent → swap-channels → cleanup protocol
//! for switching blocks/modules within a running guitar rig.
//!
//! ## Protocol
//!
//! 1. Add new FX to chain (silenced via zero output pin mappings)
//! 2. Wait for FX to be fully loaded (poll `info()` for parameter_count > 0)
//! 3. Atomically swap: silence old FX pins, restore new FX pins
//! 4. Clean up old FX (remove or leave silenced)
//!
//! ## Why pin mappings instead of bypass?
//!
//! Bypassing an FX causes an audible click/gap because REAPER crossfades the
//! bypass transition. Zeroing output pin mappings suppresses all output routing
//! without any crossfade artifact — the FX simply has no output pins mapped.
//! This is the same technique used by ParanormalFX for parallel routing lanes.
//!
//! ## Why pin mappings instead of channel_config?
//!
//! REAPER's `TrackFX_SetNamedConfigParm("channel_config", ...)` silently fails —
//! `channel_config` is read-only via `GetNamedConfigParm` but not writable via
//! `SetNamedConfigParm`. Pin mappings (`TrackFX_SetPinMappings`) are the reliable
//! writable mechanism for controlling FX output routing.

use daw::{FxChain, FxHandle};
use daw::service::{FxContainerChannelConfig, FxNodeId, FxPinMappings};
use std::time::Duration;

/// Result of a gapless swap operation.
#[derive(Debug)]
pub enum SwapResult {
    /// Swap completed successfully.
    Success {
        /// GUID of the newly active FX.
        new_fx_guid: String,
        /// GUID of the old (now silenced/removed) FX.
        old_fx_guid: String,
    },
    /// New FX failed to load within the configured timeout.
    LoadTimeout {
        /// Name of the plugin that failed to load.
        fx_name: String,
    },
    /// Swap failed for another reason.
    Failed(String),
}

/// Configuration for gapless swap behavior.
#[derive(Debug, Clone)]
pub struct SwapConfig {
    /// How long to wait for the new FX to load before giving up.
    pub load_timeout: Duration,
    /// Whether to remove the old FX after swap (vs leaving it silenced).
    pub remove_old: bool,
    /// Polling interval while waiting for FX load.
    pub poll_interval: Duration,
}

impl Default for SwapConfig {
    fn default() -> Self {
        Self {
            load_timeout: Duration::from_secs(10),
            remove_old: true,
            poll_interval: Duration::from_millis(100),
        }
    }
}

/// Gapless swap engine — manages the pin-mapping protocol for live FX switching.
///
/// This engine operates at the `daw-control` level, using `FxChain` and `FxHandle`
/// to orchestrate the load → silence → swap → cleanup sequence. It is the concrete
/// implementation of the `SlotDiff::LoadAndActivate` path from the diff engine.
///
/// Non-container FX use output pin mapping zeroing (`TrackFX_SetPinMappings`)
/// to achieve instant silence without crossfade artifacts. Container FX use
/// the `container_nch_out` named config param instead.
pub struct GaplessSwapEngine {
    config: SwapConfig,
}

impl GaplessSwapEngine {
    pub fn new() -> Self {
        Self {
            config: SwapConfig::default(),
        }
    }

    pub fn with_config(config: SwapConfig) -> Self {
        Self { config }
    }

    /// Swap a block: add new FX silently, wait for load, swap pin mappings, cleanup old.
    pub async fn swap_block(
        &self,
        chain: &FxChain,
        old_fx: &FxHandle,
        new_fx_name: &str,
    ) -> SwapResult {
        // Step 1: Add the new FX to the chain.
        let new_fx = match chain.add(new_fx_name).await {
            Ok(fx) => fx,
            Err(e) => {
                return SwapResult::Failed(format!("Failed to add FX '{}': {}", new_fx_name, e))
            }
        };

        // Step 2: Immediately silence the new FX (zero all output pin mappings).
        // Save the original pin mappings so we can restore them during the swap.
        let new_fx_saved_pins = match self.silence_fx(&new_fx).await {
            Ok(saved) => saved,
            Err(e) => {
                let _ = new_fx.remove().await;
                return SwapResult::Failed(format!("Failed to silence new FX: {}", e));
            }
        };

        // Step 3: Wait for the FX to be fully loaded.
        if !self.wait_for_fx_ready(&new_fx).await {
            let _ = new_fx.remove().await;
            return SwapResult::LoadTimeout {
                fx_name: new_fx_name.to_string(),
            };
        }

        // Step 4: Atomic swap — activate new FIRST (restore its pins), then silence old.
        // Order matters: activate new FIRST so there's never a gap where neither is outputting.
        if let Err(e) = self.activate_fx(&new_fx, new_fx_saved_pins).await {
            let _ = new_fx.remove().await;
            return SwapResult::Failed(format!("Failed to activate new FX: {}", e));
        }

        let old_guid = old_fx.guid().to_string();

        if let Err(e) = self.silence_fx(old_fx).await {
            return SwapResult::Failed(format!("Failed to silence old FX: {}", e));
        }

        // Step 5: Cleanup old FX if configured.
        if self.config.remove_old {
            let _ = old_fx.remove().await;
        }

        SwapResult::Success {
            new_fx_guid: new_fx.guid().to_string(),
            old_fx_guid: old_guid,
        }
    }

    /// Swap a block using a pre-configured state chunk.
    pub async fn swap_block_with_chunk(
        &self,
        chain: &FxChain,
        old_fx: &FxHandle,
        new_fx_name: &str,
        state_chunk: &str,
    ) -> SwapResult {
        let new_fx = match chain.add(new_fx_name).await {
            Ok(fx) => fx,
            Err(e) => {
                return SwapResult::Failed(format!("Failed to add FX '{}': {}", new_fx_name, e))
            }
        };

        let new_fx_saved_pins = match self.silence_fx(&new_fx).await {
            Ok(saved) => saved,
            Err(e) => {
                let _ = new_fx.remove().await;
                return SwapResult::Failed(format!("Failed to silence new FX: {}", e));
            }
        };

        if let Err(e) = new_fx
            .set_state_chunk_encoded(state_chunk.to_string())
            .await
        {
            let _ = new_fx.remove().await;
            return SwapResult::Failed(format!("Failed to apply state chunk: {}", e));
        }

        if !self.wait_for_fx_ready(&new_fx).await {
            let _ = new_fx.remove().await;
            return SwapResult::LoadTimeout {
                fx_name: new_fx_name.to_string(),
            };
        }

        if let Err(e) = self.activate_fx(&new_fx, new_fx_saved_pins).await {
            let _ = new_fx.remove().await;
            return SwapResult::Failed(format!("Failed to activate new FX: {}", e));
        }

        let old_guid = old_fx.guid().to_string();

        if let Err(e) = self.silence_fx(old_fx).await {
            return SwapResult::Failed(format!("Failed to silence old FX: {}", e));
        }

        if self.config.remove_old {
            let _ = old_fx.remove().await;
        }

        SwapResult::Success {
            new_fx_guid: new_fx.guid().to_string(),
            old_fx_guid: old_guid,
        }
    }

    /// Swap an entire module container.
    pub async fn swap_module(
        &self,
        chain: &FxChain,
        old_fx: &FxHandle,
        old_container_id: &FxNodeId,
        new_container_chunk: &str,
    ) -> SwapResult {
        let count_before = match chain.count().await {
            Ok(c) => c,
            Err(e) => return SwapResult::Failed(format!("Failed to count FX: {}", e)),
        };

        if let Err(e) = chain.insert_chunk(new_container_chunk).await {
            return SwapResult::Failed(format!("Failed to insert container chunk: {}", e));
        }

        let new_container = match chain.by_index(count_before).await {
            Ok(Some(fx)) => fx,
            Ok(None) => {
                return SwapResult::Failed("New container not found after insert".to_string())
            }
            Err(e) => return SwapResult::Failed(format!("Failed to find new container: {}", e)),
        };
        let new_container_id = FxNodeId::container(count_before.to_string());

        if let Err(e) = self.silence_container(chain, &new_container_id).await {
            return SwapResult::Failed(format!("Failed to silence new container: {}", e));
        }

        if !self.wait_for_fx_ready(&new_container).await {
            return SwapResult::LoadTimeout {
                fx_name: "container".to_string(),
            };
        }

        if let Err(e) = self.activate_container(chain, &new_container_id).await {
            return SwapResult::Failed(format!("Failed to activate new container: {}", e));
        }

        let old_guid = old_fx.guid().to_string();

        if let Err(e) = self.silence_container(chain, old_container_id).await {
            return SwapResult::Failed(format!("Failed to silence old container: {}", e));
        }

        if self.config.remove_old {
            let _ = old_fx.remove().await;
        }

        SwapResult::Success {
            new_fx_guid: new_container.guid().to_string(),
            old_fx_guid: old_guid,
        }
    }

    /// Silence a non-container FX by zeroing all output pin mappings.
    ///
    /// Returns the saved pin mappings for later restoration via `activate_fx`.
    async fn silence_fx(&self, fx: &FxHandle) -> eyre::Result<FxPinMappings> {
        fx.silence_output().await.map_err(|e| eyre::eyre!("{:?}", e))
    }

    /// Activate a non-container FX by restoring its output pin mappings.
    async fn activate_fx(&self, fx: &FxHandle, saved: FxPinMappings) -> eyre::Result<()> {
        fx.restore_output(saved).await.map_err(|e| eyre::eyre!("{:?}", e))
    }

    /// Silence a container by setting its `container_nch_out` to 0.
    async fn silence_container(
        &self,
        chain: &FxChain,
        container_id: &FxNodeId,
    ) -> eyre::Result<()> {
        chain
            .set_container_channel_config(container_id, FxContainerChannelConfig::silent())
            .await
            .map_err(|e| eyre::eyre!("{:?}", e))
    }

    /// Activate a container by restoring its channel config to stereo.
    async fn activate_container(
        &self,
        chain: &FxChain,
        container_id: &FxNodeId,
    ) -> eyre::Result<()> {
        chain
            .set_container_channel_config(container_id, FxContainerChannelConfig::stereo())
            .await
            .map_err(|e| eyre::eyre!("{:?}", e))
    }

    /// Poll until an FX is fully loaded and ready.
    async fn wait_for_fx_ready(&self, fx: &FxHandle) -> bool {
        let deadline = tokio::time::Instant::now() + self.config.load_timeout;
        loop {
            if tokio::time::Instant::now() > deadline {
                return false;
            }
            match fx.info().await {
                Ok(info) if info.parameter_count > 0 => return true,
                _ => {}
            }
            tokio::time::sleep(self.config.poll_interval).await;
        }
    }
}

impl Default for GaplessSwapEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_config_defaults() {
        let config = SwapConfig::default();
        assert_eq!(config.load_timeout, Duration::from_secs(10));
        assert!(config.remove_old);
        assert_eq!(config.poll_interval, Duration::from_millis(100));
    }
}
