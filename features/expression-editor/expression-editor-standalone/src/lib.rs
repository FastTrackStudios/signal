//! The expression editor, standalone.
//!
//! One `cargo run --example editor -- <thing>` puts the real editor on
//! screen against a project, a `.mid` file, or a built-in demo scene —
//! no REAPER, no plugin host, no arrangement view, no mixer. The editor
//! is the whole window.
//!
//! ## Why this crate exists
//!
//! Until now the editor ran in exactly one place: a REAPER dockable
//! panel. `spec/reaper-testing.md` records that headless REAPER cannot
//! open a Dioxus panel at all — it aborts inside GDK and takes the daw
//! socket with it — so seeing a change meant a private Xvfb, a window
//! manager, and a REAPER launch. That is the wrong loop for building
//! nine surfaces.
//!
//! ## What is in the library and what is in the examples
//!
//! The library is the *loading*: turn a command line into an
//! [`Editor`], through the same `daw` facade the REAPER module uses, so
//! the standalone path and the in-REAPER path cannot diverge. It has no
//! window in it, which is what keeps it testable — [`Runner::open`]
//! runs headless in an ordinary `cargo test`.
//!
//! The window lives in `examples/editor.rs` and the PNG rasterizer in
//! `examples/shot.rs`, both mounting [`App`]. Blitz and vello are
//! dev-dependencies for exactly that reason: a consumer that wants the
//! loading should not link a GPU stack.
//!
//! ## The window rule
//!
//! Windows go through `dioxus_native::launch_cfg` — Blitz → Vello →
//! winit, dioxus's own desktop path. Not `dioxus::desktop`, which is
//! WebKit/WRY and a completely different rendering engine.
//!
//! And not `nice-plug-dioxus`, which is what this used to open: that
//! opens a *plugin editor* window through baseview, and the expression
//! editor is an application, not a VST3/CLAP plugin. Carrying a plugin
//! framework to open an app window cost a second windowing path, a
//! second renderer to keep in parity, and a `native` feature on the UI
//! crate that existed only to pull it in.
//!
//! `launch_cfg` takes a bare `fn() -> Element`, which is why the loaded
//! document reaches the component through [`stage`]/[`App`] rather than
//! through props.

use std::path::{Path, PathBuf};

use daw::service::midi::MidiTakeLocation;
use daw::service::{ItemRef, Items, ProjectContext, TakeRef, Takes, Tracks};
use daw::standalone::Standalone;
use daw::standalone::project_loader::load_rpp_text;
use expression_editor_audio::{AudioSession, TakeConfig};
use expression_editor_core::{Editor, Mode, Viewport};
use expression_editor_daw::Session;
use expression_editor_ui::demo::{self, Scene};
use expression_editor_ui::workflow::Workflow;

pub mod app;
pub mod cli;
pub mod drum_host;
pub mod library;
pub mod practice;
pub mod probe;
/// Where the window's size comes from on each renderer.
pub mod window_size;
pub mod workstation;

pub use app::{App, stage};
pub use cli::Args;

/// Default MPE bend range, matching the REAPER module. Must match the
/// receiving instrument or every pitch curve reads wrong by a factor;
/// 48 is the MPE convention.
pub const DEFAULT_BEND_RANGE: f64 = 48.0;

/// What the runner was pointed at.
///
/// Dispatch is on the file extension rather than on a flag, because the
/// thing a user has in hand is a path and asking them to also classify
/// it is a question the program can answer itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A built-in scene from [`expression_editor_ui::demo`] — the same
    /// documents the screenshot tests mount, so a scene shown here
    /// cannot drift from the committed pictures.
    Scene(Scene),
    /// A job rather than a fixture — the list a person is handed. See
    /// `expression_editor_ui::workflow`.
    Workflow(Workflow),
    /// A REAPER project. Loaded through `daw-standalone`, so items,
    /// takes, tempo and markers all arrive as facade types.
    Rpp(PathBuf),
    /// A standard MIDI file, read through the facade's own reader.
    Midi(PathBuf),
    /// A Guitar Pro transcription — gp3/4/5, gpx or gp.
    ///
    /// Scenario 4 of #149 is "a Guitar Pro file imported and displayed
    /// as a six-string roll with bend flow", and until this variant the
    /// only way to see one was a test. The runner is the demo
    /// application, so it has to be able to open the material the
    /// scenarios are written about.
    GuitarPro(PathBuf),
}

impl Source {
    /// Classify a command-line argument.
    ///
    /// A scene name wins over a path, and nothing on disk is touched to
    /// decide — an argument that is not a known scene and has no
    /// recognised extension is an error here rather than a confusing
    /// failure three layers down.
    pub fn parse(arg: &str) -> Result<Self, LoadError> {
        // Workflows first: they are the list a person is handed, and a
        // name collision should resolve to the job rather than to the
        // fixture that happens to share its material.
        if let Some(w) = Workflow::by_slug(arg) {
            return Ok(Source::Workflow(w));
        }
        if let Some(scene) = scene_by_name(arg) {
            return Ok(Source::Scene(scene));
        }
        let path = PathBuf::from(arg);
        match extension(&path).as_deref() {
            Some("rpp") => Ok(Source::Rpp(path)),
            Some("mid") | Some("midi") => Ok(Source::Midi(path)),
            Some("gp") | Some("gpx") | Some("gp3") | Some("gp4") | Some("gp5") => {
                Ok(Source::GuitarPro(path))
            }
            // An audio file on its own is deliberately not accepted:
            // analysing a take needs its length, and a bare file gives
            // no item to read one from. Reading past the end of a
            // source yields silence rather than a short read, so a
            // guessed length would analyse minutes of silence and open
            // on a document that is mostly empty. Put the file in a
            // project and point at that.
            Some("wav") | Some("flac") | Some("aif") | Some("aiff") | Some("ogg") | Some("mp3") => {
                Err(LoadError::BareAudio(path))
            }
            _ => Err(LoadError::Unrecognised(arg.to_string())),
        }
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// Find a demo scene by slug (`26-guitar`), by bare name (`guitar`), or
/// by its label (`Guitar riff`), case-insensitively.
pub fn scene_by_name(name: &str) -> Option<Scene> {
    let want = name.trim().to_ascii_lowercase();
    Scene::ALL.into_iter().find(|s| {
        let slug = s.slug();
        // Slugs are numbered for screenshot ordering; the number is
        // there for `ls`, not for a person typing an argument.
        let bare = slug.split_once('-').map(|(_, r)| r).unwrap_or(slug);
        slug == want || bare == want || s.label().to_ascii_lowercase() == want
    })
}

/// Every scene name the runner accepts, for a `--list` and for error
/// messages that would otherwise leave the user guessing.
pub fn scene_names() -> Vec<(&'static str, &'static str)> {
    Scene::ALL
        .into_iter()
        .map(|s| {
            let slug = s.slug();
            let bare = slug.split_once('-').map(|(_, r)| r).unwrap_or(slug);
            (bare, s.label())
        })
        .collect()
}

/// Why a source could not be opened.
///
/// Every variant names the thing that was wrong and, where there is
/// one, the next thing to try — an editor that opens on nothing looks
/// like a bug in the editor.
#[derive(Debug)]
pub enum LoadError {
    Unrecognised(String),
    BareAudio(PathBuf),
    Read(PathBuf, String),
    Rpp(String),
    /// The project loaded but held nothing this editor can open.
    NoEditableItem {
        project: String,
        items: usize,
    },
    /// A track or item was asked for by name or index and is not there.
    NoSuchTarget(String),
    /// `--drums` was asked for and no folder track qualifies as the kit.
    NoKitFolder {
        project: String,
        wanted: Option<String>,
    },
    /// A `.mid` the facade's reader declined.
    MidiFile(PathBuf),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unrecognised(arg) => write!(
                f,
                "don't know what to do with {arg:?} — expected a .rpp, a .mid, \
                 or a demo scene name (try --list)"
            ),
            LoadError::BareAudio(p) => write!(
                f,
                "{} is a bare audio file; the analyser needs the item length \
                 that only a project carries. Put it on a track in a .rpp and \
                 open that instead.",
                p.display()
            ),
            LoadError::Read(p, e) => write!(f, "reading {}: {e}", p.display()),
            LoadError::Rpp(e) => write!(f, "loading project: {e}"),
            LoadError::NoEditableItem { project, items } => write!(
                f,
                "{project} has {items} item(s) but none the editor can open — \
                 a MIDI take, or an audio take whose source file resolves"
            ),
            LoadError::NoSuchTarget(what) => write!(f, "no such {what}"),
            LoadError::NoKitFolder { project, wanted } => match wanted {
                Some(name) => write!(f, "{project} has no folder track named {name:?}"),
                None => write!(
                    f,
                    "{project} has no folder track named like a kit \
                     (Drums, Kit) — name one with --drums <folder>"
                ),
            },
            LoadError::MidiFile(p) => write!(f, "{} is not a readable MIDI file", p.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// Which item in a project to open, when it is not the first editable
/// one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    /// Track by name (case-insensitive substring) or by 1-based index.
    pub track: Option<String>,
    /// Item index within the chosen track — or within the project, when
    /// no track was named. Zero-based, because it addresses a list
    /// rather than naming a thing a user sees numbered.
    pub item: Option<usize>,
    /// `Some` opens a `.rpp` as a drum workspace instead of a single
    /// item: every track under the kit folder becomes a
    /// [`Mode::UnpitchedAudio`] track, folded into role lanes. The inner
    /// value names the kit folder; `Some(None)` means "find the first
    /// folder named like a kit".
    ///
    /// On `Target` rather than a parameter because it narrows *what in
    /// the project to open*, which is exactly what a target is.
    pub drums: Option<Option<String>>,
}

/// A loaded document and everything that has to stay alive behind it.
///
/// The backend is held for the session's lifetime rather than dropped
/// after the load: a session's whole point is that "open this take" and
/// "write it back" mean the same take, and the location it remembers is
/// only meaningful against the backend it came from.
pub struct Runner {
    /// `None` for a demo scene, which has no project behind it.
    pub daw: Option<Standalone>,
    pub loaded: Loaded,
    /// What to show in the window title: enough to tell two runs apart.
    pub label: String,
    /// The drum workspace's write half — `Some` only for
    /// [`Loaded::DrumWorkspace`], where the panel's Apply and the slip
    /// drag land through it.
    pub host: Option<drum_host::SharedDrumHost>,
}

/// The document, and the shape of the round trip behind it.
pub enum Loaded {
    /// A demo document, with nothing to write back to. Boxed only to
    /// keep the variants a similar size; an `Editor` is a kilobyte.
    Scene(Box<Editor>),
    /// A MIDI take. The take *is* the document.
    Midi(Box<Session>),
    /// An analysed audio take.
    Audio(Box<AudioSession>),
    /// A whole kit folder loaded as one workspace: one track per mic,
    /// folded into role lanes and shown stacked.
    ///
    /// Its own variant rather than `Scene` because a scene's contract is
    /// "nothing to write back to" — a drum workspace has a live project
    /// behind it, and a later write-back path must be able to tell them
    /// apart without re-deriving it from `daw.is_some()`.
    DrumWorkspace(Box<Editor>),
}

/// What a runner holds, without printing a whole document to say it.
///
/// Hand-written because neither `Standalone` nor a session is `Debug`,
/// and because the useful facts about a load are three: what it is
/// called, what kind of take is behind it, and whether anything came
/// through.
impl std::fmt::Debug for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("label", &self.label)
            .field("kind", &self.loaded.kind())
            .field("notes", &self.loaded.editor().doc.notes.len())
            .finish()
    }
}

impl Loaded {
    /// One word for the round trip behind this document.
    pub fn kind(&self) -> &'static str {
        match self {
            Loaded::Scene(_) => "scene",
            Loaded::Midi(_) => "midi",
            Loaded::Audio(_) => "audio",
            Loaded::DrumWorkspace(_) => "drums",
        }
    }

    pub fn editor(&self) -> &Editor {
        match self {
            Loaded::Scene(e) => e,
            Loaded::Midi(s) => &s.editor,
            Loaded::Audio(s) => &s.editor,
            Loaded::DrumWorkspace(e) => e,
        }
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        match self {
            Loaded::Scene(e) => e,
            Loaded::Midi(s) => &mut s.editor,
            Loaded::Audio(s) => &mut s.editor,
            Loaded::DrumWorkspace(e) => e,
        }
    }

    pub fn into_editor(self) -> Editor {
        match self {
            Loaded::Scene(e) => *e,
            Loaded::Midi(s) => s.editor,
            Loaded::Audio(s) => s.editor,
            Loaded::DrumWorkspace(e) => *e,
        }
    }
}

impl Source {
    /// The `.rpp` this source names, if it names one.
    ///
    /// For the two-phase open: everything else goes through
    /// [`Runner::open`], which is one phase because nothing else is slow
    /// enough to need two.
    pub fn rpp_path(&self) -> Option<&Path> {
        match self {
            Self::Rpp(path) => Some(path),
            _ => None,
        }
    }
}

/// A project that has been parsed and has a backend, before its kit has
/// been looked at. See [`Runner::open_rpp_project`].
pub struct OpenProject {
    pub daw: Standalone,
    pub ctx: ProjectContext,
    pub name: String,
    /// The `.rpp` it was parsed from — where detected hits are kept
    /// between opens.
    pub path: Option<std::path::PathBuf>,
}

impl Runner {
    /// Load a source into an editor.
    ///
    /// `mode` overrides whatever the source implies. A MIDI take
    /// carries no mode at all, and an audio take always analyses as
    /// [`Mode::PitchedAudio`]; both are starting points rather than
    /// facts, which is why the runner can say otherwise — a drum
    /// multitrack is an audio take that wants
    /// [`Mode::UnpitchedAudio`], and the mode screenshots need to reach
    /// all seven from the command line.
    pub fn open(
        source: &Source,
        target: &Target,
        viewport: Viewport,
        mode: Option<Mode>,
    ) -> Result<Self, LoadError> {
        let mut runner = match source {
            Source::Scene(scene) => Runner {
                daw: None,
                loaded: Loaded::Scene(Box::new(demo::editor(*scene, viewport))),
                label: format!("scene: {}", scene.label()),
                host: None,
            },
            Source::Workflow(w) => Runner {
                daw: None,
                loaded: Loaded::Scene(Box::new(w.editor(viewport))),
                label: w.label().to_string(),
                host: None,
            },
            Source::Rpp(path) => match &target.drums {
                Some(folder) => Self::open_rpp_drums(path, folder.as_deref(), viewport)?,
                None => Self::open_rpp(path, target, viewport)?,
            },
            Source::Midi(path) => Self::open_midi(path, target, viewport)?,
            Source::GuitarPro(path) => Self::open_guitar_pro(path, viewport)?,
        };
        if let Some(mode) = mode {
            runner.loaded.editor_mut().set_mode(mode);
        }
        Ok(runner)
    }

    /// A transcription, straight onto a document.
    ///
    /// No DAW: a `.gp` carries its own notes, tempo and tuning, and
    /// there is no audio to analyse or item to read a length from.
    fn open_guitar_pro(path: &Path, viewport: Viewport) -> Result<Self, LoadError> {
        let imported = expression_editor_guitarpro::import_file(&path.to_string_lossy())
            .map_err(|e| LoadError::Read(path.to_path_buf(), e))?;
        let mut editor = Editor::new(imported.doc, viewport);
        // The importer's tuning wins over the mode preset: a drop-D or
        // seven-string file must not be forced onto standard six.
        editor.set_mode(Mode::Guitar);
        let space = expression_editor_core::RowSpace::Strings(imported.tuning);
        editor.row_space = space.clone();
        editor.doc.row_space = space;
        editor.reset_view();
        let notes = editor.doc.notes.len();
        Ok(Runner {
            daw: None,
            loaded: Loaded::Scene(Box::new(editor)),
            label: format!(
                "{}: {} notes",
                path.file_name().unwrap_or_default().to_string_lossy(),
                notes,
            ),
            host: None,
        })
    }

    fn open_midi(path: &Path, target: &Target, viewport: Viewport) -> Result<Self, LoadError> {
        let daw = Standalone::new();
        // A file session still needs somewhere a later write would go.
        // Nothing writes yet, but seeding a project now means adding
        // the write does not change the load.
        let project_guid = seed_empty_project(&daw, path);
        let location = MidiTakeLocation::new(
            ProjectContext::Project(project_guid),
            ItemRef::Index(0),
            TakeRef::Active,
        );
        let text = path.to_string_lossy().to_string();
        let track = Self::midi_track_to_open(&daw, &text, target);
        let session =
            Session::from_file(&daw, &text, track, location, DEFAULT_BEND_RANGE, viewport)
                .ok_or_else(|| LoadError::MidiFile(path.to_path_buf()))?;
        Ok(Runner {
            label: format!(
                "{} — {} notes",
                file_label(path),
                session.editor.doc.notes.len()
            ),
            daw: Some(daw),
            loaded: Loaded::Midi(Box::new(session)),
            host: None,
        })
    }
    /// Which track of a standard MIDI file to open.
    ///
    /// `--track N` wins, as an index. Otherwise the first track that has any
    /// notes in it.
    ///
    /// Not simply track 0, which is what this used to do: an SMF **format 1**
    /// file — which is nearly every file anyone has — puts tempo, time
    /// signature and the sequence name in track 0 and the music in track 1
    /// onward. So opening a real `.mid` showed an empty roll saying "No
    /// notes" while the notes sat one track over.
    ///
    /// Falls back to 0 when nothing has notes, so an genuinely empty file
    /// still opens (empty) rather than refusing.
    fn midi_track_to_open<D: daw::service::midi::Midi>(
        daw: &D,
        path: &str,
        target: &Target,
    ) -> u32 {
        if let Some(want) = target.track.as_ref().and_then(|t| t.parse::<u32>().ok()) {
            return want;
        }
        // Bounded: a file with this many empty tracks is not one we are
        // going to find music in by looking further.
        const MAX_PROBE: u32 = 64;
        (0..MAX_PROBE)
            .find(|&t| {
                daw.read_midi_file(path.to_string(), t)
                    .is_some_and(|s| !s.notes.is_empty())
            })
            .unwrap_or(0)
    }

    fn open_rpp(path: &Path, target: &Target, viewport: Viewport) -> Result<Self, LoadError> {
        let (daw, name, summary) = open_project(path)?;
        let ctx = ProjectContext::Project(summary.project_guid.clone());

        let candidates = candidate_items(&daw, &ctx, target)?;
        let total = candidates.len();
        for cand in candidates {
            if let Some(runner) = Self::try_item(&daw, &ctx, &cand, viewport) {
                return Ok(Runner {
                    label: format!("{name} — {}", cand.describe()),
                    daw: Some(daw),
                    loaded: runner,
                    host: None,
                });
            }
        }
        Err(LoadError::NoEditableItem {
            project: name,
            items: total,
        })
    }

    /// Open a project as a drum workspace: every non-folder audio track
    /// under the kit folder becomes an [`Mode::UnpitchedAudio`] track —
    /// its longest playing audio item read through the accessor, gated
    /// into one note per transient — folded into role lanes and shown
    /// stacked.
    // r[impl drums.open.runner]
    fn open_rpp_drums(
        path: &Path,
        kit_folder: Option<&str>,
        viewport: Viewport,
    ) -> Result<Self, LoadError> {
        let opened = Self::open_rpp_project(path)?;
        let kit = Self::analyse_kit(&opened, kit_folder, viewport)?;
        Ok(Runner {
            label: kit.0,
            daw: Some(opened.daw),
            loaded: Loaded::DrumWorkspace(Box::new(kit.1)),
            host: Some(kit.2),
        })
    }

    /// Phase one: parse the project and stand up its backend.
    ///
    /// Split from the kit analysis so a window can open on the
    /// arrangement, mixer and transport while the drums are still
    /// decoding — which is 28.8 s of the 44 s it takes to open a real
    /// kit, and 28.8 s of staring at nothing if both are done before
    /// `launch`.
    pub fn open_rpp_project(path: &Path) -> Result<OpenProject, LoadError> {
        let (daw, name, summary) = open_project(path)?;
        Ok(OpenProject {
            ctx: ProjectContext::Project(summary.project_guid.clone()),
            daw,
            name,
            path: Some(path.to_path_buf()),
        })
    }

    /// Phase two: decode the kit's audio and detect its hits.
    ///
    /// The slow half. Everything it touches is `Send`, so a caller that
    /// has already opened its window can run this on a plain thread and
    /// hand the result over with
    /// [`crate::workstation::publish_kit`].
    pub fn analyse_kit(
        opened: &OpenProject,
        kit_folder: Option<&str>,
        viewport: Viewport,
    ) -> Result<(String, Editor, std::sync::Arc<crate::drum_host::DrumHost>), LoadError> {
        let built = drum_workspace(
            &opened.daw,
            opened.ctx.clone(),
            &opened.name,
            kit_folder,
            viewport,
            opened.path.as_deref(),
        )?;
        Ok((built.label, built.editor, std::sync::Arc::new(built.host)))
    }

    /// Try one item, audio first.
    ///
    /// The order matters only because a take is one or the other:
    /// asking the audio path first means an audio item never falls
    /// through to a MIDI reader that would find no notes and report an
    /// empty take. It mirrors the REAPER module exactly, which is the
    /// point — the two hosts must not decide this differently.
    fn try_item(
        daw: &Standalone,
        ctx: &ProjectContext,
        cand: &Candidate,
        viewport: Viewport,
    ) -> Option<Loaded> {
        let location = expression_editor_audio::AudioTakeLocation {
            project: ctx.clone(),
            item: ItemRef::Guid(cand.item_guid.clone()),
            take: TakeRef::Active,
        };
        if cand.kind == TakeKind::Audio
            && let Some(s) = AudioSession::load(
                daw,
                location,
                cand.length_secs,
                cand.volume,
                viewport,
                TakeConfig::default(),
            )
        {
            tracing::info!(
                item = %cand.item_guid,
                notes = s.editor.doc.notes.len(),
                rate = s.sample_rate(),
                "analysed audio take"
            );
            return Some(Loaded::Audio(Box::new(s)));
        }
        if cand.kind == TakeKind::Midi {
            let midi = MidiTakeLocation::new(
                ctx.clone(),
                ItemRef::Guid(cand.item_guid.clone()),
                TakeRef::Active,
            );
            let s = Session::load(daw, midi, DEFAULT_BEND_RANGE, viewport);
            tracing::info!(item = %cand.item_guid, notes = s.editor.doc.notes.len(), "loaded MIDI take");
            return Some(Loaded::Midi(Box::new(s)));
        }
        None
    }
}

/// What kind of take an item holds, as far as the editor cares.
///
/// `Other` covers video, empty and unknown, and matters because an
/// item with no take reads back from the MIDI service as a take with
/// no notes. Treating that as MIDI would let an empty slot win over
/// the real take two items later, and the user would get a blank roll
/// with no explanation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TakeKind {
    Audio,
    Midi,
    Other,
}

/// One item the runner might open, with everything the two session
/// constructors need read off the facade once.
struct Candidate {
    track_name: String,
    item_guid: String,
    length_secs: f64,
    volume: f64,
    kind: TakeKind,
    take_name: String,
}

impl Candidate {
    fn describe(&self) -> String {
        let kind = match self.kind {
            TakeKind::Audio => "audio",
            TakeKind::Midi => "MIDI",
            TakeKind::Other => "no take",
        };
        if self.take_name.is_empty() {
            format!("{} ({kind})", self.track_name)
        } else {
            format!("{} / {} ({kind})", self.track_name, self.take_name)
        }
    }
}

/// The items worth trying, narrowed by `target` and in project order.
fn candidate_items(
    daw: &Standalone,
    ctx: &ProjectContext,
    target: &Target,
) -> Result<Vec<Candidate>, LoadError> {
    let tracks = Tracks::all(daw, ctx.clone());
    let tracks: Vec<_> = match &target.track {
        None => tracks,
        Some(want) => {
            // A bare number is an index, because that is what a user
            // reads off a track panel; anything else is a name.
            let picked = match want.parse::<usize>() {
                Ok(n) if n >= 1 && n <= tracks.len() => vec![tracks[n - 1].clone()],
                Ok(_) => Vec::new(),
                Err(_) => {
                    let needle = want.to_ascii_lowercase();
                    tracks
                        .into_iter()
                        .filter(|t| t.name.to_ascii_lowercase().contains(&needle))
                        .collect()
                }
            };
            if picked.is_empty() {
                return Err(LoadError::NoSuchTarget(format!("track {want:?}")));
            }
            picked
        }
    };

    let mut out = Vec::new();
    for track in &tracks {
        for item in Items::get_items(
            daw,
            ctx.clone(),
            daw::service::TrackRef::Guid(track.guid.clone()),
        ) {
            let take = Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item.guid.clone()));
            let (kind, take_name) = match &take {
                Some(t) => {
                    let kind = match t.source_type {
                        daw::service::SourceType::Audio => TakeKind::Audio,
                        daw::service::SourceType::Midi => TakeKind::Midi,
                        _ => TakeKind::Other,
                    };
                    (kind, t.name.clone())
                }
                // An item with no take is left in the list rather than
                // filtered out, so the count in the error is honest
                // about how much was looked at.
                None => (TakeKind::Other, String::new()),
            };
            out.push(Candidate {
                track_name: track.name.clone(),
                item_guid: item.guid.clone(),
                length_secs: item.length.as_seconds(),
                volume: item.volume,
                kind,
                take_name,
            });
        }
    }

    if let Some(index) = target.item {
        let one = out
            .into_iter()
            .nth(index)
            .ok_or_else(|| LoadError::NoSuchTarget(format!("item {index}")))?;
        return Ok(vec![one]);
    }
    Ok(out)
}

// Compatibility exports: applications should use expression-editor-host directly.
pub use expression_editor_host::{
    DrumWorkspace, attach_timeline, bar_grid, read_take_mono, track_timeline,
};

pub fn drum_workspace<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: ProjectContext,
    name: &str,
    kit_folder: Option<&str>,
    viewport: Viewport,
    cache_in: Option<&std::path::Path>,
) -> Result<DrumWorkspace<D>, LoadError> {
    expression_editor_host::drum_workspace(daw, ctx, name, kit_folder, viewport, cache_in).map_err(
        |e| match e {
            expression_editor_host::WorkspaceError::NoKitFolder { project, wanted } => {
                LoadError::NoKitFolder { project, wanted }
            }
            expression_editor_host::WorkspaceError::NoEditableItem { project, items } => {
                LoadError::NoEditableItem { project, items }
            }
        },
    )
}

/// A project with nothing in it, so a file-backed session has a context
/// to name.
fn seed_empty_project(daw: &Standalone, path: &Path) -> String {
    let guid = format!("standalone-runner-{}", file_label(path));
    daw.seed_project(daw::service::project::ProjectInfo {
        guid: guid.clone(),
        name: file_label(path),
        path: path.to_string_lossy().into_owned(),
    });
    guid
}

/// Load an `.rpp` into a fresh backend with its media resolved and
/// materialized, the way `Projects::open` does it: sources resolve
/// against the file's directory (REAPER stores them relative to the
/// project), and uncompressed PCM is mmap'd rather than decoded. A
/// source that cannot be found is a per-take warning inside the
/// materialize report, never a failed open.
// r[impl drums.open.rpp]
fn open_project(
    path: &Path,
) -> Result<
    (
        Standalone,
        String,
        daw::standalone::project_loader::LoadedProject,
    ),
    LoadError,
> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| LoadError::Read(path.to_path_buf(), e.to_string()))?;
    let daw = Standalone::new();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    daw.media_bay().set_file_resolver(Box::new(
        daw::standalone::media_bay::ProjectRelativeResolver::new(dir),
    ));
    let name = file_label(path);
    let summary =
        load_rpp_text(&daw, &name, &path.to_string_lossy(), &text).map_err(LoadError::Rpp)?;
    let audio = daw::standalone::audio_engine::materialize::materialize_via_bay(
        &daw,
        &summary.project_guid,
    )
    .map_err(LoadError::Rpp)?;
    if !audio.failed.is_empty() {
        tracing::warn!(
            failed = audio.failed.len(),
            loaded = audio.loaded,
            "some sources did not materialize"
        );
    }
    Ok((daw, name, summary))
}

fn file_label(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
