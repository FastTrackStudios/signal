//! Client API for DAW protocol
//!
//! Provides a reaper-rs style hierarchical API over the flat RPC services.
//! Supports both global singleton usage and per-instance usage for multi-host scenarios.
//!
//! # Single Host (Global)
//!
//! ```no_run
//! use daw_control::Daw;
//!
//! # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
//! // Initialize global connection
//! Daw::init(handle)?;
//!
//! // Use the global API
//! let daw = Daw::get();
//! let project = daw.current_project().await?;
//! project.transport().play().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Multiple Hosts (Instance-based)
//!
//! ```no_run
//! use daw_control::Daw;
//!
//! # async fn example(handle1: vox::Caller, handle2: vox::Caller) -> daw_control::Result<()> {
//! // Create Daw instances for each host
//! let daw1 = Daw::new(handle1);
//! let daw2 = Daw::new(handle2);
//!
//! // Use the same API on each
//! daw1.current_project().await?.transport().play().await?;
//! daw2.current_project().await?.transport().play().await?;
//! # Ok(())
//! # }
//! ```

// Re-export daw-proto types for convenience
// Note: We selectively re-export to avoid shadowing our local modules (fx, tracks, transport, etc.)
pub use daw_proto::{
    // FX types
    AddFxAtRequest,
    // Audio engine types
    AudioEngineState,
    AudioInputChannel,
    AudioInputInfo,
    AudioLatency,
    CreateContainerRequest,
    // Error types
    DawError,
    DawResult,
    // Primitives
    Duration,
    EncloseInContainerRequest,
    Fx,
    FxChainContext,
    FxContainerChannelConfig,
    FxError,
    FxEvent,
    FxLatency,
    FxNode,
    FxNodeId,
    FxNodeKind,
    FxParamModulation,
    FxParameter,
    FxRef,
    FxRoutingMode,
    FxTarget,
    FxTree,
    FxType,
    InstalledFx,
    LastTouchedFx,
    // Marker types
    Marker,
    MarkerError,
    MarkerEvent,
    MidiPosition,
    MoveFromContainerRequest,
    MoveToContainerRequest,
    MusicalPosition,
    // Transport types
    PlayState,
    Position,
    // Project types
    ProjectContext,
    ProjectEvent,
    ProjectInfo,
    RecordMode,
    // Region types
    Region,
    RegionError,
    RegionEvent,
    SetContainerChannelConfigRequest,
    SetNamedConfigRequest,
    SetParameterByNameRequest,
    SetParameterRequest,
    Tempo,
    // Tempo map types
    TempoMapError,
    TempoMapEvent,
    TempoPoint,
    TimePosition,
    TimeRange,
    TimeSignature,
    // Track types
    Track,
    TrackError,
    TrackEvent,
    TrackRef,
    TransportError,
};
// Re-export Transport struct with a different name to avoid conflict with our Transport handle
pub use daw_proto::transport::transport::Transport as TransportState;

use std::sync::Arc;

// Service clients are internal — consumers use high-level handles instead.
pub(crate) use daw_proto::ActionRegistrationClient;
pub(crate) use daw_proto::AudioAccessorsClient;
pub(crate) use daw_proto::AudioEngineClient;
pub(crate) use daw_proto::AutomationClient;
pub(crate) use daw_proto::DawFileOpsClient;
pub(crate) use daw_proto::EffectsClient;
pub(crate) use daw_proto::ExtStateClient;
pub(crate) use daw_proto::HealthClient;
pub(crate) use daw_proto::InputClient;
pub(crate) use daw_proto::ItemsClient;
pub(crate) use daw_proto::LiveMidiClient;
pub(crate) use daw_proto::MarkersClient;
pub(crate) use daw_proto::MidiClient;
pub(crate) use daw_proto::PeaksClient;
pub(crate) use daw_proto::PeaksStreamClient;
pub(crate) use daw_proto::PositionConversionClient;
pub(crate) use daw_proto::ProjectsClient;
pub(crate) use daw_proto::RegionsClient;
pub(crate) use daw_proto::RoutingClient;
pub(crate) use daw_proto::ScreensetsClient;
pub(crate) use daw_proto::SongFilesClient;
pub(crate) use daw_proto::StretchMarkersClient;
pub(crate) use daw_proto::TakesClient;
pub(crate) use daw_proto::TempoMapClient;
pub(crate) use daw_proto::TracksClient;
pub(crate) use daw_proto::TransportClient;
pub(crate) use daw_proto::TransportSyncClient;
pub(crate) use daw_proto::TransportSyncStreamClient;
pub(crate) use daw_proto::WindowGeometryClient;
pub(crate) use daw_proto::batch::BatchExecutionClient;
pub(crate) use daw_proto::diagnostics::DiagnosticsClient;
pub(crate) use daw_proto::dock_host::DockHostingClient;
pub(crate) use daw_proto::event_bus::EventBusStreamClient;
pub(crate) use daw_proto::marker::MarkersStreamClient;
pub(crate) use daw_proto::plugin_loader::PluginLoadingClient;
pub(crate) use daw_proto::region::RegionsStreamClient;
pub(crate) use daw_proto::resource::ResourcePathsClient;
pub(crate) use daw_proto::tempo_map::TempoMapStreamClient;
pub(crate) use daw_proto::toolbar::ToolbarClient;
pub(crate) use daw_proto::track::TracksStreamClient;
pub(crate) use daw_proto::transport::TransportStreamClient;
pub(crate) use daw_proto::window_manager::WindowManagerClient;
pub use vox::Caller;

pub mod error;
pub use error::{Error, Result};

pub mod lock;

mod action_registry;
mod audio_engine;
mod automation;
pub mod batch;
mod dawfile;
mod diagnostics;
mod dock_host;
mod event_bus;
mod ext_state;
mod fx;
mod input;
mod items;
mod markers;
mod midi_editor;
mod plugin_loader;
mod project;
mod regions;
mod resource;
mod routing;
mod screenset;
pub mod song_files;
mod stream;
mod tempo_map;
mod toolbar;
mod tracks;
mod transport;
pub mod transport_sync;
mod window_geometry;
mod window_manager;

pub use self::action_registry::ActionRegistry;
pub use self::audio_engine::AudioEngine;
pub use self::automation::{EnvelopeHandle, Envelopes};
pub use self::batch::{BatchBuilder, BatchExtractError, BatchResponseExt, StepHandle};
pub use self::dawfile::DawFile;
pub use self::diagnostics::Probes;
pub use self::dock_host::DockHost;
pub use self::event_bus::Events;
pub use self::ext_state::ExtState;
pub use self::fx::{FxChain, FxHandle, FxParamHandle};
pub use self::input::Input;
pub use self::items::{ItemHandle, Items, ProjectItems, TakeHandle, Takes};
pub use self::markers::Markers;
pub use self::midi_editor::MidiEditor;
pub use self::plugin_loader::PluginLoader;
pub use self::project::Project;
pub use self::regions::Regions;
pub use self::resource::Resources;
pub use self::routing::{HardwareOutputs, Receives, RouteHandle, Sends};
pub use self::screenset::Screensets;
pub use self::song_files::SongFiles;
pub use self::stream::EventStream;
pub use self::tempo_map::TempoMap;
pub use self::toolbar::Toolbar;
pub use self::tracks::{TrackHandle, Tracks};
pub use self::transport::Transport;
pub use self::transport_sync::{ProjectTransportSync, TransportLeader, TransportSync};
pub use self::window_geometry::WindowGeometry;
pub use self::window_manager::WindowManager;

architect::clients! {
    /// Service clients for a DAW connection — one generated client per
    /// service over one shared `Caller` (kept; see `Daw::caller`).
    pub struct DawClients {
        pub(crate) action_registry: ActionRegistrationClient,
        pub(crate) dock_host: DockHostingClient,
        pub(crate) transport: TransportClient,
        pub(crate) project: ProjectsClient,
        pub(crate) marker: MarkersClient,
        pub(crate) region: RegionsClient,
        pub(crate) tempo_map: TempoMapClient,
        pub(crate) track: TracksClient,
        pub(crate) fx: EffectsClient,
        pub(crate) position_conversion: PositionConversionClient,
        pub(crate) item: ItemsClient,
        pub(crate) take: TakesClient,
        pub(crate) routing: RoutingClient,
        pub(crate) screenset: ScreensetsClient,
        pub(crate) dawfile: DawFileOpsClient,
        pub(crate) window_geometry: WindowGeometryClient,
        pub(crate) window_manager: WindowManagerClient,
        pub(crate) automation: AutomationClient,
        pub(crate) live_midi: LiveMidiClient,
        pub(crate) midi: MidiClient,
        pub(crate) peaks: PeaksClient,
        pub(crate) audio_accessor: AudioAccessorsClient,
        pub(crate) audio_engine: AudioEngineClient,
        pub(crate) ext_state: ExtStateClient,
        pub(crate) health: HealthClient,
        pub(crate) input: InputClient,
        pub(crate) resource: ResourcePathsClient,
        pub(crate) toolbar: ToolbarClient,
        pub(crate) plugin_loader: PluginLoadingClient,
        pub(crate) batch: BatchExecutionClient,
        pub(crate) diagnostics: DiagnosticsClient,
        pub(crate) transport_sync: TransportSyncClient,
        pub(crate) song_files: SongFilesClient,
        pub(crate) stretch_markers: StretchMarkersClient,
        // `#[subscribe]` stream siblings — argless subscriptions;
        // filtering happens client-side in the handle wrappers.
        pub(crate) transport_stream: TransportStreamClient,
        pub(crate) track_stream: TracksStreamClient,
        pub(crate) marker_stream: MarkersStreamClient,
        pub(crate) region_stream: RegionsStreamClient,
        pub(crate) tempo_map_stream: TempoMapStreamClient,
        pub(crate) event_bus_stream: EventBusStreamClient,
        pub(crate) peaks_stream: PeaksStreamClient,
        pub(crate) transport_sync_stream: TransportSyncStreamClient,
    }
}

/// DAW API entry point
///
/// This is the main entry point for the DAW client API. It can be used in two ways:
///
/// 1. **Instance-based**: Create with `Daw::new(handle)` for multi-host scenarios
/// 2. **Global**: Initialize once with `Daw::init(handle)`, then use static methods
///
/// # Example (Instance-based)
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// project.transport().play().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Daw {
    clients: Arc<DawClients>,
}

impl Daw {
    /// Create a new Daw instance from a connection handle.
    ///
    /// Use this for multi-host scenarios where you need separate Daw instances
    /// for each host connection.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// daw.current_project().await?.transport().play().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(handle: Caller) -> Self {
        Self {
            clients: Arc::new(DawClients::new(handle)),
        }
    }

    /// Borrow the underlying `Caller` so consumers can build additional
    /// service clients (e.g. `keyflow_daw_analysis::MidiChartServiceClient`)
    /// that share the same in-process channel as the daw services.
    pub fn caller(&self) -> &Caller {
        self.clients.caller()
    }

    /// Cross-domain event-bus handle. Subscribe with a `BusFilter` to
    /// receive every enabled domain's events on one channel. Per-domain
    /// `project.tracks().subscribe()` etc. remain — use this when a
    /// consumer wants several domains at once.
    pub fn events(&self) -> Events {
        Events::new(self.clients.clone())
    }

    /// In-process diagnostic probes (latency, throughput). The probe
    /// bodies run on REAPER's main thread inside a single dispatched
    /// closure, so they measure intrinsic backend latency without
    /// per-sample RPC / IPC overhead.
    pub fn diagnostics(&self) -> Probes {
        Probes::new(self.clients.clone())
    }

    /// Transport sync: the server's sync clock and clock-stamped
    /// playhead positions, so a remote client can follow any project's
    /// transport to the sample (see [`TransportSync::leader`]).
    pub fn transport_sync(&self) -> TransportSync {
        TransportSync::new(self.clients.clone())
    }

    /// Get the current/active project
    ///
    /// Returns the project that is currently focused/active in the DAW.
    ///
    /// # Errors
    ///
    /// Returns an error if no current project is available or RPC fails.
    pub async fn current_project(&self) -> crate::Result<Project> {
        let info = self
            .clients
            .project
            .current()
            .await?
            .ok_or(Error::NoCurrentProject)?;

        Ok(Project::new(info.guid, self.clients.clone()))
    }

    /// All-projects transport event stream (architect `#[subscribe]`):
    /// discrete state changes + ~30 Hz position ticks, multiplexed as
    /// `TransportStreamEvent::{State, Position}`. Not scoped to any one
    /// project — consumers that track several (e.g. the setlist player)
    /// route each event by its `project_guid`. Served from each backend's
    /// `TransportStreamSource` hub; replaces the legacy `SynchronizationEngine`
    /// transport bridge. Drop the stream to unsubscribe.
    pub fn transport_events(
        &self,
    ) -> crate::EventStream<daw_proto::transport::TransportStreamEvent> {
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

    /// All-projects live meter stream (architect `#[subscribe]`): one
    /// [`MeterFrame`](daw_proto::MeterFrame) at ~30 Hz per project
    /// with an active engine, linear `0..1` peak + peak-hold for every
    /// track (`tracks[i]` = project track index `i`). Unfiltered —
    /// consumers that follow the "current" project route each frame by
    /// its `project_guid` (see [`Project::meter_events`] for the
    /// single-project flavor). Drop the stream to unsubscribe.
    pub fn meter_events(&self) -> crate::EventStream<daw_proto::MeterFrame> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.peaks_stream.clone();
        crate::EventStream::spawn(
            async move {
                let _ = stream.meters(raw_tx).await;
            },
            raw_rx,
            Box::new(|_| true),
        )
    }

    /// Get a specific project by GUID
    ///
    /// # Errors
    ///
    /// Returns an error if the project doesn't exist or RPC fails.
    pub async fn project(&self, guid: impl Into<String>) -> crate::Result<Project> {
        let guid = guid.into();

        // Verify the project exists
        self.clients
            .project
            .get(guid.clone())
            .await?
            .ok_or_else(|| Error::ProjectNotFound(guid.clone()))?;

        Ok(Project::new(guid, self.clients.clone()))
    }

    /// List all open projects
    pub async fn projects(&self) -> crate::Result<Vec<Project>> {
        let infos = self.clients.project.list().await?;

        Ok(infos
            .into_iter()
            .map(|info| Project::new(info.guid, self.clients.clone()))
            .collect())
    }

    /// Select/switch to a specific project by GUID
    ///
    /// Makes the specified project the currently active/focused project.
    /// This is equivalent to switching tabs in a DAW that supports multiple
    /// open projects.
    ///
    /// # Arguments
    ///
    /// * `guid` - The GUID of the project to switch to
    ///
    /// # Returns
    ///
    /// Returns the selected project on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the project doesn't exist or the switch fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(daw: &Daw) -> daw_control::Result<()> {
    /// // Switch to a specific project
    /// let project = daw.select_project("project-guid-123").await?;
    /// println!("Now on project: {}", project.guid());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn select_project(&self, guid: impl Into<String>) -> crate::Result<Project> {
        let guid = guid.into();

        let success = self.clients.project.select(guid.clone()).await?;

        if success {
            Ok(Project::new(guid, self.clients.clone()))
        } else {
            Err(Error::InvalidOperation(format!(
                "Failed to select project: {}",
                guid
            )))
        }
    }

    /// Create a new empty project tab.
    ///
    /// Returns the newly created project handle.
    pub async fn create_project(&self) -> crate::Result<Project> {
        let info =
            self.clients.project.create().await?.ok_or_else(|| {
                Error::InvalidOperation("Failed to create new project".to_string())
            })?;

        Ok(Project::new(info.guid, self.clients.clone()))
    }

    /// Open a project file (.rpp) in a new tab.
    ///
    /// Uses REAPER's `Main_openProject` API to properly load the project,
    /// avoiding proxy rendering issues that can occur with CLI arguments.
    pub async fn open_project(&self, path: impl Into<String>) -> crate::Result<Project> {
        let info = self
            .clients
            .project
            .open(path.into())
            .await?
            .ok_or_else(|| Error::InvalidOperation("Failed to open project".to_string()))?;

        Ok(Project::new(info.guid, self.clients.clone()))
    }

    /// Close a specific project tab by GUID.
    pub async fn close_project(&self, guid: impl Into<String>) -> crate::Result<()> {
        let guid = guid.into();
        let success = self.clients.project.close(guid.clone()).await?;
        if success {
            Ok(())
        } else {
            Err(Error::InvalidOperation(format!(
                "Failed to close project: {}",
                guid
            )))
        }
    }

    /// Get a project by tab slot index (0-based).
    pub async fn project_by_slot(&self, slot: u32) -> crate::Result<Project> {
        let info = self
            .clients
            .project
            .get_by_slot(slot)
            .await?
            .ok_or_else(|| Error::InvalidOperation(format!("No project at slot {}", slot)))?;

        Ok(Project::new(info.guid, self.clients.clone()))
    }

    /// Save all open projects.
    ///
    /// Equivalent to REAPER's "File: Save all projects" action (40897).
    pub async fn save_all_projects(&self) -> crate::Result<()> {
        self.clients.project.save_all().await?;
        Ok(())
    }

    // `subscribe_projects` retired with the architect::rpc port —
    // project event streaming lives on a sibling trait.

    /// Get a handle to the audio engine.
    ///
    /// The audio engine provides access to global audio device state
    /// including latency information useful for synchronization.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(daw: &Daw) -> daw_control::Result<()> {
    /// let latency = daw.audio_engine().output_latency_seconds().await?;
    /// println!("Audio output latency: {}ms", latency * 1000.0);
    /// # Ok(())
    /// # }
    /// ```
    /// Read a track's audio as samples.
    ///
    /// What a detector needs and peaks cannot give: peaks are the
    /// loudest value across a block, so a transient's EDGE is only ever
    /// located to the nearest block — which is the one measurement a
    /// tempo map is made of.
    #[must_use]
    pub fn audio(&self) -> Audio {
        Audio {
            clients: self.clients.clone(),
        }
    }

    pub fn audio_engine(&self) -> AudioEngine {
        AudioEngine::new(self.clients.clone())
    }

    /// Get a handle to the persistent key-value storage (ExtState).
    ///
    /// This provides access to REAPER's ExtState API for storing and retrieving
    /// persistent values scoped by section and key.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(daw: &Daw) -> daw_control::Result<()> {
    /// let ext = daw.ext_state();
    /// ext.set("MyExt", "theme", "dark", true).await?;
    /// let theme = ext.get("MyExt", "theme").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn ext_state(&self) -> ExtState {
        ExtState::new(self.clients.clone())
    }

    /// Access the action registry for registering custom REAPER actions.
    pub fn action_registry(&self) -> ActionRegistry {
        ActionRegistry::new(self.clients.clone())
    }

    /// Access the dock host for registering and toggling UI panels.
    ///
    /// Backed by `DockHostService` — the platform-portable trait. The
    /// REAPER+Dioxus implementation lives in `daw-reaper-dioxus`; tests
    /// can swap in `MockDockHost` (under `daw-proto`'s `test-utils`
    /// feature) without spinning up REAPER or a GPU.
    pub fn dock_host(&self) -> DockHost {
        DockHost::new(self.clients.clone())
    }

    /// Access the input interception service.
    ///
    /// Extensions use this to subscribe to keyboard/mouse events and
    /// manage key filter configuration.
    pub fn input(&self) -> Input {
        Input::new(self.clients.clone())
    }

    /// Access the toolbar management service.
    ///
    /// Extensions use this to add, update, and remove toolbar buttons.
    pub fn toolbar(&self) -> Toolbar {
        Toolbar::new(self.clients.clone())
    }

    /// Access REAPER's resource paths and color-theme loading.
    ///
    /// `resources().reload_color_theme()` is the theme-development loop:
    /// REAPER re-reads rtconfig.txt and the images on every open, so
    /// re-opening the active theme applies edits without a restart.
    pub fn resources(&self) -> Resources {
        Resources::new(self.clients.clone())
    }

    /// Access named FTS screensets.
    pub fn screensets(&self) -> Screensets {
        Screensets::new(self.clients.clone())
    }

    /// Access named workspace layouts (mode toolbars + docking).
    /// Layouts are stored in REAPER's `reaper-screensets.ini`; the
    /// service resolves names against that file and dispatches the
    /// matching `Screenset: Load #N` action when applied.
    pub fn window_manager(&self) -> WindowManager {
        WindowManager::new(self.clients.clone())
    }

    /// Access on-disk DAW project file helpers (`.RPP` summary, `.RPL`
    /// combine). Pure-function operations — no REAPER context required.
    pub fn dawfile(&self) -> DawFile {
        DawFile::new(self.clients.clone())
    }

    /// Access the window geometry service — drives nudge / resize /
    /// re-position of REAPER windows for keyboard-bindable layout edits.
    pub fn window_geometry(&self) -> WindowGeometry {
        WindowGeometry::new(self.clients.clone())
    }

    pub fn plugin_loader(&self) -> PluginLoader {
        PluginLoader::new(self.clients.clone())
    }

    /// List all installed FX plugins in the DAW.
    ///
    /// Returns every plugin known to REAPER (VST2, VST3, CLAP, AU, JS, etc.)
    /// with its display name and full identifier string.
    pub async fn installed_plugins(&self) -> crate::Result<Vec<InstalledFx>> {
        Ok(self.clients.fx.list_installed().await?)
    }

    /// Get the last touched FX parameter.
    ///
    /// Returns information about which FX parameter was most recently
    /// adjusted by the user in the DAW UI.
    pub async fn last_touched_fx(&self) -> crate::Result<Option<LastTouchedFx>> {
        Ok(self.clients.fx.last_touched().await?)
    }

    /// Show a message in the DAW's console/log window.
    pub async fn show_console_msg(&self, msg: impl Into<String>) -> crate::Result<()> {
        Ok(self.clients.health.show_console_msg(msg.into()).await?)
    }

    /// Lightweight health check — pings the DAW and returns `true` if reachable.
    ///
    /// Returns `false` if the RPC fails (connection dead). Used by the
    /// health-check loop in `daw_registry` for fast disconnect detection.
    pub async fn healthcheck(&self) -> bool {
        self.clients.health.ping().await.unwrap_or(false)
    }

    /// Execute a batch program of instructions in a single RPC call.
    ///
    /// Use [`BatchBuilder`] to construct the request.
    pub async fn execute_batch(
        &self,
        request: daw_proto::batch::BatchRequest,
    ) -> crate::Result<daw_proto::batch::BatchResponse> {
        Ok(self.clients.batch.execute(request).await?)
    }

    /// Inject a MIDI message into REAPER's virtual keyboard queue.
    ///
    /// Messages reach armed tracks whose record input is set to MIDI VKB.
    /// Use `StuffMidiTarget::VirtualMidiKeyboard` (default) for most cases.
    pub async fn stuff_midi(
        &self,
        target: daw_proto::StuffMidiTarget,
        message: daw_proto::MidiEvent,
    ) -> crate::Result<()> {
        self.clients
            .live_midi
            .stuff_midi_message(target, message)
            .await?;
        Ok(())
    }
}

// ============================================================================
// Global singleton support (for backwards compatibility / single-host usage)
//
// Native: an `OnceLock<Daw>` static (needs `Daw: Sync`).
// WASM: vox's `CallerDyn` uses the empty `MaybeSend`/`MaybeSync` markers, so
// `Daw` is not `Sync` and can't live in a static. But wasm is single-threaded
// and the browser remote talks to exactly one engine, set once at startup and
// never torn down — so a `thread_local` `OnceCell` holding a leaked
// `&'static Daw` gives the identical `Daw::get()`/`try_get()` API without any
// `Sync` bound. Both paths expose the same surface, so `daw-ui` components use
// `Daw::get()` unchanged on web.
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod global {
    use super::*;

    /// The one native `Daw`, replaceable.
    ///
    /// This was a `OnceLock` on the assumption that a process attaches to one
    /// DAW for its life. That holds inside a REAPER extension, where the DAW
    /// *is* the host — but not for an external client like session-desktop's
    /// Recording Mode, which dials a REAPER over a socket and has to survive
    /// that REAPER being quit and reopened under a new pid. A `OnceLock` left
    /// every `daw::get()` consumer (the Mixer panel, "Open in REAPER", the
    /// armed-track poll) holding a dead connection with no way to re-point it.
    ///
    /// Each `Daw` is leaked rather than dropped so `get`/`try_get` keep
    /// returning `&'static Daw` and their call sites are unchanged. What leaks
    /// is one handle per reconnect within a single run — bounded by how often
    /// the host bounces. The wasm side has always leaked for the same reason.
    static GLOBAL_DAW: std::sync::RwLock<Option<&'static Daw>> = std::sync::RwLock::new(None);

    impl Daw {
        /// Initialize the global DAW connection (for single-host usage)
        ///
        /// This must be called at startup before using the global static
        /// methods. Calling it again *replaces* the instance, which is what a
        /// reconnect to a restarted host is.
        ///
        /// # Errors
        ///
        /// Returns an error only if the lock is poisoned.
        pub fn init(handle: Caller) -> crate::Result<()> {
            let daw: &'static Daw = Box::leak(Box::new(Daw::new(handle)));
            *GLOBAL_DAW
                .write()
                .map_err(|_| Error::InvalidOperation("the global DAW lock is poisoned".into()))? =
                Some(daw);
            Ok(())
        }

        /// Forget the current instance, so `try_get` reports uninitialized
        /// again — for a client whose host has gone away.
        pub fn clear() {
            if let Ok(mut slot) = GLOBAL_DAW.write() {
                *slot = None;
            }
        }

        /// Get the global DAW instance
        ///
        /// # Panics
        ///
        /// Panics if `init()` has not been called.
        pub fn get() -> &'static Daw {
            Self::try_get().expect("DAW not initialized. Call Daw::init() first.")
        }

        /// Try to get the global DAW instance without panicking.
        ///
        /// Returns `None` if `init()` has not been called yet.
        /// Useful for gracefully handling the case where DAW is not yet initialized.
        pub fn try_get() -> Option<&'static Daw> {
            *GLOBAL_DAW.read().ok()?
        }

        /// Check if the DAW has been initialized.
        pub fn is_initialized() -> bool {
            Self::try_get().is_some()
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod global {
    use super::*;
    use std::cell::OnceCell;

    thread_local! {
        /// The one browser-side `Daw`, leaked to `'static` on first `init`
        /// (it lives for the whole page session). Single-threaded, so a
        /// `thread_local` is effectively global without needing `Sync`.
        static GLOBAL_DAW: OnceCell<&'static Daw> = const { OnceCell::new() };
    }

    impl Daw {
        /// Initialize the global DAW connection (for single-host usage).
        ///
        /// # Errors
        ///
        /// Returns an error if already initialized.
        pub fn init(handle: Caller) -> crate::Result<()> {
            GLOBAL_DAW.with(|cell| {
                cell.set(&*Box::leak(Box::new(Daw::new(handle))))
                    .map_err(|_| Error::InvalidOperation("DAW already initialized".to_string()))
            })
        }

        /// Get the global DAW instance.
        ///
        /// # Panics
        ///
        /// Panics if `init()` has not been called.
        pub fn get() -> &'static Daw {
            Self::try_get().expect("DAW not initialized. Call Daw::init() first.")
        }

        /// Try to get the global DAW instance without panicking.
        ///
        /// Returns `None` if `init()` has not been called yet.
        pub fn try_get() -> Option<&'static Daw> {
            GLOBAL_DAW.with(|cell| cell.get().copied())
        }

        /// Check if the DAW has been initialized.
        pub fn is_initialized() -> bool {
            Self::try_get().is_some()
        }
    }
}

/// Reading a track's audio, for anything that has to measure it.
///
/// An accessor is a REAPER resource and has to be given back, so this
/// opens one, reads, and closes rather than handing the caller a handle
/// to forget about. A detector reads a few seconds once; an accessor
/// left open holds the take.
#[derive(Clone)]
pub struct Audio {
    clients: std::sync::Arc<DawClients>,
}

impl Audio {
    /// Samples from a track, mixed to mono.
    ///
    /// Mono because everything that measures audio for TIMING wants one
    /// signal — a transient is a moment, and asking which channel it
    /// happened in is a question nobody has. Stereo callers can read
    /// the accessor directly.
    ///
    /// # Errors
    ///
    /// When the track cannot be resolved or the accessor cannot be
    /// created.
    pub async fn mono(
        &self,
        project: &str,
        track: daw_proto::TrackRef,
        from: f64,
        seconds: f64,
        sample_rate: f64,
    ) -> Result<Vec<f64>> {
        let context = daw_proto::ProjectContext::Project(project.to_owned());
        let accessor = self
            .clients
            .audio_accessor
            .create_track_accessor(context, track)
            .await?
            .ok_or_else(|| Error::InvalidOperation("no audio accessor for that track".into()))?;

        let channels = 2;
        let wanted = (seconds.max(0.0) * sample_rate).ceil();
        let data = self
            .clients
            .audio_accessor
            .get_samples(daw_proto::audio_accessor::GetSamplesRequest {
                accessor_id: accessor.clone(),
                sample_rate,
                num_channels: channels,
                start_time: from.max(0.0),
                num_samples: u32::try_from(wanted as i64).unwrap_or(0),
            })
            .await?;
        // Given back whatever the read did: an accessor is a resource,
        // and a detector that leaked one per press would hold every
        // take it ever looked at.
        let _ = self.clients.audio_accessor.destroy_accessor(accessor).await;

        let channels = data.num_channels.max(1) as usize;
        Ok(data
            .samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f64>() / frame.len().max(1) as f64)
            .collect())
    }
}
