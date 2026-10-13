//! The arrangement view the app mounts, as a picture, a benchmark or a
//! window.
//!
//! ```sh
//! cargo run -r -p session-daw --bin blitz_shot -- \
//!     features/dynamic-template/fixtures/golden/template.rpp /tmp/studio.png
//! ```
//!
//! Nothing here draws the arrangement. The session is opened and planned
//! the way the app opens it ([`session_daw::studio::StudioSession`]), and
//! the tree is the app's: in a window it is
//! [`session_daw::mixer_panel::DawPanels`] — the arrangement panel, its
//! chrome and the docked mixer, exactly what the desktop shell mounts —
//! and in a headless document it is [`session_daw::panel::NativeTree`],
//! the same panel and chrome without the winit bridge a headless document
//! has no window for. The painting is `ArrangementWidget`'s, the one
//! painter.
//!
//! - **A picture** (the default): one frame of the headless document to
//!   `<out.png>`, at `FTS_BLITZ_SIZE` (2560x1440), scrolled to
//!   `[scroll_x] [scroll_y]` and zoomed to `FTS_BLITZ_ZOOM=x,y`.
//! - **A benchmark**: `FTS_BLITZ_FRAMES=<n>` drives `FTS_BLITZ_GESTURE`
//!   (`pan`, `down`, `zoom-x`, `zoom-y`) through the panel's own view and
//!   reports what a frame of the whole document costs — reconciling,
//!   style and layout, and the paint. `FTS_BLITZ_DUMP=<dir>` writes every
//!   frame out.
//! - **A window**: `FTS_BLITZ_WINDOW=1` opens it on dioxus-native, played
//!   through the audio engine; `=animate` runs the benchmark's gestures
//!   on screen. `FTS_BLITZ_DOCKED=1` opens it the way the Overview does,
//!   the mixer docked and the panel compact. `FTS_BLITZ_FPS=1` puts the
//!   widget's frame-time graph over it.
//!
//! `FTS_BLITZ_OPEN=Drums,Kick` (or `*`) opens those folders first.
//!
//! The session is laid out by `FTS_BLITZ_SCENE` (`drum-mixing` by
//! default, the app's), and prepared first if `FTS_BLITZ_ORGANIZE`,
//! `FTS_BLITZ_CHART` or `FTS_BLITZ_GUIDE` ask — see
//! [`session_daw::prepare`].

use std::sync::Arc;
use std::time::Instant;

use anyrender::ImageRenderer as _;
use anyrender_vello::VelloImageRenderer;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;

use session_daw::panel::{ArrangementPanel, PanelEvent, Slot};
use session_daw::studio::{PPS, StudioSession};

/// The scene the app lays a session out by.
const SCENE: &str = "drum-mixing";

fn main() {
    // A window writes to a LOG rather than to a terminal it does not
    // have. `FTS_BLITZ_LOG` names the file; a window defaults to one, so
    // that what the run did can be read afterwards instead of watched
    // live — which is the only way to know what a window did on somebody
    // else's screen.
    let windowed = std::env::var_os("FTS_BLITZ_WINDOW");
    let log = std::env::var("FTS_BLITZ_LOG")
        .ok()
        .or_else(|| windowed.as_ref().map(|_| "/tmp/fts-studio.log".to_owned()));
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(
            // `blitz_shot` and not `session_daw`: a binary's tracing target
            // is the BINARY's crate name, so a filter naming the library it
            // lives beside silently drops everything this file says.
            |_| "warn,blitz=info,usvg=error,session_daw=info,blitz_shot=info,daw_ui=info".into(),
        )
    };
    if let Some(path) = log.as_deref() {
        match std::fs::File::create(path) {
            Ok(file) => {
                tracing_subscriber::fmt()
                    .with_env_filter(filter())
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(file))
                    .init();
                println!("logging to {path}");
            }
            Err(error) => {
                tracing_subscriber::fmt().with_env_filter(filter()).init();
                tracing::warn!(%error, path, "could not open the log; using stderr");
            }
        }
    } else {
        tracing_subscriber::fmt().with_env_filter(filter()).init();
    }

    let mut args = std::env::args().skip(1);
    let (Some(project_path), Some(out)) = (args.next(), args.next()) else {
        eprintln!("blitz_shot <song.rpp> <out.png> [scroll_x] [scroll_y]");
        std::process::exit(2);
    };
    let scroll = (
        args.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
        args.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
    );
    let (width, height) = size();

    let session = open(std::path::Path::new(&project_path), windowed.is_some());

    if let Some(mode) = windowed {
        // `animate` runs the benchmark's own gestures on screen, so the
        // numbers in the table and what the window feels like are the
        // same thing measured twice. Anything else is a window you drive
        // yourself, with the app's own hands: the wheel scrolls, shift
        // makes it sideways, `z` zooms, the middle button is the hand.
        let props = WindowProps {
            session,
            size: (width, height),
            animate: mode.to_string_lossy() == "animate",
            docked: std::env::var("FTS_BLITZ_DOCKED").is_ok_and(|v| !v.is_empty() && v != "0"),
        };
        println!(
            "opening the studio{} — close the window to exit",
            if props.animate {
                ", running the benchmark's gestures"
            } else {
                ""
            }
        );
        // Opened at the size the rest of this program is measured at,
        // rather than winit's 800x600 default — a window that opens at a
        // size nobody benchmarks is a window whose frame rate cannot be
        // compared to anything.
        let wanted = winit::dpi::PhysicalSize::new(width, height);
        let attributes = winit::window::WindowAttributes::default()
            .with_title("FastTrackStudio — studio")
            .with_surface_size(wanted);
        dioxus_native::launch_cfg_with_props(Window, props, Vec::new(), vec![Box::new(attributes)]);
        return;
    }

    let slot = Slot::default();
    let vdom = VirtualDom::new_with_props(
        Still,
        StillProps {
            session,
            size: (width, height),
            slot: slot.clone(),
        },
    );
    let mut document = DioxusDocument::new(
        vdom,
        DocumentConfig {
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Dark)),
            // Without a provider the document's default one is a no-op
            // that never answers, so every image in the page stays in
            // flight forever — which looks exactly like art that was
            // never built. The toolbar's icons are `data:` images, so the
            // shot needs a provider that answers for those.
            net_provider: Some(Arc::new(DataUris)),
            ..Default::default()
        },
    );
    document.initial_build();
    let Some(panel) = slot.0.borrow().clone() else {
        eprintln!("the arrangement panel never mounted");
        std::process::exit(1);
    };
    // Where the picture looks: the arguments' scroll, `FTS_BLITZ_ZOOM`.
    let (mut across, mut down, mut zoom) = (panel.scroll, panel.down, panel.zoom);
    document.vdom.in_scope(ScopeId::ROOT, || {
        across.set(scroll.0);
        down.set(scroll.1);
        zoom.set(initial_zoom());
    });
    // Several rounds, with a moment between: the panel measures where it
    // is from the layout (asynchronously), and a `data:` image is fetched
    // through the document's resource provider, which answers on another
    // thread. A picture taken before both have answered is a picture of
    // the UI mid-construction — scrollbars in the corner, art missing —
    // which looks exactly like a renderer dropping content.
    for _ in 0..12 {
        tick(&mut document, &panel);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // `FTS_BLITZ_FRAMES=240` measures a gesture instead of taking a
    // picture. The same tree, the same data and the same panel — so the
    // number is the real UI's, not a model of it.
    if let Some(frames) = std::env::var("FTS_BLITZ_FRAMES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        // `FTS_BLITZ_PROFILE=/tmp/frame.svg` writes a flame graph of the
        // run. Reading a renderer's source to work out where a frame
        // goes is how two days disappear into a phase that turns out not
        // to be the one; this says which function, and it costs a
        // feature flag.
        let profiler = profiler();
        pan(&mut document, &panel, width, height, frames);
        report(profiler);
        return;
    }

    let Some(image) = picture(&mut document, width, height) else {
        eprintln!("the renderer returned a buffer the wrong size");
        std::process::exit(1);
    };
    image.save(&out).expect("write the picture");
    println!(
        "wrote {out} — {width}x{height}, scroll ({}, {})",
        scroll.0, scroll.1
    );
}

/// Open the project the way the app does and read it back as the panels
/// read it: prepared if asked, planned by the scene, previews read.
///
/// A window plays the session; a picture only draws it. `open_silent`
/// exists so a render never waits on a sound server — see its docs.
fn open(path: &std::path::Path, audio: bool) -> StudioSession {
    let opened = if audio {
        session_daw::open::open_and_serve(path)
    } else {
        session_daw::open::open_silent(path)
    }
    .expect("open project");
    // Organize / build from the chart / generate the guide, if asked —
    // before the view reads the session, so it opens on the prepared one.
    let prepare = session_daw::prepare::Prepare::from_env();
    if !prepare.is_empty()
        && let Err(e) = prepare.run(&opened)
    {
        tracing::error!(error = %e, "preparing the session failed; opening it as it was");
    }
    // `FTS_BLITZ_EXPORT=name`: the prepared song written as `name.RPP`
    // beside its source, to finish the mix in REAPER.
    if let Ok(name) = std::env::var("FTS_BLITZ_EXPORT") {
        match session_daw::mix_export::write_rpp(&opened.daw, &opened.project_guid, &name) {
            Ok(done) => {
                tracing::info!(path = %done.path.display(), chained = ?done.chained, "blitz: exported")
            }
            Err(e) => tracing::error!(error = %e, "blitz: export failed"),
        }
    }
    // `FTS_BLITZ_LEVELS=from:to`: the staged tracks as heard — each
    // rendered alone over that span — against their targets, then the
    // drum folder's peak and the mix's.
    if let Some((from, to)) = std::env::var("FTS_BLITZ_LEVELS").ok().and_then(|span| {
        span.split_once(':')
            .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
    }) {
        let (heard, kit, mix) =
            session_daw::gain_stage::heard_levels(&opened.daw, &opened.project_guid, from, to);
        for each in &heard {
            tracing::info!(
                track = %each.name,
                role = ?each.role,
                target = each.target,
                heard = (each.level * 10.0).round() / 10.0,
                off = ((each.level - each.target) * 10.0).round() / 10.0,
                "blitz: level"
            );
        }
        tracing::info!(?kit, ?mix, "blitz: peaks");
    }
    let scene: &'static str = match std::env::var("FTS_BLITZ_SCENE") {
        Ok(slug) => dynamic_template::scenes::scene(&slug).map_or_else(
            || {
                tracing::warn!(scene = %slug, "no such scene; laying the session out by the app's");
                SCENE
            },
            |known| known.slug.as_str(),
        ),
        Err(_) => SCENE,
    };
    // `FTS_BLITZ_STAGE=stage` / `clear`: Shift+G / Alt+G, through the
    // facade as the keys run them.
    if let Ok(how) = std::env::var("FTS_BLITZ_STAGE") {
        let clear = how == "clear";
        let said = session_daw::open::block_on_engine(async move {
            let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("no facade"))?;
            let project = daw.current_project().await?;
            if clear {
                session_daw::gain_stage::clear_live(&project)
                    .await
                    .map(|cleared| format!("cleared {}", cleared.len()))
            } else {
                session_daw::gain_stage::stage_live(&project)
                    .await
                    .map(|report| {
                        format!(
                            "staged {:?}",
                            report
                                .staged
                                .iter()
                                .map(|s| (s.name.clone(), (s.gain_db * 10.0).round() / 10.0))
                                .collect::<Vec<_>>()
                        )
                    })
            }
        });
        tracing::info!(?said, "blitz: gain staging");
    }
    // `FTS_BLITZ_COMP=Folder:take:from:to`: a swipe comp of that folder
    // from that take, through the engine, and the first lane track's comp
    // lane read back.
    if let Some((folder, take, from, to)) = std::env::var("FTS_BLITZ_COMP").ok().and_then(|spec| {
        let parts: Vec<String> = spec.split(':').map(str::to_owned).collect();
        Some((
            parts.first()?.clone(),
            parts.get(1)?.parse::<u32>().ok()?,
            parts.get(2)?.parse::<f64>().ok()?,
            parts.get(3)?.parse::<f64>().ok()?,
        ))
    }) {
        let mut project = session_daw::studio::fetch().expect("read the project back");
        let folder = project
            .tracks
            .iter()
            .find(|t| t.is_folder && t.name.trim().eq_ignore_ascii_case(&folder))
            .map(|t| t.guid.clone())
            .expect("the folder");
        let mut effects = Vec::new();
        session_daw::arrange_edit::Editor::comp(
            &mut project,
            &folder,
            take,
            from,
            to,
            &mut effects,
        );
        let sent = effects
            .iter()
            .filter_map(|e| match e {
                session_daw::arrange_edit::Effect::Send(edit) => Some(edit),
                _ => None,
            })
            .filter(|edit| session_daw::engine::apply_now(edit))
            .count();
        let back = session_daw::studio::fetch().expect("read it back");
        let lane = back
            .tracks
            .iter()
            .find(|t| t.lane_count > 0)
            .expect("a lane track");
        let comp = session_daw::arrange_edit::comp_lane(lane);
        let pieces: Vec<(f64, f64, f64)> = back
            .items
            .get(&lane.guid)
            .into_iter()
            .flatten()
            .filter(|i| i.fixed_lane == comp)
            .map(|i| {
                let at = i.position.as_seconds();
                (at, at + i.length.as_seconds(), i.start_offset.as_seconds())
            })
            .collect();
        tracing::info!(sent, track = %lane.name, ?comp, ?pieces, "blitz: comp");
    }
    let raw = session_daw::studio::fetch().expect("read the project back");
    // `FTS_BLITZ_OPEN=Drums,Kick` (or `*`): those folders opened, as a
    // click on each would — the shot of what is under a folder that
    // starts shut.
    if let Ok(names) = std::env::var("FTS_BLITZ_OPEN") {
        let names: Vec<&str> = names.split(',').map(str::trim).collect();
        for track in raw.tracks.iter().filter(|t| t.is_folder) {
            if names.contains(&"*")
                || names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(track.name.trim()))
            {
                session_daw::folds::toggle(&track.guid, false);
            }
        }
    }
    let planner = session_daw::studio::Planner {
        raw: Arc::new(raw),
        scene: Some(scene),
        kinds: Arc::new(session_daw::plan::Kinds::read(path)),
    };
    let (project, rows) = planner.plan(&planner.raw);
    let previews = session_daw::studio::previews_of(&project);
    let chart = prepare.chart.as_deref().and_then(|chart_path| {
        let text = std::fs::read_to_string(chart_path)
            .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not read"))
            .ok()?;
        keyflow::parse(text.as_str())
            .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not parse"))
            .ok()
            .map(Arc::new)
    });
    StudioSession {
        project,
        rows,
        previews,
        chart,
        chart_file: None,
        planner,
    }
}

/// One frame of the headless document's life: the panel's frame tick
/// (the view to the widget, the panel's rectangle re-read), then the
/// document reconciled and laid out.
fn tick(document: &mut DioxusDocument, panel: &ArrangementPanel) {
    // No transport in a headless shot: nothing is playing, so the
    // playhead is at the start.
    document.vdom.in_scope(ScopeId::ROOT, || {
        panel.handle(PanelEvent::Frame { play_at: 0.0 })
    });
    document.poll(None);
    document.inner_mut().resolve(0.0);
}

/// The headless document as a picture.
fn picture(document: &mut DioxusDocument, width: u32, height: u32) -> Option<image::RgbaImage> {
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            let mut inner = document.inner_mut();
            blitz_paint::paint_scene(painter, &mut inner, 1.0, width, height, 0, 0);
        },
        &mut buffer,
    );
    image::RgbaImage::from_raw(width, height, buffer)
}

/// The headless document: the session in context, and the arrangement
/// panel's tree filling the surface.
#[derive(Props, Clone, PartialEq)]
struct StillProps {
    session: StudioSession,
    size: (u32, u32),
    /// Where the panel is handed back, so the harness can drive it.
    slot: Slot,
}

#[component]
fn Still(props: StillProps) -> Element {
    use_context_provider(|| props.session.clone());
    use_context_provider(|| props.slot.clone());
    let (width, height) = props.size;
    rsx! {
        Ground {}
        div {
            style: "position:relative; width:{width}px; height:{height}px; overflow:hidden;",
            HeadlessArrangement {}
        }
    }
}

/// The arrangement panel with no window under it: the app's panel and
/// widget ([`session_daw::panel::use_arrangement_panel`]) and its tree
/// ([`session_daw::panel::NativeTree`]). What `studio::Arrangement` adds
/// on top — winit's events, the pointer's shape, the engine's transport —
/// needs a window, and a headless document has none to give it.
#[component]
fn HeadlessArrangement() -> Element {
    let engine: session_daw::panel::Engine = std::rc::Rc::new(|edit| {
        tracing::debug!(?edit, "a picture carries out no edits");
    });
    let (panel, widget) = session_daw::panel::use_arrangement_panel(
        None,
        engine,
        dioxus_native_dom::CustomWidgetAttr::new,
    );
    rsx! {
        session_daw::panel::NativeTree { panel, widget }
    }
}

/// A window, not a document.
///
/// The margin is the user-agent's eight pixels, which is eight pixels of
/// the session pushed off the bottom and every row eight pixels from
/// where it belongs. And `overflow: hidden` leaves the shell nothing to
/// scroll, so a wheel event reaches the arrangement rather than moving
/// the page under it.
#[component]
fn Ground() -> Element {
    rsx! {
        style {
            "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; \
             overflow: hidden; }}"
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct WindowProps {
    session: StudioSession,
    size: (u32, u32),
    animate: bool,
    docked: bool,
}

/// The studio in a window: the session in context and the app's own DAW
/// panels — the same component the desktop shell mounts — sized to the
/// surface.
#[component]
fn Window(props: WindowProps) -> Element {
    use_context_provider(|| props.session.clone());
    let slot = use_context_provider(Slot::default);
    // The window's ACTUAL size, not the one it was asked for. Laying out
    // at a fixed number is what made the window ignore a resize and show
    // its left third in fullscreen.
    let size = use_signal(|| (f64::from(props.size.0), f64::from(props.size.1)));
    let (width, height) = size();
    rsx! {
        Ground {}
        div {
            style: "position:relative; width:{width}px; height:{height}px; overflow:hidden;",
            session_daw::mixer_panel::DawPanels { docked: props.docked }
        }
        Driver { size, animate: props.animate, slot }
    }
}

/// Keeps `size` in step with the window, runs the gestures when asked,
/// and logs what a frame cost. Draws nothing.
#[component]
fn Driver(size: Signal<(f64, f64)>, animate: bool, slot: Slot) -> Element {
    let handle = dioxus_native::use_window();
    let mut size = size;
    use_hook({
        let handle = handle.clone();
        move || {
            let px = handle.surface_size();
            tracing::info!(width = px.width, height = px.height, "opened at");
            size.set((f64::from(px.width.max(1)), f64::from(px.height.max(1))));
        }
    });
    let started = use_hook(Instant::now);
    // What was last written to the log, so the same number is not written
    // twice, and the recent frames it is the median of.
    let last = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0.0_f64)));
    let counted =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::with_capacity(RECENT))));
    dioxus_native::use_window_event(move |event, _| match event {
        winit::event::WindowEvent::SurfaceResized(px) => {
            tracing::info!(width = px.width, height = px.height, "resized to");
            size.set((f64::from(px.width.max(1)), f64::from(px.height.max(1))));
        }
        winit::event::WindowEvent::RedrawRequested => {
            // The gestures are driven from the window's own clock: the
            // thing that IS guaranteed to happen every frame is the frame,
            // and asking for the next one from inside it is an animation
            // loop at whatever rate the window can present.
            if animate && let Some(panel) = slot.0.borrow().as_ref() {
                let elapsed = started.elapsed().as_secs_f64();
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "whole gestures elapsed, small and positive"
                )]
                let index = (elapsed / GESTURE_SECS) as usize % GESTURES.len();
                let (_, at) = GESTURES[index];
                let t = (elapsed % GESTURE_SECS) / GESTURE_SECS;
                let (x, y, zx, zy) = at(t);
                let (mut scroll, mut down, mut zoom) = (panel.scroll, panel.down, panel.zoom);
                // The zoom BEFORE how far the view may travel, or the
                // gesture spends the frame somewhere the previous zoom
                // allowed and this one does not.
                zoom.set((zx, zy));
                let (across, deep) = panel.extent(*panel.rect.peek(), zx, zy);
                scroll.set(x * across);
                down.set(y * deep);
                handle.request_redraw();
            }
            log_frame(&counted, &last);
        }
        _ => {}
    });
    rsx! {}
}

/// What the frame the shell just drew cost, to the log — rate-limited to
/// a change in the recent median, because a line a frame is a log nobody
/// reads. What the WINDOW shows is the widget's own graph
/// (`FTS_BLITZ_FPS=1`); this is for reading a run back afterwards.
fn log_frame(
    counted: &std::rc::Rc<std::cell::RefCell<Vec<f64>>>,
    last: &std::rc::Rc<std::cell::Cell<f64>>,
) {
    // This long is the shell having done something other than draw this
    // window, not a frame that took this long.
    const IDLE: f64 = 200.0;
    let ms = |micros: u64| f64::from(u32::try_from(micros).unwrap_or(u32::MAX)) / 1000.0;
    let cost = ms(blitz_traits::LAST_FRAME_MICROS.load(std::sync::atomic::Ordering::Relaxed));
    let mut state = counted.borrow_mut();
    if cost.max(0.01) < IDLE {
        if state.len() == RECENT {
            state.remove(0);
        }
        state.push(cost.max(0.01));
    }
    if state.len() < 8 {
        return;
    }
    let mut recent = state.clone();
    recent.sort_by(f64::total_cmp);
    let middle = recent[recent.len() / 2];
    let worst = recent.last().copied().unwrap_or(middle);
    if (middle - last.get()).abs() > 0.5 {
        last.set(middle);
        // Split at the hand-off to the GPU. A window that draws too much
        // and one that draws little and waits on the compositor are the
        // same frame time and want opposite fixes. See #119.
        let encode =
            ms(blitz_traits::LAST_ENCODE_MICROS.load(std::sync::atomic::Ordering::Relaxed));
        let present =
            ms(blitz_traits::LAST_PRESENT_MICROS.load(std::sync::atomic::Ordering::Relaxed));
        tracing::info!(
            frame_ms = format!("{middle:.1}"),
            worst_ms = format!("{worst:.1}"),
            encode_ms = format!("{encode:.2}"),
            present_ms = format!("{present:.2}"),
            "presented"
        );
    }
}

/// The benchmark's gestures, as the window runs them: where the view
/// should be at `t` (0..1 across the gesture), as fractions of the
/// travel and the two zooms.
const GESTURES: [(&str, fn(f64) -> (f64, f64, f64, f64)); 7] = [
    ("scroll down/up", |t| (0.0, tri(t), 1.0, 1.0)),
    ("scroll right/left", |t| (tri(t), 0.0, 1.0, 1.0)),
    ("scroll both", |t| (tri(t), tri((t * 1.7) % 1.0), 1.0, 1.0)),
    ("zoom vertical", |t| (0.0, 0.3, 1.0, 0.25 + slow(t) * 3.75)),
    ("zoom horizontal", |t| {
        (0.0, 0.3, 0.25 + slow(t) * 7.75, 1.0)
    }),
    ("zoom both", |t| {
        (0.0, 0.3, 0.25 + slow(t) * 7.75, 0.25 + slow(t) * 3.75)
    }),
    ("fit whole session", |t| {
        (0.0, 0.0, 1.0, 0.08 + slow(t) * 0.4)
    }),
];

/// How many full traversals a scrolling gesture makes: someone throwing
/// the scrollbar from end to end, not a gentle sweep.
const LAPS: f64 = 14.0;

/// A triangle wave: out to the far end and all the way back.
fn tri(t: f64) -> f64 {
    let t = (t * LAPS) % 1.0;
    if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
}

/// The zoom's sweep — three passes over the range, not fourteen. A zoom
/// is a wheel or a pinch, not a yank.
fn slow(t: f64) -> f64 {
    let t = (t * 3.0) % 1.0;
    if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
}

/// How long each gesture runs before the next.
const GESTURE_SECS: f64 = 6.0;

/// How many recent frames the log's median is taken over.
const RECENT: usize = 32;

/// A sampling profiler over the benchmark, when one is asked for.
///
/// `pprof` samples on a SIGPROF timer rather than through `perf`, which
/// matters because this box has no `perf` and
/// `kernel.perf_event_paranoid` would not allow it anyway.
#[cfg(feature = "profile")]
fn profiler() -> Option<(pprof::ProfilerGuard<'static>, String)> {
    let out = std::env::var("FTS_BLITZ_PROFILE").ok()?;
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(2000)
        // The frames are short and the interesting work is deep, so the
        // default blocklist matters: without it every sample lands in
        // libc and the graph says nothing.
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .ok()?;
    Some((guard, out))
}

#[cfg(not(feature = "profile"))]
const fn profiler() -> Option<()> {
    None
}

/// Write the flame graph out, if one was being collected.
#[cfg(feature = "profile")]
fn report(profiler: Option<(pprof::ProfilerGuard<'static>, String)>) {
    let Some((guard, out)) = profiler else {
        return;
    };
    match guard.report().build() {
        Ok(report) => match std::fs::File::create(&out) {
            Ok(file) => {
                if let Err(error) = report.flamegraph(file) {
                    eprintln!("could not write {out}: {error}");
                } else {
                    println!("  flame graph: {out}");
                }
            }
            Err(error) => eprintln!("could not open {out}: {error}"),
        },
        Err(error) => eprintln!("the profiler reported nothing: {error}"),
    }
}

#[cfg(not(feature = "profile"))]
const fn report(_profiler: Option<()>) {}

/// Which gesture the headless benchmark runs.
///
/// A pan and a zoom are not the same measurement and never were: a pan
/// moves one transform, a zoom changes where every item is. Reporting
/// one number for "the window" hid that for weeks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gesture {
    /// Across the session.
    Pan,
    /// And down it, which is the axis the track panel shares.
    Down,
    /// In and out horizontally.
    ZoomX,
    /// And vertically.
    ZoomY,
}

impl Gesture {
    /// The gesture named by `FTS_BLITZ_GESTURE`, defaulting to the pan.
    fn from_env() -> Self {
        match std::env::var("FTS_BLITZ_GESTURE").as_deref() {
            Ok("down") => Self::Down,
            Ok("zoom-x" | "zoomx") => Self::ZoomX,
            Ok("zoom-y" | "zoomy") => Self::ZoomY,
            _ => Self::Pan,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Pan => "pan",
            Self::Down => "down",
            Self::ZoomX => "zoom-x",
            Self::ZoomY => "zoom-y",
        }
    }
}

/// How far down a benchmarked vertical scroll travels.
///
/// Past the end of most sessions on purpose: the interesting frames are
/// the ones where a band of rows comes on screen and goes off it, and the
/// last of those is at the bottom.
const DOWN_TRAVEL: f64 = 6000.0;

/// How far either way a benchmarked zoom travels: four times in and four
/// times out is a gesture a hand makes in about a second.
const ZOOM_SWING: f64 = 4.0;

/// Put the view where the gesture says it is, `t` of the way through.
///
/// Writing the panel's own view rather than simulating input, because
/// what is being measured is what the tree costs at a given view — not
/// what winit costs to deliver a wheel notch.
fn drive(document: &DioxusDocument, panel: &ArrangementPanel, gesture: Gesture, t: f64) {
    let (mut scroll, mut down, mut zoom) = (panel.scroll, panel.down, panel.zoom);
    document.vdom.in_scope(ScopeId::ROOT, || match gesture {
        // A minute of session under the playhead, which moves every item
        // on screen and changes which ones are there at all.
        Gesture::Pan => scroll.set(t * 60.0 * PPS),
        Gesture::Down => down.set(t * DOWN_TRAVEL),
        Gesture::ZoomX | Gesture::ZoomY => {
            // In and back out again. Exponential because a zoom is
            // multiplicative — a linear sweep spends most of its frames at
            // the far end and measures the wrong thing.
            let scale = ZOOM_SWING.powf((t * std::f64::consts::TAU).sin());
            if gesture == Gesture::ZoomX {
                zoom.set((scale, 1.0));
            } else {
                zoom.set((1.0, scale));
            }
        }
    });
}

/// Drive a gesture through the headless document and say what a frame
/// of it costs.
///
/// Split the way the other benchmarks split it, because a frame time says
/// a gesture is slow and only the split says which pass is: Dioxus
/// reconciling the tree, then Stylo and Taffy solving what came out, then
/// the scene being encoded for the GPU.
fn pan(
    document: &mut DioxusDocument,
    panel: &ArrangementPanel,
    width: u32,
    height: u32,
    frames: usize,
) {
    let gesture = Gesture::from_env();
    let dump = std::env::var("FTS_BLITZ_DUMP").ok();
    if let Some(dir) = dump.as_deref() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut renderer =
        session_daw::headless::Headless::new(width, height).expect("a headless renderer");
    let mut stages = session_daw::profile::Stages::with_capacity(frames);
    let mut diff = session_daw::profile::Samples::with_capacity(frames);
    let mut solve = session_daw::profile::Samples::with_capacity(frames);
    let batch = session_daw::headless::BATCH;

    for chunk in 0..frames / batch {
        let started = Instant::now();
        let mut painted = 0.0;
        for step in 0..batch {
            let frame = chunk * batch + step;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a frame index over a few hundred"
            )]
            let t = frame as f64 / frames as f64;
            drive(document, panel, gesture, t);
            document.vdom.in_scope(ScopeId::ROOT, || {
                panel.handle(PanelEvent::Frame { play_at: 0.0 })
            });
            let at = Instant::now();
            document.poll(None);
            diff.push_ms(at.elapsed().as_secs_f64() * 1000.0);
            let at = Instant::now();
            document.inner_mut().resolve(0.0);
            solve.push_ms(at.elapsed().as_secs_f64() * 1000.0);
            painted += renderer
                .frame(|painter| {
                    let mut inner = document.inner_mut();
                    blitz_paint::paint_scene(painter, &mut inner, 1.0, width, height, 0, 0);
                })
                .expect("render a frame");
            // `FTS_BLITZ_DUMP=/tmp/frames` writes every frame out. A fault
            // that only exists while something is moving cannot be found
            // by rendering a still at the same place — the state that is
            // wrong is the state left over from the frame before.
            if let Some(dir) = dump.as_deref()
                && let Some(image) = picture(document, width, height)
            {
                let _ = image.save(format!("{dir}/{frame:04}.png"));
            }
        }
        renderer.wait().expect("the gpu to finish the batch");
        #[expect(clippy::cast_precision_loss, reason = "a batch size of thirty")]
        let batch = batch as f64;
        stages
            .frame
            .push_ms(started.elapsed().as_secs_f64() * 1000.0 / batch);
        stages.paint.push_ms(painted / batch);
    }

    let (Some(frame), Some(paint), Some(diff), Some(solve)) = (
        stages.frame.summary(),
        stages.paint.summary(),
        diff.summary(),
        solve.summary(),
    ) else {
        println!("  nothing measured");
        return;
    };
    // How many nodes are in the tree, because that is the unit style and
    // layout are paid in: they scale with what is THERE, not with what
    // changed.
    let nodes = document.inner().tree().len();
    println!(
        "\n  The arrangement panel, {} across the session\n",
        gesture.name()
    );
    println!("  surface       {width}x{height}");
    println!("  tree          {nodes} nodes");
    println!(
        "  {:<12} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "", "mean", "p99", "worst", "style+lay", "paint", "fps(p99)"
    );
    println!("  {}", "-".repeat(74));
    println!(
        "  {:<12} {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>9.0}",
        gesture.name(),
        frame.mean,
        frame.p99,
        frame.worst,
        solve.mean,
        paint.mean,
        1000.0 / frame.p99.max(0.001)
    );
    println!(
        "\n  reconciling   p50 {:>6.2}ms   p99 {:>6.2}ms   worst {:>6.2}ms",
        diff.p50, diff.p99, diff.worst
    );
    println!(
        "  style+layout  p50 {:>6.2}ms   p99 {:>6.2}ms   worst {:>6.2}ms",
        solve.p50, solve.p99, solve.worst
    );
}

/// A resource provider that answers `data:` URIs and nothing else.
///
/// Blitz ships a real one in `blitz-net`, which fetches over HTTP and
/// off the filesystem and is asynchronous. A renderer that draws one
/// frame and exits wants neither: it wants the image it just built,
/// now, on the thread that asked. Anything else is left unanswered on
/// purpose — a shot that quietly reached the network would be a shot
/// whose picture depended on this machine.
struct DataUris;

impl blitz_traits::net::NetProvider for DataUris {
    fn fetch(
        &self,
        _doc: usize,
        request: blitz_traits::net::Request,
        handler: Box<dyn blitz_traits::net::NetHandler>,
    ) {
        let url = request.url.as_str();
        let Some(payload) = url.strip_prefix("data:") else {
            return;
        };
        let Some((_, body)) = payload.split_once(',') else {
            return;
        };
        handler.bytes(url.to_owned(), unescape(body).into());
    }
}

/// Undo the percent-encoding a `data:` URI carries.
fn unescape(body: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut bytes = body.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            if let Ok(decoded) = u8::from_str_radix(&hex, 16) {
                out.push(decoded);
                continue;
            }
        }
        out.push(byte);
    }
    out
}

/// The surface, `FTS_BLITZ_SIZE=WxH` (2560x1440 by default).
fn size() -> (u32, u32) {
    std::env::var("FTS_BLITZ_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((2560, 1440))
}

/// The zoom a still is taken at, from `FTS_BLITZ_ZOOM=x,y`.
fn initial_zoom() -> (f64, f64) {
    std::env::var("FTS_BLITZ_ZOOM")
        .ok()
        .and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .unwrap_or((1.0, 1.0))
}
