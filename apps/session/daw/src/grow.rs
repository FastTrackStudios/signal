//! Growing a part: a double, a second arrangement, a second part.
//!
//! The narrative's three gestures, on Session's own engine and its Action
//! List (`docs/spec/session/original-composition.md`, phases 10–13):
//!
//! - **`TRACK_DOUBLE`** — "double track", and the narrative's "add
//!   layer" (Q1). The part becomes a folder over `Main` (what was played:
//!   its takes, its lanes, its comp) and `DBL` (empty, recording from the
//!   same input), panned hard left and right. The part's chain stays on
//!   the folder, so ONE rig takes both sides, in stereo.
//! - **`TRACK_ADD_ARRANGEMENT`** — a part beside it under the instrument
//!   (`Guitar/{Rhythm, Lead}`). The first time, a folder named for the
//!   instrument goes round the part, and the part — named for the stack
//!   it played while it was the only one (`Guitar Clean`) — takes its
//!   role's name, `Rhythm` (Q2). The new one is a plain track with no
//!   rig: what it plays is loaded onto it.
//! - **`TRACK_ADD_PART`** — "add part" (Q3): the part becomes a folder
//!   over `Main` (the part as it was) and the new part (`Harmony`), each
//!   with its own rig — the new one a copy of Main's, its state and all,
//!   though not its automation (a part's tone changes are its own).
//!
//! One rule decides which track is which: **a rig never moves.** The
//! track the chain sits on keeps its GUID, its FX and their envelopes, and
//! the folders are built round it. A double's rig is the folder's, so the
//! performance moves down onto a new `Main`; an arrangement's or a part's
//! rig is the part's own, so the new folder goes above it. (Moving a chain
//! would mean recreating its instances, and the engine's undo does not
//! keep chains — an undone move would lose the rig.)
//!
//! Each is one undo step. Each is worked out from the track list before
//! anything changes, so a refusal leaves no step behind.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{
    DawResult, Effects, FxChainContext, FxRef, FxTarget, ItemRef, Items, ProjectContext, Projects,
    TrackRef, TrackTree, Tracks,
};
use daw::standalone::Standalone;
use daw_proto::Track;
use daw_proto::track::RecordInput;
use facet::Facet;

/// A double's sides (Q1): the part as played, and its double.
pub const MAIN: &str = "Main";
/// See [`MAIN`].
pub const DOUBLE: &str = "DBL";

/// The words a part's name starts with when it is named for its
/// instrument — what `RIG_LOAD` calls it (`Guitar`, `Guitar Clean`).
const INSTRUMENTS: [&str; 2] = ["Guitar", "Bass"];

/// The role an instrument's first part takes when a second joins it.
pub const FIRST_ROLE: &str = "Rhythm";

/// What a part plays in the song, in the order a new arrangement is
/// named from when no name is given.
pub const ROLES: [&str; 9] = [
    FIRST_ROLE,
    "Lead",
    "Chug",
    "Riff",
    "Solo",
    "Arp",
    "Pad",
    "Strum",
    "Fingerpick",
];

/// A new part's name when none is given, in order.
pub const PARTS: [&str; 3] = ["Harmony", "OCT", "Melody"];

/// Names compare as a person reads them: `rhythm` is `Rhythm`.
fn same(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// A part's name split into its instrument and what follows it:
/// `Guitar Clean` is the guitar and `Clean`, `Guitar` the guitar and
/// nothing. A name not led by an instrument is all instrument.
#[must_use]
pub fn instrument_and_rest(name: &str) -> (&str, &str) {
    let name = name.trim();
    for word in INSTRUMENTS {
        if let Some(head) = name.get(..word.len())
            && head.eq_ignore_ascii_case(word)
            && let Some(rest) = name.get(word.len()..)
            && (rest.is_empty() || rest.starts_with(' '))
        {
            return (head, rest.trim());
        }
    }
    (name, "")
}

/// The name a part goes by among its instrument's arrangements (Q2).
///
/// The role its name already says (`Guitar Lead`, `Lead`), else
/// [`FIRST_ROLE`] — a part named for the stack it plays (`Guitar Clean`)
/// or for the instrument alone (`Guitar`) was the only one, and the first.
#[must_use]
pub fn role_of(name: &str) -> &'static str {
    let (instrument, rest) = instrument_and_rest(name);
    let said = if rest.is_empty() { instrument } else { rest };
    ROLES
        .into_iter()
        .find(|role| same(role, said))
        .unwrap_or(FIRST_ROLE)
}

/// A folder over `Main` and `DBL`: a doubled part.
fn is_doubled(tree: &TrackTree, guid: &str) -> bool {
    tree.children_of(guid).any(|c| same(&c.name, MAIN))
        && tree.children_of(guid).any(|c| same(&c.name, DOUBLE))
}

/// A folder over `Main` and other parts (`Lead/{Main, Harmony}`).
fn is_layered(tree: &TrackTree, guid: &str) -> bool {
    !is_doubled(tree, guid) && tree.children_of(guid).any(|c| same(&c.name, MAIN))
}

/// A folder over arrangements: neither of the two above.
fn is_instrument(tree: &TrackTree, guid: &str) -> bool {
    tree.children_of(guid).next().is_some() && !is_doubled(tree, guid) && !is_layered(tree, guid)
}

/// The part a track is: itself, or — a side of a double — the part.
fn part_of<'a>(tree: &'a TrackTree, track: &'a Track) -> &'a Track {
    tree.parent_of(track)
        .filter(|parent| is_doubled(tree, &parent.guid))
        .unwrap_or(track)
}

/// `Guitar/Lead/Main`.
fn path(tree: &TrackTree, track: &Track) -> String {
    let mut names = vec![track.name.as_str()];
    let mut at = track;
    // Bounded: a parent cycle would be a broken project, not a hang.
    for _ in tree.all() {
        let Some(parent) = tree.parent_of(at) else {
            break;
        };
        names.push(parent.name.as_str());
        at = parent;
    }
    names.reverse();
    names.join("/")
}

/// The name a new member takes: `given`, unless a member already has it;
/// else the first of `offered` no member has.
fn new_name(given: Option<&str>, taken: &[&str], offered: &[&str]) -> Result<String, String> {
    let is_taken = |name: &str| taken.iter().any(|t| same(t, name));
    match given.map(str::trim) {
        Some("") => Err("give the new track a name, or leave `name` out".to_owned()),
        Some(name) if is_taken(name) => Err(format!("there is a {name} there already")),
        Some(name) => Ok(name.to_owned()),
        None => offered
            .iter()
            .find(|name| !is_taken(name))
            .map(|name| (*name).to_owned())
            .ok_or_else(|| {
                format!(
                    "{} are all there already: name the new one with `name`",
                    offered.join(", ")
                )
            }),
    }
}

// ── plans: what each gesture does, before anything changes ──────────

/// The part [`double`] makes a folder of: the track picked, or the part
/// it is a side of.
///
/// # Errors
///
/// No such track; a part already doubled; a folder that is not a part.
pub fn plan_double(tracks: &[Track], target: &str) -> Result<String, String> {
    let tree = TrackTree::new(tracks.to_vec());
    let picked = tree
        .get(target)
        .ok_or_else(|| format!("no track {target}"))?;
    let part = part_of(&tree, picked);
    if is_doubled(&tree, &part.guid) {
        return Err(format!(
            "{} is doubled already: {MAIN} and {DOUBLE}",
            path(&tree, part)
        ));
    }
    if tree.children_of(&part.guid).next().is_some() {
        return Err(format!(
            "{} is a folder: double one of the parts in it",
            path(&tree, part)
        ));
    }
    Ok(part.guid.clone())
}

/// What [`add_arrangement`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrangementPlan {
    /// The part is its instrument's only one, at the top: a folder named
    /// for the instrument goes round it, the part takes `role` for its
    /// name, and `name` goes in beside it.
    Wrap {
        part: String,
        instrument: String,
        role: String,
        name: String,
    },
    /// The part is in its instrument's folder (`container`): `name` goes
    /// in after the others. When the part was the only one, `rename`
    /// gives it its role first.
    Beside {
        container: String,
        part: String,
        rename: Option<(String, String)>,
        name: String,
    },
}

/// What [`add_arrangement`] does from `target`.
///
/// From a side of a double (`Rhythm/Main`), or a part beside a Main
/// (`Lead/Harmony`), it climbs to the arrangement those belong to; from
/// the instrument's own folder (`Guitar`), it adds to that.
///
/// # Errors
///
/// No such track; `name` taken or empty; every role taken.
pub fn plan_arrangement(
    tracks: &[Track],
    target: &str,
    name: Option<&str>,
) -> Result<ArrangementPlan, String> {
    let tree = TrackTree::new(tracks.to_vec());
    let mut part = tree
        .get(target)
        .ok_or_else(|| format!("no track {target}"))?;
    for _ in tracks {
        let Some(parent) = tree
            .parent_of(part)
            .filter(|p| is_doubled(&tree, &p.guid) || is_layered(&tree, &p.guid))
        else {
            break;
        };
        part = parent;
    }
    let container = if is_instrument(&tree, &part.guid) {
        Some(part)
    } else {
        tree.parent_of(part)
    };
    let Some(container) = container else {
        let (instrument, _) = instrument_and_rest(&part.name);
        let role = role_of(&part.name);
        let name = new_name(name, &[role], &ROLES)?;
        return Ok(ArrangementPlan::Wrap {
            part: part.guid.clone(),
            instrument: instrument.to_owned(),
            role: role.to_owned(),
            name,
        });
    };
    let members: Vec<&Track> = tree.children_of(&container.guid).collect();
    let rename = match members.as_slice() {
        [only] if !same(&only.name, role_of(&only.name)) => {
            Some((only.guid.clone(), role_of(&only.name).to_owned()))
        }
        _ => None,
    };
    // The only member, renamed, is taken by its new name.
    let taken: Vec<&str> = rename.as_ref().map_or_else(
        || members.iter().map(|m| m.name.as_str()).collect(),
        |(_, role)| vec![role.as_str()],
    );
    let name = new_name(name, &taken, &ROLES)?;
    Ok(ArrangementPlan::Beside {
        container: container.guid.clone(),
        part: part.guid.clone(),
        rename,
        name,
    })
}

/// What [`add_part`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartPlan {
    /// The part becomes `Main` under a new folder of its own name, and
    /// `name` goes in beside it with a copy of its rig.
    Wrap { part: String, name: String },
    /// The part is already over a `Main` (`container`): `name` goes in
    /// after the others, with a copy of `main`'s rig.
    Beside {
        container: String,
        main: Option<String>,
        name: String,
    },
}

/// What [`add_part`] does from `target`: a part, a side of its double,
/// or a part already beside a Main.
///
/// # Errors
///
/// No such track; `name` is `Main` or `DBL`, taken, or empty; every
/// part name taken.
pub fn plan_part(tracks: &[Track], target: &str, name: Option<&str>) -> Result<PartPlan, String> {
    if let Some(name) = name
        && (same(name, MAIN) || same(name, DOUBLE))
    {
        return Err(format!(
            "{MAIN} and {DOUBLE} are a double's sides: name the new part something else"
        ));
    }
    let tree = TrackTree::new(tracks.to_vec());
    let picked = tree
        .get(target)
        .ok_or_else(|| format!("no track {target}"))?;
    let part = part_of(&tree, picked);
    let container = if is_layered(&tree, &part.guid) {
        Some(part)
    } else {
        tree.parent_of(part)
            .filter(|parent| is_layered(&tree, &parent.guid))
    };
    let Some(container) = container else {
        let name = new_name(name, &[MAIN], &PARTS)?;
        return Ok(PartPlan::Wrap {
            part: part.guid.clone(),
            name,
        });
    };
    let members: Vec<&Track> = tree.children_of(&container.guid).collect();
    let taken: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    let name = new_name(name, &taken, &PARTS)?;
    Ok(PartPlan::Beside {
        container: container.guid.clone(),
        main: members
            .iter()
            .find(|m| same(&m.name, MAIN))
            .map(|m| m.guid.clone()),
        name,
    })
}

// ── the gestures ────────────────────────────────────────────────────

/// Double the part `track` is (a part, or a side of its double): a folder
/// over `Main` and `DBL`, panned left and right into the part's rig, as
/// one undo step. Says the part's shape now.
///
/// # Errors
///
/// As [`plan_double`]; the engine refusing a write (what was done so far
/// is the one undo step).
pub fn double(daw: &Standalone, project: &ProjectContext, track: &str) -> Result<String, String> {
    let all = Tracks::all(daw, project.clone());
    let guid = plan_double(&all, track)?;
    let part = all
        .iter()
        .find(|t| t.guid == guid)
        .ok_or("the part went")?
        .clone();
    step(daw, project, &format!("Double {}", part.name), || {
        double_part(daw, project, &part)
    })?;
    Ok(shape(daw, project, &part.guid))
}

/// Add an arrangement beside the one `track` belongs to — `name`, else
/// the first role not there — as one undo step. Says the instrument's
/// shape now.
///
/// # Errors
///
/// As [`plan_arrangement`]; the engine refusing a write.
pub fn add_arrangement(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    name: Option<&str>,
) -> Result<String, String> {
    let all = Tracks::all(daw, project.clone());
    let plan = plan_arrangement(&all, track, name)?;
    let tree = TrackTree::new(all);
    let (ArrangementPlan::Wrap { name, .. } | ArrangementPlan::Beside { name, .. }) = &plan;
    let container = step(
        daw,
        project,
        &format!("Add arrangement {name}"),
        || match &plan {
            ArrangementPlan::Wrap {
                part,
                instrument,
                role,
                name,
            } => {
                let folder = wrap(daw, project, part, instrument)?;
                rename(daw, project, part, role)?;
                let added = append_child(daw, project, &folder, name)?;
                inherit_input(daw, project, &tree, part, &added)?;
                Ok(folder)
            }
            ArrangementPlan::Beside {
                container,
                part,
                rename: renamed,
                name,
            } => {
                if let Some((guid, role)) = renamed {
                    rename(daw, project, guid, role)?;
                }
                let added = append_child(daw, project, container, name)?;
                inherit_input(daw, project, &tree, part, &added)?;
                Ok(container.clone())
            }
        },
    )?;
    Ok(shape(daw, project, &container))
}

/// Add a part beside the one `track` is — `name`, else the first of
/// [`PARTS`] not there — each with its own rig, as one undo step. Says
/// the shape now.
///
/// # Errors
///
/// As [`plan_part`]; the engine refusing a write.
pub fn add_part(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    name: Option<&str>,
) -> Result<String, String> {
    let all = Tracks::all(daw, project.clone());
    let plan = plan_part(&all, track, name)?;
    let tree = TrackTree::new(all);
    let (PartPlan::Wrap { name, .. } | PartPlan::Beside { name, .. }) = &plan;
    let container = step(daw, project, &format!("Add part {name}"), || match &plan {
        PartPlan::Wrap { part, name } => {
            let called = tree
                .get(part)
                .map(|t| t.name.clone())
                .ok_or("the part went")?;
            let folder = wrap(daw, project, part, &called)?;
            rename(daw, project, part, MAIN)?;
            let added = append_child(daw, project, &folder, name)?;
            copy_chain(daw, project, part, &added)?;
            inherit_input(daw, project, &tree, part, &added)?;
            Ok(folder)
        }
        PartPlan::Beside {
            container,
            main,
            name,
        } => {
            let added = append_child(daw, project, container, name)?;
            if let Some(main) = main {
                copy_chain(daw, project, main, &added)?;
                inherit_input(daw, project, &tree, main, &added)?;
            }
            Ok(container.clone())
        }
    })?;
    Ok(shape(daw, project, &container))
}

// ── the moves ───────────────────────────────────────────────────────

/// `edit` as one undo step called `label` — ended whatever it returns, so
/// a refusal halfway is still one step to take back.
fn step<T>(
    daw: &Standalone,
    project: &ProjectContext,
    label: &str,
    edit: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    Projects::begin_undo_block(daw, project.clone(), label);
    let done = edit();
    Projects::end_undo_block(daw, project.clone(), label, None);
    done
}

/// An engine refusal, said with what was being done.
fn or_say<T>(done: DawResult<T>, doing: &str) -> Result<T, String> {
    done.map_err(|e| format!("{doing}: {e}"))
}

fn at(guid: &str) -> TrackRef {
    TrackRef::Guid(guid.to_owned())
}

/// The part's track opens over `Main` and `DBL`. Main takes what was
/// played — every item (each on its lane), the lanes, the comp — and the
/// input and the arm; both sides record from the part's input; the part
/// keeps its GUID, its chain and its envelopes.
fn double_part(daw: &Standalone, project: &ProjectContext, part: &Track) -> Result<(), String> {
    let p = || project.clone();
    let comping = Tracks::comping(daw, p(), at(&part.guid)).unwrap_or_default();
    let items = Items::get_items(daw, p(), at(&part.guid));
    let main = or_say(
        Tracks::add(daw, p(), MAIN, Some(part.index.saturating_add(1))),
        "adding Main",
    )?;
    let dbl = or_say(
        Tracks::add(daw, p(), DOUBLE, Some(part.index.saturating_add(2))),
        "adding DBL",
    )?;
    // DBL closes the part, and whatever the part used to close.
    or_say(
        Tracks::set_folder_depth(
            daw,
            p(),
            at(&dbl),
            part.folder_depth.min(0).saturating_sub(1),
        ),
        "closing the part's folder",
    )?;
    or_say(
        Tracks::set_folder_depth(daw, p(), at(&part.guid), 1),
        "opening the part's folder",
    )?;
    copy_lanes(daw, project, part, &comping, &main)?;
    for item in items {
        or_say(
            Items::move_to_track(daw, p(), ItemRef::Guid(item.guid), at(&main)),
            "moving a take onto Main",
        )?;
    }
    for side in [&main, &dbl] {
        or_say(
            Tracks::set_record_input(daw, p(), at(side), part.record_input),
            "a side's input",
        )?;
        or_say(
            Tracks::set_input_monitor(daw, p(), at(side), part.input_monitor),
            "a side's monitoring",
        )?;
    }
    if part.record_input != RecordInput::None {
        or_say(
            Tracks::set_record_input(daw, p(), at(&part.guid), RecordInput::None),
            "the folder's input",
        )?;
    }
    if part.armed {
        or_say(Tracks::set_armed(daw, p(), at(&main), true), "arming Main")?;
        or_say(
            Tracks::set_armed(daw, p(), at(&part.guid), false),
            "disarming the folder",
        )?;
    }
    if part.lane_count > 0 {
        or_say(
            Tracks::set_lane_count(daw, p(), at(&part.guid), 0),
            "the folder's lanes",
        )?;
    }
    or_say(Tracks::set_pan(daw, p(), at(&main), -1.0), "panning Main")?;
    or_say(Tracks::set_pan(daw, p(), at(&dbl), 1.0), "panning DBL")?;
    Ok(())
}

/// `part`'s lanes onto `main`: how many, their names, which plays, the
/// comp areas and the comping lane — so the takes moved onto it land
/// where they were. (The pinned record lane has no setter; it is left.)
fn copy_lanes(
    daw: &Standalone,
    project: &ProjectContext,
    part: &Track,
    comping: &daw_proto::track::LaneComping,
    main: &str,
) -> Result<(), String> {
    if part.lane_count == 0 {
        return Ok(());
    }
    let p = || project.clone();
    or_say(
        Tracks::set_lane_count(daw, p(), at(main), part.lane_count),
        "Main's lanes",
    )?;
    for (lane, name) in (0_u32..).zip(&part.lane_names) {
        or_say(
            Tracks::set_lane_name(daw, p(), at(main), lane, name),
            "naming Main's lanes",
        )?;
    }
    or_say(
        Tracks::set_lane_play_mask(daw, p(), at(main), part.lane_play_mask),
        "the lane that plays",
    )?;
    if !comping.areas.is_empty() {
        or_say(
            Tracks::set_comp_areas(daw, p(), at(main), comping.areas.clone()),
            "Main's comp",
        )?;
    }
    // The comp before the current one first, so both are as they were.
    for lane in [comping.last_comp_lane, comping.comp_lane]
        .into_iter()
        .flatten()
    {
        or_say(
            Tracks::set_active_comp(daw, p(), at(main), Some(lane)),
            "Main's comping lane",
        )?;
    }
    Ok(())
}

/// A new folder called `name` round `guid` and everything under it, where
/// it stood. Its GUID.
fn wrap(
    daw: &Standalone,
    project: &ProjectContext,
    guid: &str,
    name: &str,
) -> Result<String, String> {
    let tree = TrackTree::new(Tracks::all(daw, project.clone()));
    let track = tree.get(guid).ok_or("the part went")?.clone();
    // The last track of its subtree (itself, a leaf) closes one level more.
    let last = tree
        .subtree_end_index(guid)
        .and_then(|end| tree.at_index(end.saturating_sub(1)))
        .ok_or("the part's subtree went")?
        .clone();
    let folder = or_say(
        Tracks::add(daw, project.clone(), name, Some(track.index)),
        "adding the folder",
    )?;
    or_say(
        Tracks::set_folder_depth(daw, project.clone(), at(&folder), 1),
        "opening the folder",
    )?;
    or_say(
        Tracks::set_folder_depth(
            daw,
            project.clone(),
            at(&last.guid),
            last.folder_depth.saturating_sub(1),
        ),
        "closing the folder",
    )?;
    Ok(folder)
}

/// A new plain track called `name`, last in `parent`. Its GUID.
fn append_child(
    daw: &Standalone,
    project: &ProjectContext,
    parent: &str,
    name: &str,
) -> Result<String, String> {
    let plan = TrackTree::new(Tracks::all(daw, project.clone()))
        .plan_append(parent)
        .ok_or("the folder went")?;
    if let Some((guid, depth)) = &plan.retarget {
        or_say(
            Tracks::set_folder_depth(daw, project.clone(), at(guid), *depth),
            "making room in the folder",
        )?;
    }
    or_say(
        Tracks::set_folder_depth(daw, project.clone(), at(parent), 1),
        "opening the folder",
    )?;
    let added = or_say(
        Tracks::add(daw, project.clone(), name, Some(plan.index)),
        "adding the track",
    )?;
    or_say(
        Tracks::set_folder_depth(
            daw,
            project.clone(),
            at(&added),
            plan.closes.saturating_neg(),
        ),
        "closing the folder",
    )?;
    Ok(added)
}

fn rename(
    daw: &Standalone,
    project: &ProjectContext,
    guid: &str,
    name: &str,
) -> Result<(), String> {
    let had = Tracks::get(daw, project.clone(), at(guid)).map(|t| t.name);
    if had.as_deref() == Some(name) {
        return Ok(());
    }
    or_say(
        Tracks::rename(daw, project.clone(), at(guid), name),
        "renaming the part",
    )
}

/// What `track` records from: its own input, else the first under it.
fn input_under(tree: &TrackTree, track: &Track) -> RecordInput {
    if track.record_input != RecordInput::None {
        return track.record_input;
    }
    tree.children_of(&track.guid)
        .map(|child| input_under(tree, child))
        .find(|input| *input != RecordInput::None)
        .unwrap_or(RecordInput::None)
}

/// `to` records from what `from` (as `tree` had it) did: a new part plays
/// the same instrument, through the same input.
fn inherit_input(
    daw: &Standalone,
    project: &ProjectContext,
    tree: &TrackTree,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let Some(input) = tree
        .get(from)
        .map(|track| input_under(tree, track))
        .filter(|input| *input != RecordInput::None)
    else {
        return Ok(());
    };
    or_say(
        Tracks::set_record_input(daw, project.clone(), at(to), input),
        "the new track's input",
    )
}

/// A copy of `from`'s chain onto the end of `to`'s: each FX by name, with
/// its state (the instance's own, else its parameters) and its bypass —
/// not its automation.
fn copy_chain(
    daw: &Standalone,
    project: &ProjectContext,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let source = FxChainContext::Track(from.to_owned());
    let target = FxChainContext::Track(to.to_owned());
    for fx in Effects::list(daw, project.clone(), source.clone()) {
        let added = Effects::add(daw, project.clone(), target.clone(), &fx.name)
            .ok_or_else(|| format!("{} would not add", fx.name))?;
        let here = FxTarget::new(source.clone(), FxRef::Guid(fx.guid.clone()));
        let there = FxTarget::new(target.clone(), FxRef::Guid(added.clone()));
        if let Some(chunk) = Effects::state_chunk(daw, project.clone(), here.clone()) {
            or_say(
                Effects::set_state_chunk(daw, project.clone(), there.clone(), chunk),
                "copying the rig's chunk",
            )?;
        }
        if let Ok(state) = daw.save_plugin_state(&fx.guid) {
            daw.apply_plugin_state(&added, &state)
                .map_err(|e| format!("{} would not take the copy's state: {e}", fx.name))?;
        } else {
            // No live instance: its state is its parameters.
            for param in Effects::parameters(daw, project.clone(), here.clone()) {
                or_say(
                    Effects::set_parameter(
                        daw,
                        project.clone(),
                        daw_proto::fx::SetParameterRequest {
                            target: there.clone(),
                            index: param.index,
                            value: param.value,
                        },
                    ),
                    "copying the rig's parameters",
                )?;
            }
        }
        if !fx.enabled {
            or_say(
                Effects::set_enabled(daw, project.clone(), there, false),
                "bypassing the copy",
            )?;
        }
    }
    Ok(())
}

/// `Guitar/{Rhythm, Lead}`: a folder's path and its tracks.
fn shape(daw: &Standalone, project: &ProjectContext, guid: &str) -> String {
    let tree = TrackTree::new(Tracks::all(daw, project.clone()));
    let Some(track) = tree.get(guid) else {
        return String::new();
    };
    let members: Vec<&str> = tree.children_of(guid).map(|c| c.name.as_str()).collect();
    format!("{}/{{{}}}", path(&tree, track), members.join(", "))
}

// ── the Action List ─────────────────────────────────────────────────

pub(crate) static DOUBLE_TRACK: ActionMeta = crate::actions::meta(
    "TRACK_DOUBLE",
    "track_double",
    "Double",
    "Double a part — `track` (a path, name or GUID; the selected track when left out), or the part it is a side of: it becomes a folder over `Main` (what was played: every take, lane and comp, its input and arm) and `DBL` (empty, recording from the same input), panned hard left and right. The part keeps its GUID and its rig, so one rig takes both sides in stereo. The narrative's \"add layer\". Refused on a part doubled already. Says the part's shape. One undo step.",
    "Tracks",
    "",
);

pub(crate) static ADD_ARRANGEMENT: ActionMeta = crate::actions::meta(
    "TRACK_ADD_ARRANGEMENT",
    "track_add_arrangement",
    "Add arrangement",
    "Add an arrangement beside the part `track` belongs to (the selected track when left out): a plain track `name` (else the first of Rhythm, Lead, Chug… not there) under the instrument, with no rig of its own yet, recording from the part's input. The first time, a folder named for the instrument goes round the part, and the part — named for its stack while it was the only one (`Guitar Clean`) — takes its role's name (`Rhythm`): `Guitar/{Rhythm, Lead}`. Says the instrument's shape. One undo step.",
    "Tracks",
    "",
);

pub(crate) static ADD_PART: ActionMeta = crate::actions::meta(
    "TRACK_ADD_PART",
    "track_add_part",
    "Add part",
    "Add a part beside `track` (the selected track when left out; a side of a double means its part): the part becomes a folder over `Main` (the part as it was, its rig and takes) and a new track `name` (else Harmony, OCT, Melody — the first not there) carrying a copy of Main's rig, without its automation: `Lead/{Main, Harmony}`. On a part already over a Main, the new one goes beside the others. Says the shape. One undo step.",
    "Tracks",
    "",
);

/// Which part to double.
#[derive(Facet)]
struct DoubleArg {
    /// The selected track when left out.
    track: Option<String>,
}

/// Which part to grow, and what the new track is called.
#[derive(Facet)]
struct GrowArg {
    /// The selected track when left out.
    track: Option<String>,
    /// Offered from the list's names when left out.
    name: Option<String>,
}

/// The track asked for, else the selected one.
fn target(
    daw: &Standalone,
    project: &ProjectContext,
    track: Option<&str>,
) -> Result<String, String> {
    track.map_or_else(
        || {
            Tracks::selected(daw, project.clone())
                .into_iter()
                .next()
                .map(|t| t.guid)
                .ok_or_else(|| "no track selected: select one, or name it with `track`".to_owned())
        },
        |track| crate::actions::resolve(daw, project, track),
    )
}

/// The window reads the tree back, and a watcher hears it changed.
fn grew(said: &str) {
    crate::studio::request_resync();
    crate::actions::list().changed("tracks", said);
}

pub(crate) fn register(list: &ActionList) {
    list.register(&DOUBLE_TRACK, Effect::Mutating, |a: DoubleArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = target(daw, &project, a.track.as_deref())?;
        let said = double(daw, &project, &guid)?;
        grew(&said);
        Ok(said)
    });
    list.register(&ADD_ARRANGEMENT, Effect::Mutating, |a: GrowArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = target(daw, &project, a.track.as_deref())?;
        let said = add_arrangement(daw, &project, &guid, a.name.as_deref())?;
        grew(&said);
        Ok(said)
    });
    list.register(&ADD_PART, Effect::Mutating, |a: GrowArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = target(daw, &project, a.track.as_deref())?;
        let said = add_part(daw, &project, &guid, a.name.as_deref())?;
        grew(&said);
        Ok(said)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A track list from `(name, folder depth)`, in order, its parents
    /// worked out from the depths as the engine does.
    fn tracks(spec: &[(&str, i32)]) -> Vec<Track> {
        let mut open: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for (index, (name, depth)) in (0_u32..).zip(spec) {
            let mut track = Track::new(format!("{index}:{name}"), index, (*name).to_owned());
            track.folder_depth = *depth;
            track.is_folder = *depth > 0;
            track.parent_guid = open.last().cloned();
            if *depth > 0 {
                open.push(track.guid.clone());
            }
            for _ in 0..(*depth).min(0).unsigned_abs() {
                open.pop();
            }
            out.push(track);
        }
        out
    }

    fn guid_of(list: &[Track], index: u32) -> String {
        list.iter()
            .find(|t| t.index == index)
            .map(|t| t.guid.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_name_is_its_instrument_and_what_follows() {
        assert_eq!(instrument_and_rest("Guitar Clean"), ("Guitar", "Clean"));
        assert_eq!(instrument_and_rest("guitar"), ("guitar", ""));
        assert_eq!(instrument_and_rest("Bass Drive"), ("Bass", "Drive"));
        // A word that only starts with one is not it.
        assert_eq!(instrument_and_rest("Guitars"), ("Guitars", ""));
        assert_eq!(instrument_and_rest("Keys Pad"), ("Keys Pad", ""));
    }

    /// Q2: named for its stack, or the instrument alone, it was the first
    /// part — the Rhythm; a role it already says, it keeps.
    #[test]
    fn a_part_named_for_its_stack_takes_the_first_role() {
        assert_eq!(role_of("Guitar Clean"), "Rhythm");
        assert_eq!(role_of("Guitar"), "Rhythm");
        assert_eq!(role_of("Clean"), "Rhythm");
        assert_eq!(role_of("Guitar Lead"), "Lead");
        assert_eq!(role_of("chug"), "Chug");
    }

    #[test]
    fn a_double_is_refused_on_a_part_doubled_already() {
        let list = tracks(&[("Bass", 0), ("Guitar Clean", 1), ("Main", 0), ("DBL", -1)]);
        let part = guid_of(&list, 1);
        assert!(plan_double(&list, &part).is_err());
        // A side means its part.
        assert!(plan_double(&list, &guid_of(&list, 3)).is_err());
        assert_eq!(
            plan_double(&list, &guid_of(&list, 0)),
            Ok(guid_of(&list, 0))
        );
    }

    #[test]
    fn the_first_arrangement_wraps_the_part_in_its_instrument() {
        let list = tracks(&[("Guitar Clean", 1), ("Main", 0), ("DBL", -1)]);
        // From the part, or from a side of it.
        for from in [0, 2] {
            assert_eq!(
                plan_arrangement(&list, &guid_of(&list, from), None),
                Ok(ArrangementPlan::Wrap {
                    part: guid_of(&list, 0),
                    instrument: "Guitar".to_owned(),
                    role: "Rhythm".to_owned(),
                    name: "Lead".to_owned(),
                })
            );
        }
        assert!(
            plan_arrangement(&list, &guid_of(&list, 0), Some("rhythm")).is_err(),
            "the part's own new name is taken"
        );
    }

    #[test]
    fn a_later_arrangement_goes_beside_the_others() {
        let list = tracks(&[
            ("Guitar", 1),
            ("Rhythm", 1),
            ("Main", 0),
            ("DBL", -1),
            ("Lead", 1),
            ("Main", 1),
            ("Main", 0),
            ("DBL", -1),
            ("Harmony", -2),
        ]);
        let guitar = guid_of(&list, 0);
        // From a side of a double inside a part beside a Main: Guitar.
        for from in [0, 2, 6, 8] {
            assert_eq!(
                plan_arrangement(&list, &guid_of(&list, from), None),
                Ok(ArrangementPlan::Beside {
                    container: guitar.clone(),
                    part: match from {
                        0 => guitar.clone(),
                        2 => guid_of(&list, 1),
                        _ => guid_of(&list, 4),
                    },
                    rename: None,
                    name: "Chug".to_owned(),
                }),
                "from track {from}"
            );
        }
    }

    #[test]
    fn the_only_part_in_its_folder_is_renamed_to_its_role() {
        let list = tracks(&[("Guitar", 1), ("Guitar Clean", -1)]);
        assert_eq!(
            plan_arrangement(&list, &guid_of(&list, 1), Some("Lead")),
            Ok(ArrangementPlan::Beside {
                container: guid_of(&list, 0),
                part: guid_of(&list, 1),
                rename: Some((guid_of(&list, 1), "Rhythm".to_owned())),
                name: "Lead".to_owned(),
            })
        );
    }

    #[test]
    fn a_part_beside_a_main_goes_in_with_the_others() {
        let list = tracks(&[
            ("Lead", 1),
            ("Main", 1),
            ("Main", 0),
            ("DBL", -1),
            ("Harmony", -1),
        ]);
        let expected = Ok(PartPlan::Beside {
            container: guid_of(&list, 0),
            main: Some(guid_of(&list, 1)),
            name: "OCT".to_owned(),
        });
        for from in [0, 1, 3, 4] {
            assert_eq!(
                plan_part(&list, &guid_of(&list, from), None),
                expected,
                "from track {from}"
            );
        }
        assert!(plan_part(&list, &guid_of(&list, 0), Some("DBL")).is_err());
        assert!(plan_part(&list, &guid_of(&list, 0), Some("harmony")).is_err());
    }

    #[test]
    fn a_lone_part_becomes_main_under_its_own_name() {
        let list = tracks(&[
            ("Guitar", 1),
            ("Rhythm", 0),
            ("Lead", 1),
            ("Main", 0),
            ("DBL", -2),
        ]);
        assert_eq!(
            plan_part(&list, &guid_of(&list, 4), Some("Harmony")),
            Ok(PartPlan::Wrap {
                part: guid_of(&list, 2),
                name: "Harmony".to_owned(),
            })
        );
    }
}
