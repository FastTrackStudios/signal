//! signal-cli library — reusable components for Signal CLI tools.
//!
//! Provides connection management, command implementations, and formatting
//! for querying and manipulating the Signal library (presets, rigs, profiles,
//! macros, songs, setlists).

use std::path::{Path, PathBuf};

use clap::Subcommand;
use daw_control::Daw;
use eyre::Result;
use serde_json::json;
use signal_controller::SignalController;
use signal_proto::profile::{Patch, PatchId};

// ============================================================================
// Connection
// ============================================================================

const DEFAULT_DB_PATH: &str = "~/Music/FastTrackStudio/Library/signal.db";

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub async fn connect_signal(db: Option<PathBuf>) -> Result<SignalController> {
    let path = match db {
        Some(p) => p,
        None => expand_tilde(DEFAULT_DB_PATH),
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
        /// Config ID (e.g., "fts-tracks", "fts-guitar")
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

        SignalCommand::Modules(ModulesCommand::List) => {
            cmd_modules_list(&signal, as_json).await
        }
        SignalCommand::Modules(ModulesCommand::Show { ref id }) => {
            cmd_modules_show(&signal, id, as_json).await
        }

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
                    daw_cli::launch_and_connect("fts-guitar").await
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
        SignalCommand::Daw(_) | SignalCommand::Load { .. } | SignalCommand::Rigs(RigsCommand::Open { .. }) => unreachable!(),
    }
}

// ============================================================================
// Block Type Resolution
// ============================================================================

fn parse_block_type(s: &str) -> Result<signal_proto::BlockType> {
    signal_proto::BlockType::from_str(s)
        .ok_or_else(|| eyre::eyre!("Unknown block type: \"{s}\". Valid types: amp, drive, eq, reverb, delay, compressor, gate, chorus, flanger, phaser, tremolo, cabinet, etc."))
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
// Command Implementations — Import
// ============================================================================

async fn cmd_presets_import(signal: &SignalController, cmd: &ImportCommand) -> Result<()> {
    // Compute library root for file-based preset writing
    let library_root = expand_tilde("~/Music/FastTrackStudio/Library");

    match cmd {
        ImportCommand::Fabfilter {
            plugin,
            all,
            dry_run,
        } => {
            let importer = signal_import::fabfilter::FabFilterImporter::new();

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
                    let report = signal_import::import_presets_with_library(
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
                    print!("{}", signal_import::dry_run_report(&collection));
                    println!("[dry run] No changes made.");
                    return Ok(());
                }
                let report = signal_import::import_presets_with_library(
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
            let collection = signal_import::rfxchain::RfxChainImporter::scan(
                source,
                bt,
                name.as_deref(),
            )?;
            if *dry_run {
                print!("{}", signal_import::dry_run_report(&collection));
                println!("[dry run] No changes made.");
                return Ok(());
            }
            let report = signal_import::import_presets_with_library(
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
    preset_id: &signal_proto::PresetId,
) -> String {
    // Try common block types
    for bt in &[
        signal_proto::BlockType::Amp,
        signal_proto::BlockType::Drive,
        signal_proto::BlockType::Eq,
        signal_proto::BlockType::Reverb,
        signal_proto::BlockType::Delay,
        signal_proto::BlockType::Compressor,
        signal_proto::BlockType::Gate,
        signal_proto::BlockType::Chorus,
        signal_proto::BlockType::Flanger,
        signal_proto::BlockType::Phaser,
        signal_proto::BlockType::Tremolo,
        signal_proto::BlockType::Cabinet,
        signal_proto::BlockType::Boost,
        signal_proto::BlockType::Saturator,
        signal_proto::BlockType::Limiter,
        signal_proto::BlockType::Volume,
    ] {
        if let Ok(presets) = signal.block_presets().list(*bt).await {
            if let Some(p) = presets.iter().find(|p| p.id() == preset_id) {
                return p.name().to_string();
            }
        }
    }
    preset_id.to_string()
}

fn parse_engine_type(s: &str) -> Result<signal_proto::EngineType> {
    match s.to_lowercase().as_str() {
        "guitar" => Ok(signal_proto::EngineType::Guitar),
        "bass" => Ok(signal_proto::EngineType::Bass),
        "vocal" | "vocals" => Ok(signal_proto::EngineType::Vocal),
        "keys" => Ok(signal_proto::EngineType::Keys),
        "synth" => Ok(signal_proto::EngineType::Synth),
        "organ" => Ok(signal_proto::EngineType::Organ),
        "pad" => Ok(signal_proto::EngineType::Pad),
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
    let lid = signal_proto::layer::LayerId::from(layer_id.to_string());
    let pid = signal_proto::PresetId::from(preset_id.to_string());

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
        signal_proto::layer::BlockRef::new(pid.clone())
            .with_variant(signal_proto::SnapshotId::from(vid.to_string()))
    } else {
        signal_proto::layer::BlockRef::new(pid.clone())
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
    let lid = signal_proto::layer::LayerId::from(layer_id.to_string());
    let pid = signal_proto::PresetId::from(preset_id.to_string());

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
        let lid = signal_proto::layer::LayerId::from(lid_str.to_string());
        let layer = signal
            .layers()
            .load(lid.clone())
            .await?
            .ok_or_else(|| eyre::eyre!("Layer not found: {lid_str}"))?;
        layer_selections.push(signal_proto::engine::LayerSelection::new(
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
    let eid = signal_proto::engine::EngineId::from(engine_id.to_string());
    let lid = signal_proto::layer::LayerId::from(layer_id.to_string());

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

    let selection = signal_proto::engine::LayerSelection::new(
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
    let eid = signal_proto::engine::EngineId::from(engine_id.to_string());
    let lid = signal_proto::layer::LayerId::from(layer_id.to_string());

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
    let rid = signal_proto::rig::RigId::from(rig_id.to_string());
    let eid = signal_proto::engine::EngineId::from(engine_id.to_string());

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

    let selection = signal_proto::rig::EngineSelection::new(
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
    let rid = signal_proto::rig::RigId::from(rig_id.to_string());
    let eid = signal_proto::engine::EngineId::from(engine_id.to_string());

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
        let (daw, pid, sock) = daw_cli::launch_and_connect("fts-guitar")
            .await
            .map_err(|e| eyre::eyre!("Failed to launch REAPER: {e}"))?;
        (daw, Some((pid, sock)))
    } else {
        let daw = daw_cli::connect(socket)
            .await
            .map_err(|e| eyre::eyre!("REAPER required for rig open: {e}"))?;
        (daw, None)
    };

    // Load rig into current REAPER project
    let project = daw.current_project().await?;
    let load_result = signal
        .service()
        .load_rig_to_daw(&rig, None, &project)
        .await
        .map_err(|e| eyre::eyre!("{e}"));

    // Verify FX actually loaded on each layer track (runs before teardown so
    // REAPER is still alive). Collects issues but doesn't fail the command —
    // we still want teardown and the summary to print.
    let mut verify_issues: Vec<String> = Vec::new();
    if let Ok(ref result) = load_result {
        for layer_result in &result.layer_results {
            let track = project
                .tracks()
                .by_guid(&layer_result.track_guid)
                .await;
            let track = match track {
                Ok(Some(t)) => t,
                _ => {
                    verify_issues.push(format!(
                        "Layer track {} not found in REAPER",
                        layer_result.track_guid
                    ));
                    continue;
                }
            };

            // Count expected FX: modules (blocks inside modules) + standalone blocks
            let module_fx: usize = layer_result
                .modules
                .iter()
                .map(|m| m.loaded_fx.len())
                .sum();
            let expected_fx = module_fx + layer_result.standalone_blocks.len();

            let actual_fx = match track.fx_chain().all().await {
                Ok(fx_list) => fx_list,
                Err(e) => {
                    verify_issues.push(format!(
                        "Failed to query FX on track {}: {e}",
                        layer_result.track_guid
                    ));
                    continue;
                }
            };

            if actual_fx.len() != expected_fx {
                verify_issues.push(format!(
                    "Track '{}': expected {} FX, found {} in REAPER",
                    layer_result.track_guid,
                    expected_fx,
                    actual_fx.len()
                ));
            } else {
                // Verify each FX GUID matches what we expected
                let expected_guids: Vec<&str> = layer_result
                    .modules
                    .iter()
                    .flat_map(|m| m.loaded_fx.iter().map(|b| b.fx_guid.as_str()))
                    .chain(layer_result.standalone_blocks.iter().map(|b| b.fx_guid.as_str()))
                    .collect();
                for (i, expected_guid) in expected_guids.iter().enumerate() {
                    if i < actual_fx.len() && actual_fx[i].guid != *expected_guid {
                        verify_issues.push(format!(
                            "FX[{}] GUID mismatch: expected {}, found {}",
                            i, expected_guid, actual_fx[i].guid
                        ));
                    }
                }
            }
        }
    }

    // Teardown if requested (runs even on error)
    if close_after_load {
        if let Some((pid, sock)) = owned {
            daw_cli::teardown_owned(pid, &sock);
        }
    } else if let Some((pid, _)) = &owned {
        eprintln!("REAPER (PID {pid}) left open for inspection.");
    }

    let result = load_result?;

    // Print verification summary
    if verify_issues.is_empty() {
        let total_fx: usize = result
            .layer_results
            .iter()
            .map(|l| {
                let module_fx: usize = l.modules.iter().map(|m| m.loaded_fx.len()).sum();
                module_fx + l.standalone_blocks.len()
            })
            .sum();
        eprintln!(
            "Rig \"{}\" loaded and verified: {} layers, {} FX confirmed in REAPER.",
            rig.name,
            result.layer_results.len(),
            total_fx,
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
    let nam_root = nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
    let packs_dir = nam_root.join("packs");

    let packs = nam_manager::pack::load_packs(&packs_dir)
        .map_err(|e| eyre::eyre!("Failed to load packs: {e}"))?;

    let cat_filter = category
        .map(|c| match c.to_lowercase().as_str() {
            "amp" => Ok(nam_manager::PackCategory::Amp),
            "drive" => Ok(nam_manager::PackCategory::Drive),
            "ir" => Ok(nam_manager::PackCategory::Ir),
            "archetype" => Ok(nam_manager::PackCategory::Archetype),
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
async fn nam_capture_state(fx: &daw_control::FxHandle, model_path: &str) -> Result<String> {
    let reaper_chunk = fx.state_chunk_encoded().await?
        .ok_or_else(|| eyre::eyre!("FX has no default chunk"))?;
    let segments = nam_manager::extract_state_base64(&reaper_chunk)
        .ok_or_else(|| eyre::eyre!("Failed to extract base64 from chunk"))?;
    let unified_b64 = nam_manager::first_base64_segment(&segments);
    let mut nam_chunk = nam_manager::decode_chunk(unified_b64.trim())
        .map_err(|e| eyre::eyre!("Failed to decode NAM chunk: {e}"))?;
    nam_manager::rewrite_paths(&mut nam_chunk, Some(model_path), None);
    let new_b64 = nam_manager::encode_chunk(&nam_chunk);
    let rebuilt = nam_manager::rebuild_chunk_with_state(&reaper_chunk, &new_b64);
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
) -> Result<Vec<nam_manager::PackDefinition>> {
    let packs = nam_manager::pack::load_packs(packs_dir)
        .map_err(|e| eyre::eyre!("Failed to load packs: {e}"))?;

    let cat_filter = category
        .map(|c| match c.to_lowercase().as_str() {
            "amp" => Ok(nam_manager::PackCategory::Amp),
            "drive" => Ok(nam_manager::PackCategory::Drive),
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
            matches!(p.category, nam_manager::PackCategory::Amp | nam_manager::PackCategory::Drive)
        })
        .collect())
}

/// Collect (tone, filename) pairs from a pack definition.
fn collect_tone_files(pack: &nam_manager::PackDefinition) -> Vec<(String, String)> {
    let is_amp = pack.category == nam_manager::PackCategory::Amp;
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
    let nam_root = nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
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

        let is_amp = pack.category == nam_manager::PackCategory::Amp;
        let category_prefix = if is_amp { "nam-amp" } else { "nam-drive" };
        let preset_id = signal_proto::seed_id(&format!("{}-{}", category_prefix, pack.id));
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
    let nam_root = nam_manager::nam_root_from_env(&expand_tilde(DEFAULT_NAM_ROOT));
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

        let is_amp = pack.category == nam_manager::PackCategory::Amp;
        let category_prefix = if is_amp { "nam-amp" } else { "nam-drive" };
        let block_type = if is_amp {
            signal_proto::BlockType::Amp
        } else {
            signal_proto::BlockType::Drive
        };

        let preset_id = signal_proto::seed_id(&format!("{}-{}", category_prefix, pack.id));
        let gear_model = pack.gear_model.as_deref().unwrap_or(&pack.label);
        let preset_name = format!("{} [NAM]", gear_model);

        // Build snapshots by loading each tone in REAPER
        let mut snapshots: Vec<signal_proto::Snapshot> = Vec::new();

        for (tone, filename) in &tone_files {
            let snap_id =
                signal_proto::seed_id(&format!("{}-{}-{}", category_prefix, pack.id, tone));
            let path = resolve_nam_path(&nam_root, pack, filename);

            let path_str = match path {
                Some(p) => p,
                None => {
                    eprintln!("  warning: {} not found, skipping tone '{}'", filename, tone);
                    continue;
                }
            };

            // Add NAM FX, capture state, then remove
            let block = signal_proto::Block::from_parameters(nam_block_params());
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
                    signal_proto::Snapshot::new(
                        signal_proto::SnapshotId::from(snap_id.to_string()),
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

        let metadata = signal_proto::metadata::Metadata::new()
            .with_tag(format!("source:{}", NAM_PLUGIN_NAME));

        let preset = signal_proto::Preset::new(
            signal_proto::PresetId::from(preset_id.to_string()),
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
        .remove(daw_control::TrackRef::Guid(scratch_track.guid().to_string()))
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
    pack: &nam_manager::PackDefinition,
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
fn nam_block_params() -> Vec<signal_proto::BlockParameter> {
    vec![
        signal_proto::BlockParameter::new("INPUT_LEVEL", "Input Level", 0.5),
        signal_proto::BlockParameter::new("OUTPUT_LEVEL", "Output Level", 0.5),
        signal_proto::BlockParameter::new("NOISE_GATE_THRESHOLD", "Noise Gate Threshold", 0.0),
        signal_proto::BlockParameter::new("NOISE_GATE_ACTIVE", "Noise Gate Active", 0.0),
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
        .browse(signal_proto::tagging::BrowserQuery {
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
    _db: Option<PathBuf>,
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
        // Already handled above
        DawCommand::Launch { .. } | DawCommand::Quit { .. } => unreachable!(),
    }
}

async fn cmd_daw_tracks(daw: &Daw, as_json: bool) -> Result<()> {
    daw_cli::cmd_tracks(daw, as_json).await
}

async fn cmd_daw_fx(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    daw_cli::cmd_fx(daw, track_arg, as_json).await
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

    let snap_id = snapshot_id.map(|s| signal_proto::SnapshotId::from(s.to_string()));

    // Try block type first.
    if let Some(bt) = signal_proto::BlockType::from_str(preset_type) {
        let pid = signal_proto::PresetId::from(preset_id.to_string());

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
    if let Some(mt) = signal_proto::ModuleType::from_str(preset_type) {
        let pid = signal_proto::ModulePresetId::from(preset_id.to_string());

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

