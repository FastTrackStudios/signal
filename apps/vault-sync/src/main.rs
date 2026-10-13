//! `session-vault-sync` — turns Task's `Assets/Tracks` folder into a real,
//! portable Song Library (one `.md` note per song, written INTO that
//! song's own folder alongside its audio/lyrics/chart), and writes
//! setlist notes in Task's own `[[wikilink]]` format under
//! `Assets/Setlists`. See `Cargo.toml` for why this stops at "correct
//! notes on disk" rather than a live web player.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use session_vault_sync::library::{self, LibrarySong};
use session_vault_sync::setlist::resolve_song;
use session_vault_sync::{live_bus, rpp, vault};

#[derive(Parser)]
#[command(
    name = "session-vault-sync",
    about = "Sync Task's Tracks folder into vault notes"
)]
struct Cli {
    /// Root of the Tracks folder to scan.
    #[arg(
        long,
        global = true,
        default_value = "/home/cody/Task/Assets/days-to-praise/Tracks"
    )]
    tracks_dir: PathBuf,

    /// Directory setlist notes are written into. Under the same Assets
    /// tree as `tracks_dir` — NOT Task's internal `.task/orgs/*/vault`
    /// storage — so setlists are visible next to the tracks they
    /// reference and travel with the org's own asset folder. Point this
    /// at a subfolder (e.g. `Assets/Setlists/Testing`) to keep test
    /// setlists separate from real ones.
    #[arg(
        long,
        global = true,
        default_value = "/home/cody/Task/Assets/days-to-praise/Setlists"
    )]
    setlists_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan the Tracks folder and (re)write a `type: song` note for every
    /// song found, INSIDE that song's own folder — see `vault::write_song_notes`.
    SyncLibrary,
    /// List the songs the scanner currently finds, without writing anything.
    ListSongs,
    /// Write a `type: setlist` note under `setlists_dir` linking the
    /// given songs, in order. Each song is matched by its title (e.g.
    /// "Holy Forever") or, if that's ambiguous between two artists, the
    /// full "Title - Artist" form — either way it's resolved to the
    /// canonical `[[Title - Artist]]` link the song's note is filed
    /// under. Must match songs already synced by `sync-library`.
    CreateSetlist {
        /// Name of the setlist note (also its filename). Name it after
        /// the day of the set, not a generic label — e.g. "Sunday,
        /// August 30 2026" beats "Sunday Set": setlists pile up fast and
        /// only the date actually distinguishes one from the next.
        name: String,
        /// Songs, in performance order.
        #[arg(required = true)]
        songs: Vec<String>,
    },
    /// Build a real REAPER project for one song: one track per stem,
    /// organized/routed/coloured by dynamic-template — see `rpp.rs`.
    /// Written into the song's own folder as `"{Title} - {Artist}.RPP"`.
    /// Pass `--live` to build the Master Setlist Template instead (see
    /// `live_bus.rs`) — a fixed live-FOH bus scheme (Click + Cues kept
    /// separate, Leads/Pads as their own buses) rather than
    /// dynamic-template's general studio taxonomy.
    BuildProject {
        /// The song to build (bare title, or "Title - Artist" if ambiguous).
        song: String,
        /// Build the Master Setlist Template (live) scheme instead of
        /// the dynamic-template studio one.
        #[arg(long)]
        live: bool,
    },
    /// Run `build-project` for every song currently in the library.
    BuildProjects {
        /// Build the Master Setlist Template (live) scheme instead of
        /// the dynamic-template studio one.
        #[arg(long)]
        live: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::SyncLibrary => {
            let songs = library::scan(&cli.tracks_dir);
            if songs.is_empty() {
                eprintln!("no songs found under {}", cli.tracks_dir.display());
                return std::process::ExitCode::FAILURE;
            }
            match vault::write_song_notes(&songs) {
                Ok(written) => {
                    println!("wrote {} song note(s):", written.len());
                    for path in written {
                        println!("  {}", path.display());
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("failed writing song notes: {e}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Command::ListSongs => {
            let songs = library::scan(&cli.tracks_dir);
            for song in &songs {
                let key = song.key.as_deref().unwrap_or("key unknown");
                println!(
                    "{} — {} ({}) [{} stems]",
                    song.title,
                    song.artist,
                    key,
                    song.stems.len()
                );
            }
            std::process::ExitCode::SUCCESS
        }
        Command::CreateSetlist { name, songs } => {
            let known = library::scan(&cli.tracks_dir);
            let mut links = Vec::with_capacity(songs.len());
            for wanted in &songs {
                match resolve_song(&known, wanted) {
                    Ok(song) => links.push(vault::song_link_name(song)),
                    Err(e) => {
                        eprintln!("{e}");
                        return std::process::ExitCode::FAILURE;
                    }
                }
            }

            match vault::write_setlist_note(&cli.setlists_dir, &name, &links) {
                Ok(path) => {
                    println!("wrote setlist note: {}", path.display());
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("failed writing setlist note: {e}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Command::BuildProject { song, live } => {
            let known = library::scan(&cli.tracks_dir);
            let song = match resolve_song(&known, &song) {
                Ok(song) => song,
                Err(e) => {
                    eprintln!("{e}");
                    return std::process::ExitCode::FAILURE;
                }
            };
            match build_one_project(song, live) {
                Ok(path) => {
                    println!("wrote project: {}", path.display());
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("failed building project for {}: {e}", song.title);
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Command::BuildProjects { live } => {
            let known = library::scan(&cli.tracks_dir);
            if known.is_empty() {
                eprintln!("no songs found under {}", cli.tracks_dir.display());
                return std::process::ExitCode::FAILURE;
            }
            let mut failed = 0;
            for song in &known {
                match build_one_project(song, live) {
                    Ok(path) => println!("wrote project: {}", path.display()),
                    Err(e) => {
                        eprintln!("failed building project for {}: {e}", song.title);
                        failed += 1;
                    }
                }
            }
            if failed > 0 {
                eprintln!("{failed} of {} projects failed", known.len());
                std::process::ExitCode::FAILURE
            } else {
                std::process::ExitCode::SUCCESS
            }
        }
    }
}

fn build_one_project(
    song: &LibrarySong,
    live: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let (text, suffix) = if live {
        (live_bus::build_live_rpp(song), " (Live)")
    } else {
        (rpp::build_organized_rpp(song)?, "")
    };
    let path = song
        .folder
        .join(format!("{}{suffix}.RPP", vault::song_link_name(song)));
    std::fs::write(&path, text)?;
    Ok(path)
}
