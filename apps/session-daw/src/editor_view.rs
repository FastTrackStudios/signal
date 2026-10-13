//! The Editor view: the expression editor, up close, on the selected
//! track's item.
//!
//! [`crate::expression::Expression`] is the editor mounted as a box in
//! a window — its picture a scene, its gestures calls into the core.
//! This is the view around it: which item it is on (the selected
//! track's first MIDI item, as a drum kit's lanes when the track is a
//! drum), the window's events turned into the editor's, and a word on
//! screen when there is nothing to edit rather than an empty roll that
//! reads as a broken load.

#[cfg(feature = "native")]
use anyrender::Scene;
#[cfg(feature = "native")]
use blitz_traits::events::{BlitzWheelDelta, MouseEventButton, UiEvent};
use dioxus::prelude::*;

use crate::expression::Expression;

/// What the view is on: a track's MIDI item, by guid, with what the
/// editor needs to read it.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub track: String,
    pub item: String,
    pub drums: bool,
    pub bpm: f64,
    pub length: f64,
    /// Where the item starts in the project, seconds: the take's zero.
    pub start: f64,
    /// An audio take (analysed into notes) rather than a MIDI one.
    pub audio: bool,
    /// The item's gain, which the analysis reads the take at.
    pub volume: f64,
    /// The track's name.
    pub name: String,
    /// More takes to stack with this one (a singer's doubles, a guitar's
    /// takes): the folder's other children, each its first audio item.
    /// Empty for one track.
    pub takes: Vec<Self>,
}

/// The item the Editor opens on in `project`: the selected track's
/// first MIDI item, else the first MIDI item of any track. `None` when
/// the session has no MIDI to edit.
#[must_use]
pub fn target(project: &daw_ui::studio::project::Project) -> Option<Target> {
    let song = Song::of(project);
    song.selected()
        .or_else(|| song.tracks.iter().find_map(|t| song.first_midi(t)))
}

/// What the view needs of the session to pick its item: the tracks (kept
/// current from the engine's events), their items, which are MIDI, and
/// the tempo.
#[derive(Clone, Default)]
pub struct Song {
    pub tracks: Vec<daw_proto::Track>,
    items: std::collections::HashMap<String, Vec<daw_proto::Item>>,
    midi: std::collections::HashSet<String>,
    bpm: f64,
}

impl Song {
    #[must_use]
    pub fn of(project: &daw_ui::studio::project::Project) -> Self {
        Self {
            tracks: project.tracks.clone(),
            items: project.items.clone(),
            midi: project.midi.clone(),
            bpm: project.bpm,
        }
    }

    /// `track`'s first item, as a target: its first MIDI item, else its
    /// first item as audio.
    fn first_midi(&self, track: &daw_proto::Track) -> Option<Target> {
        let items = self.items.get(&track.guid)?;
        let item = items
            .iter()
            .find(|i| self.midi.contains(&i.guid))
            .or_else(|| items.first())?;
        Some(self.target_of(track, item))
    }

    /// The item `guid`, wherever it is, as the editor's target: what a
    /// click on it in the arrangement opens.
    #[must_use]
    pub fn item(&self, guid: &str) -> Option<Target> {
        self.tracks.iter().find_map(|track| {
            let item = self
                .items
                .get(&track.guid)?
                .iter()
                .find(|i| i.guid == guid)?;
            Some(self.target_of(track, item))
        })
    }

    /// `item` on `track`, as a target.
    fn target_of(&self, track: &daw_proto::Track, item: &daw_proto::Item) -> Target {
        Target {
            audio: !self.midi.contains(&item.guid),
            volume: item.volume,
            name: track.name.clone(),
            takes: Vec::new(),
            track: track.guid.clone(),
            item: item.guid.clone(),
            drums: self.is_drums(track),
            bpm: self.bpm,
            length: item.length.as_seconds(),
            start: item.position.as_seconds(),
        }
    }

    /// Whether `track` is a drum's: by its name, or by a folder it is in
    /// (a kick's `Trig` is a drum though its name says nothing of it).
    fn is_drums(&self, track: &daw_proto::Track) -> bool {
        let mut at = Some(track);
        let mut steps = 0;
        while let Some(t) = at {
            if crate::expression::is_drum_track(&t.name) {
                return true;
            }
            steps += 1;
            if steps > 64 {
                break;
            }
            at = t
                .parent_guid
                .as_deref()
                .and_then(|g| self.tracks.iter().find(|p| p.guid == g));
        }
        false
    }

    /// What the selected track opens: a folder of takes stacked (two or
    /// more of its children with audio — a drum's folder excepted), else
    /// the track's own first item.
    #[must_use]
    pub fn selected(&self) -> Option<Target> {
        self.tracks.iter().filter(|t| t.selected).find_map(|t| {
            // A drum's folder holds its mics, not takes: the kit is
            // edited in the arrangement, on its folder items.
            if t.is_folder && !self.is_drums(t) {
                self.takes(t)
            } else {
                self.first_midi(t)
            }
        })
    }

    /// Whether `track` has an audio item.
    fn has_audio(&self, track: &daw_proto::Track) -> bool {
        self.items
            .get(&track.guid)
            .is_some_and(|items| items.iter().any(|i| !self.midi.contains(&i.guid)))
    }

    /// `track`'s audio item overlapping `from..to` the most, as a target.
    fn overlapping(&self, track: &daw_proto::Track, from: f64, to: f64) -> Option<Target> {
        let overlap = |i: &daw_proto::Item| {
            let start = i.position.as_seconds();
            (to.min(start + i.length.as_seconds()) - from.max(start)).max(0.0)
        };
        let item = self
            .items
            .get(&track.guid)?
            .iter()
            .filter(|i| !self.midi.contains(&i.guid))
            .max_by(|a, b| overlap(a).total_cmp(&overlap(b)))?;
        Some(Target {
            track: track.guid.clone(),
            item: item.guid.clone(),
            drums: false,
            bpm: self.bpm,
            length: item.length.as_seconds(),
            start: item.position.as_seconds(),
            audio: true,
            volume: item.volume,
            name: track.name.clone(),
            takes: Vec::new(),
        })
    }

    /// The tracks directly in `folder`, in order.
    fn children<'a>(&'a self, folder: &'a str) -> impl Iterator<Item = &'a daw_proto::Track> {
        self.tracks
            .iter()
            .filter(move |c| c.parent_guid.as_deref() == Some(folder))
    }

    /// Every track under `folder` with no folder of its own, in order.
    fn leaves<'a>(&'a self, folder: &'a str) -> Vec<&'a daw_proto::Track> {
        let mut out = Vec::new();
        for c in self.children(folder) {
            if c.is_folder {
                out.extend(self.leaves(&c.guid));
            } else {
                out.push(c);
            }
        }
        out
    }

    /// A folder's takes stacked: its children with an audio item, the
    /// first the one the others line up against.
    fn takes(&self, folder: &daw_proto::Track) -> Option<Target> {
        // Each child is a take: a track with audio on it, or a folder of
        // its sources (a singer's Main over its EN), whose first source
        // with audio stands for it, named for the take.
        let sources: Vec<(&daw_proto::Track, &str)> = self
            .children(&folder.guid)
            .filter_map(|c| {
                let source = if c.is_folder {
                    self.leaves(&c.guid).into_iter().find(|l| self.has_audio(l))
                } else {
                    Some(c).filter(|c| self.has_audio(c))
                };
                source.map(|s| (s, c.name.as_str()))
            })
            .collect();
        let (first, rest) = sources.split_first()?;
        if rest.is_empty() {
            return None;
        }
        let named = |mut t: Target, name: &str| {
            name.clone_into(&mut t.name);
            t
        };
        let first = named(self.first_midi(first.0)?, first.1);
        // The others at the same place in the song: the item that overlaps
        // the first take's most, so the stack is one stretch of the song.
        let (from, to) = (first.start, first.start + first.length);
        let takes: Vec<Target> = rest
            .iter()
            .filter_map(|(t, name)| self.overlapping(t, from, to).map(|x| named(x, name)))
            .collect();
        let mut first = first;
        first.takes = takes;
        Some(first)
    }
}

/// The item last clicked in the arrangement, and how many clicks there
/// have been: the Editor opens what you click, as REAPER's does. Written
/// by the arrangement ([`picked`]), read by the Editor each frame.
static PICKED: std::sync::Mutex<(u64, Option<String>)> = std::sync::Mutex::new((0, None));

/// The arrangement picked `item` (a plain click selects it alone).
pub fn picked(item: &str) {
    let mut slot = PICKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    slot.0 = slot.0.wrapping_add(1);
    slot.1 = Some(item.to_owned());
}

/// How many items have been picked.
fn picked_count() -> u64 {
    PICKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .0
}

/// The item picked since `seen`, with the count it was picked at.
fn picked_since(seen: u64) -> Option<(u64, String)> {
    let slot = PICKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    (slot.0 != seen)
        .then(|| slot.1.clone().map(|g| (slot.0, g)))
        .flatten()
}

/// The Editor view, on the session's selected track.
#[component]
pub fn EditorPanel() -> Element {
    let Some(session) = try_use_context::<crate::studio::StudioSession>() else {
        return rsx! { crate::shell::EditorComing {} };
    };
    let song = Song::of(&session.project.0);
    #[cfg(feature = "native")]
    {
        let sink = crate::window_cursor::use_cursor_sink();
        let widget = use_hook(|| {
            dioxus_native_dom::CustomWidgetAttr::new(EditorView::new(song).with_cursor(sink))
        });
        rsx! {
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%; background:#0b0c0e;",
                data: widget,
            }
        }
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = song;
        rsx! { crate::shell::EditorComing {} }
    }
}

/// The widget: the editor on its item, or why there is none.
#[cfg(feature = "native")]
pub struct EditorView {
    /// The session, kept current from the engine: a selection made in
    /// the arrangement moves the editor to that track's item.
    song: Song,
    watch: Option<crate::engine::Watch>,
    /// The engine's events heard since the last frame.
    pending: std::cell::RefCell<Vec<daw_proto::track::TrackEvent>>,
    target: Option<Target>,
    view: Option<Expression>,
    look: expression_editor_paint::paint::Look,
    /// The theme the look was taken from: a new one (a saved edit to the
    /// theme file) and the look is taken again.
    skin: u64,
    font: Option<crate::text::Font>,
    /// Loaded yet: the take is read on the first paint, when the box's
    /// size is known.
    tried: bool,
    /// A read in flight on the engine's runtime: its result lands here,
    /// and the next frame takes it. Replaced (and so ignored) when the
    /// editor moves to another item before it lands.
    loading: Option<Slot>,
    /// The last read found nothing to edit.
    failed: bool,
    /// The saver: one task on the engine's runtime writing each saved
    /// take back in the order it was saved, so a save never holds the
    /// window and two saves never land out of order.
    saver: Option<tokio::sync::mpsc::UnboundedSender<(String, Content)>>,
    dirty: bool,
    /// The view on screen is the previous take's, kept up while the next
    /// one loads ([`Self::retarget`]).
    stale: bool,
    /// The last item click followed ([`picked`]'s count).
    picked_seen: u64,
    /// The stack's takes, first the one the others line up on.
    takes: Vec<Take>,
    /// An alignment on the engine's runtime, landing on a later frame.
    aligning: Option<Aligning>,
    /// Where the pointer's shape goes: the window's cursor.
    cursor: Option<crate::window_cursor::Sink>,
    /// The keys held at the last event, and the zoom key as last shown:
    /// the zoom key going down turns the pointer into the magnifier at
    /// once, as in the arrangement, with no event of the Editor's own.
    mods: std::cell::Cell<crate::mousemap::Mods>,
    zoom_shown: std::cell::Cell<bool>,
}

/// What a read hands back: nothing yet, then the take or `None`.
#[cfg(feature = "native")]
type Slot = std::sync::Arc<std::sync::Mutex<Option<Read>>>;

/// What a read hands back: a MIDI take, or an audio take analysed into
/// the editor — `None` inside when there was nothing to edit.
#[cfg(feature = "native")]
enum Read {
    Midi(Option<daw::service::midi::MidiTakeSnapshot>),
    Audio(Option<(Box<expression_editor_core::Editor>, Vec<Take>)>),
}

/// One take of a stack, as aligning it needs it: its item, where the item
/// starts in the song, and its analysis.
#[cfg(feature = "native")]
#[derive(Clone)]
struct Take {
    item: String,
    start: f64,
    analysis: std::sync::Arc<expression_editor_audio::Analysis>,
}

/// The editor action that lines a stack's takes up on its first.
pub const ALIGN_TAKES: &str = expression_editor_core::actions::takes::ALIGN.id;

/// An alignment in flight: each take aligned, by item, when it lands.
#[cfg(feature = "native")]
type Aligning = std::sync::Arc<
    std::sync::Mutex<Option<Vec<(String, f64, expression_editor_audio::Alignment)>>>,
>;
#[cfg(feature = "native")]
type Content = daw::service::midi::MidiTakeContent;

/// One audio take's analysis, through the in-process engine: what its
/// lane draws and what aligning it reads. `None` with no engine, or a
/// take with no samples.
#[cfg(feature = "native")]
#[must_use]
pub fn analysis(item: &str, length: f64, volume: f64) -> Option<expression_editor_audio::Analysis> {
    session(item.to_owned(), length, volume).map(|s| s.analysis().clone())
}

/// One audio take loaded and analysed through the in-process engine:
/// `None` with no engine, or a take with no samples.
#[cfg(feature = "native")]
fn session(
    item: String,
    length: f64,
    volume: f64,
) -> Option<expression_editor_audio::AudioSession> {
    let daw = crate::open::local_engine()?;
    expression_editor_audio::AudioSession::load(
        daw,
        expression_editor_audio::AudioTakeLocation {
            project: daw::service::ProjectContext::Current,
            item: daw::service::ItemRef::Guid(item),
            take: daw::service::TakeRef::Active,
        },
        length,
        volume,
        expression_editor_core::Viewport::new(1000.0, 500.0),
        expression_editor_audio::TakeConfig::default(),
    )
}

/// The first take analysed, with `takes` stacked under it: each analysed
/// in turn, moved onto the first take's timeline (its item starting
/// where it does in the project), on a lane of its own.
#[cfg(feature = "native")]
fn stacked(
    (item, length, volume, start, name): (String, f64, f64, f64, String),
    takes: &[Target],
) -> Option<(Box<expression_editor_core::Editor>, Vec<Take>)> {
    let first = session(item.clone(), length, volume)?;
    let mut aligned = vec![Take {
        item: item.clone(),
        start,
        analysis: std::sync::Arc::new(first.analysis().clone()),
    }];
    let mut editor = first.editor;
    if takes.is_empty() {
        return Some((Box::new(editor), aligned));
    }
    editor.adopt_track_identity(item, Some(name));
    let ups = editor.doc.time_base.units_per_second(editor.bpm);
    for take in takes {
        let Some(other) = session(take.item.clone(), take.length, take.volume) else {
            continue;
        };
        aligned.push(Take {
            item: take.item.clone(),
            start: take.start,
            analysis: std::sync::Arc::new(other.analysis().clone()),
        });
        let mut doc = other.editor.doc;
        let delta = (take.start - start) * ups;
        if delta.abs() > 1e-9 {
            doc.start += delta;
            doc.end += delta;
            for n in &mut doc.notes {
                n.shift_time(delta);
            }
        }
        editor.add_track_with_guid(take.item.clone(), take.name.clone(), doc);
    }
    editor.stacked = true;
    editor.fit_lanes();
    Some((Box::new(editor), aligned))
}

/// Start the saver on the engine's runtime: each take sent is written
/// back in turn. `None` with no engine.
#[cfg(feature = "native")]
fn saver() -> Option<tokio::sync::mpsc::UnboundedSender<(String, Content)>> {
    let runtime = crate::open::runtime()?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, Content)>();
    runtime.spawn(async move {
        while let Some((item, content)) = rx.recv().await {
            if !crate::expression::write_take(item.clone(), content).await {
                tracing::warn!(editor.item = %item, "the editor's take could not be written back");
            }
        }
    });
    Some(tx)
}

#[cfg(feature = "native")]
impl EditorView {
    #[must_use]
    pub fn new(song: Song) -> Self {
        let target = song
            .selected()
            .or_else(|| song.tracks.iter().find_map(|t| song.first_midi(t)));
        Self {
            song,
            watch: crate::engine::Watch::start(),
            pending: std::cell::RefCell::default(),
            target,
            view: None,
            look: look(),
            skin: crate::skin::version(),
            font: crate::text::Font::embedded().ok(),
            tried: false,
            loading: None,
            failed: false,
            saver: saver(),
            dirty: true,
            picked_seen: picked_count(),
            stale: false,
            takes: Vec::new(),
            aligning: None,
            cursor: None,
            mods: std::cell::Cell::default(),
            zoom_shown: std::cell::Cell::new(false),
        }
    }

    /// Read the target's take into the editor, `size` big. A take with
    /// no notes, or no engine to read it from, leaves no editor.
    fn load(&mut self) {
        self.tried = true;
        self.failed = false;
        let (Some(t), Some(runtime)) = (&self.target, crate::open::runtime()) else {
            self.failed = self.target.is_some();
            // Nothing will land to replace the previous take's picture.
            if std::mem::take(&mut self.stale) {
                self.view = None;
            }
            return;
        };
        let slot: Slot = std::sync::Arc::default();
        let into = std::sync::Arc::clone(&slot);
        let (item, bpm, length) = (t.item.clone(), t.bpm, t.length);
        if t.audio {
            // Read and analysed off the window: a take's samples through
            // the engine's accessor, its notes found by pitch analysis —
            // and a stack's other takes after it, each on its own lane.
            let (main, takes) = (
                (item, length, t.volume, t.start, t.name.clone()),
                t.takes.clone(),
            );
            runtime.spawn_blocking(move || {
                let editor = stacked(main, &takes);
                if let Ok(mut slot) = into.lock() {
                    *slot = Some(Read::Audio(editor));
                }
            });
        } else {
            runtime.spawn(async move {
                let read = crate::expression::read_take(item, bpm, length).await;
                if let Ok(mut slot) = into.lock() {
                    *slot = Some(Read::Midi(read));
                }
            });
        }
        self.loading = Some(slot);
    }

    /// The read that landed, into the editor, `size` big.
    fn landed(&mut self, size: (f64, f64)) {
        let Some(read) = self
            .loading
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|mut s| s.take()))
        else {
            return;
        };
        self.loading = None;
        let view = match (&self.target, read) {
            (Some(t), Read::Midi(Some(snapshot))) => Some(Expression::from_take(
                &snapshot,
                t.item.clone(),
                t.drums,
                (0.0, 0.0),
                size,
            )),
            (Some(_), Read::Audio(Some((editor, takes)))) => {
                self.takes = takes;
                Some(Expression::from_audio(*editor, (0.0, 0.0), size))
            }
            _ => None,
        };
        // Whatever lands, the previous take's picture goes.
        self.stale = false;
        let Some(mut view) = view else {
            tracing::warn!("the editor's take could not be read, or has nothing to edit");
            self.view = None;
            self.failed = true;
            return;
        };
        view.set_look(self.look);
        view.relayout();
        frame_item(&mut view);
        self.view = Some(view);
        self.dirty = true;
    }

    fn event(&mut self, event: &UiEvent) {
        if self.stale {
            return;
        }
        let Some(view) = self.view.as_mut() else {
            return;
        };
        let changed = match event {
            UiEvent::PointerDown(e) => {
                let button = match e.button {
                    MouseEventButton::Secondary => 2,
                    MouseEventButton::Auxiliary => 1,
                    _ => 0,
                };
                view.press(
                    f64::from(e.coords.client_x),
                    f64::from(e.coords.client_y),
                    crate::widget::mods(e.mods),
                    button,
                )
            }
            UiEvent::PointerMove(e) => view.moved(
                f64::from(e.coords.client_x),
                f64::from(e.coords.client_y),
                crate::widget::mods(e.mods),
            ),
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) => view.release(
                f64::from(e.coords.client_x),
                f64::from(e.coords.client_y),
                crate::widget::mods(e.mods),
            ),
            UiEvent::Wheel(e) => {
                let (dx, dy) = match e.delta {
                    BlitzWheelDelta::Lines(x, y) => (x * 40.0, y * 40.0),
                    BlitzWheelDelta::Pixels(x, y) => (x, y),
                };
                view.wheel(
                    f64::from(e.coords.client_x),
                    f64::from(e.coords.client_y),
                    dx,
                    dy,
                    crate::widget::mods(e.mods),
                )
            }
            UiEvent::KeyDown(e) => view.key(&key_name(&e.key), crate::widget::mods(e.modifiers)),
            UiEvent::KeyUp(e) => view.key_up(&key_name(&e.key), crate::widget::mods(e.modifiers)),
            _ => false,
        };
        self.dirty |= changed;
        if view.host_actions().iter().any(|a| a == ALIGN_TAKES) {
            self.align_takes();
        }
        self.show_pointer(event);
        // A gesture finished or a key pressed that changed the take:
        // write it back, so the arrangement and the engine have it.
        if matches!(event, UiEvent::PointerUp(_) | UiEvent::KeyDown(_)) {
            self.save();
        }
    }

    /// Point the pointer's shape at `sink` (the window's cursor).
    #[must_use]
    pub fn with_cursor(mut self, sink: crate::window_cursor::Sink) -> Self {
        self.cursor = Some(sink);
        self
    }

    /// Put the Editor's glyph for the pointer on the window: on every
    /// pointer event over it, and on a key while the pointer is over the
    /// roll (a held `z` turns the arrow into the magnifier in place).
    fn show_pointer(&self, event: &UiEvent) {
        let (Some(sink), Some(view)) = (&self.cursor, &self.view) else {
            return;
        };
        let mods = match event {
            UiEvent::PointerMove(e) | UiEvent::PointerDown(e) | UiEvent::PointerUp(e) => {
                crate::widget::mods(e.mods)
            }
            UiEvent::KeyDown(e) | UiEvent::KeyUp(e) => crate::widget::mods(e.modifiers),
            _ => return,
        };
        self.mods.set(mods);
        let key = matches!(event, UiEvent::KeyDown(_) | UiEvent::KeyUp(_));
        Self::put_pointer(sink, view, mods, key);
    }

    /// The glyph for the pointer with `mods`, onto the window; `quiet`
    /// leaves the window's own shape when the pointer is off the roll
    /// (a key, or the zoom key, that the Editor did not see it over).
    fn put_pointer(
        sink: &crate::window_cursor::Sink,
        view: &Expression,
        mods: crate::mousemap::Mods,
        quiet: bool,
    ) {
        use crate::window_cursor::{Glyph, Shape};
        use expression_editor_core::cursor::Cursor;
        let pointer = view.pointer(mods);
        if quiet && pointer.is_none() {
            return;
        }
        sink(match pointer {
            None | Some(Glyph::Editor(Cursor::Arrow)) => {
                Shape::Icon(cursor_icon::CursorIcon::Default)
            }
            Some(glyph) => Shape::Glyph(glyph),
        });
    }

    /// Line the stack's takes up on its first: each aligned over the
    /// stretch of the song it shares with the first, off the window, its
    /// stretch markers written to its item; the lanes follow when the
    /// answers land ([`Self::aligned`]).
    pub fn align_takes(&mut self) {
        let (Some((reference, dubs)), Some(runtime)) =
            (self.takes.split_first(), crate::open::runtime())
        else {
            return;
        };
        if dubs.is_empty() || self.aligning.is_some() {
            return;
        }
        let (reference, dubs) = (reference.clone(), dubs.to_vec());
        let slot: Aligning = std::sync::Arc::default();
        let into = std::sync::Arc::clone(&slot);
        runtime.spawn_blocking(move || {
            let done = align_all(&reference, &dubs);
            if let Ok(mut slot) = into.lock() {
                *slot = Some(done);
            }
        });
        self.aligning = Some(slot);
    }

    /// Whether an alignment is still running.
    #[must_use]
    pub const fn aligning(&self) -> bool {
        self.aligning.is_some()
    }

    /// The alignments that landed, onto the lanes: each take's notes moved
    /// to where its audio now plays.
    fn aligned(&mut self) {
        let Some(done) = self
            .aligning
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|mut s| s.take()))
        else {
            return;
        };
        self.aligning = None;
        let (Some(view), Some(reference)) = (self.view.as_mut(), self.takes.first()) else {
            return;
        };
        let editor = &mut view.editor;
        let ups = editor.doc.time_base.units_per_second(editor.bpm);
        for (item, start, alignment) in &done {
            let delta = (start - reference.start) * ups;
            let warp = |u: f64| {
                expression_editor_audio::align::warped(alignment, (u - delta) / ups)
                    .mul_add(ups, delta)
            };
            let active = editor
                .tracks
                .track(editor.tracks.active())
                .is_some_and(|t| t.guid == *item);
            if active {
                warp_doc(&mut editor.doc, warp);
            } else if let Some(doc) = editor
                .tracks
                .index_of_guid(item)
                .and_then(|i| editor.tracks.doc_mut(i))
            {
                warp_doc(doc, warp);
            }
        }
        // The stack's picture is keyed on the camera and the counts, not
        // the notes' times: a warp moves notes and changes no count.
        view.invalidate_stack();
        view.relayout();
        self.dirty = true;
    }

    /// Write the editor's take back to its item, if it changed.
    fn save(&mut self) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if !view.unsaved() {
            return;
        }
        let Some(item) = view.item().map(str::to_owned) else {
            return;
        };
        let content = view.take_content();
        let sent = self
            .saver
            .as_ref()
            .is_some_and(|tx| tx.send((item.clone(), content)).is_ok());
        if !sent {
            tracing::warn!(editor.item = %item, "the editor has no engine to write its take back to");
        }
    }

    /// Take the engine's track events: a newly selected track with a
    /// MIDI item is the editor's next item (a track with none leaves it
    /// where it is). `true` when the item changed.
    fn follow(&mut self) -> bool {
        // An item clicked in the arrangement: that item, MIDI or audio.
        if let Some((count, guid)) = picked_since(self.picked_seen) {
            self.picked_seen = count;
            if let Some(next) = self.song.item(&guid) {
                return self.retarget(next);
            }
        }
        let mut events = std::mem::take(&mut *self.pending.borrow_mut());
        if let Some(watch) = &self.watch {
            events.extend(watch.drain());
        }
        if events.is_empty() {
            return false;
        }
        for event in &events {
            crate::engine::apply_event(&mut self.song.tracks, event);
        }
        let Some(next) = self.song.selected() else {
            return false;
        };
        self.retarget(next)
    }

    /// Move the editor onto `next`, unless it is already there.
    fn retarget(&mut self, next: Target) -> bool {
        if self.target.as_ref().is_some_and(|t| t.item == next.item) {
            return false;
        }
        // Whatever is unsaved goes back before the editor moves on.
        self.save();
        self.target = Some(next);
        // The take on screen stays up until the next one lands, and is
        // swapped for it in one frame: dropping it here painted the box
        // empty for the frames the read took — a black flash on every
        // click. Until then it is a picture: it takes no input and does
        // not follow the transport ([`Self::stale`]).
        self.stale = self.view.is_some();
        self.loading = None;
        self.tried = false;
        true
    }

    /// One frame, `width` × `height` at `scale`, offscreen: what the
    /// window paints, for a test or the bench.
    pub fn frame(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_scene(width, height, scale)
    }

    /// The editor, once its take has landed.
    pub const fn editor(&mut self) -> Option<&mut Expression> {
        self.view.as_mut()
    }

    /// Write the take back now, if it changed — what the end of a
    /// gesture does.
    pub fn flush(&mut self) {
        self.save();
    }

    /// Whether the engine has said anything since the last frame.
    fn heard(&self) -> bool {
        if crate::tool::zoom_held() != self.zoom_shown.get() {
            return true;
        }
        if picked_count() != self.picked_seen {
            return true;
        }
        let Some(watch) = &self.watch else {
            return false;
        };
        let mut pending = self.pending.borrow_mut();
        pending.extend(watch.drain());
        !pending.is_empty()
    }

    fn paint_scene(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.follow();
        let scale = scale.max(f64::EPSILON);
        let size = (f64::from(width) / scale, f64::from(height) / scale);
        if !self.tried {
            self.load();
        }
        self.landed(size);
        self.aligned();
        // The zoom key went down or up at the window: the pointer follows.
        let zoom = crate::tool::zoom_held();
        if zoom != self.zoom_shown.replace(zoom)
            && let (Some(sink), Some(view)) = (&self.cursor, &self.view)
        {
            Self::put_pointer(sink, view, self.mods.get(), true);
        }
        // The transport's playhead, and the page following it.
        if let (false, Some(view), Some(t), Some(transport)) = (
            self.stale,
            self.view.as_mut(),
            self.target.as_ref(),
            crate::engine::Transport::shared(),
        ) {
            let (at, playing) = transport.read();
            view.follow_transport(at, playing, t.start);
        }
        if self.skin != crate::skin::version() {
            self.skin = crate::skin::version();
            self.look = look();
            if let Some(view) = self.view.as_mut() {
                view.set_look(self.look);
            }
        }
        self.dirty = false;
        let mut scene = Scene::new();
        let mut inner = Scene::new();
        if let Some(view) = self.view.as_mut() {
            view.layout((0.0, 0.0), size);
            view.paint(&mut inner);
        } else if let Some(font) = &self.font {
            let why = if self.loading.is_some() {
                "Loading…"
            } else if self.target.is_some() && self.failed {
                "This item's notes could not be read."
            } else {
                "Select a track with a MIDI item to edit it here."
            };
            crate::tcp::glyphs(
                &mut inner,
                font,
                self.look.text_dim,
                why,
                24.0,
                size.1 / 2.0,
                14.0,
            );
        }
        anyrender::PaintScene::append_scene(&mut scene, inner, vello::kurbo::Affine::scale(scale));
        scene
    }
}

/// The editor's colours from the loaded theme: the window's palette, and
/// the theme's `editor.*` tokens over it.
#[cfg(feature = "native")]
fn look() -> expression_editor_paint::paint::Look {
    crate::expression::look_of(&crate::skin::palette(
        &crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark()),
    ))
}

/// A key as the editor names it: a character's text, else the W3C name
/// (`"Delete"`, `"ArrowLeft"`).
#[cfg(feature = "native")]
/// Frame what was just opened: the whole item across, its notes top to
/// bottom — the editor's "fit item" (`z i`), MeMagic's fixed framing. A
/// stack frames its own lanes.
#[cfg(feature = "native")]
fn frame_item(view: &mut Expression) {
    if view.stacked() {
        return;
    }
    use expression_editor_core::actions;
    actions::run(&mut view.editor, actions::view::FIT_ITEM.id);
}

/// Each of `dubs` aligned to `reference` and its markers written: the
/// takes that aligned, each with its item's start and its alignment.
#[cfg(feature = "native")]
fn align_all(
    reference: &Take,
    dubs: &[Take],
) -> Vec<(String, f64, expression_editor_audio::Alignment)> {
    let Some(daw) = crate::open::local_engine() else {
        return Vec::new();
    };
    dubs.iter()
        .filter_map(|dub| {
            let alignment = expression_editor_audio::align_placed(
                &reference.analysis,
                reference.start,
                &dub.analysis,
                dub.start,
                expression_editor_audio::AlignConfig::default(),
            )?;
            if let Err(error) = expression_editor_audio::apply_quantize::apply_warp(
                daw,
                daw::service::ProjectContext::Current,
                &[daw::service::ItemRef::Guid(dub.item.clone())],
                &alignment,
            ) {
                tracing::warn!(editor.item = %dub.item, error = ?error, "a take's alignment could not be written");
                return None;
            }
            Some((dub.item.clone(), dub.start, alignment))
        })
        .collect()
}

/// Every note of `doc` moved by `warp` (a unit to a unit): its start and
/// end, its curves stretched between them.
#[cfg(feature = "native")]
fn warp_doc(doc: &mut expression_editor_core::ExpressionDoc, warp: impl Fn(f64) -> f64) {
    use expression_editor_core::doc::Dimension;
    for n in &mut doc.notes {
        let (s, e) = (n.start, n.end);
        let (s2, e2) = (warp(s), warp(e));
        n.shift_time(s2 - s);
        if (e2 - (e + s2 - s)).abs() > 1e-9 {
            for d in Dimension::ALL {
                n.curve_mut(d).remap_time(s2, e + s2 - s, s2, e2);
            }
            n.end = e2;
        }
    }
}

fn key_name(key: &blitz_traits::events::Key) -> String {
    match key {
        blitz_traits::events::Key::Character(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(feature = "native")]
impl blitz_dom::node::Widget for EditorView {
    fn handle_event(&mut self, event: &UiEvent) {
        self.event(event);
    }

    fn needs_redraw(&self) -> bool {
        self.dirty
            || self.loading.is_some()
            || self.aligning.is_some()
            || crate::engine::moving()
            || self.heard()
            || self.skin != crate::skin::version()
            || self.view.as_ref().is_some_and(Expression::dragging)
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_scene(width, height, scale)
    }
}
