//! Recording, as actions: arm a track to an input (`TRACK_ARM`), and start
//! or stop recording the armed tracks (`TRANSPORT_RECORD`).
//!
//! The engine does the recording (`daw::standalone::recording`): while the
//! transport records, every armed track's input is written to a WAV in the
//! song's `Media/` folder, and on stop each one goes on its track as a new
//! take on a fixed lane — the lowest take lane free over the recorded
//! range, else a new one named with the next number; the newest take plays
//! unless a comp lane holds something. The window reads the lanes back
//! from the engine like any other edit (`crate::studio::request_resync`).
//!
//! The record button, the record key (`SESSION_TOGGLE_RECORD`, carried out
//! by the window) and a model all come here: one path.

use architect::action::{ActionList, Effect};
use daw::service::{ProjectContext, TrackRef, Tracks};
use daw_proto::RecordInput;
use daw_proto::transport::service::Transport as TransportService;
use facet::Facet;

pub(crate) static TRACK_ARM: architect::action::ActionMeta = crate::actions::meta(
    "TRACK_ARM",
    "track_arm",
    "Arm a track",
    "Arm `track` (a path, name or GUID) to record, or disarm it: `on` true or false, toggled when left out. `input`: the hardware input it records, from 1 (`audio 1` in SESSION_TRACKS) — when arming a track that records nothing yet, input 1. Says what it is armed to now.",
    "Record",
    "",
);

pub(crate) static TRANSPORT_RECORD: architect::action::ActionMeta = crate::actions::meta(
    "TRANSPORT_RECORD",
    "transport_record",
    "Record",
    "Start recording the armed tracks from where the transport is, or stop: `on` true or false, toggled when left out. Stopping puts each armed track's recording on it as a new take, on the lowest take lane free over what was recorded (else a new lane), the newest take playing unless a comp holds something there. Says whether it records now and, on stop, the takes it placed.",
    "Transport",
    "",
);

/// Which track, armed or not, to what.
#[derive(Facet)]
struct ArmArg {
    track: String,
    on: Option<bool>,
    /// The hardware input, from 1.
    input: Option<u32>,
}

/// What a track is armed to now.
#[derive(Facet)]
struct ArmOut {
    track: String,
    armed: bool,
    /// The hardware input it records, from 1, when it records one.
    input: Option<u32>,
}

/// Start or stop recording; toggled when `on` is left out.
#[derive(Facet)]
struct RecordArg {
    on: Option<bool>,
}

/// Whether it records now, and the takes a stop placed.
#[derive(Facet)]
struct RecordOut {
    recording: bool,
    /// Some armed track is being captured.
    capturing: bool,
    takes: Vec<TakeRow>,
}

/// One take a recording placed.
#[derive(Facet)]
struct TakeRow {
    track: String,
    /// Its fixed lane, from 0.
    lane: u32,
    /// The lane's name: `1`, `2`, …
    lane_name: String,
    /// The recording it plays: its file's name.
    source: String,
    position: f64,
    length: f64,
}

/// The hardware input `input` records, from 1, if it records one.
const fn input_of(input: RecordInput) -> Option<u32> {
    match input {
        RecordInput::Audio { channel } => channel.checked_add(1),
        RecordInput::None | RecordInput::Midi { .. } | RecordInput::Raw(_) => None,
    }
}

fn arm(a: &ArmArg) -> Result<ArmOut, String> {
    let (daw, project) = crate::actions::song()?;
    let guid = crate::actions::resolve(daw, &project, &a.track)?;
    let at = || TrackRef::Guid(guid.clone());
    let failed = |e: daw_proto::DawError| e.to_string();
    let before = Tracks::get(daw, project.clone(), at()).ok_or("the track went")?;
    let on = a.on.unwrap_or(!before.armed);
    // REAPER's default for a track armed with no input: input 1.
    let input = a
        .input
        .or_else(|| (on && input_of(before.record_input).is_none()).then_some(1));
    if let Some(input) = input {
        let channel = input
            .checked_sub(1)
            .ok_or("inputs count from 1: `input` 1 is the first")?;
        Tracks::set_record_input(daw, project.clone(), at(), RecordInput::Audio { channel })
            .map_err(failed)?;
    }
    Tracks::set_armed(daw, project.clone(), at(), on).map_err(failed)?;
    let now = Tracks::get(daw, project, at()).ok_or("the track went")?;
    crate::studio::request_resync();
    crate::actions::list().changed("tracks", &guid);
    Ok(ArmOut {
        track: now.name,
        armed: now.armed,
        input: input_of(now.record_input),
    })
}

fn run(on: Option<bool>) -> Result<RecordOut, String> {
    let (daw, project) = crate::actions::song()?;
    let ProjectContext::Project(guid) = project.clone() else {
        return Err("no song open".to_owned());
    };
    let failed = |e: daw_proto::DawError| e.to_string();
    let was = TransportService::is_recording(daw, project.clone());
    let want = on.unwrap_or(!was);
    if want && !was {
        TransportService::record(daw, project).map_err(failed)?;
        crate::actions::list().changed("transport", "recording");
        return Ok(RecordOut {
            recording: true,
            capturing: daw.is_capturing(&guid),
            takes: Vec::new(),
        });
    }
    if !want && was {
        TransportService::stop_recording(daw, project.clone()).map_err(failed)?;
        let tracks = Tracks::all(daw, project);
        let takes: Vec<TakeRow> = daw
            .last_recording(&guid)
            .into_iter()
            .map(|t| TakeRow {
                track: tracks
                    .iter()
                    .find(|track| track.guid == t.track_guid)
                    .map_or_else(|| t.track_guid.clone(), |track| track.name.clone()),
                lane: t.lane,
                lane_name: t.lane_name,
                source: t
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                position: t.position,
                length: t.length,
            })
            .collect();
        crate::studio::request_resync();
        crate::actions::list().changed("lanes", "a recording pass placed its takes");
        return Ok(RecordOut {
            recording: false,
            capturing: false,
            takes,
        });
    }
    Ok(RecordOut {
        recording: was,
        capturing: daw.is_capturing(&guid),
        takes: Vec::new(),
    })
}

/// The record button and the window's record key: `TRANSPORT_RECORD`
/// toggled through the Action List. A refusal is one warning.
pub fn toggle_through_the_list() {
    if let Err(e) = crate::actions::list().run_json(TRANSPORT_RECORD.id, b"{}") {
        tracing::warn!(action.id = TRANSPORT_RECORD.id, error = %e, "record refused");
    }
}

pub(crate) fn register(list: &ActionList) {
    list.register(&TRACK_ARM, Effect::Mutating, |a: ArmArg| arm(&a));
    list.register(&TRANSPORT_RECORD, Effect::Destructive, |a: RecordArg| {
        run(a.on)
    });
}
