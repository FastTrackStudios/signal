//! Which folders are open: the window's own choice over the layout's.
//!
//! The scene decides a first layout (`plan::apply_scene`); a person then
//! opens and shuts folders — from a folder's button in the arrangement or
//! at the foot of its mixer strip — and can fold every top-level folder at
//! once ([`set_tops`]): the whole session as its families, the drums one
//! row and one strip, to see everything that is going on and open only
//! what is being worked on.
//!
//! One state for the window, so the arrangement and the mixer always show
//! the same folders; applied where rows are planned
//! (`studio::plan_rows_with`), so a folder shut here gets its children's
//! items folded onto its row, as a folder the scene shut does. Every
//! change bumps [`generation`], which the views compare each frame to plan
//! their rows again.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

struct State {
    /// A folder's own choice, by guid: open (`true`) or shut.
    chosen: HashMap<String, bool>,
    /// Every top-level folder shut unless chosen open.
    tops: bool,
}

impl Default for State {
    /// Every top-level folder shut: a session opens as its families —
    /// Guide, Drums, Bass, Guitars, Vocals — one row and one strip each,
    /// each folder item showing what is in it. A folder opened is opened
    /// on its own; the Folder switch opens them all.
    fn default() -> Self {
        Self {
            chosen: HashMap::new(),
            tops: true,
        }
    }
}

#[cfg(not(test))]
static STATE: std::sync::LazyLock<Mutex<State>> =
    std::sync::LazyLock::new(|| Mutex::new(State::default()));
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(not(test))]
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// Under test, each thread's own: a test folding folders must not fold the
// rows another test is planning beside it.
#[cfg(test)]
thread_local! {
    static STATE: &'static Mutex<State> = Box::leak(Box::default());
}

#[cfg(test)]
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .with(|state| *state)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Plan the rows again, as a fold does: something else they are drawn
/// from changed (which tracks show all their lanes).
pub fn touch() {
    changed();
}

fn changed() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// How many times the folds have changed: a view planning rows compares
/// this with the value it planned at.
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Whether a shut folder's row shows every cut in what it holds — in
/// Edit mode, where the cuts are the work — or the takes whole, the seams
/// joined (`daw_ui::studio::folded::join`), everywhere else: mixing or
/// playing a song, a slip-edited kit is still one take a song long.
static CUTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether shut folders show their cuts.
#[must_use]
pub fn cuts_shown() -> bool {
    CUTS.load(Ordering::Relaxed)
}

/// Show shut folders' cuts or not; a change plans the rows again.
pub fn show_cuts(on: bool) {
    if CUTS.swap(on, Ordering::Relaxed) != on {
        changed();
    }
}

/// Whether the folder at `index` in `rows` (track, depth) shows shut:
/// a folder whose next row is not one of its own. What its fold button
/// draws, and what a click on it undoes.
#[must_use]
pub fn shut(rows: &[(daw_proto::Track, u32)], index: usize) -> bool {
    let Some((track, depth)) = rows.get(index) else {
        return false;
    };
    track.is_folder
        && rows
            .get(index.saturating_add(1))
            .is_none_or(|(_, next)| next <= depth)
}

/// Open the folder `guid` if it is shut (`open` = whether it shows open
/// now), shut it if it is open.
pub fn toggle(guid: &str, open: bool) {
    state().chosen.insert(guid.to_owned(), !open);
    changed();
}

/// Whether every top-level folder is folded.
#[must_use]
pub fn tops() -> bool {
    state().tops
}

/// Fold every top-level folder (`true`), or open them again (`false`).
/// Either way the folders chosen one by one start over: the switch is a
/// fresh overview, not a filter over earlier choices.
pub fn set_tops(on: bool) {
    let mut state = state();
    state.tops = on;
    state.chosen.clear();
    drop(state);
    changed();
}

/// `rows` (tracks and their depths, in order) with the shut folders'
/// contents left out: each folder keeps its row.
#[must_use]
pub fn apply(rows: Vec<(daw_proto::Track, u32)>) -> Vec<(daw_proto::Track, u32)> {
    // Worked out before the lock: a scan of every row, no state in it.
    let parts = parts_and_sides(&rows);
    // The choices copied out, the lock let go before the walk.
    let (chosen, tops) = {
        let state = state();
        (state.chosen.clone(), state.tops)
    };
    let mut out = Vec::with_capacity(rows.len());
    // The depth of the shut folder whose contents are being skipped.
    let mut skipping: Option<u32> = None;
    for (track, depth) in rows {
        if let Some(shut) = skipping {
            if depth > shut {
                continue;
            }
            skipping = None;
        }
        if track.is_folder {
            let open = chosen.get(&track.guid).copied().unwrap_or_else(|| {
                !(shut_by_default(&track)
                    || parts.contains(track.guid.as_str())
                    || tops && depth == 0)
            });
            if !open {
                skipping = Some(depth);
            }
        }
        out.push((track, depth));
    }
    out
}

/// Whether a folder is one whose own folders start shut: the
/// guitars' (`Electric GTR`, `GTR E`, `Guitars`) — each a part — and the
/// drums' — each a piece of the kit (Kick, Snare, Toms, Cymbals, Rooms) —
/// by the organizer's classification. Opened, the family is a short list
/// of rows, not every mic.
fn shuts_its_folders(name: &str) -> bool {
    dynamic_template::track_schema::classify_track(name)
        .matched_groups
        .first()
        .is_some_and(|g| g == "Guitars" || g == "Drums")
}

/// Whether a folder name is a side of a stereo part: `L`, `R`, `C`.
fn is_side(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "l" | "r" | "c" | "left" | "right" | "center" | "centre"
    )
}

/// The folders that start shut because they are one part played to
/// several sides: a part (`Clean`, `Rhythm`, `Lead Harm` — a folder whose
/// children are its `L`/`R`/`C` sides) and each side. Shut, the part is
/// one row, its folder item showing what is under it; the sides and their
/// mics are there when it is opened. By guid.
fn parts_and_sides(rows: &[(daw_proto::Track, u32)]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for (i, (track, depth)) in rows.iter().enumerate() {
        if !track.is_folder {
            continue;
        }
        let children = rows
            .iter()
            .skip(i.saturating_add(1))
            .take_while(|(_, d)| d > depth)
            .filter(|(_, d)| *d == depth.saturating_add(1));
        let children: Vec<&daw_proto::Track> = children.map(|(t, _)| t).collect();
        let sides: Vec<&&daw_proto::Track> = children
            .iter()
            .filter(|t| t.is_folder && is_side(&t.name))
            .collect();
        if !sides.is_empty() {
            out.insert(track.guid.clone());
            out.extend(sides.iter().map(|t| t.guid.clone()));
        }
        // A guitars folder's own folders are its parts (`Clean`, `Rhythm`,
        // `Solo`), sides or none; a drums folder's, the kit's pieces: each
        // one row.
        // At any depth: Drums opens to Drum Kit and Process, Drum Kit to
        // its pieces.
        if shuts_its_folders(&track.name) {
            out.extend(
                children
                    .iter()
                    .filter(|t| t.is_folder)
                    .map(|t| t.guid.clone()),
            );
        }
    }
    out
}

/// Whether a folder starts shut until it is opened: the bus tree, the
/// reference material and the rehearsal in it, and the guide's rendered
/// prints. Buses are wiring and references are for listening
/// against, not tracks you work on — opened, they were rows of nothing
/// between the instruments.
fn shut_by_default(track: &daw_proto::Track) -> bool {
    let name = track.name.trim();
    [
        dynamic_template::buses::names::MIX,
        dynamic_template::apply::REFERENCES_FOLDER,
        dynamic_template::apply::REHEARSAL_FOLDER,
        dynamic_template::apply::PRINTS_FOLDER,
    ]
    .iter()
    .any(|shut| name.eq_ignore_ascii_case(shut))
}

#[cfg(test)]
mod tests {
    /// A folder shows shut when the row after it is not one of its own;
    /// a track that is not a folder never does.
    #[test]
    fn a_folder_is_shut_when_its_children_are_not_under_it() {
        let row = |name: &str, folder: bool, depth: u32| {
            (
                daw_proto::Track {
                    guid: name.to_owned(),
                    is_folder: folder,
                    ..daw_proto::Track::default()
                },
                depth,
            )
        };
        let rows = [
            row("drums", true, 0),
            row("kick", true, 1),
            row("snare", false, 1),
            row("bass", true, 0),
        ];
        assert!(!super::shut(&rows, 0), "the kick is under it: open");
        assert!(super::shut(&rows, 1), "the snare is beside it: shut");
        assert!(!super::shut(&rows, 2), "not a folder");
        assert!(super::shut(&rows, 3), "nothing after it: shut");
    }

    use super::*;

    fn row(name: &str, folder: bool, depth: u32) -> (daw_proto::Track, u32) {
        let track = daw_proto::Track {
            guid: name.to_owned(),
            name: name.to_owned(),
            is_folder: folder,
            ..daw_proto::Track::default()
        };
        (track, depth)
    }

    fn names(rows: &[(daw_proto::Track, u32)]) -> Vec<&str> {
        rows.iter().map(|(t, _)| t.name.as_str()).collect()
    }

    /// A session opens as its families: every top-level folder shut, its
    /// row kept, a loose top-level track shown.
    #[test]
    fn a_session_opens_with_its_top_level_folders_shut() {
        let rows = vec![
            row("Guide", true, 0),
            row("Click", false, 1),
            row("Drums", true, 0),
            row("Kick", false, 1),
            row("Bass", true, 0),
            row("DI", false, 1),
            row("Loose", false, 0),
        ];
        assert!(tops(), "the switch starts on");
        assert_eq!(names(&apply(rows)), ["Guide", "Drums", "Bass", "Loose"]);
    }

    #[test]
    fn folders_fold_by_choice_and_all_at_once() {
        // With the top-level folders open: what is under test is beneath them.
        crate::folds::set_tops(false);
        let rows = vec![
            row("Click", false, 0),
            row("Keys", true, 0),
            row("Piano", false, 1),
            row("Pads", true, 1),
            row("Pad 1", false, 2),
            row("Bass", true, 0),
            row("DI", false, 1),
        ];
        assert_eq!(
            names(&apply(rows.clone())).len(),
            7,
            "nothing chosen, nothing folded"
        );

        toggle("Keys", true);
        assert_eq!(
            names(&apply(rows.clone())),
            ["Click", "Keys", "Bass", "DI"],
            "a shut folder keeps its row and hides what is in it, however deep"
        );

        set_tops(true);
        assert_eq!(names(&apply(rows.clone())), ["Click", "Keys", "Bass"]);
        toggle("Bass", false);
        assert_eq!(
            names(&apply(rows.clone())),
            ["Click", "Keys", "Bass", "DI"],
            "one folder opened out of the folded whole"
        );

        crate::folds::set_tops(false);
        assert_eq!(
            names(&apply(rows)).len(),
            7,
            "and back, every choice forgotten"
        );
    }
}

#[cfg(test)]
mod default_tests {
    /// The bus tree, the references and the guide's prints start shut,
    /// every other folder open; opened, one stays open.
    #[test]
    fn the_mix_bus_starts_shut_and_opens_when_asked() {
        // With the top-level folders open: what is under test is beneath them.
        crate::folds::set_tops(false);
        let row = |guid: &str, name: &str, folder: bool, depth: u32| {
            (
                daw_proto::Track {
                    guid: guid.to_owned(),
                    name: name.to_owned(),
                    is_folder: folder,
                    ..daw_proto::Track::default()
                },
                depth,
            )
        };
        let rows = vec![
            row("drums", "Drums", true, 0),
            row("kick", "Kick", false, 1),
            row("mix", "MIX BUS", true, 0),
            row("vox", "VOX BUS", false, 1),
            row("refs", "REFERENCES", true, 0),
            row("mixdown", "Mixdown", false, 1),
            row("rehearsal", "REHEARSAL", true, 1),
            row("reh-kick", "Kick - Song", false, 2),
            row("guide", "Guide", true, 0),
            row("prints", "PRINTS", true, 1),
            row("clickprint", "ClickPrint", false, 2),
        ];
        let names = |rows: Vec<(daw_proto::Track, u32)>| -> Vec<String> {
            rows.into_iter().map(|(t, _)| t.guid).collect()
        };
        assert_eq!(
            names(super::apply(rows.clone())),
            ["drums", "kick", "mix", "refs", "guide", "prints"]
        );
        super::toggle("mix", false);
        assert_eq!(
            names(super::apply(rows.clone())),
            ["drums", "kick", "mix", "vox", "refs", "guide", "prints"]
        );
        // The references opened: the rehearsal in them is still shut.
        super::toggle("refs", false);
        assert_eq!(
            names(super::apply(rows)),
            [
                "drums",
                "kick",
                "mix",
                "vox",
                "refs",
                "mixdown",
                "rehearsal",
                "guide",
                "prints"
            ]
        );
    }
}

#[cfg(test)]
mod part_tests {
    /// A part played to sides starts as one row; a folder of plain tracks
    /// (a kick's mics) does not.
    #[test]
    fn a_part_and_its_sides_start_shut() {
        // With the top-level folders open: what is under test is beneath them.
        crate::folds::set_tops(false);
        let row = |guid: &str, folder: bool, depth: u32| {
            (
                daw_proto::Track {
                    guid: guid.to_owned(),
                    name: guid.to_owned(),
                    is_folder: folder,
                    ..daw_proto::Track::default()
                },
                depth,
            )
        };
        let rows = vec![
            row("Electric GTR", true, 0),
            row("Clean", true, 1),
            row("L", true, 2),
            row("Main", false, 3),
            row("R", true, 2),
            row("Main 2", false, 3),
            row("Solo", true, 1),
            row("Main 3", false, 2),
            row("Kick", true, 0),
            row("In", false, 1),
        ];
        let shown: Vec<String> = super::apply(rows)
            .into_iter()
            .map(|(t, _)| t.guid)
            .collect();
        assert_eq!(shown, ["Electric GTR", "Clean", "Solo", "Kick", "In"]);
    }

    /// The drums opened are the kit's pieces, each shut: Kick, Snare,
    /// Toms, Cymbals, Rooms — a loose drum track still shown — not
    /// every mic.
    #[test]
    fn the_drums_open_to_the_kit_pieces_shut() {
        crate::folds::set_tops(false);
        let row = |guid: &str, folder: bool, depth: u32| {
            (
                daw_proto::Track {
                    guid: guid.to_owned(),
                    name: guid.to_owned(),
                    is_folder: folder,
                    ..daw_proto::Track::default()
                },
                depth,
            )
        };
        let rows = vec![
            row("Drums", true, 0),
            row("Kick", true, 1),
            row("Kick In", false, 2),
            row("Kick Out", false, 2),
            row("Snare", true, 1),
            row("Snare Top", false, 2),
            row("Toms", true, 1),
            row("Tom 1", false, 2),
            row("Cymbals", true, 1),
            row("OH L", false, 2),
            row("Rooms", true, 1),
            row("Room L", false, 2),
            row("Drum Loop", false, 1),
            row("Bass", true, 0),
            row("Bass DI", false, 1),
        ];
        let shown: Vec<String> = super::apply(rows)
            .into_iter()
            .map(|(t, _)| t.guid)
            .collect();
        assert_eq!(
            shown,
            [
                "Drums",
                "Kick",
                "Snare",
                "Toms",
                "Cymbals",
                "Rooms",
                "Drum Loop",
                "Bass",
                "Bass DI"
            ]
        );
    }
}
