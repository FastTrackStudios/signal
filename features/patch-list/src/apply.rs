//! Applying a plan to a session.
//!
//! Spec #48, decision #29, `flow.patch-list.apply`: every source
//! track's record input, set from its matching entry, as **one undo
//! step**. Arm and input monitoring are untouched — they are live
//! state owned by #55's rig and layer gangs, keyed by the same
//! taxonomy.
//!
//! Matching a [`Selector`] against a session's own tracks by name is
//! the scene engine's job (`dynamic_template::scenes`' own doc: "land
//! with the scene engine (#50)"), and #50 has not landed. Apply does
//! not wait on it: [`Resolve`] is the seam a caller hands in — every
//! track the session has, each with the selector that picks it out —
//! so a hand-built session, an ext-state-backed reader, or eventually
//! the scene engine itself can all drive apply the same way.
//!
//! This module also holds the two things apply leaves behind: the
//! **applied copy** (the styx text just applied, the profile's name,
//! and when — `ext_state::SECTION`/`APPLIED_KEY`) and the **session
//! override** (`OVERRIDE_KEY`, layered by [`crate::layer`]), plus
//! [`stale`], which compares the two.

use std::collections::HashSet;

use chrono::{SecondsFormat, Utc};
use daw_proto::{DawResult, ExtState, ProjectContext, Projects, RecordInput, Tracks};
use dynamic_template::scenes::Selector;

use crate::list::PatchList;
use crate::profile::Resolved;
use crate::validate::Plan;

/// The ext-state section apply's bookkeeping lives under.
pub const SECTION: &str = "fts.patch-list";
/// The applied copy: the full text last applied, its profile, its time.
pub const APPLIED_KEY: &str = "applied";
/// The session override, in the album's own schema.
pub const OVERRIDE_KEY: &str = "override";

/// The undo block's label — what shows in REAPER's Edit > Undo menu.
const UNDO_LABEL: &str = "Apply patch list";

/// What a session can tell apply about its own tracks: every track
/// paired with the selector that names it, in the same vocabulary an
/// entry's own selector is written in.
pub trait Resolve {
    /// Every track the session has, with its own taxonomy.
    fn tracks(&self) -> Vec<(daw_proto::TrackRef, Selector)>;
}

/// Whether a track's own taxonomy satisfies a selector.
///
/// Every field the selector sets must equal the track's, and an absent
/// field never constrains — `Selector::default()` matches everything.
/// `group` matches as a **prefix**, the way the vocabulary's own doc
/// defines it (`("Drum Kit" "Kick")` is the kick piece and everything
/// under it); every other field is exact.
// r[impl flow.patch-list.apply]
#[must_use]
pub fn selector_matches(selector: &Selector, track: &Selector) -> bool {
    if !track.group.starts_with(&selector.group) {
        return false;
    }
    field_matches(selector.kind.as_ref(), track.kind.as_ref())
        && field_matches(selector.performer.as_ref(), track.performer.as_ref())
        && field_matches(selector.layer.as_ref(), track.layer.as_ref())
        && field_matches(selector.channel.as_ref(), track.channel.as_ref())
        && field_matches(selector.multi_mic.as_ref(), track.multi_mic.as_ref())
        && field_matches(selector.arrangement.as_ref(), track.arrangement.as_ref())
        && field_matches(selector.name.as_ref(), track.name.as_ref())
}

fn field_matches(want: Option<&String>, have: Option<&String>) -> bool {
    want.is_none_or(|w| have.map(String::as_str) == Some(w.as_str()))
}

/// The `RecordInput` a resolution comes to, or `None` when apply has
/// nothing to set — an unresolved role, or an output pair (a
/// headphone bus is never a track's input).
///
/// A MIDI role is named by **device**, not numbered: the profile
/// carries the device's name (`"Nord Stage 3"`), while the daw's
/// `RecordInput::Midi` wants a numeric `device_id` — REAPER's own
/// index into its live device list, which nothing in this crate
/// enumerates. Naming the device by index would be a second, more
/// fragile vocabulary for exactly what the profile already names by
/// hand, so apply sets the **channel** only and leaves the device
/// unconstrained (`device_id: None`, "all devices") — never the wrong
/// hardware, only broader than the profile's precise one, until a
/// backend that knows the live device list resolves the name for real.
const fn record_input(resolved: &Resolved) -> Option<RecordInput> {
    match resolved {
        Resolved::Audio { channel } => Some(RecordInput::Audio { channel: *channel }),
        Resolved::Midi { channel, .. } => Some(RecordInput::Midi {
            device_id: None,
            channel: *channel,
        }),
        Resolved::Pair { .. } | Resolved::Unresolved => None,
    }
}

/// How many entries and tracks apply touched, or could not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    /// Entries whose input was set.
    pub applied: usize,
    /// Tracks the session has that no entry named — a view state,
    /// never acted on.
    pub unpatched: usize,
    /// Entries with no matching track — a view state, never acted on.
    pub unused: usize,
}

/// Apply a plan to a session: every source track's input, set from its
/// matching entry, as one undo step. Arm and input monitoring are not
/// touched.
///
/// # Errors
///
/// The first `set_record_input` failure the backend reports. The undo
/// block is still closed — a partial apply is still one step, and
/// leaving it open would corrupt every edit after it.
// r[impl flow.patch-list.apply]
pub fn apply<D>(
    daw: &D,
    project: &ProjectContext,
    plan: &Plan,
    resolve: &dyn Resolve,
) -> DawResult<Report>
where
    D: Tracks + Projects,
{
    let span = tracing::info_span!("patch_list.apply");
    let _enter = span.enter();

    let tracks = resolve.tracks();
    // `TrackRef` derives neither `Eq` nor `Hash` (it is a wire type,
    // not a set key), so which tracks matched is tracked by index into
    // `tracks` rather than by the ref itself.
    let mut matched: HashSet<usize> = HashSet::new();
    let mut applied = 0_usize;
    let mut unused = 0_usize;

    daw.begin_undo_block(project.clone(), UNDO_LABEL);
    let outcome = (|| -> DawResult<()> {
        for planned in &plan.entries {
            let Some((index, (track_ref, _))) = tracks
                .iter()
                .enumerate()
                .find(|(_, (_, taxonomy))| selector_matches(&planned.lowered.selector, taxonomy))
            else {
                unused = unused.saturating_add(1);
                continue;
            };
            matched.insert(index);
            if let Some(input) = record_input(&planned.resolved) {
                daw.set_record_input(project.clone(), track_ref.clone(), input)?;
                applied = applied.saturating_add(1);
            }
        }
        Ok(())
    })();
    daw.end_undo_block(project.clone(), UNDO_LABEL, None);
    outcome?;

    let unpatched = tracks.len().saturating_sub(matched.len());

    // `opentelemetry::Value` has no `From<u64>` (only the signed
    // integer, float, bool and string forms), so the counts go through
    // `i64` — plenty of range for a track count.
    architect_telemetry::wide::set("patch.applied", i64::try_from(applied).unwrap_or(i64::MAX));
    architect_telemetry::wide::set(
        "patch.unpatched",
        i64::try_from(unpatched).unwrap_or(i64::MAX),
    );
    architect_telemetry::wide::set("patch.unused", i64::try_from(unused).unwrap_or(i64::MAX));

    Ok(Report {
        applied,
        unpatched,
        unused,
    })
}

/// The applied copy: what apply stores after a successful apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The full styx text that was applied — the effective list,
    /// album and override already layered.
    pub text: String,
    /// The active profile's name at the time.
    pub profile: String,
    /// RFC 3339, UTC.
    pub at: String,
}

/// The ext-state value's own envelope: `profile`, then `at`, then the
/// applied text verbatim to the end.
///
/// Not styx-in-styx: the text is arbitrary album styx and a styx
/// writer is free to reformat a string value it re-serializes (a
/// trailing heredoc newline, reordered fields) — fine for a document,
/// wrong for a copy whose entire point is to be compared byte for byte
/// against what would be applied now (`stale`). Two header lines are
/// simple enough to own by hand and never rewrite what follows them.
impl Applied {
    fn encode(&self) -> String {
        format!("{}\n{}\n{}", self.profile, self.at, self.text)
    }

    fn decode(blob: &str) -> Option<Self> {
        let mut lines = blob.splitn(3, '\n');
        let profile = lines.next()?.to_owned();
        let at = lines.next()?.to_owned();
        let text = lines.next().unwrap_or_default().to_owned();
        Some(Self { text, profile, at })
    }
}

/// What storing or reading apply's ext-state can fail with.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Daw(#[from] daw_proto::DawError),
    #[error("the list did not serialize back to styx: {0}")]
    Serialize(#[from] crate::styx::WriteError),
    #[error("the patch list does not parse: {0}")]
    Parse(facet_styx::DeserializeError),
    #[error("the applied copy is malformed (missing its profile/at header)")]
    Applied,
}

/// The text apply actually applies: the album's own text, unchanged,
/// when there is no override — or, when there is one, the album with
/// it layered on top ([`crate::layer`]) and serialized back to styx.
///
/// Byte-fidelity matters here more than it does for [`PatchList`]
/// generally: this is what [`store_applied`] should be given and what
/// [`stale`] compares against, and a merge is the only case that has
/// to go through the serializer at all — with no override, handing
/// back the album's own source (comments, formatting, and all) is
/// exact where re-serializing it would not be. An override changes
/// staleness the same way an album edit does, per `stale`'s own
/// contract ("the album (with any override layered) or the active
/// profile has moved on").
///
/// # Errors
///
/// [`Error::Parse`] when `album_text` is not a patch list;
/// [`Error::Serialize`] when the layered list cannot be written back
/// as styx — which a list built from these types cannot fail at.
// r[impl flow.patch-list.apply]
// r[impl flow.patch-list.session-override]
pub fn effective_text(album_text: &str, over: Option<&PatchList>) -> Result<String, Error> {
    let Some(over) = over else {
        return Ok(album_text.to_owned());
    };
    let album = PatchList::from_styx(album_text).map_err(Error::Parse)?;
    Ok(crate::layer::layer(&album, Some(over)).list.to_styx()?)
}

/// Store the applied copy: the text just applied, the profile's name,
/// and now.
///
/// # Errors
///
/// [`Error::Daw`] when the backend refuses the write.
// r[impl flow.patch-list.apply]
pub fn store_applied<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    text: &str,
    profile: &str,
) -> Result<(), Error> {
    let record = Applied {
        text: text.to_owned(),
        profile: profile.to_owned(),
        at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    };
    daw.set_project(project, SECTION, APPLIED_KEY, &record.encode())
        .map_err(Error::Daw)
}

/// The applied copy, if one has ever been stored.
///
/// # Errors
///
/// [`Error::Applied`] when there is a value and it is not a copy this
/// crate wrote — which means a hand-edited `.RPP`, not a missing copy.
pub fn applied<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
) -> Result<Option<Applied>, Error> {
    let Some(blob) = daw.get_project(project, SECTION, APPLIED_KEY) else {
        return Ok(None);
    };
    if blob.trim().is_empty() {
        return Ok(None);
    }
    Applied::decode(&blob).map(Some).ok_or(Error::Applied)
}

/// Write the session override — a partial document in the album's own
/// schema.
///
/// # Errors
///
/// [`Error::Serialize`] when the list cannot be written back as styx;
/// [`Error::Daw`] when the backend refuses the write.
// r[impl flow.patch-list.session-override]
pub fn set_override<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    list: &PatchList,
) -> Result<(), Error> {
    let blob = crate::styx::write(list)?;
    daw.set_project(project, SECTION, OVERRIDE_KEY, &blob)
        .map_err(Error::Daw)
}

/// The session override, if one is set.
///
/// # Errors
///
/// [`Error::Parse`] when the stored override does not parse.
// r[impl flow.patch-list.session-override]
pub fn get_override<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
) -> Result<Option<PatchList>, Error> {
    let Some(text) = daw.get_project(project, SECTION, OVERRIDE_KEY) else {
        return Ok(None);
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    crate::styx::read(&text).map(Some).map_err(Error::Parse)
}

/// Remove the session override — "removing it returns the session to
/// the list" (decision #29).
///
/// # Errors
///
/// When the backend refuses the delete.
// r[impl flow.patch-list.session-override]
pub fn clear_override<D: ExtState + ?Sized>(daw: &D, project: ProjectContext) -> DawResult<()> {
    daw.delete_project(project, SECTION, OVERRIDE_KEY)
}

/// The applied copy against what would be applied now: stale when the
/// effective text or the active profile has moved on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// The copy that is out of date.
    pub applied: Applied,
    /// Lines only the applied copy has, and lines only the current
    /// text has — not a structural diff, but enough for a view to
    /// show what moved.
    pub diff: Diff,
}

/// A line-level diff between two texts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// Lines the applied copy had that the current text does not.
    pub removed: Vec<String>,
    /// Lines the current text has that the applied copy did not.
    pub added: Vec<String>,
}

impl Diff {
    /// Whether the two texts actually differ.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty()
    }

    fn of(old: &str, new: &str) -> Self {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let new_set: HashSet<&str> = new_lines.iter().copied().collect();
        let old_set: HashSet<&str> = old_lines.iter().copied().collect();
        Self {
            removed: old_lines
                .iter()
                .filter(|l| !new_set.contains(*l))
                .map(|l| (*l).to_owned())
                .collect(),
            added: new_lines
                .iter()
                .filter(|l| !old_set.contains(*l))
                .map(|l| (*l).to_owned())
                .collect(),
        }
    }
}

/// Whether the session is stale.
///
/// Stale means the album (with any override layered) or the active
/// profile has moved on since the last apply. Never auto-applies —
/// this only says whether the Patch List should show the banner.
///
/// `None` when nothing has ever been applied (nothing to be stale
/// against) or the current state matches the applied copy exactly.
///
/// # Errors
///
/// [`Error::Applied`] when the stored applied copy is malformed.
// r[impl flow.patch-list.apply]
pub fn stale<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    effective_text: &str,
    active_profile: &str,
) -> Result<Option<Stale>, Error> {
    let Some(record) = applied(daw, project)? else {
        return Ok(None);
    };
    let text_changed = record.text != effective_text;
    let profile_changed = record.profile != active_profile;
    if !text_changed && !profile_changed {
        return Ok(None);
    }
    let diff = Diff::of(&record.text, effective_text);
    Ok(Some(Stale {
        applied: record,
        diff,
    }))
}
