//! What a frame actually costs, at a real display size.
use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Viewport};
use expression_editor_ui::{canvas, paint, text};
use std::time::Instant;

const PPQ: f64 = 960.0;

fn big(notes: usize, w: f64, h: f64) -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0 * notes as f64);
    for i in 0..notes as u64 {
        let mut n = Note::new(
            NoteId(i + 1),
            PPQ * i as f64 * 0.5,
            PPQ * i as f64 * 0.5 + PPQ * 0.4,
            36 + (i % 60) as i32,
        );
        for k in 0..24 {
            let f = k as f64 / 23.0;
            n.pitch.set(n.start + (n.end - n.start) * f, -1.0 + 2.0 * f);
        }
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(w, h));
    ed.reset_view();
    ed
}

#[test]
#[ignore = "measurement"]
fn frame_cost_at_5120x1440() {
    // The roll's box on a 5120x1440 display, less chrome.
    let (w, h) = (
        5120.0 - 236.0 - canvas::GUTTER_W,
        1440.0 - 116.0 - canvas::RULER_H,
    );
    for notes in [200usize, 2000] {
        let ed = big(notes, w, h);
        let mut labels = text::Labeller::new();
        let ov = paint::Overlay::default();

        // Warm the shaping cache, as a running editor's would be.
        let scene = paint::roll_scene(
            &ed,
            w + canvas::GUTTER_W,
            h + canvas::RULER_H,
            &ov,
            &mut labels,
        );
        let commands = scene.commands.len();

        let t = Instant::now();
        const N: u32 = 20;
        for _ in 0..N {
            std::hint::black_box(paint::roll_scene(
                &ed,
                w + canvas::GUTTER_W,
                h + canvas::RULER_H,
                &ov,
                &mut labels,
            ));
        }
        let build = t.elapsed().as_secs_f64() * 1000.0 / N as f64;

        let t = Instant::now();
        for _ in 0..N {
            std::hint::black_box(scene.clone());
        }
        let clone = t.elapsed().as_secs_f64() * 1000.0 / N as f64;

        println!(
            "{notes:>5} notes: {commands:>7} commands  build {build:>7.2}ms  clone {clone:>7.2}ms"
        );
    }
}

/// Where the build time goes: computing the geometry, or encoding it?
#[test]
#[ignore = "measurement"]
fn what_the_build_spends_its_time_on() {
    let (w, h) = (
        5120.0 - 236.0 - canvas::GUTTER_W,
        1440.0 - 116.0 - canvas::RULER_H,
    );
    let ed = big(2000, w, h);
    const N: u32 = 20;

    let t = Instant::now();
    for _ in 0..N {
        std::hint::black_box(canvas::note_rects(&ed));
    }
    println!(
        "note_rects   {:>7.2}ms",
        t.elapsed().as_secs_f64() * 1000.0 / N as f64
    );

    let t = Instant::now();
    for _ in 0..N {
        std::hint::black_box(canvas::curve_paths(&ed));
    }
    println!(
        "curve_paths  {:>7.2}ms",
        t.elapsed().as_secs_f64() * 1000.0 / N as f64
    );

    // Curve points used to be `"x,y x,y "` strings — an svg attribute —
    // which the painter parsed straight back into numbers. They are
    // numbers the whole way now, so there is no round trip left to time.
    let curves = canvas::curve_paths(&ed);
    let points: usize = curves.iter().map(|c| c.points.len()).sum();
    println!("             {points} curve points, carried as numbers");
}

/// The rest of the build, piece by piece.
#[test]
#[ignore = "measurement"]
fn the_remaining_build_cost() {
    let (w, h) = (
        5120.0 - 236.0 - canvas::GUTTER_W,
        1440.0 - 116.0 - canvas::RULER_H,
    );
    let ed = big(2000, w, h);
    const N: u32 = 20;
    macro_rules! t {
        ($name:expr, $e:expr) => {{
            let t = Instant::now();
            for _ in 0..N {
                std::hint::black_box($e);
            }
            println!(
                "{:<14} {:>7.2}ms",
                $name,
                t.elapsed().as_secs_f64() * 1000.0 / N as f64
            );
        }};
    }
    t!("rows", canvas::rows(&ed));
    t!("grid_lines", canvas::grid_lines(&ed));
    t!("cc_paths", canvas::cc_paths(&ed));
    t!("keyboard", canvas::keyboard(&ed));
    t!("note_handles", canvas::note_handles(&ed));
    t!("separators", canvas::separators(&ed));
    t!("reference", canvas::reference_rects(&ed));

    // Label lookup: a cache hit still allocates a `String` key and
    // clones the shaped run, once per label per frame.
    let mut labels = text::Labeller::new();
    labels.shape("C4", 10.0);
    let t = Instant::now();
    for _ in 0..N {
        for _ in 0..2000 {
            std::hint::black_box(labels.shape("C4", 10.0));
        }
    }
    println!(
        "{:<14} {:>7.2}ms  (2000 cache hits)",
        "label hits",
        t.elapsed().as_secs_f64() * 1000.0 / N as f64
    );
}

/// The layers `roll_scene` calls that nothing above measured.
#[test]
#[ignore = "measurement"]
fn the_unmeasured_layers() {
    let (w, h) = (
        5120.0 - 236.0 - canvas::GUTTER_W,
        1440.0 - 116.0 - canvas::RULER_H,
    );
    let ed = big(2000, w, h);
    const N: u32 = 20;
    macro_rules! t {
        ($name:expr, $e:expr) => {{
            let t = Instant::now();
            for _ in 0..N {
                std::hint::black_box($e);
            }
            println!(
                "{:<16} {:>7.2}ms",
                $name,
                t.elapsed().as_secs_f64() * 1000.0 / N as f64
            );
        }};
    }
    t!("guitar::joins", expression_editor_ui::guitar::joins(&ed));
    t!("take_waveform", canvas::take_waveform(&ed));
    t!("sibilant_bands", canvas::sibilant_bands(&ed));
    t!("midi_reference", canvas::midi_reference_rects(&ed));
    t!("razor_rects", canvas::razor_rects(&ed));
    t!("zone_guides", canvas::zone_guides(&ed));
    t!("tuning_guides", canvas::tuning_guides(&ed));
    t!("lane_boxes", canvas::lane_boxes(&ed));

    // Emitting the labels, as opposed to shaping them: `draw` clones the
    // shaped glyph runs into a command, once per note.
    let mut labels = text::Labeller::new();
    let shaped = labels.shape("C4", 10.0);
    let t = Instant::now();
    for _ in 0..N {
        let mut scene = anyrender::Scene::new();
        for _ in 0..2000 {
            text::draw(
                &mut scene,
                &shaped,
                10.0,
                10.0,
                text::Align::Left,
                peniko::Color::BLACK,
                kurbo::Affine::IDENTITY,
            );
        }
        std::hint::black_box(scene);
    }
    println!(
        "{:<16} {:>7.2}ms  (2000 labels)",
        "label draw",
        t.elapsed().as_secs_f64() * 1000.0 / N as f64
    );
}

/// What a nearly empty roll puts in a frame at 5120x1440.
///
/// The note count is not the only thing that scales: rows scale with
/// height over zoom, and gridlines with width over the grid step.
#[test]
#[ignore = "measurement"]
fn what_five_notes_cost_on_a_wide_display() {
    let (w, h) = (
        5120.0 - 236.0 - canvas::GUTTER_W,
        1440.0 - 116.0 - canvas::RULER_H,
    );
    let ed = big(5, w, h);
    println!(
        "viewport {:.0}x{:.0}  px_per_row {:.2}",
        ed.viewport.w, ed.viewport.h, ed.camera.vertical.px_per_row
    );
    println!("rows        {}", canvas::rows(&ed).len());
    println!("gridlines   {}", canvas::grid_lines(&ed).len());
    println!("keys        {}", canvas::keyboard(&ed).len());
    println!("notes       {}", canvas::note_rects(&ed).len());

    let mut labels = text::Labeller::new();
    let ov = paint::Overlay::default();
    let scene = paint::roll_scene(
        &ed,
        w + canvas::GUTTER_W,
        h + canvas::RULER_H,
        &ov,
        &mut labels,
    );
    println!("commands    {}", scene.commands.len());

    const N: u32 = 20;
    let t = Instant::now();
    for _ in 0..N {
        std::hint::black_box(paint::roll_scene(
            &ed,
            w + canvas::GUTTER_W,
            h + canvas::RULER_H,
            &ov,
            &mut labels,
        ));
    }
    println!(
        "build       {:.2}ms",
        t.elapsed().as_secs_f64() * 1000.0 / N as f64
    );
}

// ── the cost of moving the camera ────────────────────────────────────

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use expression_editor_ui::ExpressionEditor;

thread_local! {
    static STAGED: std::cell::RefCell<Option<Editor>> = const { std::cell::RefCell::new(None) };
}

#[component]
fn PanSurface() -> Element {
    use_hook(|| expression_editor_ui::available_space(5120.0, 1440.0));
    let mut editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    rsx! {
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; }}" }
        div {
            style: "width: 100vw; height: 100vh;",
            button {
                "data-testid": "pan",
                style: "position: absolute; top: 0; left: 0; z-index: 10; height: 12px;",
                onclick: move |_| editor.write().pan_px(37.0, 11.0),
                "pan"
            }
            ExpressionEditor { editor }
        }
    }
}

/// Panning is a camera write, and a camera write re-renders every
/// component that reads the editor — the toolbar, the switcher, the
/// chord row, the inspector, the strip and the status bar, none of which
/// the camera changed. This measures what one pan costs *through the
/// DOM*, which is what a drag pays per mouse event.
#[test]
#[ignore = "measurement"]
fn what_a_pan_costs() {
    for notes in [5usize, 500, 2000] {
        let ed = big(
            notes,
            5120.0 - 236.0 - canvas::GUTTER_W,
            1440.0 - 116.0 - canvas::RULER_H,
        );
        STAGED.with(|s| *s.borrow_mut() = Some(ed));
        let doc = render(PanSurface).with_window_size(5120, 1440).build();
        doc.drain();
        doc.relayout();

        let pan = doc
            .query(by_testid("pan"))
            .immediately()
            .expect("no pan button");
        const N: u32 = 30;
        // Split, because the two halves have completely different fixes:
        // `drain` is dioxus re-rendering every component that reads the
        // editor and applying the DOM mutations, `relayout` is Blitz
        // resolving style and layout for the whole document.
        let (mut render_ms, mut layout_ms) = (0.0, 0.0);
        let t = Instant::now();
        for _ in 0..N {
            pan.click();
            let a = Instant::now();
            doc.drain();
            render_ms += a.elapsed().as_secs_f64() * 1000.0;
            let b = Instant::now();
            doc.relayout();
            layout_ms += b.elapsed().as_secs_f64() * 1000.0;
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / N as f64;
        println!(
            "{notes:>5} notes:   render+DOM {:>6.2}ms   style+layout {:>6.2}ms",
            render_ms / N as f64,
            layout_ms / N as f64
        );
        // A frame at 120 fps is 8.3ms, and this is only the DOM half —
        // paint and the GPU still have to happen inside it.
        println!(
            "{notes:>5} notes: pan (render + DOM + layout) {per:>6.2}ms  \
             = {:.0} pans/s before a frame is missed",
            1000.0 / per
        );
    }
}
