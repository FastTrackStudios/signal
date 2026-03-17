//! signal-cli library — reusable components for Signal CLI tools.
//!
//! Provides connection management, command implementations, and formatting
//! for querying and manipulating the Signal library (presets, rigs, profiles,
//! macros, songs, setlists).

use std::path::{Path, PathBuf};

use clap::Subcommand;
use daw::Daw;
use eyre::Result;
use serde_json::json;
use signal::SignalController;
use signal::profile::{Patch, PatchId};
use signal::traits::Collection;

// ============================================================================
// Connection
// ============================================================================

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

pub async fn connect_signal(db: Option<PathBuf>) -> Result<SignalController> {
    let path = match db {
        Some(p) => p,
        None => utils::paths::signal_db(),
    };
    let path_str = path
        .to_str()
        .ok_or_else(|| eyre::eyre!("Invalid DB path"))?;

    eprintln!("Opening signal DB: {}", path.display());
    let controller = signal::connect_db_seeded(path_str)
        .await
        .map_err(|e| eyre::eyre!("Failed to open signal DB: {e}"))?;
    Ok(controller)
}

// ============================================================================
// CLI Definitions
// ============================================================================

#[derive(Subcommand)]
pub enum SignalCommand {
    /// Block preset operations
    #[command(subcommand)]
    Presets(PresetsCommand),
    /// Module preset operations
    #[command(subcommand)]
    Modules(ModulesCommand),
    /// Layer operations
    #[command(subcommand)]
    Layers(LayersCommand),
    /// Engine operations
    #[command(subcommand)]
    Engines(EnginesCommand),
    /// Rig operations
    #[command(subcommand)]
    Rigs(RigsCommand),
    /// NAM model operations
    #[command(subcommand)]
    Nam(NamCommand),
    /// Profile operations
    #[command(subcommand)]
    Profiles(ProfilesCommand),
    /// Patch operations within a profile
    #[command(subcommand)]
    Patches(PatchesCommand),
    /// Macro bank operations
    #[command(subcommand)]
    Macro(MacroCommand),
    /// Search across all signal entities
    Browse {
        /// Search query
        query: String,
    },
    /// Song operations (signal-level)
    #[command(subcommand)]
    Songs(EntityCommand),
    /// Setlist operations (signal-level)
    #[command(subcommand)]
    Setlists(EntityCommand),
    /// DAW operations (connect to REAPER via socket)
    #[command(subcommand)]
    Daw(DawCommand),
    /// Load a block or module preset onto a DAW track
    Load {
        /// Type (eq, amp, drive, etc.) — matches both block types and module types
        #[arg(name = "type")]
        preset_type: String,
        /// Preset ID (block or module)
        preset_id: String,
        /// Track (index, GUID, or name)
        track: String,
        /// Snapshot ID (omit for default snapshot)
        #[arg(long)]
        snapshot: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DawCommand {
    /// List all tracks in the current project
    Tracks,
    /// List all installed plugins
    Plugins,
    /// List FX chain on a track (by index, GUID, or name)
    Fx {
        /// Track (index, GUID, or name)
        track: String,
    },
    /// Launch a REAPER instance
    Launch {
        /// Config ID (e.g., "fts-tracks", "fts-signal")
        #[arg(long)]
        config: Option<String>,
    },
    /// Quit a running REAPER instance (sends SIGTERM)
    Quit {
        /// PID of the REAPER instance to kill
        #[arg(long)]
        pid: Option<u32>,
    },
    /// List open project tabs
    Projects,
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
    /// Infer the signal chain structure from a track's FX chain
    Scan {
        /// Track name or index
        track: String,
    },
    /// Import a track's FX chain as a new rig preset
    Import {
        /// Track name or index
        track: String,
        /// Name for the new rig
        name: String,
    },
}

#[derive(Subcommand)]
pub enum PresetsCommand {
    /// List presets for a block type
    List {
        /// Block type (amp, drive, eq, reverb, delay, etc.)
        block_type: String,
    },
    /// Show preset detail + parameters
    Show {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        id: String,
    },
    /// Create a new preset
    Create {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset name
        name: String,
    },
    /// Delete a preset
    Delete {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        id: String,
    },
    /// Import presets from vendor plugin formats
    #[command(subcommand)]
    Import(ImportCommand),
    /// Capture the current state of a live REAPER FX as a new block preset
    Capture {
        /// Block type (reverb, eq, drive, etc.)
        #[arg(long, short = 't')]
        block_type: String,
        /// Name for the new preset
        #[arg(long, short = 'n')]
        name: String,
        /// Name for the default snapshot/variation (defaults to preset name)
        #[arg(long, short = 'v')]
        variation: Option<String>,
        /// Track containing the FX to capture (index, GUID, or name)
        #[arg(long)]
        track: String,
        /// FX slot index to capture (default: 0)
        #[arg(long, default_value = "0")]
        fx: u32,
    },
    /// Re-capture a live REAPER FX over an existing block preset snapshot
    Recapture {
        /// Block type (amp, reverb, drive, etc.)
        #[arg(long, short = 't')]
        block_type: String,
        /// Preset ID to overwrite
        id: String,
        /// Snapshot ID to overwrite (default: overwrites the default snapshot)
        #[arg(long, short = 's')]
        snapshot: Option<String>,
        /// Track name or index
        #[arg(long)]
        track: String,
        /// FX index (0-based)
        #[arg(long, default_value = "0")]
        fx: u32,
    },
    /// Set a single parameter value on an existing block preset snapshot
    SetParam {
        /// Block type
        #[arg(long, short = 't')]
        block_type: String,
        /// Preset ID
        id: String,
        /// Snapshot ID (default: default snapshot)
        #[arg(long, short = 's')]
        snapshot: Option<String>,
        /// Assignment: param_name=value (e.g. "Mix=0.75")
        assignment: String,
    },
}

#[derive(Subcommand)]
pub enum ImportCommand {
    /// Import FabFilter plugin presets
    Fabfilter {
        /// Plugin name (e.g. "Pro-Q 4")
        #[arg(long)]
        plugin: Option<String>,
        /// Import all discoverable FabFilter plugins
        #[arg(long)]
        all: bool,
        /// Show what would be imported without persisting
        #[arg(long)]
        dry_run: bool,
    },
    /// Import rfxchain presets from signal-library directories
    Rfxchain {
        /// Source directory containing preset subdirectories
        #[arg(long)]
        source: PathBuf,
        /// Block type (amp, eq, reverb, etc.)
        #[arg(long)]
        block_type: String,
        /// Optional plugin name override
        #[arg(long)]
        name: Option<String>,
        /// Show what would be imported without persisting
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum ModulesCommand {
    /// List module presets
    List,
    /// Show module preset detail
    Show {
        /// Module preset ID
        id: String,
    },
    /// Create a module preset from block preset references
    Create {
        /// Module type (amp, drive, time, etc.)
        #[arg(long, short = 't')]
        module_type: String,
        /// Name for the new module preset
        #[arg(long, short = 'n')]
        name: String,
        /// Block slots as block_type:preset_id pairs (e.g. amp:abc123 reverb:def456)
        #[arg(required = true)]
        blocks: Vec<String>,
    },
    /// Add a variation (snapshot) to an existing module preset
    AddVariation {
        /// Module preset ID
        id: String,
        /// Name for the new variation
        #[arg(long, short = 'n')]
        name: String,
        /// Parameter overrides: block_id:param_name=value (e.g. "reverb_0:Mix=0.75"), repeatable
        #[arg(long = "override", short = 'o')]
        overrides: Vec<String>,
    },
    /// Edit overrides or block sources on an existing module variation
    EditVariation {
        /// Module preset ID
        id: String,
        /// Snapshot ID to update
        snapshot: String,
        /// Update overrides: block_id:param_name=value, repeatable
        #[arg(long = "override", short = 'o')]
        overrides: Vec<String>,
        /// Reassign block source: block_id:block_type:preset_id, repeatable
        #[arg(long = "block", short = 'b')]
        blocks: Vec<String>,
    },
}

/// Shared CRUD subcommands for songs, setlists.
#[derive(Subcommand)]
pub enum EntityCommand {
    /// List all
    List,
    /// Show detail
    Show {
        /// Entity ID
        id: String,
    },
    /// Create new
    Create {
        /// Name
        name: String,
    },
    /// Delete
    Delete {
        /// Entity ID
        id: String,
    },
}

#[derive(Subcommand)]
pub enum LayersCommand {
    /// List all layers
    List,
    /// Show layer detail (block refs, module refs)
    Show {
        /// Layer ID
        id: String,
    },
    /// Create a new layer
    Create {
        /// Layer name
        name: String,
        /// Engine type (guitar, bass, keys, drums, vocals)
        #[arg(long, default_value = "guitar")]
        r#type: String,
    },
    /// Delete a layer
    Delete {
        /// Layer ID
        id: String,
    },
    /// Add a block preset reference to a layer's default snapshot
    AddBlock {
        /// Layer ID
        layer_id: String,
        /// Block preset ID
        preset_id: String,
        /// Snapshot variant ID (omit for default)
        #[arg(long)]
        variant: Option<String>,
    },
    /// Remove a block preset reference from a layer's default snapshot
    RemoveBlock {
        /// Layer ID
        layer_id: String,
        /// Block preset ID to remove
        preset_id: String,
    },
}

#[derive(Subcommand)]
pub enum EnginesCommand {
    /// List all engines
    List,
    /// Show engine detail (resolves layer names)
    Show {
        /// Engine ID
        id: String,
    },
    /// Create a new engine
    Create {
        /// Engine name
        name: String,
        /// Engine type (guitar, bass, keys, drums, vocals)
        #[arg(long, default_value = "guitar")]
        r#type: String,
        /// Layer IDs to include
        #[arg(long)]
        layer: Vec<String>,
    },
    /// Delete an engine
    Delete {
        /// Engine ID
        id: String,
    },
    /// Add a layer to an engine (updates all scenes)
    AddLayer {
        /// Engine ID
        engine_id: String,
        /// Layer ID
        layer_id: String,
    },
    /// Remove a layer from an engine (updates all scenes)
    RemoveLayer {
        /// Engine ID
        engine_id: String,
        /// Layer ID
        layer_id: String,
    },
}

#[derive(Subcommand)]
pub enum RigsCommand {
    /// List all rigs
    List,
    /// Show rig detail (full hierarchy: engine -> layer -> block)
    Show {
        /// Rig ID
        id: String,
    },
    /// Create a new rig
    Create {
        /// Rig name
        name: String,
    },
    /// Delete a rig
    Delete {
        /// Rig ID
        id: String,
    },
    /// Add an engine to a rig (updates all scenes)
    AddEngine {
        /// Rig ID
        rig_id: String,
        /// Engine ID
        engine_id: String,
    },
    /// Remove an engine from a rig (updates all scenes)
    RemoveEngine {
        /// Rig ID
        rig_id: String,
        /// Engine ID
        engine_id: String,
    },
    /// Open a rig in REAPER (creates [R]/[E]/[L] track hierarchy and loads all FX)
    Open {
        /// Rig ID
        id: String,
        /// Spawn and manage a dedicated REAPER instance instead of connecting to a running one
        #[arg(long)]
        own_reaper: bool,
        /// Kill REAPER after the rig loads (only meaningful with --own-reaper; useful for testing)
        #[arg(long)]
        close_after_load: bool,
    },
}

#[derive(Subcommand)]
pub enum NamCommand {
    /// List available NAM packs
    Packs {
        /// Filter by vendor
        #[arg(long)]
        vendor: Option<String>,
        /// Filter by category (amp, drive)
        #[arg(long)]
        category: Option<String>,
    },
    /// Import NAM packs as block presets
    Import {
        /// Filter by vendor
        #[arg(long)]
        vendor: Option<String>,
        /// Filter by category (amp, drive)
        #[arg(long)]
        category: Option<String>,
        /// Show what would be imported without persisting
        #[arg(long)]
        dry_run: bool,
        /// Spawn and manage a dedicated REAPER instance instead of connecting to a running one
        #[arg(long)]
        own_reaper: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfilesCommand {
    /// List all profiles
    List,
    /// Show profile detail + patches
    Show {
        /// Profile ID
        id: String,
    },
    /// Activate a profile (optionally a specific patch)
    Activate {
        /// Profile ID
        id: String,
        /// Patch ID (optional, uses default if omitted)
        patch: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum PatchesCommand {
    /// List patches in a profile
    List {
        /// Profile ID
        profile_id: String,
    },
    /// Add a patch to a profile
    Add {
        /// Profile ID
        profile_id: String,
        /// Patch name
        name: String,
    },
    /// Remove a patch from a profile
    Remove {
        /// Profile ID
        profile_id: String,
        /// Patch ID
        patch_id: String,
    },
}

#[derive(Subcommand)]
pub enum MacroCommand {
    /// Show macro bank for a block preset
    Bank {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        preset_id: String,
        /// Snapshot ID (optional)
        snapshot_id: Option<String>,
    },
    /// Set entire macro bank from JSON
    SetBank {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        preset_id: String,
        /// JSON string (or - for stdin)
        json: String,
    },
    /// Show parameter curation
    Curation {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        preset_id: String,
    },
    /// Set parameter curation from JSON
    SetCuration {
        /// Block type
        #[arg(name = "type")]
        block_type: String,
        /// Preset ID
        preset_id: String,
        /// JSON string (or - for stdin)
        json: String,
    },
}

// ============================================================================
// Dispatch
// ============================================================================

pub async fn run(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    cmd: SignalCommand,
    as_json: bool,
) -> Result<()> {
    // DAW commands get their own branch — they may or may not need the signal DB.
    if let SignalCommand::Daw(ref daw_cmd) = cmd {
        return run_daw(db, socket, daw_cmd, as_json).await;
    }

    // Load needs both signal DB and DAW connection.
    if let SignalCommand::Load {
        ref preset_type,
        ref preset_id,
        ref track,
        snapshot,
    } = cmd
    {
        return cmd_signal_load(db, socket, preset_type, preset_id, track, snapshot.as_deref(), as_json)
            .await;
    }

    // Rigs Open needs both signal DB and DAW connection.
    if let SignalCommand::Rigs(RigsCommand::Open { ref id, own_reaper, close_after_load }) = cmd {
        return cmd_rigs_open(db, socket, id, own_reaper, close_after_load).await;
    }

    // Capture needs both signal DB and DAW connection.
    if let SignalCommand::Presets(PresetsCommand::Capture {
        ref block_type, ref name, ref variation, ref track, fx
    }) = cmd {
        return cmd_presets_capture(db, socket, block_type, name, variation.as_deref(), track, fx).await;
    }

    // Recapture needs both signal DB and DAW connection.
    if let SignalCommand::Presets(PresetsCommand::Recapture {
        ref block_type, ref id, ref snapshot, ref track, fx
    }) = cmd {
        return cmd_presets_recapture(db, socket, block_type, id, snapshot.as_deref(), track, fx).await;
    }

    let signal = connect_signal(db).await?;

    match cmd {
        SignalCommand::Presets(PresetsCommand::List { ref block_type }) => {
            cmd_presets_list(&signal, block_type, as_json).await
        }
        SignalCommand::Presets(PresetsCommand::Show {
            ref block_type,
            ref id,
        }) => cmd_presets_show(&signal, block_type, id, as_json).await,
        SignalCommand::Presets(PresetsCommand::Create {
            ref block_type,
            ref name,
        }) => cmd_presets_create(&signal, block_type, name, as_json).await,
        SignalCommand::Presets(PresetsCommand::Delete {
            ref block_type,
            ref id,
        }) => cmd_presets_delete(&signal, block_type, id, as_json).await,
        SignalCommand::Presets(PresetsCommand::Import(ref import_cmd)) => {
            cmd_presets_import(&signal, import_cmd).await
        }
        SignalCommand::Presets(PresetsCommand::SetParam {
            ref block_type, ref id, ref snapshot, ref assignment,
        }) => cmd_presets_set_param(&signal, block_type, id, snapshot.as_deref(), assignment).await,

        SignalCommand::Modules(ModulesCommand::List) => {
            cmd_modules_list(&signal, as_json).await
        }
        SignalCommand::Modules(ModulesCommand::Show { ref id }) => {
            cmd_modules_show(&signal, id, as_json).await
        }
        SignalCommand::Modules(ModulesCommand::Create {
            ref module_type, ref name, ref blocks,
        }) => cmd_modules_create(&signal, module_type, name, blocks).await,
        SignalCommand::Modules(ModulesCommand::AddVariation {
            ref id, ref name, ref overrides,
        }) => cmd_modules_add_variation(&signal, id, name, overrides).await,
        SignalCommand::Modules(ModulesCommand::EditVariation {
            ref id, ref snapshot, ref overrides, ref blocks,
        }) => cmd_modules_edit_variation(&signal, id, snapshot, overrides, blocks).await,

        SignalCommand::Layers(LayersCommand::List) => cmd_layers_list(&signal, as_json).await,
        SignalCommand::Layers(LayersCommand::Show { ref id }) => {
            cmd_layers_show(&signal, id, as_json).await
        }
        SignalCommand::Layers(LayersCommand::Create { ref name, ref r#type }) => {
            cmd_layers_create(&signal, name, r#type, as_json).await
        }
        SignalCommand::Layers(LayersCommand::Delete { ref id }) => {
            cmd_layers_delete(&signal, id, as_json).await
        }
        SignalCommand::Layers(LayersCommand::AddBlock { ref layer_id, ref preset_id, ref variant }) => {
            cmd_layers_add_block(&signal, layer_id, preset_id, variant.as_deref(), as_json).await
        }
        SignalCommand::Layers(LayersCommand::RemoveBlock { ref layer_id, ref preset_id }) => {
            cmd_layers_remove_block(&signal, layer_id, preset_id, as_json).await
        }

        SignalCommand::Engines(EnginesCommand::List) => cmd_engines_list(&signal, as_json).await,
        SignalCommand::Engines(EnginesCommand::Show { ref id }) => {
            cmd_engines_show(&signal, id, as_json).await
        }
        SignalCommand::Engines(EnginesCommand::Create { ref name, ref r#type, ref layer }) => {
            cmd_engines_create(&signal, name, r#type, layer, as_json).await
        }
        SignalCommand::Engines(EnginesCommand::Delete { ref id }) => {
            cmd_engines_delete(&signal, id, as_json).await
        }
        SignalCommand::Engines(EnginesCommand::AddLayer { ref engine_id, ref layer_id }) => {
            cmd_engines_add_layer(&signal, engine_id, layer_id, as_json).await
        }
        SignalCommand::Engines(EnginesCommand::RemoveLayer { ref engine_id, ref layer_id }) => {
            cmd_engines_remove_layer(&signal, engine_id, layer_id, as_json).await
        }

        SignalCommand::Rigs(RigsCommand::List) => cmd_rigs_list(&signal, as_json).await,
        SignalCommand::Rigs(RigsCommand::Show { ref id }) => {
            cmd_rigs_show(&signal, id, as_json).await
        }
        SignalCommand::Rigs(RigsCommand::Create { ref name }) => {
            cmd_rigs_create(&signal, name, as_json).await
        }
        SignalCommand::Rigs(RigsCommand::Delete { ref id }) => {
            cmd_rigs_delete(&signal, id, as_json).await
        }
        SignalCommand::Rigs(RigsCommand::AddEngine { ref rig_id, ref engine_id }) => {
            cmd_rigs_add_engine(&signal, rig_id, engine_id, as_json).await
        }
        SignalCommand::Rigs(RigsCommand::RemoveEngine { ref rig_id, ref engine_id }) => {
            cmd_rigs_remove_engine(&signal, rig_id, engine_id, as_json).await
        }

        SignalCommand::Nam(NamCommand::Packs { ref vendor, ref category }) => {
            cmd_nam_packs(vendor.as_deref(), category.as_deref()).await
        }
        SignalCommand::Nam(NamCommand::Import { ref vendor, ref category, dry_run, own_reaper }) => {
            if dry_run {
                cmd_nam_import_dry_run(vendor.as_deref(), category.as_deref()).await
            } else if own_reaper {
                let (daw, pid, sock) =
                    daw_cli::launch_and_connect("fts-signal").await
                        .map_err(|e| eyre::eyre!("Failed to launch REAPER: {e}"))?;
                let result = cmd_nam_import(&signal, &daw, vendor.as_deref(), category.as_deref()).await;
                daw_cli::teardown_owned(pid, &sock);
                result
            } else {
                let daw = daw_cli::connect(socket.clone()).await
                    .map_err(|e| eyre::eyre!("REAPER required for nam import: {e}"))?;
                cmd_nam_import(&signal, &daw, vendor.as_deref(), category.as_deref()).await
            }
        }

        SignalCommand::Profiles(ProfilesCommand::List) => {
            cmd_profiles_list(&signal, as_json).await
        }
        SignalCommand::Profiles(ProfilesCommand::Show { ref id }) => {
            cmd_profiles_show(&signal, id, as_json).await
        }
        SignalCommand::Profiles(ProfilesCommand::Activate { ref id, ref patch }) => {
            cmd_profiles_activate(&signal, id, patch.as_deref(), as_json).await
        }

        SignalCommand::Patches(PatchesCommand::List { ref profile_id }) => {
            cmd_patches_list(&signal, profile_id, as_json).await
        }
        SignalCommand::Patches(PatchesCommand::Add {
            ref profile_id,
            ref name,
        }) => cmd_patches_add(&signal, profile_id, name, as_json).await,
        SignalCommand::Patches(PatchesCommand::Remove {
            ref profile_id,
            ref patch_id,
        }) => cmd_patches_remove(&signal, profile_id, patch_id, as_json).await,

        SignalCommand::Macro(ref _macro_cmd) => {
            eyre::bail!("Macro commands not yet implemented (Phase 2)")
        }

        SignalCommand::Browse { ref query } => cmd_browse(&signal, query, as_json).await,

        SignalCommand::Songs(EntityCommand::List) => cmd_songs_list(&signal, as_json).await,
        SignalCommand::Songs(EntityCommand::Show { ref id }) => {
            cmd_songs_show(&signal, id, as_json).await
        }
        SignalCommand::Songs(EntityCommand::Create { ref name }) => {
            cmd_songs_create(&signal, name, as_json).await
        }
        SignalCommand::Songs(EntityCommand::Delete { ref id }) => {
            cmd_songs_delete(&signal, id, as_json).await
        }

        SignalCommand::Setlists(EntityCommand::List) => {
            cmd_setlists_list(&signal, as_json).await
        }
        SignalCommand::Setlists(EntityCommand::Show { ref id }) => {
            cmd_setlists_show(&signal, id, as_json).await
        }
        SignalCommand::Setlists(EntityCommand::Create { ref name }) => {
            cmd_setlists_create(&signal, name, as_json).await
        }
        SignalCommand::Setlists(EntityCommand::Delete { ref id }) => {
            cmd_setlists_delete(&signal, id, as_json).await
        }

        // Handled above before signal DB connection.
        SignalCommand::Daw(_) | SignalCommand::Load { .. } | SignalCommand::Rigs(RigsCommand::Open { .. }) | SignalCommand::Presets(PresetsCommand::Capture { .. }) | SignalCommand::Presets(PresetsCommand::Recapture { .. }) => unreachable!(),
    }
}

// ============================================================================
// Block Type Resolution
// ============================================================================

fn parse_block_type(s: &str) -> Result<signal::BlockType> {
    signal::BlockType::from_str(s)
        .ok_or_else(|| eyre::eyre!("Unknown block type: \"{s}\". Valid types: amp, drive, eq, reverb, delay, compressor, gate, chorus, flanger, phaser, tremolo, cabinet, etc."))
}

fn parse_module_type(s: &str) -> Result<signal::ModuleType> {
    signal::ModuleType::from_str(s)
        .ok_or_else(|| eyre::eyre!("Unknown module type: \"{s}\". Valid types: amp, drive, eq, time, dynamics, modulation, special, source, volume, master, etc."))
}

// ============================================================================
// Command Implementations — Presets
// ============================================================================

async fn cmd_presets_list(
    signal: &SignalController,
    block_type: &str,
    as_json: bool,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    let presets = signal.block_presets().list(bt).await?;

    if as_json {
        let arr: Vec<_> = presets
            .iter()
            .map(|p| {
                json!({
                    "id": p.id().to_string(),
                    "name": p.name(),
                    "block_type": block_type,
                    "snapshot_count": p.snapshots().len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if presets.is_empty() {
            println!("No {} presets.", block_type);
            return Ok(());
        }
        println!("{} presets ({}):", block_type, presets.len());
        for p in &presets {
            println!(
                "  {} — {} ({} snapshots)",
                p.id(),
                p.name(),
                p.snapshots().len()
            );
        }
    }
    Ok(())
}

async fn cmd_presets_show(
    signal: &SignalController,
    block_type: &str,
    id: &str,
    as_json: bool,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    let block = signal
        .block_presets()
        .load_default(bt, id.to_string())
        .await?;

    match block {
        Some(block) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": id,
                        "block_type": block_type,
                        "block": format!("{block:?}"),
                    }))?
                );
            } else {
                println!("{} preset: {}", block_type, id);
                println!("{block:#?}");
            }
        }
        None => eyre::bail!("Preset not found: {id}"),
    }
    Ok(())
}

async fn cmd_presets_create(
    signal: &SignalController,
    block_type: &str,
    name: &str,
    as_json: bool,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    let block = signal.blocks().get(bt).await?;
    let preset = signal
        .block_presets()
        .create(name.to_string(), bt, block)
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_preset",
                "id": preset.id().to_string(),
                "name": preset.name(),
                "block_type": block_type,
                "ok": true,
            }))?
        );
    } else {
        println!("created {} preset: {} ({})", block_type, preset.name(), preset.id());
    }
    Ok(())
}

async fn cmd_presets_delete(
    signal: &SignalController,
    block_type: &str,
    id: &str,
    as_json: bool,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    signal
        .block_presets()
        .delete(bt, id.to_string())
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_preset",
                "id": id,
                "block_type": block_type,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted {} preset: {}", block_type, id);
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Capture
// ============================================================================

async fn cmd_presets_capture(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    block_type: &str,
    name: &str,
    variation: Option<&str>,
    track_arg: &str,
    fx_index: u32,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    let signal = connect_signal(db).await?;
    let daw = daw_cli::connect(socket).await
        .map_err(|e| eyre::eyre!("REAPER required for capture: {e}"))?;

    // Resolve track and FX
    let track = daw_cli::resolve_track_handle(&daw, track_arg).await?;
    let fx = track
        .fx_chain()
        .by_index(fx_index)
        .await?
        .ok_or_else(|| eyre::eyre!("No FX at index {fx_index}"))?;

    // Get plugin name, parameters, and binary state
    let info = fx.info().await?;
    let params = fx.parameters().await?;
    let state_bytes = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("FX returned no state chunk"))?;

    let snap_name = variation.unwrap_or(name);

    eprintln!(
        "Capturing: \"{}\" from \"{}\" ({} params, {} bytes)",
        info.plugin_name, track_arg, params.len(), state_bytes.len()
    );

    // Build param tuples for the ops method
    let param_tuples: Vec<(u32, String, f32)> = params
        .iter()
        .map(|p| (p.index, p.name.clone(), p.value as f32))
        .collect();

    let preset = signal
        .block_presets()
        .create_from_capture(bt, name, snap_name, &info.plugin_name, &param_tuples, state_bytes)
        .await?;

    eprintln!("Saved {} preset \"{}\" ({})", block_type, name, preset.id());
    Ok(())
}

async fn cmd_presets_recapture(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    block_type: &str,
    preset_id_str: &str,
    snapshot_arg: Option<&str>,
    track_arg: &str,
    fx_index: u32,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;
    let signal = connect_signal(db).await?;
    let daw = daw_cli::connect(socket).await
        .map_err(|e| eyre::eyre!("REAPER required for recapture: {e}"))?;

    // Resolve track and FX
    let track = daw_cli::resolve_track_handle(&daw, track_arg).await?;
    let fx = track
        .fx_chain()
        .by_index(fx_index)
        .await?
        .ok_or_else(|| eyre::eyre!("No FX at index {fx_index}"))?;

    // Get parameters and binary state
    let info = fx.info().await?;
    let params = fx.parameters().await?;
    let state_bytes = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("FX returned no state chunk"))?;

    // Find the preset
    let preset_id = signal::PresetId::from(preset_id_str.to_string());
    let preset = signal
        .block_presets()
        .list(bt)
        .await?
        .into_iter()
        .find(|p| *p.id() == preset_id)
        .ok_or_else(|| eyre::eyre!("Block preset not found: {preset_id_str}"))?;

    // Resolve snapshot ID
    let snapshot_id = match snapshot_arg {
        Some(s) => signal::SnapshotId::from(s.to_string()),
        None => preset.default_variant_id().clone(),
    };

    eprintln!(
        "Recapturing: \"{}\" from \"{}\" ({} params, {} bytes)",
        info.plugin_name, track_arg, params.len(), state_bytes.len()
    );

    let param_tuples: Vec<(u32, String, f32)> = params
        .iter()
        .map(|p| (p.index, p.name.clone(), p.value as f32))
        .collect();

    signal
        .block_presets()
        .update_snapshot_from_capture(bt, preset_id, snapshot_id, &param_tuples, state_bytes)
        .await?;

    eprintln!("Recaptured {} preset \"{}\"", block_type, preset.name());
    Ok(())
}

async fn cmd_presets_set_param(
    signal: &SignalController,
    block_type: &str,
    preset_id_str: &str,
    snapshot_arg: Option<&str>,
    assignment: &str,
) -> Result<()> {
    let bt = parse_block_type(block_type)?;

    // Parse "param_name=value"
    let (param_name, val_str) = assignment.split_once('=')
        .ok_or_else(|| eyre::eyre!(
            "Invalid assignment \"{assignment}\". Expected format: param_name=value (e.g. \"Mix=0.75\")"
        ))?;
    let value: f32 = val_str.parse()
        .map_err(|_| eyre::eyre!("Invalid value \"{val_str}\" in assignment \"{assignment}\""))?;

    // Find the preset
    let preset_id = signal::PresetId::from(preset_id_str.to_string());
    let preset = signal
        .block_presets()
        .list(bt)
        .await?
        .into_iter()
        .find(|p| *p.id() == preset_id)
        .ok_or_else(|| eyre::eyre!("Block preset not found: {preset_id_str}"))?;

    // Resolve snapshot ID
    let snapshot_id = match snapshot_arg {
        Some(s) => signal::SnapshotId::from(s.to_string()),
        None => preset.default_variant_id().clone(),
    };

    signal
        .block_presets()
        .update_snapshot_param_by_name(bt, preset_id, snapshot_id, param_name, value)
        .await?;

    eprintln!("Set {}={} on {} preset \"{}\"", param_name, value, block_type, preset.name());
    Ok(())
}

// ============================================================================
// Command Implementations — Import
// ============================================================================

async fn cmd_presets_import(signal: &SignalController, cmd: &ImportCommand) -> Result<()> {
    // Compute library root for file-based preset writing
    let library_root = utils::paths::library_dir();

    match cmd {
        ImportCommand::Fabfilter {
            plugin,
            all,
            dry_run,
        } => {
            let importer = signal::signal_import::fabfilter::FabFilterImporter::new();

            if *all {
                let plugins = importer.discover_plugins()?;
                if plugins.is_empty() {
                    println!("No FabFilter preset directories found.");
                    return Ok(());
                }
                println!("Discovered {} FabFilter plugins:", plugins.len());
                for p in &plugins {
                    let format = if p.is_text_format { "text" } else { "binary" };
                    println!(
                        "  {} — {} presets ({}, {})",
                        p.plugin_name,
                        p.preset_count,
                        p.block_type.display_name(),
                        format,
                    );
                }
                if *dry_run {
                    println!("\n[dry run] No changes made.");
                    return Ok(());
                }
                for p in &plugins {
                    let collection = importer.scan(&p.plugin_name)?;
                    let report = signal::signal_import::import_presets_with_library(
                        signal, collection, Some(&library_root),
                    ).await?;
                    println!(
                        "  Imported {}: {} snapshots",
                        report.preset_name, report.snapshots_imported
                    );
                }
            } else if let Some(name) = plugin {
                let collection = importer.scan(name)?;
                if *dry_run {
                    print!("{}", signal::signal_import::dry_run_report(&collection));
                    println!("[dry run] No changes made.");
                    return Ok(());
                }
                let report = signal::signal_import::import_presets_with_library(
                    signal, collection, Some(&library_root),
                ).await?;
                println!(
                    "Imported {}: {} snapshots",
                    report.preset_name, report.snapshots_imported
                );
            } else {
                eyre::bail!("Specify --plugin <name> or --all");
            }
            Ok(())
        }
        ImportCommand::Rfxchain {
            source,
            block_type,
            name,
            dry_run,
        } => {
            let bt = parse_block_type(block_type)?;
            let collection = signal::signal_import::rfxchain::RfxChainImporter::scan(
                source,
                bt,
                name.as_deref(),
            )?;
            if *dry_run {
                print!("{}", signal::signal_import::dry_run_report(&collection));
                println!("[dry run] No changes made.");
                return Ok(());
            }
            let report = signal::signal_import::import_presets_with_library(
                signal, collection, Some(&library_root),
            ).await?;
            println!(
                "Imported {}: {} snapshots",
                report.preset_name, report.snapshots_imported
            );
            Ok(())
        }
    }
}

// ============================================================================
// Command Implementations — Modules
// ============================================================================

async fn cmd_modules_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let modules = signal.module_presets().list().await?;

    if as_json {
        let arr: Vec<_> = modules
            .iter()
            .map(|m| {
                json!({
                    "id": m.id().to_string(),
                    "name": m.name(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if modules.is_empty() {
            println!("No module presets.");
            return Ok(());
        }
        println!("Module presets ({}):", modules.len());
        for m in &modules {
            println!("  {} — {}", m.id(), m.name());
        }
    }
    Ok(())
}

async fn cmd_modules_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let snapshot = signal
        .module_presets()
        .load_default(id.to_string())
        .await?;

    match snapshot {
        Some(snap) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": id,
                        "snapshot": format!("{snap:?}"),
                    }))?
                );
            } else {
                println!("Module preset: {}", id);
                println!("{snap:#?}");
            }
        }
        None => eyre::bail!("Module preset not found: {id}"),
    }
    Ok(())
}

async fn cmd_modules_create(
    signal: &SignalController,
    module_type: &str,
    name: &str,
    block_specs: &[String],
) -> Result<()> {
    let mt = parse_module_type(module_type)?;

    // Parse block specs: "block_type:preset_id" pairs → (BlockType, PresetId, label)
    let mut blocks = Vec::new();
    for spec in block_specs {
        let (bt_str, preset_id_str) = spec.split_once(':')
            .ok_or_else(|| eyre::eyre!(
                "Invalid block spec \"{spec}\". Expected format: block_type:preset_id (e.g. amp:abc123)"
            ))?;

        let bt = parse_block_type(bt_str)?;
        let preset_id = signal::PresetId::from(preset_id_str.to_string());

        // Look up label for display
        let label = signal.block_presets().list(bt).await?
            .into_iter()
            .find(|p| *p.id() == preset_id)
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| preset_id_str.to_string());

        eprintln!("  [{}] {} → \"{}\"", blocks.len(), bt.display_name(), label);
        blocks.push((bt, preset_id, label));
    }

    if blocks.is_empty() {
        eyre::bail!("At least one block spec is required");
    }

    let preset = signal.module_presets().create(name, mt, blocks).await?;
    eprintln!("Saved {} module preset \"{}\" ({})", module_type, name, preset.id());
    Ok(())
}

async fn cmd_modules_add_variation(
    signal: &SignalController,
    preset_id_str: &str,
    name: &str,
    override_specs: &[String],
) -> Result<()> {
    let preset_id = signal::ModulePresetId::from(preset_id_str.to_string());

    // Load the module preset
    let preset = signal
        .module_presets()
        .list()
        .await?
        .into_iter()
        .find(|p| *p.id() == preset_id)
        .ok_or_else(|| eyre::eyre!("Module preset not found: {preset_id_str}"))?;

    // Get blocks from the default snapshot
    let default_snapshot = preset.default_variant()
        .ok_or_else(|| eyre::eyre!("Module preset has no default snapshot"))?;
    let source_blocks = default_snapshot.module().blocks();

    // Parse overrides: "block_id:param_name=value"
    let mut parsed_overrides: Vec<(String, String, f32)> = Vec::new();
    for spec in override_specs {
        let (block_id, rest) = spec.split_once(':')
            .ok_or_else(|| eyre::eyre!(
                "Invalid override \"{spec}\". Expected format: block_id:param_name=value"
            ))?;
        let (param_name, val_str) = rest.split_once('=')
            .ok_or_else(|| eyre::eyre!(
                "Invalid override \"{spec}\". Expected format: block_id:param_name=value"
            ))?;
        let value: f32 = val_str.parse()
            .map_err(|_| eyre::eyre!("Invalid value \"{val_str}\" in override \"{spec}\""))?;
        parsed_overrides.push((block_id.to_string(), param_name.to_string(), value));
    }

    // Rebuild blocks with overrides applied
    let rebuilt_blocks: Vec<signal::ModuleBlock> = source_blocks
        .into_iter()
        .map(|block| {
            let overrides_for_block: Vec<signal::BlockParameterOverride> = parsed_overrides
                .iter()
                .filter(|(bid, _, _)| bid == block.id())
                .map(|(_, param, val)| signal::BlockParameterOverride::new(param, *val))
                .collect();

            let mut new_block = signal::ModuleBlock::new(
                block.id(),
                block.label(),
                block.block_type(),
                block.source().clone(),
            );
            if !overrides_for_block.is_empty() {
                new_block = new_block.with_overrides(overrides_for_block);
            }
            new_block
        })
        .collect();

    let module = signal::Module::from_blocks(rebuilt_blocks);
    let snapshot = signal::ModuleSnapshot::new(
        signal::ModuleSnapshotId::new(),
        name,
        module,
    );

    signal.module_presets().add_snapshot(preset_id, snapshot).await?;
    eprintln!("Added variation \"{}\" to module preset {}", name, preset_id_str);
    Ok(())
}

async fn cmd_modules_edit_variation(
    signal: &SignalController,
    preset_id_str: &str,
    snapshot_id_str: &str,
    override_specs: &[String],
    block_specs: &[String],
) -> Result<()> {
    let preset_id = signal::ModulePresetId::from(preset_id_str.to_string());
    let snapshot_id = signal::ModuleSnapshotId::from(snapshot_id_str.to_string());

    // Load the module preset and find the target snapshot
    let preset = signal
        .module_presets()
        .list()
        .await?
        .into_iter()
        .find(|p| *p.id() == preset_id)
        .ok_or_else(|| eyre::eyre!("Module preset not found: {preset_id_str}"))?;

    let snapshot = preset
        .variants()
        .iter()
        .find(|s| *s.id() == snapshot_id)
        .ok_or_else(|| eyre::eyre!("Snapshot not found: {snapshot_id_str}"))?;

    let source_blocks = snapshot.module().blocks();

    // Parse overrides: "block_id:param_name=value"
    let mut parsed_overrides: Vec<(String, String, f32)> = Vec::new();
    for spec in override_specs {
        let (block_id, rest) = spec.split_once(':')
            .ok_or_else(|| eyre::eyre!(
                "Invalid override \"{spec}\". Expected format: block_id:param_name=value"
            ))?;
        let (param_name, val_str) = rest.split_once('=')
            .ok_or_else(|| eyre::eyre!(
                "Invalid override \"{spec}\". Expected format: block_id:param_name=value"
            ))?;
        let value: f32 = val_str.parse()
            .map_err(|_| eyre::eyre!("Invalid value \"{val_str}\" in override \"{spec}\""))?;
        parsed_overrides.push((block_id.to_string(), param_name.to_string(), value));
    }

    // Parse block source reassignments: "block_id:block_type:preset_id"
    let mut parsed_blocks: Vec<(String, signal::BlockType, signal::PresetId)> = Vec::new();
    for spec in block_specs {
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        if parts.len() != 3 {
            eyre::bail!(
                "Invalid block spec \"{spec}\". Expected format: block_id:block_type:preset_id"
            );
        }
        let bt = parse_block_type(parts[1])?;
        let pid = signal::PresetId::from(parts[2].to_string());
        parsed_blocks.push((parts[0].to_string(), bt, pid));
    }

    // Rebuild blocks with overrides and source reassignments applied
    let rebuilt_blocks: Vec<signal::ModuleBlock> = source_blocks
        .into_iter()
        .map(|block| {
            // Check for source reassignment
            let source = if let Some((_, _, ref pid)) = parsed_blocks.iter().find(|(bid, _, _)| bid == block.id()) {
                signal::ModuleBlockSource::PresetDefault {
                    preset_id: pid.clone(),
                    saved_at_version: None,
                }
            } else {
                block.source().clone()
            };

            // Check for block type reassignment
            let block_type = if let Some((_, bt, _)) = parsed_blocks.iter().find(|(bid, _, _)| bid == block.id()) {
                *bt
            } else {
                block.block_type()
            };

            // Check for overrides
            let overrides_for_block: Vec<signal::BlockParameterOverride> = parsed_overrides
                .iter()
                .filter(|(bid, _, _)| bid == block.id())
                .map(|(_, param, val)| signal::BlockParameterOverride::new(param, *val))
                .collect();

            let mut new_block = signal::ModuleBlock::new(
                block.id(),
                block.label(),
                block_type,
                source,
            );
            if !overrides_for_block.is_empty() {
                new_block = new_block.with_overrides(overrides_for_block);
            }
            new_block
        })
        .collect();

    let module = signal::Module::from_blocks(rebuilt_blocks);
    signal
        .module_presets()
        .update_snapshot_module(preset_id, snapshot_id, module)
        .await?;

    eprintln!("Updated variation \"{}\" on module preset {}", snapshot_id_str, preset_id_str);
    Ok(())
}

// ============================================================================
// Command Implementations — Layers
// ============================================================================

async fn cmd_layers_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let layers = signal.layers().list().await?;

    if as_json {
        let arr: Vec<_> = layers
            .iter()
            .map(|l| {
                json!({
                    "id": l.id.to_string(),
                    "name": l.name,
                    "variant_count": l.variants.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if layers.is_empty() {
            println!("No layers.");
            return Ok(());
        }
        println!("Layers ({}):", layers.len());
        for l in &layers {
            println!(
                "  {} — {} ({} variants)",
                l.id, l.name, l.variants.len()
            );
        }
    }
    Ok(())
}

async fn cmd_layers_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let layer = signal.layers().load(id.to_string()).await?;
    match layer {
        Some(l) => {
            // Load default snapshot to show block_refs, module_refs
            let snapshot = signal
                .layers()
                .load_variant(l.id.clone(), l.default_variant_id.clone())
                .await?;

            if as_json {
                let mut obj = json!({
                    "id": l.id.to_string(),
                    "name": l.name,
                    "engine_type": l.engine_type.as_str(),
                    "variants": l.variants.iter().map(|v| json!({
                        "id": v.id.to_string(),
                        "name": v.name,
                    })).collect::<Vec<_>>(),
                });
                if let Some(ref snap) = snapshot {
                    obj["block_refs"] = json!(snap.block_refs.iter().map(|br| json!({
                        "collection_id": br.collection_id.to_string(),
                        "variant_id": br.variant_id.as_ref().map(|v| v.to_string()),
                    })).collect::<Vec<_>>());
                    obj["module_refs"] = json!(snap.module_refs.iter().map(|mr| json!({
                        "collection_id": mr.collection_id.to_string(),
                        "variant_id": mr.variant_id.as_ref().map(|v| v.to_string()),
                    })).collect::<Vec<_>>());
                }
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                println!("Layer: {} ({}) [{:?}]", l.name, l.id, l.engine_type);
                println!("  Variants:");
                for v in &l.variants {
                    let is_default = v.id == l.default_variant_id;
                    println!(
                        "    {} {} — {}",
                        if is_default { "*" } else { " " },
                        v.id,
                        v.name,
                    );
                }
                if let Some(snap) = snapshot {
                    if !snap.block_refs.is_empty() {
                        println!("  Block refs (default snapshot):");
                        for br in &snap.block_refs {
                            // Try to look up the preset name
                            let name = lookup_preset_name(signal, &br.collection_id).await;
                            println!("    - {} ({})", name, br.collection_id);
                        }
                    }
                    if !snap.module_refs.is_empty() {
                        println!("  Module refs (default snapshot):");
                        for mr in &snap.module_refs {
                            println!("    - {}", mr.collection_id);
                        }
                    }
                    if !snap.plugin_refs.is_empty() {
                        println!("  Plugin refs (default snapshot):");
                        for pr in &snap.plugin_refs {
                            println!("    - {:?}", pr.def);
                        }
                    }
                }
            }
        }
        None => eyre::bail!("Layer not found: {id}"),
    }
    Ok(())
}

/// Try to find a human-readable name for a block preset by checking all block types.
async fn lookup_preset_name(
    signal: &SignalController,
    preset_id: &signal::PresetId,
) -> String {
    // Try common block types
    for bt in &[
        signal::BlockType::Amp,
        signal::BlockType::Drive,
        signal::BlockType::Eq,
        signal::BlockType::Reverb,
        signal::BlockType::Delay,
        signal::BlockType::Compressor,
        signal::BlockType::Gate,
        signal::BlockType::Chorus,
        signal::BlockType::Flanger,
        signal::BlockType::Phaser,
        signal::BlockType::Trem,
        signal::BlockType::Cabinet,
        signal::BlockType::Boost,
        signal::BlockType::Saturator,
        signal::BlockType::Limiter,
        signal::BlockType::Volume,
    ] {
        if let Ok(presets) = signal.block_presets().list(*bt).await {
            if let Some(p) = presets.iter().find(|p| p.id() == preset_id) {
                return p.name().to_string();
            }
        }
    }
    preset_id.to_string()
}

fn parse_engine_type(s: &str) -> Result<signal::EngineType> {
    match s.to_lowercase().as_str() {
        "guitar" => Ok(signal::EngineType::Guitar),
        "bass" => Ok(signal::EngineType::Bass),
        "vocal" | "vocals" => Ok(signal::EngineType::Vocal),
        "keys" => Ok(signal::EngineType::Keys),
        "synth" => Ok(signal::EngineType::Synth),
        "organ" => Ok(signal::EngineType::Organ),
        "pad" => Ok(signal::EngineType::Pad),
        _ => eyre::bail!("Unknown engine type: \"{s}\". Valid: guitar, bass, vocals, keys, synth, organ, pad"),
    }
}

async fn cmd_layers_create(
    signal: &SignalController,
    name: &str,
    type_str: &str,
    as_json: bool,
) -> Result<()> {
    let engine_type = parse_engine_type(type_str)?;
    let layer = signal
        .layers()
        .create(name.to_string(), engine_type)
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_layer",
                "id": layer.id.to_string(),
                "name": layer.name,
                "ok": true,
            }))?
        );
    } else {
        println!("created layer: {} ({})", layer.name, layer.id);
    }
    Ok(())
}

async fn cmd_layers_delete(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    signal.layers().delete(id.to_string()).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_layer",
                "id": id,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted layer: {}", id);
    }
    Ok(())
}

async fn cmd_layers_add_block(
    signal: &SignalController,
    layer_id: &str,
    preset_id: &str,
    variant_id: Option<&str>,
    as_json: bool,
) -> Result<()> {
    let lid = signal::layer::LayerId::from(layer_id.to_string());
    let pid = signal::PresetId::from(preset_id.to_string());

    let layer = signal
        .layers()
        .load(lid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Layer not found: {layer_id}"))?;

    let mut snapshot = signal
        .layers()
        .load_variant(lid.clone(), layer.default_variant_id.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Default snapshot not found for layer {layer_id}"))?;

    let block_ref = if let Some(vid) = variant_id {
        signal::layer::BlockRef::new(pid.clone())
            .with_variant(signal::SnapshotId::from(vid.to_string()))
    } else {
        signal::layer::BlockRef::new(pid.clone())
    };
    snapshot.block_refs.push(block_ref);

    signal.layers().save_variant(lid.clone(), snapshot).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "add_block",
                "layer_id": layer_id,
                "preset_id": preset_id,
                "ok": true,
            }))?
        );
    } else {
        println!("added block {} to layer {}", preset_id, layer.name);
    }
    Ok(())
}

async fn cmd_layers_remove_block(
    signal: &SignalController,
    layer_id: &str,
    preset_id: &str,
    as_json: bool,
) -> Result<()> {
    let lid = signal::layer::LayerId::from(layer_id.to_string());
    let pid = signal::PresetId::from(preset_id.to_string());

    let layer = signal
        .layers()
        .load(lid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Layer not found: {layer_id}"))?;

    let mut snapshot = signal
        .layers()
        .load_variant(lid.clone(), layer.default_variant_id.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Default snapshot not found for layer {layer_id}"))?;

    let before = snapshot.block_refs.len();
    snapshot.block_refs.retain(|br| br.collection_id != pid);
    let removed = before - snapshot.block_refs.len();

    if removed == 0 {
        eyre::bail!("Block {} not found in layer {}", preset_id, layer.name);
    }

    signal.layers().save_variant(lid.clone(), snapshot).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "remove_block",
                "layer_id": layer_id,
                "preset_id": preset_id,
                "ok": true,
            }))?
        );
    } else {
        println!("removed block {} from layer {}", preset_id, layer.name);
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Engines
// ============================================================================

async fn cmd_engines_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let engines = signal.engines().list().await?;

    if as_json {
        let arr: Vec<_> = engines
            .iter()
            .map(|e| {
                json!({
                    "id": e.id.to_string(),
                    "name": e.name,
                    "variant_count": e.variants.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if engines.is_empty() {
            println!("No engines.");
            return Ok(());
        }
        println!("Engines ({}):", engines.len());
        for e in &engines {
            println!(
                "  {} — {} ({} scenes)",
                e.id, e.name, e.variants.len()
            );
        }
    }
    Ok(())
}

async fn cmd_engines_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let engine = signal.engines().load(id.to_string()).await?;
    match engine {
        Some(e) => {
            // Resolve layer names
            let mut layer_info = Vec::new();
            for lid in &e.layer_ids {
                let name = if let Some(l) = signal.layers().load(lid.clone()).await? {
                    l.name
                } else {
                    format!("(missing: {})", lid)
                };
                layer_info.push((lid.to_string(), name));
            }

            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": e.id.to_string(),
                        "name": e.name,
                        "engine_type": e.engine_type.as_str(),
                        "layers": layer_info.iter().map(|(id, name)| json!({
                            "id": id,
                            "name": name,
                        })).collect::<Vec<_>>(),
                        "scenes": e.variants.iter().map(|v| json!({
                            "id": v.id.to_string(),
                            "name": v.name,
                            "layer_selections": v.layer_selections.iter().map(|s| json!({
                                "layer_id": s.layer_id.to_string(),
                                "variant_id": s.variant_id.to_string(),
                            })).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Engine: {} ({}) [{}]", e.name, e.id, e.engine_type.as_str());
                println!("  Layers:");
                for (lid, name) in &layer_info {
                    println!("    - {} ({})", name, lid);
                }
                for v in &e.variants {
                    let is_default = v.id == e.default_variant_id;
                    println!(
                        "  {} Scene: {} — {}",
                        if is_default { "*" } else { " " },
                        v.name,
                        v.id,
                    );
                    for sel in &v.layer_selections {
                        println!("      Layer {} → snapshot {}", sel.layer_id, sel.variant_id);
                    }
                }
            }
        }
        None => eyre::bail!("Engine not found: {id}"),
    }
    Ok(())
}

async fn cmd_engines_create(
    signal: &SignalController,
    name: &str,
    type_str: &str,
    layer_ids_str: &[String],
    as_json: bool,
) -> Result<()> {
    let engine_type = parse_engine_type(type_str)?;

    // Parse and validate layer IDs
    let mut layer_ids = Vec::new();
    let mut layer_selections = Vec::new();
    for lid_str in layer_ids_str {
        let lid = signal::layer::LayerId::from(lid_str.to_string());
        let layer = signal
            .layers()
            .load(lid.clone())
            .await?
            .ok_or_else(|| eyre::eyre!("Layer not found: {lid_str}"))?;
        layer_selections.push(signal::engine::LayerSelection::new(
            lid.clone(),
            layer.default_variant_id.clone(),
        ));
        layer_ids.push(lid);
    }

    let mut engine = signal
        .engines()
        .create(name.to_string(), engine_type, layer_ids)
        .await?;

    // Wire layer selections into the default scene
    if !layer_selections.is_empty() {
        for scene in &mut engine.variants {
            for sel in &layer_selections {
                if !scene.layer_selections.iter().any(|s| s.layer_id == sel.layer_id) {
                    scene.layer_selections.push(sel.clone());
                }
            }
        }
        engine = signal.engines().save(engine).await?;
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_engine",
                "id": engine.id.to_string(),
                "name": engine.name,
                "ok": true,
            }))?
        );
    } else {
        println!("created engine: {} ({})", engine.name, engine.id);
    }
    Ok(())
}

async fn cmd_engines_delete(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    signal.engines().delete(id.to_string()).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_engine",
                "id": id,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted engine: {}", id);
    }
    Ok(())
}

async fn cmd_engines_add_layer(
    signal: &SignalController,
    engine_id: &str,
    layer_id: &str,
    as_json: bool,
) -> Result<()> {
    let eid = signal::engine::EngineId::from(engine_id.to_string());
    let lid = signal::layer::LayerId::from(layer_id.to_string());

    let mut engine = signal
        .engines()
        .load(eid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Engine not found: {engine_id}"))?;
    let layer = signal
        .layers()
        .load(lid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Layer not found: {layer_id}"))?;

    engine.layer_ids.push(lid.clone());

    let selection = signal::engine::LayerSelection::new(
        lid,
        layer.default_variant_id.clone(),
    );
    for scene in &mut engine.variants {
        scene.layer_selections.push(selection.clone());
    }

    signal.engines().save(engine).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "add_layer",
                "engine_id": engine_id,
                "layer_id": layer_id,
                "ok": true,
            }))?
        );
    } else {
        println!("added layer {} to engine {}", layer.name, engine_id);
    }
    Ok(())
}

async fn cmd_engines_remove_layer(
    signal: &SignalController,
    engine_id: &str,
    layer_id: &str,
    as_json: bool,
) -> Result<()> {
    let eid = signal::engine::EngineId::from(engine_id.to_string());
    let lid = signal::layer::LayerId::from(layer_id.to_string());

    let mut engine = signal
        .engines()
        .load(eid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Engine not found: {engine_id}"))?;

    let before = engine.layer_ids.len();
    engine.layer_ids.retain(|l| *l != lid);
    if engine.layer_ids.len() == before {
        eyre::bail!("Layer {} not found in engine {}", layer_id, engine.name);
    }

    for scene in &mut engine.variants {
        scene.layer_selections.retain(|s| s.layer_id != lid);
    }

    signal.engines().save(engine).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "remove_layer",
                "engine_id": engine_id,
                "layer_id": layer_id,
                "ok": true,
            }))?
        );
    } else {
        println!("removed layer {} from engine {}", layer_id, engine_id);
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Rigs
// ============================================================================

async fn cmd_rigs_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let rigs = signal.rigs().list().await?;

    if as_json {
        let arr: Vec<_> = rigs
            .iter()
            .map(|r| {
                json!({
                    "id": r.id.to_string(),
                    "name": r.name,
                    "engine_count": r.engine_ids.len(),
                    "scene_count": r.variants.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if rigs.is_empty() {
            println!("No rigs.");
            return Ok(());
        }
        println!("Rigs ({}):", rigs.len());
        for r in &rigs {
            println!(
                "  {} — {} ({} engines, {} scenes)",
                r.id, r.name, r.engine_ids.len(), r.variants.len()
            );
        }
    }
    Ok(())
}

async fn cmd_rigs_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let rig = signal.rigs().load(id.to_string()).await?;
    match rig {
        Some(r) => {
            if as_json {
                let mut engines_json = Vec::new();
                for eid in &r.engine_ids {
                    if let Some(engine) = signal.engines().load(eid.clone()).await? {
                        let mut layers_json = Vec::new();
                        for lid in &engine.layer_ids {
                            if let Some(layer) = signal.layers().load(lid.clone()).await? {
                                let snap = signal
                                    .layers()
                                    .load_variant(lid.clone(), layer.default_variant_id.clone())
                                    .await?;
                                let blocks: Vec<_> = if let Some(ref s) = snap {
                                    s.block_refs.iter().map(|br| json!({
                                        "collection_id": br.collection_id.to_string(),
                                    })).collect()
                                } else {
                                    vec![]
                                };
                                layers_json.push(json!({
                                    "id": lid.to_string(),
                                    "name": layer.name,
                                    "block_refs": blocks,
                                }));
                            }
                        }
                        engines_json.push(json!({
                            "id": eid.to_string(),
                            "name": engine.name,
                            "layers": layers_json,
                        }));
                    }
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": r.id.to_string(),
                        "name": r.name,
                        "engines": engines_json,
                        "scenes": r.variants.iter().map(|v| json!({
                            "id": v.id.to_string(),
                            "name": v.name,
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Rig: {} ({})", r.name, r.id);
                for v in &r.variants {
                    let is_default = v.id == r.default_variant_id;
                    println!(
                        "  {} Scene: {}",
                        if is_default { "*" } else { " " },
                        v.name,
                    );
                    for es in &v.engine_selections {
                        if let Some(engine) = signal.engines().load(es.engine_id.clone()).await? {
                            println!("    Engine: {} (scene: {})", engine.name, es.variant_id);
                            // Find the engine scene to get layer selections
                            if let Some(scene) = engine.variants.iter().find(|s| s.id == es.variant_id) {
                                for ls in &scene.layer_selections {
                                    if let Some(layer) = signal.layers().load(ls.layer_id.clone()).await? {
                                        println!("      Layer: {} (snapshot: {})", layer.name, ls.variant_id);
                                        // Load the layer snapshot to show block refs
                                        if let Some(snap) = signal.layers().load_variant(ls.layer_id.clone(), ls.variant_id.clone()).await? {
                                            for br in &snap.block_refs {
                                                let name = lookup_preset_name(signal, &br.collection_id).await;
                                                println!("        Block: {}", name);
                                            }
                                        }
                                    } else {
                                        println!("      Layer: (missing: {})", ls.layer_id);
                                    }
                                }
                            }
                        } else {
                            println!("    Engine: (missing: {})", es.engine_id);
                        }
                    }
                }
            }
        }
        None => eyre::bail!("Rig not found: {id}"),
    }
    Ok(())
}

async fn cmd_rigs_create(signal: &SignalController, name: &str, as_json: bool) -> Result<()> {
    let rig = signal.rigs().create(name.to_string(), vec![]).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_rig",
                "id": rig.id.to_string(),
                "name": rig.name,
                "ok": true,
            }))?
        );
    } else {
        println!("created rig: {} ({})", rig.name, rig.id);
    }
    Ok(())
}

async fn cmd_rigs_delete(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    signal.rigs().delete(id.to_string()).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_rig",
                "id": id,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted rig: {}", id);
    }
    Ok(())
}

async fn cmd_rigs_add_engine(
    signal: &SignalController,
    rig_id: &str,
    engine_id: &str,
    as_json: bool,
) -> Result<()> {
    let rid = signal::rig::RigId::from(rig_id.to_string());
    let eid = signal::engine::EngineId::from(engine_id.to_string());

    let mut rig = signal
        .rigs()
        .load(rid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Rig not found: {rig_id}"))?;
    let engine = signal
        .engines()
        .load(eid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Engine not found: {engine_id}"))?;

    rig.engine_ids.push(eid.clone());

    let selection = signal::rig::EngineSelection::new(
        eid,
        engine.default_variant_id.clone(),
    );
    for scene in &mut rig.variants {
        scene.engine_selections.push(selection.clone());
    }

    signal.rigs().save(rig).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "add_engine",
                "rig_id": rig_id,
                "engine_id": engine_id,
                "ok": true,
            }))?
        );
    } else {
        println!("added engine {} to rig {}", engine.name, rig_id);
    }
    Ok(())
}

async fn cmd_rigs_remove_engine(
    signal: &SignalController,
    rig_id: &str,
    engine_id: &str,
    as_json: bool,
) -> Result<()> {
    let rid = signal::rig::RigId::from(rig_id.to_string());
    let eid = signal::engine::EngineId::from(engine_id.to_string());

    let mut rig = signal
        .rigs()
        .load(rid.clone())
        .await?
        .ok_or_else(|| eyre::eyre!("Rig not found: {rig_id}"))?;

    let before = rig.engine_ids.len();
    rig.engine_ids.retain(|e| *e != eid);
    if rig.engine_ids.len() == before {
        eyre::bail!("Engine {} not found in rig {}", engine_id, rig.name);
    }

    for scene in &mut rig.variants {
        scene.engine_selections.retain(|s| s.engine_id != eid);
    }

    signal.rigs().save(rig).await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "remove_engine",
                "rig_id": rig_id,
                "engine_id": engine_id,
                "ok": true,
            }))?
        );
    } else {
        println!("removed engine {} from rig {}", engine_id, rig_id);
    }
    Ok(())
}

/// Replace children within a module container by matching 1:1 by position
/// against stored raw_block state data from the rig's block presets.
///
/// Handles both Plugin and Container (sub-container) children. For plugins,
/// state_data and raw_block are transplanted. For containers, the entire node
/// is replaced wholesale (children, raw_block, and all) since REAPER's
/// `set_state_chunk` API doesn't work on container FX.
fn replace_by_position(
    children: &mut [daw::file::types::FxChainNode],
    block_states: &[Vec<u8>],
    replaced: &mut usize,
    skipped: &mut usize,
) {
    let mut block_idx = 0;
    for child in children.iter_mut() {
        if block_idx >= block_states.len() {
            *skipped += 1;
            continue;
        }
        let source_bytes = &block_states[block_idx];
        block_idx += 1;

        if source_bytes.is_empty() {
            *skipped += 1;
            continue;
        }

        match child {
            daw::file::types::FxChainNode::Plugin(p) => {
                if try_replace_raw_block(p, source_bytes) {
                    let display = p.custom_name.as_deref().unwrap_or(&p.name);
                    eprintln!(
                        "[state] replaced plugin '{}' ({} bytes)",
                        display,
                        source_bytes.len(),
                    );
                    *replaced += 1;
                } else {
                    *skipped += 1;
                }
            }
            daw::file::types::FxChainNode::Container(c) => {
                if try_replace_container(c, source_bytes) {
                    eprintln!(
                        "[state] replaced container '{}' ({} bytes)",
                        c.name,
                        source_bytes.len(),
                    );
                    *replaced += 1;
                } else {
                    *skipped += 1;
                }
            }
        }
    }
}

/// Parse source raw_block bytes into an FxChainNode (Plugin or Container).
fn parse_raw_block_bytes(source_bytes: &[u8]) -> Option<daw::file::types::FxChainNode> {
    let source_str = std::str::from_utf8(source_bytes).ok()?;
    let source_chain = daw::file::FxChain::parse(&format!(
        "<FXCHAIN\nSHOW 0\nLASTSEL 0\nDOCKED 0\n{source_str}\n>\n"
    ))
    .ok()?;
    source_chain.nodes.into_iter().next()
}

/// Parse source raw_block bytes and transplant state into a loaded plugin,
/// preserving the loaded plugin's FXID. Returns true on success.
fn try_replace_raw_block(
    plugin: &mut daw::file::types::FxPlugin,
    source_bytes: &[u8],
) -> bool {
    if let Some(daw::file::types::FxChainNode::Plugin(source_plugin)) =
        parse_raw_block_bytes(source_bytes)
    {
        let loaded_fxid = plugin.fxid.clone();
        plugin.state_data = source_plugin.state_data.clone();
        plugin.raw_block = source_plugin.raw_block.clone();
        plugin.fxid = loaded_fxid;
        true
    } else {
        false
    }
}

/// Replace a container node's contents with data from stored raw_block bytes.
/// Preserves the loaded container's FXID but replaces children, raw_block,
/// and container_cfg from the source.
fn try_replace_container(
    container: &mut daw::file::types::FxContainer,
    source_bytes: &[u8],
) -> bool {
    if let Some(daw::file::types::FxChainNode::Container(source_container)) =
        parse_raw_block_bytes(source_bytes)
    {
        let loaded_fxid = container.fxid.clone();
        container.children = source_container.children;
        container.raw_block = source_container.raw_block;
        container.container_cfg = source_container.container_cfg;
        container.fxid = loaded_fxid;
        true
    } else {
        false
    }
}

async fn cmd_rigs_open(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    rig_id: &str,
    own_reaper: bool,
    close_after_load: bool,
) -> Result<()> {
    let signal = connect_signal(db).await?;

    // Load rig from DB
    let rig = signal
        .rigs()
        .load(rig_id.to_string())
        .await?
        .ok_or_else(|| eyre::eyre!("Rig not found: {rig_id}"))?;

    eprintln!("Opening rig: {} ({})", rig.name, rig.id);

    // Connect to REAPER — owned or existing
    let (daw, owned) = if own_reaper {
        let (daw, pid, sock) = daw_cli::launch_and_connect("fts-signal")
            .await
            .map_err(|e| eyre::eyre!("Failed to launch REAPER: {e}"))?;
        (daw, Some((pid, sock)))
    } else {
        let daw = daw_cli::connect(socket)
            .await
            .map_err(|e| eyre::eyre!("REAPER required for rig open: {e}"))?;
        (daw, None)
    };

    let project = daw.current_project().await?;

    // ── 1. Collect all block preset state data from DB ──
    // Index by PresetId → raw_block bytes. Also keep name/source indexes for verification.
    let mut state_by_preset_id: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();
    let mut state_by_preset_name: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();
    let mut source_plugin_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut state_by_source_plugin: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();
    for &bt in signal::ALL_BLOCK_TYPES {
        if let Ok(presets) = signal.block_presets().list(bt).await {
            for preset in presets {
                if let Some(data) = preset.default_snapshot().state_data() {
                    let bytes = data.to_vec();
                    state_by_preset_id.insert(preset.id().to_string(), bytes.clone());
                    state_by_preset_name.insert(preset.name().to_string(), bytes.clone());
                    for tag in preset.metadata().tags.as_slice() {
                        if let Some(source) = tag.strip_prefix("source:") {
                            *source_plugin_counts.entry(source.to_string()).or_insert(0) += 1;
                            state_by_source_plugin.insert(source.to_string(), bytes.clone());
                        }
                    }
                }
            }
        }
    }
    for (key, count) in &source_plugin_counts {
        if *count > 1 {
            state_by_source_plugin.remove(key);
        }
    }
    eprintln!(
        "[rigs open] loaded {} block presets with state data",
        state_by_preset_id.len(),
    );

    // ── 2. Resolve rig hierarchy → per-layer module/block specs ──
    use signal::plugin_block::FxRole;
    use signal::ModuleBlockSource;

    struct BlockSpec {
        #[allow(dead_code)]
        display_name: String,
        state_data: Option<Vec<u8>>,
    }
    struct ModuleSpec {
        container_name: String,
        blocks: Vec<BlockSpec>,
    }
    struct LayerSpec {
        name: String,
        modules: Vec<ModuleSpec>,
    }
    struct EngineSpec {
        name: String,
        layers: Vec<LayerSpec>,
    }

    let all_mp = signal.module_presets().list().await.unwrap_or_default();
    let default_scene = rig
        .default_variant()
        .ok_or_else(|| eyre::eyre!("Rig has no default scene"))?;

    let mut engine_specs: Vec<EngineSpec> = Vec::new();
    for engine_sel in &default_scene.engine_selections {
        let engine = signal
            .engines()
            .load(engine_sel.engine_id.to_string())
            .await?
            .ok_or_else(|| eyre::eyre!("Engine not found: {}", engine_sel.engine_id))?;
        let engine_scene = engine
            .variant(&engine_sel.variant_id)
            .or_else(|| engine.default_variant())
            .ok_or_else(|| eyre::eyre!("No scene for engine {}", engine.name))?;

        let mut layer_specs = Vec::new();
        for layer_sel in &engine_scene.layer_selections {
            let layer = signal
                .layers()
                .load(layer_sel.layer_id.to_string())
                .await?
                .ok_or_else(|| eyre::eyre!("Layer not found: {}", layer_sel.layer_id))?;
            let layer_snap = layer
                .variant(&layer_sel.variant_id)
                .or_else(|| layer.default_variant())
                .ok_or_else(|| eyre::eyre!("No snapshot for layer {}", layer.name))?;

            let mut module_specs = Vec::new();
            for module_ref in &layer_snap.module_refs {
                if let Some(mp) = all_mp.iter().find(|p| p.id() == &module_ref.collection_id) {
                    let snap = module_ref
                        .variant_id
                        .as_ref()
                        .and_then(|vid| mp.snapshot(vid))
                        .unwrap_or_else(|| mp.default_snapshot().clone());

                    let mut blocks = Vec::new();
                    for block in snap.module().blocks() {
                        let preset_data = match block.source() {
                            ModuleBlockSource::PresetDefault { preset_id, .. }
                            | ModuleBlockSource::PresetSnapshot { preset_id, .. } => {
                                state_by_preset_id.get(&preset_id.to_string()).cloned()
                            }
                            _ => None,
                        };
                        let role = FxRole::Block {
                            block_type: block.block_type(),
                            name: block.label().to_string(),
                        };
                        blocks.push(BlockSpec {
                            display_name: role.display_name(),
                            state_data: preset_data,
                        });
                    }

                    let role = FxRole::Module {
                        module_type: mp.module_type(),
                        name: mp.name().to_string(),
                    };
                    module_specs.push(ModuleSpec {
                        container_name: role.display_name(),
                        blocks,
                    });
                }
            }
            layer_specs.push(LayerSpec {
                name: layer.name.clone(),
                modules: module_specs,
            });
        }
        engine_specs.push(EngineSpec {
            name: engine.name.clone(),
            layers: layer_specs,
        });
    }

    // ── 3. Check if fast path is viable (all blocks have state_data) ──
    let total_blocks: usize = engine_specs
        .iter()
        .flat_map(|e| &e.layers)
        .flat_map(|l| &l.modules)
        .map(|m| m.blocks.len())
        .sum();
    let blocks_with_state: usize = engine_specs
        .iter()
        .flat_map(|e| &e.layers)
        .flat_map(|l| &l.modules)
        .flat_map(|m| &m.blocks)
        .filter(|b| b.state_data.is_some())
        .count();
    let fast_path = blocks_with_state == total_blocks && total_blocks > 0;

    // Track layer tracks for verification (both paths populate this).
    let mut layer_track_guids: Vec<String> = Vec::new();

    if fast_path {
        // ── 4a. FAST PATH: build FXCHAIN from raw_blocks, single set_chunk ──
        eprintln!(
            "[rigs open] fast path: building FXCHAIN from {} stored chunks",
            blocks_with_state,
        );

        use signal::plugin_block::TrackRole;

        // Create track hierarchy: [R] → [E] → [L]
        let rig_track = project
            .tracks()
            .add(
                &TrackRole::Rig {
                    name: rig.name.clone(),
                }
                .display_name(),
                None,
            )
            .await?;
        rig_track.set_folder_depth(1).await?;

        let engine_count = engine_specs.len();
        for (ei, engine) in engine_specs.iter().enumerate() {
            let engine_track = project
                .tracks()
                .add(
                    &TrackRole::Engine {
                        name: engine.name.clone(),
                    }
                    .display_name(),
                    None,
                )
                .await?;
            engine_track.set_folder_depth(1).await?;

            let layer_count = engine.layers.len();
            for (li, layer) in engine.layers.iter().enumerate() {
                let layer_track = project
                    .tracks()
                    .add(
                        &TrackRole::Layer {
                            name: layer.name.clone(),
                        }
                        .display_name(),
                        None,
                    )
                    .await?;

                // Close folders: last layer closes engine, last engine closes rig
                let is_last_layer = li == layer_count - 1;
                let is_last_engine = ei == engine_count - 1;
                if is_last_layer {
                    let close = if is_last_engine { -2 } else { -1 };
                    layer_track.set_folder_depth(close).await?;
                }

                // Build FXCHAIN for this layer
                let mut fxchain_nodes = Vec::new();
                let mut fx_count = 0usize;
                for module in &layer.modules {
                    let mut children = Vec::new();
                    for block in &module.blocks {
                        if let Some(ref data) = block.state_data {
                            if let Some(node) = parse_raw_block_bytes(data) {
                                children.push(node);
                                fx_count += 1;
                            }
                        }
                    }
                    fxchain_nodes.push(daw::file::types::FxChainNode::Container(
                        daw::file::types::FxContainer {
                            name: module.container_name.clone(),
                            bypassed: false,
                            offline: false,
                            fxid: None,
                            float_pos: None,
                            parallel: false,
                            container_cfg: None, // serial (REAPER default)
                            show: 0,
                            last_sel: 0,
                            docked: false,
                            children,
                            raw_block: String::new(),
                        },
                    ));
                }

                let fxchain = daw::file::FxChain {
                    window_rect: None,
                    show: 0,
                    last_sel: 0,
                    docked: false,
                    nodes: fxchain_nodes,
                    raw_content: String::new(),
                };

                // Inject FXCHAIN into the track chunk
                let chunk = layer_track.get_chunk().await?;
                let fxchain_text = fxchain.to_rpp_string();
                let new_chunk =
                    if let Some(existing) = daw::file::chunk_ops::extract_fxchain_block(&chunk)
                    {
                        chunk.replace(existing, &fxchain_text)
                    } else {
                        // Insert FXCHAIN before the closing >
                        let pos = chunk.rfind('>').ok_or_else(|| {
                            eyre::eyre!("Invalid track chunk: no closing >")
                        })?;
                        format!("{}{}\n{}", &chunk[..pos], fxchain_text, &chunk[pos..])
                    };

                layer_track.set_chunk(new_chunk).await?;
                eprintln!(
                    "[rigs open] set FXCHAIN on '{}' ({} FX in {} modules)",
                    layer.name,
                    fx_count,
                    layer.modules.len(),
                );

                layer_track_guids.push(layer_track.guid().to_string());
            }
        }

        // Brief settle for REAPER to process the chunks
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    } else {
        // ── 4b. FALLBACK: API-based loading + post-load chunk patching ──
        eprintln!(
            "[rigs open] fallback path: {}/{} blocks have state data, using API loading",
            blocks_with_state, total_blocks,
        );

        let load_result = signal
            .service()
            .load_rig_to_daw(&rig, None, &project)
            .await
            .map_err(|e| eyre::eyre!("{e}"))?;

        // Post-load: patch raw_blocks via track chunk manipulation
        for layer_result in &load_result.layer_results {
            let track = match project
                .tracks()
                .by_guid(&layer_result.track_guid)
                .await
            {
                Ok(Some(t)) => t,
                _ => continue,
            };

            let chunk_str = match track.get_chunk().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[state] could not get track chunk: {e}");
                    continue;
                }
            };
            let fxchain_text = match daw::file::chunk_ops::extract_fxchain_block(&chunk_str) {
                Some(t) => t,
                None => {
                    eprintln!("[state] no FXCHAIN block in loaded track");
                    continue;
                }
            };
            let mut parsed = match daw::file::FxChain::parse(fxchain_text) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[state] failed to parse loaded FXCHAIN: {e}");
                    continue;
                }
            };

            // Build per-module state lists from resolved hierarchy
            let mut module_states: std::collections::HashMap<String, Vec<Vec<u8>>> =
                std::collections::HashMap::new();
            for engine in &engine_specs {
                for layer in &engine.layers {
                    for module in &layer.modules {
                        let block_data: Vec<Vec<u8>> = module
                            .blocks
                            .iter()
                            .map(|b| b.state_data.clone().unwrap_or_default())
                            .collect();
                        module_states.insert(module.container_name.clone(), block_data);
                    }
                }
            }

            let mut replaced = 0usize;
            let mut skipped = 0usize;
            for node in parsed.nodes.iter_mut() {
                match node {
                    daw::file::types::FxChainNode::Container(c) => {
                        if let Some(block_states) = module_states.get(&c.name) {
                            replace_by_position(
                                &mut c.children,
                                block_states,
                                &mut replaced,
                                &mut skipped,
                            );
                        } else {
                            eprintln!("[state] container '{}' not in load result", c.name);
                            skipped += c.children.len();
                        }
                    }
                    daw::file::types::FxChainNode::Plugin(p) => {
                        let source = state_by_source_plugin
                            .get(&p.name)
                            .or_else(|| {
                                p.custom_name.as_deref().and_then(|cn| {
                                    cn.strip_prefix("[B] ")
                                        .and_then(|s| s.split_once(": ").map(|(_, name)| name))
                                        .and_then(|name| state_by_preset_name.get(name))
                                })
                            });
                        if let Some(source_bytes) = source {
                            if try_replace_raw_block(p, source_bytes) {
                                replaced += 1;
                                continue;
                            }
                        }
                        skipped += 1;
                    }
                }
            }

            if replaced > 0 {
                let new_fxchain = parsed.to_rpp_string();
                let new_chunk = chunk_str.replace(fxchain_text, &new_fxchain);
                if let Err(e) = track.set_chunk(new_chunk).await {
                    eprintln!("[state] failed to set track chunk: {e}");
                }
                eprintln!(
                    "[state] replaced state for {replaced} plugins ({skipped} skipped)"
                );
            }

            layer_track_guids.push(layer_result.track_guid.clone());
        }
    }

    // ── 5. Verify FX loaded correctly by parsing track chunks ──
    let mut verify_issues: Vec<String> = Vec::new();
    let mut verified_fx = 0usize;
    for guid in &layer_track_guids {
        let track = match project.tracks().by_guid(guid).await {
            Ok(Some(t)) => t,
            _ => {
                verify_issues.push(format!("Layer track {guid} not found in REAPER"));
                continue;
            }
        };

        let chunk_str = match track.get_chunk().await {
            Ok(c) => c,
            Err(e) => {
                verify_issues.push(format!("Could not get track chunk: {e}"));
                continue;
            }
        };
        let fxchain_text = match daw::file::chunk_ops::extract_fxchain_block(&chunk_str) {
            Some(t) => t,
            None => {
                verify_issues.push("No FXCHAIN block in loaded track".to_string());
                continue;
            }
        };
        let parsed = match daw::file::FxChain::parse(fxchain_text) {
            Ok(p) => p,
            Err(e) => {
                verify_issues.push(format!("Failed to parse FXCHAIN: {e}"));
                continue;
            }
        };

        fn verify_nodes(
            nodes: &[daw::file::types::FxChainNode],
            state_by_preset: &std::collections::HashMap<String, Vec<u8>>,
            state_by_source: &std::collections::HashMap<String, Vec<u8>>,
            issues: &mut Vec<String>,
            count: &mut usize,
        ) {
            for node in nodes {
                match node {
                    daw::file::types::FxChainNode::Plugin(p) => {
                        let display = p.custom_name.as_deref().unwrap_or(&p.name);
                        let loaded_size = p.raw_block.len();

                        let source = state_by_source
                            .get(&p.name)
                            .or_else(|| {
                                p.custom_name.as_deref().and_then(|cn| {
                                    let stripped = cn
                                        .strip_prefix("[B] ")
                                        .or_else(|| cn.strip_prefix("[M] "))
                                        .unwrap_or(cn);
                                    stripped
                                        .split_once(": ")
                                        .map(|(_, name)| name)
                                        .and_then(|name| state_by_preset.get(name))
                                })
                            });

                        let match_status = if let Some(source_data) = source {
                            let source_size = source_data.len();
                            if loaded_size > 0 && (loaded_size as f64 / source_size as f64) > 0.1 {
                                "ok"
                            } else if loaded_size == 0 {
                                issues.push(format!(
                                    "'{}': source has {} bytes but loaded plugin has no state",
                                    display, source_size,
                                ));
                                "EMPTY"
                            } else {
                                issues.push(format!(
                                    "'{}': raw_block size mismatch (loaded={}, source={})",
                                    display, loaded_size, source_size,
                                ));
                                "size-mismatch"
                            }
                        } else if loaded_size > 0 {
                            "ok (unmatched)"
                        } else {
                            "no-state"
                        };

                        eprintln!(
                            "[verify]   {} '{}': {} bytes [{}]",
                            if match_status.starts_with("ok") { "✓" } else { "✗" },
                            display,
                            loaded_size,
                            match_status,
                        );
                        *count += 1;
                    }
                    daw::file::types::FxChainNode::Container(c) => {
                        eprintln!(
                            "[verify] ┌ container '{}' ({} children)",
                            c.name,
                            c.children.len()
                        );
                        verify_nodes(&c.children, state_by_preset, state_by_source, issues, count);
                        eprintln!("[verify] └ end '{}'", c.name);
                    }
                }
            }
        }
        verify_nodes(
            &parsed.nodes,
            &state_by_preset_name,
            &state_by_source_plugin,
            &mut verify_issues,
            &mut verified_fx,
        );
    }

    // Teardown if requested (runs even on error)
    if close_after_load {
        if let Some((pid, sock)) = owned {
            daw_cli::teardown_owned(pid, &sock);
        }
    } else if let Some((pid, _)) = &owned {
        eprintln!("REAPER (PID {pid}) left open for inspection.");
    }

    // Print verification summary
    if verify_issues.is_empty() {
        eprintln!(
            "Rig \"{}\" loaded and verified: {} layers, {} FX confirmed in REAPER.",
            rig.name,
            layer_track_guids.len(),
            verified_fx,
        );
    } else {
        eprintln!("Rig \"{}\" loaded with verification issues:", rig.name);
        for issue in &verify_issues {
            eprintln!("  ⚠ {issue}");
        }
        return Err(eyre::eyre!(
            "{} verification issue(s) — FX may not have loaded correctly",
            verify_issues.len()
        ));
    }

    Ok(())
}

// ============================================================================
// Command Implementations — NAM
// ============================================================================

const DEFAULT_NAM_ROOT: &str =
    "~/Documents/Development/FastTrackStudio/signal-library/nam";

async fn cmd_nam_packs(
    vendor: Option<&str>,
    category: Option<&str>,
) -> Result<()> {
    let nam_root = signal::nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
    let packs_dir = nam_root.join("packs");

    let packs = signal::nam_manager::pack::load_packs(&packs_dir)
        .map_err(|e| eyre::eyre!("Failed to load packs: {e}"))?;

    let cat_filter = category
        .map(|c| match c.to_lowercase().as_str() {
            "amp" => Ok(signal::nam_manager::PackCategory::Amp),
            "drive" => Ok(signal::nam_manager::PackCategory::Drive),
            "ir" => Ok(signal::nam_manager::PackCategory::Ir),
            "archetype" => Ok(signal::nam_manager::PackCategory::Archetype),
            _ => Err(eyre::eyre!("Unknown category: {c}. Valid: amp, drive, ir, archetype")),
        })
        .transpose()?;

    let filtered: Vec<_> = packs
        .into_iter()
        .filter(|p| {
            if let Some(ref v) = vendor {
                if !p.vendor.to_lowercase().contains(&v.to_lowercase()) {
                    return false;
                }
            }
            if let Some(ref c) = cat_filter {
                if p.category != *c {
                    return false;
                }
            }
            true
        })
        .collect();

    if filtered.is_empty() {
        println!("No packs found.");
        return Ok(());
    }

    println!("NAM packs ({}):", filtered.len());
    for p in &filtered {
        let file_count = p.files.len();
        let gear = p.gear_model.as_deref().unwrap_or("-");
        println!(
            "  {} — {} [{}] vendor={} files={} gear={}",
            p.id,
            p.label,
            p.category.as_str(),
            p.vendor,
            file_count,
            gear,
        );
    }
    Ok(())
}

/// Capture the full REAPER state chunk for a NAM FX instance.
///
/// Loads the NAM plugin, injects the model path into its state, then reads
/// back the complete REAPER chunk. This produces a portable, host-validated
/// state representation rather than storing raw file paths.
async fn nam_capture_state(fx: &daw::FxHandle, model_path: &str) -> Result<String> {
    let reaper_chunk = fx.state_chunk_encoded().await?
        .ok_or_else(|| eyre::eyre!("FX has no default chunk"))?;
    let segments = signal::nam_manager::extract_state_base64(&reaper_chunk)
        .ok_or_else(|| eyre::eyre!("Failed to extract base64 from chunk"))?;
    let unified_b64 = signal::nam_manager::first_base64_segment(&segments);
    let mut nam_chunk = signal::nam_manager::decode_chunk(unified_b64.trim())
        .map_err(|e| eyre::eyre!("Failed to decode NAM chunk: {e}"))?;
    signal::nam_manager::rewrite_paths(&mut nam_chunk, Some(model_path), None);
    let new_b64 = signal::nam_manager::encode_chunk(&nam_chunk);
    let rebuilt = signal::nam_manager::rebuild_chunk_with_state(&reaper_chunk, &new_b64);
    fx.set_state_chunk_encoded(rebuilt).await
        .map_err(|e| eyre::eyre!("Failed to set chunk: {e}"))?;
    // Read back the final state after REAPER has processed it
    fx.state_chunk_encoded().await?
        .ok_or_else(|| eyre::eyre!("No state after injection"))
}

/// Filter packs by vendor/category for NAM import.
fn filter_nam_packs(
    packs_dir: &Path,
    vendor: Option<&str>,
    category: Option<&str>,
) -> Result<Vec<signal::nam_manager::PackDefinition>> {
    let packs = signal::nam_manager::pack::load_packs(packs_dir)
        .map_err(|e| eyre::eyre!("Failed to load packs: {e}"))?;

    let cat_filter = category
        .map(|c| match c.to_lowercase().as_str() {
            "amp" => Ok(signal::nam_manager::PackCategory::Amp),
            "drive" => Ok(signal::nam_manager::PackCategory::Drive),
            _ => Err(eyre::eyre!("Unknown category for import: {c}. Valid: amp, drive")),
        })
        .transpose()?;

    Ok(packs
        .into_iter()
        .filter(|p| {
            if let Some(ref v) = vendor {
                if !p.vendor.to_lowercase().contains(&v.to_lowercase()) {
                    return false;
                }
            }
            if let Some(ref c) = cat_filter {
                if p.category != *c {
                    return false;
                }
            }
            matches!(p.category, signal::nam_manager::PackCategory::Amp | signal::nam_manager::PackCategory::Drive)
        })
        .collect())
}

/// Collect (tone, filename) pairs from a pack definition.
fn collect_tone_files(pack: &signal::nam_manager::PackDefinition) -> Vec<(String, String)> {
    let is_amp = pack.category == signal::nam_manager::PackCategory::Amp;
    let mut tone_files: Vec<(String, String)> = Vec::new();

    if is_amp {
        for (filename, file_override) in &pack.files {
            if let Some(ref tone) = file_override.tone {
                tone_files.push((tone.clone(), filename.clone()));
            }
        }
        tone_files.sort_by(|a, b| tone_sort_key(&a.0).cmp(&tone_sort_key(&b.0)));
    } else {
        for (filename, file_override) in &pack.files {
            let tone = file_override
                .tone
                .clone()
                .or_else(|| pack.default_tone.clone())
                .unwrap_or_else(|| filename_to_tone(filename));
            tone_files.push((tone, filename.clone()));
        }
        tone_files.sort_by(|a, b| a.0.cmp(&b.0));
    }

    tone_files
}

const NAM_PLUGIN_NAME: &str = "VST3: NeuralAmpModeler (Steven Atkinson)";

/// Dry-run NAM import: prints what would be imported without REAPER or DB changes.
async fn cmd_nam_import_dry_run(
    vendor: Option<&str>,
    category: Option<&str>,
) -> Result<()> {
    let nam_root = signal::nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
    let packs_dir = nam_root.join("packs");
    let filtered = filter_nam_packs(&packs_dir, vendor, category)?;

    if filtered.is_empty() {
        println!("No importable packs found.");
        return Ok(());
    }

    let mut total_presets = 0;
    let mut total_snapshots = 0;

    for pack in &filtered {
        let tone_files = collect_tone_files(pack);
        if tone_files.is_empty() {
            continue;
        }

        let is_amp = pack.category == signal::nam_manager::PackCategory::Amp;
        let category_prefix = if is_amp { "nam-amp" } else { "nam-drive" };
        let preset_id = signal::seed_id(&format!("{}-{}", category_prefix, pack.id));
        let gear_model = pack.gear_model.as_deref().unwrap_or(&pack.label);
        let preset_name = format!("{} [NAM]", gear_model);
        let snap_count = tone_files.len();

        println!(
            "  [dry run] {} — {} ({} snapshots: {})",
            preset_id,
            preset_name,
            snap_count,
            tone_files
                .iter()
                .map(|(t, _)| capitalize(t))
                .collect::<Vec<_>>()
                .join(", "),
        );

        total_presets += 1;
        total_snapshots += snap_count;
    }

    println!(
        "\n[dry run] Would import {} presets ({} total snapshots). No changes made.",
        total_presets, total_snapshots,
    );

    Ok(())
}

/// Live NAM import: loads each model in REAPER, captures real state chunks.
async fn cmd_nam_import(
    signal: &SignalController,
    daw: &Daw,
    vendor: Option<&str>,
    category: Option<&str>,
) -> Result<()> {
    let nam_root = signal::nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
    let packs_dir = nam_root.join("packs");
    let filtered = filter_nam_packs(&packs_dir, vendor, category)?;

    if filtered.is_empty() {
        println!("No importable packs found.");
        return Ok(());
    }

    // Create a scratch track for loading NAM instances
    let project = daw.current_project().await?;
    let scratch_track = project.tracks().add("__nam_import__", None).await?;

    let mut total_presets = 0;
    let mut total_snapshots = 0;

    for pack in &filtered {
        let tone_files = collect_tone_files(pack);
        if tone_files.is_empty() {
            continue;
        }

        let is_amp = pack.category == signal::nam_manager::PackCategory::Amp;
        let category_prefix = if is_amp { "nam-amp" } else { "nam-drive" };
        let block_type = if is_amp {
            signal::BlockType::Amp
        } else {
            signal::BlockType::Drive
        };

        let preset_id = signal::seed_id(&format!("{}-{}", category_prefix, pack.id));
        let gear_model = pack.gear_model.as_deref().unwrap_or(&pack.label);
        let preset_name = format!("{} [NAM]", gear_model);

        // Build snapshots by loading each tone in REAPER
        let mut snapshots: Vec<signal::Snapshot> = Vec::new();

        for (tone, filename) in &tone_files {
            let snap_id =
                signal::seed_id(&format!("{}-{}-{}", category_prefix, pack.id, tone));
            let path = resolve_nam_path(&nam_root, pack, filename);

            let path_str = match path {
                Some(p) => p,
                None => {
                    eprintln!("  warning: {} not found, skipping tone '{}'", filename, tone);
                    continue;
                }
            };

            // Add NAM FX, capture state, then remove
            let block = signal::Block::from_parameters(nam_block_params());
            let snapshot = match async {
                let fx = scratch_track
                    .fx_chain()
                    .add(NAM_PLUGIN_NAME)
                    .await
                    .map_err(|e| eyre::eyre!("Failed to add NAM FX: {e}"))?;

                let chunk_text = nam_capture_state(&fx, &path_str).await?;
                let state_data = chunk_text.into_bytes();

                fx.remove().await
                    .map_err(|e| eyre::eyre!("Failed to remove FX: {e}"))?;

                Ok::<_, eyre::Report>(
                    signal::Snapshot::new(
                        signal::SnapshotId::from(snap_id.to_string()),
                        capitalize(tone),
                        block,
                    )
                    .with_state_data(state_data),
                )
            }
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  warning: failed to capture '{}' ({}): {}", tone, filename, e);
                    continue;
                }
            };

            snapshots.push(snapshot);
        }

        if snapshots.is_empty() {
            eprintln!("  skipping {} — no tones captured successfully", preset_name);
            continue;
        }

        let default_snapshot = snapshots.remove(0);
        let snap_count = 1 + snapshots.len();

        let metadata = signal::metadata::Metadata::new()
            .with_tag(format!("source:{}", NAM_PLUGIN_NAME));

        let preset = signal::Preset::new(
            signal::PresetId::from(preset_id.to_string()),
            preset_name.clone(),
            block_type,
            default_snapshot,
            snapshots,
        )
        .with_metadata(metadata);

        signal.block_presets().save(preset).await?;
        println!(
            "  imported: {} — {} ({} snapshots)",
            preset_id, preset_name, snap_count,
        );

        total_presets += 1;
        total_snapshots += snap_count;
    }

    // Clean up scratch track
    project
        .tracks()
        .remove(daw::TrackRef::Guid(scratch_track.guid().to_string()))
        .await?;

    println!(
        "\nImported {} presets ({} total snapshots).",
        total_presets, total_snapshots,
    );

    Ok(())
}

/// Resolve a NAM file path: {nam_root}/{category_dir}/{pack_directory}/{filename}
fn resolve_nam_path(
    nam_root: &Path,
    pack: &signal::nam_manager::PackDefinition,
    filename: &str,
) -> Option<String> {
    let dir = pack.directory.as_deref().unwrap_or(&pack.id);
    let path = nam_root
        .join(pack.category.directory())
        .join(dir)
        .join(filename);
    if path.exists() {
        Some(path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Default NAM block parameters.
fn nam_block_params() -> Vec<signal::BlockParameter> {
    vec![
        signal::BlockParameter::new("INPUT_LEVEL", "Input Level", 0.5),
        signal::BlockParameter::new("OUTPUT_LEVEL", "Output Level", 0.5),
        signal::BlockParameter::new("NOISE_GATE_THRESHOLD", "Noise Gate Threshold", 0.0),
        signal::BlockParameter::new("NOISE_GATE_ACTIVE", "Noise Gate Active", 0.0),
    ]
}

/// Tone sort key for ordering: clean first, then crunch, drive, lead, overdrive.
fn tone_sort_key(tone: &str) -> u8 {
    match tone.to_lowercase().as_str() {
        "clean" => 0,
        "crunch" => 1,
        "drive" => 2,
        "lead" => 3,
        "overdrive" => 4,
        _ => 5,
    }
}

/// Capitalize first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Extract a tone-like label from a filename.
fn filename_to_tone(filename: &str) -> String {
    let stem = filename
        .rsplit('.')
        .nth(1)
        .unwrap_or(filename);
    // Remove common prefixes like "ML PEAV Block" etc.
    stem.split_whitespace()
        .last()
        .unwrap_or(stem)
        .to_lowercase()
}

// ============================================================================
// Command Implementations — Profiles
// ============================================================================

async fn cmd_profiles_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let profiles = signal.profiles().list().await?;

    if as_json {
        let arr: Vec<_> = profiles
            .iter()
            .map(|p| {
                json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "patch_count": p.patches.len(),
                    "default_patch_id": p.default_patch_id.to_string(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if profiles.is_empty() {
            println!("No profiles.");
            return Ok(());
        }
        println!("Profiles ({}):", profiles.len());
        for p in &profiles {
            println!(
                "  {} — {} ({} patches)",
                p.id, p.name, p.patches.len()
            );
        }
    }
    Ok(())
}

async fn cmd_profiles_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let profile = signal.profiles().load(id.to_string()).await?;
    match profile {
        Some(p) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": p.id.to_string(),
                        "name": p.name,
                        "default_patch_id": p.default_patch_id.to_string(),
                        "patches": p.patches.iter().map(|patch| json!({
                            "id": patch.id.to_string(),
                            "name": patch.name,
                            "target": format!("{:?}", patch.target),
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Profile: {} ({})", p.name, p.id);
                println!("  Default patch: {}", p.default_patch_id);
                for patch in &p.patches {
                    let is_default = patch.id == p.default_patch_id;
                    println!(
                        "  {} {} — {}{}",
                        if is_default { "*" } else { " " },
                        patch.id,
                        patch.name,
                        if is_default { " (default)" } else { "" },
                    );
                }
            }
        }
        None => eyre::bail!("Profile not found: {id}"),
    }
    Ok(())
}

async fn cmd_profiles_activate(
    signal: &SignalController,
    id: &str,
    patch: Option<&str>,
    as_json: bool,
) -> Result<()> {
    let patch_id = patch.map(|p| PatchId::from(p.to_string()));
    let graph = signal
        .profiles()
        .activate(id.to_string(), patch_id)
        .await
        .map_err(|e| eyre::eyre!("Failed to activate: {e:?}"))?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "activate",
                "profile_id": id,
                "patch_id": patch,
                "graph": format!("{graph:?}"),
                "ok": true,
            }))?
        );
    } else {
        println!("activated profile {} patch {:?}", id, patch.unwrap_or("(default)"));
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Patches
// ============================================================================

async fn cmd_patches_list(
    signal: &SignalController,
    profile_id: &str,
    as_json: bool,
) -> Result<()> {
    let profile = signal
        .profiles()
        .load(profile_id.to_string())
        .await?
        .ok_or_else(|| eyre::eyre!("Profile not found: {profile_id}"))?;

    if as_json {
        let arr: Vec<_> = profile
            .patches
            .iter()
            .map(|p| {
                json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "is_default": p.id == profile.default_patch_id,
                    "target": format!("{:?}", p.target),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if profile.patches.is_empty() {
            println!("No patches in profile \"{}\".", profile.name);
            return Ok(());
        }
        println!(
            "Patches in \"{}\" ({}):",
            profile.name, profile.patches.len()
        );
        for p in &profile.patches {
            let is_default = p.id == profile.default_patch_id;
            println!(
                "  {} {} — {}",
                if is_default { "*" } else { " " },
                p.id,
                p.name,
            );
        }
    }
    Ok(())
}

async fn cmd_patches_add(
    signal: &SignalController,
    profile_id: &str,
    name: &str,
    as_json: bool,
) -> Result<()> {
    let profile = signal
        .profiles()
        .load(profile_id.to_string())
        .await?
        .ok_or_else(|| eyre::eyre!("Profile not found: {profile_id}"))?;

    let template_target = profile
        .patches
        .first()
        .map(|p| p.target.clone())
        .ok_or_else(|| eyre::eyre!("Profile has no patches to use as template"))?;

    let patch = Patch {
        id: PatchId::from(uuid::Uuid::new_v4().to_string()),
        name: name.to_string(),
        target: template_target,
        overrides: vec![],
        metadata: Default::default(),
    };

    let result = signal
        .profiles()
        .try_add_patch(profile_id.to_string(), patch)
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "add_patch",
                "profile_id": profile_id,
                "name": name,
                "patch_count": result.patches.len(),
                "ok": true,
            }))?
        );
    } else {
        println!("added patch \"{}\" to \"{}\"", name, result.name);
    }
    Ok(())
}

async fn cmd_patches_remove(
    signal: &SignalController,
    profile_id: &str,
    patch_id: &str,
    as_json: bool,
) -> Result<()> {
    let removed = signal
        .profiles()
        .try_remove_patch(profile_id.to_string(), patch_id.to_string())
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "remove_patch",
                "profile_id": profile_id,
                "patch_id": patch_id,
                "removed_name": removed.name,
                "ok": true,
            }))?
        );
    } else {
        println!("removed patch \"{}\"", removed.name);
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Browse
// ============================================================================

async fn cmd_browse(signal: &SignalController, query: &str, as_json: bool) -> Result<()> {
    let results = signal
        .browse(signal::tagging::BrowserQuery {
            text: Some(query.to_string()),
            ..Default::default()
        })
        .await?;

    if as_json {
        let arr: Vec<_> = results
            .iter()
            .map(|h| {
                json!({
                    "kind": format!("{:?}", h.node.kind),
                    "id": h.node.id,
                    "score": h.score,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if results.is_empty() {
            println!("No results for \"{}\".", query);
            return Ok(());
        }
        println!("Results for \"{}\" ({}):", query, results.len());
        for h in &results {
            println!("  [{:?}] {} (score: {:.2})", h.node.kind, h.node.id, h.score);
        }
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Songs (signal-level)
// ============================================================================

async fn cmd_songs_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let songs = signal.songs().list().await?;

    if as_json {
        let arr: Vec<_> = songs
            .iter()
            .map(|s| {
                json!({
                    "id": s.id.to_string(),
                    "name": s.name,
                    "section_count": s.sections.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if songs.is_empty() {
            println!("No songs.");
            return Ok(());
        }
        println!("Songs ({}):", songs.len());
        for s in &songs {
            println!(
                "  {} — {} ({} sections)",
                s.id, s.name, s.sections.len()
            );
        }
    }
    Ok(())
}

async fn cmd_songs_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let song = signal.songs().load(id.to_string()).await?;
    match song {
        Some(s) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": s.id.to_string(),
                        "name": s.name,
                        "sections": s.sections.iter().map(|sec| json!({
                            "id": sec.id.to_string(),
                            "name": sec.name,
                            "source": format!("{:?}", sec.source),
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Song: {} ({})", s.name, s.id);
                for sec in &s.sections {
                    println!("  {} — {}", sec.id, sec.name);
                }
            }
        }
        None => eyre::bail!("Song not found: {id}"),
    }
    Ok(())
}

async fn cmd_songs_create(signal: &SignalController, name: &str, as_json: bool) -> Result<()> {
    let profiles = signal.profiles().list().await?;
    let profile = profiles
        .first()
        .ok_or_else(|| eyre::eyre!("No profiles exist — create a profile first"))?;

    let song = signal
        .songs()
        .create_from_profile(name.to_string(), profile.id.clone())
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_song",
                "id": song.id.to_string(),
                "name": song.name,
                "ok": true,
            }))?
        );
    } else {
        println!("created song: {} ({})", song.name, song.id);
    }
    Ok(())
}

async fn cmd_songs_delete(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    signal.songs().delete(id.to_string()).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_song",
                "id": id,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted song: {}", id);
    }
    Ok(())
}

// ============================================================================
// Command Implementations — Setlists (signal-level)
// ============================================================================

async fn cmd_setlists_list(signal: &SignalController, as_json: bool) -> Result<()> {
    let setlists = signal.setlists().list().await?;

    if as_json {
        let arr: Vec<_> = setlists
            .iter()
            .map(|s| {
                json!({
                    "id": s.id.to_string(),
                    "name": s.name,
                    "entry_count": s.entries.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if setlists.is_empty() {
            println!("No setlists.");
            return Ok(());
        }
        println!("Setlists ({}):", setlists.len());
        for s in &setlists {
            println!(
                "  {} — {} ({} entries)",
                s.id, s.name, s.entries.len()
            );
        }
    }
    Ok(())
}

async fn cmd_setlists_show(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    let setlist = signal.setlists().load(id.to_string()).await?;
    match setlist {
        Some(s) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": s.id.to_string(),
                        "name": s.name,
                        "entries": s.entries.iter().map(|e| json!({
                            "id": e.id.to_string(),
                            "name": e.name,
                            "song_id": e.song_id.to_string(),
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Setlist: {} ({})", s.name, s.id);
                for (i, e) in s.entries.iter().enumerate() {
                    println!("  {}. {} (song: {})", i + 1, e.name, e.song_id);
                }
            }
        }
        None => eyre::bail!("Setlist not found: {id}"),
    }
    Ok(())
}

async fn cmd_setlists_create(
    signal: &SignalController,
    name: &str,
    as_json: bool,
) -> Result<()> {
    let songs = signal.songs().list().await?;
    let song = songs
        .first()
        .ok_or_else(|| eyre::eyre!("No songs exist — create a song first"))?;

    let setlist = signal
        .setlists()
        .create(name.to_string(), song.name.clone(), song.id.clone())
        .await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "create_setlist",
                "id": setlist.id.to_string(),
                "name": setlist.name,
                "ok": true,
            }))?
        );
    } else {
        println!("created setlist: {} ({})", setlist.name, setlist.id);
    }
    Ok(())
}

async fn cmd_setlists_delete(signal: &SignalController, id: &str, as_json: bool) -> Result<()> {
    signal.setlists().delete(id.to_string()).await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "delete_setlist",
                "id": id,
                "ok": true,
            }))?
        );
    } else {
        println!("deleted setlist: {}", id);
    }
    Ok(())
}

// ============================================================================
// DAW Commands
// ============================================================================

async fn run_daw(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    cmd: &DawCommand,
    as_json: bool,
) -> Result<()> {
    // Commands that don't need an RPC connection
    match cmd {
        DawCommand::Launch { ref config } => {
            return daw_cli::cmd_launch(config.as_deref());
        }
        DawCommand::Quit { pid } => {
            return daw_cli::cmd_quit(*pid);
        }
        // Import needs both DAW and signal DB — handle before the shared daw connection.
        DawCommand::Import {
            ref track,
            ref name,
        } => {
            return cmd_daw_import(db, socket, track, name).await;
        }
        _ => {}
    }

    let daw = daw_cli::connect(socket).await?;

    match cmd {
        DawCommand::Tracks => cmd_daw_tracks(&daw, as_json).await,
        DawCommand::Plugins => daw_cli::cmd_plugins(&daw, as_json).await,
        DawCommand::Fx { ref track } => cmd_daw_fx(&daw, track, as_json).await,
        DawCommand::Projects => daw_cli::cmd_projects(&daw, as_json).await,
        DawCommand::Open { ref path } => daw_cli::cmd_open(&daw, path, as_json).await,
        DawCommand::Close { ref guid } => daw_cli::cmd_close(&daw, guid.as_deref()).await,
        DawCommand::AddTrack { ref name, at } => daw_cli::cmd_add_track(&daw, name.as_deref(), *at, as_json).await,
        DawCommand::RemoveTrack { ref track } => daw_cli::cmd_remove_track(&daw, track).await,
        DawCommand::Scan { ref track } => cmd_daw_scan(&daw, track, as_json).await,
        // Already handled above
        DawCommand::Launch { .. } | DawCommand::Quit { .. } | DawCommand::Import { .. } => {
            unreachable!()
        }
    }
}

async fn cmd_daw_tracks(daw: &Daw, as_json: bool) -> Result<()> {
    daw_cli::cmd_tracks(daw, as_json).await
}

async fn cmd_daw_fx(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    daw_cli::cmd_fx(daw, track_arg, as_json).await
}

async fn cmd_daw_scan(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    let handle = daw_cli::resolve_track_handle(daw, track_arg).await?;
    let tree = handle.fx_chain().tree().await?;
    let chain = signal::signal_daw_bridge::infer_chain_from_fx_tree(&tree);

    if as_json {
        println!("{}", serde_json::to_string_pretty(&chain)?);
    } else {
        println!("Signal chain for track \"{}\":", track_arg);
        for module in &chain.modules {
            let block_count = module.chain.blocks().len();
            println!(
                "  [{}] {} ({} block{})",
                module.module_type.as_str(),
                module.name,
                block_count,
                if block_count == 1 { "" } else { "s" }
            );
            for block in module.chain.blocks() {
                println!(
                    "    - {} ({})",
                    block.label(),
                    block.block_type().as_str()
                );
            }
        }
        for block in &chain.standalone_blocks {
            println!(
                "  [{}] {} (standalone)",
                block.block_type.as_str(),
                block.name
            );
        }
    }
    Ok(())
}

async fn cmd_daw_import(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    track_arg: &str,
    rig_name: &str,
) -> Result<()> {
    use signal::ops::rig_importer::{ImportBlock, ImportChain, ImportModule};
    use std::collections::HashMap;

    let daw = daw_cli::connect(socket).await?;
    let signal = connect_signal(db).await?;

    let handle = daw_cli::resolve_track_handle(&daw, track_arg).await?;
    let tree = handle.fx_chain().tree().await?;
    let inferred = signal::signal_daw_bridge::infer_chain_from_fx_tree(&tree);

    // Capture per-plugin state by parsing the full track RPP chunk.
    // This avoids REAPER API limitations with encoded container-child indices.
    // dawfile-reaper parses the nested container structure and gives us
    // per-plugin raw_block text matched by FXID (GUID).
    let mut state_by_guid: HashMap<String, Vec<u8>> = HashMap::new();
    match handle.get_chunk().await {
        Ok(chunk_str) => {
            if let Some(fxchain_text) =
                daw::file::chunk_ops::extract_fxchain_block(&chunk_str)
            {
                if let Ok(parsed) = daw::file::FxChain::parse(fxchain_text) {
                    fn collect_plugin_state(
                        nodes: &[daw::file::types::FxChainNode],
                        out: &mut HashMap<String, Vec<u8>>,
                    ) {
                        for node in nodes {
                            match node {
                                daw::file::types::FxChainNode::Plugin(p) => {
                                    if !p.raw_block.is_empty() {
                                        if let Some(fxid) = &p.fxid {
                                            // Store by GUID (strip braces: RPP {GUID} → tree GUID)
                                            let guid = fxid
                                                .strip_prefix('{')
                                                .and_then(|s| s.strip_suffix('}'))
                                                .unwrap_or(fxid);
                                            out.insert(
                                                guid.to_string(),
                                                p.raw_block.as_bytes().to_vec(),
                                            );
                                        } else if let Some(cn) = &p.custom_name {
                                            // Fallback for plugins without FXID (e.g. JS
                                            // inside containers): store by custom_name.
                                            // The inferred chain uses the display name as
                                            // the block ID for these plugins.
                                            out.insert(
                                                cn.clone(),
                                                p.raw_block.as_bytes().to_vec(),
                                            );
                                        }
                                    }
                                }
                                daw::file::types::FxChainNode::Container(c) => {
                                    // Store the entire container's raw_block keyed by its
                                    // name — sub-containers use name as their block ID in
                                    // the inferred chain. This allows `rigs open` to
                                    // restore the full container structure.
                                    if !c.raw_block.is_empty() {
                                        out.insert(
                                            c.name.clone(),
                                            c.raw_block.as_bytes().to_vec(),
                                        );
                                    }
                                    collect_plugin_state(&c.children, out);
                                }
                            }
                        }
                    }
                    collect_plugin_state(&parsed.nodes, &mut state_by_guid);
                    eprintln!(
                        "[import] captured state for {} plugins from track chunk",
                        state_by_guid.len()
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("[import] warning: could not get track chunk for state capture: {e}");
        }
    }

    // Capture per-plugin parameters by walking the FX tree and querying
    // each plugin's parameter list via the DAW API. Keyed by GUID.
    let mut params_by_guid: HashMap<String, Vec<(String, String, f64)>> = HashMap::new();
    for node in &tree.nodes {
        collect_fx_params(&handle, node, &mut params_by_guid).await;
    }
    eprintln!(
        "[import] captured parameters for {} plugins from DAW",
        params_by_guid.len()
    );

    // Convert InferredChain → ImportChain (bridge-free input type).
    let chain = ImportChain {
        modules: inferred
            .modules
            .iter()
            .map(|m| {
                let blocks_vec = m.chain.blocks();
                ImportModule {
                    name: m.name.clone(),
                    module_type: m.module_type,
                    has_parallel_routing: !m.chain.is_serial(),
                    blocks: blocks_vec
                        .iter()
                        .enumerate()
                        .map(|(i, b)| {
                            // Look up by GUID first, then fall back to label
                            // (JS plugins inside containers may lack FXID, so
                            // collect_plugin_state stores them by custom_name).
                            let label_str = b.label();
                            let sd = state_by_guid
                                .get(b.id())
                                .or_else(|| state_by_guid.get(label_str))
                                .cloned();
                            eprintln!(
                                "[import]   block '{}' id={} state={}",
                                label_str,
                                b.id(),
                                sd.as_ref().map_or("NONE".to_string(), |d| format!("{} bytes", d.len()))
                            );
                            ImportBlock {
                                label: label_str.to_string(),
                                block_type: b.block_type(),
                                plugin_name: m
                                    .block_plugin_names
                                    .get(i)
                                    .filter(|s| !s.is_empty())
                                    .cloned(),
                                state_data: sd,
                                parameters: params_by_guid
                                    .get(b.id())
                                    .or_else(|| params_by_guid.get(label_str))
                                    .cloned()
                                    .unwrap_or_default(),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
        standalone_blocks: inferred
            .standalone_blocks
            .iter()
            .map(|b| ImportBlock {
                label: b.name.clone(),
                block_type: b.block_type,
                plugin_name: Some(b.plugin_name.clone()),
                state_data: None, // standalone blocks don't have GUIDs in the tree
                parameters: Vec::new(),
            })
            .collect(),
    };

    println!("Importing rig \"{rig_name}\" from track \"{track_arg}\"...");
    let result = signal.import_rig_from_chain(&chain, rig_name).await?;

    println!("Created rig: {} ({})", result.rig.name, result.rig_id);
    for (name, id) in &result.module_preset_ids {
        println!("  module: {name} ({id})");
    }
    println!(
        "  {} new block preset{}, {} reused",
        result.new_block_preset_count,
        if result.new_block_preset_count == 1 { "" } else { "s" },
        result.reused_block_preset_count
    );
    println!("Run: signal rigs open {}", result.rig_id);
    Ok(())
}

/// Recursively collect plugin parameters from the FX tree by querying each
/// plugin's parameter list via the DAW API. Results are keyed by GUID.
async fn collect_fx_params(
    track: &daw::TrackHandle,
    node: &daw::FxNode,
    out: &mut std::collections::HashMap<String, Vec<(String, String, f64)>>,
) {
    use daw::FxNodeKind;
    match &node.kind {
        FxNodeKind::Plugin(fx) => {
            let guid = &fx.guid;
            match track.fx_chain().by_guid(guid).await {
                Ok(Some(fx_handle)) => match fx_handle.parameters().await {
                    Ok(params) => {
                        let mapped: Vec<(String, String, f64)> = params
                            .into_iter()
                            .map(|p| (format!("p{}", p.index), p.name, p.value))
                            .collect();
                        eprintln!(
                            "[import]   params for '{}': {} parameters",
                            fx.name,
                            mapped.len()
                        );
                        out.insert(guid.clone(), mapped);
                    }
                    Err(e) => {
                        eprintln!(
                            "[import]   warning: could not get params for '{}': {e}",
                            fx.name
                        );
                    }
                },
                _ => {
                    eprintln!(
                        "[import]   warning: could not find FX handle for '{}'",
                        fx.name
                    );
                }
            }
        }
        FxNodeKind::Container { children, .. } => {
            for child in children {
                Box::pin(collect_fx_params(track, child, out)).await;
            }
        }
    }
}

// ============================================================================
// Signal Load Command (block + module auto-detection)
// ============================================================================

async fn cmd_signal_load(
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
    preset_type: &str,
    preset_id: &str,
    track_arg: &str,
    snapshot_id: Option<&str>,
    as_json: bool,
) -> Result<()> {
    let signal = connect_signal(db).await?;
    let daw = daw_cli::connect(socket).await?;
    let track_handle = daw_cli::resolve_track_handle(&daw, track_arg).await?;

    let snap_id = snapshot_id.map(|s| signal::SnapshotId::from(s.to_string()));

    // Try block type first.
    if let Some(bt) = signal::BlockType::from_str(preset_type) {
        let pid = signal::PresetId::from(preset_id.to_string());

        // Check if it's a block preset.
        let block_presets = signal.block_presets().list(bt).await?;
        if block_presets.iter().any(|p| p.id() == &pid) {
            let result = signal
                .service()
                .load_block_to_track(bt, &pid, snap_id.as_ref(), &track_handle)
                .await
                .map_err(|e| eyre::eyre!("{e}"))?;

            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "load",
                        "kind": "block",
                        "preset_type": preset_type,
                        "preset_id": preset_id,
                        "display_name": result.display_name,
                        "fx_guid": result.fx_guid,
                        "ok": true,
                    }))?
                );
            } else {
                println!(
                    "Loaded \"{}\" to track \"{}\" — FX GUID: {}",
                    result.display_name, track_arg, result.fx_guid,
                );
            }
            return Ok(());
        }
    }

    // Try module type.
    if let Some(mt) = signal::ModuleType::from_str(preset_type) {
        let pid = signal::ModulePresetId::from(preset_id.to_string());

        let module_presets = signal.module_presets().list().await?;
        if module_presets.iter().any(|p| p.id() == &pid) {
            let result = signal
                .service()
                .load_module_to_track(mt, &pid, 0, &track_handle)
                .await
                .map_err(|e| eyre::eyre!("{e}"))?;

            if as_json {
                let fx_list: Vec<_> = result
                    .loaded_fx
                    .iter()
                    .map(|f| {
                        json!({
                            "fx_guid": f.fx_guid,
                            "display_name": f.display_name,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "load",
                        "kind": "module",
                        "preset_type": preset_type,
                        "preset_id": preset_id,
                        "display_name": result.display_name,
                        "loaded_fx": fx_list,
                        "ok": true,
                    }))?
                );
            } else {
                println!(
                    "Loaded module \"{}\" to track \"{}\" — {} FX instances",
                    result.display_name,
                    track_arg,
                    result.loaded_fx.len(),
                );
            }
            return Ok(());
        }
    }

    Err(eyre::eyre!(
        "No block or module preset found for type \"{preset_type}\" with ID \"{preset_id}\""
    ))
}

