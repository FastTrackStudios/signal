//! The studio as a panel: the painted arrangement, its ruler, its track
//! panel and its scrollbars, in whatever rectangle it is given.
//!
//! Lifted out of `bin/blitz_shot.rs`, where it was one whole window, so the
//! Session app can dock it beside the other panels (`docs/app-on-blitz.md`).
//! What changed on the way:
//!
//! - **Sized by its container**, not by the window. The window may hold
//!   other panels; this one fills the tile it is put in and reads its own
//!   rectangle back from the layout.
//! - **Input only over itself.** Blitz does not send wheel events to the
//!   DOM, so the wheel, the middle-drag hand and the zoom spring are read
//!   at the winit level — and with other panels in the window, only while
//!   the pointer is inside this panel's rectangle.
//! - **No side rails and no modes.** The modes belong to the app's top bar
//!   now, visible whichever view is up.
//!
//! [`StudioSession`] is what the panel stands on: the opened, prepared
//! session and the reads the widget is built from. The app opens it once
//! and provides it as context; every Arrangement panel reads that.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;

use daw_ui::studio::{ProjectRef, RowsRef};

/// Pixels a second at zoom 1 — the studio's base scale.
pub const PPS: f64 = 40.0;

use crate::panel::BAR;

/// The arrangement's queue of edits for the engine, shared with what sits on
/// the panel beside the widget (the main toolbar). Equal by identity.
#[derive(Clone)]
pub struct Edits(pub Rc<RefCell<Vec<crate::engine::Edit>>>);

impl PartialEq for Edits {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The opened session, as the panels read it.
///
/// Cheap to clone: everything in it is shared.
#[derive(Clone)]
pub struct StudioSession {
    pub project: ProjectRef,
    pub rows: RowsRef,
    pub previews: crate::midi::Previews,
    /// The song's chart, parsed once at [`StudioSession::open`] — what the
    /// Chart panel paints (`crate::chart_panel`). `None` when the session
    /// was opened with no `--chart`, or the file failed to parse: the DAW
    /// and Performance views still work without it.
    pub chart: Option<Arc<keyflow::Chart>>,
    /// The `.kf` the chart was read from — what Organize mode's editor
    /// opens and saves. `None` with no chart file.
    pub chart_file: Option<std::path::PathBuf>,
    /// How the rows were planned, kept so they can be planned again after
    /// a change the plan depends on (a track shown or hidden) without
    /// reading the whole session back from the engine.
    pub planner: Planner,
}

/// The row plan's inputs: the project as the engine had it (before the
/// plan folds anything), the scene it was laid out by, and the track kinds
/// the template wrote into the file (read once, at open).
#[derive(Clone)]
pub struct Planner {
    pub raw: Arc<daw_ui::studio::project::Project>,
    pub scene: Option<&'static str>,
    pub kinds: Arc<crate::plan::Kinds>,
}

impl Planner {
    /// Plan the rows for `raw`: [`plan_rows`] with this planner's scene
    /// and file.
    #[must_use]
    pub fn plan(&self, raw: &daw_ui::studio::project::Project) -> (ProjectRef, RowsRef) {
        plan_rows_with(raw, self.scene, &self.kinds)
    }

    /// The mixer's strips for `raw` outside Mix mode — docked under the
    /// arrangement or on its own: the scene's MIXER rows (a folder the
    /// scene hides from the mixer is gone, one it folds only in the
    /// arrangement keeps every strip), and the folders the window shut
    /// (`crate::folds`), which both panels share. Each strip is the track
    /// as the engine has it: the scene decides which strips, not how wide
    /// a strip is outside Mix mode.
    ///
    /// Not the arrangement's rows: the two panels answer different
    /// questions, and the scene says so per surface.
    #[must_use]
    pub fn plan_strips(&self, raw: &daw_ui::studio::project::Project) -> RowsRef {
        RowsRef(Arc::new(plan_strips_with(raw, self.scene, &self.kinds)))
    }

    /// The mixer's rows for `raw`: the scene laid out for the mixer — each
    /// track as wide as the scene opens it, the one it focuses selected —
    /// and every folder as the scene leaves it, the way the scene renders
    /// (`tests/golden_scenes.rs`), then as the window's folds leave it.
    /// `extent` is the panel's height.
    #[must_use]
    pub fn plan_mixer(
        &self,
        raw: &daw_ui::studio::project::Project,
        scene: Option<&str>,
        settings: crate::settings::Settings,
        extent: f64,
    ) -> RowsRef {
        let (visible, depths) =
            daw_ui::components::folders::FolderState::default().visible(&raw.tracks);
        let planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
        let Some(scene) = scene
            .or(self.scene)
            .and_then(dynamic_template::scenes::scene)
        else {
            return RowsRef(Arc::new(crate::folds::apply(planned)));
        };
        // And the folders the window shut (`crate::folds`), over the
        // scene's: shut in the arrangement, shut here — and a folder
        // opened or shut on a strip in Mix mode is.
        RowsRef(Arc::new(crate::folds::apply(crate::plan::apply_scene(
            &planned,
            &self.kinds,
            scene,
            crate::plan::Panel {
                surface: crate::plan::Surface::Mixer,
                mode: None,
                settings,
                extent,
                active_language: None,
            },
        ))))
    }
}

impl PartialEq for StudioSession {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.project.0, &other.project.0) && Arc::ptr_eq(&self.rows.0, &other.rows.0)
    }
}

impl StudioSession {
    /// The song that opened as `project` in the engine — current or not —
    /// read back as the studio lays it out, by the project rather than
    /// whichever is current, so a song opening behind the one on screen
    /// does not move it: how a page reads every song it opens, and a
    /// window the songs of a streamed set that arrive after the first.
    /// `rpp_text` is the project's text (for its track kinds), `chart_text`
    /// its chart's; `waves` also reads its audio items' peaks from the
    /// engine (a page is handed them instead).
    ///
    /// # Errors
    ///
    /// The facade is not up, or the project is not in the engine or could
    /// not be read.
    pub async fn read_project(
        project: &str,
        name: &str,
        rpp_text: &str,
        chart_text: Option<&str>,
        waves: bool,
    ) -> eyre::Result<Self> {
        const SCENE: &str = "drum-mixing";
        // Say which step fails: `fetch_of` answers only yes or no.
        let facade =
            daw_control::Daw::try_get().ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
        let handle = facade
            .project(project.to_owned())
            .await
            .map_err(|e| eyre::eyre!("{name} is not in the engine: {e}"))?;
        let raw = daw_ui::studio::project::fetch_of(handle.clone())
            .await
            .ok_or_else(|| eyre::eyre!("could not read {name} back"))?;
        // Its automation lanes: from this process's engine, or through
        // the facade from wherever the engine is.
        #[cfg(feature = "native")]
        let local = crate::open::local_engine().is_some();
        #[cfg(not(feature = "native"))]
        let local = false;
        if local {
            #[cfg(feature = "native")]
            crate::automation::refresh_in(project, &raw.tracks);
        } else {
            let (lanes, takes) = crate::automation::read_remote(&raw.tracks).await;
            crate::automation::adopt_remote(lanes, takes);
        }
        let planner = Planner {
            raw: Arc::new(raw),
            scene: Some(SCENE),
            kinds: Arc::new(crate::plan::Kinds::from_text(rpp_text)),
        };
        let (studio, rows) = planner.plan(&planner.raw);
        let previews = crate::midi::Previews::default();
        let items = || studio.0.items.values().flatten();
        previews
            .fill_in(
                &handle,
                items()
                    .filter(|item| studio.0.is_midi(&item.guid))
                    .map(|item| (item.guid.clone(), item.length.as_seconds()))
                    .collect(),
            )
            .await;
        if waves {
            previews
                .fill_waves_in(
                    &handle,
                    items()
                        .filter(|item| !studio.0.is_midi(&item.guid))
                        .map(|item| item.guid.clone())
                        .collect(),
                )
                .await;
        }
        let chart = chart_text
            .and_then(|text| {
                keyflow::parse(text)
                    .inspect_err(|e| tracing::error!(error = %e, "chart: could not parse"))
                    .ok()
                    .map(Arc::new)
            })
            .or_else(|| chart_of_song(Some(project)));
        Ok(Self {
            project: studio,
            rows,
            previews,
            chart,
            chart_file: None,
            planner,
        })
    }
}

#[cfg(feature = "native")]
impl StudioSession {
    /// Open `path` on the process's engine with audio, run `prepare` on it
    /// (organize / chart / guide — see [`crate::prepare`]), and read it back
    /// the way the studio lays it out.
    ///
    /// # Errors
    ///
    /// The project could not be opened or read back. A prepare step that
    /// fails is logged and the session opens as it was.
    pub fn open(path: &std::path::Path, prepare: &crate::prepare::Prepare) -> eyre::Result<Self> {
        Self::open_as(path, prepare, true).map(|(session, _)| session)
    }

    /// [`Self::open`] for a song of a setlist: the first (`first`) stands
    /// the engine up and takes the audio; the rest load into the same
    /// engine beside it. Each is current while it is prepared and read —
    /// the guide and the read-back work on the current project — so the
    /// caller makes the song it wants current afterwards
    /// ([`crate::open::switch_to`]). Returns the song's project guid too.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: bool,
    ) -> eyre::Result<(Self, String)> {
        Self::open_first_as(
            path,
            prepare,
            first.then_some(true),
            crate::open::Media::All,
        )
    }

    /// [`Self::open_as`], where `first` is `Some(audio)` for the song that
    /// stands the engine up — with or without a device — and `None` for
    /// the rest; `media` is when its media loads.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_first_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: Option<bool>,
        media: crate::open::Media,
    ) -> eyre::Result<(Self, String)> {
        let began = std::time::Instant::now();
        let plan = crate::open::song_plan(path, prepare);
        let opened = if let Some(audio) = first {
            crate::open::open_first_with(&plan.open, audio, media)?
        } else {
            let opened = crate::open::open_another_with(&plan.open, media)?;
            crate::open::switch_to(&opened.daw, &opened.project_guid, false);
            opened
        };
        let engine = began.elapsed();
        crate::open::prepare_and_save(crate::folder::disk(), &opened, &plan, prepare);
        // The song as opened (and prepared): what "unsaved changes" is
        // measured from.
        #[cfg(feature = "native")]
        crate::projects::mark_saved(&opened.daw, &opened.project_guid);
        let prepared = began.elapsed();
        let session = Self::read_current(Some(plan.open.as_path()), prepare.chart.clone())?;
        // One line per song: where its opening went.
        tracing::info!(
            song.name = %opened.name,
            song.tracks = session.project.0.tracks.len(),
            open.engine_ms = engine.as_millis(),
            open.prepare_ms = (prepared - engine).as_millis(),
            open.read_ms = (began.elapsed() - prepared).as_millis(),
            open.total_ms = began.elapsed().as_millis(),
            "song opened"
        );
        Ok((session, opened.project_guid))
    }

    /// The facade's current project, read the way the studio lays it out —
    /// whatever backend the facade points at. `path` is where the project
    /// sits on this machine, when it does (the track kinds the template
    /// wrote are read from it); `chart` the `.kf` to show beside it.
    ///
    /// What a Remote window's songs are made of: REAPER's project, read
    /// back rather than opened (see `crate::setlist::Setlist::attach`).
    ///
    /// # Errors
    ///
    /// The project could not be read back.
    pub fn read_current(
        path: Option<&std::path::Path>,
        chart: Option<std::path::PathBuf>,
    ) -> eyre::Result<Self> {
        const SCENE: &str = "drum-mixing";
        let raw = fetch().ok_or_else(|| {
            eyre::eyre!(
                "could not read {} back",
                path.map_or_else(
                    || "the current project".to_owned(),
                    |p| p.display().to_string()
                )
            )
        })?;
        let planner = Planner {
            raw: Arc::new(raw),
            scene: Some(SCENE),
            kinds: Arc::new({
                let engine = crate::open::local_engine()
                    .zip(crate::open::current_song())
                    .map(|(daw, song)| crate::plan::Kinds::from_engine(daw, &song))
                    .unwrap_or_default();
                if engine.is_empty() {
                    path.filter(|p| p.exists())
                        .and_then(|p| crate::open::project_text(p).ok())
                        .map(|read| crate::plan::Kinds::from_text(&read.text))
                        .unwrap_or_default()
                } else {
                    engine
                }
            }),
        };
        let planning = std::time::Instant::now();
        let (project, rows) = planner.plan(&planner.raw);
        let planned = planning.elapsed();
        let previews = previews_of(&project);
        tracing::info!(
            read.plan_ms = planned.as_millis(),
            read.previews_ms = (planning.elapsed() - planned).as_millis(),
            "song laid out"
        );
        let parsed = chart.as_deref().and_then(|chart_path| {
            let text = std::fs::read_to_string(chart_path)
                .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not read"))
                .ok()?;
            keyflow::parse(text.as_str())
                .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not parse"))
                .ok()
                .map(Arc::new)
        })
        .or_else(|| chart_of_song(None));
        Ok(Self {
            project,
            rows,
            previews,
            chart: parsed,
            chart_file: chart,
            planner,
        })
    }
}

/// The open project, as the studio's own refs: every visible row, laid out
/// by `scene` (the visual track manager's scene slug) when one is given.
#[cfg(feature = "native")]
#[must_use]
pub fn read_back(
    scene: Option<&str>,
    project_path: &std::path::Path,
) -> Option<(ProjectRef, RowsRef)> {
    Some(plan_rows(&fetch()?, scene, project_path))
}

/// Asked for when something changed the session in the ENGINE rather
/// than through the arrangement — a toolbar action, an edited chart laid
/// over the song — so the arrangement reads it back on its next frame.
static RESYNC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask the arrangement to read the session back from the engine.
pub fn request_resync() {
    RESYNC.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a read-back is asked for; asking clears it.
#[must_use]
pub fn take_resync() -> bool {
    RESYNC.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Whether one is waiting, without taking it — what a panel's
/// `needs_redraw` asks.
#[must_use]
pub fn resync_pending() -> bool {
    RESYNC.load(std::sync::atomic::Ordering::Relaxed)
}

/// The open project, as the engine has it.
#[cfg(feature = "native")]
#[must_use]
pub fn fetch() -> Option<daw_ui::studio::project::Project> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let project = rt.block_on(daw_ui::studio::project::fetch())?;
    if crate::open::local_engine().is_some() {
        crate::automation::refresh(&project.tracks);
    } else {
        // The engine is elsewhere: its lanes through the facade.
        let (lanes, takes) = rt.block_on(crate::automation::read_remote(&project.tracks));
        crate::automation::adopt_remote(lanes, takes);
    }
    Some(project)
}

/// No engine in a page's process to write a chart from.
#[cfg(not(feature = "native"))]
const fn chart_of_song(_project: Option<&str>) -> Option<Arc<keyflow::Chart>> {
    None
}

/// A chart for the song `project` (the current one when `None`) written
/// from its sections ([`crate::song_chart`]), when it is in this
/// process's engine and has sections: what the chart view shows for a
/// song with no chart of its own.
#[cfg(feature = "native")]
fn chart_of_song(project: Option<&str>) -> Option<Arc<keyflow::Chart>> {
    use daw::service::Projects as _;
    let daw = crate::open::local_engine()?;
    let guid = match project {
        Some(guid) => guid.to_owned(),
        None => crate::open::current_song().or_else(|| daw.current().map(|p| p.guid))?,
    };
    let text = crate::song_chart::of_song(daw, &daw::service::ProjectContext::Project(guid))?;
    let chart = keyflow::parse(text.as_str())
        .inspect_err(|e| tracing::warn!(error = %e, "chart: the song's sections did not make one"))
        .ok()?;
    tracing::info!(
        chart.sections = chart.sections.len(),
        chart.lines = text.lines().count(),
        "chart: written from the song's sections"
    );
    Some(Arc::new(chart))
}

/// The rows of `raw`, laid out by `scene`: the visible tracks (a hidden
/// folder hides what it holds), planned and folded the way the studio
/// shows them.
#[must_use]
pub fn plan_rows(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    project_path: &std::path::Path,
) -> (ProjectRef, RowsRef) {
    plan_rows_with(raw, scene, &crate::plan::Kinds::current(project_path))
}

/// [`plan_rows`], with the track kinds already read.
#[must_use]
pub fn plan_rows_with(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    kinds: &crate::plan::Kinds,
) -> (ProjectRef, RowsRef) {
    let project = ProjectRef(Arc::new(raw.clone()));
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&project.tracks);
    let mut planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    // The session file carries no per-track height; the heights are a
    // layout the scene decides.
    let settings = crate::settings::Settings {
        folded_takes: std::env::var("FTS_BLITZ_FOLDED_TAKES").as_deref() != Ok("0"),
        ..crate::settings::Settings::default()
    };
    if let Some(slug) = scene.and_then(dynamic_template::scenes::scene) {
        planned = crate::plan::apply_scene(
            &planned,
            kinds,
            slug,
            crate::plan::Panel {
                surface: crate::plan::Surface::Arrange,
                mode: None,
                settings,
                extent: 1440.0,
                active_language: None,
            },
        );
    }
    // And the folders the window shut (`crate::folds`), over the scene's.
    planned = crate::folds::apply(planned);
    // A folder the scene shut keeps its row and gets its children's items,
    // folded: the mix happens on the folder, so its name, fader and colour
    // stay, and so does what is in it.
    let mut project = project;
    {
        let shown: Vec<daw_proto::Track> = planned.iter().map(|(t, _)| t.clone()).collect();
        let folded = Arc::make_mut(&mut project.0);
        // A shut folder whose `+` is open shows every lane under it, lane
        // by lane (`crate::lanes::folder_view`).
        daw_ui::studio::folded::refold_lanes(
            folded,
            &shown,
            settings.folded_takes,
            &|tracks, folder| crate::lanes::folder_view(tracks, folder).is_some(),
        );
        if !crate::folds::cuts_shown() {
            daw_ui::studio::folded::join_seams(folded);
        }
    }
    (project, RowsRef(Arc::new(planned)))
}

/// The strips of `raw` laid out by `scene` for the mixer — see
/// [`Planner::plan_strips`].
#[must_use]
pub fn plan_strips_with(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    kinds: &crate::plan::Kinds,
) -> Vec<(daw_proto::Track, u32)> {
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&raw.tracks);
    let planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    let Some(slug) = scene.and_then(dynamic_template::scenes::scene) else {
        return crate::folds::apply(planned);
    };
    let strips = crate::plan::apply_scene(
        &planned,
        kinds,
        slug,
        crate::plan::Panel {
            surface: crate::plan::Surface::Mixer,
            mode: None,
            settings: crate::settings::Settings::default(),
            extent: 1440.0,
            active_language: None,
        },
    );
    // The track as it was, not as the scene sized and selected it.
    let as_it_was: std::collections::HashMap<&str, &daw_proto::Track> = planned
        .iter()
        .map(|(track, _)| (track.guid.as_str(), track))
        .collect();
    crate::folds::apply(
        strips
            .into_iter()
            .map(|(track, depth)| {
                let track = as_it_was
                    .get(track.guid.as_str())
                    .map_or(track, |was| (*was).clone());
                (track, depth)
            })
            .collect(),
    )
}

/// Every MIDI item's notes, read before anything is drawn — an item drawn
/// from a waveform it does not have is why a chord track once looked like
/// a shaker.
#[must_use]
pub fn previews_of(project: &ProjectRef) -> crate::midi::Previews {
    let previews = crate::midi::Previews::default();
    previews.fill_blocking(
        project
            .0
            .items
            .values()
            .flatten()
            .filter(|item| project.0.is_midi(&item.guid))
            .map(|item| (item.guid.clone(), item.length.as_seconds()))
            .collect(),
    );
    let waves: Vec<String> = project
        .0
        .items
        .values()
        .flatten()
        .filter(|item| !project.0.is_midi(&item.guid))
        .map(|item| item.guid.clone())
        .collect();
    // The waveforms behind the window, each item drawn as it arrives (and
    // the bottom bar showing how far): reading them first held the window
    // still while a song of thousands of items opened.
    #[cfg(feature = "native")]
    {
        let reading = previews.clone();
        crate::wave_pool::spawn(move || reading.fill_waves_blocking(waves));
    }
    #[cfg(not(feature = "native"))]
    previews.fill_waves_blocking(waves);
    previews
}

/// A window position as winit gives it (device pixels from the window's
/// corner) in CSS pixels from the page's: the space the panels' rects
/// (`get_client_rect`) and Blitz's own events are in. The safe area comes
/// off as Blitz takes it off its own — except on a Mac, where Blitz draws
/// under the title bar and takes nothing off.
///
/// Without it a 2x screen put the pointer twice as far along as the panel
/// thought it was, and an iPad's status bar further down still.
#[cfg(feature = "native")]
#[must_use]
pub fn css_point(window: &dyn winit::window::Window, x: f64, y: f64) -> (f64, f64) {
    let scale = window.scale_factor().max(f64::EPSILON);
    #[cfg(not(target_os = "macos"))]
    let (left, top) = {
        let insets = window.safe_area();
        (f64::from(insets.left), f64::from(insets.top))
    };
    #[cfg(target_os = "macos")]
    let (left, top) = (0.0, 0.0);
    ((x - left) / scale, (y - top) / scale)
}

/// A wheel's pixel delta, which winit gives in device pixels, in CSS
/// pixels.
#[cfg(feature = "native")]
#[must_use]
pub fn css_delta(window: &dyn winit::window::Window, dx: f64, dy: f64) -> (f64, f64) {
    let scale = window.scale_factor().max(f64::EPSILON);
    (dx / scale, dy / scale)
}

/// The arrangement panel, on dioxus-native: the widget as a Blitz custom
/// widget, winit's window events translated into [`crate::panel`]'s.
///
/// No props: the session comes from context ([`StudioSession`]), so the
/// same panel renders in any tile of any window.
#[cfg(feature = "native")]
#[component]
pub fn Arrangement() -> Element {
    use crate::panel::{Button, PanelEvent, WHEEL_LINE};
    let window = dioxus_native::use_window();
    let win = window.clone();
    // The pointer's shapes, the painted glyphs among them (`window_cursor`).
    let sink: crate::tool::Sink = crate::window_cursor::use_cursor_sink();
    let applier = use_hook(|| Rc::new(crate::engine::Applier::start()));
    let engine: crate::panel::Engine = {
        let applier = Rc::clone(&applier);
        Rc::new(move |edit| match applier.as_ref() {
            Some(applier) => applier.send(edit),
            None => tracing::debug!(?edit, "no engine to carry out the edit"),
        })
    };
    let (panel, widget) = crate::panel::use_arrangement_panel(
        Some(sink),
        engine,
        dioxus_native_dom::CustomWidgetAttr::new,
    );
    let transport = crate::engine::Transport::shared();
    let events = panel.clone();
    dioxus_native::use_window_event(move |event, _| {
        let panel = &events;
        match event {
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                // By the physical key, not the character: with Shift held
                // the character is "Z", and Shift+z is a zoom of its own.
                if event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                {
                    panel.handle(PanelEvent::ZKey {
                        pressed: event.state.is_pressed(),
                        repeat: event.repeat,
                    });
                }
            }
            winit::event::WindowEvent::ModifiersChanged(state) => {
                let keys = state.state();
                panel.handle(PanelEvent::Modifiers(crate::mousemap::Mods {
                    shift: keys.shift_key(),
                    ctrl: keys.control_key(),
                    alt: keys.alt_key(),
                }));
            }
            winit::event::WindowEvent::PointerMoved { position, .. } => {
                let (x, y) = css_point(&*win, position.x, position.y);
                panel.handle(PanelEvent::Pointer { x, y });
            }
            winit::event::WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                // Where it is pressed, first: a finger has not moved there
                // beforehand, so the last position is where the previous
                // one lifted.
                let (x, y) = css_point(&*win, position.x, position.y);
                panel.handle(PanelEvent::Pointer { x, y });
                let button = match button {
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Middle) => {
                        Button::Middle
                    }
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Left) => {
                        Button::Left
                    }
                    winit::event::ButtonSource::Mouse(_) => Button::Other,
                    _ => Button::Left,
                };
                panel.handle(PanelEvent::Button {
                    button,
                    pressed: state.is_pressed(),
                });
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (f64::from(*x) * WHEEL_LINE, f64::from(*y) * WHEEL_LINE)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(at) => css_delta(&*win, at.x, at.y),
                };
                panel.handle(PanelEvent::Wheel { dx, dy });
            }
            // A pinch, on a touchscreen (see `touch::recognize_gestures`)
            // or a trackpad.
            winit::event::WindowEvent::PinchGesture { delta, .. } => {
                panel.handle(PanelEvent::Pinch { by: *delta });
            }
            winit::event::WindowEvent::RedrawRequested => {
                let play_at = transport.map_or(0.0, |t| t.read().0);
                panel.handle(PanelEvent::Frame { play_at });
            }
            _ => {}
        }
    });

    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    let mounted = Rc::clone(&panel.mounted);
    let focus_node = Rc::clone(&panel.focus_node);
    rsx! {
        div {
            // Filling the positioned tile it is in. Absolute rather than
            // `height:100%`: a percentage of a flex item's height does not
            // resolve in Blitz, and the panel came out zero tall.
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                // `<object>` is not in Blitz's default-focusable list, and
                // the arrangement takes the keyboard (a rename is a field
                // inside it).
                tabindex: "0",
                data: widget.clone(),
                // The keyboard is the arrangement's from the start: with
                // nothing focused, every shortcut waited for a first click.
                onmounted: move |event| {
                    let node = event.data();
                    *focus_node.borrow_mut() = Some(Rc::clone(&node));
                    async move {
                        let _ = node.set_focus(true).await;
                    }
                },
            }
            crate::panel::PanelChrome { panel: panel.clone() }
        }
    }
}

/// One scrollbar: a track, and a thumb saying where in the session the
/// view is and how much of it is on screen. Dragged: the thumb IS the view.
#[component]
pub fn ScrollBar(
    across: bool,
    at: f64,
    travel: f64,
    window: f64,
    left: f64,
    top: f64,
    length: f64,
    /// How thick, in CSS pixels: the lanes' [`BAR`] as drawn, which touch
    /// mode's zoom makes a finger's width.
    #[props(default = BAR)]
    thick: f64,
    colors: daw_ui::studio::lanes::Colors,
    on_move: EventHandler<f64>,
) -> Element {
    let whole = (travel + window).max(1.0);
    // Never smaller than a thumb you can hit, however long the session.
    let thumb = (window / whole * length).max(24.0);
    let along = (at / travel.max(1.0)) * (length - thumb);
    let mut held = use_signal(|| Option::<f64>::None);
    let (size, place) = if across {
        (
            format!("left:{left}px; top:{top}px; width:{length}px; height:{thick}px;"),
            format!(
                "left:{along:.1}px; top:1px; width:{thumb:.1}px; height:{}px;",
                thick - 2.0
            ),
        )
    } else {
        (
            format!("left:{left}px; top:{top}px; width:{thick}px; height:{length}px;"),
            format!(
                "left:1px; top:{along:.1}px; width:{}px; height:{thumb:.1}px;",
                thick - 2.0
            ),
        )
    };
    rsx! {
        div {
            style: "position:absolute; {size} background:{colors.tcp_column};",
            // Pointer events rather than mouse ones: Blitz gives a finger
            // pointer and touch events, never mouse ones.
            onpointerdown: move |event| {
                let at = event.data().element_coordinates();
                held.set(Some(if across { at.x } else { at.y }));
            },
            onpointerup: move |_| held.set(None),
            onpointerleave: move |_| held.set(None),
            onpointermove: move |event| {
                if held().is_none() {
                    return;
                }
                let at = event.data().element_coordinates();
                let along = if across { at.x } else { at.y };
                let room = (length - thumb).max(1.0);
                let fraction = ((along - thumb / 2.0) / room).clamp(0.0, 1.0);
                on_move.call(fraction * travel.max(0.0));
            },
            div {
                style: "position:absolute; {place} background:{colors.text_dim}; border-radius:2px;",
            }
        }
    }
}

#[cfg(all(test, feature = "native"))]
mod source_tests {
    // How a song file opens: [`crate::open::SongPlan`].
    use crate::prepare::Prepare;

    fn prepared() -> Prepare {
        Prepare {
            organize: true,
            chart: None,
            guide: true,
        }
    }

    fn song() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "session-source-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a folder");
        let rpp = dir.join("Song.RPP");
        std::fs::write(&rpp, "<REAPER_PROJECT\n>\n").expect("an rpp");
        (dir, rpp)
    }

    /// The first open prepares the multitrack and saves it beside itself.
    #[test]
    fn an_unprepared_song_is_prepared_once_and_saved_beside_it() {
        let (dir, rpp) = song();
        let _ = std::fs::remove_dir_all(dir.join("Song.session"));
        assert_eq!(
            crate::open::song_plan_with(crate::folder::disk(), &rpp, &prepared(), false),
            crate::open::SongPlan {
                open: rpp.clone(),
                prepare: true,
                save_to: Some(dir.join("Song.session")),
            }
        );
        // Nothing to prepare is nothing to save.
        assert_eq!(
            crate::open::song_plan_with(crate::folder::disk(), &rpp, &Prepare::default(), false),
            crate::open::SongPlan {
                open: rpp,
                prepare: false,
                save_to: None,
            }
        );
    }

    /// Once saved, the session is what opens — asked for by its `.RPP` or
    /// by itself — and it is not prepared again unless asked.
    #[test]
    fn a_saved_session_opens_instead_of_its_multitrack() {
        let (dir, rpp) = song();
        // A project file: one file, as a save writes it.
        let saved = dir.join("Song.session");
        std::fs::write(&saved, b"").expect("a project file");
        let as_saved = crate::open::SongPlan {
            open: saved.clone(),
            prepare: false,
            save_to: None,
        };
        assert_eq!(
            crate::open::song_plan_with(crate::folder::disk(), &rpp, &prepared(), false),
            as_saved
        );
        assert_eq!(
            crate::open::song_plan_with(crate::folder::disk(), &saved, &prepared(), false),
            as_saved
        );
        // Asked to, the multitrack is prepared afresh over it.
        assert_eq!(
            crate::open::song_plan_with(crate::folder::disk(), &rpp, &prepared(), true),
            crate::open::SongPlan {
                open: rpp,
                prepare: true,
                save_to: Some(saved),
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod mixer_fold_tests {
    /// Mix mode's mixer is folded as the window is: a session opens with
    /// its top-level folders shut there too, and one opened opens there.
    #[test]
    fn the_mix_mode_mixer_follows_the_folds() {
        let track = |guid: &str, folder: bool, depth: i32, parent: Option<&str>| daw_proto::Track {
            guid: guid.to_owned(),
            name: guid.to_owned(),
            is_folder: folder,
            folder_depth: depth,
            visible_in_tcp: true,
            visible_in_mixer: true,
            parent_guid: parent.map(str::to_owned),
            ..daw_proto::Track::default()
        };
        let raw = daw_ui::studio::project::Project {
            tracks: vec![
                track("Bass", true, 1, None),
                track("Bass DI", false, -1, Some("Bass")),
            ],
            ..daw_ui::studio::project::Project::default()
        };
        let planner = super::Planner {
            raw: std::sync::Arc::new(raw.clone()),
            scene: None,
            kinds: std::sync::Arc::new(crate::plan::Kinds::default()),
        };
        let strips = |planner: &super::Planner| -> Vec<String> {
            planner
                .plan_mixer(&raw, None, crate::settings::Settings::default(), 800.0)
                .as_slice()
                .iter()
                .map(|(t, _)| t.guid.clone())
                .collect()
        };
        assert_eq!(strips(&planner), ["Bass"], "shut as it opens");
        crate::folds::toggle("Bass", false);
        assert_eq!(strips(&planner), ["Bass", "Bass DI"], "opened");
    }
}
