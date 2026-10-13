//! `session-player` — plays a setlist's multitracks, natively.
//!
//! Loads songs (from a `type: setlist` note's `[[wikilink]]`s, or bare
//! titles for a quick test), builds each song's real Master Setlist
//! Template project in-process
//! (`session_vault_sync::live_bus::build_live_rpp` — the exact generator
//! `session-vault-sync build-project --live` writes to disk, just not
//! written here) and loads it via
//! `daw_standalone::project_loader::load_rpp_via_bay` — a real RPP
//! parse + audio materialize, the same pipeline that would load any
//! hand-built `.rpp` — then drives real transport/mute/solo through a
//! small stdin REPL. Only one song's audio engine is attached (real cpal
//! output) at a time — matching the "one render graph at a time" model
//! Task's own aspirational browser player already settled on for the
//! same reason (100+ live media elements at once is not a real design).

use std::io::Write as _;
use std::path::PathBuf;

use clap::Parser;
// `Transport` names both the RPC trait and a plain data struct in
// `daw_proto`, at the same top-level path — the struct's named
// re-export shadows the trait's glob one there. `transport::service`
// is `pub mod` specifically so the trait stays reachable; import from
// there instead of the shadowed `daw_proto::Transport`.
use daw_proto::Tracks as _;
use daw_proto::transport::service::Transport as _;
use daw_proto::{ProjectContext, Track, TrackRef};
use daw_standalone::Standalone;
use daw_standalone::audio_engine::AudioEngine;
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_via_bay;
use session_vault_sync::library::{self, LibrarySong};
use session_vault_sync::live_bus;
use session_vault_sync::setlist::{parse_setlist_links, resolve_song};

#[derive(Parser)]
#[command(
    name = "session-player",
    about = "Play a setlist's multitracks, natively"
)]
struct Cli {
    /// Root of the Tracks folder to scan.
    #[arg(long, default_value = "/home/cody/Task/Assets/days-to-praise/Tracks")]
    tracks_dir: PathBuf,

    /// A `type: setlist` note to load — its `[[Title - Artist]]` links,
    /// in order. Mutually exclusive with passing songs directly.
    #[arg(long)]
    setlist: Option<PathBuf>,

    /// Songs to play directly, in order (bare title, or "Title - Artist"
    /// if that title is ambiguous). Ignored if `--setlist` is given.
    songs: Vec<String>,
}

/// A song loaded into its own `daw-standalone` project, ready to attach
/// an audio engine to.
struct LoadedSong {
    song: LibrarySong,
    guid: String,
}

/// Build each song's Master Setlist Template in-memory and load it via
/// the real RPP-import pipeline — the player and `build-project --live`
/// now agree on grouping/colors/routing because they run the identical
/// generator, they just differ in whether the text ever touches disk.
fn load_songs(daw: &Standalone, songs: Vec<LibrarySong>) -> Vec<LoadedSong> {
    let mut loaded = Vec::with_capacity(songs.len());
    for song in songs {
        let rpp_text = live_bus::build_live_rpp(&song);
        // Stem paths in the generated RPP are relative to the song's own
        // folder (see `rpp.rs`'s doc comment on why) — the bay's file
        // resolver needs to be pointed there before the load call reads
        // them. `set_file_resolver` takes `&self`, so this is safe to
        // reset before every song, one `Standalone` for the whole set.
        daw.media_bay()
            .set_file_resolver(Box::new(ProjectRelativeResolver::new(song.folder.clone())));
        let synthetic_path = song.folder.join(format!("{}.rpp", song.title));

        match load_rpp_via_bay(
            daw,
            &song.title,
            synthetic_path.to_string_lossy().as_ref(),
            &rpp_text,
        ) {
            Ok((proj, audio)) => {
                println!(
                    "loaded {} ({} tracks, {} audio source(s) decoded, {} failed)",
                    song.title,
                    proj.track_count,
                    audio.loaded,
                    audio.failed.len()
                );
                for (take, err) in &audio.failed {
                    eprintln!("  ! {take}: {err}");
                }
                loaded.push(LoadedSong {
                    song,
                    guid: proj.project_guid,
                });
            }
            Err(e) => {
                eprintln!("failed to load {}: {e}", song.title);
            }
        }
    }
    loaded
}

fn attach(daw: &Standalone, loaded: &LoadedSong) -> Option<AudioEngine> {
    match daw.attach_audio_engine(&loaded.guid) {
        Ok(engine) => {
            println!("→ now playing: {}", loaded.song.title);
            Some(engine)
        }
        Err(e) => {
            eprintln!(
                "failed to open an audio device for {}: {e}",
                loaded.song.title
            );
            None
        }
    }
}

fn print_help() {
    println!(
        "\
commands:
  n, next          go to the next song
  p, prev          go to the previous song
  pl, play         toggle play/pause on the current song
  s, stop          stop the current song
  list             list the current song's tracks
  mute <name>      mute a track (matches any track whose name contains <name>)
  unmute <name>    unmute a track
  solo <name>      solo a track
  unsolo <name>    un-solo a track
  h, help          show this
  q, quit          quit"
    );
}

fn find_track(daw: &Standalone, guid: &str, name: &str) -> Option<Track> {
    let needle = name.to_lowercase();
    daw.all(ProjectContext::Project(guid.to_string()))
        .into_iter()
        .find(|t| t.name.to_lowercase().contains(&needle))
}

// `daw-standalone`'s loading/seeding/audio-engine calls spawn background
// tasks via `architect::platform::spawn` internally even though the
// surface API is plain sync — they panic ("no reactor running") without
// an ambient Tokio runtime, so this needs one even though nothing here
// ever `.await`s.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let known = library::scan(&cli.tracks_dir);
    if known.is_empty() {
        eprintln!("no songs found under {}", cli.tracks_dir.display());
        return std::process::ExitCode::FAILURE;
    }

    let wanted: Vec<String> = match &cli.setlist {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => parse_setlist_links(&text),
            Err(e) => {
                eprintln!("failed to read {}: {e}", path.display());
                return std::process::ExitCode::FAILURE;
            }
        },
        None => cli.songs.clone(),
    };
    if wanted.is_empty() {
        eprintln!("usage: session-player --setlist <note.md>  |  session-player <song> [song...]");
        return std::process::ExitCode::FAILURE;
    }

    let mut songs = Vec::with_capacity(wanted.len());
    for name in &wanted {
        match resolve_song(&known, name) {
            Ok(song) => songs.push(song.clone()),
            Err(e) => {
                eprintln!("{e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    let daw = Standalone::new();
    let loaded = load_songs(&daw, songs);
    if loaded.is_empty() {
        eprintln!("no songs loaded successfully");
        return std::process::ExitCode::FAILURE;
    }

    let mut current = 0usize;
    let mut engine = attach(&daw, &loaded[current]);
    let _ = daw.play(ProjectContext::Project(loaded[current].guid.clone()));

    print_help();
    loop {
        print!("[{}/{}] > ", current + 1, loaded.len());
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF (e.g. piped input ran out)
        }
        let line = line.trim();
        let mut parts = line.split_whitespace();
        let command = parts.next();
        let arg = parts.collect::<Vec<_>>().join(" ");

        match command {
            Some("n") | Some("next") => {
                if current + 1 >= loaded.len() {
                    println!("already at the last song");
                    continue;
                }
                let _ = daw.stop(ProjectContext::Project(loaded[current].guid.clone()));
                drop(engine.take());
                current += 1;
                engine = attach(&daw, &loaded[current]);
                let _ = daw.play(ProjectContext::Project(loaded[current].guid.clone()));
            }
            Some("p") | Some("prev") => {
                if current == 0 {
                    println!("already at the first song");
                    continue;
                }
                let _ = daw.stop(ProjectContext::Project(loaded[current].guid.clone()));
                drop(engine.take());
                current -= 1;
                engine = attach(&daw, &loaded[current]);
                let _ = daw.play(ProjectContext::Project(loaded[current].guid.clone()));
            }
            Some("pl") | Some("play") => {
                let _ = daw.play_pause(ProjectContext::Project(loaded[current].guid.clone()));
            }
            Some("s") | Some("stop") => {
                let _ = daw.stop(ProjectContext::Project(loaded[current].guid.clone()));
            }
            Some("list") => {
                for track in daw.all(ProjectContext::Project(loaded[current].guid.clone())) {
                    println!("  {}", track.name);
                }
            }
            Some(cmd @ ("mute" | "unmute" | "solo" | "unsolo")) => {
                if arg.is_empty() {
                    println!("usage: {cmd} <track name>");
                    continue;
                }
                match find_track(&daw, &loaded[current].guid, &arg) {
                    Some(track) => {
                        let ctx = ProjectContext::Project(loaded[current].guid.clone());
                        let target = TrackRef::Guid(track.guid.clone());
                        let result = match cmd {
                            "mute" => daw.set_muted(ctx, target, true),
                            "unmute" => daw.set_muted(ctx, target, false),
                            "solo" => daw.set_soloed(ctx, target, true),
                            _ => daw.set_soloed(ctx, target, false),
                        };
                        match result {
                            Ok(()) => println!("{cmd}: {}", track.name),
                            Err(e) => eprintln!("{cmd} failed: {e}"),
                        }
                    }
                    None => println!(
                        "no track matching \"{arg}\" in {}",
                        loaded[current].song.title
                    ),
                }
            }
            Some("h") | Some("help") => print_help(),
            Some("q") | Some("quit") => break,
            Some(other) => println!("unknown command: {other} (h for help)"),
            None => {}
        }
    }

    let _ = daw.stop(ProjectContext::Project(loaded[current].guid.clone()));
    drop(engine);
    std::process::ExitCode::SUCCESS
}
