use architect::platform::Instant;
use daw::module::ModuleContext;
use daw::service::transport::service::Transport as TransportService;
use daw::service::{Markers, MusicalPosition, ProjectContext, Projects, Region, Regions, TempoMap};
use keyflow::sections::colors_for_section_type;
use session_proto::SectionType;
use session_proto::keyflow_actions::{KeyflowActions, register_keyflow_actions};
use session_proto::ruler_lanes::{CoreLane, FtsLane, InstrumentLane, classify_marker_lane};
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

const TOUCH_EPSILON_SECONDS: f64 = 0.001;
const MAX_SECTION_TIME_SELECTION_SECONDS: f64 = 60.0 * 60.0;

// The vocabulary itself lives in `crate::section_kinds` so wasm callers
// (the browser setlist engine) can name a section without pulling in
// this module's REAPER dependencies. Re-exported here so every existing
// `keyflow::actions::SectionKind` path keeps resolving.
pub use crate::section_kinds::{KeyflowAction, MarkerKind, SectionKind};

#[derive(Debug, Clone)]
struct SectionRegion {
    id: u32,
    start: f64,
    end: f64,
    section_type: SectionType,
}

pub const fn init(_ctx: &ModuleContext) {}

/// Run one keyflow section/marker action directly against `daw`.
///
/// `register_actions` is the REAPER-facing path (wires these up as named
/// actions an `ActionBackend` can bind a keystroke to). A host with no
/// action menu — a standalone toolbar button, say — has nothing to bind
/// to and just wants to fire the action once; this is that entry point,
/// same dispatch either way.
pub fn dispatch<D>(daw: &D, action: KeyflowAction)
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    tracing::debug!(?action, "[session] Dispatching Keyflow action");
    if let Err(err) = run_action(daw, action) {
        tracing::error!(?err, "[session] Keyflow action failed");
    } else {
        tracing::debug!(?action, "[session] Keyflow action completed");
    }
}

fn run_action<D>(daw: &D, action: KeyflowAction) -> eyre::Result<()>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    let started = architect::platform::now();
    ensure_action_lane(daw, action);

    match action {
        KeyflowAction::InsertSection(kind) => insert_section_region(daw, kind)?,
        KeyflowAction::InsertMarker(kind) => {
            insert_marker(daw, kind)?;
            normalize_marker_lanes(daw)?;
        }
        KeyflowAction::ConvertMarkersToSessionFormat => {
            convert_markers_to_session_format(daw)?;
            normalize_marker_lanes(daw)?;
        }
    }

    hide_stray_lanes(daw);

    debug!(
        ?action,
        elapsed_ms = started.elapsed().as_millis(),
        "[session] Keyflow action timing",
    );
    Ok(())
}

fn ensure_action_lane<D>(daw: &D, action: KeyflowAction)
where
    D: Projects,
{
    ensure_core_lanes(daw);

    let lane = match action {
        KeyflowAction::InsertSection(_) | KeyflowAction::ConvertMarkersToSessionFormat => {
            FtsLane::Core(CoreLane::Sections)
        }
        KeyflowAction::InsertMarker(kind) => classify_marker_lane(kind.name()),
    };
    // RULER_LANE_NAME uses 0-based name_key_index, *not* 1-based
    // lane_index. Mixing them rewrote lane 2 to "SECTIONS" every
    // insert and left an extra unnamed lane above SONG. See the
    // matching fix in ensure_core_lanes.
    let key_idx = match lane {
        FtsLane::Core(core) => core.name_key_index(),
        // Instrument lanes follow CoreLane::count() and stay 1-based
        // for now; if they ever break the same way, swap to a
        // dedicated `name_key_index` on InstrumentLane.
        FtsLane::Instrument(_) => lane.lane_index(),
    };
    daw.set_ruler_lane_name(ProjectContext::Current, key_idx, lane.display_name());
}

pub(crate) fn ensure_core_lanes<D>(daw: &D)
where
    D: Projects,
{
    let project = ProjectContext::Current;
    for lane in CoreLane::all() {
        // RULER_LANE_NAME:N / RULER_LANE_FLAGS:N use the 0-based
        // *name-table* index (N=0 names the leftmost UI lane), not
        // the 1-based `lane_index` markers/regions use via
        // `I_LANENUMBER`. Passing `lane_index` (1-based) here named
        // lane 2 in the UI and left an empty unnamed lane 1 above
        // SONG. Use `name_key_index` so the rename hits the lane the
        // user actually sees.
        let key_idx = lane.name_key_index();
        daw.set_ruler_lane_name(project.clone(), key_idx, lane.display_name());
        // SECTIONS = 8 (default region lane), SONG = 4 (default
        // marker lane). The flag steers *future* inserts; the
        // explicit `set_lane` calls in normalize_section_regions /
        // place_marker_via_action handle existing entities.
        daw.set_project_info(
            project.clone(),
            &format!("RULER_LANE_FLAGS:{key_idx}"),
            f64::from(lane.flags()),
        );
    }
}

/// Hide any ruler lanes that aren't part of the FTS layout and have no
/// markers/regions on them. REAPER's fresh-project "1" fallback lane can
/// linger after FTS lanes are registered; this clears it without touching
/// lanes the user has actually populated.
pub(crate) fn hide_stray_lanes<D>(daw: &D)
where
    D: Projects + Markers + Regions,
{
    let project = ProjectContext::Current;
    let valid_names: HashSet<&str> = CoreLane::all()
        .iter()
        .map(session_proto::ruler_lanes::CoreLane::display_name)
        .chain(
            InstrumentLane::all()
                .iter()
                .map(session_proto::ruler_lanes::InstrumentLane::display_name),
        )
        .collect();

    let mut used_lanes: HashSet<u32> = HashSet::new();
    for m in Markers::all(daw, project.clone()) {
        if let Some(lane) = m.lane {
            used_lanes.insert(lane);
        }
    }
    for r in Regions::all(daw, project.clone()) {
        if let Some(lane) = r.lane {
            used_lanes.insert(lane);
        }
    }

    // Probe a fixed range past the named-lanes count instead of
    // stopping at `ruler_lane_count` — REAPER sometimes leaves a
    // trailing empty lane after a sequence of `RULER_LANE_ORDER`
    // inserts, and that empty lane sits beyond what `ruler_lane_count`
    // reports. 12 is generous (we currently use 3 core lanes and
    // ~8 instrument lanes; anything past that is the user's).
    //
    // Both `RULER_LANE_HIDDEN:N` and `I_LANENUMBER` use the same
    // 0-based index now, so a marker with `lane=Some(N)` shows up in
    // `used_lanes` with the same N we'd hide.
    for key_idx in 0u32..12 {
        let name = daw.get_ruler_lane_name(project.clone(), key_idx);
        if valid_names.contains(name.trim()) || used_lanes.contains(&key_idx) {
            continue;
        }
        daw.set_project_info(
            project.clone(),
            &format!("RULER_LANE_HIDDEN:{key_idx}"),
            1.0,
        );
    }
}

/// Convert the current project's section markers to session format — see
/// [`convert_markers_in`]. The action has no idea of the media's end, so a
/// last section with no marker after it is left a marker.
pub(crate) fn convert_markers_to_session_format<D>(daw: &D) -> eyre::Result<()>
where
    D: Projects + Markers + Regions,
{
    convert_markers_in(daw, &ProjectContext::Current, None, None).map(|_| ())
}

/// What [`convert_markers_in`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkersConverted {
    /// The section markers made regions, by name.
    pub sections: Vec<String>,
    /// The markers left as marks (notes: `Scream`, `Kick in`), by name.
    pub marks: Vec<String>,
    /// The song's span, when one was placed.
    pub song: Option<(f64, f64)>,
}

/// A multitrack's song structure, kept as markers, made the session's:
///
/// - every **section** marker (`IN`, `VS 1`, `PRE-CH 1`, `CH 2B`, `BD`,
///   `SOLO 2`, `OUT`, `A`, `B - Pre Chorus`) becomes a SECTIONS-lane region,
///   named as the marker was and coloured by its kind, running to the next
///   section — never to a note between them — and the last to `SONGEND`,
///   `=END` or `end` (the media's end), whichever is first after it;
/// - the **song**: a SONG-lane region over the sections, named `song` (else
///   the project's name), with `SONGSTART` and `SONGEND` markers at its
///   ends where the project has none;
/// - every **other** marker (`Scream`, `DRUM HIT`… a note) stays, a mark,
///   on the MARKS lane.
///
/// A project that already has section regions keeps them: only its
/// markers' lanes are set. Count-in markers stay markers.
///
/// # Errors
///
/// A backend call failed.
pub fn convert_markers_in<D>(
    daw: &D,
    project: &ProjectContext,
    song: Option<&str>,
    end: Option<f64>,
) -> eyre::Result<MarkersConverted>
where
    D: Projects + Markers + Regions,
{
    let section_lane = CoreLane::Sections.lane_index();
    let song_lane = CoreLane::Song.lane_index();
    let mut done = MarkersConverted::default();

    let regions = Regions::all(daw, project.clone());
    let has_sections = regions.iter().any(|r| {
        r.lane == Some(section_lane)
            || (r.lane != Some(song_lane) && parse_region_section_type(&r.name).is_some())
    });
    let mut markers: Vec<(f64, daw_proto::Marker)> = Markers::all(daw, project.clone())
        .into_iter()
        .filter_map(|m| Some((m.position.seconds()?, m)))
        .collect();
    markers.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at = |name: &str| {
        markers
            .iter()
            .find(|(_, m)| m.name.trim().eq_ignore_ascii_case(name))
            .map(|(t, _)| *t)
    };
    let (songstart, songend, abs_end) = (at("SONGSTART"), at("SONGEND"), at("=END"));

    if !has_sections {
        let sections: Vec<(f64, &daw_proto::Marker, SectionType)> = markers
            .iter()
            .filter_map(|(t, m)| Some((*t, m, marker_section_type(&m.name)?)))
            .collect();
        for (i, (start, marker, section_type)) in sections.iter().enumerate() {
            let next = sections.get(i.saturating_add(1)).map(|(t, ..)| *t);
            let last = [songend, abs_end, end]
                .into_iter()
                .flatten()
                .filter(|t| *t > start + TOUCH_EPSILON_SECONDS)
                .reduce(f64::min);
            let Some(stop) = next.or(last).filter(|t| *t > start + TOUCH_EPSILON_SECONDS) else {
                warn!(marker = %marker.name, start, "[session] Convert: no end for the last section");
                continue;
            };
            let Some(id) = marker.id else { continue };
            let region = Regions::add(daw, project.clone(), *start, stop, marker.name.trim())?;
            Regions::set_lane(daw, project.clone(), region, Some(section_lane))?;
            Regions::set_color(
                daw,
                project.clone(),
                region,
                section_type_color(section_type),
            )?;
            Markers::remove(daw, project.clone(), id)?;
            done.sections.push(marker.name.trim().to_string());
            let span = done
                .song
                .map_or((*start, stop), |(a, b)| (a.min(*start), b.max(stop)));
            done.song = Some(span);
        }
    }

    // The song: its region and its two markers, where it has none.
    if let Some((first, last)) = done.song {
        let (start, stop) = (songstart.unwrap_or(first), songend.unwrap_or(last));
        let name = song
            .map(str::to_string)
            .or_else(|| daw.info(project.clone()).ok().map(|info| info.name))
            .filter(|name| !name.trim().is_empty() && name != "Untitled")
            .unwrap_or_else(|| "Song".to_string());
        if !regions.iter().any(|r| r.lane == Some(song_lane)) && stop > start {
            let id = Regions::add(daw, project.clone(), start, stop, &name)?;
            Regions::set_lane(daw, project.clone(), id, Some(song_lane))?;
        }
        for (missing, kind, position) in [
            (songstart.is_none(), MarkerKind::SongStart, start),
            (songend.is_none(), MarkerKind::SongEnd, stop),
        ] {
            if missing {
                Markers::add(daw, project.clone(), position, kind.name())?;
            }
        }
        done.song = Some((start, stop));
    }

    // What is left is marks, on their lane.
    normalize_marker_lanes_in(daw, project)?;
    done.marks = Markers::all(daw, project.clone())
        .into_iter()
        .map(|m| m.name)
        .filter(|name| !is_song_marker(name))
        .collect();
    debug!(
        sections = done.sections.len(),
        marks = done.marks.len(),
        "[session] Convert markers to session format",
    );
    Ok(done)
}

/// A marker that is a section's start: a section name ([`parse_region_section_type`]),
/// or a lettered part (`A`, `B - Pre Chorus`) — by what follows the letter
/// when anything does. A count-in is a marker, never a section.
fn marker_section_type(name: &str) -> Option<SectionType> {
    let name = name.trim();
    if is_song_marker(name) {
        return None;
    }
    let lettered = |part: &str| part.len() == 1 && part.chars().all(|c| c.is_ascii_uppercase());
    let section = match name.split_once(" - ") {
        Some((letter, rest)) if lettered(letter.trim()) => {
            parse_region_section_type(rest).or_else(|| Some(SectionType::Custom(name.to_string())))
        }
        _ if lettered(name) => Some(SectionType::Custom(name.to_string())),
        _ => parse_region_section_type(name),
    };
    section.filter(|t| !matches!(t, SectionType::CountIn))
}

/// The song's own markers: its bounds, the count-in, the render tail.
fn is_song_marker(name: &str) -> bool {
    let upper = name.trim().to_uppercase();
    matches!(
        upper.as_str(),
        "SONGSTART"
            | "SONGEND"
            | "=START"
            | "=END"
            | "COUNT-IN"
            | "COUNT IN"
            | "COUNTIN"
            | "PREROLL"
            | "=PREROLL"
            | "POSTROLL"
            | "=POSTROLL"
    )
}

fn insert_marker<D>(daw: &D, kind: MarkerKind) -> eyre::Result<()>
where
    D: TransportService + Markers + TempoMap,
{
    let started = architect::platform::now();
    let project = ProjectContext::Current;
    let position = edit_cursor_position(daw, project.clone());
    let name = kind.name();
    let lane = classify_marker_lane(name).lane_index();
    let id = Markers::add(daw, project.clone(), position, name)?;
    Markers::set_color(daw, project.clone(), id, kind.default_color())?;
    Markers::set_lane(daw, project.clone(), id, Some(lane))?;
    if kind == MarkerKind::CountIn {
        let cursor_position = two_measures_after(daw, project.clone(), position);
        TransportService::set_position(daw, project, cursor_position)?;
    }
    debug!(
        ?kind,
        elapsed_ms = started.elapsed().as_millis(),
        "[session] Keyflow marker insert timing",
    );
    Ok(())
}

fn two_measures_after<D>(daw: &D, project: ProjectContext, position: f64) -> f64
where
    D: TempoMap,
{
    let (measure, beat, fraction) = daw.time_to_musical(project.clone(), position);
    daw.musical_to_time(project, measure.saturating_add(2), beat, fraction)
}

fn insert_section_region<D>(daw: &D, kind: SectionKind) -> eyre::Result<()>
where
    D: TransportService + Regions + TempoMap,
{
    let started = architect::platform::now();
    let project = ProjectContext::Current;
    let position = edit_cursor_position(daw, project.clone());
    let time_selection = daw.get_time_selection(project.clone());
    let regions = Regions::all(daw, project.clone());
    let used_time_selection = time_selection.as_ref().is_some_and(|selection| {
        is_valid_time_selection(selection.start_seconds, selection.end_seconds)
    });
    let (start, end) = infer_insert_bounds(daw, kind, position, time_selection.as_ref(), &regions);
    debug!(
        kind = ?kind,
        position,
        time_selection_start = ?time_selection.as_ref().map(|selection| selection.start_seconds),
        time_selection_end = ?time_selection.as_ref().map(|selection| selection.end_seconds),
        insert_start = start,
        insert_end = end,
        insert_length_seconds = end - start,
        "[session] Keyflow section bounds",
    );
    debug!("[session] Keyflow section insert: carving overlapping regions");
    carve_overlapping_section_regions(daw, &regions, start, end)?;
    debug!("[session] Keyflow section insert: carve complete");
    debug!("[session] Keyflow section insert: adding region");
    let id = Regions::add(
        daw,
        project.clone(),
        start,
        end,
        &kind.section_type().abbreviation(),
    )?;
    debug!(id, "[session] Keyflow section insert: add complete");
    debug!(id, "[session] Keyflow section insert: setting color");
    Regions::set_color(
        daw,
        project.clone(),
        id,
        section_type_color(&kind.section_type()),
    )?;
    debug!(id, "[session] Keyflow section insert: color complete");
    debug!(
        end,
        "[session] Keyflow section insert: moving cursor to end"
    );
    TransportService::set_position(daw, project.clone(), end)?;
    debug!("[session] Keyflow section insert: cursor move complete");
    if used_time_selection {
        debug!("[session] Keyflow section insert: clearing consumed time selection");
        TransportService::clear_time_selection(daw, project.clone())?;
    }

    debug!("[session] Keyflow section insert: reloading regions");
    let regions = Regions::all(daw, project);
    debug!(
        region_count = regions.len(),
        "[session] Keyflow section insert: reloaded regions",
    );
    debug!("[session] Keyflow section insert: normalizing section regions");
    normalize_section_regions(daw, regions)?;
    debug!("[session] Keyflow section insert: normalize complete");
    debug!(
        ?kind,
        elapsed_ms = started.elapsed().as_millis(),
        "[session] Keyflow section insert timing",
    );
    Ok(())
}

pub(crate) fn edit_cursor_position<D>(daw: &D, project: ProjectContext) -> f64
where
    D: TransportService,
{
    daw.get_state(project.clone())
        .edit_position
        .time
        .map_or_else(
            || daw.get_position(project),
            |position| position.as_seconds(),
        )
}

fn infer_insert_bounds<D>(
    daw: &D,
    kind: SectionKind,
    position: f64,
    time_selection: Option<&daw::service::LoopRegion>,
    regions: &[Region],
) -> (f64, f64)
where
    D: TempoMap,
{
    if let Some(selection) = time_selection
        .filter(|selection| is_valid_time_selection(selection.start_seconds, selection.end_seconds))
    {
        return (selection.start_seconds, selection.end_seconds);
    }
    if let Some(selection) = time_selection.filter(|selection| {
        is_valid_range(selection.start_seconds, selection.end_seconds)
            && !is_valid_time_selection(selection.start_seconds, selection.end_seconds)
    }) {
        warn!(
            kind = ?kind,
            selection_start = selection.start_seconds,
            selection_end = selection.end_seconds,
            selection_length_seconds = selection.end_seconds - selection.start_seconds,
            "[session] Ignoring oversized Keyflow section time selection",
        );
    }

    let musical_end = default_section_end(daw, kind, position);
    let next_section_start = regions
        .iter()
        .filter(|region| region.start_seconds() > position + TOUCH_EPSILON_SECONDS)
        .filter(|region| region.lane == Some(CoreLane::Sections.lane_index()))
        .map(Region::start_seconds)
        .min_by(f64::total_cmp);
    let end = choose_default_end(position, musical_end, next_section_start);

    (position, end)
}

fn carve_overlapping_section_regions<D>(
    daw: &D,
    regions: &[Region],
    insert_start: f64,
    insert_end: f64,
) -> eyre::Result<()>
where
    D: Regions,
{
    let section_lane = CoreLane::Sections.lane_index();
    let project = ProjectContext::Current;

    for region in regions {
        if region.lane != Some(section_lane) && parse_region_section_type(&region.name).is_none() {
            continue;
        }
        let Some(id) = region.id else {
            continue;
        };

        let start = region.start_seconds();
        let end = region.end_seconds();
        if end <= insert_start + TOUCH_EPSILON_SECONDS
            || start >= insert_end - TOUCH_EPSILON_SECONDS
        {
            continue;
        }

        if start < insert_start - TOUCH_EPSILON_SECONDS {
            debug!(
                id,
                region_start = start,
                region_end = end,
                new_end = insert_start,
                "[session] Carving section region: trimming right edge",
            );
            Regions::set_bounds(daw, project.clone(), id, start, insert_start)?;
        } else if end > insert_end + TOUCH_EPSILON_SECONDS {
            debug!(
                id,
                region_start = start,
                region_end = end,
                new_start = insert_end,
                "[session] Carving section region: trimming left edge",
            );
            Regions::set_bounds(daw, project.clone(), id, insert_end, end)?;
        } else {
            debug!(
                id,
                region_start = start,
                region_end = end,
                "[session] Carving section region: removing covered region",
            );
            Regions::remove(daw, project.clone(), id)?;
        }
    }

    Ok(())
}

fn choose_default_end(position: f64, musical_end: f64, next_section_start: Option<f64>) -> f64 {
    next_section_start
        .filter(|next| next.is_finite())
        .map_or(musical_end, |next| next.min(musical_end))
        .max(position + TOUCH_EPSILON_SECONDS)
}

fn is_valid_range(start: f64, end: f64) -> bool {
    start.is_finite() && end.is_finite() && end > start + TOUCH_EPSILON_SECONDS
}

fn is_valid_time_selection(start: f64, end: f64) -> bool {
    is_valid_range(start, end) && end - start <= MAX_SECTION_TIME_SELECTION_SECONDS
}

fn default_section_end<D>(daw: &D, kind: SectionKind, position: f64) -> f64
where
    D: TempoMap,
{
    let project = ProjectContext::Current;
    let (measure, beat, fraction) = daw.time_to_musical(project.clone(), position);
    let start = musical_position_from_fraction(measure, beat, fraction);
    let measure_count = i32::try_from(kind.default_measure_count()).unwrap_or(4);
    let end = MusicalPosition::new(
        start.measure.saturating_add(measure_count),
        start.beat,
        start.subdivision,
    );
    let end_fraction = f64::from(end.subdivision) / 1000.0;
    daw.musical_to_time(project, end.measure, end.beat, end_fraction)
}

fn musical_position_from_fraction(measure: i32, beat: i32, fraction: f64) -> MusicalPosition {
    let subdivision = if fraction.is_finite() {
        let millis = (fraction * 1000.0).round();
        if millis >= 0.0 && millis <= 1000.0 {
            // Range-checked above (0.0..=1000.0); std has no non-`as`
            // float-to-int conversion.
            #[allow(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            {
                millis as i32
            }
        } else {
            0
        }
    } else {
        0
    };
    MusicalPosition::new(measure, beat, subdivision.clamp(0, 999))
}

pub(crate) fn normalize_section_regions<D>(daw: &D, regions: Vec<Region>) -> eyre::Result<()>
where
    D: Regions,
{
    let existing: HashMap<u32, (String, Option<u32>, Option<u32>)> = regions
        .iter()
        .filter_map(|region| Some((region.id?, (region.name.clone(), region.lane, region.color))))
        .collect();
    let normalized = normalized_section_updates(regions);
    let project = ProjectContext::Current;
    for update in normalized {
        let SectionUpdate {
            id,
            name,
            section_type,
        } = update;
        let desired_color = section_type_color(&section_type);
        let current = existing.get(&id);
        if current.is_none_or(|(current_name, _, _)| current_name != &name) {
            debug!(id, name, "[session] Normalizing section region: rename");
            Regions::rename(daw, project.clone(), id, &name)?;
        }
        if current.is_none_or(|(_, _, current_color)| *current_color != Some(desired_color)) {
            debug!(
                id,
                color = desired_color,
                "[session] Normalizing section region: set color",
            );
            Regions::set_color(daw, project.clone(), id, desired_color)?;
        }
        // Force the SECTIONS lane assignment too. `Regions::add`
        // lands in REAPER's default region lane, which is only
        // guaranteed to be SECTIONS after `ensure_core_lanes` has
        // written `RULER_LANE_FLAGS:N=8` — and that flag only steers
        // *future* inserts, not existing ones. Without this loop
        // applying the lane explicitly, freshly-stamped section
        // regions stick in lane 0 (REAPER's unnamed default).
        let section_lane = CoreLane::Sections.lane_index();
        if current.is_none_or(|(_, current_lane, _)| *current_lane != Some(section_lane)) {
            debug!(
                id,
                lane = section_lane,
                "[session] Normalizing section region: set lane",
            );
            Regions::set_lane(daw, project.clone(), id, Some(section_lane))?;
        }
    }
    Ok(())
}

pub(crate) fn normalize_marker_lanes<D>(daw: &D) -> eyre::Result<()>
where
    D: Markers,
{
    normalize_marker_lanes_in(daw, &ProjectContext::Current)
}

/// [`normalize_marker_lanes`], in `project`.
fn normalize_marker_lanes_in<D>(daw: &D, project: &ProjectContext) -> eyre::Result<()>
where
    D: Markers,
{
    let project = project.clone();
    for marker in Markers::all(daw, project.clone()) {
        let Some(id) = marker.id else {
            continue;
        };
        if let Some(color) = marker_color_for_name(&marker.name)
            && marker.color != Some(color)
        {
            Markers::set_color(daw, project.clone(), id, color)?;
        }
        let lane = classify_marker_lane(&marker.name).lane_index();
        if marker.lane != Some(lane) {
            Markers::set_lane(daw, project.clone(), id, Some(lane))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct SectionUpdate {
    id: u32,
    name: String,
    section_type: SectionType,
}

#[cfg(test)]
fn normalized_section_names(regions: Vec<Region>) -> Vec<(u32, String)> {
    normalized_section_updates(regions)
        .into_iter()
        .map(|update| (update.id, update.name))
        .collect()
}

fn normalized_section_updates(regions: Vec<Region>) -> Vec<SectionUpdate> {
    // Song boundaries: SONG-lane regions delimit songs on a shared timeline.
    // Section numbering resets at each song, so "Verse 2" means the second
    // verse *within its song*, not the second across the whole project.
    let song_lane = CoreLane::Song.lane_index();
    let mut song_starts: Vec<f64> = regions
        .iter()
        .filter(|region| region.lane == Some(song_lane))
        .map(daw_proto::Region::start_seconds)
        .collect();
    song_starts.sort_by(f64::total_cmp);

    let mut sections: Vec<_> = regions
        .into_iter()
        // A Song-lane region is never itself a section, no matter what it's
        // named — but its name is the project/song title, which collides
        // with a real `SectionType` word disturbingly often ("Intro",
        // "Bridge", "Outro", …). Without this exclusion, a song literally
        // titled "Intro" swept its own whole-song region into the section
        // list below, corrupting every other section's numbering.
        .filter(|region| region.lane != Some(CoreLane::Song.lane_index()))
        .filter(|region| {
            region.lane == Some(CoreLane::Sections.lane_index())
                || parse_region_section_type(&region.name).is_some()
        })
        .filter_map(|region| {
            Some(SectionRegion {
                id: region.id?,
                start: region.start_seconds(),
                end: region.end_seconds(),
                section_type: parse_region_section_type(&region.name)?,
            })
        })
        .collect();

    sections.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| a.end.total_cmp(&b.end))
    });

    // Which song a section belongs to = the last SONG start at or before it.
    // Sections before the first song (or when there are no song regions) fall
    // into bucket 0, preserving the single-song / whole-project behavior.
    let song_of = |start: f64| -> usize {
        song_starts
            .partition_point(|&song_start| song_start <= start)
            .saturating_sub(1)
    };

    let mut by_song: std::collections::BTreeMap<usize, Vec<SectionRegion>> =
        std::collections::BTreeMap::new();
    for section in sections {
        by_song
            .entry(song_of(section.start))
            .or_default()
            .push(section);
    }

    let mut updates = Vec::new();
    for (_song, song_sections) in by_song {
        updates.extend(number_sections_in_song(song_sections));
    }
    updates
}

/// Number one song's sections: each section type is numbered by occurrence
/// within the song (Verse 1, Verse 2, …), touching sections of the same type
/// collapsing into one lettered group.
fn number_sections_in_song(sections: Vec<SectionRegion>) -> Vec<SectionUpdate> {
    let mut by_type: HashMap<String, Vec<SectionRegion>> = HashMap::new();
    for section in sections {
        by_type
            .entry(section.section_type.key())
            .or_default()
            .push(section);
    }

    let mut updates = Vec::new();
    for mut same_type in by_type.into_values() {
        same_type.sort_by(|a, b| {
            a.start
                .total_cmp(&b.start)
                .then_with(|| a.end.total_cmp(&b.end))
        });
        let groups = section_groups(&same_type);
        let multiple_groups = groups.len() > 1;

        for (group_index, group) in groups.iter().enumerate() {
            let group_number = group_index.saturating_add(1);
            let multiple_in_group = group.len() > 1;

            for (letter_index, section_index) in group.iter().enumerate() {
                let Some(section) = same_type.get(*section_index) else {
                    continue;
                };
                let new_name = section_name(
                    &section.section_type,
                    multiple_groups,
                    group_number,
                    multiple_in_group,
                    letter_index,
                );
                updates.push(SectionUpdate {
                    id: section.id,
                    name: new_name,
                    section_type: section.section_type.clone(),
                });
            }
        }
    }

    updates
}

fn section_groups(sections: &[SectionRegion]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (idx, section) in sections.iter().enumerate() {
        if let Some(group) = groups.last_mut()
            && let Some(prev_idx) = group.last()
            && let Some(prev) = sections.get(*prev_idx)
            && (prev.end - section.start).abs() <= TOUCH_EPSILON_SECONDS
        {
            group.push(idx);
            continue;
        }
        groups.push(vec![idx]);
    }
    groups
}

fn section_name(
    section_type: &SectionType,
    multiple_groups: bool,
    group_number: usize,
    multiple_in_group: bool,
    letter_index: usize,
) -> String {
    let base = section_type.abbreviation();
    match (multiple_groups, multiple_in_group) {
        (false, false) => base,
        (false, true) => format!("{} {}", base, alpha_suffix(letter_index)),
        (true, false) => format!("{base} {group_number}"),
        (true, true) => format!("{} {}{}", base, group_number, alpha_suffix(letter_index)),
    }
}

fn alpha_suffix(index: usize) -> String {
    let mut n = index;
    let mut chars = Vec::new();
    loop {
        let offset = u8::try_from(n % 26).unwrap_or(0);
        if let Some(c) = char::from_u32(u32::from(b'A').saturating_add(u32::from(offset))) {
            chars.push(c);
        }
        n = n.saturating_div(26);
        if n == 0 {
            break;
        }
        n = n.saturating_sub(1);
    }
    chars.iter().rev().collect()
}

pub fn parse_region_section_type(name: &str) -> Option<SectionType> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut token = trimmed.split_whitespace().next()?;
    if token.ends_with(':') {
        token = token.get(..token.len().saturating_sub(1)).unwrap_or("");
    }

    SectionType::parse(token)
        .ok()
        .or_else(|| SectionType::parse(trimmed).ok())
        .or_else(|| parse_loose(trimmed))
}

/// The spellings real REAPER regions actually carry, which the canonical
/// vocabulary does not accept.
///
/// Surveyed against the Crescendum album's 58 distinct region names: the
/// canonical parser already handles `pre-chorus`, `Pre- Chorus`, `CH 1`,
/// `Solo {GTR}` and `OUT 2`, but not `pre chorus` — the plain space-separated
/// form, and the single most common section name in the album after `chorus`
/// itself. Those regions were silently dropped from every setlist.
///
/// # Order is the whole design
///
/// Hyphen-joining is tried **before** the trailing word. Reversed, `pre
/// chorus` would parse as `Chorus` — not a failure anyone would notice,
/// just five regions quietly filed under the wrong section for the rest of
/// the song's life. A wrong answer here is worse than no answer.
fn parse_loose(trimmed: &str) -> Option<SectionType> {
    // "pre chorus/drum solo" — a region named for two things is named for the
    // first one.
    let head = trimmed.split('/').next().unwrap_or(trimmed).trim();
    if head.is_empty() {
        return None;
    }

    // A bare "PRE" is pre-chorus. Observed, not invented: it appears four
    // times across the album, always in songs that also carry a plain
    // "chorus", and the canonical vocabulary spells the code "PC". Kept as an
    // explicit single alias rather than a rule about prefixes, because that
    // is the only reading it has ever had here.
    if head.eq_ignore_ascii_case("pre") {
        return SectionType::parse("pre-chorus").ok();
    }

    // "pre chorus" -> "pre-chorus", which the canonical parser knows.
    let hyphenated = head.split_whitespace().collect::<Vec<_>>().join("-");
    if let Ok(parsed) = SectionType::parse(&hyphenated) {
        return Some(parsed);
    }
    if head != trimmed
        && let Ok(parsed) = SectionType::parse(head)
    {
        return Some(parsed);
    }

    // Only now the trailing word: "1st solo", "2nd solo", "drum solo" are all
    // solos. Reaching this point means no whole-name reading worked, so a
    // qualifier-plus-section shape is the remaining sensible guess.
    let last = head.split_whitespace().last()?;
    if last == head {
        return None;
    }
    SectionType::parse(last).ok()
}

/// The colour keyflow gives a section of `section_type`, packed as the
/// engine keeps a region's (REAPER's native colour): its palette's bright
/// shade, a count-in the count-in marker's.
#[must_use]
pub fn section_type_color(section_type: &SectionType) -> u32 {
    if matches!(section_type, SectionType::CountIn) {
        return MarkerKind::CountIn.default_color();
    }

    // Packed here rather than by the palette's `to_reaper_native`: the
    // palette keyflow builds on still packs RGB off Windows, which every
    // engine reads back byte-swapped (a chorus came out orange).
    let bright = colors_for_section_type(section_type).bright;
    reaper_native_rgb(
        (u32::from(bright.r()) << 16) | (u32::from(bright.g()) << 8) | u32::from(bright.b()),
    )
}

fn marker_color_for_name(name: &str) -> Option<u32> {
    let upper = name.trim().to_uppercase();
    match upper.as_str() {
        "COUNT-IN" | "COUNT IN" | "COUNTIN" => Some(MarkerKind::CountIn.default_color()),
        "SONGSTART" => Some(MarkerKind::SongStart.default_color()),
        "SONGEND" => Some(MarkerKind::SongEnd.default_color()),
        "=START" => Some(MarkerKind::Start.default_color()),
        "=END" => Some(MarkerKind::End.default_color()),
        _ => None,
    }
}

impl SectionKind {
    /// The keyflow `SectionType` this kind stamps a region as — also
    /// what a non-REAPER caller (a toolbar button) uses to look up the
    /// same color the region gets, via
    /// `keyflow::sections::colors_for_section_type`.
    #[must_use]
    pub fn section_type(self) -> SectionType {
        match self {
            Self::Intro => SectionType::Intro,
            Self::Verse => SectionType::Verse,
            Self::PreChorus => SectionType::Pre(Box::new(SectionType::Chorus)),
            Self::Chorus => SectionType::Chorus,
            Self::Bridge => SectionType::Bridge,
            Self::Outro => SectionType::Outro,
            Self::Instrumental => SectionType::Instrumental,
            Self::Solo => SectionType::Solo,
            Self::Hits => SectionType::Hits,
            Self::Interlude => SectionType::Interlude,
            Self::Breakdown => SectionType::Breakdown,
            Self::Vamp => SectionType::Vamp,
            Self::Refrain => SectionType::Refrain,
            Self::Tag => SectionType::Tag,
            Self::Turnaround => SectionType::Turnaround,
            Self::CountIn => SectionType::CountIn,
            Self::End => SectionType::End,
        }
    }

    /// Map a keyflow `SectionType` back to the session `SectionKind` used for
    /// marker insertion. `Pre(_)` collapses to `PreChorus`, `Opening`→`Intro`,
    /// `Post(_)`→`Outro`, and `Custom(_)` falls back to `Instrumental` — the
    /// session model has no free-form section kind, so a custom section is
    /// stamped as a generic instrumental region (its label is carried
    /// separately).
    pub(crate) const fn from_section_type(st: &SectionType) -> Self {
        match st {
            SectionType::Intro | SectionType::Opening => Self::Intro,
            SectionType::Verse => Self::Verse,
            SectionType::Chorus => Self::Chorus,
            SectionType::Bridge => Self::Bridge,
            SectionType::Outro | SectionType::Post(_) => Self::Outro,
            SectionType::Instrumental | SectionType::Custom(_) => Self::Instrumental,
            SectionType::Solo => Self::Solo,
            SectionType::CountIn => Self::CountIn,
            SectionType::End => Self::End,
            SectionType::Hits => Self::Hits,
            SectionType::Interlude => Self::Interlude,
            SectionType::Breakdown => Self::Breakdown,
            SectionType::Vamp => Self::Vamp,
            SectionType::Refrain => Self::Refrain,
            SectionType::Tag => Self::Tag,
            SectionType::Turnaround => Self::Turnaround,
            SectionType::Pre(_) => Self::PreChorus,
        }
    }

    pub(crate) fn default_measure_count(self) -> u32 {
        self.verified_default_measure_count().unwrap_or(4)
    }

    const fn verified_default_measure_count(self) -> Option<u32> {
        match self {
            Self::CountIn | Self::Turnaround => Some(2),
            Self::Verse | Self::Chorus | Self::Bridge | Self::Refrain => Some(8),
            Self::Intro | Self::Outro | Self::Instrumental | Self::Tag => Some(4),
            Self::PreChorus
            | Self::Solo
            | Self::Hits
            | Self::Interlude
            | Self::Breakdown
            | Self::Vamp
            | Self::End => None,
        }
    }
}

impl MarkerKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::CountIn => "COUNT-IN",
            Self::Start => "=START",
            Self::End => "=END",
            Self::SongStart => "SONGSTART",
            Self::SongEnd => "SONGEND",
        }
    }

    const fn default_color(self) -> u32 {
        reaper_native_rgb(self.rgb())
    }
}

impl SectionKind {
    /// The colour its region is stamped in, as CSS (`#rrggbb`) — so a
    /// toolbar button and the region it inserts cannot disagree.
    #[must_use]
    pub fn css_color(self) -> String {
        let native = section_type_color(&self.section_type());
        // Undo `reaper_native_rgb`: BGR with a flag.
        let rgb = ((native & 0xff) << 16) | (native & 0xff00) | ((native >> 16) & 0xff);
        format!("#{rgb:06x}")
    }
}

/// `0xRRGGBB` as REAPER's native colour, `0x01BBGGRR` — on every
/// platform: the packing is the file format's and the API's, not the
/// machine's (a branch that wrote RGB off Windows byte-swapped every
/// colour a Linux session set).
const fn reaper_native_rgb(rgb: u32) -> u32 {
    let r = rgb & 0x0000FF;
    let g = rgb & 0x00FF00;
    let b = rgb & 0xFF0000;
    0x01000000 | (r << 16) | g | (b >> 16)
}

// ── architect::actions implementation ───────────────────────────────────
//
// The contract lives in `session_proto::keyflow_actions`. `KeyflowAction`
// / `SectionKind` / `MarkerKind` stay: unlike the pass-through enums in
// the other action modules, `run_action` genuinely matches on them, so
// they are domain logic rather than a parallel dispatch path. What's gone
// is the string-keyed `action_for_id` lookup and the `session_actions`
// `define_actions!` entries that declared the same `FTS_SESSION_*` command
// ids a second time.

/// Serves the twenty keyflow-insert actions against a `daw` backend.
pub struct KeyflowActionsImpl<D> {
    daw: D,
}

impl<D> KeyflowActions for KeyflowActionsImpl<D>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    fn insert_intro_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Intro));
    }
    fn insert_verse_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Verse));
    }
    fn insert_pre_chorus_region(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertSection(SectionKind::PreChorus),
        );
    }
    fn insert_chorus_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Chorus));
    }
    fn insert_bridge_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Bridge));
    }
    fn insert_outro_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Outro));
    }
    fn insert_instrumental_region(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertSection(SectionKind::Instrumental),
        );
    }
    fn insert_solo_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Solo));
    }
    fn insert_hits_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Hits));
    }
    fn insert_interlude_region(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertSection(SectionKind::Interlude),
        );
    }
    fn insert_breakdown_region(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertSection(SectionKind::Breakdown),
        );
    }
    fn insert_vamp_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::Vamp));
    }
    fn insert_count_in_region(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertSection(SectionKind::CountIn),
        );
    }
    fn insert_end_region(&self) {
        dispatch(&self.daw, KeyflowAction::InsertSection(SectionKind::End));
    }
    fn insert_count_in_marker(&self) {
        dispatch(&self.daw, KeyflowAction::InsertMarker(MarkerKind::CountIn));
    }
    fn insert_start_marker(&self) {
        dispatch(&self.daw, KeyflowAction::InsertMarker(MarkerKind::Start));
    }
    fn insert_end_marker(&self) {
        dispatch(&self.daw, KeyflowAction::InsertMarker(MarkerKind::End));
    }
    fn insert_songstart_marker(&self) {
        dispatch(
            &self.daw,
            KeyflowAction::InsertMarker(MarkerKind::SongStart),
        );
    }
    fn insert_songend_marker(&self) {
        dispatch(&self.daw, KeyflowAction::InsertMarker(MarkerKind::SongEnd));
    }
    fn convert_markers_to_session_format(&self) {
        dispatch(&self.daw, KeyflowAction::ConvertMarkersToSessionFormat);
    }
}

/// Registers all twenty keyflow actions with `backend`, dispatching each
/// through a fresh `KeyflowActionsImpl` bound to `daw`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: Projects + TransportService + Markers + Regions + TempoMap + Send + Sync + 'static,
    B: ::architect::action::ActionBackend + ?Sized,
{
    register_keyflow_actions(backend, std::sync::Arc::new(KeyflowActionsImpl { daw }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw::service::Region;

    fn region(id: u32, start: f64, end: f64, name: &str) -> Region {
        let mut region = Region::from_seconds(start, end, name.to_string());
        region.id = Some(id);
        region.lane = Some(CoreLane::Sections.lane_index());
        region
    }

    #[test]
    fn one_section_keeps_plain_abbreviation() {
        assert_eq!(
            normalized_section_names(vec![region(1, 0.0, 8.0, "Chorus")]),
            vec![(1, "CH".to_string())]
        );
    }

    #[test]
    fn touching_sections_get_letters() {
        assert_eq!(
            normalized_section_names(vec![region(1, 0.0, 8.0, "CH"), region(2, 8.0, 16.0, "CH"),]),
            vec![(1, "CH A".to_string()), (2, "CH B".to_string())]
        );
    }

    /// A song titled "Intro" (or "Bridge", "Outro", …) has a whole-song
    /// region whose name is also a real `SectionType` word. That region
    /// must never be treated as a section itself — regardless of its name
    /// — or it gets swept into the real sections' numbering and corrupts
    /// it (a real bug found while validating the Rockstars album: the
    /// song "Intro" lost one of its four real sections this way).
    #[test]
    fn song_lane_region_is_never_treated_as_a_section_even_if_named_like_one() {
        let mut song_region = region(99, 0.0, 24.0, "Intro");
        song_region.lane = Some(CoreLane::Song.lane_index());

        assert_eq!(
            normalized_section_names(vec![
                song_region,
                region(1, 0.0, 8.0, "IN"),
                region(2, 8.0, 16.0, "IN"),
            ]),
            vec![(1, "IN A".to_string()), (2, "IN B".to_string())],
            "the Song-lane region must be excluded, leaving only the two real sections"
        );
    }

    #[test]
    fn separated_sections_get_numbers() {
        assert_eq!(
            normalized_section_names(vec![region(1, 0.0, 8.0, "CH"), region(2, 20.0, 28.0, "CH"),]),
            vec![(1, "CH 1".to_string()), (2, "CH 2".to_string())]
        );
    }

    #[test]
    fn separated_touching_runs_get_numbers_and_letters() {
        assert_eq!(
            normalized_section_names(vec![
                region(1, 0.0, 8.0, "CH"),
                region(2, 8.0, 16.0, "CH"),
                region(3, 32.0, 40.0, "CH"),
            ]),
            vec![
                (1, "CH 1A".to_string()),
                (2, "CH 1B".to_string()),
                (3, "CH 2".to_string()),
            ]
        );
    }

    #[test]
    fn pre_chorus_uses_keyflow_abbreviation() {
        assert_eq!(
            section_name(
                &SectionType::Pre(Box::new(SectionType::Chorus)),
                false,
                1,
                false,
                0,
            ),
            "PRE-CH"
        );
    }

    #[test]
    fn default_end_is_capped_by_next_section() {
        let result = choose_default_end(4.0, 20.0, Some(12.0));
        assert!((result - 12.0).abs() < 1e-10);
    }

    #[test]
    fn default_end_uses_musical_end_without_next_section() {
        let result = choose_default_end(4.0, 20.0, None);
        assert!((result - 20.0).abs() < 1e-10);
    }

    #[test]
    fn oversized_time_selection_is_not_valid_for_section_insert() {
        assert!(is_valid_time_selection(4.0, 12.0));
        assert!(!is_valid_time_selection(0.0, 64_000.0));
    }

    #[test]
    fn section_default_measure_counts_match_session_convention() {
        assert_eq!(SectionKind::Verse.default_measure_count(), 8);
        assert_eq!(SectionKind::Chorus.default_measure_count(), 8);
        assert_eq!(SectionKind::Bridge.default_measure_count(), 8);
        assert_eq!(SectionKind::Intro.default_measure_count(), 4);
        assert_eq!(SectionKind::Outro.default_measure_count(), 4);
        assert_eq!(SectionKind::Instrumental.default_measure_count(), 4);
        assert_eq!(SectionKind::CountIn.default_measure_count(), 2);
    }

    #[test]
    fn unverified_section_defaults_fall_back_to_four_measures() {
        assert_eq!(SectionKind::PreChorus.default_measure_count(), 4);
        assert_eq!(SectionKind::Solo.default_measure_count(), 4);
        assert_eq!(SectionKind::Hits.default_measure_count(), 4);
        assert_eq!(SectionKind::Interlude.default_measure_count(), 4);
        assert_eq!(SectionKind::Breakdown.default_measure_count(), 4);
        assert_eq!(SectionKind::Vamp.default_measure_count(), 4);
        assert_eq!(SectionKind::End.default_measure_count(), 4);
    }

    #[test]
    fn musical_position_from_fraction_converts_to_subdivision() {
        assert_eq!(
            musical_position_from_fraction(4, 2, 0.5),
            MusicalPosition::new(4, 2, 500)
        );
        assert_eq!(
            musical_position_from_fraction(4, 2, f64::INFINITY),
            MusicalPosition::new(4, 2, 0)
        );
    }

    #[test]
    fn section_colors_use_keyflow_palette() {
        // Keyflow's chorus, blue-500 (#3b82f6), as REAPER packs it on
        // every platform: 0x01BBGGRR.
        let bright = colors_for_section_type(&SectionType::Chorus).bright;
        assert_eq!((bright.r(), bright.g(), bright.b()), (0x3b, 0x82, 0xf6));
        assert_eq!(section_type_color(&SectionType::Chorus), 0x01f6_823b);
        assert_eq!(SectionKind::Chorus.css_color(), "#3b82f6");
        assert_eq!(
            section_type_color(&SectionType::CountIn),
            MarkerKind::CountIn.default_color()
        );
    }

    #[test]
    fn structural_markers_have_default_colors() {
        assert_eq!(
            marker_color_for_name("COUNT-IN"),
            Some(MarkerKind::CountIn.default_color())
        );
        assert_eq!(
            marker_color_for_name("SONGSTART"),
            Some(MarkerKind::SongStart.default_color())
        );
        assert_eq!(
            marker_color_for_name("=END"),
            Some(MarkerKind::End.default_color())
        );
    }
}
