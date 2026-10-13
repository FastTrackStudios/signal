//! The `daw` clap command tree + dispatcher.
//!
//! [`cli_main`] is the embeddable entry point: it takes pre-split argv
//! (element 0 is the program/subcommand name, e.g. `daw`), so both the
//! standalone `daw` binary and `fts daw <...>` mount the same surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// CLI flag form of [`crate::service::ScreensetKind`].
#[derive(Copy, Clone, Debug, ValueEnum)]
enum ScreensetKindArg {
    Window,
    TrackSet,
    SelectionSet,
}

impl From<ScreensetKindArg> for crate::service::ScreensetKind {
    fn from(arg: ScreensetKindArg) -> Self {
        match arg {
            ScreensetKindArg::Window => crate::service::ScreensetKind::Window,
            ScreensetKindArg::TrackSet => crate::service::ScreensetKind::TrackSet,
            ScreensetKindArg::SelectionSet => crate::service::ScreensetKind::SelectionSet,
        }
    }
}
use crate::cli::cli_values::{
    OnOff, ToolbarIconKindValue, TrackColor, TrackFolderDepth, TrackName,
};
use eyre::Result;
use serde_json::Value;

#[derive(Parser)]
#[command(name = "daw", about = "Live-query a running REAPER instance")]
struct Cli {
    /// Unix socket path (auto-discovers from /tmp if omitted)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::enum_variant_names)]
enum Command {
    /// Version this machine's REAPER configuration into the repo, or
    /// restore it.
    ///
    /// Only the ~350 KB that defines the setup travels: keybindings,
    /// toolbars, mouse modifiers, the ReaPack manifest and a filtered
    /// reaper.ini. ReaPack's downloads are never versioned — the
    /// registry restores them.
    ReaperConfig {
        #[command(subcommand)]
        action: ReaperConfigCommand,
    },
    /// Show project info
    Info,
    /// List all tracks
    Tracks,
    /// Show details for a specific track
    Track {
        /// Track name or index
        track: String,
    },
    /// List FX chain for a track
    Fx {
        /// Track name or index
        track: String,
    },
    /// List parameters for an FX on a track
    Params {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
    },
    /// Return the last touched FX parameter
    LastTouchedFx,
    /// Bypass an FX
    BypassFx {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
    },
    /// Enable an FX
    EnableFx {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
    },
    /// Add an FX to a track chain
    FxAdd {
        /// Track name or index
        track: String,
        /// Plugin/FX name
        name: String,
        /// Insert at zero-based FX index
        #[arg(long)]
        at: Option<u32>,
    },
    /// Remove an FX from a track chain
    FxRemove {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
    },
    /// Enable or bypass an FX
    FxEnable {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// Set enabled state
        enabled: OnOff,
    },
    /// Move an FX to a new chain index
    FxMove {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// New zero-based FX index
        index: u32,
    },
    /// Set an FX parameter by index
    FxSetParam {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// Parameter index
        param: u32,
        /// Normalized value
        value: f64,
    },
    /// Set an FX parameter by name
    FxSetParamName {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// Parameter name
        param: String,
        /// Normalized value
        value: f64,
    },
    /// Open, close, or toggle an FX UI
    FxUi {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// Action: open, close, toggle
        action: String,
    },
    /// Get or change an FX preset
    FxPreset {
        /// Track name or index
        track: String,
        /// FX name or index
        fx: String,
        /// Action: get, next, previous, set
        action: String,
        /// Preset index for action=set
        #[arg(long)]
        index: Option<u32>,
    },
    /// Show transport state
    Transport,
    /// Start playback
    Play,
    /// Stop playback
    Stop,
    /// Pause playback
    Pause,
    /// Start recording
    Record,
    /// Toggle playback
    PlayPause,
    /// Toggle loop mode
    Loop,
    /// List markers
    Markers,
    /// List regions
    Regions,
    /// List all installed plugins
    Plugins,
    /// Return loaded plugin binaries
    LoadedPlugins,
    /// Load a plugin binary into REAPER
    LoadPlugin {
        /// Plugin binary path
        path: String,
    },
    /// Check if a DAW instance is reachable
    Ping,
    /// Return generated DAW service and method catalog
    ServiceCatalog,
    /// Call any ops-covered service method: `daw call transport.play`
    /// (current project is injected when a `project` arg is omitted);
    /// `daw call marker.add --args '{"position":2.5,"name":"verse"}'`
    Call {
        /// Target as <service>.<method> (see `daw service-catalog`)
        target: String,
        /// Method arguments as JSON (named fields; defaults to {})
        #[arg(long)]
        args: Option<String>,
    },
    /// Execute one reified op from externally-tagged JSON
    Op {
        /// Op JSON: {"<Service>":{"<Method>":{..args..}}}
        op: String,
    },
    /// Execute a JSON batch program (path, or '-' for stdin) in one round trip
    Batch {
        /// Path to a BatchRequest JSON file, or '-' to read stdin
        program: String,
    },
    // -- Process & Project Management --
    /// Launch a REAPER instance
    Launch {
        /// Profile ID (e.g., "fts-reaper", "fts-tracks", "fts-dev")
        #[arg(long)]
        profile: Option<String>,
        /// Legacy alias for --profile
        #[arg(long)]
        config: Option<String>,
    },
    /// Manage multi-instance sync (daw-bridge sync runtime).
    ///
    /// Spawns/connects/monitors REAPER instances with the
    /// `FTS_SYNC_ENABLED=1` daw-bridge sync runtime enabled so they form a
    /// TCP peer mesh and propagate transport/track/marker/region/tempo
    /// changes between each other.
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// List configured DAW launch profiles
    Profiles,
    /// Quit a running REAPER instance (sends SIGTERM)
    Quit {
        /// PID of the REAPER instance to kill
        #[arg(long)]
        pid: Option<u32>,
    },
    /// List open project tabs
    Projects,
    /// Create a new project tab
    NewProject,
    /// Select an open project by GUID
    SelectProject {
        /// Project GUID
        guid: String,
    },
    /// Open a project file
    Open {
        /// Path to the .rpp project file
        path: String,
    },
    /// Close a project tab
    Close {
        /// GUID of the project to close (defaults to current)
        #[arg(long)]
        guid: Option<String>,
    },
    /// Save the current project
    Save,
    /// Save all open projects
    SaveAll,
    /// Undo in the current project
    Undo,
    /// Redo in the current project
    Redo,
    /// Run a REAPER command/action in the current project
    RunCommand {
        /// Numeric command ID or named command
        command: String,
    },
    /// Add a new track
    AddTrack {
        /// Track name (default: "New Track")
        #[arg(long)]
        name: Option<String>,
        /// Insert at index (default: append)
        #[arg(long)]
        at: Option<u32>,
    },
    /// Remove a track
    RemoveTrack {
        /// Track name or index
        track: String,
    },
    /// Mute a track
    Mute {
        /// Track name or index
        track: String,
    },
    /// Unmute a track
    Unmute {
        /// Track name or index
        track: String,
    },
    /// Solo a track
    Solo {
        /// Track name or index
        track: String,
    },
    /// Unsolo a track
    Unsolo {
        /// Track name or index
        track: String,
    },
    /// Arm a track for recording
    Arm {
        /// Track name or index
        track: String,
    },
    /// Disarm a track for recording
    Disarm {
        /// Track name or index
        track: String,
    },
    /// Select a track
    SelectTrack {
        /// Track name or index
        track: String,
    },
    /// Deselect a track
    DeselectTrack {
        /// Track name or index
        track: String,
    },
    /// Rename a track with a non-empty typed name
    TrackRename {
        /// Track name or index
        track: String,
        /// New non-empty track name
        name: TrackName,
    },
    /// Set a track color using #RRGGBB, 0xRRGGBB, decimal, or default
    TrackColor {
        /// Track name or index
        track: String,
        /// Color value: #RRGGBB, 0xRRGGBB, decimal, or default
        color: TrackColor,
    },
    /// Set REAPER folder depth using named values instead of raw integers
    TrackFolderDepth {
        /// Track name or index
        track: String,
        /// Depth: normal, folder-start, close, close:N, or an integer
        depth: TrackFolderDepth,
    },
    /// Set a track field
    TrackSet {
        /// Track name or index
        track: String,
        /// Field: muted, soloed, armed, selected, volume, pan, name, color, folder_depth, num_channels, visible_in_tcp, visible_in_mixer, parent_send
        field: String,
        /// JSON value, or a raw string if it is not valid JSON
        value: String,
    },
    /// Move a track to a new zero-based index
    TrackMove {
        /// Track name or index
        track: String,
        /// New zero-based track index
        index: u32,
    },
    /// Get or set track-scoped P_EXT state
    TrackExtState {
        /// Track name or index
        track: String,
        /// ExtState section
        section: String,
        /// ExtState key
        key: String,
        /// Optional value to set
        value: Option<String>,
    },
    /// Delete track-scoped P_EXT state
    TrackExtStateDelete {
        /// Track name or index
        track: String,
        /// ExtState section
        section: String,
        /// ExtState key
        key: String,
    },
    /// Return audio engine state and latency
    AudioEngine,
    /// Run an audio engine action
    AudioEngineDo {
        /// Action: init, quit
        action: String,
    },
    /// Execute a REAPER action by numeric ID or command name
    #[command(hide = true)]
    Action {
        /// Action ID or command name
        action_id: String,
    },
    /// Look up an action registration and command ID
    #[command(hide = true)]
    ActionLookup {
        /// Command name
        command_name: String,
    },
    /// Set a registered action toggle state
    #[command(hide = true)]
    ActionToggle {
        /// Command name
        command_name: String,
        /// Toggle state
        is_on: OnOff,
    },
    /// Work with registered REAPER/extension actions
    Actions {
        #[command(subcommand)]
        command: ActionsCommand,
    },
    /// Reload REAPER's color theme, applying edits without a restart
    ///
    /// With no path this re-opens whatever theme is active — REAPER re-reads
    /// rtconfig.txt and the image folder on every open, so that *is* a reload.
    ThemeReload {
        /// Theme to load (.ReaperTheme). Omit to reload the active one.
        path: Option<PathBuf>,
    },
    /// Show REAPER's resource, ini and active-theme paths
    ThemePaths,
    /// Return dynamic toolbar availability and tracked buttons
    Toolbar,
    /// Query live REAPER toolbar contents
    ToolbarLive {
        /// Toolbar target: main or floating toolbar 1-32. Omit to list all non-empty toolbars.
        #[arg(long)]
        target: Option<String>,
    },
    /// Parse toolbar contents from a reaper-menu.ini file
    ToolbarConfig {
        /// Path to reaper-menu.ini
        path: String,
        /// Toolbar target: main or floating toolbar 1-32. Omit to list all toolbar sections.
        #[arg(long)]
        target: Option<String>,
    },
    /// Add a registered action to a toolbar
    ToolbarAdd {
        /// REAPER command name, e.g. _FTS_GUEST_TOGGLE_EXAMPLE
        command_name: String,
        /// Toolbar button label
        label: String,
        /// Toolbar target: main or floating toolbar 1-32
        #[arg(long, default_value = "main")]
        target: String,
        /// Workflow owner used for grouped removal
        #[arg(long, default_value = "daw-cli")]
        workflow: String,
        /// Insert at zero-based toolbar position instead of appending
        #[arg(long)]
        position: Option<u32>,
        /// Icon file name or path
        #[arg(long)]
        icon: Option<String>,
        /// Icon value type
        #[arg(long, default_value = "file-name")]
        icon_kind: ToolbarIconKindValue,
        /// Toolbar flags bitmask
        #[arg(long, default_value_t = 0)]
        flags: u32,
    },
    /// Update an existing toolbar action label/icon/flags
    ToolbarUpdate {
        /// REAPER command name, e.g. _FTS_GUEST_TOGGLE_EXAMPLE
        command_name: String,
        /// Toolbar button label
        label: String,
        /// Toolbar target: main or floating toolbar 1-32
        #[arg(long, default_value = "main")]
        target: String,
        /// Workflow owner used for grouped removal
        #[arg(long, default_value = "daw-cli")]
        workflow: String,
        /// Move to a zero-based toolbar position while updating
        #[arg(long)]
        position: Option<u32>,
        /// Icon file name or path
        #[arg(long)]
        icon: Option<String>,
        /// Icon value type
        #[arg(long, default_value = "file-name")]
        icon_kind: ToolbarIconKindValue,
        /// Toolbar flags bitmask
        #[arg(long, default_value_t = 0)]
        flags: u32,
    },
    /// Remove a toolbar action
    ToolbarRemove {
        /// REAPER command name
        command_name: String,
        /// Toolbar target: main or floating toolbar 1-32
        #[arg(long, default_value = "main")]
        target: String,
    },
    /// Move a toolbar action to a zero-based position
    ToolbarMove {
        /// REAPER command name
        command_name: String,
        /// Zero-based toolbar position
        position: u32,
        /// Toolbar target: main or floating toolbar 1-32
        #[arg(long, default_value = "main")]
        target: String,
    },
    /// Set or clear a toolbar action icon
    ToolbarIcon {
        /// REAPER command name
        command_name: String,
        /// Toolbar target: main or floating toolbar 1-32
        #[arg(long, default_value = "main")]
        target: String,
        /// Icon file name or path
        #[arg(long)]
        icon: Option<String>,
        /// Icon value type
        #[arg(long, default_value = "file-name")]
        icon_kind: ToolbarIconKindValue,
        /// Clear the toolbar icon
        #[arg(long)]
        clear: bool,
    },
    /// List named FTS screensets
    Screensets,
    /// Capture the current workspace as a named FTS screenset
    ScreensetCapture {
        /// Stable screenset id
        id: String,
        /// Display name
        #[arg(long)]
        name: Option<String>,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// What to capture: window (windows + monitors + dock layout),
        /// track-set (track TCP/MCP visibility), or selection-set (track
        /// selection + time selection). Defaults to `window`.
        #[arg(long, value_enum, default_value_t = ScreensetKindArg::Window)]
        kind: ScreensetKindArg,
        /// Tag; may be passed multiple times
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// REAPER command id/name to run when applying; may be passed multiple times
        #[arg(long = "apply-action")]
        actions_on_apply: Vec<String>,
        /// Persist across REAPER restarts
        #[arg(long, default_value_t = true)]
        persist: bool,
    },
    /// Show one named FTS screenset
    ScreensetShow {
        /// Stable screenset id
        id: String,
    },
    /// Apply one named FTS screenset
    ScreensetApply {
        /// Stable screenset id
        id: String,
    },
    /// Delete one named FTS screenset
    ScreensetDelete {
        /// Stable screenset id
        id: String,
        /// Delete persistent storage too
        #[arg(long, default_value_t = true)]
        persist: bool,
    },

    // -- File Operations --
    /// Parse an RPP file and return a project summary
    RppSummary {
        /// Path to .RPP project file
        path: String,
    },
    /// List items on a track
    Items {
        /// Track name or index
        track: String,
    },
    /// Show takes on a specific item
    Takes {
        /// Track name or index
        track: String,
        /// Item zero-based index on the track
        item: u32,
    },
    /// Delete a take from an item by index
    TakeDelete {
        /// Track name or index
        track: String,
        /// Item zero-based index on the track
        item: u32,
        /// Take zero-based index on the item
        take: u32,
    },
    /// Toggle preserve-pitch (B_PPITCH) on a take
    TakePreservePitch {
        /// Track name or index
        track: String,
        /// Item zero-based index on the track
        item: u32,
        /// Take zero-based index on the item
        take: u32,
        /// Preserve pitch when changing rate
        preserve: OnOff,
    },
    /// Replace a take's source media file
    TakeSetSource {
        /// Track name or index
        track: String,
        /// Item zero-based index on the track
        item: u32,
        /// Take zero-based index on the item
        take: u32,
        /// Path to the new source file
        path: String,
    },
    /// Combine multiple RPP files into a single project
    Combine {
        /// Path to .RPL file or list of .RPP files
        input: String,
        /// Output .RPP file path (default: derived from input name)
        #[arg(short, long)]
        output: Option<String>,
        /// Gap between songs in measures (uses next song's tempo)
        #[arg(long, default_value = "0")]
        gap: u32,
    },
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Spawn N sync-enabled REAPER instances and wait for each socket.
    Spawn {
        /// How many REAPER instances to spawn. Use 2+ for cross-peer sync.
        #[arg(long, default_value = "2")]
        count: u32,
        /// DAW launch profile to use for every spawned instance.
        #[arg(long, default_value = "fts-reaper")]
        profile: String,
    },
    /// Wire every sync-ready REAPER into a direct-TCP mesh.
    Connect,
    /// Print FTS_SYNC_EXT/{status,peer_id,mesh_port,peer_count} for every
    /// locally-discovered REAPER.
    Status,
    /// Kill every locally-discovered sync-enabled REAPER.
    Stop,
}

#[derive(Subcommand)]
enum ActionsCommand {
    /// List actions from REAPER's main action list
    List {
        /// Filter: all, reaper, non-reaper, sws, fts, registered
        #[arg(long, default_value = "all")]
        filter: String,
        /// Section: main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or numeric ID
        #[arg(long, default_value = "main")]
        section: String,
        /// Case-insensitive search over description and command name
        #[arg(long)]
        query: Option<String>,
        /// Maximum number of actions to return
        #[arg(long)]
        limit: Option<u32>,
        /// Non-JSON columns: id,name,section,origin,provider,toggle,description
        #[arg(long, default_value = "id,name,section,provider,description")]
        columns: String,
    },
    /// List SWS/S&M extension actions
    Sws {
        /// Case-insensitive search over description and command name
        #[arg(long)]
        query: Option<String>,
        /// Section: main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or numeric ID
        #[arg(long, default_value = "main")]
        section: String,
        /// Maximum number of actions to return
        #[arg(long)]
        limit: Option<u32>,
        /// Non-JSON columns: id,name,section,origin,provider,toggle,description
        #[arg(long, default_value = "id,name,section,provider,description")]
        columns: String,
    },
    /// List built-in REAPER actions
    Reaper {
        /// Case-insensitive search over description and command name
        #[arg(long)]
        query: Option<String>,
        /// Section: main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or numeric ID
        #[arg(long, default_value = "main")]
        section: String,
        /// Maximum number of actions to return
        #[arg(long)]
        limit: Option<u32>,
        /// Non-JSON columns: id,name,section,origin,provider,toggle,description
        #[arg(long, default_value = "id,name,section,provider,description")]
        columns: String,
    },
    /// List extension, script, and custom actions
    NonReaper {
        /// Case-insensitive search over description and command name
        #[arg(long)]
        query: Option<String>,
        /// Section: main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or numeric ID
        #[arg(long, default_value = "main")]
        section: String,
        /// Maximum number of actions to return
        #[arg(long)]
        limit: Option<u32>,
        /// Non-JSON columns: id,name,section,origin,provider,toggle,description
        #[arg(long, default_value = "id,name,section,provider,description")]
        columns: String,
    },
    /// List actions registered by FastTrackStudio
    Registered {
        /// Case-insensitive search over description and command name
        #[arg(long)]
        query: Option<String>,
        /// Section: main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or numeric ID
        #[arg(long, default_value = "main")]
        section: String,
        /// Maximum number of actions to return
        #[arg(long)]
        limit: Option<u32>,
        /// Non-JSON columns: id,name,section,origin,provider,toggle,description
        #[arg(long, default_value = "id,name,section,provider,description")]
        columns: String,
    },
    /// List stable convenience aliases for common actions
    Aliases,
    /// Execute a stable convenience alias
    ExecAlias {
        /// Alias from `daw actions aliases`, such as transport.play_stop
        alias: String,
    },
    /// Look up an action registration and command ID
    #[command(visible_alias = "status")]
    Lookup {
        /// Command name
        command_name: String,
    },
    /// Execute a REAPER action by numeric ID or command name
    #[command(visible_alias = "run")]
    Exec {
        /// Action ID or command name
        action_id: String,
    },
    /// Set a registered action toggle state
    Toggle {
        /// Command name
        command_name: String,
        /// Toggle state: on, off, true, false, 1, 0
        state: String,
    },
    /// Return toolbar availability and tracked buttons
    Toolbar,
}

fn cli_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn bool_value(value: bool) -> Value {
    Value::Bool(value)
}

fn parse_toggle_state(state: &str) -> Result<bool> {
    match state.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => eyre::bail!("toggle state must be on/off, true/false, yes/no, or 1/0"),
    }
}

fn print_value(value: Value, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", serde_json::to_string(&value)?);
    }
    Ok(())
}

fn print_action_list(value: Value, as_json: bool, columns: &str) -> Result<()> {
    if as_json {
        return print_value(value, true);
    }

    let columns = columns
        .split(',')
        .map(|col| col.trim())
        .filter(|col| !col.is_empty())
        .collect::<Vec<_>>();
    for col in &columns {
        match *col {
            "id" | "name" | "section" | "origin" | "provider" | "toggle" | "description" => {}
            _ => eyre::bail!(
                "unknown action list column '{col}' (expected id,name,section,origin,provider,toggle,description)"
            ),
        }
    }

    let count = value.get("count").and_then(Value::as_u64).unwrap_or(0);
    let total = value
        .get("total_count")
        .and_then(Value::as_u64)
        .unwrap_or(count);
    let filter = value.get("filter").and_then(Value::as_str).unwrap_or("All");
    println!("actions: {count}/{total} filter={filter}");

    let Some(actions) = value.get("actions").and_then(Value::as_array) else {
        return Ok(());
    };

    for action in actions {
        let mut fields = Vec::new();
        for col in &columns {
            let text = match *col {
                "id" => action
                    .get("command_id")
                    .and_then(Value::as_u64)
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                "name" => action
                    .get("command_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                "origin" => action
                    .get("origin")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                "section" => action
                    .get("section_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                "provider" => action
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                "toggle" => action
                    .get("toggle_state")
                    .and_then(Value::as_bool)
                    .map(|v| if v { "on" } else { "off" })
                    .unwrap_or("")
                    .to_string(),
                "description" => action
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                _ => unreachable!(),
            };
            fields.push(text);
        }
        println!("{}", fields.join("\t"));
    }

    Ok(())
}

async fn run_sync(command: &SyncCommand, json_out: bool) -> Result<()> {
    use crate::cli::sync as s;
    match command {
        SyncCommand::Spawn { count, profile } => {
            if *count == 0 {
                eyre::bail!("--count must be >= 1");
            }
            let instances = s::spawn_sync_instances(*count, profile).await?;
            println!(
                "Spawned {} sync-enabled REAPER(s). Run `daw sync connect` to wire them up.",
                instances.len()
            );
        }
        SyncCommand::Connect => s::connect_all().await?,
        SyncCommand::Status => {
            let instances = s::status_all().await?;
            if json_out {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&s::status_json(&instances))?
                );
            } else {
                s::print_status_table(&instances);
            }
        }
        SyncCommand::Stop => {
            let killed = s::stop_all().await?;
            println!("Stopped {killed} sync-enabled REAPER(s).");
        }
    }
    Ok(())
}

/// Run the `daw` CLI from pre-split argv (element 0 is the program name).
///
/// Tracing/log setup is the caller's job — the standalone binary installs
/// an env-filter subscriber, `fts` installs its own.
pub async fn cli_main<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    // Commands that don't need an RPC connection
    match cli.command {
        Command::Launch {
            ref profile,
            ref config,
        } => {
            let selected = profile.as_deref().or(config.as_deref());
            return crate::cli::cmd_launch(selected);
        }
        Command::Profiles => {
            return crate::cli::cmd_profiles(cli.json);
        }
        Command::Sync { ref command } => {
            return run_sync(command, cli.json).await;
        }
        Command::Quit { pid } => {
            return crate::cli::cmd_quit(pid);
        }
        Command::ServiceCatalog => {
            return print_value(crate::cli::ops::service_catalog(), cli.json);
        }
        Command::Actions {
            command: ActionsCommand::Aliases,
        } => {
            return print_value(crate::cli::ops::action_aliases(), cli.json);
        }
        _ => {}
    }

    let daw = crate::cli::connect(cli.socket).await?;
    run_rpc_command(&daw, cli.command, cli.json).await
}

async fn run_rpc_command(daw: &crate::rpc::Daw, command: Command, json: bool) -> Result<()> {
    match command {
        Command::Info => crate::cli::cmd_info(daw, json).await?,
        // Purely local file work — no DAW connection needed, so this
        // arm must come before anything that assumes a live session.
        Command::ReaperConfig { ref action } => run_reaper_config(action)?,
        Command::Tracks => crate::cli::cmd_tracks(daw, json).await?,
        Command::Track { ref track } => crate::cli::cmd_track(daw, track, json).await?,
        Command::Call {
            ref target,
            ref args,
        } => print_value(
            crate::cli::ops::run_call(daw, target, args.as_deref()).await?,
            json,
        )?,
        Command::Op { ref op } => print_value(crate::cli::ops::run_op(daw, op).await?, json)?,
        Command::Batch { ref program } => {
            let text = if program == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(program)?
            };
            print_value(crate::cli::ops::run_batch(daw, &text).await?, json)?
        }
        Command::Fx { ref track } => crate::cli::cmd_fx(daw, track, json).await?,
        Command::Params { ref track, ref fx } => {
            crate::cli::cmd_params(daw, track, fx, json).await?
        }
        Command::LastTouchedFx => print_value(crate::cli::ops::last_touched_fx(daw).await?, json)?,
        Command::BypassFx { ref track, ref fx } => print_value(
            crate::cli::ops::fx_set_enabled(daw, track, fx, false).await?,
            json,
        )?,
        Command::EnableFx { ref track, ref fx } => print_value(
            crate::cli::ops::fx_set_enabled(daw, track, fx, true).await?,
            json,
        )?,
        Command::FxAdd {
            ref track,
            ref name,
            at,
        } => print_value(crate::cli::ops::fx_add(daw, track, name, at).await?, json)?,
        Command::FxRemove { ref track, ref fx } => {
            print_value(crate::cli::ops::fx_remove(daw, track, fx).await?, json)?
        }
        Command::FxEnable {
            ref track,
            ref fx,
            enabled,
        } => print_value(
            crate::cli::ops::fx_set_enabled(daw, track, fx, enabled.0).await?,
            json,
        )?,
        Command::FxMove {
            ref track,
            ref fx,
            index,
        } => print_value(crate::cli::ops::fx_move(daw, track, fx, index).await?, json)?,
        Command::FxSetParam {
            ref track,
            ref fx,
            param,
            value,
        } => print_value(
            crate::cli::ops::fx_set_param(daw, track, fx, param, value).await?,
            json,
        )?,
        Command::FxSetParamName {
            ref track,
            ref fx,
            ref param,
            value,
        } => print_value(
            crate::cli::ops::fx_set_param_by_name(daw, track, fx, param, value).await?,
            json,
        )?,
        Command::FxUi {
            ref track,
            ref fx,
            ref action,
        } => print_value(crate::cli::ops::fx_ui(daw, track, fx, action).await?, json)?,
        Command::FxPreset {
            ref track,
            ref fx,
            ref action,
            index,
        } => print_value(
            crate::cli::ops::fx_preset(daw, track, fx, action, index).await?,
            json,
        )?,
        Command::Transport => crate::cli::cmd_transport(daw, json).await?,
        Command::Play => print_value(crate::cli::ops::transport_control(daw, "play").await?, json)?,
        Command::Stop => print_value(crate::cli::ops::transport_control(daw, "stop").await?, json)?,
        Command::Pause => print_value(
            crate::cli::ops::transport_control(daw, "pause").await?,
            json,
        )?,
        Command::Record => print_value(
            crate::cli::ops::transport_control(daw, "record").await?,
            json,
        )?,
        Command::PlayPause => print_value(
            crate::cli::ops::transport_control(daw, "play_pause").await?,
            json,
        )?,
        Command::Loop => print_value(
            crate::cli::ops::transport_control(daw, "toggle_loop").await?,
            json,
        )?,

        Command::Markers => crate::cli::cmd_markers(daw, json).await?,

        Command::Regions => crate::cli::cmd_regions(daw, json).await?,

        Command::Plugins => crate::cli::cmd_plugins(daw, json).await?,
        Command::LoadedPlugins => {
            print_value(crate::cli::ops::plugin_loader_list(daw).await?, json)?
        }
        Command::LoadPlugin { ref path } => {
            print_value(crate::cli::ops::plugin_loader_load(daw, path).await?, json)?
        }
        Command::Ping => crate::cli::cmd_ping(daw).await?,
        Command::Projects => crate::cli::cmd_projects(daw, json).await?,
        Command::NewProject => print_value(crate::cli::ops::create_project(daw).await?, json)?,
        Command::SelectProject { ref guid } => {
            print_value(crate::cli::ops::select_project(daw, guid).await?, json)?
        }
        Command::Open { ref path } => crate::cli::cmd_open(daw, path, json).await?,
        Command::Close { ref guid } => crate::cli::cmd_close(daw, guid.as_deref()).await?,
        Command::Save => print_value(crate::cli::ops::save_project(daw).await?, json)?,
        Command::SaveAll => print_value(crate::cli::ops::save_all_projects(daw).await?, json)?,
        Command::Undo => print_value(crate::cli::ops::project_undo(daw).await?, json)?,
        Command::Redo => print_value(crate::cli::ops::project_redo(daw).await?, json)?,
        Command::RunCommand { ref command } => print_value(
            crate::cli::ops::project_run_command(daw, command).await?,
            json,
        )?,

        Command::AddTrack { ref name, at } => {
            crate::cli::cmd_add_track(daw, name.as_deref(), at, json).await?
        }
        Command::RemoveTrack { ref track } => crate::cli::cmd_remove_track(daw, track).await?,
        Command::Mute { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "muted", bool_value(true)).await?,
            json,
        )?,
        Command::Unmute { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "muted", bool_value(false)).await?,
            json,
        )?,
        Command::Solo { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "soloed", bool_value(true)).await?,
            json,
        )?,
        Command::Unsolo { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "soloed", bool_value(false)).await?,
            json,
        )?,
        Command::Arm { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "armed", bool_value(true)).await?,
            json,
        )?,
        Command::Disarm { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "armed", bool_value(false)).await?,
            json,
        )?,
        Command::SelectTrack { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "selected", bool_value(true)).await?,
            json,
        )?,
        Command::DeselectTrack { ref track } => print_value(
            crate::cli::ops::track_set(daw, track, "selected", bool_value(false)).await?,
            json,
        )?,
        Command::TrackRename {
            ref track,
            ref name,
        } => print_value(
            crate::cli::ops::track_rename(daw, track, &name.0).await?,
            json,
        )?,
        Command::TrackColor { ref track, color } => print_value(
            crate::cli::ops::track_set_color(daw, track, color.0).await?,
            json,
        )?,
        Command::TrackFolderDepth { ref track, depth } => print_value(
            crate::cli::ops::track_set_folder_depth(daw, track, depth.0).await?,
            json,
        )?,
        Command::TrackSet {
            ref track,
            ref field,
            ref value,
        } => print_value(
            crate::cli::ops::track_set(daw, track, field, cli_value(value)).await?,
            json,
        )?,
        Command::TrackMove { ref track, index } => {
            print_value(crate::cli::ops::track_move(daw, track, index).await?, json)?
        }
        Command::TrackExtState {
            ref track,
            ref section,
            ref key,
            ref value,
        } => print_value(
            crate::cli::ops::track_ext_state(daw, track, section, key, value.as_deref()).await?,
            json,
        )?,
        Command::TrackExtStateDelete {
            ref track,
            ref section,
            ref key,
        } => print_value(
            crate::cli::ops::track_delete_ext_state(daw, track, section, key).await?,
            json,
        )?,

        Command::AudioEngine => print_value(crate::cli::ops::audio_engine(daw).await?, json)?,
        Command::AudioEngineDo { ref action } => print_value(
            crate::cli::ops::audio_engine_control(daw, action).await?,
            json,
        )?,
        Command::Action { ref action_id } => {
            print_value(crate::cli::ops::action_execute(daw, action_id).await?, json)?
        }
        Command::ActionLookup { ref command_name } => print_value(
            crate::cli::ops::action_lookup(daw, command_name).await?,
            json,
        )?,
        Command::ActionToggle {
            ref command_name,
            is_on,
        } => print_value(
            crate::cli::ops::action_set_toggle(daw, command_name, is_on.0).await?,
            json,
        )?,
        Command::Actions { ref command } => match command {
            ActionsCommand::List {
                filter,
                section,
                query,
                limit,
                columns,
            } => print_action_list(
                crate::cli::ops::action_list(daw, filter, section, query.as_deref(), *limit)
                    .await?,
                json,
                columns,
            )?,
            ActionsCommand::Sws {
                query,
                section,
                limit,
                columns,
            } => print_action_list(
                crate::cli::ops::action_list(daw, "sws", section, query.as_deref(), *limit).await?,
                json,
                columns,
            )?,
            ActionsCommand::Reaper {
                query,
                section,
                limit,
                columns,
            } => print_action_list(
                crate::cli::ops::action_list(daw, "reaper", section, query.as_deref(), *limit)
                    .await?,
                json,
                columns,
            )?,
            ActionsCommand::NonReaper {
                query,
                section,
                limit,
                columns,
            } => print_action_list(
                crate::cli::ops::action_list(daw, "non-reaper", section, query.as_deref(), *limit)
                    .await?,
                json,
                columns,
            )?,
            ActionsCommand::Registered {
                query,
                section,
                limit,
                columns,
            } => print_action_list(
                crate::cli::ops::action_list(daw, "registered", section, query.as_deref(), *limit)
                    .await?,
                json,
                columns,
            )?,
            ActionsCommand::Aliases => print_value(crate::cli::ops::action_aliases(), json)?,
            ActionsCommand::ExecAlias { alias } => print_value(
                crate::cli::ops::action_execute_alias(daw, alias).await?,
                json,
            )?,
            ActionsCommand::Lookup { command_name } => print_value(
                crate::cli::ops::action_lookup(daw, command_name).await?,
                json,
            )?,
            ActionsCommand::Exec { action_id } => {
                print_value(crate::cli::ops::action_execute(daw, action_id).await?, json)?
            }
            ActionsCommand::Toggle {
                command_name,
                state,
            } => print_value(
                crate::cli::ops::action_set_toggle(daw, command_name, parse_toggle_state(state)?)
                    .await?,
                json,
            )?,
            ActionsCommand::Toolbar => {
                print_value(crate::cli::ops::toolbar_status(daw).await?, json)?
            }
        },
        Command::ThemeReload { ref path } => print_value(
            crate::cli::ops::theme_reload(daw, path.as_deref()).await?,
            json,
        )?,
        Command::ThemePaths => print_value(crate::cli::ops::theme_paths(daw).await?, json)?,
        Command::Toolbar => print_value(crate::cli::ops::toolbar_status(daw).await?, json)?,
        Command::ToolbarLive { target } => print_value(
            crate::cli::ops::toolbar_live(daw, target.as_deref()).await?,
            json,
        )?,
        Command::ToolbarConfig { path, target } => print_value(
            crate::cli::ops::toolbar_config(daw, &path, target.as_deref()).await?,
            json,
        )?,
        Command::ToolbarAdd {
            command_name,
            label,
            target,
            workflow,
            position,
            icon,
            icon_kind,
            flags,
        } => print_value(
            crate::cli::ops::toolbar_add(
                daw,
                &command_name,
                &label,
                &target,
                &workflow,
                position,
                icon.as_deref(),
                icon_kind.0,
                flags,
            )
            .await?,
            json,
        )?,
        Command::ToolbarUpdate {
            command_name,
            label,
            target,
            workflow,
            position,
            icon,
            icon_kind,
            flags,
        } => print_value(
            crate::cli::ops::toolbar_update(
                daw,
                &command_name,
                &label,
                &target,
                &workflow,
                position,
                icon.as_deref(),
                icon_kind.0,
                flags,
            )
            .await?,
            json,
        )?,
        Command::ToolbarRemove {
            command_name,
            target,
        } => print_value(
            crate::cli::ops::toolbar_remove(daw, &command_name, &target).await?,
            json,
        )?,
        Command::ToolbarMove {
            command_name,
            position,
            target,
        } => print_value(
            crate::cli::ops::toolbar_move(daw, &command_name, &target, position).await?,
            json,
        )?,
        Command::ToolbarIcon {
            command_name,
            target,
            icon,
            icon_kind,
            clear,
        } => print_value(
            crate::cli::ops::toolbar_icon(
                daw,
                &command_name,
                &target,
                icon.as_deref(),
                icon_kind.0,
                clear,
            )
            .await?,
            json,
        )?,
        Command::Screensets => print_value(crate::cli::ops::screenset_list(daw).await?, json)?,
        Command::ScreensetCapture {
            id,
            name,
            description,
            kind,
            tags,
            actions_on_apply,
            persist,
        } => print_value(
            crate::cli::ops::screenset_capture(
                daw,
                &id,
                name.as_deref(),
                description.as_deref(),
                kind.into(),
                tags,
                actions_on_apply,
                persist,
            )
            .await?,
            json,
        )?,
        Command::ScreensetShow { id } => {
            print_value(crate::cli::ops::screenset_show(daw, &id).await?, json)?
        }
        Command::ScreensetApply { id } => {
            print_value(crate::cli::ops::screenset_apply(daw, &id).await?, json)?
        }
        Command::ScreensetDelete { id, persist } => print_value(
            crate::cli::ops::screenset_delete(daw, &id, persist).await?,
            json,
        )?,
        Command::Combine {
            ref input,
            ref output,
            gap,
        } => crate::cli::cmd_combine(daw, input, output.as_deref(), gap).await?,
        Command::RppSummary { ref path } => {
            print_value(crate::cli::ops::rpp_summary(daw, path).await?, json)?
        }
        Command::Items { ref track } => crate::cli::cmd_items(daw, track, json).await?,
        Command::Takes { ref track, item } => crate::cli::cmd_takes(daw, track, item, json).await?,
        Command::TakeDelete {
            ref track,
            item,
            take,
        } => crate::cli::cmd_take_delete(daw, track, item, take).await?,
        Command::TakePreservePitch {
            ref track,
            item,
            take,
            preserve,
        } => crate::cli::cmd_take_preserve_pitch(daw, track, item, take, preserve.0).await?,
        Command::TakeSetSource {
            ref track,
            item,
            take,
            ref path,
        } => crate::cli::cmd_take_set_source(daw, track, item, take, path).await?,
        // Already handled above
        Command::Launch { .. }
        | Command::Profiles
        | Command::Quit { .. }
        | Command::Sync { .. }
        | Command::ServiceCatalog => unreachable!(),
    }

    Ok(())
}

#[derive(clap::Subcommand, Debug)]
enum ReaperConfigCommand {
    /// Copy a live REAPER resource dir into the repo.
    Export {
        /// REAPER resource directory (default: $HOME/fts-dev).
        resources: Option<std::path::PathBuf>,
    },
    /// Copy versioned config into a REAPER resource dir.
    ///
    /// `reaper.ini` is merged, not replaced: the target keeps its own
    /// audio device and window layout.
    Apply {
        resources: Option<std::path::PathBuf>,
    },
    /// List what differs between a resource dir and the repo.
    Diff {
        resources: Option<std::path::PathBuf>,
    },
}

fn reaper_config_dir() -> std::path::PathBuf {
    std::env::var("FTS_REAPER_CONFIG_REPO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../nix/reaper-config")
        })
}

fn default_resources() -> std::path::PathBuf {
    std::env::var("FTS_REAPER_RESOURCES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("fts-dev")
        })
}

/// Run a `reaper-config` subcommand.
fn run_reaper_config(action: &ReaperConfigCommand) -> std::io::Result<()> {
    use crate::reaper_config;
    let repo = reaper_config_dir();
    match action {
        ReaperConfigCommand::Export { resources } => {
            let live = resources.clone().unwrap_or_else(default_resources);
            let written = reaper_config::export(&live, &repo)?;
            println!("exported {} files from {}", written.len(), live.display());
            for f in &written {
                println!("  {}", f.display());
            }
        }
        ReaperConfigCommand::Apply { resources } => {
            let live = resources.clone().unwrap_or_else(default_resources);
            let written = reaper_config::apply(&repo, &live)?;
            println!("applied {} files to {}", written.len(), live.display());
        }
        ReaperConfigCommand::Diff { resources } => {
            let live = resources.clone().unwrap_or_else(default_resources);
            let changed = reaper_config::diff(&live, &repo);
            if changed.is_empty() {
                println!("in sync");
            } else {
                println!("{} files differ:", changed.len());
                for f in &changed {
                    println!("  {}", f.display());
                }
            }
        }
    }
    Ok(())
}
