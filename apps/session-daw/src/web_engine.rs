//! The engine in a browser: daw-standalone in-process, the session opened
//! from text.
//!
//! The native app opens a song from disk, stands up the `daw` facade on a
//! tokio runtime, and reads the session back ([`crate::open`],
//! `StudioSession::open`). A page has no disk and no threads, so it opens
//! the song the same one way from a memory folder
//! ([`crate::open_core::open_song_in`] over [`crate::folder::Memory`],
//! mirrored from a share link by [`crate::song_stream`]) and awaits
//! everything instead of blocking: `build_in_process_daw`,
//! `daw::init_from_parts`, then the same read-back and row plan
//! ([`crate::studio::Planner`]). The engine clients
//! ([`crate::engine::Applier`], [`crate::engine::Transport`]) have web
//! implementations that find the facade this installs.
//!
//! **Reference mode.** A shared song is heard by its reference by default
//! ([`crate::reference`]: its mix but the guide, one stream of a few MB),
//! the guide playing live over it — so joining a set on a phone does not
//! mean downloading every stem. Its takes are attached all the same (their
//! waveforms draw); their bytes are not fetched. A change to the mix needs
//! the stems ([`EngineRef::send`] asks instead, [`Notice::Blocked`]); once
//! they are asked for ([`load_multitracks`]) every song is heard by its
//! stems, each trading its reference for them the moment theirs have
//! arrived at the playhead ([`hear_stems`]).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use daw_standalone::audio_engine::media_fetch::StreamedTake;

use crate::song_stream::Heard;

use crate::studio::StudioSession;

/// The engine, as the page's panels reach it.
#[derive(Clone)]
pub struct EngineRef {
    applier: Rc<Option<crate::engine::Applier>>,
}

impl PartialEq for EngineRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.applier, &other.applier)
    }
}

impl EngineRef {
    /// Carry out an edit. Dropped (logged) with no engine — and, while the
    /// song is heard by its reference, a change to the mix of its stems is
    /// not made: the page is asked to offer the multitracks instead.
    pub fn send(&self, edit: crate::engine::Edit) {
        if needs_stems(&edit) {
            notify(Notice::Blocked);
            return;
        }
        match self.applier.as_ref() {
            Some(applier) => applier.send(edit),
            None => tracing::debug!(?edit, "no engine to carry out the edit"),
        }
    }

    /// Where the play cursor is, in seconds.
    #[must_use]
    pub fn position(&self) -> f64 {
        crate::engine::Transport::shared().map_or(0.0, |t| t.read().0)
    }
}

/// What the page opens.
#[derive(Clone, Debug, PartialEq)]
pub enum WebSource {
    /// A song shared by a Task share link: its files mirrored into memory,
    /// its proxies streamed by range, what will be heard first first.
    Shared { link: String },
    /// A copy bundled with the page: its project and chart by URL; silent.
    Bundled {
        name: String,
        rpp_url: String,
        chart_url: Option<String>,
    },
    /// A live share link to a set Task keeps (`live-proto`): the page joins
    /// it — its first song opened from that song's files link, everyone in
    /// the set in one session with it (`collab::join_task`), by `name`.
    Live { link: String, name: String },
}

pub use crate::loading::Progress;

pub use crate::setlist::Arrival;

/// A song the page will open, and where it comes from.
struct Wanted {
    title: Option<String>,
    from: Origin,
}

enum Origin {
    /// A Task share link to the song's session folder.
    Share(String),
    /// A copy bundled with the page.
    Bundled {
        name: String,
        rpp_url: String,
        chart_url: Option<String>,
    },
}

/// A song fetched for opening: the folder it sits in, its project's path
/// there, and — shared — its proxies to stream.
struct Fetched {
    folder: Arc<dyn crate::folder::Folder>,
    path: std::path::PathBuf,
    streamed: Option<crate::song_stream::StreamedSong>,
}

/// A song opened into the page's engine, before it is read back.
struct Loaded {
    title: String,
    project: String,
    rpp_text: String,
    chart_text: Option<String>,
    streamed: Option<crate::song_stream::StreamedSong>,
}

/// What the page keeps of each song it opened: its streamed media, started
/// the first time the song is picked ([`switch_song`]) — a set of seven
/// streams only the song on screen.
struct WebSong {
    streamed: Option<Rc<crate::song_stream::StreamedSong>>,
    previews: crate::midi::Previews,
    started: std::cell::Cell<bool>,
    /// Heard by its reference: the stems attached and held, not fetched —
    /// taken when they are ([`hear_stems`]).
    held: std::cell::RefCell<Option<Arc<std::sync::Mutex<Vec<StreamedTake>>>>>,
    /// Stops the reference's fetch, while it is heard by it.
    reference: std::cell::RefCell<Option<Arc<AtomicBool>>>,
}

impl WebSong {
    /// How the song is heard now.
    fn listening(&self) -> Listening {
        match (&*self.held.borrow(), self.reference.borrow().is_some()) {
            (Some(_), _) => Listening::Reference {
                stems_mb: self
                    .streamed
                    .as_ref()
                    .map_or(0, |s| s.stems_bytes().div_ceil(1 << 20)),
            },
            (None, true) => Listening::Loading,
            (None, false) => Listening::Stems,
        }
    }
}

/// How the song on screen is heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listening {
    /// By its reference; its stems (`stems_mb` of them) not loaded.
    Reference { stems_mb: u64 },
    /// Its stems on their way, the reference playing until they are here.
    Loading,
    /// By its stems: the mix is yours to change.
    Stems,
}

/// What the page is told, as it happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// How the song on screen is heard changed.
    Listening(Listening),
    /// A change to the mix was not made: the song is heard by its
    /// reference.
    Blocked,
}

struct Web {
    standalone: daw_standalone::sync::Standalone,
    songs: HashMap<String, WebSong>,
    /// The songs' projects in set order, as they opened — what "the next
    /// song" is.
    order: Vec<String>,
    /// Stops the cache warming started for the song last shown.
    warming: Arc<AtomicBool>,
}

thread_local! {
    static WEB: std::cell::RefCell<Option<Web>> = const { std::cell::RefCell::new(None) };
    static ARRIVALS: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>> =
        const { std::cell::RefCell::new(None) };
    /// Whether the multitracks were asked for: every song heard by its
    /// stems from then on.
    static MULTITRACKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NOTICES: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedSender<Notice>>> =
        const { std::cell::RefCell::new(None) };
}

/// What the page is told from here on ([`Notice`]) — once, for the view
/// that shows it.
pub fn notices() -> tokio::sync::mpsc::UnboundedReceiver<Notice> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    NOTICES.with(|n| *n.borrow_mut() = Some(tx));
    rx
}

fn notify(notice: Notice) {
    NOTICES.with(|n| {
        if let Some(tx) = n.borrow().as_ref() {
            let _ = tx.send(notice);
        }
    });
}

/// How the song on screen is heard.
#[must_use]
pub fn listening() -> Listening {
    WEB.with(|web| {
        let web = web.borrow();
        let web = web.as_ref()?;
        let current = crate::open::current_song()?;
        Some(web.songs.get(&current)?.listening())
    })
    .unwrap_or(Listening::Stems)
}

fn tell_listening() {
    notify(Notice::Listening(listening()));
}

/// Whether `edit` changes what the stems of a song heard by its reference
/// sound like — which they cannot, not being loaded. The tracks the
/// reference leaves out (the guide) play live, so are changed as ever.
fn needs_stems(edit: &crate::engine::Edit) -> bool {
    use crate::engine::Edit;
    let track = match edit {
        Edit::ToggleMute(t)
        | Edit::ToggleSolo(t)
        | Edit::SetVolume(t, _)
        | Edit::SetPan(t, _)
        | Edit::SetPhase(t, _)
        | Edit::SetParentSend(t, _)
        | Edit::AddSend(t, _)
        | Edit::RemoveRoute(t, _)
        | Edit::SetRouteVolume(t, ..)
        | Edit::SetRoutePan(t, ..)
        | Edit::SetRouteMute(t, ..)
        | Edit::SetSendMode(t, ..) => t,
        _ => return false,
    };
    WEB.with(|web| {
        let web = web.borrow();
        let Some(web) = web.as_ref() else {
            return false;
        };
        let Some(current) = crate::open::current_song() else {
            return false;
        };
        web.songs
            .get(&current)
            .is_some_and(|song| matches!(song.listening(), Listening::Reference { .. }))
            && !crate::reference::left_out(&web.standalone, &current).contains(track)
    })
}

/// Where this viewer's choice of the multitracks is kept, so a page opened
/// again (a playground set starting over, a reload) keeps it.
const MULTITRACKS_KEY: &str = "fts-session-multitracks";

/// Whether this viewer chose the multitracks before, on this browser.
fn chose_multitracks() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(MULTITRACKS_KEY).ok().flatten())
        .is_some()
}

/// Hear every song by its stems from now on, the one on screen first — the
/// answer to [`Notice::Blocked`].
pub fn load_multitracks() {
    MULTITRACKS.with(|m| m.set(true));
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(MULTITRACKS_KEY, "1");
    }
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        let Some(web) = web.as_mut() else { return };
        if let Some(current) = crate::open::current_song() {
            hear_stems(web, &current);
            warm_around(web, &current);
        }
    });
    tell_listening();
}

/// The set's other songs as they open — once, for the view that shows
/// them (see [`open`]).
pub fn take_arrivals() -> Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>> {
    ARRIVALS.with(|a| a.borrow_mut().take())
}

/// How long the song on screen streams alone before the set's other songs
/// start coming in behind it.
const NEXT_SONGS_AFTER_MS: u32 = 1_500;

/// Open the page the one way songs open (`open_core::open_song_in`, from
/// a memory folder), first song first: a live set is joined (and joined
/// again until Task answers — it may be restarting), its first song
/// fetched, opened, read back and returned to be shown, and the set's
/// other songs opened into the same engine behind it, one by one
/// ([`take_arrivals`]) — each joining the live session as it does.
/// `progress` hears each step for the loading screen. `guide` is a share
/// link to the guide sample library (as Ogg) the click, count and cue
/// tracks play from; without it they play synthesized ticks and beeps.
///
/// # Errors
///
/// The first song could not be opened or read back, or the facade did
/// not come up.
pub async fn open(
    source: &WebSource,
    guide: Option<&str>,
    progress: impl Fn(Progress),
) -> eyre::Result<(EngineRef, crate::setlist::Setlist)> {
    // What to open: a live set's songs (joined first, for their files
    // links), or the one song a link or the page names.
    let mut live: Option<crate::collab::TaskSet> = None;
    let mut wanted: Vec<Wanted> = match source {
        WebSource::Live { link, .. } => {
            let set = crate::collab::TaskSet::parse(&format!("share:{link}"));
            progress(Progress::Joining { retry: None });
            let joined = crate::task_set::until_ok(
                None,
                || crate::task_set::join(&set.url),
                |why| {
                    progress(Progress::Joining { retry: Some(why) });
                },
            )
            .await?;
            tracing::info!(live.setlist = %joined.title, live.songs = joined.songs.len(), live.epoch = joined.epoch, "web: joined a live set");
            watch_set(set.url.clone(), joined.setlist.clone(), joined.epoch);
            let wanted = joined
                .songs
                .iter()
                .filter_map(|s| {
                    Some(Wanted {
                        title: Some(s.title.clone()),
                        from: Origin::Share(s.files.clone()?),
                    })
                })
                .collect();
            live = Some(crate::collab::TaskSet {
                setlist: joined.setlist,
                ..set
            });
            wanted
        }
        WebSource::Shared { link } => vec![Wanted {
            title: None,
            from: Origin::Share(link.clone()),
        }],
        WebSource::Bundled {
            name,
            rpp_url,
            chart_url,
        } => vec![Wanted {
            title: None,
            from: Origin::Bundled {
                name: name.clone(),
                rpp_url: rpp_url.clone(),
                chart_url: chart_url.clone(),
            },
        }],
    };
    if wanted.is_empty() {
        eyre::bail!("the set has no songs with files to open");
    }
    let rest = wanted.split_off(1);
    let first = wanted.remove(0);
    let first_title = first.title.clone().unwrap_or_default();

    progress(Progress::Fetching {
        title: first_title.clone(),
        retry: None,
    });
    // The first song and the guide's samples at once: neither needs the
    // other, and the guide's were fetched only once the song was in.
    let song = crate::task_set::until_ok(
        Some(8),
        || fetch_song(&first),
        |why| {
            progress(Progress::Fetching {
                title: first_title.clone(),
                retry: Some(why),
            });
        },
    );
    let samples = async {
        match guide {
            Some(base) => guide_library(base).await,
            None => HashMap::new(),
        }
    };
    let (fetched, library) = futures_util::future::join(song, samples).await;
    let fetched = fetched?;
    progress(Progress::Opening {
        title: first_title.clone(),
    });
    MULTITRACKS.with(|m| m.set(chose_multitracks()));
    let standalone = daw_standalone::sync::Standalone::new();
    // The guide instrument, before the guide tracks it plays on are made.
    crate::guide_instrument::install(
        &standalone,
        crate::guide_instrument::Library::Files {
            files: Arc::new(library),
            ext: "ogg",
        },
    );
    let loaded = open_fetched(&standalone, first.title, fetched)?;
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone())
        .await
        .map_err(|e| eyre::eyre!("build_in_process_daw: {e:?}"))?;
    daw::init_from_parts(bundle.daw.clone());
    // The facade and the engine live as long as the page.
    std::mem::forget(bundle);
    let (song, kept) = read_back(loaded).await?;
    let project = song.project.clone();
    WEB.with(|web| {
        *web.borrow_mut() = Some(Web {
            standalone: standalone.clone(),
            songs: HashMap::from([(project.clone(), kept)]),
            order: vec![project.clone()],
            warming: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
    });
    crate::web_audio::install(standalone.clone(), &project);
    switch_song(&project);

    // In the live set with everyone else (the facade is up: the session's
    // bridges read and drive the engine through it). The songs that open
    // after this join it as they do (`collab::song_opened`).
    if let (Some(set), WebSource::Live { name, .. }) = (live, source) {
        if let Err(e) = crate::collab::join_task(&set, name.clone()) {
            tracing::warn!(collab.error = %e, "web: could not join the live set");
        }
    }

    // The set's other songs, behind the one on screen, in set order.
    let mut setlist = crate::setlist::Setlist::of(vec![song]);
    setlist.pending = rest
        .iter()
        .map(|w| w.title.clone().unwrap_or_default())
        .collect();
    let (arrive, arrivals) = tokio::sync::mpsc::unbounded_channel();
    ARRIVALS.with(|a| *a.borrow_mut() = Some(arrivals));
    wasm_bindgen_futures::spawn_local(async move {
        // The song on screen first: its opening seconds of audio start
        // streaming as `open` returns, and the next song's hundred-odd
        // requests landing on the same connection at that moment delayed
        // the first sound. A head start, not a wait for it: a set whose
        // first song never plays still fills in behind it.
        if !rest.is_empty() {
            gloo_timers::future::TimeoutFuture::new(NEXT_SONGS_AFTER_MS).await;
        }
        for want in rest {
            let title = want.title.clone().unwrap_or_default();
            let opened = async {
                let fetched =
                    crate::task_set::until_ok(Some(6), || fetch_song(&want), |_| {}).await?;
                read_back(open_fetched(&standalone, want.title.clone(), fetched)?).await
            }
            .await;
            let arrival = match opened {
                Ok((song, kept)) => {
                    let project = song.project.clone();
                    WEB.with(|web| {
                        if let Some(web) = web.borrow_mut().as_mut() {
                            let streamed = kept.streamed.clone();
                            web.songs.insert(project.clone(), kept);
                            web.order.push(project.clone());
                            // Heard by its reference, all of it (a few MB);
                            // by its stems, the opening of the song right
                            // after the one on screen — so moving on to it
                            // plays at once.
                            let showing = crate::open::current_song();
                            let after_showing = web.order.len() >= 2
                                && showing.as_deref()
                                    == web.order.get(web.order.len() - 2).map(String::as_str);
                            let warm: Vec<(Heard, Option<u64>)> = match streamed
                                .as_ref()
                                .map(|s| heard(s))
                            {
                                Some(Heard::Stems) if after_showing => {
                                    vec![(Heard::Stems, Some(NEXT_SONG_OPENING))]
                                }
                                Some(Heard::Stems) | None => Vec::new(),
                                Some(_) => vec![(Heard::Preview, None), (Heard::Reference, None)],
                            };
                            if let (Some(streamed), false) = (streamed, warm.is_empty()) {
                                let stop = Arc::clone(&web.warming);
                                wasm_bindgen_futures::spawn_local(async move {
                                    for (heard, blocks) in warm {
                                        streamed.warm(heard, blocks, Arc::clone(&stop)).await;
                                    }
                                });
                            }
                        }
                    });
                    crate::collab::song_opened(&project);
                    Arrival::Song(song)
                }
                Err(e) => {
                    tracing::warn!(song.title = %title, error = %e, "web: a song did not open; the set goes on without it");
                    Arrival::Failed(title)
                }
            };
            if arrive.send(arrival).is_err() {
                break;
            }
        }
    });

    let engine = EngineRef {
        applier: Rc::new(crate::engine::Applier::start()),
    };
    Ok((engine, setlist))
}

/// A song's files, fetched: mirrored from its share link into memory (its
/// proxies stream later, by range), or the page's bundled copy.
async fn fetch_song(want: &Wanted) -> eyre::Result<Fetched> {
    use crate::folder::Memory;
    use crate::song_stream::{Keep, ShareSource, mirror};
    match &want.from {
        Origin::Share(link) => {
            let song = mirror(
                Arc::new(ShareSource::new(link)?),
                Keep::Memory(Memory::new()),
            )
            .await?;
            Ok(Fetched {
                folder: Arc::clone(&song.folder),
                path: song.project.clone(),
                streamed: Some(song),
            })
        }
        Origin::Bundled {
            name,
            rpp_url,
            chart_url,
        } => {
            let memory = Memory::new();
            let dir = std::path::Path::new("/bundled");
            let rpp = dir.join(format!("{name}.RPP"));
            memory.insert(
                &rpp,
                fetch_bytes(rpp_url).await.map_err(|e| eyre::eyre!(e))?,
            );
            if let Some(url) = chart_url {
                memory.insert(
                    dir.join(format!("{name}.kf")),
                    fetch_bytes(url).await.map_err(|e| eyre::eyre!(e))?,
                );
            }
            Ok(Fetched {
                folder: Arc::new(memory),
                path: rpp,
                streamed: None,
            })
        }
    }
}

/// Open a fetched song into `standalone`: prepared (its `.session` when it
/// carries one, else organized, built from its chart, given its guide) —
/// it does not become current.
fn open_fetched(
    standalone: &daw_standalone::sync::Standalone,
    title: Option<String>,
    song: Fetched,
) -> eyre::Result<Loaded> {
    let prepare = crate::prepare::Prepare::for_song_in(song.folder.as_ref(), &song.path);
    let (opened, plan) = crate::open_core::open_song_in(
        song.folder.as_ref(),
        standalone,
        &song.path,
        &prepare,
        crate::open_core::Media::Deferred,
    )?;
    let rpp_text = crate::open_core::project_text_in(song.folder.as_ref(), &plan.open)?.text;
    let chart_text = prepare
        .chart
        .as_ref()
        .and_then(|p| song.folder.read_to_string(p).ok());
    // The chart a live set's doc for this song is seeded with.
    if let Some(chart) = &chart_text {
        crate::collab::remember_chart(&plan.open, chart);
    }
    Ok(Loaded {
        title: title.unwrap_or_else(|| opened.name.clone()),
        project: opened.project_guid.clone(),
        rpp_text,
        chart_text,
        streamed: song.streamed,
    })
}

/// A song the page opened, read back as the studio lays it out — by its
/// project, so a song opening behind the one on screen does not move it.
async fn read_back(song: Loaded) -> eyre::Result<(crate::setlist::Song, WebSong)> {
    let session = StudioSession::read_project(
        &song.project,
        &song.title,
        &song.rpp_text,
        song.chart_text.as_deref(),
        false,
    )
    .await?;
    let previews = session.previews.clone();
    let kept = WebSong {
        streamed: song.streamed.map(Rc::new),
        previews,
        started: std::cell::Cell::new(false),
        held: std::cell::RefCell::default(),
        reference: std::cell::RefCell::default(),
    };
    Ok((
        crate::setlist::Song::of(song.title, song.project, session),
        kept,
    ))
}

/// Make `project` the song on screen — what picking a setlist tab does on
/// a page (`crate::open::switch_song`): the engine's current project, the
/// audio moved to it, and — the first time — its proxies streaming and its
/// waveforms loading.
pub fn switch_song(project: &str) {
    // Told once the switch is done, whichever way it returns.
    struct Tell;
    impl Drop for Tell {
        fn drop(&mut self) {
            tell_listening();
        }
    }
    let _tell = Tell;
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        let Some(web) = web.as_mut() else { return };
        web.standalone.set_current_project(project);
        crate::open::set_current_song(project);
        crate::web_audio::switch(project);
        warm_around(web, project);
        let Some(song) = web.songs.get(project) else {
            return;
        };
        if song.started.replace(true) {
            return;
        }
        let Some(streamed) = song.streamed.clone() else {
            return;
        };
        stream_song(&web.standalone, project, song, &streamed);
        if MULTITRACKS.with(std::cell::Cell::get) {
            hear_stems(web, project);
        }
        // The waveforms: each take's original's peaks, fetched and reduced,
        // then drawn — the audio need not have arrived.
        let previews = song.previews.clone();
        let items: Vec<String> =
            daw_standalone::audio_engine::materialize::pending_media(&web.standalone, project)
                .into_iter()
                .map(|m| m.item_guid)
                .collect();
        wasm_bindgen_futures::spawn_local(async move {
            if streamed.load_peaks(crate::midi::WAVE_BLOCK).await > 0 {
                previews.fill_waves(items).await;
            }
        });
    });
}

/// Blocks of each stem the song after the one on screen is warmed with —
/// its opening (a block is 256 KiB, some fifteen seconds of a proxy), so
/// moving on to it plays at once.
const NEXT_SONG_OPENING: u64 = 2;

/// What `song` is heard by: its reference, unless the multitracks were
/// asked for or it has none.
fn heard(song: &crate::song_stream::StreamedSong) -> Heard {
    if MULTITRACKS.with(std::cell::Cell::get) || !song.has_reference() {
        Heard::Stems
    } else {
        Heard::Reference
    }
}

/// Fill the browser's cache around `project`, now on screen, with what it
/// is heard by: the whole of it — so a jump anywhere in the song plays from
/// disk, not the network — then, by stems, the opening of the song after
/// it. By references, every song's preview first, in set order from here
/// (a megabyte each: the whole set is soon playable), then every song's
/// reference proper. What was warming for the song before stops.
fn warm_around(web: &mut Web, project: &str) {
    use std::sync::atomic::Ordering;
    web.warming.store(true, Ordering::Relaxed);
    let stop = Arc::new(AtomicBool::new(false));
    web.warming = Arc::clone(&stop);
    let at = web.order.iter().position(|p| p == project).unwrap_or(0);
    let from_here = web.order[at..].iter().chain(&web.order[..at]);
    let songs: Vec<_> = from_here
        .filter_map(|p| web.songs.get(p).and_then(|s| s.streamed.clone()))
        .collect();
    let mut warm = Vec::new();
    for tier in [Heard::Preview, Heard::Reference] {
        for song in songs.iter().filter(|s| heard(s) != Heard::Stems) {
            warm.push((Rc::clone(song), tier, None));
        }
    }
    for (i, song) in songs
        .iter()
        .enumerate()
        .filter(|(_, s)| heard(s) == Heard::Stems)
    {
        match i {
            0 => warm.insert(0, (Rc::clone(song), Heard::Stems, None)),
            1 => warm.push((Rc::clone(song), Heard::Stems, Some(NEXT_SONG_OPENING))),
            _ => {}
        }
    }
    wasm_bindgen_futures::spawn_local(async move {
        for (song, heard, blocks) in warm {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            song.warm(heard, blocks, Arc::clone(&stop)).await;
        }
    });
}

/// How far ahead of the playhead a song's stems must have arrived before
/// they take over from its reference.
const STEMS_READY_AHEAD: f64 = 1.0;
/// How long the reference waits for them before giving way regardless (a
/// stem that will not come must not keep the song on its reference).
const STEMS_READY_WITHIN: std::time::Duration = std::time::Duration::from_secs(20);

/// Hear `project` by its stems: fetch them (the reference plays on), and
/// once every one has arrived at the playhead, trade the reference for
/// them — its fetch stopped. Nothing when it is heard by them already.
/// Called with [`WEB`] borrowed: the page is told by the caller.
fn hear_stems(web: &Web, project: &str) {
    use std::sync::atomic::Ordering;
    let Some(song) = web.songs.get(project) else {
        return;
    };
    let (Some(takes), Some(streamed)) = (song.held.borrow_mut().take(), song.streamed.clone())
    else {
        return;
    };
    let playhead = playhead();
    streamed.fetch(
        Arc::clone(&takes),
        Arc::clone(&playhead),
        Arc::new(AtomicBool::new(false)),
    );
    let project = project.to_owned();
    wasm_bindgen_futures::spawn_local(async move {
        let began = web_time::Instant::now();
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(100)).await;
            let t = playhead();
            let ready = takes.lock().map_or(true, |takes| {
                takes.iter().all(|take| take.has_at(t, STEMS_READY_AHEAD))
            });
            if ready || began.elapsed() > STEMS_READY_WITHIN {
                tracing::info!(
                    stems.ready = ready,
                    stems.waited_ms = began.elapsed().as_millis() as u64,
                    "web: the stems take over from the reference"
                );
                break;
            }
        }
        crate::web_audio::multitracks(&project);
        WEB.with(|web| {
            if let Some(song) = web.borrow().as_ref().and_then(|w| w.songs.get(&project))
                && let Some(stop) = song.reference.borrow_mut().take()
            {
                stop.store(true, Ordering::Relaxed);
            }
        });
        tell_listening();
    });
}

/// Where the play cursor is, as the fetchers follow it.
fn playhead() -> Arc<dyn Fn() -> f64 + Send + Sync> {
    Arc::new(|| crate::engine::Transport::shared().map_or(0.0, |t| t.read().0))
}

/// Keep the page in the set it joined. A playground set (the public
/// demo) starts over every few minutes — its songs' docs made new, the
/// ones this page syncs no longer served — so when the set's epoch moves
/// past `epoch` the page opens again, its songs from the browser's cache,
/// into the new run. And when the connection to Task goes (Task
/// restarting, a network blip), the set is joined again once Task answers
/// — which is also what has Task serve the set's docs again after a
/// restart; the session's replicas reconnect by themselves. Only a set
/// that moved on meanwhile opens the page again.
fn watch_set(url: String, setlist: String, epoch: u64) {
    use live_proto::LiveSessionsStreamClient;
    let reload = || {
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    };
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            if let Ok(stream) =
                task_dial::establish_at::<LiveSessionsStreamClient>(&url, None).await
            {
                let (tx, mut rx) = vox::channel::<live_proto::LiveEpoch>();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = stream.epochs(tx).await;
                });
                while let Ok(Some(message)) = rx.recv().await {
                    let mut next = None;
                    let _ = message.map(|e| next = Some((e.setlist.clone(), e.epoch)));
                    if let Some((set, moved)) = next
                        && set == setlist
                        && moved > epoch
                    {
                        tracing::info!(
                            live.epoch = moved,
                            "web: the set starts over; opening it again"
                        );
                        reload();
                        return;
                    }
                }
            }
            // The connection went: join again once Task answers.
            tracing::info!("web: lost Task; joining the set again");
            let Ok(joined) =
                crate::task_set::until_ok(None, || crate::task_set::join(&url), |_| {}).await
            else {
                continue;
            };
            if joined.setlist == setlist && joined.epoch != epoch {
                tracing::info!(
                    live.epoch = joined.epoch,
                    "web: the set moved on while away; opening it again"
                );
                reload();
                return;
            }
            tracing::info!("web: back in the set");
        }
    });
}

/// Every take of a shared song attached as its proxy, streaming in — the
/// way a streamed song's takes are attached natively (`stream_in`), pumped
/// by this page's audio loop, fetched in the order they will be heard from
/// the play cursor. A song with a reference is heard by it first: only its
/// bytes are fetched, and the takes are held for [`hear_stems`].
fn stream_song(
    standalone: &daw_standalone::sync::Standalone,
    project: &str,
    kept: &WebSong,
    song: &crate::song_stream::StreamedSong,
) {
    let playhead = playhead();
    // Before the takes are attached, so their feeders are held.
    let reference = song.reference();
    let by_reference = reference.is_some();
    if let Some(crate::song_stream::Reference { full, preview }) = reference {
        let (preview_take, preview) = preview.unzip();
        crate::web_audio::reference(project, full.1.boxed(), preview.map(|p| p.boxed()));
        // The preview first: small, so it is heard almost at once.
        let takes = preview_take.into_iter().chain([full.0]).collect();
        let stop = Arc::new(AtomicBool::new(false));
        song.fetch(
            Arc::new(std::sync::Mutex::new(takes)),
            Arc::clone(&playhead),
            Arc::clone(&stop),
        );
        *kept.reference.borrow_mut() = Some(stop);
    }
    let media = daw_standalone::audio_engine::materialize::pending_media(standalone, project);
    let mut meters = Vec::new();
    let takes: Vec<_> = media
        .iter()
        .filter_map(|m| {
            let attached = song.attach(standalone, project, m)?;
            meters.push(crate::web_audio::MeterTake {
                track: m.track_guid.clone(),
                start: m.start,
                end: m.end,
                source_offset: m.source_offset,
                playrate: m.playrate,
                source: attached.source,
            });
            Some(attached.take)
        })
        .collect();
    // Heard by its reference, the stems' meters read their waveforms.
    if by_reference {
        crate::web_audio::peak_meters(project, meters);
    }
    tracing::info!(
        stream.media = media.len(),
        stream.attached = takes.len(),
        stream.by_reference = by_reference,
        "audio: streaming the song's proxies"
    );
    let takes = Arc::new(std::sync::Mutex::new(takes));
    if by_reference {
        *kept.held.borrow_mut() = Some(takes);
    } else {
        // Fetching for as long as the page lives.
        song.fetch(takes, playhead, Arc::new(AtomicBool::new(false)));
    }
}

/// A root-relative path as a URL path: each segment encoded.
fn url_path(path: &str) -> String {
    path.split('/')
        .map(|segment| String::from(js_sys::encode_uri_component(segment)))
        .collect::<Vec<_>>()
        .join("/")
}

/// A URL's whole body.
async fn fetch_bytes(url: &str) -> Result<Arc<[u8]>, String> {
    use wasm_bindgen::JsCast as _;
    let window = web_sys::window().ok_or("no window")?;
    let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| "not a response")?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let body = response.array_buffer().map_err(|e| format!("{e:?}"))?;
    let body = wasm_bindgen_futures::JsFuture::from(body)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&body).to_vec().into())
}

/// The guide library's files MIDI-mode playback needs, fetched together
/// from the library's share link. A file that does not arrive is left out
/// (the instrument synthesizes its slot).
async fn guide_library(base: &str) -> HashMap<String, Vec<u8>> {
    let wanted = session_guide::samples::library::files(
        crate::guide_instrument::CLICK,
        crate::guide_instrument::VOICE,
        "ogg",
    );
    let base = base.trim_end_matches('/');
    let fetches = wanted.into_iter().map(|path| async move {
        let url = format!("{base}/download/{}", url_path(&path));
        match fetch_bytes(&url).await {
            Ok(bytes) => Some((path, bytes.to_vec())),
            Err(e) => {
                tracing::warn!(path, error = %e, "guide: a sample did not arrive");
                None
            }
        }
    });
    let files: HashMap<String, Vec<u8>> = futures_util::future::join_all(fetches)
        .await
        .into_iter()
        .flatten()
        .collect();
    tracing::info!(samples = files.len(), "guide: the sample library arrived");
    files
}
