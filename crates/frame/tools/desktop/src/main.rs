//! `frame` — the desktop surface player.
//!
//! ```text
//! frame [FILE.frame | FILE.fm] [--theme stage-dark|daylight] [--stats] [--size WxH] [--no-fx | --fx gpu|fallback|off]
//! frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob|drag-first-handle]
//! frame [FILE] --engine ws://127.0.0.1:4040/vox [--rig guitar] [...]
//! frame DIR                        # gallery: ] / Tab next, [ / Shift-Tab previous
//! frame --library ws://127.0.0.1:4050/vox [--surface ID | --browse] [--engine …] [--install-dir DIR]
//! ```
//! Runs a control surface live against the in-process mock engine
//! (`surface_memory::Memory::demo()`): moving meters, knobs/faders/buttons
//! driven by mouse and keyboard. With no file it plays the surface
//! generated from the demo catalog (`--theme` picks its look). With a
//! markup or bundle file it hot-reloads on save — edit the `.frame` (by hand
//! or with a model) and the window follows; a parse error keeps the last
//! good surface and shows `line:col: message` in a red banner.
//!
//! `--engine` plays against a live engine (e.g. signal serving a rig)
//! instead of the mock: meters and parameter changes stream in, edits go
//! out, and with no file the surface is generated from the engine's own
//! catalog. `--rig` picks the engine instance (scope) on that endpoint. If
//! the link drops, a banner says so, the surface keeps its last state, and
//! the player reconnects with backoff.
//!
//! `--library` plays surfaces from a library server
//! (`frame-library-server`): `--surface ID` puts one on stage and follows
//! it — every new version (an agent's edit through `frame-mcp --library`,
//! a save on the server's disk) swaps in place, keeping values, a drag in
//! flight and meter ballistics; an update that doesn't parse or adds lint
//! errors is refused with a toast and the stage stays as it was. Without
//! `--surface` (or with `--browse`) it opens the browser: a grid of
//! previews (only those fitting the engine, with `--engine`), arrows to
//! select, Enter or click to preview live, I to install into
//! `--install-dir`, F to fork, B to go back. With no `--engine`, the mock
//! engine also serves the catalogs the library knows.
//!
//! `--screenshot` runs the same loop offscreen for N frames and writes the
//! last one — a headless check that input, engine and rendering agree.

mod browser;
mod headless;
mod input;
mod library;
mod libview;
mod player;
mod remote;
mod source;
mod window;

use std::path::PathBuf;
use std::time::Duration;

use surface_runtime::Surface;
use surface_runtime::sync::Engine as _;
use surface_widgets::Theme;

use crate::libview::LibraryView;
use crate::player::Backend;
use crate::remote::Remote;
use crate::source::Source;

/// How long startup waits for `--engine` to answer.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A live engine to play against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineArg {
    pub url: String,
    /// Instance scope on the endpoint (`guitar`); `None` = unscoped.
    pub rig: Option<String>,
}

pub struct Args {
    pub source: Source,
    /// With a directory argument: every surface in it, `source` first.
    /// `]`/Tab and `[`/Shift-Tab step through them in the window.
    pub gallery: Vec<Source>,
    pub stats: bool,
    /// Initial window / screenshot size, logical px.
    pub size: (u32, u32),
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub script: Option<Script>,
    pub engine: Option<EngineArg>,
    /// Extra catalogs the mock engine serves alongside the demo rig
    /// (`--catalog FILE`, plus any `*.catalog.json` next to the surfaces).
    pub catalogs: Vec<PathBuf>,
    /// Light/shader effects: the GPU stage by default; `--no-fx` = the
    /// vector fallback (soft glows, no shaders).
    pub fx: render::fx::Mode,
    /// `--library URL`: surfaces from a library server.
    pub library: Option<LibraryArg>,
    /// `--screenshot` with a library: stop after this many live updates,
    /// writing `OUT-<n>.png` after each (0 = don't wait for any).
    pub updates: u32,
}

/// A library to play from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryArg {
    pub url: String,
    /// `--surface ID`: start on it, followed live.
    pub surface: Option<String>,
    /// Start on the browser grid.
    pub browse: bool,
    pub install_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    DragFirstKnob,
    /// Library browser: preview the selected surface on frame 1.
    OpenSelected,
    DragFirstHandle,
    /// Click the third shown handle: its panel comes up.
    SelectHandle,
    /// Double-click empty canvas: a band is added there.
    AddOnCanvas,
    /// Double-click the third shown handle and type a name (left editing).
    RenameHandle,
    /// Drag a marquee across the canvas (held, mid-drag).
    Marquee,
    /// Right-click the third shown handle: its context menu.
    ContextHandle,
}

impl Script {
    const NAMES: &str = "drag-first-knob | drag-first-handle | open-selected | select-handle | add-on-canvas | rename-handle | marquee | context-handle";
}

const USAGE: &str = "usage: frame [FILE.frame|FILE.fm] [--theme stage-dark|daylight] [--stats] [--size WxH] [--no-fx|--fx gpu|fallback|off]\n       \
                     frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob|drag-first-handle]\n       \
                     frame [FILE] --engine ws://HOST:PORT/vox [--rig NAME] [...]\n       \
                     frame DIR   (gallery of every .frame/.fm in DIR; ] / Tab next, [ / Shift-Tab previous)\n       \
                     frame --library ws://HOST:PORT/vox [--surface ID | --browse] [--engine …] [--install-dir DIR] [--screenshot OUT.png [--updates N]]";

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut file = None;
    let mut theme = Theme::stage_dark();
    let mut rig = None;
    let (mut library, mut surface, mut browse, mut install_dir) = (None, None, false, None);
    let mut out = Args {
        source: Source::Generated { theme: Box::new(Theme::stage_dark()) },
        gallery: Vec::new(),
        stats: false,
        size: (1280, 480),
        screenshot: None,
        frames: 60,
        script: None,
        engine: None,
        catalogs: Vec::new(),
        fx: render::fx::Mode::Gpu,
        library: None,
        updates: 0,
    };
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--theme" => {
                theme = match value()?.as_str() {
                    "daylight" => Theme::daylight(),
                    "stage-dark" => Theme::stage_dark(),
                    other => return Err(format!("unknown theme {other:?} (stage-dark|daylight)")),
                }
            }
            "--stats" => out.stats = true,
            "--no-fx" => out.fx = render::fx::Mode::Fallback,
            "--fx" => out.fx = value()?.parse()?,
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size wants WxH")?;
                out.size = (w.parse().map_err(|_| "--size wants WxH")?, h.parse().map_err(|_| "--size wants WxH")?);
            }
            "--screenshot" => out.screenshot = Some(value()?.into()),
            "--frames" => out.frames = value()?.parse().map_err(|_| "--frames wants a count")?,
            "--script" => {
                out.script = match value()?.as_str() {
                    "drag-first-knob" => Some(Script::DragFirstKnob),
                    "open-selected" => Some(Script::OpenSelected),
                    "drag-first-handle" => Some(Script::DragFirstHandle),
                    "select-handle" => Some(Script::SelectHandle),
                    "add-on-canvas" => Some(Script::AddOnCanvas),
                    "rename-handle" => Some(Script::RenameHandle),
                    "marquee" => Some(Script::Marquee),
                    "context-handle" => Some(Script::ContextHandle),
                    other => return Err(format!("unknown script {other:?} ({})", Script::NAMES)),
                }
            }
            "--engine" => out.engine = Some(EngineArg { url: value()?, rig: None }),
            "--rig" => rig = Some(value()?),
            "--catalog" => out.catalogs.push(value()?.into()),
            "--library" => library = Some(value()?),
            "--surface" => surface = Some(value()?),
            "--browse" => browse = true,
            "--install-dir" => install_dir = Some(PathBuf::from(value()?)),
            "--updates" => out.updates = value()?.parse().map_err(|_| "--updates wants a count")?,
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}\n{USAGE}")),
            path if file.is_none() => file = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}\n{USAGE}")),
        }
    }
    match (library, &file) {
        (Some(_), Some(_)) => return Err(format!("--library plays from the library; drop the file argument\n{USAGE}")),
        (Some(url), None) => {
            let install_dir = install_dir.unwrap_or_else(library::default_install_dir);
            out.library = Some(LibraryArg { url, surface, browse, install_dir });
        }
        (None, _) if surface.is_some() || browse || install_dir.is_some() => {
            return Err(format!("--surface/--browse/--install-dir need --library\n{USAGE}"));
        }
        (None, _) => {}
    }
    match file {
        Some(dir) if dir.is_dir() => {
            out.gallery = gallery(&dir)?;
            out.source = out.gallery[0].clone();
            out.catalogs.extend(sibling_catalogs(&dir));
        }
        Some(path) => {
            if let Some(dir) = path.parent() {
                out.catalogs.extend(sibling_catalogs(dir));
            }
            out.source = Source::from_path(path);
        }
        None => out.source = Source::Generated { theme: Box::new(theme) },
    }
    match (&mut out.engine, rig) {
        (Some(e), rig) => e.rig = rig,
        (None, Some(_)) => return Err(format!("--rig needs --engine\n{USAGE}")),
        (None, None) => {}
    }
    Ok(out)
}

/// `*.catalog.json` files in `dir` — engines a surface there was designed for.
fn sibling_catalogs(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(if dir.as_os_str().is_empty() { std::path::Path::new(".") } else { dir })
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".catalog.json")))
        .collect();
    out.sort();
    out
}

/// The demo rig plus every extra catalog (files, and those a library
/// served), as one mock engine.
fn mock_engine(extra: &[PathBuf], served: Vec<surface_proto::Catalog>) -> Result<surface_memory::Memory, String> {
    let mut catalog = surface_memory::demo_catalog();
    let mut all = Vec::new();
    for path in extra {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let c: surface_proto::Catalog = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("frame: mock engine also serves `{}` ({} params) from {}", c.id, c.params.len(), path.display());
        all.push(c);
    }
    if !served.is_empty() {
        let ids: Vec<&str> = served.iter().map(|c| c.id.as_str()).collect();
        eprintln!("frame: mock engine also serves the library's catalogs: {}", ids.join(", "));
    }
    for c in all.into_iter().chain(served) {
        catalog.params.extend(c.params);
        catalog.actions.extend(c.actions);
        catalog.streams.extend(c.streams);
        catalog.collections.extend(c.collections);
    }
    Ok(surface_memory::Memory::new(catalog))
}

/// Every frame document in `dir`, sorted by name.
fn gallery(dir: &std::path::Path) -> Result<Vec<Source>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| document_io::is_frame_document(p))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{}: no .frame or .fm surfaces", dir.display()));
    }
    Ok(paths.into_iter().map(Source::from_path).collect())
}

/// What [`start`] sets up.
pub struct Started {
    pub engine: Backend,
    pub surface: Surface,
    /// Why the first surface couldn't load (shown in the banner).
    pub error: Option<String>,
    pub library: Option<LibraryView>,
}

/// The engine (mock, or connected live), the library (if any) and the
/// first surface to play. A broken file isn't fatal: it yields the
/// placeholder plus its error.
///
/// # Errors
/// When `--engine` or `--library` can't be reached.
pub fn start(args: &Args) -> Result<Started, String> {
    let mut library = match &args.library {
        Some(l) => Some(LibraryView::start(&l.url, l.surface.clone(), l.browse, l.install_dir.clone())?),
        None => None,
    };
    let engine = match &args.engine {
        None => {
            let served = library.as_mut().map(LibraryView::catalogs).unwrap_or_default();
            Backend::Local(mock_engine(&args.catalogs, served)?)
        }
        Some(e) => {
            eprintln!("frame: connecting to {}{}", e.url, e.rig.as_ref().map_or(String::new(), |r| format!(" (rig {r})")));
            let mut remote = Remote::start(&e.url, e.rig.as_deref())?;
            remote.wait_connected(CONNECT_TIMEOUT)?;
            Backend::Remote(remote)
        }
    };
    let catalog = pollster::block_on(engine.catalog()).map_err(|e| format!("engine: {e}"))?;
    if let (Some(lib), Some(_)) = (&mut library, &args.engine) {
        // Only offer surfaces whose bindings all resolve on this engine.
        lib.set_fits(&catalog.id, library_link::Signature::of_catalog(&catalog));
    }
    let loaded = match &mut library {
        Some(lib) => lib.initial().map(|s| s.unwrap_or_else(source::placeholder)),
        None => args.source.load(&catalog),
    };
    Ok(match loaded {
        Ok(surface) => Started { engine, surface, error: None, library },
        Err(e) => {
            eprintln!("{e}");
            Started { engine, surface: source::placeholder(), error: Some(e), library }
        }
    })
}

fn main() {
    let result = parse_args(std::env::args().skip(1))
        .and_then(|args| if args.screenshot.is_some() { headless::run(&args) } else { window::run(&args) });
    if let Err(err) = result {
        eprintln!("frame: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(s.split_whitespace().map(String::from))
    }

    #[test]
    fn no_args_plays_the_demo() {
        let a = args("").unwrap();
        assert!(matches!(a.source, Source::Generated { .. }));
        assert!(a.screenshot.is_none());
        assert!(a.engine.is_none());
    }

    #[test]
    fn files_and_flags() {
        let a = args("s.frame --theme daylight --stats --size 800x600").unwrap();
        assert!(matches!(a.source, Source::Markup(_)));
        assert!(a.stats);
        assert_eq!(a.size, (800, 600));
        let a = args("--screenshot o.png --frames 5 --script drag-first-knob x.frame").unwrap();
        assert!(matches!(a.source, Source::Markup(_)));
        assert_eq!((a.frames, a.script), (5, Some(Script::DragFirstKnob)));
    }

    #[test]
    fn effects_default_to_gpu() {
        use render::fx::Mode;
        assert_eq!(args("s.frame").unwrap().fx, Mode::Gpu);
        assert_eq!(args("s.frame --no-fx").unwrap().fx, Mode::Fallback);
        assert_eq!(args("--fx off s.frame").unwrap().fx, Mode::Off);
        assert!(args("--fx neon").is_err());
    }

    #[test]
    fn engine_and_rig() {
        let a = args("--engine ws://127.0.0.1:4040/vox --rig guitar").unwrap();
        assert!(matches!(a.source, Source::Generated { .. }), "no file → generated from the engine's catalog");
        assert_eq!(a.engine, Some(EngineArg { url: "ws://127.0.0.1:4040/vox".into(), rig: Some("guitar".into()) }));
        let a = args("--rig keys s.frame --engine ws://h/vox").unwrap();
        assert_eq!(a.engine.unwrap().rig.as_deref(), Some("keys"), "flag order doesn't matter");
        assert_eq!(args("--engine ws://h/vox").unwrap().engine.unwrap().rig, None);
        assert!(args("--rig guitar").is_err(), "--rig without --engine");
        assert!(args("--engine").is_err());
    }

    #[test]
    fn rejects_junk() {
        assert!(args("--nope").is_err());
        assert!(args("a.frame b.frame").is_err());
        assert!(args("--theme neon").is_err());
        assert!(args("--size 800").is_err());
    }
}
