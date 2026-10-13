//! A track's own markers and regions.
//!
//! What is known about a stretch of one track — or of every track under a
//! folder — kept with the track rather than with a take, so it holds for
//! every take, every comp and every edit lane of it.
//!
//! A region says how its stretch is edited: on a grid of its own (`Grid`:
//! 78.3 to 79 is half-note triplets, those are the hits), left as played
//! (`NoQuantize`), a fill (`Fill`), or just a note. A marker is a named
//! moment. They are set ahead of the edit — by hand, or proposed from the
//! drum MIDI ([`GRID_FROM_MIDI`]) — and every quantize after reads them.
//! The kit's live on its folder (`Drums`) and count for every track under
//! it.
//!
//! Kept as the track's own data (`fts.marks` in its extension state):
//! saved with the song, in REAPER's project too, and in the project
//! document — an undo takes them back with everything else.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, TrackRef, Tracks};
use facet::Facet;

/// Where on a track they are kept.
pub const KEY: &str = "fts.marks";

/// What a region says about its stretch.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RegionKind {
    /// Its hits on a grid of its own (`division`).
    Grid,
    /// Left as played: no quantize moves a hit in it.
    NoQuantize,
    /// A fill: left as played unless a quantize is asked to edit fills.
    Fill,
    /// Only a note.
    Note,
}

/// Who put it there.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Origin {
    /// By hand.
    Manual,
    /// Proposed (from the MIDI, from the hits), not yet decided.
    Auto,
    /// Proposed and approved.
    Approved,
    /// Proposed and turned down: kept, so it is not proposed again.
    Rejected,
}

/// A stretch of a track.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct TrackRegion {
    pub id: String,
    /// Project seconds.
    pub start: f64,
    pub end: f64,
    pub kind: RegionKind,
    /// A grid region's grid: a whole note's division (16 sixteenths, 3
    /// half-note triplets, 12 eighth triplets, 24 sixteenth triplets).
    pub division: Option<u32>,
    pub origin: Origin,
    pub note: String,
}

impl TrackRegion {
    /// What it is called where it is drawn: `Grid 1/2T`, `No quantize`.
    #[must_use]
    pub fn label(&self) -> String {
        let what = match self.kind {
            RegionKind::Grid => {
                format!("Grid {}", self.division.map_or_else(String::new, grid_name))
            }
            RegionKind::NoQuantize => "No quantize".to_owned(),
            RegionKind::Fill => self
                .division
                .map_or_else(|| "Fill".to_owned(), |d| format!("Fill {}", grid_name(d))),
            RegionKind::Note => String::new(),
        };
        let who = match self.origin {
            Origin::Manual => "",
            Origin::Auto => " (auto)",
            Origin::Approved => " (approved)",
            Origin::Rejected => " (rejected)",
        };
        let label = format!("{what}{who}");
        match (label.is_empty(), self.note.is_empty()) {
            (_, true) => label,
            (true, false) => self.note.clone(),
            (false, false) => format!("{label} — {}", self.note),
        }
    }

    /// Whether it holds hits — anything but one turned down.
    #[must_use]
    pub fn counts(&self) -> bool {
        self.origin != Origin::Rejected
    }
}

/// A named moment on a track.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct TrackMarker {
    pub id: String,
    /// Project seconds.
    pub at: f64,
    pub name: String,
}

/// Everything a track keeps: its regions and markers, in time order.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub regions: Vec<TrackRegion>,
    pub markers: Vec<TrackMarker>,
}

/// A grid written as a note value: 16 → `1/16`, 3 → `1/2T`, 12 → `1/8T`.
#[must_use]
pub fn grid_name(division: u32) -> String {
    if division.is_multiple_of(3) {
        let straight = division.checked_div(3).unwrap_or(0).saturating_mul(2);
        format!("1/{straight}T")
    } else {
        format!("1/{division}")
    }
}

/// A grid as a whole note's division, from `1/2T`, `1/16`, `half triplet`
/// or a number (`12`).
///
/// # Errors
/// Nothing a grid is called.
pub fn parse_grid(text: &str) -> Result<u32, String> {
    let t = text.trim().to_ascii_lowercase();
    if let Ok(n) = t.parse::<u32>() {
        return (n > 0).then_some(n).ok_or_else(|| "a grid of 0".to_owned());
    }
    let words = [
        ("whole", 1_u32),
        ("half", 2),
        ("quarter", 4),
        ("eighth", 8),
        ("sixteenth", 16),
        ("thirty-second", 32),
    ];
    let (value, triplet) = if let Some(rest) = t.strip_prefix("1/") {
        let triplet = rest.ends_with('t');
        let n = rest
            .trim_end_matches('t')
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("no grid {text}"))?;
        (n, triplet)
    } else if let Some((_, n)) = words.iter().find(|(w, _)| t.starts_with(w)) {
        (*n, t.contains("triplet"))
    } else {
        return Err(format!(
            "no grid {text}: say `1/16`, `1/8T`, `1/2T`, `half triplet` or a whole note's division"
        ));
    };
    if value == 0 {
        return Err("a grid of 0".to_owned());
    }
    Ok(if triplet {
        value.saturating_mul(3) / 2
    } else {
        value
    })
}

/// Track `track`'s markers and regions.
#[must_use]
pub fn read(daw: &daw::standalone::Standalone, project: &ProjectContext, track: &str) -> Marks {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .into_iter()
        .find(|e| e.key == KEY)
        .and_then(|e| facet_json::from_str::<Marks>(&e.value).ok())
        .unwrap_or_default()
}

/// Keep `marks` as track `track`'s, in time order.
///
/// # Errors
/// The track is not there.
pub fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    marks: &Marks,
) -> Result<(), String> {
    let mut marks = marks.clone();
    marks.regions.sort_by(|a, b| a.start.total_cmp(&b.start));
    marks.markers.sort_by(|a, b| a.at.total_cmp(&b.at));
    let text = if marks == Marks::default() {
        String::new()
    } else {
        facet_json::to_string(&marks).map_err(|e| e.to_string())?
    };
    Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        KEY,
        &text,
    )
    .map_err(|e| e.to_string())?;
    changed();
    crate::marks_strip::refresh();
    crate::actions::list().changed("tracks.marks", track);
    Ok(())
}

/// Every track's markers and regions that has any, by track.
#[must_use]
pub fn all(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<(String, Marks)> {
    Tracks::all(daw, project.clone())
        .into_iter()
        .map(|t| (t.guid.clone(), read(daw, project, &t.guid)))
        .filter(|(_, m)| *m != Marks::default())
        .collect()
}

/// The tracks a region on `track` counts for: it and every track under it.
#[must_use]
pub fn under(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> Vec<String> {
    let tracks = Tracks::all(daw, project.clone());
    let mut out = vec![track.to_owned()];
    let mut i = 0;
    while let Some(parent) = out.get(i).cloned() {
        for t in tracks
            .iter()
            .filter(|t| t.parent_guid.as_deref() == Some(parent.as_str()))
        {
            if !out.contains(&t.guid) {
                out.push(t.guid.clone());
            }
        }
        i = i.saturating_add(1);
    }
    out
}

/// The regions that count for track `track`: its own and every folder's
/// above it.
#[must_use]
pub fn over(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> Vec<TrackRegion> {
    let tracks = Tracks::all(daw, project.clone());
    let mut out = Vec::new();
    let mut at = Some(track.to_owned());
    while let Some(guid) = at {
        out.extend(read(daw, project, &guid).regions);
        at = tracks
            .iter()
            .find(|t| t.guid == guid)
            .and_then(|t| t.parent_guid.clone());
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// The kit's folder: the top-level folder over its pieces' triggers —
/// where the kit's regions live.
#[must_use]
pub fn kit_folder(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Option<String> {
    let tracks = Tracks::all(daw, project.clone());
    let (_, trigger) = crate::drum_reference::pieces(daw, project)
        .into_iter()
        .next()?;
    let mut at = trigger;
    while let Some(parent) = tracks
        .iter()
        .find(|t| t.guid == at)
        .and_then(|t| t.parent_guid.clone())
    {
        at = parent;
    }
    Some(at)
}

/// The kit's regions with the track each is on: its folder's and every
/// track's under it.
#[must_use]
pub fn kit_marks(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<(String, TrackRegion)> {
    let Some(folder) = kit_folder(daw, project) else {
        return Vec::new();
    };
    let mut out: Vec<(String, TrackRegion)> = under(daw, project, &folder)
        .into_iter()
        .flat_map(|t| {
            read(daw, project, &t)
                .regions
                .into_iter()
                .map(move |r| (t.clone(), r))
        })
        .collect();
    out.sort_by(|a, b| a.1.start.total_cmp(&b.1.start));
    out
}

/// The kit's regions: on its folder and on every track under it.
#[must_use]
pub fn kit_regions(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<TrackRegion> {
    kit_marks(daw, project)
        .into_iter()
        .map(|(_, r)| r)
        .collect()
}

/// Put `region` on track `track`. Its id.
///
/// # Errors
/// The track is not there.
pub fn add(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    mut region: TrackRegion,
) -> Result<String, String> {
    if region.id.is_empty() {
        region.id = new_id();
    }
    let id = region.id.clone();
    let mut marks = read(daw, project, track);
    marks.regions.push(region);
    write(daw, project, track, &marks)?;
    Ok(id)
}

/// Change (or, `None` back, remove) the region `id` on `track`.
///
/// # Errors
/// No such region, or the track is not there.
pub fn change(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (track, id): (&str, &str),
    f: impl FnOnce(TrackRegion) -> Option<TrackRegion>,
) -> Result<(), String> {
    let mut marks = read(daw, project, track);
    let at = marks
        .regions
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| format!("no region {id} on that track"))?;
    let region = marks.regions.remove(at);
    if let Some(changed) = f(region) {
        marks.regions.insert(at, changed);
    }
    write(daw, project, track, &marks)
}

/// A new region's id.
#[must_use]
pub fn fresh_id() -> String {
    new_id()
}

/// Whether a Grid region is a figure — a bar or less (a sixteenth being
/// `sixteenth` seconds): its stretch's hits are on its grid, whatever
/// else says. Longer, it is a part's grid ([`part_grid_at`]).
#[must_use]
pub fn is_figure(region: &TrackRegion, sixteenth: f64) -> bool {
    region.end - region.start <= sixteenth.mul_add(16.0, 1e-6)
}

/// The grid a figure sets at `time`, if one does ([`figure_at`]).
#[must_use]
pub fn grid_at(regions: &[TrackRegion], time: f64, sixteenth: f64) -> Option<u32> {
    figure_at(regions, time, sixteenth).and_then(|r| r.division)
}

/// The figure (a Grid region of a bar or less) that puts the hit at
/// `time` on its grid: the innermost that holds it.
///
/// It holds the hits played a little either side of its edges too — up to
/// half its grid's step, a sixteenth (`sixteenth` seconds) at most: a
/// figure's first stroke pushed early, or its last landing on the bar line
/// it ends on, is the figure's.
#[must_use]
pub fn figure_at(regions: &[TrackRegion], time: f64, sixteenth: f64) -> Option<&TrackRegion> {
    let reach = |r: &TrackRegion| {
        let step = sixteenth * 16.0 / f64::from(r.division.unwrap_or(16).max(1));
        (step / 2.0).min(sixteenth)
    };
    regions
        .iter()
        .filter(|r| r.kind == RegionKind::Grid && r.counts() && r.division.is_some())
        .filter(|r| is_figure(r, sixteenth))
        .filter(|r| time >= r.start - reach(r) && time < r.end + reach(r))
        .min_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
}

/// The kit's parts' grids, by drum.
///
/// Each Grid region longer than a bar (a part's), with the drums it is
/// for — every one, on the kit's folder;
/// a drum's, on its own tracks (the snare on quarters through a double-
/// kick breakdown, the kick on sixteenths).
#[derive(Debug, Default)]
pub struct PartGrids {
    /// `(the drums it is for, the region)`.
    regions: Vec<(Vec<String>, TrackRegion)>,
}

impl PartGrids {
    /// Read the kit's.
    #[must_use]
    pub fn of(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Self {
        let (kit, _) = crate::hits::kit(daw, project);
        let regions = kit_marks(daw, project)
            .into_iter()
            .filter(|(_, r)| r.kind == RegionKind::Grid && r.counts() && r.division.is_some())
            .map(|(track, r)| {
                let covers = under(daw, project, &track);
                let drums = kit
                    .iter()
                    .filter(|p| p.tracks().any(|t| covers.contains(t)))
                    .map(|p| p.name.clone())
                    .collect();
                (drums, r)
            })
            .collect();
        Self { regions }
    }

    /// The grid strokes of `drums` at `time` go to, by their parts: each
    /// drum's own region where it has one, else the kit's — the most
    /// particular (fewest drums), then the shortest — and of the drums
    /// struck together, the coarsest (a snare on its quarter takes the
    /// kick under it there). `None`: no part says.
    #[must_use]
    pub fn grid(&self, drums: &[String], time: f64, sixteenth: f64) -> Option<u32> {
        drums
            .iter()
            .filter_map(|drum| {
                self.regions
                    .iter()
                    .filter(|(for_drums, r)| {
                        for_drums.contains(drum)
                            && !is_figure(r, sixteenth)
                            && time >= r.start
                            && time < r.end
                    })
                    .min_by(|(a, ra), (b, rb)| {
                        a.len()
                            .cmp(&b.len())
                            .then((ra.end - ra.start).total_cmp(&(rb.end - rb.start)))
                    })
                    .and_then(|(_, r)| r.division)
            })
            .min()
    }
}

/// A part's own grid at `time`.
///
/// A Grid region longer than a bar (a verse of sixteenths) — the grid a
/// stroke nothing else places goes to there,
/// in place of the quantize's (eighths); a written note still says its own
/// line. The innermost that holds it.
#[must_use]
pub fn part_grid_at(regions: &[TrackRegion], time: f64, sixteenth: f64) -> Option<u32> {
    regions
        .iter()
        .filter(|r| r.kind == RegionKind::Grid && r.counts() && !is_figure(r, sixteenth))
        .filter(|r| time >= r.start && time < r.end)
        .min_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
        .and_then(|r| r.division)
}

/// How many times the marks have changed: a window holding a picture of
/// them draws again when this moves.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The marks changed (here or by an undo): drawn again.
pub fn changed() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// See [`GENERATION`].
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// A new region's or marker's id: unique enough within one track.
fn new_id() -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros());
    format!("r{now:x}{n:x}")
}

/// Where `text` is, seconds: a bar as the ruler writes it (`78.3`, `78:3`,
/// `79`), a time (`12.5s`) or a section.
pub(crate) fn moment(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<f64, String> {
    let t = text.trim();
    // `78.3` is bar 78 beat 3 (`78.3.250` a quarter into it), as the ruler
    // and the transport write a position.
    let spelled = if !t.ends_with('s') && !t.contains(':') && t.contains('.') {
        let mut parts = t.splitn(3, '.');
        let bar = parts.next().unwrap_or_default();
        let beat = parts.next().unwrap_or("1");
        let sub = parts
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map_or(0.0, |s| s / 1000.0);
        let beat = beat
            .parse::<f64>()
            .map_err(|_| format!("no position {text}"))?;
        format!("{bar}:{}", beat + sub)
    } else {
        t.to_owned()
    };
    crate::fx_actions::moment(daw, project, &spelled)
}

fn kind_of(text: &str) -> Result<RegionKind, String> {
    match text
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_'], " ")
        .as_str()
    {
        "grid" => Ok(RegionKind::Grid),
        "no quantize" | "noquantize" | "exception" | "as played" => Ok(RegionKind::NoQuantize),
        "fill" => Ok(RegionKind::Fill),
        "note" => Ok(RegionKind::Note),
        other => Err(format!("no kind {other}: Grid, No quantize, Fill or Note")),
    }
}

fn origin_of(text: &str) -> Result<Origin, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "manual" => Ok(Origin::Manual),
        "auto" => Ok(Origin::Auto),
        "approved" | "approve" => Ok(Origin::Approved),
        "rejected" | "reject" => Ok(Origin::Rejected),
        other => Err(format!(
            "no origin {other}: manual, auto, approved, rejected"
        )),
    }
}

// ── the actions ───────────────────────────────────────────────────────

pub static LIST: ActionMeta = crate::actions::meta(
    "TRACK_MARKS",
    "track_marks",
    "A track's markers and regions",
    "A track's own markers and regions — kept with the track, not a take, so they hold for every take and edit lane — with each one's id, span (seconds and bars), kind (Grid with its grid, No quantize, Fill, Note), who placed it and its note — drawn in the ruler's MARKS lane while the track (or one under it) is selected. `track` (any track when left out: every track that has some). The kit's live on its folder (`Drums`) and count for every track under it; QUANTIZE puts the hits in a Grid region on its grid, and leaves a No quantize one as played.",
    "Tracks",
    "",
);

pub static ADD: ActionMeta = crate::actions::meta(
    "TRACK_REGION_ADD",
    "track_region_add",
    "Add a track region",
    "Put a region on a track (`track`; the kit's folder when left out) from `from` to `to` — bars as the ruler writes them (`78.3`, `79`), `12.5s`, or a section — saying how that stretch is edited: `kind` Grid (its hits on `grid`: `1/2T` half-note triplets, `1/8T`, `1/16`, or a whole note's division), No quantize (left as played), Fill, or Note; `note` anything to say. On a folder it counts for every track under it. One undo step. Its id.",
    "Tracks",
    "",
);

pub static EDIT: ActionMeta = crate::actions::meta(
    "TRACK_REGION_EDIT",
    "track_region_edit",
    "Change a track region",
    "Change the region `id` on `track` (the kit's folder when left out): `from`, `to`, `kind`, `grid`, `note`, or `origin` (approve or reject a proposed one: approved, rejected). One undo step.",
    "Tracks",
    "",
);

pub static REMOVE: ActionMeta = crate::actions::meta(
    "TRACK_MARKS_REMOVE",
    "track_marks_remove",
    "Remove track markers and regions",
    "Remove the markers and regions `ids` from `track` (the kit's folder when left out). One undo step.",
    "Tracks",
    "",
);

pub static MARKER_ADD: ActionMeta = crate::actions::meta(
    "TRACK_MARKER_ADD",
    "track_marker_add",
    "Add a track marker",
    "Put a named marker on a track (`track`; the kit's folder when left out) at `at` (a bar `78.3`, `12.5s`, or a section). One undo step. Its id.",
    "Tracks",
    "",
);

pub static GRID_FROM_MIDI: ActionMeta = crate::actions::meta(
    "TRACK_GRID_FROM_MIDI",
    "track_grid_from_midi",
    "Grid regions from the drum MIDI",
    "Read the drum MIDI ahead of the edit and propose a Grid region (`auto`) on the kit's folder over every stretch it writes on a triplet grid — the notes on a half-, quarter-, eighth- or sixteenth-note triplet line and not on a straight one — so a quantize puts those hits on that grid. `from`/`to` (the whole song when left out); the proposals there replaced, none laid over a region already decided. Says each one. Approve or reject with TRACK_REGION_EDIT.",
    "Drums",
    "",
);

/// Which track.
#[derive(Facet)]
struct TrackArg {
    track: Option<String>,
}

/// A region to add.
#[derive(Facet)]
struct AddArg {
    track: Option<String>,
    from: String,
    to: String,
    kind: String,
    grid: Option<String>,
    note: Option<String>,
}

/// A change to a region.
#[derive(Facet)]
struct EditArg {
    track: Option<String>,
    id: String,
    from: Option<String>,
    to: Option<String>,
    kind: Option<String>,
    grid: Option<String>,
    note: Option<String>,
    origin: Option<String>,
}

/// What to remove.
#[derive(Facet)]
struct RemoveArg {
    track: Option<String>,
    ids: Vec<String>,
}

/// A marker to add.
#[derive(Facet)]
struct MarkerArg {
    track: Option<String>,
    at: String,
    name: String,
}

/// Where to read the MIDI.
#[derive(Facet)]
struct SpanArg {
    from: Option<String>,
    to: Option<String>,
}

/// A region, as said.
#[derive(Facet)]
struct RegionOut {
    id: String,
    kind: String,
    grid: Option<String>,
    origin: String,
    from: f64,
    to: f64,
    /// Where it starts and ends, as the ruler writes them.
    bars: String,
    note: String,
}

/// A marker, as said.
#[derive(Facet)]
struct MarkerOut {
    id: String,
    at: f64,
    bar: String,
    name: String,
}

/// One track's, as said.
#[derive(Facet)]
struct TrackOut {
    track: String,
    name: String,
    regions: Vec<RegionOut>,
    markers: Vec<MarkerOut>,
}

/// `at` as the ruler writes it: `78.3`, `78.3.500`.
pub(crate) fn bar_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: f64,
) -> String {
    let (bar, beat, fraction) = crate::song_bars::musical(daw, project, at);
    let sub = (fraction * 1000.0).round();
    if (0.5..=999.5).contains(&sub) {
        format!("{bar}.{beat}.{sub:03.0}")
    } else {
        format!("{bar}.{beat}")
    }
}

/// The track an action names, or the kit's folder.
fn track_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: Option<&str>,
) -> Result<String, String> {
    track.map_or_else(
        || {
            kit_folder(daw, project)
                .ok_or_else(|| "no kit to put it on: name the `track`".to_owned())
        },
        |t| crate::actions::resolve(daw, project, t),
    )
}

/// One undo step around `f`.
fn step<T>(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    label: &str,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let out = f();
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    out
}

fn said(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (track, name): (&str, &str),
) -> TrackOut {
    let marks = read(daw, project, track);
    TrackOut {
        track: track.to_owned(),
        name: name.to_owned(),
        regions: marks
            .regions
            .iter()
            .map(|r| RegionOut {
                id: r.id.clone(),
                kind: format!("{:?}", r.kind),
                grid: r.division.map(grid_name),
                origin: format!("{:?}", r.origin),
                from: r.start,
                to: r.end,
                bars: format!(
                    "{} – {}",
                    bar_of(daw, project, r.start),
                    bar_of(daw, project, r.end)
                ),
                note: r.note.clone(),
            })
            .collect(),
        markers: marks
            .markers
            .iter()
            .map(|m| MarkerOut {
                id: m.id.clone(),
                at: m.at,
                bar: bar_of(daw, project, m.at),
                name: m.name.clone(),
            })
            .collect(),
    }
}

pub fn register(list: &ActionList) {
    register_list(list);
    register_add(list);
    register_edit(list);
    register_remove(list);
    register_marker_add(list);
    register_grid_from_midi(list);
}

fn register_list(list: &ActionList) {
    list.register(&LIST, Effect::ReadOnly, |a: TrackArg| {
        let (daw, project) = crate::actions::song()?;
        let tracks = Tracks::all(daw, project.clone());
        let name = |guid: &str| {
            tracks
                .iter()
                .find(|t| t.guid == guid)
                .map(|t| t.name.clone())
                .unwrap_or_default()
        };
        if let Some(track) = a.track {
            let guid = crate::actions::resolve(daw, &project, &track)?;
            return Ok(vec![said(daw, &project, (&guid, &name(&guid)))]);
        }
        Ok(all(daw, &project)
            .iter()
            .map(|(guid, _)| said(daw, &project, (guid, &name(guid))))
            .collect::<Vec<_>>())
    });
}

fn register_add(list: &ActionList) {
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, a.track.as_deref())?;
        let (start, end) = (
            moment(daw, &project, &a.from)?,
            moment(daw, &project, &a.to)?,
        );
        if end <= start {
            return Err("`to` must be after `from`".to_owned());
        }
        let kind = kind_of(&a.kind)?;
        let division = a.grid.as_deref().map(parse_grid).transpose()?;
        if kind == RegionKind::Grid && division.is_none() {
            return Err("a Grid region says its `grid` (`1/2T`, `1/16` …)".to_owned());
        }
        let region = TrackRegion {
            id: new_id(),
            start,
            end,
            kind,
            division,
            origin: Origin::Manual,
            note: a.note.unwrap_or_default(),
        };
        let id = region.id.clone();
        step(daw, &project, "Add a track region", || {
            let mut marks = read(daw, &project, &track);
            marks.regions.push(region);
            write(daw, &project, &track, &marks)
        })?;
        Ok(id)
    });
}

fn register_edit(list: &ActionList) {
    list.register(&EDIT, Effect::Mutating, |a: EditArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, a.track.as_deref())?;
        let mut marks = read(daw, &project, &track);
        let region = marks
            .regions
            .iter_mut()
            .find(|r| r.id == a.id)
            .ok_or_else(|| format!("no region {} on that track", a.id))?;
        if let Some(from) = &a.from {
            region.start = moment(daw, &project, from)?;
        }
        if let Some(to) = &a.to {
            region.end = moment(daw, &project, to)?;
        }
        if region.end <= region.start {
            return Err("`to` must be after `from`".to_owned());
        }
        if let Some(kind) = &a.kind {
            region.kind = kind_of(kind)?;
        }
        if let Some(grid) = &a.grid {
            region.division = Some(parse_grid(grid)?);
        }
        if let Some(note) = a.note {
            region.note = note;
        }
        if let Some(origin) = &a.origin {
            region.origin = origin_of(origin)?;
        }
        if region.kind == RegionKind::Grid && region.division.is_none() {
            return Err("a Grid region says its `grid` (`1/2T`, `1/16` …)".to_owned());
        }
        let label = region.label();
        step(daw, &project, "Change a track region", || {
            write(daw, &project, &track, &marks)
        })?;
        Ok(label)
    });
}

fn register_remove(list: &ActionList) {
    list.register(&REMOVE, Effect::Mutating, |a: RemoveArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, a.track.as_deref())?;
        let mut marks = read(daw, &project, &track);
        let before = marks.regions.len().saturating_add(marks.markers.len());
        marks.regions.retain(|r| !a.ids.contains(&r.id));
        marks.markers.retain(|m| !a.ids.contains(&m.id));
        let removed =
            before.saturating_sub(marks.regions.len().saturating_add(marks.markers.len()));
        step(daw, &project, "Remove track markers and regions", || {
            write(daw, &project, &track, &marks)
        })?;
        Ok(format!("{removed} removed"))
    });
}

fn register_marker_add(list: &ActionList) {
    list.register(&MARKER_ADD, Effect::Mutating, |a: MarkerArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, a.track.as_deref())?;
        let at = moment(daw, &project, &a.at)?;
        let id = new_id();
        let marker = TrackMarker {
            id: id.clone(),
            at,
            name: a.name,
        };
        step(daw, &project, "Add a track marker", || {
            let mut marks = read(daw, &project, &track);
            marks.markers.push(marker);
            write(daw, &project, &track, &marks)
        })?;
        Ok(id)
    });
}

fn register_grid_from_midi(list: &ActionList) {
    list.register(&GRID_FROM_MIDI, Effect::Mutating, |a: SpanArg| {
        let (daw, project) = crate::actions::song()?;
        let folder = kit_folder(daw, &project).ok_or("no kit to put them on")?;
        let span = match (a.from.as_deref(), a.to.as_deref()) {
            (Some(f), Some(t)) => (moment(daw, &project, f)?, moment(daw, &project, t)?),
            (None, None) => (0.0, f64::INFINITY),
            _ => return Err("a span is `from` and `to` together".to_owned()),
        };
        let midi = crate::drum_reference::find_track(daw, &project).ok_or("no drum MIDI track")?;
        let notes: Vec<f64> = crate::drum_reference::notes(daw, &project, &midi)
            .into_iter()
            .map(|n| n.time)
            .filter(|t| *t >= span.0 && *t < span.1)
            .collect();
        let spacing = |t: f64, division: u32| {
            use daw::service::TempoMap as _;
            240.0 / daw.get_tempo_at(project.clone(), t).max(1.0) / f64::from(division.max(1))
        };
        let found = triplet_stretches(&notes, spacing, |t| beat_at(daw, &project, t));
        let mut marks = read(daw, &project, &folder);
        marks
            .regions
            .retain(|r| !(r.origin == Origin::Auto && r.start < span.1 && r.end > span.0));
        let decided: Vec<(f64, f64)> = marks
            .regions
            .iter()
            .filter(|r| r.origin != Origin::Auto)
            .map(|r| (r.start, r.end))
            .collect();
        let mut proposed = Vec::new();
        for (start, end, division) in found {
            if decided.iter().any(|(s, e)| *s < end && *e > start) {
                continue;
            }
            let region = TrackRegion {
                id: new_id(),
                start,
                end,
                kind: RegionKind::Grid,
                division: Some(division),
                origin: Origin::Auto,
                note: "from the drum MIDI".to_owned(),
            };
            proposed.push(format!(
                "{} – {}: {}",
                bar_of(daw, &project, start),
                bar_of(daw, &project, end),
                grid_name(division)
            ));
            marks.regions.push(region);
        }
        step(daw, &project, "Grid regions from the drum MIDI", || {
            write(daw, &project, &folder, &marks)
        })?;
        Ok(proposed)
    });
}

/// The beat at `t` (seconds a beat, and the beat's start).
fn beat_at(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> (f64, f64) {
    use daw::service::TempoMap as _;
    let (bar, beat, _) = daw.time_to_musical(project.clone(), t);
    let start = daw.musical_to_time(project.clone(), bar, beat, 0.0);
    let next = daw.musical_to_time(project.clone(), bar, beat.saturating_add(1), 0.0);
    ((next - start).max(1e-6), start)
}

/// How near a line a note must be to be on it, of the line's spacing…
const ON_A_LINE: f64 = 0.12;

/// …and at most, seconds.
const ON_A_LINE_MOST: f64 = 0.012;

/// The triplet grids, coarse to fine, as a whole note's division.
const TRIPLETS: [u32; 4] = [3, 6, 12, 24];

/// The stretches of `notes` (seconds, in order) written on a triplet grid.
///
/// Runs of at least three notes each on a line of a triplet grid and not
/// on a straight sixteenth (a note on a beat, which both share, goes on
/// with the run it is in), from the beat the first is in to the end of
/// the beat the last is in, on the finest grid any of them needs.
/// `spacing(t, division)` is a grid's line spacing at `t`, seconds, its
/// lines counted from the song's start as a quantize counts them;
/// `beat_at(t)` the beat holding `t`: `(length, start)`. `(start, end,
/// division)`.
#[must_use]
pub fn triplet_stretches(
    notes: &[f64],
    spacing: impl Fn(f64, u32) -> f64,
    beat_at: impl Fn(f64) -> (f64, f64),
) -> Vec<(f64, f64, u32)> {
    let on = |t: f64, division: u32| {
        let step = spacing(t, division).max(1e-9);
        (t / step).round().mul_add(-step, t).abs() <= (step * ON_A_LINE).min(ON_A_LINE_MOST)
    };
    // Each note's coarsest triplet grid, when it is on one and not on a
    // straight sixteenth.
    let grid_of = |t: f64| {
        if on(t, 16) {
            return None;
        }
        TRIPLETS.iter().copied().find(|d| on(t, *d))
    };
    let mut out: Vec<(f64, f64, u32)> = Vec::new();
    let mut run: Vec<(f64, u32)> = Vec::new();
    let close = |run: &mut Vec<(f64, u32)>, out: &mut Vec<(f64, f64, u32)>| {
        if run.len() >= 3
            && let (Some(&(first, _)), Some(&(last, _))) = (run.first(), run.last())
        {
            let division = run.iter().map(|(_, d)| *d).max().unwrap_or(12);
            let (_, from) = beat_at(first);
            let (len, start) = beat_at(last);
            out.push((from, start + len, division));
        }
        run.clear();
    };
    for &t in notes {
        match grid_of(t) {
            Some(d) => run.push((t, d)),
            None if on(t, 4) => {}
            None => close(&mut run, &mut out),
        }
    }
    close(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drum's own part grid beats the kit's for it, and struck together
    /// the coarser wins.
    #[test]
    fn a_drums_own_part_grid_beats_the_kits() {
        let region = |start: f64, end: f64, division: u32| TrackRegion {
            id: String::new(),
            start,
            end,
            kind: RegionKind::Grid,
            division: Some(division),
            origin: Origin::Manual,
            note: String::new(),
        };
        let drums = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let parts = PartGrids {
            regions: vec![
                (drums(&["Kick", "Snare"]), region(0.0, 30.0, 16)),
                (drums(&["Snare"]), region(0.0, 30.0, 4)),
            ],
        };
        let sixteenth = 0.1;
        assert_eq!(parts.grid(&drums(&["Kick"]), 5.0, sixteenth), Some(16));
        assert_eq!(parts.grid(&drums(&["Snare"]), 5.0, sixteenth), Some(4));
        assert_eq!(
            parts.grid(&drums(&["Kick", "Snare"]), 5.0, sixteenth),
            Some(4),
            "struck together, the snare's quarter"
        );
        assert_eq!(parts.grid(&drums(&["Tom 1"]), 5.0, sixteenth), None);
        assert_eq!(parts.grid(&drums(&["Kick"]), 31.0, sixteenth), None);
    }

    #[test]
    fn grids_are_named_as_note_values() {
        assert_eq!(grid_name(16), "1/16");
        assert_eq!(grid_name(3), "1/2T");
        assert_eq!(grid_name(12), "1/8T");
        assert_eq!(grid_name(24), "1/16T");
        for g in ["1/2T", "1/4T", "1/8T", "1/16T", "1/16", "1/8", "1/32"] {
            assert_eq!(grid_name(parse_grid(g).expect("a grid")), g);
        }
        assert_eq!(parse_grid("half triplet"), Ok(3));
        assert_eq!(parse_grid("12"), Ok(12));
        assert!(parse_grid("sometimes").is_err());
    }

    /// The innermost grid region holding a moment sets its grid; one turned
    /// down sets none.
    #[test]
    fn the_innermost_grid_region_sets_the_grid() {
        let region = |start: f64, end: f64, division: u32, origin: Origin| TrackRegion {
            id: String::new(),
            start,
            end,
            kind: RegionKind::Grid,
            division: Some(division),
            origin,
            note: String::new(),
        };
        let regions = [
            region(10.0, 20.0, 12, Origin::Manual),
            region(14.0, 15.0, 3, Origin::Manual),
            region(30.0, 31.0, 24, Origin::Rejected),
        ];
        // A sixteenth of 1 s, a bar of 16 s: every region here a figure;
        // an eighth triplet's step is 1.33 s, the reach two thirds of it.
        let sixteenth = 1.0;
        assert_eq!(grid_at(&regions, 12.0, sixteenth), Some(12));
        assert_eq!(grid_at(&regions, 14.5, sixteenth), Some(3), "the inner one");
        assert_eq!(
            grid_at(&regions, 9.5, sixteenth),
            Some(12),
            "a stroke pushed early is the figure's"
        );
        assert_eq!(
            grid_at(&regions, 20.5, sixteenth),
            Some(12),
            "one landing on its end is too"
        );
        assert_eq!(grid_at(&regions, 21.0, sixteenth), None, "further is not");
        assert_eq!(grid_at(&regions, 30.5, sixteenth), None, "turned down");
        // A sixteenth of 0.1 s, a bar of 1.6 s: the 10 s region a part's
        // grid, not a figure.
        assert_eq!(grid_at(&regions, 12.0, 0.1), None, "a part is no figure");
        assert_eq!(
            part_grid_at(&regions, 12.0, 0.1),
            Some(12),
            "its grid is the part's"
        );
        assert_eq!(
            part_grid_at(&regions, 20.0, 0.1),
            None,
            "its end is not in it"
        );
    }

    /// Quarter-note triplets written in a bar of straight time are proposed
    /// as a stretch on that grid, a beat at a time; the straight notes
    /// either side are not.
    #[test]
    fn a_triplet_figure_in_the_midi_is_found() {
        // 120 bpm: a beat is half a second.
        let beat_at = |t: f64| (0.5, (t / 0.5).floor() * 0.5);
        let mut notes: Vec<f64> = (0..8).map(|i| f64::from(i) * 0.125).collect();
        // Bar 2 (2.0 s on): quarter-note triplets, three to two beats.
        notes.extend((0..6).map(|i| f64::from(i).mul_add(1.0 / 3.0, 2.0)));
        notes.extend((0..8).map(|i| f64::from(i).mul_add(0.125, 4.0)));
        let spacing = |_: f64, division: u32| 2.0 / f64::from(division);
        let found = triplet_stretches(&notes, spacing, beat_at);
        assert_eq!(found.len(), 1, "{found:?}");
        let (start, end, division) = found[0];
        assert!((start - 2.0).abs() < 1e-9, "{found:?}");
        assert!(end <= 4.0 + 1e-9, "{found:?}");
        assert_eq!(division, 6, "on quarter-note triplet lines: {found:?}");
    }
}
