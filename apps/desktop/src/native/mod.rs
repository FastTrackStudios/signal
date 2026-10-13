//! The Session app on Blitz (dioxus-native) — see `docs/app-on-blitz.md`.
//!
//! The desktop build launches this instead of the WRY app when the `native`
//! feature is on. It is the app as panels (`session_daw::studio::Arrangement`,
//! the progress bar, …) composed into views under one top bar, with the
//! window's own title bar made transparent so the traffic lights sit in ours.
//!
//! The session is opened once, before the window, on the process's
//! daw-standalone engine, prepared the way `just studio-song` prepares it
//! (organize, build from the chart, generate the click and guide), and handed
//! to every panel as context.
//!
//! That is Engine mode. In Remote and Cue (`--audio remote|cue`,
//! `--reaper [socket]`, `FTS_AUDIO_MODE`, or the top bar's audio menu last
//! time — see `session_daw::open::launch_mode`) nothing is opened here: the
//! window attaches to the system it drives and its open projects are the
//! set (`session_daw::setlist::Setlist::attach`).

#[cfg(target_os = "ios")]
mod ios_audio;
#[cfg(target_os = "ios")]
mod ios_scene;

mod library;
mod shell;
mod start;

use std::any::Any;
use std::path::{Path, PathBuf};

/// The developer's example session, used when it is there and nothing
/// else has been chosen: Always On Time, from the sessions checkout beside
/// this repo.
const EXAMPLE: &str = "/Volumes/build-disk/development/sessions/Always On Time/Always On Time.RPP";

/// What to open as the window opens, in order: `FTS_SESSION_SETLIST` (a
/// setlist); then `FTS_SESSION_PROJECT` (one song); what was open last
/// time; and the example, if this machine has it. `None` when there is
/// nothing: the window stays on the start screen ([`start`]), which is
/// where a set is picked, joined, or streamed. `FTS_SESSION_START=1` opens
/// nothing, for the start screen itself.
fn choose() -> Option<PathBuf> {
    if std::env::var("FTS_SESSION_START").is_ok_and(|v| v != "0") {
        return None;
    }
    std::env::var_os("FTS_SESSION_SETLIST")
        .or_else(|| std::env::var_os("FTS_SESSION_PROJECT"))
        .map(PathBuf::from)
        .or_else(|| remembered().filter(|p| p.exists()))
        // Not on a phone: its disk is its own, and a simulator's reach into
        // this Mac's `/Volumes` blocks on a privacy prompt no one sees.
        .or_else(|| {
            Some(PathBuf::from(EXAMPLE)).filter(|p| cfg!(not(target_os = "ios")) && p.is_file())
        })
}

/// The songs `target` names: the one song it is (a project, a `.session`,
/// or a song's folder); or a setlist's (a folder of song folders, or a
/// `.setlist` file — see `session_daw::setlist::read_setlist`).
fn songs_of(target: &Path) -> Option<Vec<PathBuf>> {
    use session_daw::setlist::{read_setlist, song_in};
    if target.is_dir() && !session_daw::open::is_session(target) {
        if let Some(song) = song_in(target) {
            return Some(vec![song]);
        }
    } else if !target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("setlist"))
    {
        return Some(vec![target.to_path_buf()]);
    }
    read_setlist(target)
        .inspect_err(|e| tracing::error!(error = %e, "the setlist could not be read"))
        .ok()
}

/// The Open dialog: a REAPER project.
#[cfg(not(target_os = "ios"))]
fn pick() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open a session")
        .add_filter("Song or setlist", &["RPP", "rpp", "setlist"])
        .pick_file()
}

/// A phone has no Open dialog: its songs are the app's documents, which
/// the start screen lists.
#[cfg(target_os = "ios")]
fn pick() -> Option<PathBuf> {
    None
}

/// An error, and the choices `buttons` offers: the one picked, or `None`
/// (the window closed, or the last button, which is always "Quit").
#[cfg(not(target_os = "ios"))]
fn ask(title: &str, description: String, buttons: &[&str]) -> Option<String> {
    let dialog = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(description);
    let dialog = match buttons {
        [a, b, c] => dialog.set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            (*a).into(),
            (*b).into(),
            (*c).into(),
        )),
        [a, b] => dialog.set_buttons(rfd::MessageButtons::OkCancelCustom(
            (*a).into(),
            (*b).into(),
        )),
        _ => dialog,
    };
    match dialog.show() {
        rfd::MessageDialogResult::Custom(choice) if buttons.last() != Some(&choice.as_str()) => {
            Some(choice)
        }
        _ => None,
    }
}

/// On a phone the error is logged and the launch gives up (the page it
/// opens on says so).
#[cfg(target_os = "ios")]
fn ask(title: &str, description: String, _buttons: &[&str]) -> Option<String> {
    tracing::error!(dialog.title = title, dialog.description = %description, "launch: no dialog to ask on this platform");
    None
}

/// Open `url` in the browser (a sign-in's approval page).
fn open_url(url: &str) {
    #[cfg(target_os = "ios")]
    ios_scene::open_url(url);
    #[cfg(not(target_os = "ios"))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(opener).arg(url).spawn() {
            tracing::warn!(error = %e, "could not open a link in the browser");
        }
    }
}

/// What is on the clipboard, when a platform lets it be read here (a
/// phone, where typing a link is the hard way).
fn pasted() -> Option<String> {
    #[cfg(target_os = "ios")]
    return ios_scene::pasted();
    #[cfg(not(target_os = "ios"))]
    None
}

/// Where the last session's path is kept.
fn memory() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("last-session"))
}

fn remembered() -> Option<PathBuf> {
    let text = std::fs::read_to_string(memory()?).ok()?;
    let path = text.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn remember(project: &Path) {
    // The start screen's Recent, too.
    session_daw::projects::opened(project);
    let Some(file) = memory() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, project.as_os_str().as_encoded_bytes()));
    if let Err(e) = written {
        // Not fatal: the next launch asks again.
        tracing::warn!(error = %e, "could not remember the session");
    }
}

/// The window, and the setlist in it — every song in the one engine, the
/// first current. Returns when the window closes.
///
/// The window opens on the start screen ([`start`]), opening what was
/// chosen ([`choose`]) as it does — so a phone, with nothing picked before
/// launch, and a desktop take the same way in, and nothing is opened
/// before there is a window to say so. With nothing chosen, or when it
/// does not open, the start screen stays.
pub fn launch() {
    // How this app dials a Session engine (Remote on another Session): the
    // connector lives with the app's iroh identity (`crate::remote`).
    #[cfg(feature = "session-domain")]
    session_daw::open::set_engine_dialer(|address| {
        Box::pin(async move {
            let addr = crate::remote::EngineAddr::parse(&address).map_err(|e| eyre::eyre!(e))?;
            let engine = crate::remote::connect_engine_daw(&addr).await?;
            let daw = engine.daw.clone();
            Ok((daw, Box::new(engine) as session_daw::open::EngineConnection))
        })
    });
    let mode = session_daw::open::launch_mode();
    session_daw::open::set_mode(mode.clone());
    let remote = if mode.owns_project() {
        None
    } else {
        mode.target
    };
    match remote {
        Some(target) => match attach(&target) {
            Some(Attached::Set(setlist)) => run(Some(setlist), None),
            Some(Attached::Engine) => run(None, choose()),
            None => {}
        },
        None => run(None, choose()),
    }
}

/// What attaching came to: the set the other system has open, or Engine
/// mode after all (it was not there, and this window plays the set).
enum Attached {
    Set(session_daw::setlist::Setlist),
    Engine,
}

/// Attach to the system a Remote or Cue window drives. When it is not
/// there, say so and offer to try again, to open a session here in Engine
/// mode instead, or to quit — never quietly play the set here while
/// someone thinks REAPER is.
fn attach(target: &session_daw::open::RemoteTarget) -> Option<Attached> {
    loop {
        match session_daw::setlist::Setlist::attach(target) {
            Ok(setlist) => return Some(Attached::Set(setlist)),
            Err(e) => {
                tracing::error!(error = %e, audio.target = target.kind(), "could not attach to the system this window drives");
                let answer = ask(
                    "Could not reach it",
                    format!("{}\n\n{e}", target.describe()),
                    &["Try Again", "Open in Engine Mode", "Quit"],
                );
                match answer.as_deref() {
                    Some("Try Again") => {}
                    Some("Open in Engine Mode") => {
                        session_daw::open::set_mode(session_daw::open::ModeState::engine());
                        return Some(Attached::Engine);
                    }
                    _ => return None,
                }
            }
        }
    }
}

/// The window: over `setlist` when it is open already (Remote, Cue);
/// otherwise the start screen, opening `open` — the window is up at once,
/// and the song comes in behind its loading note (`start`).
fn run(setlist: Option<session_daw::setlist::Setlist>, open: Option<PathBuf>) {
    // iOS 27 stops an app that has not adopted scenes: ours is registered
    // before UIKit starts, for Info.plist to name.
    #[cfg(target_os = "ios")]
    ios_scene::register();
    let attributes = window_attributes();
    let contexts: Vec<Box<dyn Fn() -> Box<dyn Any> + Send + Sync>> = vec![
        Box::new(move || Box::new(setlist.clone()) as Box<dyn Any>),
        Box::new(move || Box::new(start::OpenFirst(open.clone())) as Box<dyn Any>),
    ];
    dioxus_native::launch_cfg(start::App, contexts, vec![Box::new(attributes)]);
}

/// The window: our top bar IS the title bar. On macOS the native one is made
/// transparent and the content runs up under it, so the traffic lights sit
/// inside the app's own bar, as in the Claude app.
/// A phone's window is its screen: no size of our own to start from (a
/// desktop size taken literally leaves the first surface a different shape
/// from the screen, and the picture stays squeezed after the resize).
///
/// And no status bar: its row is the app's, for the top bar's controls,
/// the way a full-screen instrument app takes it (Logic on iPad). A phone
/// with a notch keeps the notch's inset whatever the bar does.
#[cfg(target_os = "ios")]
fn window_attributes() -> winit::window::WindowAttributes {
    winit::window::WindowAttributes::default()
        .with_title("Session")
        .with_platform_attributes(Box::new(
            winit::platform::ios::WindowAttributesIos::default()
                .with_prefers_status_bar_hidden(true),
        ))
}

#[cfg(not(target_os = "ios"))]
fn window_attributes() -> winit::window::WindowAttributes {
    // `FTS_WINDOW_POS="x,y"` / `FTS_WINDOW_SIZE="WxH"` (logical pixels)
    // place the window instead of maximizing it — what `just duo` uses to
    // put two collaborating windows side by side.
    fn pair(var: &str, sep: char) -> Option<(f64, f64)> {
        let raw = std::env::var(var).ok()?;
        let (a, b) = raw.split_once(sep)?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
    let (pos, size) = (pair("FTS_WINDOW_POS", ','), pair("FTS_WINDOW_SIZE", 'x'));
    let (w, h) = size.unwrap_or((1600.0, 1000.0));
    let mut attributes = winit::window::WindowAttributes::default()
        .with_title("Session")
        // Opens filling the screen unless placed; this is the size it
        // restores to when un-maximized.
        .with_maximized(pos.is_none() && size.is_none())
        .with_surface_size(winit::dpi::LogicalSize::new(w, h))
        // Small enough to be phone-sized: below ~700 wide (or ~500 tall)
        // the app takes its small-screen layout (`session_daw::compact`).
        .with_min_surface_size(winit::dpi::LogicalSize::new(320.0, 300.0));
    if let Some((x, y)) = pos {
        attributes = attributes.with_position(winit::dpi::LogicalPosition::new(x, y));
    }
    #[cfg(target_os = "macos")]
    let attributes = attributes.with_platform_attributes(Box::new(
        winit::platform::macos::WindowAttributesMacOS::default()
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true),
    ));
    attributes
}
