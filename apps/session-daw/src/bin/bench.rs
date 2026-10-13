//! The arrangement, benchmarked headlessly.
//!
//! ```sh
//! just daw-bench                       # the synthetic 65/877 fixture
//! just daw-bench 200 20000             # an orchestral template
//! ```
//!
//! No window, no surface, no vsync. `anyrender`'s `ImageRenderer` draws
//! into a buffer, so this measures what the RENDERER can do rather than
//! what a monitor will accept — which is the number that says whether
//! there is headroom, and the only one that is comparable between
//! machines.
//!
//! The windowed build is pinned to whichever display it opens on: 240 fps
//! on the 240 Hz panel, 180 on the 180 Hz one, both of them simply vsync.
//! Useful for "does it keep up", useless for "how much room is left".
//! This answers the second question, and it runs on a headless box, in
//! CI, or over ssh.
//!
//! It is not a substitute for the windowed number. There is no
//! compositor here and no present, so this is the upper bound on drawing
//! alone. Quote it as that.
//!
//! The arrangement is drawn through `ArrangementWidget` — the painter the
//! window mounts — over the session as the window reads it (see
//! `Session`). There used to be a second painter here, hand-assembled
//! from the same passes, and a number from a copy is a number about the
//! copy. `FTS_BENCH_VERIFY` is the exception on purpose: it checks the
//! recording's culling against replaying all of it, below any painter.

use std::time::Instant;

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello::VelloImageRenderer;
use vello::kurbo::Affine;

use session_daw::arrangement::{Arrangement, Palette, TCP_WIDTH, Viewport};
use session_daw::headless::{BATCH, Headless};
use session_daw::mcp::Mixer;
use session_daw::widget::{ArrangementWidget, Shared, View};

/// The phase these measurements are taken in — the one the rack was
/// built for, and the one every reference image was shot in.
const TONE: session::mix_phases::MixPhase = session::mix_phases::MixPhase::Tone;
use session_daw::profile::{Counts, Stages, Summary};
use session_daw::ruler::ruler_h;

/// One phase's motion over `0..1`, as (`scroll_x`, `scroll_y`,
/// `zoom_x`, `zoom_y`) fractions.
///
/// Boxed rather than a plain `fn` because the fit-everything phase has
/// to close over the scene's height — the zoom at which a session fits
/// depends on how tall it is, and a fixture-independent phase cannot be
/// a constant.
type Gesture = Box<dyn Fn(f64) -> (f64, f64, f64, f64)>;

/// The surface to draw into, `WIDTHxHEIGHT`.
///
/// Defaults to the ultrawide's native 5120x1440 — 7.4 megapixels, 3.2x a
/// 1600x900 window. A 2D renderer is fill-rate bound as much as
/// command-bound, so a number from a small surface says little about the
/// screen this actually runs on.
const SIZE_ENV: &str = "FTS_BENCH_SIZE";
const DEFAULT_SIZE: (u32, u32) = (5120, 1440);
/// Enough to see past the first-frame costs and get a stable median.
const FRAMES: usize = 240;
/// Pixels per second, matching the window's opening zoom.
const PPS: f64 = 40.0;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,bench=info,session_daw=info".into()),
        )
        .init();

    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SESSION_DAW_PROJECT").ok())
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists());
    let Some(path) = path else {
        eprintln!("bench needs a project: cargo run --bin bench -- <song.rpp>");
        std::process::exit(2);
    };

    let (width, height) = std::env::var(SIZE_ENV)
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(DEFAULT_SIZE);

    let theme = daw_ui::theming::Theme::dark();
    // In the theme's colours, where it names them (FTS_THEME).
    session_daw::skin::from_env();
    let palette = session_daw::skin::palette(&Palette::from_theme(&theme));
    let font = session_daw::text::Font::embedded().expect("the embedded font");
    let layout = session_daw::layout::Layout::from_env();

    if let Ok(out) = std::env::var("FTS_BENCH_KIT") {
        // The audio drum workflow: a tracked kit's mics stacked as role
        // lanes, `FTS_BENCH_BARS` bars of hits (two hundred by default).
        kit_shot(&palette, &std::path::PathBuf::from(out), width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_PATCH_LIST") {
        // The Patch List view over the fixture album in the fixture
        // room — no project needed: the plan is the album's, not the
        // session's, and the picture is its committed fixture.
        patch_list_shot(&std::path::PathBuf::from(out), width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_PATCH_LIST_OVERRIDDEN_STALE") {
        // The same view with a session override on Cody's DI and a
        // stale banner (#57's fixture — #48's amendment: a structural
        // threshold plus a negative control, not byte-identical
        // pixels).
        patch_list_shot_overridden_stale(&std::path::PathBuf::from(out), width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_EXPRESSION") {
        // The expression editor over the demo drum groove — no project
        // needed, which is the point: the view is exercisable before
        // a session has any MIDI in it.
        expression_shot(&palette, &std::path::PathBuf::from(out), width, height);
        return;
    }

    // Silent: this draws a project and exits. It has nothing to play,
    // and an audio engine it never uses costs it a dependency on a
    // working sound server — which is how three render tests came to
    // time out at ten minutes each on a box whose audio was fine.
    let opened = session_daw::open::open_silent(&path).expect("open project");
    if let Ok(out) = std::env::var("FTS_BENCH_FOLDER_ITEMS") {
        // The folder items of the open session, folded from the
        // children's REAL peaks — see `folder_items_shot`.
        folder_items_shot(&palette, &std::path::PathBuf::from(out), width, height);
        return;
    }
    if std::env::var_os("FTS_BENCH_DEPTHS").is_some() {
        depths();
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_PLAN") {
        plan_dump(&path, &std::path::PathBuf::from(out));
        return;
    }
    // `FTS_BENCH_FX=out.png`: the FX — `FTS_BENCH_FX_LANES` tracks of
    // it (one, docked), from the selected one; `FTS_BENCH_FX_FOLD` the
    // phases folded, by name (`rescue,polish`), and `returns` the
    // returns' effects.
    // `FTS_BENCH_EDITOR=out.png`: the Editor view on the track
    // `FTS_BENCH_EDITOR_TRACK` names (a folder of takes stacks them).
    if let Ok(out) = std::env::var("FTS_BENCH_EDITOR") {
        editor_shot(&palette, &std::path::PathBuf::from(out), width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_FX") {
        fx_shot(&palette, &std::path::PathBuf::from(out), width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_MIXER") {
        mixer_shot(
            &palette,
            &font,
            layout,
            &std::path::PathBuf::from(out),
            &path,
            width,
            height,
        );
        return;
    }
    // The session as the window reads it: the same rows, the same
    // previews, the same recording — see `Session`.
    let session = Session::read(&path).expect("read project back");
    let scene = session.scene(&palette, &font, layout);
    tracing::info!(
        project.tracks = opened.track_count,
        scene.rows = scene.rows,
        "benchmarking"
    );

    if std::env::var_os("FTS_BENCH_STUDIO").is_some() {
        studio(&session, &scene, &palette, &font, layout, width, height);
        return;
    }
    if let Ok(out) = std::env::var("FTS_BENCH_DOCK") {
        // The arrangement with the editor docked under it, as the
        // studio benchmark draws its first frame.
        dock_shot(
            &session,
            &palette,
            &std::path::PathBuf::from(out),
            width,
            height,
        );
        return;
    }

    // The same spans the window's scrollbars travel: the session less
    // the lanes' share of the widget, which is the widget less the
    // track panel across and less the ruler down.
    let span_y = (scene.content_height() - (f64::from(height) - ruler_h())).max(1.0);
    let span_x = (scene.length_secs * PPS - (f64::from(width) - TCP_WIDTH)).max(1.0);

    // One phase per GESTURE, because they are different work and a
    // blended average hides which one hurts. Scrolling down changes
    // which lanes are on screen; scrolling across changes which part of
    // every lane; zooming changes the geometry of everything at once and
    // is the only one that can make more of the session visible than the
    // screen has ever held.
    //
    // `t` runs 0..1 over the phase. Each returns (scroll_x, scroll_y,
    // zoom_x, zoom_y).
    /// How many full end-to-end traversals each phase performs.
    ///
    /// Deliberately brutal: at this rate a phase crosses all 2,000
    /// tracks in about eight frames — someone grabbing the scrollbar and
    /// yanking it from top to bottom, not a gentle sweep. That is the
    /// case that matters, and it is the worst case for anything we might
    /// later cache or cull, because consecutive frames share almost no
    /// visible content.
    const LAPS: f64 = 14.0;

    // Vertical zoom at which every track fits on screen at once.
    // Computed from the scene rather than guessed, so the phase means
    // the same thing whatever fixture it is pointed at.
    let fit = f64::from(height) / scene.content_height().max(1.0);

    /// A triangle wave: out to the far end of the session and all the way
    /// back, `LAPS` times over the phase.
    fn tri(t: f64) -> f64 {
        let t = (t * LAPS) % 1.0;
        if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
    }
    /// The zoom sweep: three passes over the range, not fourteen.
    fn slow(t: f64) -> f64 {
        let t = (t * 3.0) % 1.0;
        if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
    }

    let phases: Vec<(&str, Gesture)> = vec![
        ("scroll down/up", Box::new(|t| (0.0, tri(t), 1.0, 1.0))),
        ("scroll right/left", Box::new(|t| (tri(t), 0.0, 1.0, 1.0))),
        (
            "scroll both",
            Box::new(|t| (tri(t), tri((t * 1.7) % 1.0), 1.0, 1.0)),
        ),
        // A zoom is a slower gesture than a yank — a wheel or a pinch,
        // not a thrown scrollbar — so these sweep their range a few times
        // rather than fourteen.
        (
            "zoom vertical",
            Box::new(|t| (0.0, 0.3, 1.0, 0.25 + slow(t) * 3.75)),
        ),
        (
            "zoom horizontal",
            Box::new(|t| (0.0, 0.3, 0.25 + slow(t) * 7.75, 1.0)),
        ),
        (
            "zoom both",
            Box::new(|t| (0.0, 0.3, 0.25 + slow(t) * 7.75, 0.25 + slow(t) * 3.75)),
        ),
        // The whole session at once, which is what per-track heights and
        // a two-pixel floor are FOR. Culling cannot help here by
        // definition — every row is on screen — so this is the genuine
        // worst case and it belongs in the table rather than in a
        // footnote about how fast the other phases are.
        (
            "fit whole session",
            Box::new(move |t| (0.0, 0.0, 1.0, fit + slow(t) * (0.25 - fit))),
        ),
    ];

    if std::env::var("FTS_BENCH_VERIFY").is_ok() {
        // Verification is the one place a readback is the POINT: it
        // compares pixels, so it uses the image renderer rather than the
        // texture-only one the timings use.
        let mut image = VelloImageRenderer::new(width, height);
        verify(&mut image, &scene, &phases, span_x, span_y, width, height);
        return;
    }

    if let Ok(out) = std::env::var("FTS_BENCH_SHOT") {
        // Look at a frame instead of arguing about one. Renders the
        // window's opening view and writes it to a PNG, which is the
        // fastest way to tell a culling bug (geometry missing) from a
        // palette bug (geometry there, wrong colour).
        shot(
            &session,
            &scene,
            &palette,
            &std::path::PathBuf::from(out),
            width,
            height,
        );
        return;
    }

    println!();
    println!(
        "  scene         {} rows, {} items",
        scene.rows,
        scene.items()
    );
    println!(
        "  surface       {width}x{height}, headless  ({:.1} megapixels)",
        f64::from(width) * f64::from(height) / 1_000_000.0
    );
    println!("  frames        {FRAMES} per phase");
    println!(
        "  drawn         through ArrangementWidget, the window's own painter — \n\
         \x20               lanes, titles, panel, ruler, controls, cursors\n"
    );
    println!(
        "  measured      frames rendered to a texture in batches of {BATCH}, waited on once\n\
         \x20               per batch — no readback, because the app never does one\n"
    );
    println!(
        "  {:<20} {:>9} {:>9} {:>9} {:>9}   {:>7} {:>7}",
        "phase", "mean", "p99", "worst", "fps(p99)", "paint", "gpu"
    );
    println!("  {}", "-".repeat(78));

    if let Ok(faces) = std::env::var("FTS_BENCH_FACES") {
        faces_bench(&faces);
        return;
    }
    if std::env::var_os("FTS_BENCH_ANIMATE").is_some() {
        animate(&palette, &font, layout, width, height);
        return;
    }

    let mut renderer = Headless::new(width, height).expect("open a gpu device");
    // One widget for every phase, as one window is: what it caches
    // between frames (the controls, cut once the zoom settles) is part
    // of what a gesture costs.
    let (mut widget, view) = session.widget();
    let mut all: Vec<(&str, Summary, Summary, Counts)> = Vec::new();
    for (name, gesture) in phases.iter() {
        let mut stages = Stages::with_capacity(FRAMES);
        let mut counts = Counts::default();
        // Frames go out in batches and the GPU is waited on once at the
        // end of each. Waiting per frame would measure a pipeline
        // deliberately drained, which no running app ever is; waiting
        // never would measure how fast work can be QUEUED.
        for batch in 0..FRAMES / BATCH {
            let batch_start = Instant::now();
            let mut painted = 0.0;
            for step in 0..BATCH {
                let frame = batch * BATCH + step;
                let t = frame as f64 / FRAMES as f64;
                let (fx, fy, zx, zy) = gesture(t);
                *view.borrow_mut() = View {
                    scroll_x: span_x * fx,
                    scroll_y: span_y * fy,
                    zoom_x: zx,
                    zoom_y: zy,
                    play_at: 0.0,
                };
                painted += renderer
                    .frame(|painter| {
                        painter
                            .append_scene(widget.paint_scene(width, height, 1.0), Affine::IDENTITY);
                    })
                    .expect("render a frame");
                counts = drawn(&widget);
            }
            renderer.wait().expect("the gpu to finish the batch");
            // Per-frame, so a batch is comparable to a frame budget.
            let per_frame = batch_start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64;
            stages.frame.push_ms(per_frame);
            stages.paint.push_ms(painted / BATCH as f64);
        }
        // The first batch warms up geometry the previous phase never
        // touched; that is the cost of CHANGING gesture, not of making it.
        stages.frame.drop_warmup(1);
        stages.paint.drop_warmup(1);
        let frame = stages.frame.summary().expect("batches");
        let paint = stages.paint.summary().expect("batches");
        println!(
            "  {name:<20} {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>9.0}   {:>7.2} {:>7.2}",
            frame.mean,
            frame.p99,
            frame.worst,
            frame.fps(),
            paint.mean,
            (frame.mean - paint.mean).max(0.0),
        );
        all.push((*name, frame, paint, counts));
    }

    let worst = all
        .iter()
        .max_by(|a, b| a.1.p99.partial_cmp(&b.1.p99).expect("no NaN"))
        .expect("at least one phase");
    let (name, frame, paint, counts) = worst;
    let gpu = (frame.mean - paint.mean).max(0.0);
    println!(
        "\n  worst gesture: {name} at {:.2}ms p99 ({:.0} fps)",
        frame.p99,
        frame.fps()
    );
    // Where the worst frame's time went, and how much of the scene it
    // had to walk to spend it. If paint dominates, the win is in what we
    // submit; if gpu dominates, it is in what we ask Vello to rasterise.
    println!(
        "    paint {:.2}ms ({:.0}%)   gpu {:.2}ms ({:.0}%)",
        paint.mean,
        paint.mean * 100.0 / frame.mean.max(0.001),
        gpu,
        gpu * 100.0 / frame.mean.max(0.001),
    );
    println!(
        "    {} commands walked, {} submitted ({:.0}% kept)",
        counts.replayed,
        counts.submitted,
        counts.kept_pct().unwrap_or(0.0),
    );
    println!(
        "  headroom there at 240Hz: {:.2}x   at 60Hz: {:.2}x\n",
        (1000.0 / 240.0) / frame.p99,
        (1000.0 / 60.0) / frame.p99,
    );
}

/// How many bars of groove the synthetic kit carries.
fn bench_bars() -> usize {
    std::env::var("FTS_BENCH_BARS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(200)
}

/// One frame of the stacked audio kit, to a PNG.
///
/// `FTS_BENCH_KIT=/tmp/kit.png`, zoomed to a four-bar page so the hits
/// are markers on the waveforms rather than a solid bar.
fn kit_shot(palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    let size = (f64::from(width), f64::from(height));
    let mut view = session_daw::expression::Expression::audio_kit(bench_bars(), (0.0, 0.0), size);
    view.set_look(session_daw::expression::look_of(palette));
    view.editor.frame_bars(4);
    view.editor.playhead = Some(view.editor.doc.time_base.units_per_second(view.editor.bpm) * 3.2);
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, size.0, size.1),
            );
            view.paint(painter);
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    let hits: usize = (0..view.editor.tracks.len())
        .map(|i| {
            if i == view.editor.tracks.active() {
                view.editor.doc.notes.len()
            } else {
                view.editor.tracks.doc_of(i).map_or(0, |d| d.notes.len())
            }
        })
        .sum();
    println!(
        "  wrote {} — {} mics, {} hits over {} bars",
        out.display(),
        view.editor.tracks.len(),
        hits,
        bench_bars()
    );
}

/// The folders a fixture sheet draws, and what each folds by.
///
/// Three, deliberately: a whole kit, one piece of it, and a double. The
/// first two are `flow.drums.comping.folder-items` and
/// `.folder-item-colours`; the third is `flow.guitars.folder-items`, and
/// it is a stereo part rather than a folder because in a session that
/// has not grown its channels out a double IS one stereo track.
const SHEET: [(&str, session_daw::folder_item::GroupBy); 3] = [
    ("Drums", session_daw::folder_item::GroupBy::Role),
    ("Snare", session_daw::folder_item::GroupBy::Role),
    ("Rhythm", session_daw::folder_item::GroupBy::Side),
];

/// One sheet of folder items over the open session, to a PNG.
///
/// `FTS_BENCH_FOLDER_ITEMS=/tmp/folder-items.png`, with
/// `FTS_BENCH_WINDOW=t0,t1` for the zoom (seconds; the whole project by
/// default) and `FTS_BENCH_FOLD=<file>` for the fold beside it — the
/// exact half of the fixture, which is what carries the decision.
///
/// The peaks are the session's own, read back through the facade:
/// nothing here is simulated.
fn folder_items_shot(palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    use session_daw::folder_item::{FolderItems, Place};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let Some(snapshot) = rt.block_on(daw_ui::studio::project::fetch()) else {
        eprintln!("could not read the project back");
        return;
    };
    let Some(daw) = daw_control::Daw::try_get() else {
        eprintln!("no facade");
        return;
    };
    let Ok(project) = rt.block_on(daw.current_project()) else {
        eprintln!("no current project");
        return;
    };
    // Named rather than picked by guid: the fixture is about what a
    // folder of a kit and a double look like, and the golden session's
    // names are the stable thing about it.
    let wanted: Vec<(String, session_daw::folder_item::GroupBy)> = SHEET
        .iter()
        .filter_map(|(name, group_by)| {
            snapshot
                .tracks
                .iter()
                .find(|t| t.name == *name)
                .map(|t| (t.guid.clone(), *group_by))
        })
        .collect();
    let (folders, found) = rt.block_on(session_daw::folder_item::load::load(
        &project,
        &snapshot,
        &wanted,
        palette.text_faint,
    ));
    if folders.is_empty() {
        eprintln!("no folder in this project has anything to fold");
        return;
    }

    let (from, to) = std::env::var("FTS_BENCH_WINDOW")
        .ok()
        .and_then(|s| {
            let (a, b) = s.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .unwrap_or((0.0, snapshot.length_secs));
    let span = (to - from).max(1e-6);
    let size = (f64::from(width), f64::from(height));
    let pixels_per_sec = size.0 / span;
    let row_height = size.1 / session_daw::num::coord(folders.len().max(1));
    let pad = (row_height * 0.08).min(12.0);

    if let Ok(path) = std::env::var("FTS_BENCH_FOLD") {
        let text = session_daw::folder_item::fixture_text(&folders, 0, from, to);
        std::fs::write(&path, text).expect("write the fold");
    }

    let mut items = FolderItems::new(folders);
    let places: Vec<(usize, Place)> = (0..items.folders.len())
        .filter_map(|i| {
            let folder = items.folders.get(i)?;
            Some((
                i,
                Place {
                    x0: (folder.start_secs - from) * pixels_per_sec,
                    top: session_daw::num::coord(i).mul_add(row_height, pad),
                    width: folder.length_secs * pixels_per_sec,
                    height: row_height - pad * 2.0,
                },
            ))
        })
        .collect();

    let mut image = VelloImageRenderer::new(width, height);
    let mut shoot = |items: &mut FolderItems, to: &std::path::Path| {
        let mut buffer = Vec::new();
        image.render_to_vec(
            |painter| {
                painter.reset();
                painter.fill(
                    vello::peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    palette.surface,
                    None,
                    &vello::kurbo::Rect::new(0.0, 0.0, size.0, size.1),
                );
                for (i, place) in &places {
                    items.paint(painter, *i, 0, *place, pixels_per_sec);
                }
            },
            &mut buffer,
        );
        image::save_buffer(to, &buffer, width, height, image::ColorType::Rgba8)
            .expect("write the sheet");
        let (hits, misses) = items.counts();
        println!(
            "  wrote {} — picture cache: {hits} hits, {misses} misses",
            to.display()
        );
    };
    shoot(&mut items, out);
    println!(
        "  {} folders, {} children, {} items, {from:.3}..{to:.3}s at {pixels_per_sec:.1} px/s; \
         off-rate children: {} (worst drift {:.4})",
        found.folders, found.children, found.items, found.off_rate, found.worst_drift,
    );

    // The same sheet with one child hidden, and again with it muted —
    // in THIS process, so the two are comparable byte for byte rather
    // than across a rasteriser. Hiding must replay the held picture;
    // muting must build a new one.
    let beside = |suffix: &str| out.with_extension(format!("{suffix}.png"));
    if let Ok(name) = std::env::var("FTS_BENCH_FOLDER_HIDE") {
        let moved = set_on_every_folder(&mut items, &name, true, false);
        println!("  hid {name} on {moved} folders");
        shoot(&mut items, &beside("hidden"));
        set_on_every_folder(&mut items, &name, false, false);
    }
    if let Ok(name) = std::env::var("FTS_BENCH_FOLDER_MUTE") {
        let moved = set_on_every_folder(&mut items, &name, true, true);
        println!("  muted {name} on {moved} folders");
        shoot(&mut items, &beside("muted"));
    }
}

/// Mute or hide every child called `name`, and say on how many folders
/// something moved.
fn set_on_every_folder(
    items: &mut session_daw::folder_item::FolderItems,
    name: &str,
    to: bool,
    mute: bool,
) -> usize {
    let mut moved = 0_usize;
    for folder in &mut items.folders {
        let guids: Vec<String> = folder
            .children
            .iter()
            .filter(|c| c.name == name)
            .map(|c| c.guid.clone())
            .collect();
        for guid in guids {
            let changed = if mute {
                folder.set_muted(&guid, to)
            } else {
                folder.set_hidden(&guid, to)
            };
            if changed {
                moved = moved.saturating_add(1);
            }
        }
    }
    moved
}

/// Write one frame of the expression editor to a PNG.
///
/// `FTS_BENCH_EXPRESSION=/tmp/expression.png`. The demo drum groove,
/// the way `e` opens it in the window with nothing selected;
/// `FTS_BENCH_EXPRESSION_MENU=1` with its context menu open.
fn expression_shot(palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    let size = (f64::from(width), f64::from(height));
    let mut view = session_daw::expression::Expression::demo((0.0, 0.0), size);
    // `FTS_BENCH_EXPRESSION_MENU=1`: with the context menu open, as a
    // right-click in the roll leaves it.
    if std::env::var_os("FTS_BENCH_EXPRESSION_MENU").is_some() {
        view.press(
            size.0 * 0.4,
            size.1 * 0.3,
            session_daw::mousemap::Mods::default(),
            2,
        );
        view.release(
            size.0 * 0.4,
            size.1 * 0.3,
            session_daw::mousemap::Mods::default(),
        );
    }
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, size.0, size.1),
            );
            view.paint(painter);
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!(
        "  wrote {} — {} hits on {} lanes",
        out.display(),
        view.editor.doc.notes.len(),
        match &view.editor.row_space {
            expression_editor_core::RowSpace::Drums(map) => map.lanes.len(),
            _ => 0,
        }
    );
}

/// Write one mixer frame to a PNG.
///
/// `FTS_BENCH_MIXER=/tmp/mixer.png`, with `FTS_BENCH_SCROLL=x,y` to move
/// along the strips.
fn editor_shot(palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let Some(project) = rt.block_on(daw_ui::studio::project::fetch()) else {
        tracing::error!("could not read the project back");
        return;
    };
    let mut song = session_daw::editor_view::Song::of(&project);
    let named = std::env::var("FTS_BENCH_EDITOR_TRACK").unwrap_or_else(|_| "Piano".to_owned());
    for t in &mut song.tracks {
        t.selected = t.name == named;
    }
    let mut view = session_daw::editor_view::EditorView::new(song);
    let deadline = Instant::now() + std::time::Duration::from_secs(60);
    while view.editor().is_none() && Instant::now() < deadline {
        let _ = view.frame(width, height, 1.0);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // `FTS_BENCH_EDITOR_ALIGN=1`: line the stack's takes up first, and
    // say where each lane's first notes moved.
    if std::env::var_os("FTS_BENCH_EDITOR_ALIGN").is_some() {
        let firsts = |view: &mut session_daw::editor_view::EditorView| -> Vec<Vec<f64>> {
            view.editor().map_or_else(Vec::new, |e| {
                (0..e.editor.tracks.len())
                    .map(|i| {
                        let doc = e.editor.tracks.doc_of(i).unwrap_or(&e.editor.doc);
                        // Past the first take's start: where the takes
                        // overlap, which is what aligning moves.
                        doc.notes
                            .iter()
                            .filter(|n| n.start >= 0.0)
                            .take(4)
                            .map(|n| n.start)
                            .collect()
                    })
                    .collect()
            })
        };
        let before = firsts(&mut view);
        view.align_takes();
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        while view.aligning() && Instant::now() < deadline {
            let _ = view.frame(width, height, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = view.frame(width, height, 1.0);
        println!(
            "  aligned: first notes per lane, before {before:.1?}\n           after  {:.1?}",
            firsts(&mut view)
        );
    }
    if let Some(e) = view.editor() {
        let starts: Vec<f64> = e.editor.doc.notes.iter().map(|n| n.start).collect();
        let max = starts.iter().copied().fold(0.0, f64::max);
        let (t0, t1) = e.editor.camera.time_span(e.editor.viewport);
        println!(
            "  notes {} start..{max:.0}; doc {:.0}..{:.0}; view {t0:.0}..{t1:.0}; tracks {}",
            starts.len(),
            e.editor.doc.start,
            e.editor.doc.end,
            e.editor.tracks.len()
        );
        for l in expression_editor_paint::stack::geometry::lanes(&e.editor, 1.8, 0.0) {
            println!(
                "  lane {} {:?} y {:.0} h {:.0} notes {} active {}",
                l.track,
                l.name,
                l.y,
                l.h,
                l.notes.len(),
                l.active
            );
        }
        println!("  stacked {} mode {:?}", e.editor.stacked, e.editor.mode);
    }
    let scene = view.frame(width, height, 1.0);
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            painter.append_scene(scene, Affine::IDENTITY);
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!("  wrote {} — the editor on {named}", out.display());
}

fn fx_shot(palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let Some(project) = rt.block_on(daw_ui::studio::project::fetch()) else {
        tracing::error!("could not read the project back");
        return;
    };
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&project.tracks);
    let mut rows: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    // `FTS_BENCH_FX_TRACK` selected (Kick unless named), or the first
    // track with a rack to show: the FX starts there.
    let mut store = session_daw::tone::Store::default();
    store.seed(&rows);
    let named = std::env::var("FTS_BENCH_FX_TRACK").unwrap_or_else(|_| "Kick".to_owned());
    let first = rows
        .iter()
        .position(|(t, _)| t.name == named)
        .or_else(|| rows.iter().position(|(t, _)| store.get(&t.guid).is_some()))
        .unwrap_or(0);
    for (i, (track, _)) in rows.iter_mut().enumerate() {
        track.selected = i == first;
    }
    let lanes = std::env::var("FTS_BENCH_FX_LANES")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(1);
    let folded: Vec<session::mix_phases::MixPhase> = std::env::var("FTS_BENCH_FX_FOLD")
        .unwrap_or_default()
        .split(',')
        .filter_map(|name| {
            session_daw::tone::RACK_PHASES
                .iter()
                .copied()
                .find(|p| format!("{p:?}").eq_ignore_ascii_case(name.trim()))
        })
        .collect();
    let Ok(font) = session_daw::text::Font::embedded() else {
        tracing::error!("no embedded font");
        return;
    };
    let returns_shut = std::env::var("FTS_BENCH_FX_FOLD")
        .unwrap_or_default()
        .split(',')
        .any(|name| name.trim().eq_ignore_ascii_case("returns"));
    let scene = session_daw::fx_view::shot(
        rows,
        store,
        lanes,
        (&folded, returns_shut),
        font,
        (width, height),
    );
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            painter.append_scene(scene, Affine::IDENTITY);
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
}

fn mixer_shot(
    palette: &Palette,
    font: &session_daw::text::Font,
    layout: session_daw::layout::Layout,
    out: &std::path::Path,
    project_file: &std::path::Path,
    width: u32,
    height: u32,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let Some(project) = rt.block_on(daw_ui::studio::project::fetch()) else {
        eprintln!("could not read the project back");
        return;
    };
    let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(project));
    // The mixer's strips: every track it shows, at its folder depth — not
    // the track panel's visibility (`FolderState::visible`), which a track
    // kept for mixing only (the drums' Process) is out of.
    let mut planned: Vec<(daw_proto::Track, u32)> = Vec::new();
    let mut depth = 0_u32;
    for track in project.tracks.iter() {
        if track.visible_in_mixer {
            planned.push((track.clone(), depth));
        }
        depth = depth.saturating_add_signed(track.folder_depth);
    }
    // A scene, if one is asked for: the visibility manager's answer to
    // which strips and how wide — `FTS_BENCH_SCENE=lead-vocal-fx`.
    //
    // Its selectors match the taxonomy the template wrote into the
    // project, so the ext-state is read back from the file the bench
    // was handed rather than guessed from the track names.
    let scene = std::env::var("FTS_BENCH_SCENE")
        .ok()
        .and_then(|slug| dynamic_template::scenes::scene(&slug));
    if let Some(scene) = scene {
        let kinds = session_daw::plan::Kinds::read(project_file);
        planned = session_daw::plan::apply_scene(
            &planned,
            &kinds,
            scene,
            session_daw::plan::Panel {
                surface: session_daw::plan::Surface::Mixer,
                mode: None,
                settings: session_daw::settings::Settings::default(),
                extent: f64::from(height),
                active_language: None,
            },
        );
    }
    // The row list the scene resolved to, as text — `FTS_BENCH_ROWS`.
    //
    // The picture is what a scene LOOKS like, and two GPUs disagree
    // about its last few bits of antialiasing. This is what the scene
    // MEANS: which rows survived the folds, how deep each sits and how
    // wide it opens, in project order. It is the same on every machine,
    // so it is the half of a scene fixture that can be compared byte for
    // byte (`apps/session-daw/tests/golden_scenes.rs`).
    if let Ok(out) = std::env::var("FTS_BENCH_ROWS") {
        let mut text = String::new();
        for (track, depth) in &planned {
            text.push_str(&format!(
                "{depth}\t{}\t{}\n",
                track.name,
                track.width.unwrap_or(0)
            ));
        }
        if let Err(e) = std::fs::write(&out, text) {
            tracing::error!(error = %e, path = out, "could not write the row list");
        }
    }
    // `FTS_BENCH_STATES=1`: every state a strip can be in, spread across
    // the strips — muted, soloed, armed or not, each monitoring mode,
    // panned, with and without a chain, sends, receives — so a shot
    // shows them all. Deterministic, so two shots compare.
    if std::env::var_os("FTS_BENCH_STATES").is_some() {
        for (i, (track, _)) in planned.iter_mut().enumerate() {
            let n = u32::try_from(i).unwrap_or(0);
            track.muted = n % 3 == 1;
            track.soloed = n % 4 == 2;
            track.armed = n % 2 == 0;
            track.input_monitor = match n % 3 {
                0 => daw_proto::InputMonitoringMode::Off,
                1 => daw_proto::InputMonitoringMode::Normal,
                _ => daw_proto::InputMonitoringMode::NotWhenPlaying,
            };
            track.pan = f64::from(n * 37 % 21) / 10.0 - 1.0;
            track.fx_count = n % 3;
            track.send_count = n % 2;
            track.receive_count = (n / 2) % 2;
            track.parent_send = n % 5 != 0;
            track.selected = n == 3;
            track.volume = 0.1 + f64::from(n % 9) * 0.2;
        }
    }
    let tracks: Vec<daw_proto::Track> = planned.iter().map(|(t, _)| t.clone()).collect();
    let rows = daw_ui::studio::RowsRef(std::sync::Arc::new(planned));
    // `FTS_BENCH_POINTER=row,control[,press]`: the pointer over (or
    // pressing) one control, by its theme name (`mute`, `fader`, `pan`…).
    let mut pointer = session_daw::pointer::Pointer::default();
    if let Ok(spec) = std::env::var("FTS_BENCH_POINTER") {
        let parts: Vec<&str> = spec.split(',').collect();
        if let (Some(row), Some(control)) = (
            parts.first().and_then(|r| r.trim().parse::<usize>().ok()),
            parts
                .get(1)
                .and_then(|c| session_daw::skin::control(c.trim())),
        ) {
            pointer.hover(Some(session_daw::pointer::Spot { row, control }));
            if parts.get(2).is_some_and(|p| p.trim() == "press") {
                pointer.press();
            }
        }
    }

    let scroll_x = std::env::var("FTS_BENCH_SCROLL")
        .ok()
        .and_then(|v| v.split(',').next()?.trim().parse().ok())
        .unwrap_or(0.0);

    // The Tone rack, which is a mix sub-mode rather than a permanent
    // fixture — so the shot asks for it explicitly. A scene is a
    // picture of the racks, so it asks for it too — and a picture of
    // racks with nothing moving through them is a picture of half of
    // what they are, so a scene runs the simulation: the meters lit,
    // the compressors reducing, the de-essers firing, the tails
    // filled. `FTS_BENCH_LIVE=1` asks for the same without a scene.
    let tone = std::env::var("FTS_BENCH_TONE").is_ok() || scene.is_some();
    let live = std::env::var("FTS_BENCH_LIVE").is_ok() || scene.is_some();
    // The whole frame, so a shot matches the window.
    //
    // The mixer splits internally — the REAPER strip takes a third off
    // the bottom, the rack fills the rest — so handing it the frame is
    // handing it the same thing the window hands it. Passing a short
    // height here is how to shoot the DOCKED look instead.
    let mcp_height = std::env::var("FTS_BENCH_MCP_HEIGHT")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|h| *h > 0.0)
        .unwrap_or_else(|| f64::from(height))
        .min(f64::from(height));
    // The rack's settings, seeded from the placeholder — see
    // `tone::Store`. The shot and the stress test both want the same
    // racks the window draws.
    let mut settings = session_daw::tone::Store::default();
    settings.seed(rows.as_slice());
    // The reverb tails, rendered before a frame is measured: a render
    // landing between two passes would make them differ, and the
    // verify run compares them byte for byte.
    settings.prerender_tails();
    let frame = session_daw::rails::Frame::new(f64::from(width), f64::from(height));
    let live_strips = std::env::var_os("FTS_BENCH_LIVE_STRIPS").is_some_and(|v| v != "0");
    let mixer = Mixer::build(
        palette,
        font,
        &project,
        &rows,
        mcp_height - session_daw::rails::TOP,
        layout,
        // The shot is of the Tone phase, which is the phase the rack
        // was built for and the one the reference images were taken in.
        if tone && !live_strips {
            session_daw::tone::panels_for(TONE)
        } else {
            &[]
        },
        live,
        // `FTS_BENCH_LIVE_STRIPS=1`: the Live-mode strips (the short
        // band) — see `strip::shape`.
        session_daw::settings::Settings {
            live_strips,
            // `FTS_BENCH_MODE=mix|record`: the strips laid out for that
            // mode, where a theme asks.
            mixing: std::env::var("FTS_BENCH_MODE").as_deref() == Ok("mix"),
            recording: std::env::var("FTS_BENCH_MODE").as_deref() == Ok("record"),
            ..session_daw::settings::Settings::default()
        },
        &settings,
    );
    // The signal, when the shot is live: seventy-two meter frames of
    // history per track ending at one instant, the way the window has
    // them after two and a half seconds of playback.
    let mut history: std::collections::HashMap<String, session_daw::tone::Levels> =
        std::collections::HashMap::new();
    let mut spectra: std::collections::HashMap<String, session_daw::tone::Analyser> =
        std::collections::HashMap::new();
    let mut levels: Vec<daw_proto::TrackLevels> = Vec::new();
    if live {
        const AT: f64 = 2.5;
        for (i, track) in tracks.iter().enumerate() {
            let Some(tone) = settings.get(&track.guid) else {
                continue;
            };
            let entry = history.entry(track.guid.clone()).or_default();
            let mut last = None;
            for k in 0..session_daw::tone::HISTORY {
                let t = AT
                    - (session_daw::tone::HISTORY - 1 - k) as f64
                        / f64::from(session_daw::tone::PUBLISH_HZ);
                let meters = session_daw::simulate::meters(i, t, tone);
                entry.push(meters.sat_peak);
                entry.push_fire(meters.deess_deepest());
                entry.push_ess(meters.ess_db, meters.ess_ref_db);
                last = Some(meters);
            }
            if let Some(meters) = last {
                let peak = meters.sat_peak;
                if let Ok(index) = usize::try_from(track.index) {
                    if levels.len() <= index {
                        levels.resize(index + 1, daw_proto::TrackLevels::default());
                    }
                    levels[index] = daw_proto::TrackLevels {
                        peak_left: peak,
                        peak_right: peak * 0.85,
                        hold_left: peak,
                        hold_right: peak,
                    };
                }
                spectra.entry(track.guid.clone()).or_default().set(meters);
            }
        }
    }
    let mut racks = if live {
        session_daw::overlay::Racks {
            settings: &settings,
            history: &mut history,
            spectra: &mut spectra,
            lit: None,
            panels: session_daw::tone::panels_for(TONE),
            folded: Box::leak(Box::new(session_daw::tone::Fold::rest())),
        }
    } else {
        session_daw::overlay::Racks::none()
    };
    // The bench applies no preset, so the map is the identity — built
    // rather than skipped so the shot exercises the same lookup the
    // window does.
    let map = session_daw::plan::Rows::of(rows.as_slice(), &tracks);

    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    let mut counts = Counts::default();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            // Pinned to the BOTTOM of the window.
            //

            let at =
                Affine::translate((session_daw::rails::SIDE - scroll_x, session_daw::rails::TOP));
            let recorded = mixer.replay(painter, scroll_x, frame.content_width(), at);
            // The live controls, exactly as the window draws them — a
            // shot that skipped them would be a shot of a mixer with no
            // faders, and the whole point of the shot is to be the same
            // frame the window shows.
            let live = session_daw::overlay::controls(
                painter,
                palette,
                font,
                &mixer,
                &tracks,
                &map,
                &pointer,
                // At rest unless live: the still shot is the reference
                // every viewport in the sweep is compared against, and
                // a lit meter in it would be a difference nobody asked
                // for. A scene is live.
                &levels,
                // Nothing has clipped in a still frame, and a latch in
                // the reference image would be a difference nobody
                // asked for.
                &session_daw::overlay::Clips::default(),
                // At the top: a shot is the reference every viewport is
                // compared against, and a scrolled rack in it would be
                // a difference nobody asked for.
                0.0,
                &mut racks,
                scroll_x,
                frame.content_width(),
                at,
            );
            counts.replayed = recorded.replayed + live.replayed;
            counts.submitted = recorded.submitted + live.submitted;
            let profile = session_daw::rails::profile(
                session_daw::rails::Surface::Mixer,
                session::modes::Mode::Mix,
                session::mix_phases::MixPhase::Tone,
                scene.map(|s| s.slug.as_str()),
                session_daw::settings::Settings::default(),
                dynamic_template::scenes::Audience::Engineer,
                // The bench draws the kit's rails.
                "drums",
            );
            session_daw::rails::draw(
                painter,
                palette,
                font,
                // Deliberately none: the shot is the reference the
                // sweep compares against, and it must not depend on
                // whether this machine has REAPER's resources on disk.
                &mut session_daw::icons::Icons::none(),
                // At rest, like every other control in the shot.
                (None, None),
                frame,
                &profile.left,
                &profile.right,
                &profile.top,
            );
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!(
        // The content width is reported because opening a strip must
        // not change it: an opened strip borrows from the others. Two
        // shots of the same project, one with a selection and one
        // without, must print the same number.
        "  wrote {} — {} strips, {} deep, {:.0}px wide, {} commands submitted",
        out.display(),
        mixer.count,
        mixer.depth,
        mixer.content_width(),
        counts.submitted,
    );
}

/// Prove the culling draws the same frame the full replay does.
///
/// Every phase is sampled across its whole sweep and each viewport is
/// rendered twice — culled, then complete — and the two buffers compared
/// byte for byte. Any difference is a dropped or mis-clipped command,
/// reported with the viewport that produced it so it can be reproduced.
///
/// This is the control for the numbers the bench prints. Without it the
/// headline is "we made it five times faster by drawing less", which is
/// only interesting if the missing part was never on screen.
fn verify(
    renderer: &mut VelloImageRenderer,
    scene: &Arrangement,
    phases: &[(&str, Gesture)],
    span_x: f64,
    span_y: f64,
    width: u32,
    height: u32,
) {
    /// Sampled per phase. Enough to land on the ends of every sweep and
    /// the awkward fractions between them.
    const SAMPLES: usize = 120;

    let mut culled = Vec::new();
    let mut complete = Vec::new();
    let mut checked = 0usize;
    let mut bad = 0usize;
    // Viewports that matched only within the tolerance. Counted and
    // reported rather than ignored: if this climbs, the tolerance is
    // hiding something and wants looking at.
    let mut rounded = 0usize;

    println!("\n  verifying culled == complete over {SAMPLES} viewports per phase\n");
    for (name, gesture) in phases.iter() {
        let mut worst_kept = 100.0f64;
        let mut empty = 0usize;
        for sample in 0..SAMPLES {
            let t = sample as f64 / SAMPLES as f64;
            let (fx, fy, zx, zy) = gesture(t);
            let (scroll_x, scroll_y) = (span_x * fx, span_y * fy);
            let view = Viewport {
                scroll_x,
                scroll_y,
                pps: PPS * zx,
                zoom_y: zy,
                width: f64::from(width),
                height: f64::from(height),
                panel_w: TCP_WIDTH,
            };
            let lanes = Affine::translate((TCP_WIDTH - scroll_x, -scroll_y))
                * Affine::scale_non_uniform(PPS * zx, zy);
            let panel = Affine::translate((0.0, -scroll_y));

            let mut counts = Counts::default();
            renderer.render_to_vec(
                |painter| {
                    painter.reset();
                    let a = scene.replay_lanes(painter, view, lanes);
                    let b = scene.replay_panel(painter, view, panel);
                    counts.replayed = a.replayed + b.replayed;
                    counts.submitted = a.submitted + b.submitted;
                },
                &mut culled,
            );
            renderer.render_to_vec(
                |painter| {
                    painter.reset();
                    session_daw::arrangement::replay_all(painter, &scene.lanes, lanes);
                    scene.replay_all_panel(painter, view, panel);
                },
                &mut complete,
            );

            checked += 1;
            // Compared with a one-LSB tolerance, not bit-for-bit.
            //
            // Vello accumulates coverage on the GPU, and the same curve
            // can land a single channel 1/255 apart depending on how
            // many commands preceded it — which culling changes by
            // design. Twelve such pixels showed up here, all of them on
            // the antialiased edge of a knob.
            //
            // The tolerance is deliberately tiny, because the thing this
            // guards against is not subtle: a control that got culled
            // while visible differs over its whole area, by the full
            // distance between it and the background. One LSB cannot
            // hide that, and demanding exactness instead would mean
            // reporting a renderer's rounding as a correctness failure
            // every run.
            const TOLERANCE: i16 = 1;

            let mut differing = 0_usize;
            let mut worst_delta = 0_i16;
            let mut at = (0_usize, 0_usize, 0_usize);
            for (i, (a, b)) in culled.iter().zip(&complete).enumerate() {
                let delta = i16::from(*a) - i16::from(*b);
                if delta == 0 {
                    continue;
                }
                differing += 1;
                if delta.abs() > worst_delta.abs() {
                    worst_delta = delta;
                    let pixel = i / 4;
                    at = (pixel % width as usize, pixel / width as usize, i % 4);
                }
            }
            if differing > 0 {
                rounded += 1;
            }
            if worst_delta.abs() > TOLERANCE {
                bad += 1;
                bad += 1;
                println!(
                    "  MISMATCH {name}: scroll ({scroll_x:.0}, {scroll_y:.0}) zoom ({zx:.2}, \
                     {zy:.2}) — {differing} bytes, worst {worst_delta:+} at ({}, {}) channel {}",
                    at.0, at.1, at.2,
                );
            }
            if let Some(kept) = counts.kept_pct() {
                worst_kept = worst_kept.min(kept);
            } else {
                empty += 1;
            }
        }
        let note = if empty > 0 {
            format!(", {empty} showed nothing at all")
        } else {
            String::new()
        };
        println!(
            "  {name:<20} {SAMPLES} viewports identical, kept as low as {worst_kept:.0}%{note}"
        );
    }

    if bad == 0 {
        println!(
            "\n  {checked} viewports: culled output matches complete \
             ({rounded} differed only by a rounding LSB)\n"
        );
    } else {
        println!(
            "\n  {bad} of {checked} viewports DIFFER — the culling is dropping visible work\n"
        );
        std::process::exit(1);
    }
}

/// Write one frame to a PNG.
///
/// `FTS_BENCH_SHOT=/tmp/frame.png`, with `FTS_BENCH_SCROLL=x,y` and
/// `FTS_BENCH_ZOOM=x,y` to move it. The view defaults to the window's
/// opening one, and the frame is the widget's — the arrangement exactly
/// as the window paints it, without the DOM chrome (toolbar, scrollbars)
/// the window lays over it. `bin/blitz_shot` is the picture WITH that
/// chrome.
fn shot(
    session: &Session,
    scene: &Arrangement,
    palette: &Palette,
    out: &std::path::Path,
    width: u32,
    height: u32,
) {
    let pair = |name: &str, default: (f64, f64)| {
        std::env::var(name)
            .ok()
            .and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or(default)
    };
    let (scroll_x, scroll_y) = pair("FTS_BENCH_SCROLL", (0.0, 0.0));
    let (zoom_x, zoom_y) = pair("FTS_BENCH_ZOOM", (1.0, 1.0));
    // `FTS_BENCH_SECTION=<n>` frames the review's window on that
    // section instead — the arrangement scrolled and zoomed to one
    // section with its run-up and tail, which is what a tablet shows
    // while a take is being judged. Same renderer, one viewport apart.
    let at = match std::env::var("FTS_BENCH_SECTION")
        .ok()
        .and_then(|n| n.trim().parse::<usize>().ok())
        .and_then(|n| scene.sections().get(n).cloned())
    {
        Some(section) => {
            let framed = session_daw::take_window::viewport(
                (section.start, section.end),
                scene.tempo(),
                f64::from(width),
                f64::from(height),
                zoom_y,
            );
            View {
                scroll_x: framed.scroll_x,
                scroll_y: framed.scroll_y,
                zoom_x: framed.pps / PPS,
                zoom_y: framed.zoom_y,
                play_at: 0.0,
            }
        }
        None => View {
            scroll_x,
            scroll_y,
            zoom_x,
            zoom_y,
            play_at: 0.0,
        },
    };
    let (mut widget, view) = session.widget();
    *view.borrow_mut() = at;
    // The buffer starts zeroed, so a pixel nothing covers reads as
    // transparent; the window's ground is the theme's surface, and the
    // shot puts that under the widget the way the panel's `div` does.
    // `FTS_BENCH_SHOT_GAPS=1` paints it a colour the theme never uses
    // instead, so an uncovered pixel cannot hide.
    let ground = if std::env::var_os("FTS_BENCH_SHOT_GAPS").is_some() {
        vello::peniko::color::palette::css::MAGENTA
    } else {
        palette.surface
    };
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                ground,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            painter.append_scene(widget.paint_scene(width, height, 1.0), Affine::IDENTITY);
        },
        &mut buffer,
    );

    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!(
        "  wrote {} — scroll ({:.0}, {:.0}) zoom ({:.2}, {:.2}), {} commands submitted",
        out.display(),
        at.scroll_x,
        at.scroll_y,
        at.zoom_x,
        at.zoom_y,
        drawn(&widget).submitted,
    );
}

/// `FTS_BENCH_PLAN=<out>`: the rows the window plans for the open
/// project, as text — the arrangement's and the mixer's, each laid out
/// by `FTS_BENCH_SCENE` (the window's opening scene, `drum-mixing`,
/// unless named). One `surface depth folder name` line a row: what the
/// window shows, without drawing it.
fn plan_dump(path: &std::path::Path, out: &std::path::Path) {
    let Some(raw) = session_daw::studio::fetch() else {
        tracing::error!("could not read the project back");
        return;
    };
    let scene = std::env::var("FTS_BENCH_SCENE")
        .ok()
        .and_then(|slug| dynamic_template::scenes::scene(&slug))
        .or_else(|| dynamic_template::scenes::scene("drum-mixing"))
        .map(|scene| scene.slug.as_str());
    let planner = session_daw::studio::Planner {
        raw: std::sync::Arc::new(raw),
        scene,
        kinds: std::sync::Arc::new(session_daw::plan::Kinds::read(path)),
    };
    let (_, arrange) = planner.plan(&planner.raw);
    let mixer = planner.plan_mixer(
        &planner.raw,
        None,
        session_daw::settings::Settings::default(),
        1440.0,
    );
    let mut text = String::new();
    for (surface, rows) in [("arrange", &arrange), ("mixer", &mixer)] {
        for (track, depth) in rows.as_slice() {
            text.push_str(&format!(
                "{surface}\t{depth}\t{}\t{}\n",
                u8::from(track.is_folder),
                track.name
            ));
        }
    }
    if let Err(e) = std::fs::write(out, text) {
        tracing::error!(error = %e, path = %out.display(), "could not write the plan");
    }
}

/// Print the first rows' folder depths, to check nesting against the
/// project rather than against the picture.
fn depths() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let Some(project) = rt.block_on(daw_ui::studio::project::fetch()) else {
        return;
    };
    let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(project));
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&project.tracks);
    for (track, depth) in visible.iter().zip(&depths).take(40) {
        println!(
            "  {:<10} depth {depth}  folder_depth {:>2}  is_folder {}",
            track.name, track.folder_depth, track.is_folder
        );
    }
}

/// The open session, read the way the window reads it.
///
/// The rows are planned by the studio's own planner
/// (`session_daw::studio::plan_rows` — `FTS_BENCH_SCENE` picks a visual
/// scene, none by default so every track is on screen), the previews are
/// the studio's (`previews_of`: every MIDI item's notes and every audio
/// item's peaks), and the arrangement is drawn through the widget the
/// window mounts. There is no second painter here to drift from it.
struct Session {
    project: daw_ui::studio::ProjectRef,
    rows: daw_ui::studio::RowsRef,
    previews: session_daw::midi::Previews,
}

impl Session {
    fn read(path: &std::path::Path) -> Option<Self> {
        let raw = session_daw::studio::fetch()?;
        // `FTS_BENCH_OPEN=Kick,Snare`: those folders opened, as a click on
        // their fold buttons would.
        if let Ok(names) = std::env::var("FTS_BENCH_OPEN") {
            for name in names.split(',').map(str::trim) {
                for track in raw.tracks.iter().filter(|t| t.is_folder && t.name == name) {
                    session_daw::folds::toggle(&track.guid, false);
                }
            }
        }
        let scene = std::env::var("FTS_BENCH_SCENE").ok();
        let (project, rows) = session_daw::studio::plan_rows(&raw, scene.as_deref(), path);
        // Read before anything is recorded, not after: a renderer draws
        // and exits, so there is no later for them to arrive in.
        let previews = session_daw::studio::previews_of(&project);
        Some(Self {
            project,
            rows,
            previews,
        })
    }

    /// The recording on its own, for the culling check and the counts —
    /// the same one the widget builds.
    fn scene(
        &self,
        palette: &Palette,
        font: &session_daw::text::Font,
        layout: session_daw::layout::Layout,
    ) -> Arrangement {
        Arrangement::build(
            palette,
            font,
            &self.project,
            &self.rows,
            layout,
            &self.previews,
            session_daw::tcp::Tcp::FULL,
        )
    }

    /// The window's arrangement, and the view it reads each paint.
    fn widget(&self) -> (ArrangementWidget, Shared) {
        let view: Shared = std::rc::Rc::new(std::cell::RefCell::new(View::OPENING));
        let widget = ArrangementWidget::for_session(
            &self.project,
            &self.rows,
            &self.previews,
            false,
            std::rc::Rc::clone(&view),
            // No frame-time graph: it is an overlay, and a benchmark
            // that drew one would be measuring it.
            false,
        );
        (widget, view)
    }
}

/// What the widget's last paint walked and submitted.
fn drawn(widget: &ArrangementWidget) -> Counts {
    let drawn = *widget.drawn().borrow();
    Counts {
        replayed: drawn.replayed,
        submitted: drawn.submitted,
    }
}

/// The mixer window as the stress tests drive it: the recorded chrome,
/// every control changing every frame, the racks lit by a simulated
/// signal.
///
/// One rig serves both the mixer-only stress test and the studio
/// benchmark, so the two measure the same mixer.
struct MixerRig {
    mixer: Mixer,
    tracks: Vec<daw_proto::Track>,
    map: session_daw::plan::Rows,
    settings: session_daw::tone::Store,
    history: std::collections::HashMap<String, session_daw::tone::Levels>,
    spectra: std::collections::HashMap<String, session_daw::tone::Analyser>,
    pointer: session_daw::pointer::Pointer,
    frame: session_daw::rails::Frame,
    levels: Vec<daw_proto::TrackLevels>,
    width: f64,
    height: f64,
}

impl MixerRig {
    fn new(
        palette: &Palette,
        font: &session_daw::text::Font,
        layout: session_daw::layout::Layout,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        let project = rt.block_on(daw_ui::studio::project::fetch())?;
        let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(project));
        let (visible, depths) =
            daw_ui::components::folders::FolderState::default().visible(&project.tracks);
        let tracks: Vec<daw_proto::Track> = visible.clone();
        let rows = daw_ui::studio::RowsRef(std::sync::Arc::new(
            visible.into_iter().zip(depths).collect(),
        ));

        // The rack's settings, seeded from the placeholder — see
        // `tone::Store`. The shot and the stress test both want the
        // same racks the window draws.
        let mut settings = session_daw::tone::Store::default();
        settings.seed(rows.as_slice());
        // The reverb tails, rendered before a frame is measured: a
        // render landing between two passes would make them differ,
        // and the verify run compares them byte for byte.
        settings.prerender_tails();
        let frame = session_daw::rails::Frame::new(f64::from(width), f64::from(height));
        let mixer = Mixer::build(
            palette,
            font,
            &project,
            &rows,
            f64::from(height) - session_daw::rails::TOP,
            layout,
            session_daw::tone::panels_for(TONE),
            // The racks are driven here, so they are drawn live and
            // the recording reserves their space without filling it —
            // which is what the window does the moment anything feeds
            // a spectrum.
            true,
            session_daw::settings::Settings::default(),
            &settings,
        );
        let map = session_daw::plan::Rows::of(rows.as_slice(), &tracks);
        Some(Self {
            mixer,
            tracks,
            map,
            settings,
            history: std::collections::HashMap::new(),
            spectra: std::collections::HashMap::new(),
            pointer: session_daw::pointer::Pointer::default(),
            frame,
            levels: Vec::new(),
            width: f64::from(width),
            height: f64::from(height),
        })
    }

    /// The session's state for this instant — every parameter, the
    /// meters, and (at the engine's own rate) the racks' displays.
    fn drive(&mut self, t: f64, frame_index: usize) {
        // The same on every run — see `animate::drive`.
        session_daw::animate::drive(&mut self.tracks, t);
        // Meters arrive on their own subscription, so they are driven
        // separately — and per frame, which is faster than the
        // engine's pump will ever publish them.
        self.levels = session_daw::animate::meters(self.tracks.len(), t);
        // And the compressor's display, at the rate the ENGINE
        // publishes meter frames — about 30 Hz — rather than at the
        // frame rate. Everything else here is driven per frame on
        // purpose, because a stress test should measure a case that
        // cannot happen. This one would measure a case that cannot
        // happen in the other direction: levels arriving faster than
        // they are drawn, which would defeat the trace's cache and
        // report a cost no session can produce.
        if frame_index % 8 == 0 {
            for (i, track) in self.tracks.iter().enumerate() {
                let Some(tone) = self.settings.get(&track.guid) else {
                    continue;
                };
                let meters = session_daw::simulate::meters(i, t * 8.0, tone);
                let entry = self.history.entry(track.guid.clone()).or_default();
                entry.push(meters.sat_peak);
                entry.push_fire(meters.deess_deepest());
                entry.push_ess(meters.ess_db, meters.ess_ref_db);
                self.spectra
                    .entry(track.guid.clone())
                    .or_default()
                    .set(meters);
            }
        }
    }

    /// One frame of the mixer window.
    fn draw(
        &mut self,
        painter: &mut impl PaintScene,
        palette: &Palette,
        font: &session_daw::text::Font,
    ) -> Counts {
        painter.reset();
        painter.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            palette.surface,
            None,
            &vello::kurbo::Rect::new(0.0, 0.0, self.width, self.height),
        );
        let at = Affine::translate((session_daw::rails::SIDE, session_daw::rails::TOP));
        // The recorded chrome, then the live values over it.
        let a = self
            .mixer
            .replay(painter, 0.0, self.frame.content_width(), at);
        let b = session_daw::overlay::controls(
            painter,
            palette,
            font,
            &self.mixer,
            &self.tracks,
            &self.map,
            &self.pointer,
            &self.levels,
            &session_daw::overlay::Clips::default(),
            0.0,
            &mut session_daw::overlay::Racks {
                folded: &session_daw::tone::Fold::default(),
                settings: &self.settings,
                history: &mut self.history,
                spectra: &mut self.spectra,
                lit: None,
                panels: session_daw::tone::panels_for(TONE),
            },
            0.0,
            self.frame.content_width(),
            at,
        );
        Counts {
            replayed: a.replayed + b.replayed,
            submitted: a.submitted + b.submitted,
            ..Counts::default()
        }
    }
}

/// Every parameter on every track, moving, measured.
///
/// `FTS_BENCH_ANIMATE=1`. The mixer's controls are drawn live so a mute
/// can change without the mixer being re-recorded; this is the frame
/// that says whether that is actually cheap.
///
/// Nothing scrolls. The question is not "can it draw a moving mixer",
/// it is "what does a still mixer cost when every control in it is
/// changing" — and mixing a scroll into that would hide the answer
/// under the cost of culling.
fn animate(
    palette: &Palette,
    font: &session_daw::text::Font,
    layout: session_daw::layout::Layout,
    width: u32,
    height: u32,
) {
    let Some(mut rig) = MixerRig::new(palette, font, layout, width, height) else {
        eprintln!("could not read the project back");
        return;
    };
    let mut renderer = Headless::new(width, height).expect("a headless renderer");
    let mut stages = Stages::with_capacity(FRAMES);
    let mut counts = Counts::default();
    for batch in 0..FRAMES / BATCH {
        let batch_start = Instant::now();
        let mut painted = 0.0;
        for step in 0..BATCH {
            let frame_index = batch * BATCH + step;
            let t = frame_index as f64 / FRAMES as f64;
            rig.drive(t, frame_index);
            let mut drawn = Counts::default();
            painted += renderer
                .frame(|painter| {
                    drawn = rig.draw(painter, palette, font);
                })
                .expect("render a frame");
            counts = drawn;
        }
        renderer.wait().expect("the gpu to finish the batch");
        let per_frame = batch_start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64;
        stages.frame.push_ms(per_frame);
        stages.paint.push_ms(painted / BATCH as f64);
    }
    // The first batch warms geometry no earlier frame touched — the
    // cost of starting, not of running.
    stages.frame.drop_warmup(1);
    stages.paint.drop_warmup(1);

    let frame = stages.frame.summary().expect("batches");
    let paint = stages.paint.summary().expect("batches");
    println!(
        "  {:<20} {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>9.0}   {:>7.2} {:>7.2}",
        "every parameter",
        frame.mean,
        frame.p99,
        frame.worst,
        frame.fps(),
        paint.mean,
        (frame.mean - paint.mean).max(0.0),
    );
    println!(
        "\n  {} strips, {} commands submitted a frame — every mute, solo, arm,",
        rig.mixer.count, counts.submitted
    );
    println!("  fader, pan and meter on every visible strip changing on every frame.");
}

/// The dock under the arrangement: its box in the frame, and the
/// arrangement's height above it. Forty percent of the frame, never less
/// than a usable editor.
fn dock_split(width: u32, height: u32) -> (vello::kurbo::Rect, u32) {
    let (w, h) = (f64::from(width), f64::from(height));
    let dock = (h * 0.4).max(160.0).min(h - 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a height inside a frame of u32 pixels"
    )]
    let above = (h - dock).floor() as u32;
    (vello::kurbo::Rect::new(0.0, f64::from(above), w, h), above)
}

/// The docked editor, in its box: the ground, the editor, and the rule
/// along its top edge that is also the grip that resizes it.
fn paint_dock(
    painter: &mut impl PaintScene,
    palette: &Palette,
    editor: &mut session_daw::expression::Expression,
    dock: vello::kurbo::Rect,
) {
    editor.layout((dock.x0, dock.y0), (dock.width(), dock.height()));
    painter.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        palette.surface,
        None,
        &dock,
    );
    editor.paint(painter);
    painter.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_rule,
        None,
        &vello::kurbo::Rect::new(dock.x0, dock.y0 - 1.0, dock.x1, dock.y0 + 1.0),
    );
}

/// One frame of the arrangement with the editor docked, to a PNG.
///
/// `FTS_BENCH_DOCK=/tmp/dock.png`. The arrangement is the widget the
/// window mounts, at rest; the editor is the demo groove in the dock
/// under it.
fn dock_shot(session: &Session, palette: &Palette, out: &std::path::Path, width: u32, height: u32) {
    let (dock_box, above) = dock_split(width, height);
    let mut editor = session_daw::expression::Expression::demo(
        (dock_box.x0, dock_box.y0),
        (dock_box.width(), dock_box.height()),
    );
    editor.set_look(session_daw::expression::look_of(palette));
    editor.editor.playhead = Some(editor.editor.doc.end * 0.3);
    let (mut widget, _view) = session.widget();
    let mut image = VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            painter.append_scene(widget.paint_scene(width, above, 1.0), Affine::IDENTITY);
            paint_dock(painter, palette, &mut editor, dock_box);
        },
        &mut buffer,
    );
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!("  wrote {}", out.display());
}

/// A pointer event at a point in the widget's own coordinates, with the
/// main button and these modifiers — what Blitz hands the widget.
fn pointer_at(
    x: f64,
    y: f64,
    mods: blitz_traits::events::Modifiers,
) -> blitz_traits::events::BlitzPointerEvent {
    use blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, Point,
        PointerCoords, PointerDetails,
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a coordinate inside a benchmark frame"
    )]
    let (x, y) = (x as f32, y as f32);
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::Primary,
        mods,
        details: PointerDetails::default(),
        element: Point { x, y },
        active_pointers: std::sync::Arc::default(),
    }
}

/// Where a slip drag takes hold of the first item: a view with the item
/// on screen, and the point on its lower half a press lands on — in the
/// widget's own coordinates, which are the ones its events arrive in.
fn slip_grip(scene: &Arrangement, span: (f64, f64)) -> Option<(View, (f64, f64))> {
    let item = scene.item_boxes().first()?;
    let (top, band) = scene.row_band(
        item.row,
        Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            pps: PPS,
            zoom_y: 1.0,
            width: 0.0,
            height: 0.0,
            panel_w: TCP_WIDTH,
        },
    )?;
    let at = View {
        scroll_x: (item.x0 * PPS - 200.0).clamp(0.0, span.0),
        scroll_y: (top - 100.0).clamp(0.0, span.1),
        zoom_x: 1.0,
        zoom_y: 1.0,
        play_at: 0.0,
    };
    let x = ((item.x0 + item.x1) / 2.0).mul_add(PPS, TCP_WIDTH - at.scroll_x);
    let y = band.mul_add(0.75, ruler_h() + top - at.scroll_y);
    Some((at, (x, y)))
}

/// The studio: the arrangement with the expression editor docked under
/// it on one display, the mixer on a second, every frame drawing both.
///
/// `FTS_BENCH_STUDIO=1`. The frame is `FTS_BENCH_SIZE` (5120x1440 by
/// default) and the mixer `FTS_BENCH_MIXER_SIZE` (2560x1440). The
/// target is 240 frames a second for the pair — a budget of 4.17 ms
/// for both windows together, since one GPU draws them in turn — and
/// the verdict is printed against it.
///
/// The arrangement is drawn through `ArrangementWidget`, the painter the
/// window mounts, so the verdict is about the window's picture and not a
/// copy of it. What moves: the arrangement scrolls and zooms as the
/// single-window phases do; the editor's playhead runs and its camera
/// pans, which is the frame every drum edit is made on; the mixer has
/// every control changing and every rack lit. Nothing is cached across
/// frames that the window does not cache.
///
/// Not drawn: the chrome the window builds as DOM over the widget (the
/// toolbar, the scrollbars) and the rails around it — `bin/blitz_shot`'s
/// `FTS_BLITZ_FRAMES` measures the frame with the DOM in it.
// r[verify flow.verify.frame-rate]
fn studio(
    session: &Session,
    scene: &Arrangement,
    palette: &Palette,
    font: &session_daw::text::Font,
    layout: session_daw::layout::Layout,
    width: u32,
    height: u32,
) {
    const BUDGET_MS: f64 = 1000.0 / 240.0;
    let (mixer_w, mixer_h) = std::env::var("FTS_BENCH_MIXER_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((2560, 1440));

    let Some(mut mixer) = MixerRig::new(palette, font, layout, mixer_w, mixer_h) else {
        eprintln!("could not read the project back");
        return;
    };
    let mut arrange =
        Headless::new(width, height).expect("a headless renderer for the arrangement");
    let mut mixer_gpu = Headless::new(mixer_w, mixer_h).expect("a headless renderer for the mixer");

    // The dock: forty percent of the window, the editor over the demo
    // groove inside it.
    let (dock_box, above) = dock_split(width, height);
    let dock = dock_box.height();
    // The audio drum workflow in the dock: the kit's mics as role
    // lanes, a song's worth of hits, framed four bars at a time the way
    // drums get edited — and paged through as the frames go by.
    let bars = bench_bars();
    let mut editor = session_daw::expression::Expression::audio_kit(
        bars,
        (dock_box.x0, dock_box.y0),
        (dock_box.width(), dock_box.height()),
    );
    editor.set_look(session_daw::expression::look_of(palette));
    editor.editor.frame_bars(4);
    let doc_end = editor.editor.doc.end;
    let (mut widget, view) = session.widget();
    // The lanes' share of the arrangement: less the panel across, less
    // the ruler down.
    let lanes = (
        (f64::from(width) - TCP_WIDTH).max(1.0),
        (f64::from(above) - ruler_h()).max(1.0),
    );
    let span_y = (scene.content_height() - lanes.1).max(1.0);
    let span_x = (scene.length_secs * PPS - lanes.0).max(1.0);
    let fit = lanes.1 / scene.content_height().max(1.0);

    fn tri(t: f64) -> f64 {
        let t = (t * 6.0) % 1.0;
        if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
    }
    fn slow(t: f64) -> f64 {
        let t = (t * 2.0) % 1.0;
        if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
    }
    let phases: Vec<(&str, Gesture)> = vec![
        (
            "scroll both",
            Box::new(|t| (tri(t), tri((t * 1.7) % 1.0), 1.0, 1.0)),
        ),
        (
            "zoom both",
            Box::new(|t| (0.0, 0.3, 0.25 + slow(t) * 7.75, 0.25 + slow(t) * 3.75)),
        ),
        (
            "fit whole session",
            Box::new(move |t| (0.0, 0.0, 1.0, fit + slow(t) * (0.25 - fit))),
        ),
        // The zoom tool: `z` held, a press in the lanes, and the
        // pointer drawn sideways and up in a slow figure. The
        // arrangement zooms about the press by the panel's own factor
        // (`panel::factor`, `panel::zoom_about`) and the docked kit
        // through its own zoom tool, at once. The closure only places
        // the start; the drag decides the view.
        ("zoom tool drag", Box::new(|_| (0.2, 0.3, 1.0, 1.0))),
        // A slip drag: the view is still and one item's waveform is
        // redrawn live at a moving offset. Measured because #130 says
        // to measure it rather than assume one more item's worth of
        // path a frame is free. Driven through the widget's own events
        // — a ctrl+alt press on the item and the pointer moving — so it
        // is the window's slip, not a flag set from outside.
        ("slip drag", Box::new(|_| (0.0, 0.3, 1.0, 1.0))),
    ];
    // Where the zoom tool is pressed, from the lanes' own origin.
    let press = (600.0, 300.0);
    let slip = slip_grip(scene, (span_x, span_y));
    if slip.is_none() {
        tracing::warn!("the session has no item to slip; the slip phase measures a still view");
    }

    println!();
    println!(
        "  studio        arrangement {width}x{above} with the editor docked ({dock:.0}px) under it,"
    );
    println!("                mixer {mixer_w}x{mixer_h} on a second display, both every frame");
    println!(
        "  scene         {} rows, {} items; {} strips; a {bars}-bar kit in the dock",
        scene.rows,
        scene.items(),
        mixer.mixer.count
    );
    println!("  drawn         the arrangement through ArrangementWidget, the window's painter");
    println!("  frames        {FRAMES} per phase, batches of {BATCH}, waited on once per batch");
    println!("  target        240 Hz — {BUDGET_MS:.2} ms for both windows together\n");
    println!(
        "  {:<20} {:>9} {:>9} {:>9} {:>9}   {:>7} {:>7}",
        "phase", "mean", "p99", "worst", "fps(p99)", "paint", "gpu"
    );
    println!("  {}", "-".repeat(78));

    let mut worst_p99 = 0.0_f64;
    let mut worst_name = "";
    for (name, gesture) in &phases {
        let mut stages = Stages::with_capacity(FRAMES);
        for batch in 0..FRAMES / BATCH {
            let batch_start = Instant::now();
            let mut painted = 0.0;
            for step in 0..BATCH {
                let frame_index = batch * BATCH + step;
                let t = frame_index as f64 / FRAMES as f64;
                let (fx, fy, zx, zy) = gesture(t);
                let mut at = View {
                    scroll_x: span_x * fx,
                    scroll_y: span_y * fy,
                    zoom_x: zx,
                    zoom_y: zy,
                    play_at: t * scene.length_secs,
                };
                editor.editor.playhead = Some(t * doc_end);
                if *name == "zoom tool drag" {
                    // The pointer's path: out to the right and up over
                    // the phase, back, and again — a slow figure, so the
                    // zoom sweeps its range rather than jumping.
                    let travel = (tri(t / 3.0) - 0.5) * 2.0;
                    let moved = (travel * 300.0, -travel * 150.0);
                    // Right zooms time in and up zooms the rows out: the
                    // panel's drag, about where the press landed.
                    let to = (
                        at.zoom_x * session_daw::panel::factor(moved.0),
                        at.zoom_y * session_daw::panel::factor(moved.1),
                    );
                    // No further than the session reaches at the new
                    // zoom, and never before its start.
                    let reach =
                        |span: f64, lane: f64, zoom: f64| ((span + lane) * zoom - lane).max(0.0);
                    at.scroll_x =
                        session_daw::panel::zoom_about(press.0, at.scroll_x, at.zoom_x, to.0)
                            .min(reach(span_x, lanes.0, to.0))
                            .max(0.0);
                    at.scroll_y =
                        session_daw::panel::zoom_about(press.1, at.scroll_y, at.zoom_y, to.1)
                            .min(reach(span_y, lanes.1, to.1))
                            .max(0.0);
                    (at.zoom_x, at.zoom_y) = to;
                    if frame_index == 0 {
                        editor.key("z", Default::default());
                        editor.press(
                            dock_box.x0 + 400.0,
                            dock_box.y0 + 26.0 + 120.0,
                            Default::default(),
                            0,
                        );
                    }
                    // The same drag on the docked kit, through its own
                    // zoom tool.
                    editor.moved(
                        dock_box.x0 + 400.0 + travel * 300.0,
                        dock_box.y0 + 26.0 + 120.0 - travel * 60.0,
                        Default::default(),
                    );
                } else if *name == "slip drag" {
                    if let Some((still, (x, y))) = slip {
                        at = View {
                            play_at: at.play_at,
                            ..still
                        };
                        // The offset sweeps a few seconds and back, so the
                        // waveform is regenerated every frame rather than
                        // landing on the same path twice.
                        let slide = tri(t) * 4.0 * PPS;
                        let event = if frame_index == 0 {
                            use blitz_traits::events::Modifiers;
                            blitz_traits::events::UiEvent::PointerDown(pointer_at(
                                x,
                                y,
                                Modifiers::CONTROL | Modifiers::ALT,
                            ))
                        } else {
                            use blitz_traits::events::Modifiers;
                            blitz_traits::events::UiEvent::PointerMove(pointer_at(
                                x + slide,
                                y,
                                Modifiers::CONTROL | Modifiers::ALT,
                            ))
                        };
                        *view.borrow_mut() = at;
                        widget.event(&event);
                    }
                } else {
                    // The playhead across the song, the camera following
                    // it a page at a time — the hits scroll past, and
                    // every frame is a fresh page of markers.
                    editor.editor.pan_px(-2.0, 0.0);
                }
                *view.borrow_mut() = at;

                painted += arrange
                    .frame(|painter| {
                        painter.fill(
                            vello::peniko::Fill::NonZero,
                            Affine::IDENTITY,
                            palette.surface,
                            None,
                            &vello::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
                        );
                        painter
                            .append_scene(widget.paint_scene(width, above, 1.0), Affine::IDENTITY);
                        paint_dock(painter, palette, &mut editor, dock_box);
                    })
                    .expect("render the arrangement");
                mixer.drive(t, frame_index);
                painted += mixer_gpu
                    .frame(|painter| {
                        mixer.draw(painter, palette, font);
                    })
                    .expect("render the mixer");
            }
            arrange
                .wait()
                .expect("the gpu to finish the arrangement batch");
            mixer_gpu.wait().expect("the gpu to finish the mixer batch");
            let per_frame = batch_start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64;
            stages.frame.push_ms(per_frame);
            stages.paint.push_ms(painted / BATCH as f64);
        }
        stages.frame.drop_warmup(1);
        stages.paint.drop_warmup(1);
        let frame = stages.frame.summary().expect("batches");
        let paint = stages.paint.summary().expect("batches");
        println!(
            "  {name:<20} {:>7.2}ms {:>7.2}ms {:>7.2}ms {:>9.0}   {:>7.2} {:>7.2}",
            frame.mean,
            frame.p99,
            frame.worst,
            frame.fps(),
            paint.mean,
            (frame.mean - paint.mean).max(0.0),
        );
        if frame.p99 > worst_p99 {
            worst_p99 = frame.p99;
            worst_name = name;
        }
    }
    // Let go of the slip, and check it was one: a press that missed the
    // item would have measured a still view and called it a slip drag.
    // The edit it queues goes nowhere — nothing here drains the queue.
    if let Some((_, (x, y))) = slip {
        widget.event(&blitz_traits::events::UiEvent::PointerUp(pointer_at(
            x,
            y,
            blitz_traits::events::Modifiers::CONTROL | blitz_traits::events::Modifiers::ALT,
        )));
        let slipped = widget
            .edits()
            .borrow()
            .iter()
            .any(|edit| matches!(edit, session_daw::engine::Edit::SlipItem(..)));
        if !slipped {
            tracing::warn!(
                "the slip drag never took hold of its item; its row measured a still view"
            );
        }
    }
    let verdict = if worst_p99 <= BUDGET_MS {
        "PASS"
    } else {
        "FAIL"
    };
    println!(
        "\n  240 Hz {verdict}: worst gesture {worst_name} at {worst_p99:.2} ms p99 — headroom {:.2}x\n",
        BUDGET_MS / worst_p99.max(0.001)
    );
}

/// Write one frame of the Patch List view to a PNG.
///
/// `FTS_BENCH_PATCH_LIST=apps/session-daw/fixtures/patch-list.png`, the
/// fixture album resolved against the fixture room — the picture
/// `apps/session-daw/tests/patch_list_view.rs` compares byte for byte.
fn patch_list_shot(out: &std::path::Path, width: u32, height: u32) {
    let table = match session_daw::patch_list::Table::fixture() {
        Ok(table) => table,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let Some(buffer) = session_daw::patch_list::shot(&table, (width, height)) else {
        eprintln!("no gpu: the patch list picture needs a real device");
        std::process::exit(2);
    };
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!(
        "  wrote {} — {} performers, {} buses, {} unresolved",
        out.display(),
        table.performers.len(),
        table.buses.len(),
        table.unresolved
    );
}

/// Write one frame of the Patch List view, overridden and stale, to a
/// PNG.
///
/// `FTS_BENCH_PATCH_LIST_OVERRIDDEN_STALE=apps/session-daw/fixtures/patch-list-overridden-stale.png`
/// — the fixture album, a session override on Cody's DI, and a stale
/// banner, the picture `apps/session-daw/tests/patch_list_view.rs`
/// compares structurally.
fn patch_list_shot_overridden_stale(out: &std::path::Path, width: u32, height: u32) {
    let table = match session_daw::patch_list::Table::fixture_overridden_stale() {
        Ok(table) => table,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let Some(buffer) = session_daw::patch_list::shot(&table, (width, height)) else {
        eprintln!("no gpu: the patch list picture needs a real device");
        std::process::exit(2);
    };
    image::save_buffer(out, &buffer, width, height, image::ColorType::Rgba8)
        .expect("write the frame");
    println!(
        "  wrote {} — {} performers, {} buses, stale: {}",
        out.display(),
        table.performers.len(),
        table.buses.len(),
        table.stale.is_some()
    );
}

/// Frame faces, drawn the two ways the FX view can: `FTS_BENCH_FACES=1`
/// for the set below, or a design (`comp/01-fet-black-tall`) for one.
/// One design a process is the reliable way: frame's renderer keeps its
/// Vello state across devices, and each face here opens its own.
///
/// Per face, on one device: its vectors recorded into a scene and
/// rendered (what the view does with no GPU path); `LiveSurface::render`,
/// its effects stage included, every frame (a face that moves); and,
/// polled at 60 Hz for a second, how many of those frames it actually
/// renders (a face that does not — the GPU path skips the rest). Sizes
/// are what a 1440p lane gives a face (1.5× its design).
fn faces_bench(which: &str) {
    const DESIGNS: [&str; 7] = [
        "comp/01-fet-black-tall",
        "comp/04-opto-leveler-tall",
        "saturate/03-transformer",
        "saturate/04-transistor",
        "rig-faces/80-verb-room-tall",
        "rig-faces/81-verb-hall-tall",
        "rig-faces/52-amp-eq",
    ];
    const RUNS: usize = 120;
    let dir = std::env::var_os("FTS_FRAME_DIR").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../frame/examples/plugins")
        },
        Into::into,
    );
    println!(
        "  {:<30} {:>9} {:>9} {:>9} {:>8} {:>8}",
        "face", "size", "vectors", "gpu", "renders", "of"
    );
    let designs: Vec<&str> = if which == "1" {
        DESIGNS.to_vec()
    } else {
        which.split(',').collect()
    };
    for design in designs {
        let path = dir.join(design);
        let (Some(parent), Some(stem)) = (path.parent(), path.file_name()) else {
            continue;
        };
        let (markup, catalog) = frame_live::face_paths(parent, &stem.to_string_lossy());
        let open = || {
            let mut s = frame_live::LiveSurface::open(&markup, catalog.clone()).ok()?;
            s.set_animate(true);
            s.set_responsive(true);
            Some(s)
        };
        let Some(mut vectors) = open() else {
            println!("  {design:<30} (did not open)");
            continue;
        };
        let (dw, dh) = vectors.content_size();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a face's design size, a few hundred pixels"
        )]
        let (w, h) = ((dw * 1.5).round() as u32, (dh * 1.5).round() as u32);
        let Ok(mut renderer) = Headless::new(w, h) else {
            eprintln!("no GPU");
            return;
        };
        // Vectors: recorded into the frame's scene, rendered with it.
        let started = Instant::now();
        for batch in 0..RUNS / BATCH {
            for _ in 0..BATCH {
                let _ = renderer.frame(|p| vectors.paint_vectors(p, w, h, 1.0));
            }
            if batch == 0 {
                let _ = renderer.wait();
            }
        }
        let _ = renderer.wait();
        let vector_ms = started.elapsed().as_secs_f64() * 1000.0 / RUNS as f64;
        // The GPU path, rendering every frame.
        let Some(mut gpu) = open() else { continue };
        let (device, queue) = renderer.gpu();
        if gpu.attach_gpu(device, queue).is_err() {
            println!("  {design:<30} (no GPU path)");
            continue;
        }
        let _ = gpu.render(w, h, 1.0);
        let _ = renderer.wait();
        let started = Instant::now();
        for _ in 0..RUNS {
            let _ = gpu.render(w, h, 1.0);
        }
        let _ = renderer.wait();
        let gpu_ms = started.elapsed().as_secs_f64() * 1000.0 / RUNS as f64;
        // Polled at 60 Hz for a second, paced at the view's 30 fps: the
        // frames it renders.
        gpu.set_max_fps(30);
        let mut rendered = 0;
        for _ in 0..60 {
            if gpu.needs_redraw() {
                let _ = gpu.render(w, h, 1.0);
                rendered += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        println!(
            "  {design:<30} {:>9} {vector_ms:>7.2}ms {gpu_ms:>7.2}ms {rendered:>8} {:>8}",
            format!("{w}x{h}"),
            60
        );
    }
}
