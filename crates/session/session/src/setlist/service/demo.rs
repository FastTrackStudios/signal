//! Demo setlist — stamps markers and regions into the current REAPER project.
//!
//! Creates a multi-song setlist with proper ruler lane organization.
//! Each song is a parent region (SONG lane) containing section child regions
//! (SECTIONS lane), with structural markers in the MARKS lane and
//! render bounds in the MARKS lane.
//!
//! Layout (3 songs, ~12 minutes total):
//!
//! ```text
//! Song 1: "Great Is Thy Faithfulness" (0s–120s)
//!   COUNT-IN → SONGSTART → Intro → Verse 1 → Chorus → Verse 2 → Chorus → Outro → SONGEND
//!
//! Song 2: "Build My Life" (130s–270s)
//!   COUNT-IN → SONGSTART → Intro → Verse 1 → Pre-Chorus → Chorus → Bridge → Chorus → SONGEND
//!
//! Song 3: "Way Maker" (280s–430s)
//!   COUNT-IN → SONGSTART → Intro → Verse 1 → Chorus → Verse 2 → Chorus → Bridge → Chorus → Tag → SONGEND
//! ```

#![allow(dead_code)]

use daw::rpc;
use daw::service::transport::service::Transport as TransportService;
use daw::service::{Markers, ProjectContext, Projects, Regions, TempoMap};
use session_proto::SessionServiceError;
use session_proto::ruler_lanes::CoreLane;
use tracing::info;

use crate::keyflow::actions::{self as keyflow_actions, KeyflowAction, MarkerKind, SectionKind};

// ── Lane indices (1-based, matching CoreLane) ──────────────────────────────

const SECTIONS_LANE: u32 = CoreLane::Sections.lane_index(); // 2
const MARKS_LANE: u32 = CoreLane::Marks.lane_index(); // 3
const SONG_LANE: u32 = CoreLane::Song.lane_index(); // 1

// ── Types ──────────────────────────────────────────────────────────────────

/// A song fixture for seeding setlist test data. Public so test harnesses can
/// build arbitrary scenarios (see [`fixture_songs`] / [`stamp_song_native`]).
pub struct DemoSong {
    /// Song name (used for the SONG-lane parent region)
    pub name: &'static str,
    /// Absolute start of the song region (includes count-in)
    pub region_start: f64,
    /// Absolute end of the song region
    pub region_end: f64,
    /// Position of COUNT-IN marker (before SONGSTART)
    pub count_in: f64,
    /// Position of SONGSTART marker
    pub song_start: f64,
    /// Position of SONGEND marker
    pub song_end: f64,
    /// Position of =END marker (render tail)
    pub abs_end: f64,
    /// Song tempo in BPM (drives the per-song tempo point + guide grid).
    pub tempo_bpm: f64,
    /// Time-signature numerator (beats per measure).
    pub time_sig_num: u32,
    /// Time-signature denominator (beat unit).
    pub time_sig_den: u32,
    /// Sections within the song
    pub sections: Vec<DemoSection>,
}

/// A section fixture within a [`DemoSong`].
pub struct DemoSection {
    /// Section kind — drives both the inserted region's name (via
    /// `keyflow_actions`' abbreviation table) and which template colour
    /// the keyflow insert action picks. We carry the kind itself
    /// rather than a string so the demo can dispatch through the
    /// existing `insert_<kind>_region` actions and inherit all their
    /// lane / colour / carving logic.
    pub kind: SectionKind,
    pub start: f64,
    pub end: f64,
}

/// Stamp demo markers and regions into the current REAPER project.
///
/// This is a free function that takes a `&Daw` so it works both with
/// the global singleton and with a locally-held `Daw` instance (e.g.
/// from `daw_extension_runtime::connect()`).
///
/// # Errors
///
/// Returns an error if the backend is not a native DAW (requires calling [`stamp_demo_setlist_with`] instead).
pub async fn stamp_demo_setlist() -> Result<(), SessionServiceError> {
    Err(SessionServiceError::DawError(
        "stamp_demo_setlist requires a native DAW backend; use stamp_demo_setlist_with".to_string(),
    ))
}

/// # Errors
///
/// Returns an error if there is no current project or if stamping fails.
pub fn stamp_demo_setlist_with<D>(daw: &D) -> Result<(), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    let project = daw
        .current()
        .ok_or_else(|| SessionServiceError::DawError("No current project".to_string()))?;
    let pc = ProjectContext::Project(project.guid);
    stamp_demo_into_project_native(daw, &pc)
}

/// Stamp demo markers and regions into a specific REAPER project.
///
/// Use this when you already have a `Project` handle (e.g. in tests
/// where each test gets its own isolated project tab).
///
/// # Errors
///
/// Returns an error if the backend is not a native DAW (requires calling [`stamp_demo_into_project_native`] instead).
pub async fn stamp_demo_into_project(project: &rpc::Project) -> Result<(), SessionServiceError> {
    let _ = project;
    Err(SessionServiceError::DawError(
        "stamp_demo_into_project requires a native DAW backend; use stamp_demo_into_project_native"
            .to_string(),
    ))
}

/// Native sync version for in-process extension/session paths.
///
/// # Errors
///
/// Returns an error if the project cannot be focused or if stamping fails.
pub fn stamp_demo_into_project_native<D>(
    daw: &D,
    project: &ProjectContext,
) -> Result<(), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    info!(
        "Stamping demo setlist markers/regions into project {}",
        match &project {
            ProjectContext::Current => "current",
            ProjectContext::Project(guid) => guid,
        }
    );

    // Every keyflow_actions::dispatch call below operates on
    // `ProjectContext::Current`. If the caller asked for a different
    // project we have to bring it to the front first.
    if let ProjectContext::Project(guid) = &project
        && !daw.select(guid)
    {
        return Err(SessionServiceError::DawError(format!(
            "could not focus project {guid} before stamping demo"
        )));
    }

    let songs = demo_songs();

    // Per-song tempo: the default tempo is the first song's, then each song
    // drops a tempo point at its count-in so the guide grid + count-in beats
    // line up for that song (`SongBuilder::build` reads tempo from the project
    // snapshot). Praise's 127 is authoritative (its chart); the other songs'
    // tempos are detected from their click tracks (approximate).
    if let Some(first) = songs.first() {
        daw.set_default_tempo(project.clone(), first.tempo_bpm)
            .map_err(|e| SessionServiceError::DawError(format!("set demo tempo: {e}")))?;
        daw.set_default_time_signature(
            project.clone(),
            first.time_sig_num.cast_signed(),
            first.time_sig_den.cast_signed(),
        )
        .map_err(|e| SessionServiceError::DawError(format!("set demo time sig: {e}")))?;
    }
    for song in &songs {
        let idx = daw
            .add_tempo_point(project.clone(), song.count_in, song.tempo_bpm)
            .map_err(|e| SessionServiceError::DawError(format!("add tempo point: {e}")))?;
        daw.set_time_signature_at_point(
            project.clone(),
            idx,
            song.time_sig_num.cast_signed(),
            song.time_sig_den.cast_signed(),
        )
        .map_err(|e| SessionServiceError::DawError(format!("set song time sig: {e}")))?;
    }

    let mut total_markers = 0u32;
    let mut total_regions = 0u32;

    for song in &songs {
        let (m, r) = stamp_song_native(daw, project, song)?;
        total_markers = total_markers.saturating_add(m);
        total_regions = total_regions.saturating_add(r);
    }

    info!(
        "  Stamped {} markers, {} regions ({} songs)",
        total_markers,
        total_regions,
        songs.len()
    );
    info!("Demo markers/regions stamped successfully");
    Ok(())
}

/// Stamp a single [`DemoSong`] (SONG-lane region + structural markers +
/// section regions) into `project`. Returns `(markers, regions)` stamped.
///
/// Reused by [`stamp_demo_into_project_native`] and by test harnesses that
/// seed one song per project (see [`fixture_songs`]). The caller is
/// responsible for focusing `project` first if the backend requires it.
///
/// # Errors
///
/// Returns an error if the project cannot be focused or if stamping fails.
pub fn stamp_song_native<D>(
    daw: &D,
    project: &ProjectContext,
    song: &DemoSong,
) -> Result<(u32, u32), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    // Keyflow insert actions operate on the *current* project, so focus the
    // target project first when a specific one is requested.
    if let ProjectContext::Project(guid) = &project
        && !daw.select(guid)
    {
        return Err(SessionServiceError::DawError(format!(
            "could not focus project {guid} before stamping song"
        )));
    }

    // SONG-lane parent region (the song-bounded named region). No keyflow
    // action targets the SONG lane yet, so add via the trait + pin the lane.
    let song_region_id = Regions::add(
        daw,
        project.clone(),
        song.region_start,
        song.region_end,
        song.name,
    )
    .map_err(|e| SessionServiceError::DawError(format!("{e}")))?;
    Regions::set_lane(daw, project.clone(), song_region_id, Some(SONG_LANE))
        .map_err(|e| SessionServiceError::DawError(format!("{e}")))?;

    // Structural markers via the keyflow insert actions (lane/colour/convention).
    place_marker_via_action(daw, project, song.count_in, MarkerKind::CountIn)?;
    place_marker_via_action(daw, project, song.song_start, MarkerKind::SongStart)?;
    place_marker_via_action(daw, project, song.song_end, MarkerKind::SongEnd)?;
    place_marker_via_action(daw, project, song.abs_end, MarkerKind::End)?;

    // Section regions via the per-section insert actions (carve/colour/SECTIONS lane).
    for section in &song.sections {
        place_section_via_action(daw, project, section)?;
    }

    let section_count = u32::try_from(song.sections.len()).unwrap_or(u32::MAX);
    Ok((4, 1u32.saturating_add(section_count)))
}

/// Stamp one [`DemoSong`] into its OWN project.
///
/// Sets the project DEFAULT tempo and time-signature at t=0 (no tempo points,
/// no time-signature points), then stamps the song's regions/markers via
/// [`stamp_song_native`].
///
/// This is the per-song-project path — each song owns a project whose entire
/// timeline runs at that song's tempo, so the first downbeat lands on measure 1
/// and the guide/click grid + count-in line up natively. The caller must have
/// already created (`seed_project`) the target project; this focuses it.
///
/// # Errors
///
/// Returns an error if the project cannot be focused or if stamping fails.
pub fn stamp_song_with_default_tempo_native<D>(
    daw: &D,
    project: &ProjectContext,
    song: &DemoSong,
) -> Result<(u32, u32), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    if let ProjectContext::Project(guid) = project
        && !daw.select(guid)
    {
        return Err(SessionServiceError::DawError(format!(
            "could not focus project {guid} before stamping song"
        )));
    }
    daw.set_default_tempo(project.clone(), song.tempo_bpm)
        .map_err(|e| SessionServiceError::DawError(format!("set song tempo: {e}")))?;
    daw.set_default_time_signature(
        project.clone(),
        song.time_sig_num.cast_signed(),
        song.time_sig_den.cast_signed(),
    )
    .map_err(|e| SessionServiceError::DawError(format!("set song time sig: {e}")))?;
    stamp_song_native(daw, project, song)
}

/// Generate `count` self-contained song fixtures.
///
/// Each song starts near time 0 and can be stamped into its own project (one song per project).
/// Songs cycle through a few section layouts of increasing complexity and alternate
/// count-in presence, to exercise the full setlist view: count-in handling,
/// many sections, repeated section kinds, and short/long songs.
///
/// # Panics
///
/// Panics if the arrays of layouts or song names are empty (which they never are, as they
/// are statically initialized with non-empty content).
#[must_use]
pub fn fixture_songs(count: usize) -> Vec<DemoSong> {
    const NAMES: [&str; 12] = [
        "Cornerstone",
        "Great Are You Lord",
        "King of Kings",
        "Goodness of God",
        "Living Hope",
        "Who You Say I Am",
        "Reckless Love",
        "Build My Life",
        "Graves Into Gardens",
        "The Blessing",
        "Yes I Will",
        "Raise a Hallelujah",
    ];
    // Section layouts of increasing complexity (intro→...→outro).
    let layouts: [&[SectionKind]; 4] = [
        &[
            SectionKind::Intro,
            SectionKind::Verse,
            SectionKind::Chorus,
            SectionKind::Outro,
        ],
        &[
            SectionKind::Intro,
            SectionKind::Verse,
            SectionKind::Chorus,
            SectionKind::Verse,
            SectionKind::Chorus,
            SectionKind::Outro,
        ],
        &[
            SectionKind::Intro,
            SectionKind::Verse,
            SectionKind::PreChorus,
            SectionKind::Chorus,
            SectionKind::Verse,
            SectionKind::PreChorus,
            SectionKind::Chorus,
            SectionKind::Bridge,
            SectionKind::Chorus,
            SectionKind::Outro,
        ],
        &[
            SectionKind::Intro,
            SectionKind::Verse,
            SectionKind::Chorus,
            SectionKind::Bridge,
            SectionKind::Chorus,
        ],
    ];

    (0..count)
        .map(|i| {
            let layout = i
                .checked_rem(layouts.len())
                .and_then(|idx| layouts.get(idx))
                .unwrap_or(&layouts[0]);
            let has_count_in = i % 2 == 0;
            // Every song lives in its own project timeline starting at 0.
            // count-in occupies the first 4s when present; song body is 16s/section.
            let count_in_pos = 0.0;
            let song_start = if has_count_in { 4.0 } else { 0.0 };
            let sec_len = 16.0;
            let mut sections = Vec::with_capacity(layout.len());
            let mut t = song_start;
            for &kind in *layout {
                sections.push(DemoSection {
                    kind,
                    start: t,
                    end: t + sec_len,
                });
                t += sec_len;
            }
            let song_end = t;
            let abs_end = song_end + 4.0;
            DemoSong {
                name: i
                    .checked_rem(NAMES.len())
                    .and_then(|idx| NAMES.get(idx))
                    .unwrap_or(&NAMES[0]),
                region_start: 0.0,
                region_end: abs_end,
                count_in: count_in_pos,
                song_start,
                song_end,
                abs_end,
                tempo_bpm: 120.0,
                time_sig_num: 4,
                time_sig_den: 4,
                sections,
            }
        })
        .collect()
}

/// Convert a real chart layout (see [`crate::setlist::chart_import`]) into a
/// [`DemoSong`] ready for [`stamp_song_native`].
///
/// `ChartLayout` already lays every section out on a seconds timeline
/// (tempo/time-sig applied), so this is a direct field mapping rather than
/// a re-derivation — `name` is supplied by the caller because `DemoSong`
/// wants a `&'static str` and a parsed chart title is owned.
#[must_use]
pub fn chart_layout_to_demo_song(
    name: &'static str,
    layout: &crate::setlist::chart_import::ChartLayout,
) -> DemoSong {
    let abs_end = layout.song_end_seconds + 4.0;
    DemoSong {
        name,
        region_start: 0.0,
        region_end: abs_end,
        count_in: 0.0,
        song_start: layout.song_start_seconds,
        song_end: layout.song_end_seconds,
        abs_end,
        tempo_bpm: layout.tempo_bpm,
        time_sig_num: layout.time_sig_num,
        time_sig_den: layout.time_sig_den,
        sections: layout
            .sections
            .iter()
            .map(|s| DemoSection {
                kind: s.kind,
                start: s.start_seconds,
                end: s.end_seconds,
            })
            .collect(),
    }
}

/// Move the edit cursor to `position`, then dispatch the matching
/// `insert_<kind>_marker` keyflow action. The action handles
/// lane / colour / convention — the helper just bridges between
/// "we know exactly where it goes" and "the action reads the cursor".
fn place_marker_via_action<D>(
    daw: &D,
    project: &ProjectContext,
    position: f64,
    kind: MarkerKind,
) -> Result<(), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    TransportService::set_position(daw, project.clone(), position)
        .map_err(|e| SessionServiceError::DawError(format!("{e}")))?;
    keyflow_actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
    Ok(())
}

/// Set the project's time selection to the section's bounds, then
/// dispatch `insert_<section>_region`. Keyflow's
/// `infer_insert_bounds` consumes the time selection (clearing it
/// afterwards) so this is the only way to feed precise bounds
/// without rewriting the insert path.
fn place_section_via_action<D>(
    daw: &D,
    project: &ProjectContext,
    section: &DemoSection,
) -> Result<(), SessionServiceError>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    TransportService::set_position(daw, project.clone(), section.start)
        .map_err(|e| SessionServiceError::DawError(format!("{e}")))?;
    TransportService::set_time_selection(daw, project.clone(), section.start, section.end)
        .map_err(|e| SessionServiceError::DawError(format!("{e}")))?;
    keyflow_actions::dispatch(daw, KeyflowAction::InsertSection(section.kind));
    Ok(())
}

/// The bundled "Praise" chart (keyflow text) — the first real demo song. Its
/// section structure, tempo (127), key (A), and count-in are all derived from
/// this text via [`crate::setlist::chart_import`], exercising the live chart→sections
/// path end to end.
const PRAISE_CHART: &str = "\
Praise - Elevation Worship
#A 127bpm 4/4

Count 2
In 4
Refrain 8
VS 8
VS
PRE 2
CH 8
VS
VS
PRE
CH
CH
Interlude \"Breakdown\" 8
BR \"Down\" 8
BR \"Build\"
CH
CH
CH
INST \"Guitar Lead\" 8
Refrain
Refrain";

/// "God, I'm Just Grateful" (Elevation Worship) — the real keyflow chart.
const GOD_IM_JUST_GRATEFUL_CHART: &str = "\
God, I'm Just Grateful - Elevation Worship
72bpm 4/4 #D

Count 2
Intro 2
VS 8
CH 8
Breakdown 1
VS
CH
CH
Interlude 4
BR 8
BR
CH
CH
CH";

/// "Washed" (Elevation Rhythm) — the real keyflow chart.
const WASHED_CHART: &str = "\
Washed - Elevation Rhythm
139bpm 4/4 #B

Count 2
Intro 4
VS 8
VS
CH 8
VS
VS
CH
CH
INST 2
BR 8
BR
BR
BR
Refrain 8
INST 8
TAG 8
CH";

/// "Who Else" (Gateway Worship) — the real keyflow chart.
const WHO_ELSE_CHART: &str = "\
Who Else - Gateway Worship
68bpm #Ab 4/4

Count 2
VS 8
CH 8
VS
CH
CH
INST 4
BR 8
BR
CH
Tag 4";

/// "Thank God I'm Free" (Elevation Rhythm) — the real keyflow chart.
const THANK_GOD_IM_FREE_CHART: &str = "\
Thank God I'm Free - Elevation Rhythm
128bpm 4/4 #E

Count 2
IN 8
VS 8
VS
CH 8
Post 8
Interlude 4
VS
CH
Post
BR 8
BR
BR
Refrain 8
CH
Post
BR
BR
Refrain
Post
Post";

/// "Holy Forever" (Bethel Music) — the real keyflow chart. Uses the new
/// `Turnaround` ("Turn") section type. Only partially mapped so far.
const HOLY_FOREVER_CHART: &str = "\
Holy Forever - Bethel
72bpm 4/4 #Bb

Count 2
Intro 4
VS 8
PRE 8
CH 8
Turn 2
VS
Tag 2
CH
CH
PRE 8
PRE
CH
CH
Tag 12
CH
CH
PRE";

/// Build a [`DemoSong`] from a keyflow chart (the accurate path — tempo, time
/// signature, and real section lengths all come from the chart). The leading
/// `CountIn` section becomes the count-in marker (`count_in`/`song_start`); the
/// musical sections become SECTION-lane regions. Used for every song we have a
/// real chart for; songs without one fall back to `layout_worship_song`.
///
/// Returns `None` if the bundled chart text fails to parse (a bug in the
/// bundled demo data, not something a caller can recover from — logged and
/// skipped rather than crashing the demo-setlist load).
fn chart_song(name: &'static str, chart: &str) -> Option<DemoSong> {
    let layout = match crate::setlist::chart_import::chart_to_layout(chart) {
        Ok(layout) => layout,
        Err(e) => {
            tracing::error!("[demo] bundled chart for {name:?} failed to parse: {e}");
            return None;
        }
    };
    let sections: Vec<DemoSection> = layout
        .sections
        .iter()
        .filter(|s| s.kind != SectionKind::CountIn)
        .map(|s| DemoSection {
            kind: s.kind,
            start: s.start_seconds,
            end: s.end_seconds,
        })
        .collect();
    let tail = 4.0; // ring-out after the last section
    Some(DemoSong {
        name,
        region_start: 0.0,
        region_end: layout.song_end_seconds + tail,
        count_in: 0.0,
        song_start: layout.song_start_seconds,
        song_end: layout.song_end_seconds,
        abs_end: layout.song_end_seconds + tail,
        tempo_bpm: layout.tempo_bpm,
        time_sig_num: layout.time_sig_num,
        time_sig_den: layout.time_sig_den,
        sections,
    })
}

/// Map a lyrics-sheet section label to a [`SectionKind`]. Trailing numbers
/// ("Verse 1", "Chorus 2") are ignored here — the setlist auto-numbers
/// repeated kinds per song downstream.
fn label_to_kind(label: &str) -> SectionKind {
    let l = label.trim().to_ascii_lowercase();
    if l.starts_with("pre") {
        SectionKind::PreChorus
    } else if l.starts_with("verse") {
        SectionKind::Verse
    } else if l.starts_with("chorus") {
        SectionKind::Chorus
    } else if l.starts_with("refrain") {
        SectionKind::Refrain
    } else if l.starts_with("bridge") {
        SectionKind::Bridge
    } else if l.starts_with("interlude") {
        SectionKind::Interlude
    } else if l.starts_with("instrumental") || l.starts_with("inst") {
        SectionKind::Instrumental
    } else if l.starts_with("intro") {
        SectionKind::Intro
    } else if l.starts_with("outro") || l.starts_with("ending") || l.starts_with("tag") {
        // "Tag" reads as a structural close, like Outro (SectionKind has no Tag).
        SectionKind::Outro
    } else {
        SectionKind::Chorus
    }
}

/// Build a worship song `DemoSong` at `region_start` 0 (the caller shifts it into
/// place). 4/4 with a 2-measure count-in; the `duration_s` musical span (from
/// `song_start` to the audio end) is split among `sections` weighted by their
/// lyric line-count, so longer parts get proportionally more time. Section
/// timings are APPROXIMATE — derived from lyrics + a detected tempo, not a real
/// chart — but give a navigable, playable song over its seeded stems.
fn layout_worship_song(
    name: &'static str,
    tempo_bpm: f64,
    time_sig_num: u32,
    time_sig_den: u32,
    duration_s: f64,
    sections: &[(&str, u32)],
) -> DemoSong {
    // A measure is `num` beats, each beat = the `den`-th note. In seconds:
    // num * (60/tempo) * (4/den) — so 4/4 → 4*(60/tempo), 6/8 → 6*(60/tempo)*0.5.
    let measure = f64::from(time_sig_num) * (60.0 / tempo_bpm) * (4.0 / f64::from(time_sig_den));
    let count_in = 0.0;
    let song_start = 2.0 * measure;
    let song_end = duration_s.max(song_start + measure);
    let tail = 4.0;

    let total_w: f64 = sections
        .iter()
        .map(|(_, w)| f64::from((*w).max(1)))
        .sum::<f64>()
        .max(1.0);
    let span = song_end - song_start;
    let mut demo_sections = Vec::with_capacity(sections.len());
    let mut t = song_start;
    for (label, w) in sections {
        let dur = span * f64::from((*w).max(1)) / total_w;
        let end = (t + dur).min(song_end);
        demo_sections.push(DemoSection {
            kind: label_to_kind(label),
            start: t,
            end,
        });
        t = end;
    }
    // Snap the last section to song_end (absorb rounding).
    if let Some(last) = demo_sections.last_mut() {
        last.end = song_end;
    }

    // Coalesce consecutive sections that map to the same kind (e.g. two
    // adjacent choruses → one CH region). Adjacent same-type regions confuse
    // the per-song section numbering, and a repeated part reads as one section.
    let mut coalesced: Vec<DemoSection> = Vec::with_capacity(demo_sections.len());
    for s in demo_sections {
        match coalesced.last_mut() {
            Some(prev) if prev.kind == s.kind => prev.end = s.end,
            _ => coalesced.push(s),
        }
    }
    let demo_sections = coalesced;

    DemoSong {
        name,
        region_start: 0.0,
        region_end: song_end + tail,
        count_in,
        song_start,
        song_end,
        abs_end: song_end + tail,
        tempo_bpm,
        time_sig_num,
        time_sig_den,
        sections: demo_sections,
    }
}

/// Shift every absolute position in a song by `delta` seconds (used to
/// re-space the placeholder songs after the much-longer Praise).
fn shift_song(mut song: DemoSong, delta: f64) -> DemoSong {
    song.region_start += delta;
    song.region_end += delta;
    song.count_in += delta;
    song.song_start += delta;
    song.song_end += delta;
    song.abs_end += delta;
    for s in &mut song.sections {
        s.start += delta;
        s.end += delta;
    }
    song
}

/// Lyric-derived FALLBACK layout for the songs we don't yet have a keyflow chart
/// for (currently Holy Forever 72, Thank God I'm Free 128 — both 4/4). Charted
/// songs live in `SONG_CHARTS` and always win. `(name, tempo_bpm, time_sig_num,
/// time_sig_den, duration_s, [(lyric-section-label, line-weight)])`. Tempos are
/// the real published BPMs; section timings are lyric-proportional (approximate,
/// not a real chart) — replace a song here with a chart to make it accurate.
type WorshipSongLayout = (
    &'static str,
    f64,
    u32,
    u32,
    f64,
    &'static [(&'static str, u32)],
);
const WORSHIP_SONGS: &[WorshipSongLayout] = &[
    // All six songs now have real keyflow charts (see SONG_CHARTS); this
    // lyric-derived fallback is currently empty but kept for future songs.
];

/// Songs we have a real keyflow chart for — accurate tempo / time-signature /
/// section lengths, built via [`chart_song`]. Add a chart here (and drop the
/// song from `WORSHIP_SONGS`) as we transcribe each one; a chart always wins
/// over the lyric-derived fallback.
const SONG_CHARTS: &[(&str, &str)] = &[
    ("Praise", PRAISE_CHART),
    ("God, I'm Just Grateful", GOD_IM_JUST_GRATEFUL_CHART),
    ("Washed", WASHED_CHART),
    ("Who Else", WHO_ELSE_CHART),
    ("Thank God I'm Free", THANK_GOD_IM_FREE_CHART),
    ("Holy Forever", HOLY_FOREVER_CHART),
];

/// The bundled keyflow chart text for a demo song, if we have one.
///
/// Hosts stamping the demo setlist use this to attach the chart to the
/// song's project (ext-state `FTS/chart_text`) so setlist hydration can
/// serve it to remotes even without a MIDI chart-analysis service — the
/// browser chart pane renders from exactly this text.
#[must_use]
pub fn demo_chart_for(name: &str) -> Option<&'static str> {
    SONG_CHARTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, c)| c)
}

/// The setlist order. Each song uses its chart if we have one, else the
/// lyric-derived proportional fallback.
const ORDER: &[&str] = &[
    "Praise",
    "God, I'm Just Grateful",
    "Holy Forever",
    "Thank God I'm Free",
    "Washed",
    "Who Else",
];

/// The demo songs, each **project-relative / 0-based** (count-in near t=0,
/// first downbeat at the song's own measure 1), in setlist [`ORDER`].
///
/// This is the per-song-project source of truth: each song is stamped into its
/// OWN standalone project with its own default tempo/time-signature at t=0, so
/// there are no shared-timeline GAP offsets and no tempo points. The
/// (legacy) single-project [`demo_songs`] shifts these sequentially instead.
#[must_use]
pub fn demo_songs_base() -> Vec<DemoSong> {
    let mut base_songs: Vec<DemoSong> = Vec::with_capacity(ORDER.len());
    for &name in ORDER {
        if let Some(&(_, chart)) = SONG_CHARTS.iter().find(|(n, _)| *n == name) {
            if let Some(song) = chart_song(name, chart) {
                base_songs.push(song);
            }
        } else if let Some(&(n, tempo, num, den, dur, sections)) =
            WORSHIP_SONGS.iter().find(|e| e.0 == name)
        {
            base_songs.push(layout_worship_song(n, tempo, num, den, dur, sections));
        }
    }
    base_songs
}

/// Legacy single-project layout: the [`demo_songs_base`] songs shifted onto one
/// shared timeline with a gap between them (used by the one-project stamp path
/// and the older harness tests). The app now stamps one project per song via
/// [`demo_songs_base`] + [`stamp_song_with_default_tempo_native`].
fn demo_songs() -> Vec<DemoSong> {
    const GAP: f64 = 10.0;
    let mut songs = Vec::new();
    let mut cursor = 0.0;
    for base in demo_songs_base() {
        let delta = cursor - base.region_start;
        let shifted = shift_song(base, delta);
        cursor = shifted.abs_end + GAP;
        songs.push(shifted);
    }
    songs
}

/// The setlist layout for media seeding: `(song name, region_start, length)` in
/// timeline order. The app seeds each song's stems at its `region_start` so the
/// audio lines up with the stamped song region.
#[must_use]
pub fn demo_song_layout() -> Vec<(&'static str, f64, f64)> {
    demo_songs()
        .iter()
        .map(|s| (s.name, s.region_start, s.region_end - s.region_start))
        .collect()
}
