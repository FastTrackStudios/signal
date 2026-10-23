//! Signal's command core: the [`SignalActions`](crate::actions::SignalActions)
//! on architect's Action List.
//!
//! [`crate::actions`] declares what Signal can do (ids, names,
//! descriptions). This module says what each id *means* — a
//! [`SignalCommand`] — and puts all of them on an
//! [`ActionList`], so a key, a REAPER named command, a MIDI pedal
//! ([`crate::midi_actions`]) and a model over MCP (`architect-mcp`) run the
//! same action by the same id.
//!
//! What a command *does* depends on the host that runs it: the headless
//! engine (`studio-desktop --engine`) steps the guitar rig's setlist; the
//! REAPER extension builds demo content and learns macros in a project. A
//! host implements [`SignalCommandHost`]; [`register`] puts every command on
//! the list with that host behind it. A command a host cannot run is still
//! listed, disabled, with the host's reason — so the ids are the same
//! everywhere and a caller learns why one is unavailable here instead of
//! finding it missing.

use std::future::Future;
use std::sync::Arc;

use architect::action::{ActionError, ActionList, ActionMeta, Effect};
use facet::Facet;

use crate::actions::{
    DEV_GENERATE_SCENE_MIDI_ITEMS, DEV_LOAD_DEMO_GUITAR_PROFILE, DEV_LOAD_DEMO_GUITAR_RIG,
    DEV_LOAD_DEMO_SETLIST, MACRO_ADD, MACRO_ARM, MACRO_CLEAR, MACRO_DISARM,
    MACRO_REMOVE_LAST_POINT, MACRO_SET_MAX, MACRO_SET_MIN, MACRO_SET_POINT, NEXT_SECTION,
    NEXT_SONG, PLACE_SCENE_SWITCH, PLACE_SECTION_SWITCH, PLACE_SONG_SWITCH, PREVIOUS_SECTION,
    PREVIOUS_SONG, SWITCH_TO_VARIATION_BY_INDEX,
};

/// A variation number, 1 to [`Variation::COUNT`]: which of the active
/// collection's variations (a song's sections, a profile's patches, a
/// rig's scenes) to switch to. Only ever built in range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Variation(u8);

impl Variation {
    /// How many variations have an action of their own.
    pub const COUNT: u8 = 24;

    /// Variation `number` (1-based), or `None` outside 1 to [`Self::COUNT`].
    #[must_use]
    pub const fn new(number: u8) -> Option<Self> {
        if number >= 1 && number <= Self::COUNT {
            Some(Self(number))
        } else {
            None
        }
    }

    /// The 1-based number, as the action's name says it.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// The 0-based index into the active collection.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0).saturating_sub(1)
    }

    /// Every variation, 1 to [`Self::COUNT`].
    pub fn all() -> impl Iterator<Item = Self> {
        (1..=Self::COUNT).map(Self)
    }
}

/// One thing Signal can be told to do — the meaning of one
/// [`SignalActions`](crate::actions::SignalActions) id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignalCommand {
    /// The next song in the active setlist.
    NextSong,
    /// The previous song in the active setlist.
    PreviousSong,
    /// The next section of the active song.
    NextSection,
    /// The previous section of the active song.
    PreviousSection,
    /// The Nth variation of the active collection.
    SwitchToVariation(Variation),
    /// REAPER: build the demo guitar rig.
    DevLoadDemoGuitarRig,
    /// REAPER: build the demo guitar profile.
    DevLoadDemoGuitarProfile,
    /// REAPER: generate the scene-switch MIDI items.
    DevGenerateSceneMidiItems,
    /// REAPER: build the demo setlist.
    DevLoadDemoSetlist,
    /// REAPER: place a section-switch item at the edit cursor.
    PlaceSectionSwitch,
    /// REAPER: place a song-switch item at the edit cursor.
    PlaceSongSwitch,
    /// REAPER: place a scene-switch item at the edit cursor.
    PlaceSceneSwitch,
    /// Macro learn: arm the next macro.
    MacroArm,
    /// Macro learn: disarm and finalize.
    MacroDisarm,
    /// Macro learn: set a curve point.
    MacroSetPoint,
    /// Macro learn: remove the last curve point.
    MacroRemoveLastPoint,
    /// Macro learn: set the minimum.
    MacroSetMin,
    /// Macro learn: set the maximum.
    MacroSetMax,
    /// Macro learn: clear pending bindings.
    MacroClear,
    /// Add a macro knob to the active bank.
    MacroAdd,
}

/// Every command but the variation switches, in declaration order.
const FIXED: [SignalCommand; 19] = [
    SignalCommand::NextSong,
    SignalCommand::PreviousSong,
    SignalCommand::NextSection,
    SignalCommand::PreviousSection,
    SignalCommand::DevLoadDemoGuitarRig,
    SignalCommand::DevLoadDemoGuitarProfile,
    SignalCommand::DevGenerateSceneMidiItems,
    SignalCommand::DevLoadDemoSetlist,
    SignalCommand::PlaceSectionSwitch,
    SignalCommand::PlaceSongSwitch,
    SignalCommand::PlaceSceneSwitch,
    SignalCommand::MacroArm,
    SignalCommand::MacroDisarm,
    SignalCommand::MacroSetPoint,
    SignalCommand::MacroRemoveLastPoint,
    SignalCommand::MacroSetMin,
    SignalCommand::MacroSetMax,
    SignalCommand::MacroClear,
    SignalCommand::MacroAdd,
];

impl SignalCommand {
    /// How many commands there are: one per `SignalActions` action.
    pub const COUNT: usize = FIXED
        .len()
        .saturating_add(SWITCH_TO_VARIATION_BY_INDEX.len());

    /// Every command, one per `SignalActions` action.
    pub fn all() -> impl Iterator<Item = Self> {
        FIXED
            .into_iter()
            .chain(Variation::all().map(Self::SwitchToVariation))
    }

    /// The command action `id` names (`FTS_SIGNAL_NEXT_SONG`), if any.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().find(|command| command.meta().id == id)
    }

    /// The action that runs it: its id, name and description.
    #[must_use]
    #[expect(
        clippy::indexing_slicing,
        reason = "a Variation is 1 to 24 by construction, and the table has 24"
    )]
    pub fn meta(self) -> &'static ActionMeta {
        match self {
            Self::NextSong => &NEXT_SONG,
            Self::PreviousSong => &PREVIOUS_SONG,
            Self::NextSection => &NEXT_SECTION,
            Self::PreviousSection => &PREVIOUS_SECTION,
            Self::SwitchToVariation(v) => SWITCH_TO_VARIATION_BY_INDEX[v.index()],
            Self::DevLoadDemoGuitarRig => &DEV_LOAD_DEMO_GUITAR_RIG,
            Self::DevLoadDemoGuitarProfile => &DEV_LOAD_DEMO_GUITAR_PROFILE,
            Self::DevGenerateSceneMidiItems => &DEV_GENERATE_SCENE_MIDI_ITEMS,
            Self::DevLoadDemoSetlist => &DEV_LOAD_DEMO_SETLIST,
            Self::PlaceSectionSwitch => &PLACE_SECTION_SWITCH,
            Self::PlaceSongSwitch => &PLACE_SONG_SWITCH,
            Self::PlaceSceneSwitch => &PLACE_SCENE_SWITCH,
            Self::MacroArm => &MACRO_ARM,
            Self::MacroDisarm => &MACRO_DISARM,
            Self::MacroSetPoint => &MACRO_SET_POINT,
            Self::MacroRemoveLastPoint => &MACRO_REMOVE_LAST_POINT,
            Self::MacroSetMin => &MACRO_SET_MIN,
            Self::MacroSetMax => &MACRO_SET_MAX,
            Self::MacroClear => &MACRO_CLEAR,
            Self::MacroAdd => &MACRO_ADD,
        }
    }

    /// What running it does to the app. Every Signal command changes
    /// what plays or what is in the project, and none of them throws
    /// anything away.
    #[must_use]
    pub const fn effect(self) -> Effect {
        Effect::Mutating
    }
}

/// What a command did, said for whoever ran it (a model reads this).
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct CommandOutcome {
    /// One line: what is active or changed now (`"song 2/8: Gratitude"`).
    pub summary: String,
}

impl CommandOutcome {
    /// An outcome saying `summary`.
    #[must_use]
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
        }
    }
}

/// What runs Signal's commands in one host (the engine's rig, a REAPER
/// project).
pub trait SignalCommandHost: Send + Sync + 'static {
    /// Whether this host can run `command` right now: `Err(why)` when it
    /// cannot (`"runs in REAPER (signal-extension)"`). The action stays
    /// listed, disabled with that reason.
    ///
    /// # Errors
    ///
    /// Why `command` cannot run here.
    fn availability(&self, command: SignalCommand) -> Result<(), String>;

    /// Run `command`, answering what it did.
    fn run(
        &self,
        command: SignalCommand,
    ) -> impl Future<Output = Result<CommandOutcome, String>> + Send + 'static + use<Self>;
}

/// Put every Signal command on `list`, run by `host` and enabled as `host`
/// says.
///
/// # Errors
///
/// [`ActionError::Duplicate`] when one of Signal's ids is already on the
/// list.
pub fn register<H: SignalCommandHost>(list: &ActionList, host: &Arc<H>) -> Result<(), ActionError> {
    SignalCommand::all().try_for_each(|command| register_one(list, host, command))
}

/// A new list holding exactly Signal's commands, run by `host`.
///
/// # Errors
///
/// As [`register`] (not on a fresh list, in practice).
pub fn list<H: SignalCommandHost>(host: &Arc<H>) -> Result<ActionList, ActionError> {
    let list = ActionList::new();
    register(&list, host)?;
    Ok(list)
}

fn register_one<H: SignalCommandHost>(
    list: &ActionList,
    host: &Arc<H>,
    command: SignalCommand,
) -> Result<(), ActionError> {
    let meta = command.meta();
    let runner = Arc::clone(host);
    list.register_async(meta, command.effect(), move |(): ()| runner.run(command))?;
    let checker = Arc::clone(host);
    list.set_enabled(meta.id, move || checker.availability(command))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::SignalActionsActions;
    use std::collections::HashSet;
    use std::sync::Mutex;

    /// Records what it ran; refuses the macro commands.
    #[derive(Default)]
    struct Recorder {
        ran: Mutex<Vec<SignalCommand>>,
    }

    impl SignalCommandHost for Recorder {
        fn availability(&self, command: SignalCommand) -> Result<(), String> {
            match command {
                SignalCommand::MacroAdd => Err("not here".into()),
                _ => Ok(()),
            }
        }

        fn run(
            &self,
            command: SignalCommand,
        ) -> impl Future<Output = Result<CommandOutcome, String>> + Send + 'static + use<> {
            self.ran.lock().unwrap().push(command);
            std::future::ready(Ok(CommandOutcome::new(format!("{command:?}"))))
        }
    }

    #[test]
    fn every_declared_action_is_exactly_one_command() {
        let declared: HashSet<&str> = SignalActionsActions::all().iter().map(|m| m.id).collect();
        let commands: HashSet<&str> = SignalCommand::all().map(|c| c.meta().id).collect();
        assert_eq!(declared.len(), 43);
        assert_eq!(SignalCommand::COUNT, 43);
        assert_eq!(commands, declared);
        for command in SignalCommand::all() {
            assert_eq!(SignalCommand::from_id(command.meta().id), Some(command));
        }
    }

    #[test]
    fn the_list_is_sound() {
        let list = list(&Arc::new(Recorder::default())).unwrap();
        assert_eq!(list.len(), SignalCommand::COUNT);
        list.validate().unwrap();
        list.probe_hostile_args().unwrap();
    }

    #[test]
    fn running_an_id_runs_its_command_on_the_host() {
        let host = Arc::new(Recorder::default());
        let list = list(&host).unwrap();
        let out = block_on_ready(list.run_json_async(NEXT_SONG.id, b"{}")).unwrap();
        let out: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(out["summary"], "NextSong");
        let seventh = Variation::new(7).unwrap();
        block_on_ready(
            list.run_json_async(SignalCommand::SwitchToVariation(seventh).meta().id, b""),
        )
        .unwrap();
        assert_eq!(
            *host.ran.lock().unwrap(),
            [
                SignalCommand::NextSong,
                SignalCommand::SwitchToVariation(seventh)
            ]
        );
    }

    #[test]
    fn a_command_the_host_cannot_run_is_listed_disabled_with_its_reason() {
        let list = list(&Arc::new(Recorder::default())).unwrap();
        let info = list.info(MACRO_ADD.id).unwrap();
        assert!(!info.enabled);
        assert_eq!(info.disabled_reason.as_deref(), Some("not here"));
        let refused = block_on_ready(list.run_json_async(MACRO_ADD.id, b""));
        assert!(
            matches!(refused, Err(ActionError::Disabled { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn variations_are_only_built_in_range() {
        assert!(Variation::new(0).is_none());
        assert!(Variation::new(25).is_none());
        assert_eq!(Variation::new(24).map(Variation::index), Some(23));
    }

    /// Drive a future that never waits on anything (the recorder's are
    /// ready at once).
    fn block_on_ready<F: Future>(future: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut cx) {
                return out;
            }
        }
    }
}
