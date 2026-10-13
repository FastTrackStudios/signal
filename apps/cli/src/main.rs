//! `session` — drive a real REAPER's setlist from the command line.
//!
//! The same surface `session-desktop`'s Recording Mode drives, without the
//! GUI: it dials a running REAPER extension's DAW socket
//! (`/tmp/fts-daw-{pid}.sock`), opens the `SetlistService` lanes, and calls
//! the same RPCs the transport bar does.
//!
//! # Why a CLI at all
//!
//! Opening an album is a batch operation — ten projects named by an `.RPL`
//! — and a batch operation wants a command, not a window. It is also the
//! only way to drive this over SSH, which is how these sessions actually
//! get set up on the studio Mac.
//!
//! ```text
//! session open "CRESCENDUM ALBUM LIST.RPL"   # launch/attach REAPER, open every project
//! session setlist                            # what is loaded, by song and section
//! session seek 3                             # jump to song 3, section 0
//! session seek 3 2                           # song 3, section 2
//! session status                             # which REAPER, and where the cursor is
//! ```

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

mod lyrics;
mod multitracks;
mod peaks;
mod proxies;
use session::SetlistServiceClient;

/// The dev-rig REAPER profile, matching `session-desktop`'s Recording Mode.
/// Only used when nothing is running and we have to launch one.
const REAPER_PROFILE: &str = "fts-dev";

const SOCKET_DIR: &str = "/tmp";
const SOCKET_PREFIX: &str = "fts-daw-";
const SOCKET_SUFFIX: &str = ".sock";

#[derive(Parser)]
#[command(
    name = "session",
    about = "Drive a real REAPER's setlist from the command line"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Open every project an `.RPL` names, then build a setlist from them.
    ///
    /// Launches REAPER (the `fts-dev` profile) if none is running.
    Open {
        /// The `.RPL` project list, or one or more `.RPP` files directly.
        paths: Vec<PathBuf>,
    },
    /// Print the loaded setlist: every song, and every section under it.
    Setlist,
    /// The song library in Task (keyflow's): setlists, and pulling a
    /// setlist's sessions down to open here. Server, org and token from
    /// FTS_TASK_SERVER / FTS_TASK_ORG / FTS_TASK_TOKEN.
    #[command(subcommand)]
    Library(LibraryCommand),
    /// Move the cursor to a song, and optionally a section within it.
    Seek {
        /// Song index, 0-based — as printed by `session setlist`.
        song: usize,
        /// Section index within that song, 0-based. Defaults to the first.
        section: Option<usize>,
    },
    /// Which REAPER this would talk to, and where its cursor is.
    Status,
    /// Write Ogg Vorbis proxies of a session's media: `Media/Bass.wav` →
    /// `Media/Proxies/Bass.ogg`, beside what they stand in for, so they
    /// sync with the session. What a browser or a phone plays from.
    ///
    /// Reads the sources from the `.RPP`; a proxy newer than its source is
    /// kept unless `--force`.
    Proxies {
        /// The session's `.RPP` (or the folder holding exactly one).
        session: PathBuf,
        /// libvorbis quality, -0.2..=1.0 (0.4 ≈ 128 kbps stereo).
        #[arg(long, default_value_t = 0.4)]
        quality: f32,
        /// Rewrite proxies that are already up to date.
        #[arg(long)]
        force: bool,
    },
    /// Write the waveform peaks cache for a session's media:
    /// `Media/Bass.wav` → `Media/Peaks/Bass.wav.sessionpeaks`, beside
    /// what it describes, so it syncs and streams with the session.
    ///
    /// The file is REAPER's own format under another name: a
    /// `Media/peaks/*.reapeaks` REAPER already wrote is adopted rather
    /// than recomputed, and a cache written here is one REAPER reads.
    /// Sources with no WAV are scanned from their Ogg proxy.
    Peaks {
        /// The session's `.RPP` (or the folder holding exactly one).
        session: PathBuf,
        /// Rescan sources whose cache is already up to date.
        #[arg(long)]
        force: bool,
    },
    /// Turn folders of multitracks into sessions on the grid: the click
    /// stem gives the tempo and where bar one is, every stem is trimmed to
    /// it, and the guide's cues become regions and a chart to start from.
    Import {
        /// A song's folder, or one holding several.
        folders: Vec<PathBuf>,
        /// Where the sessions are written.
        #[arg(long, default_value = "../sessions")]
        out: PathBuf,
        /// Write over a session that is already there.
        #[arg(long)]
        force: bool,
    },
    /// Write an Ogg Vorbis copy of the FTS-GUIDE sample library — the
    /// same layout, each `.wav` a `.ogg` — for a browser to stream the
    /// click, the count and the cues from.
    GuideLibrary {
        /// The WAV library (`~/.config/fts/guide-samples`).
        library: PathBuf,
        /// Where the Ogg copy goes.
        out: PathBuf,
        /// libvorbis quality, -0.2..=1.0. Higher than a stem's: a click is
        /// all transient.
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
    /// Line-synced lyrics for songs, as `<Song>.lrc` beside each `.RPP`.
    Lyrics {
        #[command(subcommand)]
        command: LyricsCommand,
    },
}

#[derive(Subcommand)]
enum LyricsCommand {
    /// Find line-synced lyrics for each song and write `<Song>.lrc`.
    ///
    /// Title and artists come from the song's chart (`<Song>.kf`, first
    /// line `Title - Artist1, Artist2`); each artist is searched, then the
    /// title alone. Of the versions found, the one whose length is closest
    /// to the chart's SONGSTART→SONGEND is written. Provider: LRCLIB.
    Fetch {
        /// Songs' `.RPP` files (or folders holding exactly one).
        paths: Vec<PathBuf>,
        /// Search for this title instead of the chart's.
        #[arg(long)]
        title: Option<String>,
        /// Search under this artist instead of the chart's (repeatable).
        #[arg(long = "artist")]
        artists: Vec<String>,
        /// Take this exact LRCLIB record, skipping the search.
        #[arg(long)]
        lrclib_id: Option<String>,
        /// Take candidate N (as listed, from 1) instead of the closest length.
        #[arg(long)]
        pick: Option<usize>,
        /// Replace a `.lrc` that is already there.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    init_tracing();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(run(cli.command))
}

/// Diagnostics go to stderr through `tracing`, quiet unless asked:
/// `RUST_LOG=info` prints each command's wide event (e.g. one
/// `lyrics.fetch` span per song) as it closes.
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .try_init();
}

#[derive(clap::Subcommand)]
enum LibraryCommand {
    /// Every setlist in the org, with its songs.
    List,
    /// Download a setlist's sessions (proxies, charts, lyrics, the
    /// `.session`) and write a `.setlist` the app opens.
    Pull {
        /// The setlist's title (`Worship Set`).
        setlist: String,
        /// Where to put it. Default: the platform cache, under the org.
        #[arg(long)]
        into: Option<PathBuf>,
        /// The original media too, not only the proxies.
        #[arg(long)]
        originals: bool,
    },
}

async fn library(command: LibraryCommand) -> eyre::Result<()> {
    let lib = session_library::Library::from_env();
    let setlists = lib.setlists().await?;
    match command {
        LibraryCommand::List => {
            for list in &setlists {
                println!("{} ({} songs)", list.title, list.songs.len());
                for song in &list.songs {
                    let by = song.writers.join(", ");
                    println!(
                        "  song:{:<28} {}{}",
                        song.slug,
                        song.title,
                        if by.is_empty() {
                            String::new()
                        } else {
                            format!(" — {by}")
                        }
                    );
                }
            }
        }
        LibraryCommand::Pull {
            setlist,
            into,
            originals,
        } => {
            let list = setlists
                .iter()
                .find(|l| l.title == setlist)
                .ok_or_else(|| eyre::eyre!("no setlist `{setlist}` in {}", lib.org))?;
            let into = into.unwrap_or_else(|| {
                std::env::temp_dir()
                    .join("fts-session-library")
                    .join(&lib.org)
            });
            let file = lib.pull_setlist(list, &into, originals).await?;
            println!("{}", file.display());
        }
    }
    Ok(())
}

async fn run(command: Command) -> eyre::Result<()> {
    match command {
        Command::Library(command) => library(command).await,
        Command::Open { paths } => open(&paths).await,
        Command::Setlist => setlist().await,
        Command::Seek { song, section } => seek(song, section.unwrap_or(0)).await,
        Command::Status => status().await,
        Command::Proxies {
            session,
            quality,
            force,
        } => proxies::write(&session, quality, force),
        Command::Peaks { session, force } => peaks::write(&session, force),
        Command::GuideLibrary {
            library,
            out,
            quality,
        } => proxies::guide_library(&library, &out, quality),
        Command::Import {
            folders,
            out,
            force,
        } => import(&folders, &out, force),
        Command::Lyrics {
            command:
                LyricsCommand::Fetch {
                    paths,
                    title,
                    artists,
                    lrclib_id,
                    pick,
                    force,
                },
        } => lyrics::fetch(&lyrics::FetchArgs {
            paths,
            title,
            artists,
            lrclib_id,
            pick,
            force,
        }),
    }
}

// ── Discovery and connection ────────────────────────────────────────────
//
// Deliberately the same shape as `session-desktop`'s `reaper_engine`: a
// socket per live REAPER, named by pid, newest first. Kept here rather than
// shared because the desktop's version owns a long-lived connection and a
// reconnect supervisor; a CLI process dials once and exits.

/// Every live REAPER publishing a DAW socket, newest first. A socket whose
/// process is gone is skipped, not deleted — it is not ours to unlink.
fn discover_sockets() -> Vec<(u32, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(SOCKET_DIR) else {
        return Vec::new();
    };
    let mut found: Vec<(u32, PathBuf)> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let pid: u32 = path
                .file_name()?
                .to_str()?
                .strip_prefix(SOCKET_PREFIX)?
                .strip_suffix(SOCKET_SUFFIX)?
                .parse()
                .ok()?;
            process_alive(pid).then_some((pid, path))
        })
        .collect();
    found.sort_by_key(|(pid, _)| std::cmp::Reverse(*pid));
    found
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // Signal 0 is the standard existence probe (`kill(2)`): it never
    // actually signals the process.
    unsafe { libc::kill(i32::try_from(pid).unwrap_or(-1), 0) == 0 }
}

/// Open the `SetlistService` lanes on a REAPER's socket.
async fn connect_setlist(socket: &Path) -> eyre::Result<SetlistServiceClient> {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|e| eyre::eyre!("connecting to {}: {e}", socket.display()))?;
    let link = vox_stream::StreamLink::unix(stream);
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("vox handshake with {}: {e:?}", socket.display()))?;
    let client = connection
        .open_lane::<SetlistServiceClient>()
        .await
        .map_err(|e| {
            eyre::eyre!(
                "opening the SetlistService lane on {}: {e:?}\n\
                 (is this REAPER running the FTS extension?)",
                socket.display()
            )
        })?;
    // The connection owns the link; leak it so the client stays usable for
    // the rest of this short-lived process rather than tearing down mid-call.
    std::mem::forget(connection);
    Ok(client)
}

/// The newest live REAPER's `SetlistService`, or a clear reason there isn't one.
async fn connect() -> eyre::Result<SetlistServiceClient> {
    let (_, socket) = discover_sockets().into_iter().next().ok_or_else(|| {
        eyre::eyre!(
            "no running REAPER found (no {SOCKET_DIR}/{SOCKET_PREFIX}*{SOCKET_SUFFIX}).\n\
             Start REAPER with the FTS extension loaded, or run `session open <list.RPL>`\n\
             to launch one."
        )
    })?;
    connect_setlist(&socket).await
}

// ── Commands ────────────────────────────────────────────────────────────

/// Resolve the projects to open: an `.RPL` expands to the list it names,
/// anything else is taken as a project path directly.
fn resolve_projects(paths: &[PathBuf]) -> eyre::Result<Vec<PathBuf>> {
    if paths.is_empty() {
        eyre::bail!("nothing to open — pass an .RPL project list, or .RPP paths");
    }
    let mut out = Vec::new();
    for path in paths {
        let is_rpl = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rpl"));
        if is_rpl {
            let listed = dawfile_reaper::setlist_rpp::parse_rpl(path)
                .map_err(|e| eyre::eyre!("reading {}: {e}", path.display()))?;
            out.extend(listed);
        } else {
            out.push(path.clone());
        }
    }
    Ok(out)
}

async fn open(paths: &[PathBuf]) -> eyre::Result<()> {
    let projects = resolve_projects(paths)?;
    println!("{} project(s) to open", projects.len());

    // Every path is checked before REAPER is touched. Opening half an album
    // and then failing leaves a worse state than not starting.
    let missing: Vec<&PathBuf> = projects.iter().filter(|p| !p.exists()).collect();
    if !missing.is_empty() {
        for path in &missing {
            eprintln!("  missing: {}", path.display());
        }
        eyre::bail!(
            "{} of {} projects do not exist — nothing was opened",
            missing.len(),
            projects.len()
        );
    }

    let daw = match discover_sockets().into_iter().next() {
        Some((pid, socket)) => {
            println!("attaching to REAPER (pid {pid})");
            daw::cli::connect(Some(socket)).await?.daw
        }
        None => {
            println!("no REAPER running — launching the '{REAPER_PROFILE}' profile");
            let (connection, pid, socket) = daw::cli::launch_and_connect(REAPER_PROFILE).await?;
            println!("  launched pid {pid} ({})", socket.display());
            connection.daw
        }
    };

    for path in &projects {
        let name = dawfile_reaper::setlist_rpp::song_name_from_path(path);
        daw.open_project(path.to_string_lossy().into_owned())
            .await
            .map_err(|e| eyre::eyre!("opening '{name}' ({}): {e:?}", path.display()))?;
        println!("  opened {name}");
    }

    // The projects are open as REAPER tabs; this is what turns them into a
    // setlist the transport can page through.
    let client = connect().await?;
    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;
    println!();
    print_setlist(&client).await
}

async fn setlist() -> eyre::Result<()> {
    let client = connect().await?;
    print_setlist(&client).await
}

async fn print_setlist(client: &SetlistServiceClient) -> eyre::Result<()> {
    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist: {e:?}"))?;
    if setlist.songs.is_empty() {
        println!("setlist is empty — `session open <list.RPL>` to build one");
        return Ok(());
    }
    println!("{} song(s):", setlist.songs.len());
    for (i, song) in setlist.songs.iter().enumerate() {
        println!("  {i:>2}  {}", song.name);
        for (j, section) in song.sections.iter().enumerate() {
            println!("        {j:>2}  {}", section.name);
        }
    }
    Ok(())
}

async fn seek(song: usize, section: usize) -> eyre::Result<()> {
    let client = connect().await?;
    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist: {e:?}"))?;
    // Report the name being jumped to, not just the indices — an off-by-one
    // is otherwise invisible until the wrong song starts playing.
    let name = setlist
        .songs
        .get(song)
        .map_or_else(|| "<out of range>".to_string(), |s| s.name.clone());
    client
        .seek_to_section(song, section)
        .await
        .map_err(|e| eyre::eyre!("seek_to_section({song}, {section}): {e:?}"))?;

    println!("song {song} / section {section} — {name}");

    // Read the cursor back rather than trusting the call. A `seek_to_section`
    // that returns Ok has been accepted, not necessarily applied — it bounces
    // to REAPER's main thread and the cursor update is a second, independent
    // hop. Reporting success from the return value alone would hide exactly
    // the failure this command exists to catch. (`active_song` is on the RPC
    // client; per-section confirmation needs the subscribe stream, which a
    // one-shot CLI process has no good place to pump.)
    match client.active_song().await {
        Ok(active) if active.name == name => println!("  REAPER is on this song"),
        Ok(active) => println!(
            "  WARNING: REAPER is on '{}', not '{name}' — the seek did not land",
            active.name
        ),
        Err(e) => println!("  (could not read REAPER's position back: {e:?})"),
    }
    Ok(())
}

async fn status() -> eyre::Result<()> {
    let sockets = discover_sockets();
    if sockets.is_empty() {
        println!("no running REAPER");
        return Ok(());
    }
    for (pid, socket) in &sockets {
        println!("REAPER pid {pid}  {}", socket.display());
    }
    let client = connect().await?;
    // A REAPER that is up but has no setlist yet is the normal state right
    // after launch, not an error — `setlist()` reports it as NotFound.
    let Ok(setlist) = client.setlist().await else {
        println!("connected, but no setlist built yet — `session open <list.RPL>`");
        return Ok(());
    };
    println!("{} song(s) in the setlist", setlist.songs.len());
    match client.active_song().await {
        Ok(active) => println!("cursor: {}", active.name),
        Err(_) => println!("cursor: nowhere yet — `session seek <song>`"),
    }
    Ok(())
}

/// `session import`: each folder named, or each song folder inside one.
fn import(folders: &[PathBuf], out: &Path, force: bool) -> eyre::Result<()> {
    let mut songs: Vec<PathBuf> = Vec::new();
    for folder in folders {
        // A folder of folders (the downloads directory) imports each song
        // in it; a folder with stems in it is one song.
        let holds_stems = std::fs::read_dir(folder)?.flatten().any(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
        });
        if holds_stems {
            songs.push(folder.clone());
        } else {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(folder)?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            inside.sort();
            songs.extend(inside);
        }
    }
    let mut failed = Vec::new();
    for song in &songs {
        if let Err(e) = multitracks::import(song, out, force) {
            failed.push(format!("{}: {e}", song.display()));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} did not import:\n{}",
            failed.len(),
            failed.join("\n")
        ))
    }
}
