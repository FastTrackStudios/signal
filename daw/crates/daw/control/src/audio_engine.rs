//! High-level handle for the audio engine.
//!
//! Wraps the raw `AudioEngineClient` so consumers never interact
//! with the RPC layer directly.

use std::sync::Arc;

use crate::DawClients;

/// Handle to the DAW's global audio engine.
///
/// Provides access to audio device state, latency information, and
/// engine lifecycle (init/quit). Unlike project-scoped handles, the
/// audio engine is global to the DAW instance.
///
/// # Example
///
/// ```no_run
/// # async fn example(daw: &daw_control::Daw) -> daw_control::Result<()> {
/// let engine = daw.audio_engine();
/// if !engine.is_running().await? {
///     engine.init().await?;
/// }
/// let latency = engine.output_latency_seconds().await?;
/// println!("Audio output latency: {:.1}ms", latency * 1000.0);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct AudioEngine {
    clients: Arc<DawClients>,
}

impl AudioEngine {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Get complete audio engine state including latency.
    pub async fn get_state(&self) -> crate::Result<daw_proto::AudioEngineState> {
        Ok(self.clients.audio_engine.state().await?)
    }

    /// Get current latency information (input/output in samples and seconds).
    pub async fn get_latency(&self) -> crate::Result<daw_proto::AudioLatency> {
        Ok(self.clients.audio_engine.latency().await?)
    }

    /// Get output latency in seconds.
    ///
    /// Directly usable for compensating visual elements to sync with audio output.
    /// Returns 0.0 if the audio engine is not running.
    pub async fn output_latency_seconds(&self) -> crate::Result<f64> {
        Ok(self.clients.audio_engine.output_latency_seconds().await?)
    }

    /// Check if the audio engine is currently running.
    pub async fn is_running(&self) -> crate::Result<bool> {
        Ok(self.clients.audio_engine.is_running().await?)
    }

    /// Enumerate available audio input channels on the current device.
    pub async fn get_audio_inputs(&self) -> crate::Result<daw_proto::AudioInputInfo> {
        Ok(self.clients.audio_engine.inputs().await?)
    }

    /// Open all audio and MIDI devices.
    ///
    /// If devices are already open this is a no-op. After calling this,
    /// `is_running()` should return `true`.
    pub async fn init(&self) -> crate::Result<()> {
        self.clients.audio_engine.init().await??;
        Ok(())
    }

    /// Close all audio and MIDI devices.
    pub async fn quit(&self) -> crate::Result<()> {
        self.clients.audio_engine.quit().await??;
        Ok(())
    }
}
