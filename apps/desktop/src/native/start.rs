//! The start screen: what the window shows when nothing is open yet — on a
//! phone, which has no Open dialog and no song picked before launch; on a
//! desktop, when nothing was chosen or it did not open.
//!
//! Three ways in:
//!
//! - **Join a set** — one Task keeps, by its live link: the public demo, or
//!   one someone shared. Its songs stream in and play here, with everyone
//!   else in the set (`session_daw::stream_set`).
//! - **Task** — sign in from another device (`session_daw::task_account`)
//!   and pick a setlist from an org's library; it streams in the same way.
//! - **On this device** — a song or setlist in the app's documents folder
//!   (on a phone, what the Files app shows under Session), or, on a
//!   desktop, from the Open dialog.
//!
//! Opening runs on a thread of its own (it takes seconds), and the set it
//! opened becomes the window's ([`App`]).

use std::path::PathBuf;

use dioxus::prelude::*;
use lucide_dioxus::{
    Check, ChevronRight, ChevronsUpDown, CircleAlert, CircleCheck, CloudDownload, FileMusic,
    HardDriveDownload, House, Library, LibraryBig, Link, ListMusic, LogOut, Music, Pencil, Play,
    Plus, Radio, Search, Trash2, X,
};
use session_daw::loading::{Loading, Progress, mark_src};
use session_daw::setlist::Setlist;
use session_daw::song_stream::Offline;
use session_daw::stream_set::{
    Downloading, LibrarySetlist, LibrarySong, ListKind, Remote, with_library,
};
use session_daw::task_account::{self, Account, Started};

pub(super) const BG: &str = "#0f1012";
pub(super) const BAR: &str = "#17181b";
pub(super) const RULE: &str = "#2a2c31";
pub(super) const TEXT: &str = "#e5e7eb";
pub(super) const DIM: &str = "#8b9099";
pub(super) const ACCENT: &str = "#3aa0ff";
pub(super) const WARN: &str = "#e3b341";
/// Room at the top: on macOS the window's traffic lights sit over the
/// page (its title bar is transparent, as the shell's).
pub(super) const TOP: u32 = if cfg!(target_os = "macos") { 44 } else { 28 };

/// The public demo's live link (`SESSION_DEMO_LINK` at build time, as the
/// site's /demo page takes it).
const DEMO_LINK: &str = match option_env!("SESSION_DEMO_LINK") {
    Some(link) => link,
    None => {
        "https://task.fasttrackstudio.app/org/days-to-praise/share/1e75d7fa11c540ac8dbbd461048e59fc"
    }
};

/// The window: the set once one is open, the start screen until then.
#[component]
pub fn App() -> Element {
    // iOS: winit's window is handed to the app's scene (see `ios_scene`) —
    // here, at the root, whichever the window opens on.
    #[cfg(target_os = "ios")]
    {
        let window = dioxus_native::use_window();
        use_hook(move || {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::UiKit(uikit) = handle.as_raw()
            {
                super::ios_scene::window_created(uikit.ui_view);
            }
            // An audio app, as iOS knows one: its session (the loudspeaker,
            // Bluetooth, AirPlay) set before the engine opens the device,
            // and the lock screen's commands listened for.
            super::ios_audio::configure_session();
            super::ios_audio::install_commands();
            // No UIKit pinch recognizer: the arrangement reads both
            // fingers itself (a pinch zooms time across and the rows
            // down, and carries the view), and a recognizer that took the
            // gesture would cancel the touches it is made of.
        });
    }
    let initial: Option<Setlist> = use_context();
    let mut opened = use_signal(|| initial);
    // Back to the start screen: the set stops, and the page is the one
    // that picks another (its library, a link, a file).
    use_context_provider(|| {
        session_daw::shell::Back(Callback::new(move |()| {
            session_daw::engine::transport(session_daw::engine::Move::Stop, 0.0);
            #[cfg(target_os = "ios")]
            super::ios_audio::clear();
            opened.set(None);
        }))
    });
    let page = match opened() {
        Some(setlist) => rsx! {
            WithSetlist { setlist, super::shell::Shell {} }
        },
        None => rsx! { Start { opened } },
    };
    rsx! {
        document::Style { {ROOT_CSS} }
        {page}
    }
}

/// The page itself, edge to edge. Blitz's default stylesheet gives `body`
/// an 8px margin, and Blitz places an absolutely positioned box against its
/// parent — the body — so without this every view sat 8px right and down
/// and ran off the right edge. And the canvas takes the page's background,
/// which is what fills a phone's status bar and home indicator: the bars'
/// colour (`session_daw::shell::BAR_BG`), so the top bar runs up under the
/// status bar and the bottom bar down under the home indicator, into the
/// screen's rounded corners, rather than stopping short of a darker strip.
const ROOT_CSS: &str = "html, body { margin: 0; padding: 0; background: #17181b; }";

/// What the launch chose to open as the window opens (see
/// `super::choose`), if anything.
#[derive(Clone)]
pub struct OpenFirst(pub Option<PathBuf>);

/// Whether the launch's choice ([`OpenFirst`]) has been opened: it is the
/// first start screen's to open, not every one after a Back.
static LAUNCH_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The set, as the shell reads it.
#[component]
fn WithSetlist(setlist: Setlist, children: Element) -> Element {
    use_context_provider(|| setlist);
    // `FTS_SESSION_AUTOPLAY=1` starts playing as the set opens — for
    // measuring a session while it plays without a hand on the mouse.
    use_hook(|| {
        if std::env::var("FTS_SESSION_AUTOPLAY").is_ok_and(|v| v != "0") {
            session_daw::engine::transport(session_daw::engine::Move::PlayStop, 0.0);
        }
    });
    children
}

/// Where opening has got to.
#[derive(Clone, PartialEq)]
enum Opening {
    Idle,
    /// Opening: the loading screen shows where it has got to.
    Busy(Progress),
    Failed(String),
}

/// Where signing in to Task has got to.
#[derive(Clone, PartialEq)]
enum SignIn {
    Out,
    Starting,
    /// Waiting for the code to be approved.
    Code(Started),
    In(Account),
    Failed(String),
}

/// Something loaded in the background.
#[derive(Clone, PartialEq)]
pub(super) enum Load<T> {
    Waiting,
    Ready(T),
    Failed(String),
}

/// `work` on a thread of its own; its answer, awaited.
pub(super) async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (done, answer) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    answer.await.ok()
}

/// A way of opening a set, run on a thread of its own: it says each step
/// to its `progress` — the loading screen's.
type OpenWork = Box<dyn FnOnce(&dyn Fn(Progress)) -> eyre::Result<Setlist> + Send>;

/// An error for a person: the whole chain, in a few words.
pub(super) fn brief(e: &eyre::Report) -> String {
    session_daw::task_set::brief(&format!("{e:#}"))
}

/// Where the start screen is: its sections, down a sidebar (a tab bar
/// along the foot on a phone).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Home,
    Setlists,
    Lists,
    Songs,
    Downloads,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::Home,
        Self::Setlists,
        Self::Lists,
        Self::Songs,
        Self::Downloads,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Setlists => "Setlists",
            Self::Lists => "Song lists",
            Self::Songs => "Songs",
            Self::Downloads => "Downloads",
        }
    }

    /// The word under its icon in a phone's tab bar.
    const fn short(self) -> &'static str {
        match self {
            Self::Lists => "Lists",
            other => other.label(),
        }
    }
}

/// The list editor, open: on one list, or making one.
#[derive(Clone, PartialEq)]
struct Editing {
    open: Option<String>,
    create: Option<ListKind>,
}

/// The window's width in logical pixels, as it is resized.
fn use_width() -> Signal<f64> {
    let window = dioxus_native::use_window();
    let logical = |w: &std::sync::Arc<dyn winit::window::Window>| {
        f64::from(w.surface_size().width) / w.scale_factor().max(1.0)
    };
    let mut width = use_signal(|| logical(&window));
    dioxus_native::use_window_event(move |event, _| {
        if matches!(
            event,
            winit::event::WindowEvent::SurfaceResized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
        ) {
            let now = logical(&window);
            if (now - *width.peek()).abs() > 0.5 {
                width.set(now);
            }
        }
    });
    width
}

#[component]
fn Start(opened: Signal<Option<Setlist>>) -> Element {
    let mut opening = use_signal(|| Opening::Idle);
    let mut local = use_signal(documents);
    // The songs opened lately, each with its alternatives — read again
    // each time the start screen is shown.
    let recent = use_hook(session_daw::projects::recent);
    let mut section = use_signal(|| Section::Home);
    let width = use_width();

    // Open a set on a thread of its own; it becomes the window's when it
    // is ready. Meanwhile the loading screen says where it has got to.
    let mut open_set = move |first: Progress, work: OpenWork| {
        opening.set(Opening::Busy(first));
        let (tell, mut heard) = tokio::sync::mpsc::unbounded_channel::<Progress>();
        spawn(async move {
            while let Some(step) = heard.recv().await {
                if matches!(*opening.peek(), Opening::Busy(_)) {
                    opening.set(Opening::Busy(step));
                }
            }
        });
        spawn(async move {
            let outcome = off_thread(move || work(&|step| drop(tell.send(step)))).await;
            match outcome {
                Some(Ok(setlist)) => opened.set(Some(setlist)),
                Some(Err(e)) => {
                    tracing::warn!(error = %e, "start: the set did not open");
                    opening.set(Opening::Failed(brief(&e)));
                }
                None => opening.set(Opening::Failed("opening stopped".to_owned())),
            }
        });
    };
    let mut open_path = move |path: PathBuf| {
        let title = path.file_stem().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        open_set(
            Progress::Opening { title },
            Box::new(move |_| {
                let songs = super::songs_of(&path)
                    .ok_or_else(|| eyre::eyre!("{} is not a song or a setlist", path.display()))?;
                let setlist = Setlist::open(&songs)?;
                super::remember(&path);
                Ok(setlist)
            }),
        );
    };
    // A song from nothing (`SESSION_NEW`): untitled, in Organize mode, its
    // count-in, start and guide placed.
    let mut new_project = move || {
        open_set(
            Progress::Opening {
                title: session_daw::project_new::UNTITLED.to_owned(),
            },
            Box::new(|_| session_daw::project_new::setlist(Some(true))),
        );
    };

    // What the launch chose, opened as the window opens.
    let OpenFirst(first) = use_context();
    use_hook(move || {
        // A version picked from inside a song, which came back out here to
        // open it (`session_daw::versions`, `SESSION_OPEN`) — or, the
        // first time only, what the launch chose. Every time, that was
        // reopened whenever the start screen came back: Back from a song
        // the app was launched with went straight back into it.
        let launch =
            first.filter(|_| !LAUNCH_OPENED.swap(true, std::sync::atomic::Ordering::AcqRel));
        if let Some(guid) = session_daw::project_new::take_made() {
            // A new song made from inside another (`SESSION_NEW`), which
            // came back out here to open it: it is in the engine already.
            open_set(
                Progress::Opening {
                    title: session_daw::project_new::UNTITLED.to_owned(),
                },
                Box::new(move |_| session_daw::project_new::setlist_of(guid)),
            );
        } else if let Some(path) = session_daw::projects::take_next().or(launch) {
            open_path(path);
        }
    });

    // Task: signed in already, or not yet.
    let mut sign_in = use_signal(|| task_account::signed_in().map_or(SignIn::Out, SignIn::In));
    let mut orgs = use_signal(|| Load::<Vec<String>>::Waiting);
    let mut org = use_signal(|| None::<String>);
    let mut setlists = use_signal(|| Load::<Vec<LibrarySetlist>>::Waiting);
    let mut songs = use_signal(|| Load::<Vec<(LibrarySong, bool)>>::Waiting);
    // The list editor, over the page; leaving it reads the lists again.
    let mut editing = use_signal(|| None::<Editing>);
    // The song editor, over the page: `Some(None)` making one.
    let mut song_editing = use_signal(|| None::<Option<LibrarySong>>);
    let mut reread = use_signal(|| 0_u32);
    let account = match sign_in() {
        SignIn::In(account) => Some(account),
        _ => None,
    };
    let for_orgs = account.clone();
    use_effect(use_reactive!(|for_orgs| {
        let Some(account) = for_orgs else { return };
        orgs.set(Load::Waiting);
        spawn(async move {
            match off_thread(move || task_account::orgs(&account)).await {
                Some(Ok(list)) => {
                    if org.peek().is_none() {
                        org.set(list.first().cloned());
                    }
                    orgs.set(Load::Ready(list));
                }
                Some(Err(e)) => orgs.set(Load::Failed(brief(&e))),
                None => {}
            }
        });
    }));
    // The chosen org's lists and songs, as they change.
    let for_library = account.clone().zip(org()).map(|(a, o)| (a, o, reread()));
    use_effect(use_reactive!(|for_library| {
        let Some((account, org, _)) = for_library else {
            return;
        };
        setlists.set(Load::Waiting);
        songs.set(Load::Waiting);
        let library = account.library(&org);
        let for_songs = library.clone();
        spawn(async move {
            match off_thread(move || session_daw::stream_set::setlists(&library)).await {
                Some(Ok(list)) => setlists.set(Load::Ready(list)),
                Some(Err(e)) => setlists.set(Load::Failed(brief(&e))),
                None => {}
            }
        });
        spawn(async move {
            match off_thread(move || with_library(&for_songs, |l| async move { l.songs().await }))
                .await
            {
                Some(Ok(list)) => songs.set(Load::Ready(list)),
                Some(Err(e)) => songs.set(Load::Failed(brief(&e))),
                None => {}
            }
        });
    }));
    let start_sign_in = move |()| {
        sign_in.set(SignIn::Starting);
        spawn(async move {
            let server = task_account::server();
            let at = server.clone();
            let started = match off_thread(move || task_account::start(&at)).await {
                Some(Ok(started)) => started,
                Some(Err(e)) => {
                    sign_in.set(SignIn::Failed(brief(&e)));
                    return;
                }
                None => return,
            };
            super::open_url(&started.link());
            sign_in.set(SignIn::Code(started.clone()));
            match off_thread(move || task_account::finish(&server, &started)).await {
                Some(Ok(account)) => sign_in.set(SignIn::In(account)),
                Some(Err(e)) => sign_in.set(SignIn::Failed(brief(&e))),
                None => {}
            }
        });
    };

    let mut link = use_signal(String::new);
    let who = account.as_ref().map_or_else(guest_name, Account::name);
    // Drive a REAPER through its Session bridge, at `address`: Remote mode,
    // the set its open projects are — as `FTS_AUDIO_TARGET` does at launch.
    let mut connect_bridge = move |address: String| {
        remember_bridge(&address);
        open_set(
            Progress::Joining { retry: None },
            Box::new(move |_| {
                use session_daw::open::{ModeState, RemoteTarget};
                let target = RemoteTarget::Session { address };
                session_daw::open::set_mode(ModeState {
                    requested: session_daw::open::AudioMode::Remote,
                    target: Some(target.clone()),
                    ..ModeState::engine()
                });
                Setlist::attach(&target)
            }),
        );
    };
    let bridge = use_hook(remembered_bridge);
    // The set opened last, while it is still here.
    let last = use_hook(|| super::remembered().filter(|p| p.exists()));
    let join = {
        let who = who.clone();
        move |text: String| {
            // A REAPER's Session bridge (or a Session engine): drive it.
            if let Some(address) = engine_address(&text) {
                connect_bridge(address);
                return;
            }
            let Some(live) = live_link(&text) else {
                opening.set(Opening::Failed(
                    "that is not a Session live link".to_owned(),
                ));
                return;
            };
            let name = who.clone();
            open_set(
                Progress::Joining { retry: None },
                Box::new(move |progress| {
                    session_daw::stream_set::open(Remote::Live { link: live, name }, progress)
                }),
            );
        }
    };
    // `FTS_SESSION_LIVE=<link>` joins a live set as the window opens, as
    // `FTS_SESSION_PROJECT` opens a song (`demo` is the public demo).
    use_hook({
        let mut join = join.clone();
        move || {
            if let Some(link) = std::env::var("FTS_SESSION_LIVE")
                .ok()
                .filter(|l| !l.trim().is_empty())
            {
                join(if link.trim() == "demo" {
                    DEMO_LINK.to_owned()
                } else {
                    link
                });
            }
        }
    });
    let mut join_demo = join.clone();
    let mut join_link = join.clone();
    let mut join_pasted = join;

    // Streaming one of the library's lists in, as the member signed in.
    let play = use_callback(move |(account, setlist): (Account, LibrarySetlist)| {
        let Some(org) = org() else { return };
        let library = account.library(&org);
        let name = account.name();
        let first = setlist
            .songs
            .first()
            .map(|s| s.title.clone())
            .unwrap_or_default();
        open_set(
            Progress::Fetching {
                title: first,
                retry: None,
            },
            Box::new(move |progress| {
                session_daw::stream_set::open(
                    Remote::Library {
                        library,
                        setlist,
                        name,
                    },
                    progress,
                )
            }),
        );
    });

    // Downloading a list (or a song) onto this device, to open with no
    // connection: one at a time, its progress along the foot.
    let mut downloading = use_signal(|| None::<Downloading>);
    let mut note = use_signal(|| None::<(bool, String)>);
    let download = use_callback(
        move |(account, title, list, offline): (Account, String, Vec<LibrarySong>, Offline)| {
            let (Some(org), Some(into)) = (org(), documents_dir()) else {
                return;
            };
            if downloading.peek().is_some() {
                note.set(Some((
                    false,
                    "One download at a time — this one starts when the other is done.".to_owned(),
                )));
                return;
            }
            let library = account.library(&org);
            downloading.set(Some(Downloading {
                title: title.clone(),
                song: list.first().map(|s| s.title.clone()).unwrap_or_default(),
                index: 0,
                count: list.len(),
                done: 0,
                total: 0,
            }));
            note.set(None);
            let (tell, mut heard) = tokio::sync::mpsc::unbounded_channel::<Downloading>();
            spawn(async move {
                while let Some(step) = heard.recv().await {
                    if downloading.peek().is_some() {
                        downloading.set(Some(step));
                    }
                }
            });
            spawn(async move {
                let named = title.clone();
                let outcome = off_thread(move || {
                    session_daw::stream_set::download(
                        &library,
                        &named,
                        &list,
                        &into,
                        offline,
                        &|step| {
                            drop(tell.send(step));
                        },
                    )
                })
                .await;
                downloading.set(None);
                match outcome {
                    Some(Ok(done)) if done.failed.is_empty() => {
                        note.set(Some((
                            true,
                            format!("{title} is on this device — it opens with no connection."),
                        )));
                        local.set(documents());
                    }
                    Some(Ok(done)) => {
                        // Part of it: said, with which songs and why.
                        let names: Vec<&str> =
                            done.failed.iter().map(|(song, _)| song.as_str()).collect();
                        let why = done
                            .failed
                            .first()
                            .map(|(_, why)| why.clone())
                            .unwrap_or_default();
                        note.set(Some((
                            false,
                            format!(
                                "{title} is on this device without {} of its songs ({}) — {why}",
                                names.len(),
                                names.join(", "),
                            ),
                        )));
                        local.set(documents());
                    }
                    Some(Err(e)) => note.set(Some((
                        false,
                        format!("{title} did not download — {}", brief(&e)),
                    ))),
                    None => {}
                }
            });
        },
    );
    let remove = use_callback(move |path: PathBuf| {
        let Some(into) = documents_dir() else { return };
        match session_daw::stream_set::remove_download(&into, &path) {
            Ok(()) => local.set(documents()),
            Err(e) => note.set(Some((
                false,
                format!("Could not remove it — {}", brief(&e)),
            ))),
        }
    });

    // Opening: the loading screen, whole.
    if let Opening::Busy(progress) = opening() {
        return rsx! { Loading { progress } };
    }
    // The song editor, whole; leaving it reads the songs again.
    if let Some(song) = song_editing()
        && let (Some(account), Some(slug)) = (account.clone(), org())
    {
        return rsx! {
            super::library::SongEditor {
                library: account.library(&slug),
                song,
                on_done: move |()| {
                    song_editing.set(None);
                    reread += 1;
                },
            }
        };
    }
    // The list editor, whole.
    if let Some(Editing { open, create }) = editing()
        && let (Some(account), Some(slug)) = (account.clone(), org())
    {
        let lists = match setlists() {
            Load::Ready(list) => list,
            _ => Vec::new(),
        };
        return rsx! {
            super::library::LibraryEditor {
                library: account.library(&slug),
                lists,
                open,
                create_first: create,
                on_back: move |()| {
                    editing.set(None);
                    reread += 1;
                },
                on_play: move |setlist: LibrarySetlist| {
                    editing.set(None);
                    play.call((account.clone(), setlist));
                },
            }
        };
    }

    let wide = width() >= 760.0;
    let here = section();
    // Signing in, as a card: where the library would be.
    let sign_in_card = rsx! {
        match sign_in() {
            SignIn::Out | SignIn::Failed(_) => rsx! {
                div {
                    style: "display:flex; flex-wrap:wrap; align-items:center; gap:16px; padding:20px; border-radius:20px; \
                            background:linear-gradient(120deg, #1d3357 0%, {BAR} 70%);",
                    Art { seed: "library".to_owned(), size: 60, radius: 16, LibraryBig { size: 28, color: ART_INK } }
                    div {
                        style: "flex:1 1 220px; min-width:0; display:flex; flex-direction:column; gap:4px;",
                        span { style: "font-size:18px; font-weight:750; letter-spacing:-0.01em;", "Bring your library" }
                        span { style: "font-size:13px; color:#a3aab4; line-height:1.45;", "Sign in with FastTrackStudio to play your org's setlists, make lists, and keep them on this device." }
                        if let SignIn::Failed(why) = sign_in() {
                            span { style: "font-size:12px; color:{WARN}; line-height:1.4;", "Not signed in — {why}" }
                        }
                    }
                    Pill { label: "Sign in", primary: true, on_press: start_sign_in }
                }
            },
            SignIn::Starting => rsx! {
                Card { span { style: "font-size:14px; color:{DIM};", "Getting a sign-in code…" } }
            },
            SignIn::Code(started) => rsx! {
                Card {
                    div {
                        style: "display:flex; flex-direction:column; align-items:center; gap:12px; padding:6px 0;",
                        span { style: "font-size:14px; color:{DIM};", "Approve this code in your browser" }
                        CodeBoxes { code: started.code.user_code.clone() }
                        span { style: "font-size:12px; color:#6b7280;", "Waiting for approval…" }
                        Pill {
                            label: "Open the page again",
                            primary: false,
                            on_press: move |()| super::open_url(&started.link()),
                        }
                    }
                }
            },
            SignIn::In(_) => rsx! {},
        }
    };
    let signed_in = account.is_some();
    let lists_now = match setlists() {
        Load::Ready(list) => list,
        _ => Vec::new(),
    };
    let into = documents_dir();
    let downloaded = move |title: &str| {
        into.as_ref()
            .is_some_and(|dir| session_daw::stream_set::download_file(dir, title).exists())
    };

    // What the section shows.
    let content = match here {
        Section::Home => rsx! {
            if let Opening::Failed(why) = opening() {
                Banner { good: false, text: format!("Could not open it — {why}") }
            }
            // The way in: back to the last set, or into the live demo.
            div {
                style: "display:flex; flex-wrap:wrap; gap:14px;",
                if let Some(path) = last.clone() {
                    Hero {
                        kicker: "Jump back in",
                        title: title_of(&path),
                        detail: kind_of(&path),
                        seed: title_of(&path),
                        live: false,
                        on_press: move |()| open_path(path.clone()),
                        FileMusic { size: 28, color: ART_INK }
                    }
                }
                Hero {
                    kicker: "Live now",
                    title: "The Session demo".to_owned(),
                    detail: "Play along with the set — chart, lyrics, click.".to_owned(),
                    seed: "demo".to_owned(),
                    live: true,
                    on_press: move |()| join_demo(DEMO_LINK.to_owned()),
                    Radio { size: 30, color: ART_INK }
                }
            }
            // A song from nothing.
            Shortcut {
                title: "New Project".to_owned(),
                detail: "An untitled song, in Organize, the click ready".to_owned(),
                on_press: move |()| new_project(),
                Plus { size: 18, color: ACCENT }
            }
            // Joining by link: a set someone shared, or a REAPER's bridge.
            div {
                style: "display:flex; gap:8px; align-items:center; height:50px; padding:0 6px 0 14px; \
                        border-radius:14px; background:{BAR};",
                Link { size: 18, color: DIM }
                // The hint under the field while it is empty: Blitz
                // draws no `placeholder`.
                div {
                    style: "position:relative; flex:1; min-width:0; height:40px;",
                    if link().is_empty() {
                        span {
                            style: "position:absolute; top:0; left:4px; height:40px; display:flex; \
                                    align-items:center; font-size:15px; color:#6b7280; pointer-events:none; \
                                    white-space:nowrap; overflow:hidden;",
                            "Join with a link"
                        }
                    }
                    input {
                        style: "position:absolute; top:0; left:0; width:100%; height:40px; box-sizing:border-box; \
                                padding:0 4px; border:none; background:transparent; color:{TEXT}; \
                                font-family:inherit; font-size:15px;",
                        r#type: "text",
                        value: "{link}",
                        oninput: move |e| link.set(e.value()),
                    }
                }
                if cfg!(target_os = "ios") && link().trim().is_empty() {
                    Pill {
                        label: "Paste",
                        primary: false,
                        on_press: move |()| {
                            if let Some(text) = super::pasted() {
                                link.set(text.clone());
                                join_pasted(text);
                            }
                        },
                    }
                } else {
                    Pill {
                        label: "Join",
                        primary: !link().trim().is_empty(),
                        on_press: move |()| {
                            if !link().trim().is_empty() {
                                join_link(link());
                            }
                        },
                    }
                }
            }
            // The REAPER this device drove last, through its bridge.
            if let Some(address) = bridge.clone() {
                Shortcut {
                    title: "Reconnect to REAPER".to_owned(),
                    detail: address.clone(),
                    on_press: move |()| connect_bridge(address.clone()),
                    Radio { size: 18, color: ACCENT }
                }
            }
            // The library's setlists.
            if signed_in {
                Shelf { title: "Your setlists", on_more: move |()| section.set(Section::Setlists),
                    match setlists() {
                        Load::Waiting => rsx! { Quiet { text: "Finding your setlists…" } },
                        Load::Failed(why) => rsx! { Banner { good: false, text: format!("Could not read the library — {why}") } },
                        Load::Ready(_) => rsx! {
                            Covers {
                                for list in lists_now.iter().filter(|l| l.kind == ListKind::Set).take(8).cloned() {
                                    Cover {
                                        key: "{list.id}",
                                        title: list.title.clone(),
                                        detail: format!("{} songs", list.songs.len()),
                                        seed: list.title.clone(),
                                        saved: downloaded(&list.title),
                                        on_press: {
                                            let account = account.clone();
                                            move |()| if let Some(account) = account.clone() { play.call((account, list.clone())) }
                                        },
                                        ListMusic { size: 34, color: ART_INK }
                                    }
                                }
                                Cover {
                                    title: "New setlist".to_owned(),
                                    detail: "From your song lists".to_owned(),
                                    seed: String::new(),
                                    saved: false,
                                    on_press: move |()| editing.set(Some(Editing { open: None, create: Some(ListKind::Set) })),
                                    Plus { size: 34, color: DIM }
                                }
                            }
                        },
                    }
                }
            } else {
                {sign_in_card.clone()}
            }
            // The songs worked on lately: the song, or any of its
            // checkpoints.
            if !recent.is_empty() {
                Shelf { title: "Recent",
                    Rows {
                        for song in recent.iter().take(8).cloned() {
                            RecentRow {
                                key: "{song.base.display()}",
                                song: song.clone(),
                                on_open: move |path: PathBuf| open_path(path),
                            }
                        }
                    }
                }
            }
            // What opens with no connection.
            if !local().is_empty() {
                Shelf { title: "On this device", on_more: move |()| section.set(Section::Downloads),
                    Covers {
                        for path in local().into_iter().take(8) {
                            Cover {
                                key: "{path.display()}",
                                title: title_of(&path),
                                detail: kind_of(&path),
                                seed: title_of(&path),
                                saved: true,
                                on_press: {
                                    let path = path.clone();
                                    move |()| open_path(path.clone())
                                },
                                if kind_of(&path) == "Setlist" {
                                    ListMusic { size: 34, color: ART_INK }
                                } else {
                                    Music { size: 34, color: ART_INK }
                                }
                            }
                        }
                    }
                }
            }
        },
        Section::Setlists | Section::Lists => {
            let kind = if here == Section::Setlists {
                ListKind::Set
            } else {
                ListKind::Songs
            };
            rsx! {
                if signed_in {
                    match setlists() {
                        Load::Waiting => rsx! { Quiet { text: "Finding your lists…" } },
                        Load::Failed(why) => rsx! { Banner { good: false, text: format!("Could not read the library — {why}") } },
                        Load::Ready(_) => rsx! {
                            Rows {
                                if !lists_now.iter().any(|l| l.kind == kind) {
                                                                        Quiet { text: none_yet(kind) }
                                }
                                for list in lists_now.iter().filter(|l| l.kind == kind).cloned() {
                                    ListRow2 {
                                        key: "{list.id}",
                                        title: list.title.clone(),
                                        detail: songs_line(&list),
                                        count: list.songs.len(),
                                        downloaded: downloaded(&list.title),
                                        busy: downloading().is_some_and(|d| d.title == list.title),
                                        on_play: {
                                            let (account, list) = (account.clone(), list.clone());
                                            move |()| if let Some(account) = account.clone() { play.call((account, list.clone())) }
                                        },
                                        on_download: {
                                            let (account, list) = (account.clone(), list.clone());
                                            move |offline: Offline| if let Some(account) = account.clone() { download.call((account, list.title.clone(), list.songs.clone(), offline)) }
                                        },
                                        on_edit: {
                                            let id = list.id.clone();
                                            move |()| editing.set(Some(Editing { open: Some(id.clone()), create: None }))
                                        },
                                    }
                                }
                            }
                        },
                    }
                } else {
                    {sign_in_card.clone()}
                }
            }
        }
        Section::Songs => rsx! {
            if signed_in {
                SongsList {
                    songs: songs(),
                    downloaded: move |title: String| downloaded(&title),
                    on_play: {
                        let account = account.clone();
                        move |song: LibrarySong| if let Some(account) = account.clone() {
                            play.call((account, LibrarySetlist {
                                id: format!("song-{}", song.slug),
                                title: song.title.clone(),
                                kind: ListKind::Songs,
                                songs: vec![song],
                            }));
                        }
                    },
                    on_download: {
                        let account = account.clone();
                        move |(song, offline): (LibrarySong, Offline)| if let Some(account) = account.clone() {
                            download.call((account, song.title.clone(), vec![song], offline));
                        }
                    },
                    on_edit: move |song: LibrarySong| song_editing.set(Some(Some(song))),
                }
            } else {
                {sign_in_card.clone()}
            }
        },
        Section::Downloads => rsx! {
            Quiet { text: LOCAL_HINT }
            Rows {
                if local().is_empty() {
                    Quiet { text: LOCAL_EMPTY }
                }
                for path in local() {
                    LocalRow {
                        key: "{path.display()}",
                        title: title_of(&path),
                        detail: kind_of(&path),
                        removable: path.extension().is_some_and(|e| e.eq_ignore_ascii_case("setlist")),
                        on_open: {
                            let path = path.clone();
                            move |()| open_path(path.clone())
                        },
                        on_remove: {
                            let path = path.clone();
                            move |()| remove.call(path.clone())
                        },
                    }
                }
                if cfg!(not(target_os = "ios")) {
                    LocalRow {
                        title: "Open a song or setlist…".to_owned(),
                        detail: "A REAPER project, a .session, or a .setlist".to_owned(),
                        removable: false,
                        on_open: move |()| {
                            if let Some(path) = super::pick() {
                                open_path(path);
                            }
                        },
                        on_remove: move |()| {},
                    }
                }
            }
        },
    };
    // The section's own action, beside its title.
    let action = match here {
        Section::Setlists | Section::Lists if signed_in => {
            let kind = if here == Section::Setlists {
                ListKind::Set
            } else {
                ListKind::Songs
            };
            rsx! {
                Pill {
                    label: if kind == ListKind::Set { "New setlist" } else { "New song list" },
                    primary: true,
                    on_press: move |()| editing.set(Some(Editing { open: None, create: Some(kind) })),
                }
            }
        }
        Section::Songs if signed_in => rsx! {
            Pill { label: "New song", primary: true, on_press: move |()| song_editing.set(Some(None)) }
        },
        _ => rsx! {},
    };
    let account_line = account
        .as_ref()
        .map(|a| a.session.email.clone().unwrap_or_else(|| a.name()));
    let head_top = if wide { TOP } else { TOP.max(12) };

    // The main column, beside the sidebar or over the tab bar.
    let main = rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;",
            // The section's title, its action — and, on a phone, who is
            // signed in (the sidebar carries it on a wide window).
            div {
                style: "position:relative; z-index:5; flex:none; display:flex; align-items:center; gap:12px; \
                        padding:{head_top}px 24px 12px; background:{BG};",
                if !wide {
                    img { src: mark_src(), width: "30", height: "30", style: "border-radius:8px; flex:none;" }
                }
                span { style: "flex:1; min-width:0; font-size:26px; font-weight:800; letter-spacing:-0.02em;", "{here.label()}" }
                {action}
                if !wide {
                    OrgSwitcher {
                        email: account_line.clone(),
                        orgs: orgs(),
                        org,
                        compact: true,
                        on_sign_in: start_sign_in,
                        on_sign_out: move |()| {
                            task_account::sign_out();
                            sign_in.set(SignIn::Out);
                        },
                    }
                }
            }
            div {
                style: "position:relative; z-index:1; flex:1; min-height:0; overflow-y:auto;",
                div {
                    style: "box-sizing:border-box; width:100%; max-width:1100px; padding:8px 24px 40px; \
                            display:flex; flex-direction:column; gap:28px;",
                    {content}
                }
            }
            // A download under way, or how the last one went.
            DownloadBar { downloading: downloading(), note: note(), on_dismiss: move |()| note.set(None) }
            if !wide {
                div {
                    style: "position:relative; z-index:5; flex:none; height:56px; display:flex; align-items:stretch; \
                            background:{BAR}; border-top:1px solid {RULE};",
                    for each in Section::ALL {
                        button {
                            key: "{each.label()}",
                            style: tab_style(here == each),
                            onclick: move |_| section.set(each),
                            SectionIcon { section: each, color: if here == each { ACCENT } else { DIM } }
                            span { style: "font-size:10px; font-weight:600;", "{each.short()}" }
                        }
                    }
                }
            }
        }
    };
    // A wide window and a narrow one are separate frames, not one patched:
    // turning a phone from portrait to landscape added the sidebar before
    // the column, and Blitz left the column where it was — under it.
    let frame = |direction: &str| {
        format!(
            "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; \
             flex-direction:{direction}; overflow:hidden; background:{BG}; \
             color:{TEXT}; font-family:system-ui, -apple-system, sans-serif;"
        )
    };
    if wide {
        rsx! {
            div {
                style: frame("row"),
                // The sidebar: the mark, the sections, who is signed in.
                div {
                    style: "position:relative; z-index:5; flex:none; width:236px; display:flex; flex-direction:column; \
                            padding:{TOP}px 0 16px; box-sizing:border-box; background:{BAR};",
                    div {
                        style: "display:flex; align-items:center; gap:10px; padding:4px 18px 18px;",
                        img { src: mark_src(), width: "30", height: "30", style: "border-radius:8px; flex:none;" }
                        span { style: "font-size:18px; font-weight:750; letter-spacing:-0.01em;", "Session" }
                    }
                    for each in Section::ALL {
                        NavItem {
                            key: "{each.label()}",
                            section: each,
                            on: here == each,
                            on_press: move |()| section.set(each),
                        }
                    }
                    div { style: "flex:1;" }
                    div {
                        style: "display:flex; flex-direction:column; padding:10px 8px 0; border-top:1px solid {RULE};",
                        OrgSwitcher {
                            email: account_line.clone(),
                            orgs: orgs(),
                            org,
                            compact: false,
                            on_sign_in: start_sign_in,
                            on_sign_out: move |()| {
                                task_account::sign_out();
                                sign_in.set(SignIn::Out);
                            },
                        }
                    }
                }
                {main}
            }
        }
    } else {
        rsx! {
            div { style: frame("column"), {main} }
        }
    }
}

/// What a raised part of a bar is: the section showing.
const RAISED: &str = "#26292f";

/// A phone's tab: flat, the one showing raised.
fn tab_style(on: bool) -> String {
    let ink = if on { ACCENT } else { DIM };
    format!(
        "flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; \
         justify-content:center; gap:3px; border:none; padding:0; cursor:pointer; \
         font-family:inherit; background:transparent; color:{ink};"
    )
}

/// What a kind of list says when there is none of it.
const fn none_yet(kind: ListKind) -> &'static str {
    match kind {
        ListKind::Set => "No setlists yet — make one, and fill it from your song lists.",
        ListKind::Songs => {
            "No song lists yet — gather songs to pick setlists from: \"Worship Tracks\", \"Hymns\"."
        }
    }
}

/// A section's icon, in its colour outright (Blitz resolves an SVG's
/// `currentColor` once).
#[component]
fn SectionIcon(section: Section, color: &'static str) -> Element {
    let size = 20;
    match section {
        Section::Home => rsx! { House { size, color } },
        Section::Setlists => rsx! { ListMusic { size, color } },
        Section::Lists => rsx! { Library { size, color } },
        Section::Songs => rsx! { Music { size, color } },
        Section::Downloads => rsx! { HardDriveDownload { size, color } },
    }
}

/// A section in the sidebar: its icon and its name, the one showing
/// raised.
#[component]
fn NavItem(section: Section, on: bool, on_press: EventHandler<()>) -> Element {
    let (bg, ink) = if on {
        (RAISED, TEXT)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        button {
                        style: "display:flex; align-items:center; justify-content:flex-start; gap:12px; height:44px; \
                    margin:1px 10px; padding:0 12px; border:none; border-radius:10px; background:{bg}; color:{ink}; font-family:inherit; \
                    font-size:15px; font-weight:600; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            SectionIcon { section, color: if on { ACCENT } else { DIM } }
            "{section.label()}"
        }
    }
}

/// The ink over a cover's art.
const ART_INK: &str = "#ffffffe6";

/// A cover's two colours, picked by its name: the same list always the
/// same, neighbours told apart. None (no name) is the plain tile a new
/// list gets.
fn palette(seed: &str) -> (&'static str, &'static str) {
    const PAIRS: [(&str, &str); 8] = [
        ("#3b82f6", "#1e2f6b"),
        ("#8b5cf6", "#35155e"),
        ("#ec4899", "#57142f"),
        ("#f97316", "#5e2408"),
        ("#10b981", "#073f31"),
        ("#06b6d4", "#0b3b57"),
        ("#eab308", "#533706"),
        ("#ef4444", "#561010"),
    ];
    match seed {
        "" => return ("#1f2227", "#1a1c20"),
        // The ways in, told apart from each other whatever a set is called.
        "demo" => return PAIRS[0],
        "library" => return PAIRS[1],
        _ => {}
    }
    // FNV-1a: stable across runs and builds, unlike std's hasher.
    let hash = seed.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    PAIRS[hash as usize % PAIRS.len()]
}

/// A cover's art at a fixed size: its gradient, its icon over it.
#[component]
fn Art(seed: String, size: u32, radius: u32, children: Element) -> Element {
    let (from, to) = palette(&seed);
    rsx! {
        div {
            style: "flex:none; width:{size}px; height:{size}px; border-radius:{radius}px; display:flex; \
                    align-items:center; justify-content:center; \
                    background:linear-gradient(135deg, {from} 0%, {to} 100%);",
            {children}
        }
    }
}

/// The way in, large: its art, what it is, and a play button.
#[component]
fn Hero(
    kicker: &'static str,
    title: String,
    detail: String,
    seed: String,
    live: bool,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let (from, _) = palette(&seed);
    let kicker_ink = if live { ACCENT } else { DIM };
    rsx! {
        button {
            style: "flex:1 1 300px; min-width:0; display:flex; align-items:center; gap:16px; box-sizing:border-box; \
                    padding:14px; border:none; border-radius:20px; cursor:pointer; text-align:left; \
                    background:linear-gradient(120deg, {from}40 0%, {BAR} 65%); color:{TEXT}; font-family:inherit;",
            onclick: move |_| on_press.call(()),
            Art { seed: seed.clone(), size: 72, radius: 14, {children} }
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:5px;",
                div {
                    style: "display:flex; align-items:center; gap:7px;",
                    if live {
                        span { style: "flex:none; width:7px; height:7px; border-radius:4px; background:#ff4d4d;" }
                    }
                    span {
                        style: "font-size:11px; font-weight:750; letter-spacing:0.09em; color:{kicker_ink};",
                        "{kicker.to_uppercase()}"
                    }
                }
                span { style: "font-size:19px; font-weight:750; letter-spacing:-0.01em; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:13px; color:#a3aab4; line-height:1.4; overflow:hidden; max-height:37px;", "{detail}" }
            }
            div {
                style: "flex:none; width:46px; height:46px; border-radius:23px; display:flex; align-items:center; \
                        justify-content:center; background:{TEXT};",
                Play { size: 20, color: "#0b0c0e" }
            }
        }
    }
}

/// A shelf: its title, a way to all of it, and what is on it.
#[component]
fn Shelf(title: &'static str, on_more: Option<EventHandler<()>>, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:12px;",
            div {
                style: "display:flex; align-items:center; gap:12px;",
                span { style: "flex:1; min-width:0; font-size:20px; font-weight:750; letter-spacing:-0.01em;", "{title}" }
                if let Some(on_more) = on_more {
                    button {
                        style: "flex:none; display:flex; align-items:center; gap:2px; height:32px; padding:0 4px 0 10px; \
                                border:none; background:transparent; color:{ACCENT}; font-family:inherit; \
                                font-size:14px; font-weight:600; cursor:pointer;",
                        onclick: move |_| on_more.call(()),
                        "See all"
                        ChevronRight { size: 16, color: ACCENT }
                    }
                }
            }
            {children}
        }
    }
}

/// Covers, as many across as fit, each column the same width.
#[component]
fn Covers(children: Element) -> Element {
    rsx! {
        div {
            style: "display:grid; grid-template-columns:repeat(auto-fill, minmax(136px, 1fr)); gap:18px 14px;",
            {children}
        }
    }
}

/// A list or a song as a cover: square art, its name, a line under it —
/// and a mark when it is on this device.
#[component]
fn Cover(
    title: String,
    detail: String,
    seed: String,
    saved: bool,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let (from, to) = palette(&seed);
    rsx! {
        button {
            style: "min-width:0; display:flex; flex-direction:column; gap:8px; padding:0; border:none; \
                    background:transparent; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            div {
                style: "position:relative; width:100%; aspect-ratio:1 / 1; border-radius:14px; display:flex; \
                        align-items:center; justify-content:center; \
                        background:linear-gradient(135deg, {from} 0%, {to} 100%);",
                {children}
                if saved {
                    div {
                        style: "position:absolute; right:8px; bottom:8px; width:22px; height:22px; border-radius:11px; \
                                display:flex; align-items:center; justify-content:center; background:#0b0c0ecc;",
                        Check { size: 13, color: "#4ac26b" }
                    }
                }
            }
            div {
                style: "display:flex; flex-direction:column; gap:2px; min-width:0; padding:0 2px;",
                span { style: "font-size:14px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{detail}" }
            }
        }
    }
}

/// A one-line way in: an icon, what it does, where to.
#[component]
fn Shortcut(
    title: String,
    detail: String,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        button {
            style: "display:flex; align-items:center; gap:12px; height:56px; padding:0 14px; border:none; \
                    border-radius:14px; background:{BAR}; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            {children}
            span { style: "flex:none; font-size:15px; font-weight:650;", "{title}" }
            span { style: "flex:1; min-width:0; font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{detail}" }
            ChevronRight { size: 18, color: DIM }
        }
    }
}

/// An org's name as a person reads it: its slug, words capitalised
/// (`days-to-praise` → Days To Praise).
fn org_name(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The organization this library is, and who is signed in: at the
/// sidebar's foot (a phone's page head, `compact`: the org's badge
/// alone). Pressed, it opens a menu of every org this person is in — a
/// press switches the library to it — and signing out. Signed out, a
/// Sign in button.
#[component]
fn OrgSwitcher(
    email: Option<String>,
    orgs: Load<Vec<String>>,
    org: Signal<Option<String>>,
    compact: bool,
    on_sign_in: EventHandler<()>,
    on_sign_out: EventHandler<()>,
) -> Element {
    let mut org = org;
    let mut open = use_signal(|| false);
    let Some(email) = email else {
        return rsx! {
            Pill { label: "Sign in", primary: true, on_press: on_sign_in }
        };
    };
    let current = org().unwrap_or_default();
    let name = if current.is_empty() {
        "No organization".to_owned()
    } else {
        org_name(&current)
    };
    let list = match orgs {
        Load::Ready(list) => list,
        _ => Vec::new(),
    };
    // The menu opens up from the sidebar's foot, down from a page's head.
    let (anchor, align) = if compact {
        ("top:calc(100% + 6px);", "right:0;")
    } else {
        ("bottom:calc(100% + 6px);", "left:10px; right:10px;")
    };
    let width = if compact { "width:260px;" } else { "" };
    let trigger_style = if compact {
        "flex:none; padding:0; border:none; background:transparent; cursor:pointer;".to_owned()
    } else {
        format!(
            "width:100%; display:flex; align-items:center; gap:10px; padding:10px 12px; border:none; \
             border-radius:10px; background:transparent; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;"
        )
    };
    rsx! {
        div {
            style: "position:relative; flex:none;",
            if open() {
                div {
                    style: "{session_daw::shell::SCRIM} z-index:20;",
                    onclick: move |_| open.set(false),
                }
                div {
                    style: "position:absolute; {anchor} {align} {width} z-index:21; display:flex; flex-direction:column; \
                            padding:6px; border-radius:14px; background:#1c1e23; border:1px solid {RULE}; \
                            box-shadow:0 12px 32px #0009;",
                    span { style: "padding:8px 10px 6px; font-size:11px; font-weight:700; letter-spacing:0.08em; color:{DIM};", "ORGANIZATIONS" }
                    if list.is_empty() {
                        span { style: "padding:6px 10px 10px; font-size:13px; color:{DIM};", "Finding your organizations…" }
                    }
                    for slug in list {
                        button {
                            key: "{slug}",
                            style: "display:flex; align-items:center; gap:10px; min-height:44px; padding:0 10px; border:none; \
                                    border-radius:9px; background:transparent; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
                            onclick: {
                                let slug = slug.clone();
                                move |_| {
                                    org.set(Some(slug.clone()));
                                    open.set(false);
                                }
                            },
                            Avatar { email: org_name(&slug), size: 28 }
                            span { style: "flex:1; min-width:0; font-size:14px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{org_name(&slug)}" }
                            if slug == current {
                                Check { size: 16, color: ACCENT }
                            }
                        }
                    }
                    div { style: "height:1px; margin:6px 4px; background:{RULE};" }
                    span { style: "padding:4px 10px 2px; font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{email}" }
                    button {
                        style: "display:flex; align-items:center; gap:10px; min-height:40px; padding:0 10px; border:none; \
                                border-radius:9px; background:transparent; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
                        onclick: move |_| {
                            open.set(false);
                            on_sign_out.call(());
                        },
                        LogOut { size: 16, color: DIM }
                        span { style: "font-size:14px;", "Sign out" }
                    }
                }
            }
            button {
                title: "{name}",
                style: "{trigger_style}",
                onclick: move |_| open.set(!open()),
                Avatar { email: name.clone(), size: if compact { 34 } else { 32 } }
                if !compact {
                    div {
                        style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
                        span { style: "font-size:14px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{name}" }
                        span { style: "font-size:11px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{email}" }
                    }
                    ChevronsUpDown { size: 16, color: DIM }
                }
            }
        }
    }
}

/// Initials in a circle: who is signed in.
#[component]
fn Avatar(email: String, size: u32) -> Element {
    let (from, to) = palette(&email);
    let letter = email
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let font = size * 2 / 5;
    let radius = size / 2;
    rsx! {
        div {
            style: "flex:none; width:{size}px; height:{size}px; border-radius:{radius}px; display:flex; \
                    align-items:center; justify-content:center; font-size:{font}px; font-weight:750; color:#fff; \
                    background:linear-gradient(135deg, {from} 0%, {to} 100%);",
            "{letter}"
        }
    }
}

/// Rows between hairlines.
#[component]
fn Rows(children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; border-top:1px solid {RULE};",
            {children}
        }
    }
}

/// A quiet line of text.
#[component]
fn Quiet(text: &'static str) -> Element {
    rsx! {
        span { style: "font-size:13px; color:{DIM}; line-height:1.5; padding:6px 2px;", "{text}" }
    }
}

/// A message: something went well, or did not.
#[component]
fn Banner(good: bool, text: String) -> Element {
    let (bg, border, ink) = if good {
        ("#12261a", "#1f4a2e", "#4ac26b")
    } else {
        ("#2a1f12", "#5b4219", WARN)
    };
    rsx! {
        div {
            style: "display:flex; gap:10px; align-items:flex-start; padding:12px 14px; border-radius:12px; \
                    background:{bg}; border:1px solid {border}; color:{ink}; font-size:13px; line-height:1.45;",
            if good {
                CircleCheck { size: 18, color: ink }
            } else {
                CircleAlert { size: 18, color: ink }
            }
            span { style: "flex:1; min-width:0;", "{text}" }
        }
    }
}

/// A flat icon button a finger can hit, its icon in the given colour.
#[component]
fn Act(title: &'static str, on_press: EventHandler<()>, children: Element) -> Element {
    rsx! {
        button {
            title,
            style: "flex:none; width:44px; height:44px; display:flex; align-items:center; justify-content:center; \
                    border:none; border-radius:10px; background:transparent; cursor:pointer; padding:0;",
            onclick: move |_| on_press.call(()),
            {children}
        }
    }
}

/// A list of the library as a row: what it is, and playing it,
/// downloading it, editing it.
#[component]
fn ListRow2(
    title: String,
    detail: String,
    count: usize,
    downloaded: bool,
    busy: bool,
    on_play: EventHandler<()>,
    on_download: EventHandler<Offline>,
    on_edit: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:14px; min-height:68px; padding:8px 4px; border-bottom:1px solid {RULE};",
            button {
                style: "flex:1; min-width:0; display:flex; align-items:center; gap:14px; border:none; background:transparent; \
                        color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer; padding:0;",
                onclick: move |_| on_play.call(()),
                Art { seed: title.clone(), size: 48, radius: 10, ListMusic { size: 22, color: ART_INK } }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                    div {
                        style: "display:flex; align-items:center; gap:8px; min-width:0;",
                        span { style: "font-size:16px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                        span { style: "flex:none; font-size:12px; color:{DIM};", "{count} songs" }
                        if downloaded {
                            span {
                                style: "flex:none; display:flex; align-items:center; gap:3px; font-size:11px; font-weight:650; color:#4ac26b;",
                                Check { size: 12, color: "#4ac26b" }
                                "On this device"
                            }
                        }
                    }
                    span { style: "font-size:12px; color:#6b7280; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{detail}" }
                }
            }
            Act { title: "Play", on_press: on_play, Play { size: 18, color: TEXT } }
            DownloadChoice { downloaded, busy, on_pick: on_download }
            Act { title: "Edit", on_press: on_edit, Pencil { size: 17, color: TEXT } }
        }
    }
}

/// Downloading, as a button: pressed, the two ways a song comes onto the
/// device — by its reference (small: what a phone plays by default) or
/// every track (large: to change the mix).
#[component]
fn DownloadChoice(downloaded: bool, busy: bool, on_pick: EventHandler<Offline>) -> Element {
    let mut open = use_signal(|| false);
    let title = if downloaded {
        "Download again (to update it)"
    } else {
        "Download to this device"
    };
    let option = "display:flex; flex-direction:column; align-items:flex-start; gap:2px; padding:10px 12px; \
                  border:none; border-radius:9px; background:transparent; color:#e5e7eb; font-family:inherit; \
                  text-align:left; cursor:pointer;";
    rsx! {
        div {
            style: "position:relative; flex:none;",
            Act {
                title,
                on_press: move |()| open.set(!open()),
                if busy {
                    HardDriveDownload { size: 18, color: ACCENT }
                } else if downloaded {
                    CircleCheck { size: 18, color: "#4ac26b" }
                } else {
                    CloudDownload { size: 18, color: TEXT }
                }
            }
            if open() {
                div {
                    style: "{session_daw::shell::SCRIM} z-index:20;",
                    onclick: move |_| open.set(false),
                }
                div {
                    style: "position:absolute; top:calc(100% + 4px); right:0; z-index:21; width:270px; display:flex; \
                            flex-direction:column; padding:6px; border-radius:14px; background:#1c1e23; \
                            border:1px solid {RULE}; box-shadow:0 12px 32px #0009;",
                    button {
                        style: "{option}",
                        onclick: move |_| {
                            open.set(false);
                            on_pick.call(Offline::Reference);
                        },
                        span { style: "font-size:14px; font-weight:650;", "Reference mix" }
                        span { style: "font-size:12px; color:{DIM}; line-height:1.4;", "The stereo mix, with the click and guide live over it. A few MB a song." }
                    }
                    button {
                        style: "{option}",
                        onclick: move |_| {
                            open.set(false);
                            on_pick.call(Offline::Multitracks);
                        },
                        span { style: "font-size:14px; font-weight:650;", "Multitracks" }
                        span { style: "font-size:12px; color:{DIM}; line-height:1.4;", "Every track, to change the mix. About 100 MB a song." }
                    }
                }
            }
        }
    }
}

/// Every song in the library, searched: each to play, or to download.
#[component]
fn SongsList(
    songs: Load<Vec<(LibrarySong, bool)>>,
    downloaded: Callback<String, bool>,
    on_play: EventHandler<LibrarySong>,
    on_download: EventHandler<(LibrarySong, Offline)>,
    on_edit: EventHandler<LibrarySong>,
) -> Element {
    let mut query = use_signal(String::new);
    let wanted = query().trim().to_lowercase();
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px; height:44px; padding:0 12px; border-radius:12px; \
                    background:{BAR}; border:1px solid {RULE};",
            Search { size: 17, color: DIM }
            div {
                style: "position:relative; flex:1; min-width:0; height:42px;",
                if query().is_empty() {
                    span {
                        style: "position:absolute; top:0; left:2px; height:42px; display:flex; align-items:center; \
                                font-size:15px; color:#6b7280; pointer-events:none;",
                        "Search songs, keys, writers"
                    }
                }
                input {
                    style: "position:absolute; top:0; left:0; width:100%; height:42px; box-sizing:border-box; padding:0 2px; \
                            border:none; background:transparent; color:{TEXT}; font-family:inherit; font-size:15px;",
                    r#type: "text",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
            }
        }
        match songs {
            Load::Waiting => rsx! { Quiet { text: "Reading the library…" } },
            Load::Failed(why) => rsx! { Banner { good: false, text: format!("Could not read the songs — {why}") } },
            Load::Ready(list) => rsx! {
                Rows {
                    for (song, playable) in list.into_iter().filter(|(s, _)| {
                        wanted.is_empty()
                            || s.title.to_lowercase().contains(&wanted)
                            || s.key.to_lowercase() == wanted
                            || s.writers.iter().any(|w| w.to_lowercase().contains(&wanted))
                    }) {
                        div {
                            key: "{song.slug}",
                            style: "display:flex; align-items:center; gap:12px; min-height:60px; padding:6px 4px; border-bottom:1px solid {RULE};",
                            div {
                                style: "flex:none; width:40px; height:40px; border-radius:10px; display:flex; align-items:center; \
                                        justify-content:center; background:#1c1f25; font-size:13px; font-weight:700; color:{TEXT};",
                                if song.key.is_empty() { Music { size: 18, color: DIM } } else { "{song.key}" }
                            }
                            div {
                                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
                                span { style: "font-size:15px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{song.title}" }
                                div {
                                    style: "display:flex; gap:8px; font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden;",
                                    if !playable {
                                        span { style: "flex:none; color:{WARN};", "No session yet" }
                                    }
                                    if downloaded.call(song.title.clone()) {
                                        span { style: "flex:none; color:#4ac26b;", "On this device" }
                                    }
                                    span { style: "overflow:hidden; text-overflow:ellipsis;", "{song.writers.join(\", \")}" }
                                }
                            }
                            if playable {
                                Act {
                                    title: "Play",
                                    on_press: {
                                        let song = song.clone();
                                        move |()| on_play.call(song.clone())
                                    },
                                    Play { size: 18, color: TEXT }
                                }
                                DownloadChoice {
                                    downloaded: false,
                                    busy: false,
                                    on_pick: {
                                        let song = song.clone();
                                        move |offline: Offline| on_download.call((song.clone(), offline))
                                    },
                                }
                            }
                            Act {
                                title: "Edit the song",
                                on_press: {
                                    let song = song.clone();
                                    move |()| on_edit.call(song.clone())
                                },
                                Pencil { size: 17, color: TEXT }
                            }
                        }
                    }
                }
            },
        }
    }
}

/// Something on this device: opening it, and taking a download off.
#[component]
fn LocalRow(
    title: String,
    detail: String,
    removable: bool,
    on_open: EventHandler<()>,
    on_remove: EventHandler<()>,
) -> Element {
    let mut asking = use_signal(|| false);
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:14px; min-height:64px; padding:6px 4px; border-bottom:1px solid {RULE};",
            button {
                style: "flex:1; min-width:0; display:flex; align-items:center; gap:14px; border:none; background:transparent; \
                        color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer; padding:0;",
                onclick: move |_| on_open.call(()),
                Art { seed: title.clone(), size: 48, radius: 10, HardDriveDownload { size: 20, color: ART_INK } }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                    span { style: "font-size:16px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                    span { style: "font-size:12px; color:{DIM};", "{detail}" }
                }
            }
            if removable {
                if asking() {
                    Pill { label: "Remove", primary: false, on_press: move |()| { asking.set(false); on_remove.call(()); } }
                    Pill { label: "Keep", primary: false, on_press: move |()| asking.set(false) }
                } else {
                    Act { title: "Remove from this device", on_press: move |()| asking.set(true), Trash2 { size: 17, color: DIM } }
                }
            }
            Act { title: "Open", on_press: on_open, ChevronRight { size: 18, color: DIM } }
        }
    }
}

/// A song opened lately: its name, which version was open and when, and
/// a pill for each version to open — the song itself and every
/// checkpoint saved beside it (`session_daw::projects`).
#[component]
fn RecentRow(song: session_daw::projects::Recent, on_open: EventHandler<PathBuf>) -> Element {
    let name = song.name();
    let alternatives = session_daw::projects::alternatives(&song.base);
    let main = session_daw::projects::main_of(&song.base);
    let detail = match (song.is_song(), song.opened_alternative()) {
        (_, Some(alt)) => format!("{alt} · {}", session_daw::projects::ago(song.at)),
        (true, None) => format!("Song · {}", session_daw::projects::ago(song.at)),
        (false, None) => format!(
            "{} · {}",
            kind_of(&song.opened),
            session_daw::projects::ago(song.at)
        ),
    };
    let last = song.opened.clone();
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:8px; padding:10px 4px; border-bottom:1px solid {RULE};",
            button {
                style: "display:flex; align-items:center; gap:14px; border:none; background:transparent; \
                        color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer; padding:0;",
                onclick: move |_| on_open.call(last.clone()),
                Art { seed: name.clone(), size: 48, radius: 10, FileMusic { size: 20, color: ART_INK } }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                    span { style: "font-size:16px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{name}" }
                    span { style: "font-size:12px; color:{DIM};", "{detail}" }
                }
                ChevronRight { size: 18, color: DIM }
            }
            if !alternatives.is_empty() {
                div {
                    style: "display:flex; flex-wrap:wrap; gap:8px; padding-left:62px;",
                    if let Some(main) = main.clone() {
                        Chip {
                            label: "The song".to_owned(),
                            on: song.opened_alternative().is_none(),
                            on_press: move |()| on_open.call(main.clone()),
                        }
                    }
                    for alt in alternatives {
                        Chip {
                            key: "{alt.name}",
                            label: alt.name.clone(),
                            on: song.opened_alternative().as_deref() == Some(alt.name.as_str()),
                            on_press: move |()| on_open.call(alt.path.clone()),
                        }
                    }
                }
            }
        }
    }
}

/// A download under way — its set, which song of how many, how far — or
/// how the last one went, along the foot.
#[component]
fn DownloadBar(
    downloading: Option<Downloading>,
    note: Option<(bool, String)>,
    on_dismiss: EventHandler<()>,
) -> Element {
    if let Some(d) = downloading {
        #[expect(clippy::cast_precision_loss, reason = "a fraction for a bar")]
        let song = if d.total == 0 {
            0.0
        } else {
            d.done as f64 / d.total as f64
        };
        #[expect(clippy::cast_precision_loss, reason = "a fraction for a bar")]
        let whole = if d.count == 0 {
            0.0
        } else {
            (d.index as f64 + song) / d.count as f64
        };
        let percent = (whole * 100.0).clamp(0.0, 100.0);
        #[expect(clippy::cast_precision_loss, reason = "megabytes for a person")]
        let size = if d.total > 0 {
            format!(
                " · {:.0} of {:.0} MB",
                d.done as f64 / 1e6,
                d.total as f64 / 1e6
            )
        } else {
            String::new()
        };
        return rsx! {
            div {
                style: "position:relative; z-index:6; flex:none; display:flex; flex-direction:column; gap:8px; \
                        padding:12px 20px; background:{BAR}; border-top:1px solid {RULE};",
                div {
                    style: "display:flex; align-items:center; gap:10px; font-size:13px;",
                    HardDriveDownload { size: 16, color: ACCENT }
                    span { style: "font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "Downloading {d.title}" }
                    span { style: "flex:1; min-width:0; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                        "{d.song} — song {d.index + 1} of {d.count}{size}"
                    }
                }
                div {
                    style: "height:4px; border-radius:2px; background:#23262c; overflow:hidden;",
                    div { style: "width:{percent}%; height:4px; background:{ACCENT};" }
                }
            }
        };
    }
    let Some((good, text)) = note else {
        return rsx! {};
    };
    let ink = if good { "#4ac26b" } else { WARN };
    rsx! {
        div {
            style: "position:relative; z-index:6; flex:none; display:flex; align-items:center; gap:10px; \
                    padding:10px 20px; background:{BAR}; border-top:1px solid {RULE}; font-size:13px; \
                                        color:{ink};",
            span { style: "flex:1; min-width:0;", "{text}" }
            Act { title: "Dismiss", on_press: on_dismiss, X { size: 16, color: DIM } }
        }
    }
}

/// A setlist's songs, in a line.
fn songs_line(setlist: &LibrarySetlist) -> String {
    let titles: Vec<&str> = setlist.songs.iter().map(|s| s.title.as_str()).collect();
    if titles.is_empty() {
        "No songs".to_owned()
    } else {
        titles.join(" · ")
    }
}

/// The live link in `text`: a Task live share link as it is, the one a
/// Session app link carries (`…/app/?live=<link>`), or — for the site's
/// demo page (`…/demo`) — the demo's. Typed as people type them: with or
/// without `https://`.
fn live_link(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/');
    let bare = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(text);
    // A host and a path, at least: `name.tld/…`.
    let (host, path) = bare.split_once('/')?;
    if !host.contains('.') || host.contains(' ') {
        return None;
    }
    let url = if bare.len() == text.len() {
        format!("https://{text}")
    } else {
        text.to_owned()
    };
    if let Some(carried) = url
        .split(['?', '&'])
        .find_map(|pair| pair.strip_prefix("live="))
    {
        return Some(percent_decode(carried));
    }
    if path.split(['?', '#']).next() == Some("demo") {
        return Some(DEMO_LINK.to_owned());
    }
    Some(url)
}

/// The address of a REAPER's Session bridge (or a Session engine) in
/// `text`: a `ws://`/`wss://` URL, `fts-engine:<id>`, or a bare iroh id —
/// what the bridge shows when REAPER starts.
fn engine_address(text: &str) -> Option<String> {
    let text = text.trim();
    let bare_id = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
    (text.starts_with("ws://")
        || text.starts_with("wss://")
        || text.starts_with("fts-engine:")
        || bare_id)
        .then(|| text.to_owned())
}

/// Where the last bridge's address is kept.
fn bridge_memory() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("reaper-bridge"))
}

fn remember_bridge(address: &str) {
    let Some(file) = bridge_memory() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, address));
    if let Err(e) = written {
        tracing::warn!(error = %e, "start: the bridge's address could not be kept");
    }
}

fn remembered_bridge() -> Option<String> {
    let text = std::fs::read_to_string(bridge_memory()?).ok()?;
    engine_address(&text)
}

/// `%XX` escapes undone (a link riding in another's query).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&h), Some(&l)) if hex(h).is_some() && hex(l).is_some() => {
                out.push(u8::try_from(hex(h).unwrap_or(0) * 16 + hex(l).unwrap_or(0)).unwrap_or(0));
                i += 3;
            }
            (b'+', ..) => {
                out.push(b' ');
                i += 1;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A visitor's name in a set, when not signed in: Guest and four letters,
/// so two guests are told apart (as a page names one).
fn guest_name() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
        % 65_536;
    format!("Guest {n:04x}")
}

/// What the Downloads section says it holds.
#[cfg(target_os = "ios")]
const LOCAL_HINT: &str = "Sets and songs on this iPhone open with no connection: the ones you download from your library, and any you put in Files → On My iPhone → Session.";
#[cfg(not(target_os = "ios"))]
const LOCAL_HINT: &str = "Sets and songs on this computer open with no connection: the ones you download from your library, and any in Documents/Session.";

#[cfg(target_os = "ios")]
const LOCAL_EMPTY: &str = "Nothing here yet. Put a song folder or a setlist in Files → On My iPhone → Session, and it shows here.";
#[cfg(not(target_os = "ios"))]
const LOCAL_EMPTY: &str = "No songs in Documents/Session yet.";

/// The songs and setlists in the app's documents folder: each entry that is
/// a song folder, a setlist, or a project — newest first.
fn documents() -> Vec<PathBuf> {
    let Some(dir) = documents_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            p.is_dir() || matches!(ext.as_str(), "setlist" | "rpp")
        })
        .filter(|p| {
            // Hidden files, and the downloaded songs' own store — each set
            // downloaded is listed by its `.setlist` instead.
            !p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with('.')
                    || n == session_daw::stream_set::DOWNLOADED_SONGS
            })
        })
        .map(|p| {
            let at = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (at, p)
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Where the app keeps its songs: on a phone, its own Documents (what the
/// Files app shows as On My iPhone → Session); on a desktop, a Session
/// folder in the user's Documents.
fn documents_dir() -> Option<PathBuf> {
    let docs = dirs::document_dir()?;
    if cfg!(target_os = "ios") {
        Some(docs)
    } else {
        Some(docs.join("Session"))
    }
}

/// An entry's name, as a person reads it: a `.setlist` or a project
/// without its extension.
fn title_of(path: &std::path::Path) -> String {
    // A checkpoint is its song's: "Unbreakable · Organized".
    if let (Some(alt), Some(base)) = (
        session_daw::projects::alternative_name(path),
        session_daw::projects::base_of(path),
    ) {
        let song = base
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        return format!("{song} · {alt}");
    }
    let is_list = path.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("setlist")
            || e.eq_ignore_ascii_case("rpp")
            || e.eq_ignore_ascii_case("session")
    });
    let name = if is_list {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What an entry is, in a word or two.
fn kind_of(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "setlist" => "Setlist".to_owned(),
        "rpp" => "REAPER project".to_owned(),
        "session" => "Song".to_owned(),
        _ if session_daw::setlist::song_in(path).is_some() => "Song".to_owned(),
        _ => "Setlist".to_owned(),
    }
}

/// A section's heading.
#[component]
pub(super) fn Heading(label: String) -> Element {
    rsx! {
        span {
            style: "font-size:12px; font-weight:700; letter-spacing:0.08em; color:{DIM}; padding-left:4px;",
            "{label.to_uppercase()}"
        }
    }
}

/// A panel holding one thing.
#[component]
fn Card(children: Element) -> Element {
    rsx! {
        div {
            style: "padding:18px; border-radius:18px; background:{BAR};",
            {children}
        }
    }
}

/// A row of a list: a file, or the Open dialog.
#[component]
fn ListRow(first: bool, title: String, detail: String, on_press: EventHandler<()>) -> Element {
    let rule = if first {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    rsx! {
        button {
            style: "display:flex; align-items:center; gap:12px; width:100%; box-sizing:border-box; min-height:56px; padding:10px 14px; \
                    border:none; border-top:{rule}; background:transparent; color:{TEXT}; \
                    font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            div {
                style: "flex:none; color:{DIM}; display:flex;",
                FileMusic { size: 20, color: "currentColor" }
            }
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
                span { style: "font-size:15px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:12px; color:{DIM};", "{detail}" }
            }
            ChevronRight { size: 18, color: "#6b7280" }
        }
    }
}

/// A rounded button: the primary one filled.
#[component]
pub(super) fn Pill(label: String, primary: bool, on_press: EventHandler<()>) -> Element {
    let (fg, bg, border) = if primary {
        ("#0b0c0e", ACCENT, ACCENT)
    } else {
        (TEXT, "#23262c", RULE)
    };
    rsx! {
        button {
            style: "flex:none; height:38px; padding:0 16px; border-radius:19px; border:1px solid {border}; \
                    background:{bg}; color:{fg}; font-family:inherit; font-size:14px; font-weight:650; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

#[component]
pub(super) fn Chip(label: String, on: bool, on_press: EventHandler<()>) -> Element {
    let (fg, bg, border) = if on {
        (ACCENT, "#3aa0ff1f", "#3aa0ff66")
    } else {
        (DIM, "transparent", RULE)
    };
    rsx! {
        button {
            style: "height:32px; padding:0 14px; border-radius:16px; border:1px solid {border}; \
                    background:{bg}; color:{fg}; font-family:inherit; font-size:13px; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

/// A sign-in code, one box per character, as the approval page shows it.
#[component]
fn CodeBoxes(code: String) -> Element {
    rsx! {
        div {
            style: "display:flex; gap:6px; justify-content:center;",
            for (i, c) in code.chars().enumerate() {
                span {
                    key: "{i}",
                    style: "width:30px; height:40px; border-radius:8px; display:flex; align-items:center; \
                            justify-content:center; background:#0f1012; border:1px solid {RULE}; \
                            font-size:20px; font-weight:700; font-family:ui-monospace, monospace;",
                    "{c}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::live_link;

    #[test]
    fn a_live_link_is_taken_as_it_is_or_out_of_the_app_link_carrying_it() {
        let task = "https://task.example/org/band/share/abc";
        assert_eq!(live_link(task).as_deref(), Some(task));
        assert_eq!(
            live_link(" https://session.example/app/?live=https%3A%2F%2Ftask.example%2Forg%2Fband%2Fshare%2Fabc&name=Me ")
                .as_deref(),
            Some(task)
        );
        assert_eq!(live_link("not a link"), None);
        assert_eq!(
            super::engine_address(" ws://192.168.0.65:4040/vox ").as_deref(),
            Some("ws://192.168.0.65:4040/vox")
        );
        assert!(super::engine_address("fts-engine:1aa6").is_some());
        assert!(super::engine_address("https://task.example/org/band/share/abc").is_none());
        assert_eq!(live_link("example.com"), None);
    }

    #[test]
    fn a_link_typed_without_its_scheme_or_the_demo_page_is_understood() {
        assert_eq!(
            live_link("task.example/org/band/share/abc").as_deref(),
            Some("https://task.example/org/band/share/abc")
        );
        for demo in [
            "session.fasttrackstudio.app/demo",
            "https://session.fasttrackstudio.app/demo/",
        ] {
            assert_eq!(live_link(demo).as_deref(), Some(super::DEMO_LINK), "{demo}");
        }
    }
}
