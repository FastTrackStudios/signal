//! signal-cli library — reusable components for Signal CLI tools.
//!
//! Provides connection management, command implementations, and formatting
//! for querying and manipulating the Signal library (presets, rigs, profiles,
//! macros, songs, setlists).

use std::path::PathBuf;

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
    Layers(EntityCommand),
    /// Engine operations
    #[command(subcommand)]
    Engines(EntityCommand),
    /// Rig operations
    #[command(subcommand)]
    Rigs(EntityCommand),
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

/// Shared CRUD subcommands for layers, engines, rigs, songs, setlists.
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

        SignalCommand::Layers(EntityCommand::List) => cmd_layers_list(&signal, as_json).await,
        SignalCommand::Layers(EntityCommand::Show { ref id }) => {
            cmd_layers_show(&signal, id, as_json).await
        }
        SignalCommand::Layers(EntityCommand::Create { ref name }) => {
            cmd_layers_create(&signal, name, as_json).await
        }
        SignalCommand::Layers(EntityCommand::Delete { ref id }) => {
            cmd_layers_delete(&signal, id, as_json).await
        }

        SignalCommand::Engines(EntityCommand::List) => cmd_engines_list(&signal, as_json).await,
        SignalCommand::Engines(EntityCommand::Show { ref id }) => {
            cmd_engines_show(&signal, id, as_json).await
        }
        SignalCommand::Engines(EntityCommand::Create { ref name }) => {
            cmd_engines_create(&signal, name, as_json).await
        }
        SignalCommand::Engines(EntityCommand::Delete { ref id }) => {
            cmd_engines_delete(&signal, id, as_json).await
        }

        SignalCommand::Rigs(EntityCommand::List) => cmd_rigs_list(&signal, as_json).await,
        SignalCommand::Rigs(EntityCommand::Show { ref id }) => {
            cmd_rigs_show(&signal, id, as_json).await
        }
        SignalCommand::Rigs(EntityCommand::Create { ref name }) => {
            cmd_rigs_create(&signal, name, as_json).await
        }
        SignalCommand::Rigs(EntityCommand::Delete { ref id }) => {
            cmd_rigs_delete(&signal, id, as_json).await
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
        SignalCommand::Daw(_) | SignalCommand::Load { .. } => unreachable!(),
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
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": l.id.to_string(),
                        "name": l.name,
                        "variants": l.variants.iter().map(|v| json!({
                            "id": v.id.to_string(),
                            "name": v.name,
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Layer: {} ({})", l.name, l.id);
                for v in &l.variants {
                    println!("  {} — {}", v.id, v.name);
                }
            }
        }
        None => eyre::bail!("Layer not found: {id}"),
    }
    Ok(())
}

async fn cmd_layers_create(
    signal: &SignalController,
    name: &str,
    as_json: bool,
) -> Result<()> {
    let layer = signal
        .layers()
        .create(name.to_string(), signal_proto::EngineType::Guitar)
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
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": e.id.to_string(),
                        "name": e.name,
                        "engine_type": format!("{:?}", e.engine_type),
                        "layer_ids": e.layer_ids.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
                        "scenes": e.variants.iter().map(|v| json!({
                            "id": v.id.to_string(),
                            "name": v.name,
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Engine: {} ({}) [{:?}]", e.name, e.id, e.engine_type);
                println!("  Layers: {}", e.layer_ids.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", "));
                for v in &e.variants {
                    println!("  Scene: {} — {}", v.id, v.name);
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
    as_json: bool,
) -> Result<()> {
    let engine = signal
        .engines()
        .create(name.to_string(), signal_proto::EngineType::Guitar, vec![])
        .await?;

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
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": r.id.to_string(),
                        "name": r.name,
                        "engine_ids": r.engine_ids.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
                        "scenes": r.variants.iter().map(|v| json!({
                            "id": v.id.to_string(),
                            "name": v.name,
                        })).collect::<Vec<_>>(),
                    }))?
                );
            } else {
                println!("Rig: {} ({})", r.name, r.id);
                println!("  Engines: {}", r.engine_ids.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "));
                for v in &r.variants {
                    println!("  Scene: {} — {}", v.id, v.name);
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

