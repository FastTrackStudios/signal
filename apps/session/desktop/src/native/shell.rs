//! The app's frame: one top bar over whichever view is up.
//!
//! The top bar is also the window's title bar. On macOS the traffic lights
//! sit in its left end (the window's own title bar is transparent — see
//! `super::window_attributes`), then the views, then the mode; the rest of
//! it is a drag surface. The mode lives here and not in the DAW's toolbar,
//! so it is visible whatever the view.
//!
//! Inline styles throughout: Blitz and external style sheets do not mix
//! (see the repo's CLAUDE.md). The compiled Tailwind the performance panels
//! were written against goes in once, as a plain `<style>` element.

use dioxus::prelude::*;

use session::modes::Mode;

/// The Session app's compiled Tailwind — what `session-ui`'s panels are
/// styled with. A plain `style { }` element, which Blitz reads; a
/// `document::Style` goes through a window head (see `docs/app-on-blitz.md`).
const TAILWIND: &str = include_str!("../../assets/tailwind-signal.css");

/// How much of the bar's left end belongs to the system: the traffic
/// lights on macOS, and on an iPad the window controls iPadOS keeps in
/// every app's top-left corner. A control there is a trap — the first
/// press shows the controls, the next lands on them, and the red one
/// closes the app, which looks like a crash.
#[cfg(target_os = "macos")]
const LIGHTS_W: f64 = 78.0;
#[cfg(target_os = "ios")]
const LIGHTS_W: f64 = 76.0;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
const LIGHTS_W: f64 = 12.0;

const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";

/// The whole window.
#[component]
pub fn Shell() -> Element {
    use session_daw::shell::{TopBar, View};

    // `FTS_SESSION_VIEW` opens on a view other than the DAW.
    let view = use_signal(|| match std::env::var("FTS_SESSION_VIEW").as_deref() {
        Ok("performance") => View::Performance,
        Ok("overview") => View::Overview,
        Ok("setup") => View::Setup,
        Ok("chart") => View::Chart,
        Ok("lyrics") => View::Lyrics,
        Ok("mixer") => View::Mixer,
        Ok("fx") => View::Fx,
        _ => View::Daw,
    });
    // The Mixer button, `x` and the toolbar's mixer: one toggle.
    session_daw::shell::use_mixer_toggle(view);
    // Live unless `FTS_SESSION_MODE` names another (`organize`, …): the
    // docked mixer's compact strips are the Live ones.
    let mode = use_signal(|| {
        std::env::var("FTS_SESSION_MODE")
            .ok()
            .and_then(|name| {
                Mode::ALL
                    .into_iter()
                    .find(|m| m.display_name().eq_ignore_ascii_case(&name))
            })
            .unwrap_or(Mode::Live)
    });
    // The mode, for the panels that change with it (the mixer's strips
    // are live-mode strips in Live).
    use_context_provider(|| mode);
    // A mode asked for on the Action List (`SESSION_MODE`, a model over
    // MCP): taken up within a tenth of a second.
    let mut asked = mode;
    let back = try_use_context::<session_daw::shell::Back>();
    use_future(move || async move {
        loop {
            if let Some(next) = session_daw::actions::take_mode_request() {
                asked.set(next);
            }
            // What the arrangement queued, to the engine — with no frame
            // coming (the window hidden), an action's edits would wait.
            session_daw::panel::send_pending();
            // Another song or version asked for over the Action List
            // (`SESSION_OPEN`): out of this one, and the start screen
            // opens it.
            if session_daw::projects::take_back()
                && let Some(session_daw::shell::Back(back)) = back
            {
                back.call(());
            }
            architect::platform::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    // The chart's text editor, beside the chart in the Overview: open in
    // Organize, closed in the other modes, and the button on the chart's
    // corner opens or closes it in any of them. Out here, not per song, so
    // picking another song keeps it as it was.
    let mut editor_open = use_signal(|| *mode.peek() == Mode::Organize);
    use_effect(move || editor_open.set(mode() == Mode::Organize));
    // Whether the mixer is open, per mode (Organize starts closed) — above
    // the songs, so it holds across them.
    use_context_provider(session_daw::mixer_panel::MixerMemory::new);
    // Touch mode: on where the screen is the pointer, switched in the
    // record view's menu.
    use_context_provider(session_daw::touch::Touch::detect);
    // What is shown full screen, if anything, and the racks' settings
    // every view of them shares (`session_daw::closeup`).
    use_context_provider(session_daw::closeup::Closeups::new);
    session_daw::shell::use_pins();
    // The lyrics' Audience / Performer and layer, held across songs.
    use_context_provider(session_daw::lyrics_panel::LyricsChoice::new);
    // The songs, as the launch opened them — a signal from here on, which
    // the tabs read and a pick or a recolour writes.
    let opened: session_daw::setlist::Setlist = use_context();
    let mut setlist = use_context_provider(|| Signal::new(opened));
    // Each song in the mode it was last worked in.
    session_daw::song_modes::use_song_modes(setlist, mode);
    // In Mix mode, the guide folder muted.
    session_daw::mix_guide::use_mix_guide(setlist, mode);
    // A streamed set's songs after the first, as each opens behind it.
    use_future(move || async move {
        let Some(mut arrivals) = session_daw::stream_set::take_arrivals() else {
            return;
        };
        while let Some(arrival) = arrivals.recv().await {
            setlist.write().arrive(arrival);
        }
    });
    use_live_advance(setlist, mode);
    // The window draws a frame only when something asks: an event, a
    // change here, or a panel whose picture moves (the arrangement, the
    // mixer, the chart — each asks while the transport moves). While it
    // moves, frames are asked for here too: for the views with no such
    // panel (Perform, Lyrics), whose playhead and progress read the
    // transport on each frame, and for the first frame after a start that
    // came from outside the window (the space bar, the lock screen, a
    // collaborator leading).
    {
        let window = dioxus_native::use_window();
        use_future(move || {
            let window = window.clone();
            async move {
                loop {
                    let moving = session_daw::engine::moving();
                    // Or an action run on the Action List for the window
                    // (a model's, over MCP): no event of the window's own
                    // will come to carry it out, nor to send what it
                    // edited on to the engine, so frames are asked for.
                    if moving
                        || session_daw::actions::window_pending()
                        || session_daw::actions::frame_owed()
                        || session_daw::studio::resync_pending()
                    {
                        window.request_redraw();
                    }
                    let wait = if moving { 33 } else { 100 };
                    futures_timer::Delay::new(std::time::Duration::from_millis(wait)).await;
                }
            }
        });
    }
    session_daw::collab_bar::use_follow_song(setlist);
    // `FTS_SESSION_SONG=<n>` (1-based): open the set on its n-th song — to
    // start somewhere other than the top, or (the collaboration demo) to
    // put a second window on a different song from the first.
    use_hook(move || {
        let index = std::env::var("FTS_SESSION_SONG")
            .ok()
            .and_then(|n| n.parse::<usize>().ok());
        if let Some(index) = index.and_then(|n| n.checked_sub(1)) {
            let picked = setlist
                .write()
                .pick(index, 0.0)
                .map(|song| song.project.clone());
            if let Some(project) = picked {
                session_daw::open::switch_song(&project);
            }
        }
    });
    // Space plays and stops whatever has the focus.
    session_daw::keys::use_window_transport_keys();
    let window = dioxus_native::use_window();
    // The window's size in logical pixels, kept as it is resized: the
    // bar's width says how much of it is spelled out, and the shape says
    // which layout — a phone's (or a window made that small) takes the
    // small-screen one, the same panels rearranged.
    let logical = |w: &std::sync::Arc<dyn winit::window::Window>| {
        let size = w.surface_size();
        let scale = w.scale_factor().max(1.0);
        (
            f64::from(size.width) / scale,
            f64::from(size.height) / scale,
        )
    };
    let mut size = use_signal(|| logical(&window));
    let measuring = window.clone();
    dioxus_native::use_window_event(move |event, _| {
        if matches!(
            event,
            winit::event::WindowEvent::SurfaceResized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
        ) {
            let now = logical(&measuring);
            let was = *size.peek();
            if (was.0 - now.0).abs() > 0.5 || (was.1 - now.1).abs() > 0.5 {
                size.set(now);
            }
        }
    });
    use_context_provider(|| session_daw::shell::WindowSize(size));
    // A phone on its side is drawn to both edges (`BLITZ_SAFE_AREA_SIDES=0`
    // in main): the view runs under the camera housing, and only the
    // controls on its side get out of its way — the views' rail moving to the
    // right edge when it is on the left, the inspector's strip moving in when
    // it is on the right — while the top rows keep clear of the rounded
    // corners.
    #[cfg_attr(not(target_os = "ios"), allow(unused_variables, unused_mut))]
    let mut sides = use_context_provider(|| Signal::new(session_daw::compact::Sides::default()));
    #[cfg(target_os = "ios")]
    {
        let reading = window.clone();
        dioxus_native::use_window_event(move |event, _| {
            use session_daw::compact::Sides;
            /// The housing and the gap beside it: less than the inset the
            /// system reports, which pads for the rounded corners too.
            const HOUSING: f64 = 50.0;
            if !matches!(event, winit::event::WindowEvent::RedrawRequested) {
                return;
            }
            let insets = reading.safe_area();
            let scale = reading.scale_factor().max(1.0);
            let clear = |inset: u32| (f64::from(inset) / scale).min(HOUSING);
            let now = match super::ios_scene::island_on_left() {
                Some(true) => Sides {
                    left: clear(insets.left),
                    right: 0.0,
                    corner: 16.0,
                },
                Some(false) => Sides {
                    left: 0.0,
                    right: clear(insets.right),
                    corner: 16.0,
                },
                None => Sides::default(),
            };
            if *sides.peek() != now {
                sides.set(now);
            }
        });
    }
    let width = move || size().0;
    let form = use_memo(move || {
        let (w, h) = size();
        session_daw::compact::Form::of(w, h)
    });
    let mut form_signal = use_context_provider(|| Signal::new(*form.peek()));
    use_effect(move || {
        let now = form();
        if *form_signal.peek() != now {
            form_signal.set(now);
        }
    });
    // On a phone, `FTS_SESSION_VIEW` names the tab it opens on, as it
    // names the view on a wider screen; the chart otherwise.
    let phone_view = use_signal(|| {
        use session_daw::compact::PhoneView;
        match std::env::var("FTS_SESSION_VIEW").as_deref() {
            Ok("daw") => PhoneView::Arrangement,
            Ok("performance") => PhoneView::Control,
            Ok("mixer") => PhoneView::Mixer,
            _ => PhoneView::Chart,
        }
    });
    // A song picked, from the tabs or the navigator: that song is current,
    // and the audio moves to it. Where the one it replaces had got to is
    // kept on its tab.
    let pick = move |index: usize| {
        let at = session_daw::engine::Transport::shared().map_or(0.0, |t| t.read().0);
        let picked = setlist
            .write()
            .pick(index, at)
            .map(|song| song.project.clone());
        if let Some(project) = picked {
            session_daw::open::switch_song(&project);
            // Playing together, everyone goes with it.
            session_daw::collab::transport_pressed();
        }
    };
    // The record view's song menu picks the same way.
    use_context_provider(|| session_daw::record_view::PickSong(Callback::new(pick)));
    // On iOS: the device's audio session, for Settings → This device.
    #[cfg(target_os = "ios")]
    use_context_provider(|| session_daw::device_audio::DeviceAudio {
        read: Callback::new(|()| super::ios_audio::report()),
        change: Callback::new(super::ios_audio::change),
    });
    // On iOS: Now Playing kept to the song and the transport, and the
    // lock screen's, headphones' and car's commands carried out.
    #[cfg(target_os = "ios")]
    use_future(move || async move {
        use super::ios_audio::{Command, NowPlaying};
        use session_daw::engine::{Move, Transport, transport};
        let mut pick = pick;
        let mut shown: Option<NowPlaying> = None;
        let mut since = 0_u32;
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(400)).await;
            let reading = Transport::shared().map(Transport::reading);
            let playing = reading.is_some_and(|r| r.playing);
            for command in super::ios_audio::take_commands() {
                let at = setlist.peek().at;
                match command {
                    Command::Play if !playing => transport(Move::PlayStop, 0.0),
                    Command::Pause if playing => transport(Move::PlayStop, 0.0),
                    Command::Toggle => transport(Move::PlayStop, 0.0),
                    Command::Next if at + 1 < setlist.peek().songs.len() => pick(at + 1),
                    Command::Previous if at > 0 => pick(at - 1),
                    _ => {}
                }
            }
            let Some(song) = setlist.peek().current().cloned() else {
                continue;
            };
            let at = reading.map_or(0.0, |r| r.at);
            let now = NowPlaying {
                title: song.title(),
                set: "Session".to_owned(),
                elapsed: at - song.span.0,
                duration: song.span.1 - song.span.0,
                playing,
            };
            // On a change of song or of playing, and every few seconds to
            // keep a seek honest; iOS moves the position on between.
            since += 1;
            let changed = shown
                .as_ref()
                .is_none_or(|was| was.title != now.title || was.playing != now.playing);
            if changed || since >= 12 {
                super::ios_audio::show(&now);
                shown = Some(now);
                since = 0;
            }
        }
    });
    let (dragging, zooming) = (window.clone(), window);
    let current = setlist.read().current().cloned();
    if form().compact() {
        return rsx! {
            style { {TAILWIND} }
            session_daw::collab_pointers::CollabPointers {}
            session_daw::compact::CompactShell {
                form: form(),
                view: phone_view,
                on_pick: pick,
                // Who is here and where the sound comes from: More's.
                more: rsx! {
                    div {
                        style: "display:flex; align-items:center; gap:8px; flex-wrap:wrap;",
                        session_daw::collab_bar::CollabBar {}
                        session_daw::shell::AudioBadge { density: session_daw::shell::Density::Full }
                    }
                },
                body: rsx! {
                    if let Some(song) = current {
                        PhoneViews { key: "{song.project}", session: song.session.clone(), view: phone_view }
                    }
                },
            }
            session_daw::closeup::CloseupLayer { landscape: size().0 > size().1 }
        };
    }
    rsx! {
        style { {TAILWIND} }
        div {
            style: "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; flex-direction:column; \
                    overflow:hidden; background:#0f1012; color:{TEXT}; font-family:system-ui, sans-serif;",
            // A press anywhere but the chart editor (which stops it) gives
            // the keyboard back to the transport.
            onmousedown: move |_| session_daw::keys::set_editing(false),
            // Everyone else's mouse, over everything (collab_pointers).
            session_daw::collab_pointers::CollabPointers {}
            TopBar {
                lights: LIGHTS_W,
                // Once, not per song: it is the whole set's session, and
                // mounting it starts one from the environment.
                badges: rsx! { session_daw::collab_bar::CollabBar {} },
                width: width(),
                // Anywhere on the bar that is not a control drags the
                // window; a double click zooms it.
                on_drag: move |()| {
                    if let Err(e) = dragging.drag_window() {
                        tracing::debug!(error = %e, "window drag refused");
                    }
                },
                on_zoom: move |()| zooming.set_maximized(!zooming.is_maximized()),
                // A tab picked: that song is current, and the audio moves to
                // it. Where the one it replaces had got to is kept on its tab.
                on_pick: pick,
                on_color: move |(index, color): (usize, Option<String>)| {
                    setlist.write().recolor(index, color);
                },
            }

            // The navigator down the left when it is open, and the views
            // beside it.
            div {
                style: "flex:1; min-height:0; display:flex;",
                session_daw::shell::NavigatorColumn { on_pick: pick }
                session_daw::browser::BrowserColumn {}
                div {
                    style: "position:relative; flex:1; min-width:0; display:flex; flex-direction:column;",
                    if let Some(song) = current.clone() {
                        // Keyed by the song: picking another remounts every
                        // panel on that song's session rather than patching
                        // the last one's.
                        SongViews { key: "{song.project}", session: song.session.clone(), view, editor_open }
                    }
                }
            }
            // The views, across the foot of the window, and each view's
            // own controls beside them — the arrangement's the transport,
            // which reads the song it drives, so it is mounted per song.
            session_daw::shell::BottomBar {
                view,
                mode,
                width: width(),
                transport: rsx! {
                    if let Some(song) = current {
                        WithSong {
                            key: "{song.project}",
                            session: song.session.clone(),
                            session_daw::transport_bar::TransportBar { big: true }
                        }
                    }
                },
            }
            // Whatever is zoomed into, over all of it.
            session_daw::closeup::CloseupLayer { landscape: size().0 > size().1 }
        }
    }
}

/// The small-screen layout's views but Control (which the shell draws): the
/// same panels as the wide layout's, over one song.
#[component]
fn PhoneViews(
    session: session_daw::studio::StudioSession,
    view: Signal<session_daw::compact::PhoneView>,
) -> Element {
    use session_daw::compact::PhoneView;
    use_context_provider(|| session);
    let mode: Signal<Mode> = use_context();
    match view() {
        // The shell's own pages (`compact::CompactShell`).
        PhoneView::Control | PhoneView::More | PhoneView::Setup | PhoneView::Editor => rsx! {},
        PhoneView::Chart => rsx! { session_daw::chart_panel::Chart { paged: true } },
        PhoneView::Lyrics => rsx! { session_daw::lyrics_panel::LyricsPanel {} },
        // Keyed apart: the same component in the same place would otherwise
        // be kept and handed the other view's props.
        PhoneView::Arrangement => rsx! {
            session_daw::mixer_panel::DawPanels { key: "{view():?}", mode: Some(mode()) }
        },
        PhoneView::Mixer => rsx! {
            session_daw::mixer_panel::DawPanels { key: "{view():?}", mode: Some(mode()), mixer_only: true }
        },
    }
}

/// Whatever it holds, over one song: that song's session, as context.
#[component]
fn WithSong(session: session_daw::studio::StudioSession, children: Element) -> Element {
    use_context_provider(|| session);
    children
}

/// The views, over one song: its session is what every panel below reads.
#[component]
fn SongViews(
    session: session_daw::studio::StudioSession,
    view: Signal<session_daw::shell::View>,
    editor_open: Signal<bool>,
) -> Element {
    use session_daw::chart_editor::{ChartEditor, EditorToggle};
    use session_daw::shell::{OverviewLayout, View};
    use_context_provider(|| session);
    let mode: Signal<session::modes::Mode> = use_context();
    let record_mode = move || mode() == session::modes::Mode::Record;
    // The FX, pinned under the arrangement or the mixer: the view gives
    // up its bottom to it.
    let fx = try_use_context::<session_daw::shell::FxToggle>();
    let fx_docked = fx.is_some_and(|f| (f.docked)() && (f.open)() && f.hosts.contains(&view()));
    // The Editor, pinned under the arrangement (above the FX when both
    // are): the lower part of the window, the arrangement keeping the top.
    let editor = try_use_context::<session_daw::shell::EditorPane>();
    let editor_docked =
        editor.is_some_and(|e| (e.docked)() && (e.open)() && e.hosts.contains(&view()));
    let fx_h = if fx_docked { FX_DOCK_H } else { 0.0 };
    let view_bottom = if editor_docked {
        format!("calc({EDITOR_DOCK_SHARE}% + {fx_h}px)")
    } else {
        format!("{fx_h}px")
    };
    rsx! {
        session_daw::shell::PinnedProgress { view: view() }
        div {
            style: "position:relative; flex:1; min-height:0;",
            div {
            style: "position:absolute; left:0; right:0; top:0; bottom:{view_bottom};",
            match view() {
                View::Setup => rsx! { session_daw::setup::SetupView {} },
                View::Daw => rsx! { Arrangement {} },
                View::Performance => rsx! {
                    if record_mode() {
                        session_daw::record_view::RecordView {}
                    } else {
                        PerformanceView {}
                    }
                },
                View::Chart => rsx! { session_daw::chart_panel::Chart { paged: true } },
                View::Lyrics => rsx! { session_daw::lyrics_panel::LyricsPanel {} },
                View::Editor => rsx! { session_daw::editor_view::EditorPanel {} },
                View::Mixer => rsx! {
                    session_daw::mixer_panel::DawPanels { mode: Some(mode()), mixer_only: true }
                },
                // Its own view: four tracks' chains at once.
                View::Fx => rsx! { session_daw::fx_view::FxPanel { lanes: 4 } },
                View::Overview => rsx! {
                    OverviewLayout {
                        progress: rsx! { session_daw::progress::ProgressBar {} },
                        editor: editor_open().then(|| rsx! { ChartEditor {} }),
                        chart: rsx! { session_daw::chart_panel::Chart { paged: true } },
                        chart_corner: rsx! { EditorToggle { open: editor_open } },
                        under_chart: rsx! { session_daw::lyrics_panel::LyricsPanel {} },
                        panels: rsx! { Arrangement { docked: true } },
                    }
                },
            }
            }
            if editor_docked {
                div {
                    style: "position:absolute; left:0; right:0; bottom:{fx_h}px; \
                            height:{EDITOR_DOCK_SHARE}%; border-top:1px solid #000;",
                    session_daw::editor_view::EditorPanel {}
                }
            }
            if fx_docked {
                div {
                    style: "position:absolute; left:0; right:0; bottom:0; height:{FX_DOCK_H}px; \
                            border-top:1px solid #000;",
                    session_daw::fx_view::FxPanel {}
                }
            }
        }
        session_daw::shell::PinnedTransport { view: view() }
    }
}

/// How much of the view the Editor takes, pinned under the arrangement.
const EDITOR_DOCK_SHARE: f64 = 45.0;

/// How tall the FX is, pinned under the arrangement or the mixer: one
/// lane of its full-size view, so a track's chain is the same in both.
const FX_DOCK_H: f64 = session_daw::fx_view::LANE_H;

/// Live mode runs the set: when the song playing reaches its end, the next
/// one is picked and plays from its count-in. Checked each frame — the
/// window redraws while the transport moves — and only in Live, so working
/// on a song in the other modes never jumps away from it.
fn use_live_advance(mut setlist: Signal<session_daw::setlist::Setlist>, mode: Signal<Mode>) {
    dioxus_native::use_window_event(move |event, _| {
        if !matches!(event, winit::event::WindowEvent::RedrawRequested) || mode() != Mode::Live {
            return;
        }
        // Playing together, only the leader rolls on; this window follows
        // it into the next song.
        if session_daw::collab::following() {
            return;
        }
        let Some((at, playing)) = session_daw::engine::Transport::shared().map(|t| t.read()) else {
            return;
        };
        let next = {
            let list = setlist.peek();
            match (playing, list.current(), list.next()) {
                (true, Some(song), Some(next)) if song.ended(at) => Some(next),
                _ => None,
            }
        };
        let Some(next) = next else { return };
        let picked = setlist
            .write()
            .pick(next, at)
            .map(|song| song.project.clone());
        if let Some(project) = picked {
            tracing::info!(
                setlist.next = next,
                "live: the song ended; the next one plays"
            );
            session_daw::open::switch_song(&project);
            session_daw::engine::transport(session_daw::engine::Move::PlayFrom, 0.0);
        }
    });
}

/// The arrangement, with the mixer docked under it on `x` (or from the
/// start, `docked`, in the Overview). In Organize the Organize toolbar sits
/// over it — markers, sections, time signatures — in whichever view it is,
/// and the instruments (Drums, Bass, Guitar) down its left.
/// The transport is in the top bar.
#[component]
fn Arrangement(#[props(default)] docked: bool) -> Element {
    let mode: Signal<Mode> = use_context();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; flex-direction:column;",
            if mode() == Mode::Organize {
                session_daw::organize::OrganizeToolbar {}
            }
            div {
                style: "position:relative; flex:1; min-height:0; display:flex;",
                // Organize's instruments down the left: Drums, Bass, Guitar.
                if mode() == Mode::Organize {
                    session_daw::instrument_tracks::InstrumentRail {}
                }
                div {
                    style: "position:relative; flex:1; min-width:0;",
                    session_daw::mixer_panel::DawPanels { docked, mode: Some(mode()) }
                }
            }
        }
    }
}

/// The performance view: the song's progress, its chart live with the
/// playhead, and — once it exists — the lyrics beside it.
#[component]
fn PerformanceView() -> Element {
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; gap:16px; padding:16px;",
            session_daw::progress::ProgressBar {}
            div {
                style: "position:relative; flex:1; min-height:0; display:flex; gap:16px;",
                div {
                    style: "position:relative; flex:1; min-width:0; height:100%; border-radius:8px; \
                            overflow:hidden; border:1px solid {RULE};",
                    session_daw::chart_panel::Chart { paged: true }
                }
                div {
                    style: "position:relative; width:38%; min-width:320px; height:100%; border-radius:8px; \
                            overflow:hidden; border:1px solid {RULE};",
                    session_daw::lyrics_panel::LyricsPanel {}
                }
            }
            session_daw::progress::TransportButtons {}
        }
    }
}
