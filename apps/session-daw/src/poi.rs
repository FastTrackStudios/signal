//! Points of interest: a playlist of places in the song to check.
//!
//! What has come up while editing — a kick a sixteenth off, a dropout in
//! the overheads, a fill played late — each marked where it is with a
//! note, and stepped through (`{` / `}`) to listen to one after another,
//! each ticked off (`|`) once it is right. Kept with the song (its
//! project ext state, in the project doc: saved, and undone with the rest)
//! and drawn in the ruler's MARKS lane — amber while open, green once
//! done, ringed while it is the one stepped to.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyrender::PaintScene;
use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::ProjectContext;
use facet::Facet;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

/// Where the list is kept: the song's ext state.
const SECTION: &str = "fts";
const KEY: &str = "poi";

/// One place to check.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Poi {
    pub id: String,
    /// Project seconds.
    pub at: f64,
    /// What is there to check.
    pub note: String,
    /// Checked and right.
    pub done: bool,
}

/// The list, in time order.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
struct List {
    points: Vec<Poi>,
}

/// The song's points of interest, in time order.
#[must_use]
pub fn read(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Poi> {
    daw_proto::ExtState::get_project(daw, project.clone(), SECTION, KEY)
        .and_then(|text| facet_json::from_str::<List>(&text).ok())
        .map(|l| l.points)
        .unwrap_or_default()
}

/// Keep `points` as the song's, in time order.
fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    mut points: Vec<Poi>,
) -> Result<Vec<Poi>, String> {
    points.sort_by(|a, b| a.at.total_cmp(&b.at));
    let text = facet_json::to_string(&List {
        points: points.clone(),
    })
    .map_err(|e| e.to_string())?;
    daw_proto::ExtState::set_project(daw, project.clone(), SECTION, KEY, &text)
        .map_err(|e| e.to_string())?;
    publish(points.clone());
    crate::actions::list().changed("song.poi", "");
    Ok(points)
}

/// The list as the window draws it, and which one was stepped to.
static SHOWN: Mutex<Option<Arc<Vec<Poi>>>> = Mutex::new(None);
static CURRENT: Mutex<Option<String>> = Mutex::new(None);
static VERSION: AtomicU64 = AtomicU64::new(0);
static ASKED: AtomicBool = AtomicBool::new(false);

fn publish(points: Vec<Poi>) {
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(Arc::new(points));
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// Goes up on every change: a window compares it with the one it drew.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// The list, once read; read off the window's thread the first time.
fn shown() -> Option<Arc<Vec<Poi>>> {
    let shown = SHOWN.lock().ok().and_then(|s| s.clone());
    if shown.is_none() && !ASKED.swap(true, Ordering::AcqRel) {
        refresh();
    }
    shown
}

/// Read the list again, off the window's thread (a song opened, an undo).
pub fn refresh() {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(|| {
            if let Ok((daw, project)) = crate::actions::song() {
                publish(read(daw, &project));
            }
        });
    }
}

fn current() -> Option<String> {
    CURRENT.lock().ok().and_then(|c| c.clone())
}

fn set_current(id: Option<String>) {
    if let Ok(mut c) = CURRENT.lock() {
        *c = id;
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// Where `at` is: a bar as the ruler writes it (`83.1`), `12.5s`, a
/// section; the edit cursor when left out.
fn at_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: Option<&str>,
) -> Result<f64, String> {
    at.map_or_else(
        || {
            Ok(crate::cursor::current()
                .unwrap_or_else(|| crate::organize::engine_edit(daw, project))
                .at)
        },
        |text| crate::track_regions::moment(daw, project, text),
    )
}

/// The one to step to from `here` by `by` (+1 the next after it, -1 the
/// one before), the open ones only when `open`.
fn step_from(points: &[Poi], here: f64, by: i32, open: bool) -> Option<&Poi> {
    /// Nearer than this to `here` is where it is, not after or before.
    const SAME: f64 = 0.005;
    let mut candidates = points.iter().filter(|p| !(open && p.done));
    if by >= 0 {
        candidates.find(|p| p.at > here + SAME)
    } else {
        candidates.rfind(|p| p.at < here - SAME)
    }
}

static LIST: ActionMeta = crate::actions::meta(
    "POI_LIST",
    "poi_list",
    "Points of interest",
    "The song's points of interest — places to check, each with its note — in time order: id, where (seconds, and the bar as the ruler writes it), the note, whether it is done, and which one was stepped to last.",
    "Session",
    "",
);
static ADD: ActionMeta = crate::actions::meta(
    "POI_ADD",
    "poi_add",
    "Add a point of interest",
    "Mark a place to check: `at` (a bar as the ruler writes it — `83.1`, `67.1.750` — `12.5s`, or a section; the edit cursor when left out) and `note` (what is there to check). One undo step. Its id.",
    "Session",
    "",
);
static EDIT: ActionMeta = crate::actions::meta(
    "POI_EDIT",
    "poi_edit",
    "Change a point of interest",
    "Change point `id` (the one stepped to last when left out): `note`, `done` (checked and right), or `at`. One undo step.",
    "Session",
    "",
);
static REMOVE: ActionMeta = crate::actions::meta(
    "POI_REMOVE",
    "poi_remove",
    "Remove points of interest",
    "Take points `ids` off the list; `done` true takes off every done one. One undo step. How many went.",
    "Session",
    "",
);
/// Step to the next or the one before (`}` / `{`).
pub static STEP: ActionMeta = crate::actions::meta(
    "POI_STEP",
    "poi_step",
    "Step to a point of interest",
    "Go to the next point of interest after the edit cursor (`by` 1, `}`) or the one before (`by` -1, `{`) — `open` true skips the done ones — its page of bars framed (SESSION_STEP) and the cursor on it. Says which.",
    "Session",
    "",
);
/// Tick the one stepped to off, or open it again (`|`).
pub static TOGGLE: ActionMeta = crate::actions::meta(
    "POI_TOGGLE",
    "poi_toggle",
    "Tick a point of interest off",
    "The point of interest stepped to last (or `id`) done, or open again if it was (`|`). One undo step. Says which, and what it is now.",
    "Session",
    "",
);

#[derive(Facet)]
struct ListArg {}

#[derive(Facet)]
struct PoiOut {
    id: String,
    at: f64,
    bar: String,
    note: String,
    done: bool,
    current: bool,
}

#[derive(Facet)]
struct AddArg {
    at: Option<String>,
    note: String,
}

#[derive(Facet)]
struct EditArg {
    id: Option<String>,
    note: Option<String>,
    done: Option<bool>,
    at: Option<String>,
}

#[derive(Facet)]
struct RemoveArg {
    #[facet(default)]
    ids: Vec<String>,
    done: Option<bool>,
}

#[derive(Facet)]
struct StepArg {
    by: Option<i32>,
    open: Option<bool>,
}

#[derive(Facet)]
struct ToggleArg {
    id: Option<String>,
}

fn out(daw: &daw::standalone::Standalone, project: &ProjectContext, p: &Poi) -> PoiOut {
    PoiOut {
        id: p.id.clone(),
        at: p.at,
        bar: crate::track_regions::bar_of(daw, project, p.at),
        note: p.note.clone(),
        done: p.done,
        current: current().as_deref() == Some(p.id.as_str()),
    }
}

/// The one an edit names: `id`, or the one stepped to last.
fn named(points: &[Poi], id: Option<&str>) -> Result<usize, String> {
    let id = id
        .map(str::to_owned)
        .or_else(current)
        .ok_or("no point named, and none stepped to")?;
    points
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("no point of interest {id}"))
}

pub fn register(list: &ActionList) {
    list.register(&LIST, Effect::ReadOnly, |_: ListArg| {
        let (daw, project) = crate::actions::song()?;
        Ok(read(daw, &project)
            .iter()
            .map(|p| out(daw, &project, p))
            .collect::<Vec<_>>())
    });
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = crate::actions::song()?;
        let at = at_of(daw, &project, a.at.as_deref())?;
        let id = crate::track_regions::fresh_id();
        let mut points = read(daw, &project);
        points.push(Poi {
            id: id.clone(),
            at,
            note: a.note,
            done: false,
        });
        write(daw, &project, points)?;
        Ok(id)
    });
    list.register(&EDIT, Effect::Mutating, |a: EditArg| {
        let (daw, project) = crate::actions::song()?;
        let mut points = read(daw, &project);
        let k = named(&points, a.id.as_deref())?;
        let at =
            a.at.as_deref()
                .map(|t| crate::track_regions::moment(daw, &project, t))
                .transpose()?;
        let point = points.get_mut(k).ok_or("gone")?;
        if let Some(note) = a.note {
            point.note = note;
        }
        if let Some(done) = a.done {
            point.done = done;
        }
        if let Some(at) = at {
            point.at = at;
        }
        let changed = point.clone();
        write(daw, &project, points)?;
        Ok(out(daw, &project, &changed))
    });
    list.register(&REMOVE, Effect::Mutating, |a: RemoveArg| {
        let (daw, project) = crate::actions::song()?;
        let points = read(daw, &project);
        let before = points.len();
        let done = a.done.unwrap_or(false);
        let kept: Vec<Poi> = points
            .into_iter()
            .filter(|p| !(a.ids.contains(&p.id) || done && p.done))
            .collect();
        let gone = before.saturating_sub(kept.len());
        write(daw, &project, kept)?;
        Ok(u32::try_from(gone).unwrap_or(u32::MAX))
    });
    list.register(&STEP, Effect::Mutating, |a: StepArg| {
        let (daw, project) = crate::actions::song()?;
        let points = read(daw, &project);
        let here = at_of(daw, &project, None)?;
        let point = step_from(&points, here, a.by.unwrap_or(1), a.open.unwrap_or(false))
            .ok_or("no point of interest that way")?
            .clone();
        crate::actions::list()
            .run_json(
                crate::bar_step::STEP_ACTION.id,
                format!(r#"{{"at":{}}}"#, point.at).as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        let playing = crate::engine::Transport::shared().is_some_and(|t| t.reading().playing);
        if !playing {
            daw::service::transport::service::Transport::set_position(
                daw,
                project.clone(),
                point.at,
            )
            .map_err(|e| e.to_string())?;
            crate::cursor::ask(crate::cursor::Edit {
                at: point.at,
                selection: None,
            });
        }
        set_current(Some(point.id.clone()));
        Ok(out(daw, &project, &point))
    });
    list.register(&TOGGLE, Effect::Mutating, |a: ToggleArg| {
        let (daw, project) = crate::actions::song()?;
        let mut points = read(daw, &project);
        let k = named(&points, a.id.as_deref())?;
        let point = points.get_mut(k).ok_or("gone")?;
        point.done = !point.done;
        let changed = point.clone();
        write(daw, &project, points)?;
        Ok(out(daw, &project, &changed))
    });
}

/// The points of interest in the ruler's MARKS lane: a flag at each, its
/// note beside it — amber open, green done, ringed when stepped to.
pub fn paint(
    painter: &mut impl PaintScene,
    (palette, font): (&crate::arrangement::Palette, &crate::text::Font),
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
) {
    /// The notes' size, as the ruler's lanes write theirs.
    const SIZE: f32 = 8.0;
    const OPEN: Color = Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff);
    const DONE: Color = Color::from_rgba8(0x34, 0xd3, 0x99, 0xff);
    let Some(points) = shown() else {
        return;
    };
    let row = crate::ruler::MARKS_ROW;
    if !crate::ruler::lane_shown(row) {
        return;
    }
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let y0 = oy + crate::ruler::row_top(row) + 1.0;
    let y1 = y0 + crate::ruler::LANE_H - 2.0;
    let current = current();
    for p in points.iter() {
        let x = p.at.mul_add(view.pps, left - view.scroll_x);
        if x < left - 2.0 || x > right {
            continue;
        }
        let color = if p.done { DONE } else { OPEN };
        if current.as_deref() == Some(p.id.as_str()) {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.text,
                None,
                &Rect::new(x - 3.5, y0 - 1.0, x + 3.5, y1 + 1.0),
            );
        }
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Rect::new(x - 2.0, y0, x + 2.0, y1),
        );
        crate::tcp::glyphs(painter, font, color, &p.note, x + 5.0, y1 - 2.0, SIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(at: f64, done: bool) -> Poi {
        Poi {
            id: format!("{at}"),
            at,
            note: String::new(),
            done,
        }
    }

    #[test]
    fn steps_go_to_the_next_and_the_one_before() {
        let points = [at(1.0, false), at(2.0, true), at(3.0, false)];
        assert_eq!(step_from(&points, 1.0, 1, false).map(|p| p.at), Some(2.0));
        assert_eq!(step_from(&points, 1.0, 1, true).map(|p| p.at), Some(3.0));
        assert_eq!(step_from(&points, 3.0, -1, false).map(|p| p.at), Some(2.0));
        assert_eq!(step_from(&points, 0.5, -1, false).map(|p| p.at), None);
        assert_eq!(step_from(&points, 3.0, 1, false).map(|p| p.at), None);
    }
}
