//! Signal's command core run by the guitar rig, and served to a model.
//!
//! `signal_proto::commands` is every `FTS_SIGNAL_*` action as a typed
//! [`SignalCommand`]; [`RigCommands`] is what each means on this rig — the
//! setlist's songs, the up song's sections ("parts" here), and the
//! variation switches (a section with a song up, a patch without). The
//! REAPER-only commands (demo content, switch items, macro learn) stay on
//! the list, disabled with [`REAPER_ONLY`] as the reason.
//!
//! [`serve_mcp`] serves such a list over `architect-mcp`'s HTTP transport on
//! loopback, behind a bearer token it mints — what `studio-desktop
//! --engine` does when asked to (`--mcp` / `SIGNAL_MCP=1`); it needs the
//! `mcp` feature, so a UI build of the rig never links the HTTP server.

use std::future::Future;
#[cfg(feature = "mcp")]
use std::io;

use architect::action::{ActionError, ActionList};
#[cfg(feature = "mcp")]
use architect_mcp::http::Serving;
use signal_proto::commands::{CommandOutcome, SignalCommand, SignalCommandHost, Variation};

use crate::GuitarRigBackend;
use crate::proto::rig::Rig;
use crate::proto::{Applied, PerformanceModel};

/// Why a REAPER-only command is disabled on the rig.
pub const REAPER_ONLY: &str = "runs in REAPER (the signal-extension), not on the rig";

/// The perform-grid mode in which the setlist drives the rig
/// ([`PerformanceModel::perform_mode`]).
const SETLIST_MODE: u32 = 2;

/// The guitar rig as the host of Signal's commands.
#[derive(Clone)]
pub struct RigCommands {
    rig: GuitarRigBackend,
}

impl RigCommands {
    /// Commands run on `rig` (a clone shares the running rig).
    #[must_use]
    pub const fn new(rig: GuitarRigBackend) -> Self {
        Self { rig }
    }
}

impl SignalCommandHost for RigCommands {
    fn availability(&self, command: SignalCommand) -> Result<(), String> {
        if runs_on_rig(command) {
            Ok(())
        } else {
            Err(REAPER_ONLY.to_owned())
        }
    }

    fn run(
        &self,
        command: SignalCommand,
    ) -> impl Future<Output = Result<CommandOutcome, String>> + Send + 'static + use<> {
        let rig = self.rig.clone();
        async move { run_on(&rig, command) }
    }
}

/// A list of exactly Signal's commands, run on `rig`.
///
/// # Errors
///
/// As `signal_proto::commands::list` (not on a fresh list, in practice).
pub fn command_list(rig: &GuitarRigBackend) -> Result<ActionList, ActionError> {
    signal_proto::commands::list(&std::sync::Arc::new(RigCommands::new(rig.clone())))
}

/// Whether the rig has a meaning for `command`.
#[must_use]
pub const fn runs_on_rig(command: SignalCommand) -> bool {
    matches!(
        command,
        SignalCommand::NextSong
            | SignalCommand::PreviousSong
            | SignalCommand::NextSection
            | SignalCommand::PreviousSection
            | SignalCommand::SwitchToVariation(_)
    )
}

/// Run `command` on `rig` and say where it left the rig.
fn run_on(rig: &GuitarRigBackend, command: SignalCommand) -> Result<CommandOutcome, String> {
    match command {
        SignalCommand::NextSong => {
            Rig::next_song(rig);
            Ok(song_outcome(&Rig::perf(rig)))
        }
        SignalCommand::PreviousSong => {
            Rig::prev_song(rig);
            Ok(song_outcome(&Rig::perf(rig)))
        }
        SignalCommand::NextSection => step_section(rig, 1),
        SignalCommand::PreviousSection => step_section(rig, -1),
        SignalCommand::SwitchToVariation(variation) => switch_variation(rig, variation),
        _ => Err(REAPER_ONLY.to_owned()),
    }
}

fn step_section(rig: &GuitarRigBackend, dir: i32) -> Result<CommandOutcome, String> {
    taken(Rig::step_part(rig, dir, false))?;
    section_outcome(&Rig::perf(rig))
}

/// The Nth section of the up song in setlist mode, else the Nth patch of
/// the profile.
fn switch_variation(
    rig: &GuitarRigBackend,
    variation: Variation,
) -> Result<CommandOutcome, String> {
    let perf = Rig::perf(rig);
    let index = variation_index(variation)?;
    if plays_a_setlist(&perf) {
        check_in_range(variation, perf.parts.len(), "sections")?;
        taken(Rig::select_part(rig, index))?;
        return section_outcome(&Rig::perf(rig));
    }
    taken(Rig::select_patch(rig, index))?;
    Ok(patch_outcome(variation))
}

const fn plays_a_setlist(perf: &PerformanceModel) -> bool {
    perf.perform_mode == SETLIST_MODE && !perf.songs.is_empty()
}

fn variation_index(variation: Variation) -> Result<u32, String> {
    u32::try_from(variation.index()).map_err(|e| e.to_string())
}

fn check_in_range(variation: Variation, available: usize, what: &str) -> Result<(), String> {
    if variation.index() < available {
        Ok(())
    } else {
        Err(format!(
            "variation {} asked for, the song has {available} {what}",
            variation.number()
        ))
    }
}

/// The rig's `Applied` as a `Result`.
fn taken(applied: Applied) -> Result<(), String> {
    if applied.ok {
        Ok(())
    } else {
        Err(applied.message)
    }
}

/// `"song 2/8: Gratitude"` — or that no setlist has songs.
#[must_use]
pub fn song_outcome(perf: &PerformanceModel) -> CommandOutcome {
    numbered(&perf.songs, perf.song_index).map_or_else(
        || CommandOutcome::new("no song: the active setlist is empty"),
        |(n, song)| CommandOutcome::new(format!("song {n}/{}: {}", perf.songs.len(), song.name)),
    )
}

/// `"section 3/6: Chorus 1"`.
///
/// # Errors
///
/// The song has no section at the rig's section index.
pub fn section_outcome(perf: &PerformanceModel) -> Result<CommandOutcome, String> {
    let (n, part) = numbered(&perf.parts, perf.part_index)
        .ok_or_else(|| "the song up has no sections".to_owned())?;
    Ok(CommandOutcome::new(format!(
        "section {n}/{}: {}",
        perf.parts.len(),
        part.name
    )))
}

/// The item at 0-based `index`, with the 1-based number a person reads.
fn numbered<T>(items: &[T], index: u32) -> Option<(usize, &T)> {
    let at = usize::try_from(index).ok()?;
    items.get(at).map(|item| (at.saturating_add(1), item))
}

fn patch_outcome(variation: Variation) -> CommandOutcome {
    CommandOutcome::new(format!("patch {}", variation.number()))
}

/// A new bearer token: 244 bits from the OS's random source, as hex.
#[must_use]
pub fn mint_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Serve `list` to a model over MCP's HTTP transport, on loopback only.
///
/// It listens on `127.0.0.1:port` (`0` for any free port), behind `token`
/// (`Authorization: Bearer …`), on the runtime this is awaited on. Binds
/// and spawns only; the caller says where it is ([`Serving::publish`]).
///
/// # Errors
///
/// The port could not be bound.
#[cfg(feature = "mcp")]
pub async fn serve_mcp(list: ActionList, port: u16, token: String) -> io::Result<Serving> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let addr = listener.local_addr()?;
    let server =
        architect_mcp::Server::new("FastTrackStudio Signal", env!("CARGO_PKG_VERSION"), list)
            .with_instructions(INSTRUCTIONS);
    let task = tokio::spawn(architect_mcp::http::serve(
        listener,
        server,
        Some(token.clone()),
    ));
    Ok(Serving {
        url: format!("http://{addr}/mcp"),
        token: Some(token),
        addr,
        task,
    })
}

/// What a model is told about the engine when it connects.
#[cfg(feature = "mcp")]
const INSTRUCTIONS: &str = "This is the FastTrackStudio Signal engine: the live guitar rig a \
player performs through. FTS_SIGNAL_NEXT_SONG / FTS_SIGNAL_PREVIOUS_SONG step the active setlist; \
FTS_SIGNAL_NEXT_SECTION / FTS_SIGNAL_PREVIOUS_SECTION step the song's sections; \
FTS_SIGNAL_SWITCH_TO_VARIATION_<N> jumps to section N of the song (or patch N with no setlist \
playing). Each answers with where the rig is now. The other FTS_SIGNAL_* actions run in REAPER and \
are listed disabled here. These change what the audience hears: run them when the player asks.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{PerfPart, SongSlot};

    fn songs(names: &[&str]) -> Vec<SongSlot> {
        names
            .iter()
            .map(|n| SongSlot {
                name: (*n).to_owned(),
                ..SongSlot::default()
            })
            .collect()
    }

    #[test]
    fn the_rig_runs_navigation_and_refuses_the_reaper_commands() {
        let runs: Vec<SignalCommand> = SignalCommand::all().filter(|c| runs_on_rig(*c)).collect();
        assert_eq!(runs.len(), 4 + usize::from(Variation::COUNT));
        assert!(!runs_on_rig(SignalCommand::MacroArm));
        assert!(!runs_on_rig(SignalCommand::DevLoadDemoSetlist));
    }

    #[test]
    fn a_song_outcome_names_the_song_and_where_it_is_in_the_set() {
        let perf = PerformanceModel {
            songs: songs(&["Opener", "Gratitude", "Closer"]),
            song_index: 1,
            ..PerformanceModel::default()
        };
        assert_eq!(song_outcome(&perf).summary, "song 2/3: Gratitude");
        assert_eq!(
            song_outcome(&PerformanceModel::default()).summary,
            "no song: the active setlist is empty"
        );
    }

    #[test]
    fn a_section_outcome_names_the_section() {
        let perf = PerformanceModel {
            parts: vec![
                PerfPart {
                    name: "Intro".into(),
                    ..PerfPart::default()
                },
                PerfPart {
                    name: "Verse".into(),
                    ..PerfPart::default()
                },
            ],
            part_index: 1,
            ..PerformanceModel::default()
        };
        assert_eq!(
            section_outcome(&perf).unwrap().summary,
            "section 2/2: Verse"
        );
        assert!(section_outcome(&PerformanceModel::default()).is_err());
    }

    #[test]
    fn a_variation_past_the_song_is_refused() {
        let third = Variation::new(3).unwrap();
        assert!(check_in_range(third, 3, "sections").is_ok());
        assert!(check_in_range(third, 2, "sections").is_err());
    }

    #[test]
    fn tokens_are_long_and_fresh() {
        let (a, b) = (mint_token(), mint_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
