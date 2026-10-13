//! signal's rigs on tracks, by action: load one, set its tone, switch it
//! through the song.
//!
//! A track's rig is an `FTS Signal Rig` first in its chain
//! ([`signal_fx::signal_rig`]); its service, found by the id in the rig's
//! state, is what these drive — the same the window's footswitches and
//! browser drive. Loading a profile names the track for the stack it
//! plays (`Guitar Clean`) while it is its instrument's only part; a click
//! on a tone sets it (`TONE_SET`), a Shift-click puts a switch to it into
//! the song (`TONE_INSERT`: a step on the rig's Patch lane).

use daw::service::{FxChainContext, ProjectContext, TrackRef, Tracks};
use facet::Facet;
use signal_fx::signal_rig::RigState;

static RIG_LOAD: architect::action::ActionMeta = crate::actions::meta(
    "RIG_LOAD",
    "rig_load",
    "Load a signal rig",
    "Put signal's rig on `track` (an `FTS Signal Rig` first in its chain, or the one there) playing `profile` (the instrument's first unless given) and, given, `patch`. `instrument`: `guitar` or `bass` (from the track's name when left out — a bass's is the bass). While the track is its instrument's only part it is named for the stack it plays (`Guitar Clean`). Says the rig's profile, patch, stack and the patches it has. One undo step.",
    "Signal",
    "",
);

static TONE_SET: architect::action::ActionMeta = crate::actions::meta(
    "TONE_SET",
    "tone_set",
    "Set a rig's tone",
    "Switch the signal rig on `track` to `patch` (by name) now — a click on a tone in the switches or the browser: what plays from here, replacing the one that did. Says what plays.",
    "Signal",
    "",
);

static TONE_INSERT: architect::action::ActionMeta = crate::actions::meta(
    "TONE_INSERT",
    "tone_insert",
    "Insert a tone switch",
    "Put a switch to `patch` (by name) into the song on `track`'s signal rig at `at` — a bar (`33`, `33:2.5`), a time (`12.5s`) or a section (`CH 1`): a step on the rig's Patch lane, so it switches there every time the song plays. A Shift-click on a tone. One undo step.",
    "Signal",
    "",
);

#[derive(Facet)]
struct LoadArg {
    track: String,
    instrument: Option<String>,
    profile: Option<String>,
    patch: Option<String>,
}

#[derive(Facet)]
struct ToneArg {
    track: String,
    patch: String,
}

#[derive(Facet)]
struct InsertArg {
    track: String,
    patch: String,
    at: String,
}

/// A rig as an action says it.
#[derive(Facet)]
struct RigOut {
    track: String,
    profile: String,
    patch: Option<String>,
    stack: Option<String>,
    patches: Vec<String>,
}

/// The `FTS Signal Rig` on `guid`'s track, if it has one: its FX guid and
/// its state.
fn rig_on(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Option<(String, RigState)> {
    let fx =
        daw::service::Effects::list(daw, project.clone(), FxChainContext::Track(guid.to_owned()))
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(signal_fx::SIGNAL_RIG))?;
    let state = daw
        .save_plugin_state(&fx.guid)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<RigState>(&bytes).ok())?;
    Some((fx.guid, state))
}

/// The rig on `track` playing, by its id: its service.
fn playing_on(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Result<std::sync::Arc<signal_fx::signal_rig::Playing>, String> {
    let (_, state) = rig_on(daw, project, guid).ok_or("the track has no signal rig (RIG_LOAD)")?;
    signal_fx::signal_rig::rig(&state.id).ok_or_else(|| "its rig is not playing".to_owned())
}

/// What the rig on `guid` plays, said.
fn said(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Result<RigOut, String> {
    let playing = playing_on(daw, project, guid)?;
    let patches = playing.service_patches();
    let (profile, patch) = playing.playing();
    let stack = patch.as_ref().and_then(|name| {
        patches
            .iter()
            .find(|p| &p.name == name)
            .map(|p| p.stack.clone())
            .filter(|s| !s.is_empty())
    });
    let track = Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| t.guid == guid)
        .map_or_else(String::new, |t| t.name);
    Ok(RigOut {
        track,
        profile,
        patch,
        stack,
        patches: patches.into_iter().map(|p| p.name).collect(),
    })
}

/// The instrument a track is, by its name: a bass's is the bass.
fn instrument_of(name: &str) -> &'static str {
    if name.to_ascii_lowercase().contains("bass") {
        "bass"
    } else {
        "guitar"
    }
}

/// Whether `track` is its instrument's only part — named for it alone
/// (`Guitar`) or for the stack it plays (`Guitar Clean`), with no sibling
/// part beside it — and so named for its stack.
fn named_for_its_stack(all: &[daw_proto::Track], track: &daw_proto::Track, word: &str) -> bool {
    let name = track.name.trim();
    let named = name.eq_ignore_ascii_case(word)
        || name
            .get(..word.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(word))
            && name
                .get(word.len()..)
                .is_some_and(|rest| rest.starts_with(' '));
    let siblings = all
        .iter()
        .filter(|t| t.guid != track.guid && t.parent_guid == track.parent_guid)
        .any(|t| {
            t.name
                .to_ascii_lowercase()
                .starts_with(&word.to_ascii_lowercase())
        });
    named && !siblings
}

/// Load the rig on `guid`'s track as `arg` says; then name the track for
/// its stack while it is its instrument's only part.
fn load(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
    arg: &LoadArg,
) -> Result<RigOut, String> {
    let all = Tracks::all(daw, project.clone());
    let track = all
        .iter()
        .find(|t| t.guid == guid)
        .ok_or("the track went")?
        .clone();
    let instrument = arg
        .instrument
        .clone()
        .unwrap_or_else(|| instrument_of(&track.name).to_owned());
    let fx = match rig_on(daw, project, guid) {
        Some((fx, _)) => fx,
        None => daw::service::Effects::add_at(
            daw,
            project.clone(),
            daw_proto::fx::AddFxAtRequest {
                context: FxChainContext::Track(guid.to_owned()),
                name: signal_fx::SIGNAL_RIG.to_owned(),
                index: 0,
            },
        )
        .ok_or("the signal rig would not add")?,
    };
    let state = serde_json::json!({
        "instrument": instrument,
        "profile": arg.profile,
        "patch": arg.patch,
    });
    daw.apply_plugin_state(&fx, state.to_string().as_bytes())
        .map_err(|e| format!("the signal rig would not load: {e}"))?;
    let out = said(daw, project, guid)?;
    let word = if instrument == "bass" {
        "Bass"
    } else {
        "Guitar"
    };
    if let Some(stack) = &out.stack
        && named_for_its_stack(&all, &track, word)
    {
        let name = format!("{word} {stack}");
        daw::service::Tracks::rename(daw, project.clone(), TrackRef::Guid(guid.to_owned()), &name)
            .map_err(|e| format!("the track would not take its name: {e}"))?;
    }
    said(daw, project, guid)
}

pub(crate) fn register(list: &architect::action::ActionList) {
    use architect::action::Effect;
    list.register(&RIG_LOAD, Effect::Mutating, |a: LoadArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = crate::actions::resolve(daw, &project, &a.track)?;
        let out = load(daw, &project, &guid, &a)?;
        crate::studio::request_resync();
        crate::actions::list().changed("signal.rig", "a signal rig was loaded");
        Ok(out)
    });
    list.register(&TONE_SET, Effect::Mutating, |a: ToneArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = crate::actions::resolve(daw, &project, &a.track)?;
        let playing = playing_on(daw, &project, &guid)?;
        let index = playing
            .service_patches()
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(a.patch.trim()))
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| format!("the rig has no patch {:?}", a.patch))?;
        let applied = playing.select(index);
        if !applied {
            return Err(format!("the rig would not switch to {:?}", a.patch));
        }
        crate::actions::list().changed("signal.tone", "a rig's tone was set");
        said(daw, &project, &guid)
    });
    list.register(&TONE_INSERT, Effect::Mutating, |a: InsertArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = crate::actions::resolve(daw, &project, &a.track)?;
        // The patch must be one the rig has: the lane takes it by name.
        let playing = playing_on(daw, &project, &guid)?;
        if !playing
            .service_patches()
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(a.patch.trim()))
        {
            return Err(format!("the rig has no patch {:?}", a.patch));
        }
        let write = serde_json::json!({
            "track": guid,
            "envelope": format!("{}/Patch", signal_fx::SIGNAL_RIG),
            "points": [{ "at": a.at, "text": a.patch, "shape": "step" }],
        });
        crate::actions::list()
            .run_json("AUTOMATION_WRITE", write.to_string().as_bytes())
            .map_err(|e| format!("the switch would not write: {e}"))?;
        crate::actions::list().changed("signal.tone", "a tone switch was put in");
        said(daw, &project, &guid)
    });
}

/// What the browser shows for a track: its rig's profiles, stacks and
/// patches — or, with no rig yet, the profiles its instrument's library
/// has to load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigView {
    /// `guitar` or `bass`.
    pub instrument: String,
    /// Whether a rig is on the track.
    pub loaded: bool,
    /// The profile playing (empty with no rig).
    pub profile: String,
    pub profiles: Vec<String>,
    /// Each stack and its patches, each with whether it plays.
    pub stacks: Vec<(String, Vec<(String, bool)>)>,
}

/// What the browser shows for track `guid`, read from its rig (or its
/// instrument's library). `None` for a track that is no guitar or bass.
#[must_use]
pub fn browse(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Option<RigView> {
    let track = Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| t.guid == guid)?;
    if let Ok(playing) = playing_on(daw, project, guid) {
        use signal_fx::signal_guitar::proto::rig::Rig as _;
        let mut stacks: Vec<(String, Vec<(String, bool)>)> = Vec::new();
        for p in playing.service_patches() {
            let stack = if p.stack.is_empty() {
                "Patches".to_owned()
            } else {
                p.stack
            };
            match stacks.iter_mut().find(|(s, _)| *s == stack) {
                Some((_, patches)) => patches.push((p.name, p.active)),
                None => stacks.push((stack, vec![(p.name, p.active)])),
            }
        }
        return Some(RigView {
            instrument: playing.instrument.id().to_owned(),
            loaded: true,
            profile: playing.playing().0,
            profiles: playing
                .service
                .library()
                .profiles
                .into_iter()
                .map(|p| p.name)
                .collect(),
            stacks,
        });
    }
    let lower = track.name.to_ascii_lowercase();
    if !(lower.contains("guitar") || lower.contains("gtr") || lower.contains("bass")) {
        return None;
    }
    let instrument =
        signal_fx::signal_guitar::instrument::Instrument::from_id(instrument_of(&track.name));
    let home = signal_fx::signal_guitar::RigHome::new(
        instrument,
        signal_fx::signal_rig::library_dir(instrument),
    );
    Some(RigView {
        instrument: instrument.id().to_owned(),
        loaded: false,
        profile: String::new(),
        profiles: home
            .load_or_bootstrap()
            .profiles
            .into_iter()
            .map(|p| p.name)
            .collect(),
        stacks: Vec::new(),
    })
}
