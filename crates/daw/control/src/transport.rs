//! Transport handle and operations

use std::sync::Arc;

use crate::DawClients;
use crate::Result;
use daw_proto::{
    LoopRegion, PlayState, ProjectContext, TimeSignature, Transport as TransportState,
};

/// Transport handle for a specific project
///
/// This handle provides access to transport control (play, stop, record, etc.)
/// for a specific project. Like reaper-rs, it's lightweight and cheap to clone.
///
/// All methods return `Result` so callers can use `?` for clean error propagation.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let transport = project.transport();
///
/// // All methods use ? for error propagation
/// transport.play().await?;
/// transport.pause().await?;
/// transport.stop().await?;
/// transport.set_position(10.5).await?;
///
/// let pos = transport.get_position().await?;
/// let bpm = transport.get_tempo().await?;
/// transport.set_tempo(140.0).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Transport {
    project_id: String,
    clients: Arc<DawClients>,
}

impl Transport {
    /// Create a new transport handle for a project
    pub(crate) fn new(project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    // =========================================================================
    // Playback Control
    // =========================================================================

    /// Play this project's transport
    ///
    /// Starts playback from the current playhead position.
    pub async fn play(&self) -> Result<()> {
        self.clients.transport.play(self.context()).await??;
        Ok(())
    }

    /// Pause playback
    ///
    /// Maintains the playhead position so playback can be resumed.
    pub async fn pause(&self) -> Result<()> {
        self.clients.transport.pause(self.context()).await??;
        Ok(())
    }

    /// Stop playback
    ///
    /// Stops playback and typically resets to the edit cursor or start position.
    pub async fn stop(&self) -> Result<()> {
        self.clients.transport.stop(self.context()).await??;
        Ok(())
    }

    /// Toggle between play and pause
    pub async fn play_pause(&self) -> Result<()> {
        self.clients.transport.play_pause(self.context()).await??;
        Ok(())
    }

    /// Toggle between play and stop
    pub async fn play_stop(&self) -> Result<()> {
        self.clients.transport.play_stop(self.context()).await??;
        Ok(())
    }

    // =========================================================================
    // Recording Control
    // =========================================================================

    /// Start recording
    pub async fn record(&self) -> Result<()> {
        self.clients.transport.record(self.context()).await??;
        Ok(())
    }

    /// Stop recording (also stops transport)
    pub async fn stop_recording(&self) -> Result<()> {
        self.clients
            .transport
            .stop_recording(self.context())
            .await??;
        Ok(())
    }

    /// Toggle recording on/off
    pub async fn toggle_recording(&self) -> Result<()> {
        self.clients
            .transport
            .toggle_recording(self.context())
            .await??;
        Ok(())
    }

    // =========================================================================
    // Position Control
    // =========================================================================

    /// Set playhead position in seconds
    pub async fn set_position(&self, seconds: f64) -> Result<()> {
        self.clients
            .transport
            .set_position(self.context(), seconds)
            .await??;
        Ok(())
    }

    /// Get current playhead position in seconds
    pub async fn get_position(&self) -> Result<f64> {
        let pos = self.clients.transport.get_position(self.context()).await?;
        Ok(pos)
    }

    /// Go to the start of the project (position 0)
    pub async fn goto_start(&self) -> Result<()> {
        self.clients.transport.goto_start(self.context()).await??;
        Ok(())
    }

    /// Go to the end of the project
    pub async fn goto_end(&self) -> Result<()> {
        self.clients.transport.goto_end(self.context()).await??;
        Ok(())
    }

    // =========================================================================
    // State Queries
    // =========================================================================

    /// Get complete transport state
    pub async fn get_state(&self) -> Result<TransportState> {
        let state = self.clients.transport.get_state(self.context()).await?;
        Ok(state)
    }

    /// Get current play state
    pub async fn get_play_state(&self) -> Result<PlayState> {
        let state = self
            .clients
            .transport
            .get_play_state(self.context())
            .await?;
        Ok(state)
    }

    /// Check if currently playing (includes recording)
    pub async fn is_playing(&self) -> Result<bool> {
        let playing = self.clients.transport.is_playing(self.context()).await?;
        Ok(playing)
    }

    /// Check if currently recording
    pub async fn is_recording(&self) -> Result<bool> {
        let recording = self.clients.transport.is_recording(self.context()).await?;
        Ok(recording)
    }

    // =========================================================================
    // Tempo Control
    // =========================================================================

    /// Get current tempo in BPM
    pub async fn get_tempo(&self) -> Result<f64> {
        let tempo = self.clients.transport.get_tempo(self.context()).await?;
        Ok(tempo)
    }

    /// Set tempo in BPM
    pub async fn set_tempo(&self, bpm: f64) -> Result<()> {
        self.clients
            .transport
            .set_tempo(self.context(), bpm)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Loop Control
    // =========================================================================

    /// Toggle loop mode on/off
    /// Metronome / click on or off.
    pub async fn set_metronome(&self, enabled: bool) -> Result<()> {
        self.clients
            .transport
            .set_metronome(self.context(), enabled)
            .await??;
        Ok(())
    }

    /// Whether the metronome is enabled.
    pub async fn metronome_enabled(&self) -> Result<bool> {
        Ok(self
            .clients
            .transport
            .metronome_enabled(self.context())
            .await?)
    }

    pub async fn toggle_loop(&self) -> Result<()> {
        self.clients.transport.toggle_loop(self.context()).await??;
        Ok(())
    }

    /// Get loop enabled state
    pub async fn is_looping(&self) -> Result<bool> {
        let looping = self.clients.transport.is_looping(self.context()).await?;
        Ok(looping)
    }

    /// Set loop enabled state
    pub async fn set_loop(&self, enabled: bool) -> Result<()> {
        self.clients
            .transport
            .set_loop(self.context(), enabled)
            .await??;
        Ok(())
    }

    /// Get the current time selection, if one is set.
    pub async fn get_time_selection(&self) -> Result<Option<LoopRegion>> {
        let selection = self
            .clients
            .transport
            .get_time_selection(self.context())
            .await?;
        Ok(selection)
    }

    /// Set the current time selection in seconds.
    pub async fn set_time_selection(&self, start_seconds: f64, end_seconds: f64) -> Result<()> {
        self.clients
            .transport
            .set_time_selection(self.context(), start_seconds, end_seconds)
            .await??;
        Ok(())
    }

    /// Clear the current time selection.
    pub async fn clear_time_selection(&self) -> Result<()> {
        self.clients
            .transport
            .clear_time_selection(self.context())
            .await??;
        Ok(())
    }

    // =========================================================================
    // Playrate Control
    // =========================================================================

    /// Get current playback rate (1.0 = normal speed)
    pub async fn get_playrate(&self) -> Result<f64> {
        let rate = self.clients.transport.get_playrate(self.context()).await?;
        Ok(rate)
    }

    /// Set playback rate (0.25 to 4.0, where 1.0 = normal speed)
    pub async fn set_playrate(&self, rate: f64) -> Result<()> {
        self.clients
            .transport
            .set_playrate(self.context(), rate)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Time Signature
    // =========================================================================

    /// Get current time signature
    pub async fn get_time_signature(&self) -> Result<TimeSignature> {
        let ts = self
            .clients
            .transport
            .get_time_signature(self.context())
            .await?;
        Ok(ts)
    }

    // =========================================================================
    // Musical Position Control
    // =========================================================================

    // =========================================================================
    // Streaming
    // =========================================================================

    /// Transport events for **all open projects** via the architect
    /// `#[subscribe]` stream (state changes + ~30 Hz position ticks).
    /// Unlike [`subscribe`], this is not scoped to this handle's project —
    /// consumers that track multiple projects (e.g. the setlist player)
    /// route each event by its `project_guid`. Drop the stream to
    /// unsubscribe. Served from each backend's `TransportStreamSource` hub.
    pub fn events(&self) -> crate::EventStream<daw_proto::transport::TransportStreamEvent> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.transport_stream.clone();
        crate::EventStream::spawn(
            async move {
                let _ = stream.events(raw_tx).await;
            },
            raw_rx,
            Box::new(|_| true),
        )
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("project_id", &self.project_id)
            .finish()
    }
}
