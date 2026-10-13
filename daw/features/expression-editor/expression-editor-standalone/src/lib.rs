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
        let (daw, name, summary) = open_project(path)?;
        let ctx = ProjectContext::Project(summary.project_guid.clone());

        let tracks = Tracks::all(&daw, ctx.clone());

        // The kit folder: named wins (case-insensitively, exact then
        // substring), otherwise the first folder whose name classifies
        // as a kit.
        let kit = match kit_folder {
            Some(want) => {
                let w = want.to_ascii_lowercase();
                tracks
                    .iter()
                    .find(|t| t.is_folder && t.name.to_ascii_lowercase() == w)
                    .or_else(|| {
                        tracks
                            .iter()
                            .find(|t| t.is_folder && t.name.to_ascii_lowercase().contains(&w))
                    })
            }
            None => tracks
                .iter()
                .find(|t| t.is_folder && expression_editor_core::kit::is_kit_folder(&t.name)),
        }
        .ok_or_else(|| LoadError::NoKitFolder {
            project: name.clone(),
            wanted: kit_folder.map(str::to_string),
        })?;

        let by_guid: std::collections::HashMap<&str, &daw::service::Track> =
            tracks.iter().map(|t| (t.guid.as_str(), t)).collect();

        struct Member {
            guid: String,
            name: String,
            folder: Option<String>,
            role: expression_editor_core::kit::LaneRole,
            doc: expression_editor_core::ExpressionDoc,
            item: ItemRef,
            length_secs: f64,
        }

        // The kit's member tracks, with their folder chains — cheap
        // metadata, gathered sequentially.
        struct Job {
            guid: String,
            name: String,
            folder: Option<String>,
            role: expression_editor_core::kit::LaneRole,
            item_guid: String,
            length_secs: f64,
            volume: f64,
        }
        let mut jobs: Vec<Job> = Vec::new();
        let mut items_seen = 0usize;
        for track in &tracks {
            if track.is_folder {
                continue;
            }
            // Folder chain, nearest first, walked over `parent_guid` —
            // depth-capped so a cyclic project cannot hang the load.
            let mut chain: Vec<&str> = Vec::new();
            let mut under_kit = false;
            let mut cur = track.parent_guid.as_deref();
            for _ in 0..64 {
                let Some(parent) = cur.and_then(|g| by_guid.get(g)) else {
                    break;
                };
                chain.push(parent.name.as_str());
                if parent.guid == kit.guid {
                    under_kit = true;
                }
                cur = parent.parent_guid.as_deref();
            }
            if !under_kit {
                continue;
            }
            let role = expression_editor_core::kit::kit_role(&track.name, &chain);
            let (seen, pick) = edit_item(&daw, &ctx, track);
            items_seen += seen;
            let Some((item_guid, length_secs, volume)) = pick else {
                continue;
            };
            jobs.push(Job {
                guid: track.guid.clone(),
                name: track.name.clone(),
                folder: chain.first().map(|s| s.to_string()),
                role,
                item_guid,
                length_secs,
                volume,
            });
        }

        // The expensive half — reading each mic's audio and finding its
        // hits — is per-mic independent, so it runs one thread per mic.
        // A kit is a dozen tracks; this is the difference between a
        // multi-second open and about the longest single mic.
        let analysed: Vec<Option<(Job, Vec<f64>, f64, expression_editor_core::ExpressionDoc)>> =
            std::thread::scope(|scope| {
                let handles: Vec<_> = jobs
                    .into_iter()
                    .map(|job| {
                        let daw = daw.clone();
                        let ctx = ctx.clone();
                        scope.spawn(move || {
                            let (samples, rate) = read_take_mono(
                                &daw,
                                &ctx,
                                &job.item_guid,
                                job.length_secs,
                                job.volume,
                            )?;
                            let mut doc = percussion_doc(&samples, rate);
                            attach_regions(&daw, &ctx, &mut doc);
                            Some((job, samples, rate, doc))
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or(None))
                    .collect()
            });

        // The trigger lanes' running sums (mean of the members), built
        // while the samples are in hand — the host detects on these.
        // r[impl drums.group.detection-source]
        let mut members: Vec<Member> = Vec::new();
        let mut sums: std::collections::HashMap<
            expression_editor_core::kit::LaneRole,
            (Vec<f64>, usize),
        > = std::collections::HashMap::new();
        let mut sample_rate = 0.0f64;
        for (job, samples, rate, doc) in analysed.into_iter().flatten() {
            if sample_rate <= 0.0 {
                sample_rate = rate;
            }
            if job.role.is_detection_source() {
                let (sum, count) = sums.entry(job.role).or_default();
                if sum.len() < samples.len() {
                    sum.resize(samples.len(), 0.0);
                }
                for (o, v) in sum.iter_mut().zip(samples.iter()) {
                    *o += v;
                }
                *count += 1;
            }
            members.push(Member {
                guid: job.guid,
                name: job.name,
                folder: job.folder,
                role: job.role,
                doc,
                item: ItemRef::Guid(job.item_guid),
                length_secs: job.length_secs,
            });
        }

        if members.is_empty() {
            return Err(LoadError::NoEditableItem {
                project: name,
                items: items_seen,
            });
        }

        let mics = members.len();
        let take_secs = members.iter().map(|m| m.length_secs).fold(0.0f64, f64::max);
        // The kit group's items, by role — the host edits all of them
        // for every gesture. r[impl drums.group.kit]
        let host_lanes: Vec<drum_host::HostLane> = expression_editor_core::kit::LaneRole::ALL
            .into_iter()
            .filter_map(|role| {
                let items: Vec<ItemRef> = members
                    .iter()
                    .filter(|m| m.role == role)
                    .map(|m| m.item.clone())
                    .collect();
                if items.is_empty() {
                    return None;
                }
                let summed = sums.remove(&role).map(|(mut sum, count)| {
                    let scale = 1.0 / count.max(1) as f64;
                    for v in &mut sum {
                        *v *= scale;
                    }
                    sum
                });
                Some(drum_host::HostLane {
                    role,
                    items,
                    summed,
                })
            })
            .collect();

        let mut it = members.into_iter();
        let first = it.next().expect("checked non-empty");
        let mut editor = Editor::new(first.doc, viewport);
        editor.set_mode(Mode::UnpitchedAudio);
        let mut roles = vec![(first.guid.clone(), first.role)];
        if let Some(t) = editor.tracks.track_mut(0) {
            t.guid = first.guid;
            t.name = first.name;
            t.folder = first.folder;
        }
        for m in it {
            let i = editor.add_track_with_guid(m.guid.clone(), m.name, m.doc);
            if let Some(t) = editor.tracks.track_mut(i) {
                t.set_mode(Mode::UnpitchedAudio);
                t.folder = m.folder;
            }
            roles.push((m.guid, m.role));
        }
        editor.tracks.fold_roles(&roles);
        // The stack *is* the drum workspace view; the roll is one click
        // away per lane.
        editor.stacked = true;
        // Grid targets come from the project's tempo, not a default —
        // a hit quantized against 120 in an 84 bpm session lands
        // nowhere musical.
        // r[impl drums.group.tempo]
        let bpm = daw::service::tempo_map::TempoMap::get_tempo_at(&daw, ctx.clone(), 0.0);
        if bpm > 0.0 {
            editor.bpm = bpm;
        }

        let host = drum_host::DrumHost::new(
            daw.clone(),
            ctx,
            host_lanes,
            sample_rate,
            take_secs,
            60.0 / editor.bpm.max(1.0),
        );
        Ok(Runner {
            label: format!("{name} — drums: {} ({mics} mics)", kit.name),
            daw: Some(daw),
            loaded: Loaded::DrumWorkspace(Box::new(editor)),
            host: Some(std::sync::Arc::new(host)),
        })
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

/// The analysis hop for a drum mic, in samples: the document's frame is
/// `sample_rate / DRUM_HOP`. 512 matches the onset analyser's usual hop,
/// so a peak bin here is the same width a percussive doc draws.
const DRUM_HOP: usize = 512;

/// The item worth editing on a drum mic: the **longest** audio item on
/// the track whose fixed lane — when the track shows lanes at all — is
/// playing. Returns how many items were looked at, and the pick as
/// `(item guid, length secs, item volume)`.
// r[impl drums.open.runner]
fn edit_item(
    daw: &Standalone,
    ctx: &ProjectContext,
    track: &daw::service::Track,
) -> (usize, Option<(String, f64, f64)>) {
    let items = Items::get_items(
        daw,
        ctx.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
    );
    let seen = items.len();
    let mut best: Option<(String, f64, f64)> = None;
    for item in items {
        let is_audio = Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item.guid.clone()))
            .is_some_and(|t| t.source_type == daw::service::SourceType::Audio);
        if !is_audio {
            continue;
        }
        // A take parked on a muted lane is an alternate, not the
        // performance; an item with no fixed lane is on whatever the
        // track plays.
        if track.lane_count > 0
            && let Some(lane) = item.fixed_lane
            && (lane >= 64 || track.lane_play_mask & (1u64 << lane) == 0)
        {
            continue;
        }
        let length = item.length.as_seconds();
        if best.as_ref().is_none_or(|(_, l, _)| length > *l) {
            best = Some((item.guid.clone(), length, item.volume));
        }
    }
    (seen, best)
}

/// Compose one track's *playing* audio into a single timeline buffer —
/// every playing-lane audio item read through the accessor and placed
/// at its position, out to `take_secs`. This is what a lane draws
/// after an edit landed on the daw: the split pieces where they now
/// sit, not where the take was when it loaded.
pub(crate) fn track_timeline(
    daw: &Standalone,
    ctx: &ProjectContext,
    track: &daw::service::Track,
    take_secs: f64,
    sample_rate: f64,
) -> Vec<f64> {
    let mut out = vec![0.0f64; (take_secs * sample_rate).ceil().max(0.0) as usize];
    let items = Items::get_items(
        daw,
        ctx.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
    );
    let mut placed: Vec<_> = items
        .into_iter()
        .filter(|item| {
            if track.lane_count > 0
                && let Some(lane) = item.fixed_lane
                && (lane >= 64 || track.lane_play_mask & (1u64 << lane) == 0)
            {
                return false;
            }
            Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item.guid.clone()))
                .is_some_and(|t| t.source_type == daw::service::SourceType::Audio)
        })
        .collect();
    // Ascending position: a later piece's attack overwrites the
    // previous piece's crossfade tail, which is what detection needs.
    placed.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
    for item in placed {
        let Some((samples, rate)) =
            read_take_mono(daw, ctx, &item.guid, item.length.as_seconds(), item.volume)
        else {
            continue;
        };
        let at = (item.position.as_seconds() * sample_rate).round().max(0.0) as usize;
        if (rate - sample_rate).abs() < 1e-6 {
            for (i, s) in samples.iter().enumerate() {
                if at + i < out.len() {
                    out[at + i] = *s;
                }
            }
        } else {
            // Rates differing inside one kit is unusual; nearest-
            // neighbour is fine for drawing and detection.
            let n = (samples.len() as f64 * sample_rate / rate) as usize;
            for i in 0..n {
                let src = (i as f64 * rate / sample_rate) as usize;
                if at + i < out.len() && src < samples.len() {
                    out[at + i] = samples[src];
                }
            }
        }
    }
    out
}

/// Pull one take's audio through the accessor, chunked, mono, at the
/// source rate — the same read [`AudioSession::load`] does, capped at
/// the item length. Returns `(samples, sample_rate)`.
// r[impl drums.open.runner]
pub(crate) fn read_take_mono(
    daw: &Standalone,
    ctx: &ProjectContext,
    item_guid: &str,
    length_secs: f64,
    volume: f64,
) -> Option<(Vec<f64>, f64)> {
    use daw::service::audio_accessor::{AudioAccessors, GetSamplesRequest};

    let accessor = daw.create_take_accessor(
        ctx.clone(),
        ItemRef::Guid(item_guid.to_string()),
        TakeRef::Active,
    )?;
    // Probe for the source's own rate and channel count: asking for a
    // rate the host does not have would resample, and a resampled
    // analysis puts every hit slightly off.
    let probe = daw.get_samples(GetSamplesRequest {
        accessor_id: accessor.clone(),
        sample_rate: 0.0,
        num_channels: 0,
        start_time: 0.0,
        num_samples: 1,
    });
    let sample_rate = if probe.sample_rate > 0.0 {
        probe.sample_rate
    } else {
        48_000.0
    };
    let channels = probe.num_channels.max(1);

    // Chunked, because a whole multitrack take in one call makes the
    // host allocate all of it before returning any — same bound the
    // audio session reads at.
    const CHUNK: u32 = 1 << 16;
    let total = (length_secs.max(0.0) * sample_rate).ceil() as u32;
    let mut interleaved = Vec::with_capacity(total as usize * channels as usize);
    let mut done = 0u32;
    while done < total {
        let want = CHUNK.min(total - done);
        let chunk = daw.get_samples(GetSamplesRequest {
            accessor_id: accessor.clone(),
            sample_rate,
            num_channels: channels,
            start_time: done as f64 / sample_rate,
            num_samples: want,
        });
        if chunk.samples.is_empty() {
            break;
        }
        interleaved.extend_from_slice(&chunk.samples);
        done += want;
    }
    daw.destroy_accessor(&accessor);

    let mut samples = expression_editor_audio::to_mono(&interleaved, channels.max(1) as usize);
    if samples.is_empty() {
        return None;
    }
    // The accessor hands back source audio; the item's own gain is on
    // top, and detection thresholds are absolute.
    if volume != 1.0 && volume > 0.0 {
        for s in &mut samples {
            *s *= volume;
        }
    }
    Some((samples, sample_rate))
}

/// One drum mic as a percussive document: per-hop peaks behind one note
/// per transient.
///
/// Deliberately **not** [`expression_editor_audio::analyze_percussive`]:
/// Carry the project's regions onto a document, in its own time units.
///
/// The sections are how a player knows where they are — "bar 38" says
/// nothing, "CH 1" says everything — so every lane's ruler shows them.
/// Host colours come through as `#rrggbb`; REAPER's section colours are
/// the ones the band already knows from the arrange view.
pub(crate) fn attach_regions(
    daw: &Standalone,
    ctx: &ProjectContext,
    doc: &mut expression_editor_core::ExpressionDoc,
) {
    use daw::service::Regions;
    let ups = doc.time_base.units_per_second(120.0);
    if ups <= 0.0 {
        return;
    }
    doc.regions = Regions::all(daw, ctx.clone())
        .into_iter()
        .map(|r| expression_editor_core::doc::Region {
            start: r.time_range.start_seconds() * ups,
            end: r.time_range.end_seconds() * ups,
            label: r.name,
            color: r.color.map(|c| format!("#{c:06x}")),
        })
        .collect();
}

/// that segments by spectral flux over an STFT, which quantises every
/// hit to a hop and costs a transform per mic. The envelope gate is the
/// detector the quantize panel uses, sample-accurate and cheap — the
/// price is that it measures no centroid, so every hit lands in the
/// middle band rather than being sorted by brightness.
// r[impl drums.open.runner]
pub(crate) fn percussion_doc(
    samples: &[f64],
    sample_rate: f64,
) -> expression_editor_core::ExpressionDoc {
    use expression_editor_audio::{DetectConfig, transients};
    use expression_editor_core::rows::SliceBands;
    use expression_editor_core::{ExpressionDoc, Note, NoteId, RowSpace, TimeBase};

    let frame_rate = sample_rate / DRUM_HOP as f64;
    let total_frames = samples.len() as f64 / DRUM_HOP as f64;
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate }, 0.0, total_frames);
    doc.row_space = RowSpace::Bands(SliceBands::default());
    // The backdrop is the whole recording, hits and gaps alike — the
    // only way to see a missed ghost note is to see past the notes.
    doc.peaks = samples
        .chunks(DRUM_HOP)
        .map(|c| c.iter().fold(0.0f64, |m, v| m.max(v.abs())).min(1.0) as f32)
        .collect();

    let hits = transients(samples, sample_rate, DetectConfig::default());
    for (i, hit) in hits.iter().enumerate() {
        let start = hit.at * frame_rate;
        // A slice runs to the next hit, so the slices tile the take and
        // can be selected and levelled like notes.
        let end = hits
            .get(i + 1)
            .map(|next| next.at * frame_rate)
            .unwrap_or(total_frames);
        let mut note = Note::new(NoteId(i as u64 + 1), start, end.max(start + 1.0), 1);
        note.velocity = hit.loudness.clamp(0.0, 1.0);
        note.weight = note.velocity;
        doc.push(note);
    }
    doc
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
