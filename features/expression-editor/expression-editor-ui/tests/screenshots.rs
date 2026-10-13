//! Visual-inspection harness: rasterize the real editor to PNGs.
//!
//! Mounts `ExpressionEditor` on the headless Blitz DOM and paints it
//! through `DocumentTester::render_png` — CPU rasterizer, no GPU, no
//! DAW, no browser. Nothing asserts about looks; a wrong-looking canvas
//! is a picture you have to open.
//!
//! **Every test here is `#[ignore]`d, because none of them is a test.**
//! They produce artefacts for a human to look at, and they are
//! expensive: `shoot_every_scene` paints ~49 scenes through a software
//! rasterizer and runs for minutes. Left in the default run it did more
//! than waste time — nextest schedules the whole suite in parallel, so
//! one test holding a core for six minutes starved the rest and two
//! dozen unrelated tests failed on the 30-second timeout. A suite that
//! goes red for reasons unconnected to the code is worse than a slow
//! one.
//!
//! ```sh
//! just ee-shots        # or: cargo test -p expression-editor-ui \
//!                      #       --test screenshots -- --ignored
//! ```
//!
//! Output lands in `target/gui-shots/expression-editor/` (override with
//! `FTS_SHOTS_DIR`).
//!
//! Scenes come from `expression_editor_ui::demo`, the same module the
//! runnable example mounts, so these pictures cannot drift from what
//! the app actually launches.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::DocumentTester;
use expression_editor_core::{Editor, Viewport};
use expression_editor_ui::demo::{self, Scene};
use expression_editor_ui::theme;
use expression_editor_ui::{ExpressionEditor, ModDrawer, MultiTool};

const W: u32 = 1100;
/// The viewport a seeded document is built with.
///
/// Only an opening value now: the harness reports its window below, the
/// same way the desktop runner does, so the editor resizes to fill the
/// frame on mount. It used to be the final word, which is why every
/// committed shot showed a roll that stopped short of the inspector with
/// a dead band under the status bar — the picture was of an editor that
/// had never been told how much room it had.
const CANVAS_H: f64 = 400.0;
const H: u32 = 700;

#[component]
fn Harness(
    seed: Editor,
    drawer: Option<ModDrawer>,
    multi: Option<MultiTool>,
    draft: Option<expression_editor_core::PitchDraft>,
    flow: Option<expression_editor_ui::BendFlow>,
) -> Element {
    let editor = use_signal(|| seed.clone());
    // State the space, as a host does. Without it these pictures show an
    // editor that was never told how big its window is.
    use_hook(|| expression_editor_ui::available_space(W as f64, H as f64));
    rsx! {
        style {
            "html, body {{ width: 100%; height: 100%; margin: 0; padding: 0; \
              overflow: hidden; background: {theme::BG}; }}"
        }
        div {
            // `vh`/`vw`, not `100%`. A percentage height resolves against
            // the parent, and a headless Blitz mount gives `body` no
            // resolved height — so the editor laid out to its content
            // and left a band of background under the status bar in
            // every shot. The standalone runner's root documents the
            // same trap; the harness had not followed it.
            style: "width: 100vw; height: 100vh;",
            ExpressionEditor {
                editor,
                initial_drawer: drawer.clone(),
                initial_multi: multi.clone(),
                initial_draft: draft.clone(),
                bend_flow: flow,
            }
        }
    }
}

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/gui-shots/expression-editor")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

async fn shoot(ed: Editor, name: &str) {
    shoot_full(ed, None, None, None, name).await
}

async fn shoot_with(ed: Editor, drawer: Option<ModDrawer>, name: &str) {
    shoot_full(ed, drawer, None, None, name).await
}

async fn shoot_with_draft(ed: Editor, draft: expression_editor_core::PitchDraft, name: &str) {
    shoot_full(ed, None, None, Some(draft), name).await
}

/// A shot with an explicit bend-flow variant (#161).
async fn shoot_flow(ed: Editor, flow: expression_editor_ui::BendFlow, name: &str) {
    shoot_inner(ed, None, None, None, Some(flow), name).await
}

async fn shoot_full(
    ed: Editor,
    drawer: Option<ModDrawer>,
    multi: Option<MultiTool>,
    draft: Option<expression_editor_core::PitchDraft>,
    name: &str,
) {
    shoot_inner(ed, drawer, multi, draft, None, name).await
}

async fn shoot_inner(
    ed: Editor,
    drawer: Option<ModDrawer>,
    multi: Option<MultiTool>,
    draft: Option<expression_editor_core::PitchDraft>,
    flow: Option<expression_editor_ui::BendFlow>,
    name: &str,
) {
    // The canvas measures itself from the mounted element; headless
    // there is no resize event, so state the viewport the shot uses.
    let mut ed = ed;
    ed.resize(Viewport::new(W as f64, CANVAS_H));

    let dom = VirtualDom::new_with_props(
        Harness,
        HarnessProps {
            seed: ed,
            drawer,
            multi,
            draft,
            flow,
        },
    );
    let tester = DocumentTester::from_virtual_dom(dom)
        .with_window_size(W, H)
        .build();
    for _ in 0..4 {
        let _ = tester.pump().await;
    }
    let path = shots_dir().join(format!("{name}.png"));
    tester.render_png(&path);
    println!("shot: {}", path.display());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_every_scene() {
    for scene in Scene::ALL {
        let ed = demo::editor(scene, Viewport::new(W as f64, CANVAS_H));
        shoot_flow(ed, scene.bend_flow(), scene.slug()).await;
    }
}

/// **Prototype (#161).** The six-string roll down a zoom ladder.
///
/// The question is not "is it pretty at one size" but *where it breaks
/// and what breaks first*, so this walks the same riff from framed-item
/// out to a whole-song overview and back in to one bend filling the
/// screen.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_guitar_zoom_ladder() {
    use expression_editor_ui::BendFlow;

    let vp = Viewport::new(W as f64, CANVAS_H);
    let base = || demo::editor(Scene::Guitar, vp);

    // Zoomed in on the full bend: one gesture, all the detail there is.
    let mut ed = base();
    for _ in 0..5 {
        ed.zoom_in_at(W as f64 * 0.45, CANVAS_H * 0.5, 1.25);
    }
    shoot_flow(ed, BendFlow::OnRow, "40-guitar-zoom-close").await;

    // Progressively out. Each step halves the pixels per beat and the
    // pixels per string at once, because both axes are being asked
    // whether they still say anything.
    for (i, out) in [2.0_f64, 4.0, 8.0, 16.0].iter().enumerate() {
        let mut ed = base();
        ed.camera.units_per_px *= out;
        ed.camera.vertical.px_per_row /= out;
        // Through the editor's own clamps, so the ladder shows what the
        // real camera would allow rather than an impossible zoom.
        let (b, vp) = (ed.bounds(), ed.viewport);
        ed.camera.constrain(b, vp);
        shoot_flow(
            ed,
            BendFlow::OnRow,
            &format!("4{}-guitar-zoom-out-{out:.0}x", i + 1),
        )
        .await;
    }
}

/// The same phrase at three zoom depths — the camera is most of the
/// feel, and it is the part a still picture can still show.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_zoom_levels() {
    let base = || demo::editor(Scene::Phrase, Viewport::new(W as f64, CANVAS_H));

    let mut ed = base();
    for _ in 0..6 {
        ed.zoom_in_at(W as f64 * 0.45, CANVAS_H * 0.5, 1.25);
    }
    shoot(ed, "08-zoom-in").await;

    // Near an item edge, where the edge magnet frames the boundary.
    let mut ed = base();
    for _ in 0..8 {
        ed.zoom_in_at(W as f64 * 0.95, CANVAS_H * 0.5, 1.25);
    }
    shoot(ed, "09-edge-magnet").await;
}

/// Before and after the Robot button, on the same note.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_flatten() {
    let vp = Viewport::new(W as f64, CANVAS_H);
    let mut ed = demo::editor(Scene::Phrase, vp);
    let id = ed.selection.notes[0];
    let (t0, t1) = {
        let n = ed.doc.note(id).unwrap();
        (n.start, n.end)
    };
    for _ in 0..3 {
        ed.zoom_in_at(W as f64 * 0.5, CANVAS_H * 0.5, 1.3);
    }
    shoot(ed.clone(), "10-as-sung").await;

    ed.apply(&expression_editor_core::Edit::ReblendPitch {
        note: id,
        t0,
        t1,
        drift_amount: 0.0,
        modulation_amount: 0.0,
    });
    shoot(ed, "11-robot").await;
}

/// The modulation drawer, opened through its real capture path.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_modulation_drawer() {
    let mut ed = demo::editor(Scene::Phrase, Viewport::new(W as f64, CANVAS_H));
    for _ in 0..3 {
        ed.zoom_in_at(W as f64 * 0.5, CANVAS_H * 0.5, 1.3);
    }
    let mut drawer = ModDrawer::default();
    assert!(drawer.open_on(&ed), "a note is selected, so it must open");
    drawer.preview(&mut ed);
    shoot_with(ed, Some(drawer), "12-modulation").await;
}

/// Contextual zoom on a part whose density changes: the same gesture at
/// two positions must produce two different zoom levels.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_smart_zoom() {
    use expression_editor_core::zoom::ZoomModes;

    let vp = Viewport::new(W as f64, CANVAS_H);
    let ed = demo::editor(Scene::Density, vp);

    let mut dense = ed.clone();
    dense.smart_zoom(ZoomModes::NOTE_AREA, demo::PPQ * 1.0, 62.0);
    shoot(dense, "13-smart-zoom-dense").await;

    let mut sparse = ed.clone();
    sparse.smart_zoom(ZoomModes::NOTE_AREA, demo::PPQ * 22.0, 62.0);
    shoot(sparse, "14-smart-zoom-sparse").await;

    let mut whole = ed;
    whole.smart_zoom(ZoomModes::KEYS, demo::PPQ * 1.0, 62.0);
    shoot(whole, "15-smart-zoom-item").await;
}

/// Razor edits: an area over held notes, and the result of moving it.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_razor() {
    use expression_editor_core::razor::{self, RazorArea};

    let vp = Viewport::new(W as f64, CANVAS_H);
    let mut ed = demo::editor(Scene::Held, vp);
    let area = RazorArea::new(demo::PPQ * 2.0, demo::PPQ * 4.0, 60, 67);

    ed.razor.add(area);
    shoot(ed.clone(), "17-razor-area").await;

    // Moving it slices the held notes at both edges and carries the
    // middle — the thing a marquee cannot do.
    razor::move_contents(&mut ed.doc, area, demo::PPQ * 4.0, 0, false, false);
    ed.razor.clear();
    ed.razor.add(area.translated(demo::PPQ * 4.0, 0));
    shoot(ed, "18-razor-moved").await;
}

/// The chord box on a real chord, with the velocity strip populated.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_chord_box() {
    let vp = Viewport::new(W as f64, CANVAS_H);
    let mut ed = demo::editor(Scene::Held, vp);
    // Five stacked notes: the box should name the chord, not shrug.
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
    // Spread the velocities so the strip shows shape rather than a wall.
    for (i, n) in ed.doc.notes.iter_mut().enumerate() {
        n.velocity = 0.35 + 0.13 * i as f64;
    }
    shoot(ed, "20-chord-box").await;
}

/// Pinned controller lanes behind the roll, and CC edit mode.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_cc_lanes() {
    let vp = Viewport::new(W as f64, CANVAS_H);
    let ed = demo::editor(Scene::Orchestral, vp);
    shoot(ed.clone(), "22-cc-pinned").await;

    // Editing one brings it forward and pushes the notes back.
    let mut editing = ed;
    editing.edit_cc(11);
    shoot(editing.clone(), "23-cc-edit").await;

    // And drawing on the roll writes into that dimension.
    let vph = editing.viewport.h;
    let x0 = editing.camera.x(demo::PPQ * 1.0);
    let mut drag = expression_editor_ui::interaction::pointer_down(
        &mut editing,
        x0,
        vph * 0.8,
        Default::default(),
        0,
    );
    for i in 1..=40 {
        let f = i as f64 / 40.0;
        let x = editing.camera.x(demo::PPQ * (1.0 + 6.0 * f));
        // A jagged shape, so the result is obviously hand-drawn rather
        // than the smooth curve the scene started with.
        let y = vph * (0.15 + 0.55 * (f * 9.0).sin().abs());
        expression_editor_ui::interaction::pointer_move(
            &mut editing,
            &mut drag,
            x,
            y,
            Default::default(),
        );
    }
    shoot(editing, "24-cc-drawn").await;
}

/// The Multi Tool zones over a selection, and the modes they belong to.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_multitool_and_modes() {
    use expression_editor_core::Mode;

    let vp = Viewport::new(W as f64, CANVAS_H);

    // MPE mode shows the dimension and channel controls; MIDI hides both.
    let mut mpe = demo::editor(Scene::Phrase, vp);
    mpe.set_mode(Mode::Mpe);
    mpe.selection.notes = mpe.doc.notes.iter().map(|n| n.id).collect();
    shoot(mpe.clone(), "28-mode-mpe").await;

    let mut midi = demo::editor(Scene::Phrase, vp);
    midi.set_mode(Mode::Midi);
    shoot(midi, "29-mode-midi").await;
}

/// The Multi Tool zones lit over a selection.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_multitool() {
    let vp = Viewport::new(W as f64, CANVAS_H);
    let mut ed = demo::editor(Scene::Held, vp);
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();

    let mut mt = MultiTool::default();
    assert!(mt.arm(&ed), "a selection exists, so it must arm");
    // Hovering a zone is what labels it — twelve labels at once would
    // bury the material.
    mt.hover = Some(expression_editor_core::Zone::Warp);
    mt.steep = expression_editor_core::Steepness(1.4);
    shoot_full(ed, None, Some(mt), None, "30-multitool").await;
}

/// The seven note handles, and the temporary note scoping them.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_note_handles() {
    use expression_editor_core::Mode;

    let vp = Viewport::new(W as f64, CANVAS_H);

    // Audio mode is the Melodyne surface: handles on the selection.
    let mut ed = demo::editor(Scene::Held, vp);
    ed.set_mode(Mode::PitchedAudio);
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
    shoot(ed.clone(), "32-note-handles").await;

    // A temporary note shades the range the handles now address.
    if let Some(n) = ed.doc.notes.first().cloned() {
        let a = n.start + (n.end - n.start) * 0.25;
        let b = n.start + (n.end - n.start) * 0.7;
        assert!(ed.set_temp_note(n.id, a, b), "the range must open");
    }
    shoot(ed, "33-temporary-note").await;
}

/// The waveform carried to the audio's own pitch.
///
/// Five notes on the *same* row, each sung a different amount sharp or
/// flat. The row never changes, so any vertical separation on screen is
/// the audio's pitch and nothing else.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_blob_pitch_carry() {
    use expression_editor_core::Mode;
    use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};

    const PPQ: f64 = 960.0;
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    for (i, cents) in [-70.0_f64, -35.0, 0.0, 35.0, 70.0].iter().enumerate() {
        let start = PPQ * 3.0 * i as f64;
        let len = PPQ * 2.6;
        let mut n = Note::new(NoteId(i as u64 + 1), start, start + len, 60);
        n.weight = 0.9;
        // A steady note, plus a little vibrato so the pitch track has
        // something to do against the body.
        const STEPS: usize = 40;
        for k in 0..STEPS {
            let f = k as f64 / (STEPS - 1) as f64;
            let vib = 0.12 * (f * core::f64::consts::TAU * 5.0).sin() * f;
            n.pitch.set(start + len * f, cents / 100.0 + vib);
        }
        n.envelope = (0..180)
            .map(|k| {
                let f = k as f64 / 179.0;
                let shell = (f / 0.05).min(1.0) * (1.0 - f).powf(0.4);
                let grain = 0.75 + 0.25 * (f * 97.0).sin() * (f * 11.0).cos();
                (shell * grain).clamp(0.0, 1.0) as f32
            })
            .collect();
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(W as f64, CANVAS_H));
    ed.set_mode(Mode::PitchedAudio);
    shoot(ed, "34-blob-pitch-carry").await;
}

/// The take's waveform behind the roll, unvoiced gaps in the pitch
/// track, and sibilant bands with the hollow amplitude handle.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_audio_backdrop() {
    use expression_editor_core::Mode;
    use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};

    const PPQ: f64 = 960.0;
    let end = PPQ * 16.0;
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, end);

    // Three sung notes with consonants between them.
    let spans = [
        (PPQ * 0.6, PPQ * 4.0),
        (PPQ * 5.2, PPQ * 8.6),
        (PPQ * 9.8, PPQ * 13.5),
    ];
    for (i, (s, e)) in spans.iter().enumerate() {
        let mut n = Note::new(NoteId(i as u64 + 1), *s, *e, 60 + i as i32 * 3);
        n.weight = 0.85;
        const STEPS: usize = 48;
        for k in 0..STEPS {
            let f = k as f64 / (STEPS - 1) as f64;
            let vib = 0.15 * (f * core::f64::consts::TAU * 5.0).sin() * f;
            n.pitch.set(s + (e - s) * f, -0.2 * (1.0 - f).powi(3) + vib);
        }
        n.envelope = (0..200)
            .map(|k| {
                let f = k as f64 / 199.0;
                let shell = (f / 0.05).min(1.0) * (1.0 - f).powf(0.4);
                let grain = 0.75 + 0.25 * (f * 101.0).sin() * (f * 9.0).cos();
                (shell * grain).clamp(0.0, 1.0) as f32
            })
            .collect();
        doc.push(n);
    }

    // The consonants: the gaps between notes, plus a hard one inside
    // the second note.
    doc.unvoiced = vec![
        (PPQ * 4.0, PPQ * 5.2),
        (PPQ * 6.9, PPQ * 7.3),
        (PPQ * 8.6, PPQ * 9.8),
    ];

    // The take's waveform, loud in the consonants too — which is the
    // point of showing it: a sibilant is not silence.
    doc.peaks = (0..1200)
        .map(|k| {
            let t = end * (k as f64 / 1199.0);
            let voiced = spans.iter().any(|(s, e)| t >= *s && t <= *e);
            let sib = doc.unvoiced.iter().any(|(a, b)| t >= *a && t <= *b);
            let base = if voiced {
                0.8
            } else if sib {
                0.45
            } else {
                0.04
            };
            let grain = 0.7 + 0.3 * (t / PPQ * 30.0).sin() * (t / PPQ * 3.0).cos();
            (base * grain).clamp(0.0, 1.0) as f32
        })
        .collect();

    let mut ed = Editor::new(doc, Viewport::new(W as f64, CANVAS_H));
    ed.set_mode(Mode::PitchedAudio);
    shoot(ed.clone(), "35-audio-backdrop").await;

    // Sibilant scope armed: bands shade, amplitude handle goes hollow.
    ed.sibilant_scope = true;
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
    shoot(ed, "36-sibilant-scope").await;
}

/// A pitch drawing open: anchors, the sinusoidal line, and the original
/// underneath.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_pitch_drawing() {
    use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
    use expression_editor_core::{Mode, PitchDraft};

    const PPQ: f64 = 960.0;
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), PPQ * 0.5, PPQ * 7.0, 60);
    n.weight = 0.85;
    const STEPS: usize = 64;
    for k in 0..STEPS {
        let f = k as f64 / (STEPS - 1) as f64;
        // Sung wobbly and drifting — something worth redrawing.
        let drift = -0.7 * f;
        let wobble = 0.3 * (f * core::f64::consts::TAU * 3.0).sin();
        n.pitch.set(n.start + (n.end - n.start) * f, drift + wobble);
    }
    n.envelope = (0..200)
        .map(|k| {
            let f = k as f64 / 199.0;
            let shell = (f / 0.05).min(1.0) * (1.0 - f).powf(0.4);
            (shell * (0.75 + 0.25 * (f * 90.0).sin())).clamp(0.0, 1.0) as f32
        })
        .collect();
    doc.push(n);

    let mut ed = Editor::new(doc, Viewport::new(W as f64, CANVAS_H));
    ed.set_mode(Mode::PitchedAudio);
    ed.selection.notes = vec![NoteId(1)];

    // A drawing: straighten the drift out and put a deliberate vibrato
    // at the end, which is Tip 1 in the manual.
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(PPQ * 0.6, 0.0);
    d.add(PPQ * 2.0, 0.05);
    d.add(PPQ * 3.4, -0.05);
    d.add(PPQ * 4.4, 0.35);
    d.add(PPQ * 5.0, -0.35);
    d.add(PPQ * 5.6, 0.35);
    d.add(PPQ * 6.2, -0.35);
    d.add(PPQ * 6.9, 0.0);
    ed.preview_draft(&mut d);

    shoot_with_draft(ed, d, "37-pitch-drawing").await;
}

/// Timing separators, and the MIDI reference behind the sung notes.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_timing_and_reference() {
    use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
    use expression_editor_core::{MidiReference, Mode, RefNote};

    const PPQ: f64 = 960.0;
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    // Four abutting notes, deliberately drifting off the beat so the
    // separator colours have something to report.
    let bounds = [
        (0.0, PPQ * 3.7),
        (PPQ * 3.7, PPQ * 8.0),
        (PPQ * 8.0, PPQ * 11.4),
        (PPQ * 11.4, PPQ * 15.0),
    ];
    for (i, (s, e)) in bounds.iter().enumerate() {
        let mut n = Note::new(NoteId(i as u64 + 1), *s, *e, 60 + (i as i32 % 3) * 2);
        n.weight = 0.85;
        const STEPS: usize = 40;
        for k in 0..STEPS {
            let f = k as f64 / (STEPS - 1) as f64;
            n.pitch.set(
                s + (e - s) * f,
                0.18 * (f * core::f64::consts::TAU * 4.0).sin(),
            );
        }
        n.envelope = (0..160)
            .map(|k| {
                let f = k as f64 / 159.0;
                let shell = (f / 0.06).min(1.0) * (1.0 - f).powf(0.4);
                (shell * (0.75 + 0.25 * (f * 83.0).sin())).clamp(0.0, 1.0) as f32
            })
            .collect();
        doc.push(n);
    }

    let mut ed = Editor::new(doc, Viewport::new(W as f64, CANVAS_H));
    ed.set_mode(Mode::PitchedAudio);

    // A reference part on the beat, showing where the phrase should be.
    let mut r = MidiReference::new(
        "Vocal.mid",
        vec!["Track 1".into(), "Track 2".into()],
        (0..4)
            .map(|i| RefNote {
                start: PPQ * 4.0 * i as f64,
                end: PPQ * 4.0 * i as f64 + PPQ * 3.6,
                row: 60 + (i % 3) * 2,
            })
            .collect(),
    );
    r.bpm = Some(120.0);
    r.beats_per_bar = Some(4.0);
    ed.reference = Some(r);
    shoot(ed.clone(), "38-midi-reference").await;

    ed.timing_mode = true;
    shoot(ed, "39-timing-separators").await;
}

/// The stacked multitrack view: a vocal, its reference MIDI, a guitar
/// and a kit, each drawn in its own mode on one shared timeline.
#[tokio::test(flavor = "current_thread")]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn shoot_stack() {
    use expression_editor_core::rows::{RowSpace, SliceBands, StringTuning};
    use expression_editor_core::tracks::Track;
    use expression_editor_core::{ExpressionDoc, Mode, Note, NoteId, TimeBase};

    fn part(rate: f64, hits: &[(f64, i32)], len: f64) -> ExpressionDoc {
        let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: rate }, 0.0, rate * 4.0);
        for (i, &(s, row)) in hits.iter().enumerate() {
            doc.push(Note::new(
                NoteId(i as u64 + 1),
                s * rate,
                (s + len) * rate,
                row,
            ));
        }
        doc
    }

    // A sung line.
    let vox = [
        (0.10, 62),
        (0.55, 64),
        (1.05, 67),
        (1.60, 65),
        (2.10, 64),
        (2.70, 62),
    ];
    let mut ed = Editor::new(
        part(172.265625, &vox, 0.40),
        Viewport::new(W as f64, CANVAS_H),
    );
    ed.set_mode(Mode::PitchedAudio);
    ed.tracks.rename(0, "Lead Vox");

    // The same line as a MIDI reference, dead on the grid.
    let refs = [
        (0.00, 62),
        (0.50, 64),
        (1.00, 67),
        (1.50, 65),
        (2.00, 64),
        (2.50, 62),
    ];
    ed.tracks.push(Track::in_mode(
        "Ref MIDI",
        part(172.265625, &refs, 0.45),
        Mode::Midi,
    ));

    // A guitar part on strings 2 and 3.
    let gtr = [
        (0.00, 2),
        (0.50, 3),
        (1.00, 2),
        (1.50, 3),
        (2.00, 2),
        (2.50, 3),
    ];
    let mut guitar = part(172.265625, &gtr, 0.45);
    guitar.row_space = RowSpace::Strings(StringTuning::guitar_standard());
    ed.tracks
        .push(Track::in_mode("Guitar", guitar, Mode::Guitar));

    // Kick/snare on the beat, hats on the eighths.
    let mut kit_hits = Vec::new();
    for b in 0..6 {
        let t = b as f64 * 0.5;
        kit_hits.push((t, if b % 2 == 0 { 0 } else { 1 }));
        kit_hits.push((t + 0.25, 2));
    }
    let mut kit = part(86.1328125, &kit_hits, 0.06);
    kit.row_space = RowSpace::Bands(SliceBands::default());
    ed.tracks
        .push(Track::in_mode("Kit", kit, Mode::UnpitchedAudio));

    ed.camera.t0 = -20.0;
    ed.camera.units_per_px = 0.55;
    ed.stacked = true;

    shoot(ed, "stack-multitrack").await;
}

/// A screenshot of a **real** Guitar Pro file — one Guitar Pro itself
/// wrote, not a model built in code (#168).
///
/// `Effects.gp3` from PyGuitarPro's fixture corpus: 47 notes across six
/// strings, twelve articulations, and one bend that survives at full
/// fidelity. Skips if the corpus is not checked out.
#[tokio::test]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn real_guitar_pro_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../expression-editor-guitarpro/tests/fixtures/Effects.gp3");
    if !path.exists() {
        return;
    }
    let imported = expression_editor_guitarpro::import_file(&path.to_string_lossy())
        .expect("a real Guitar Pro file must import");

    let mut ed = Editor::new(imported.doc, demo::default_viewport());
    ed.set_mode(expression_editor_core::Mode::Guitar);
    ed.reset_view();

    shoot(ed, "45-real-guitar-pro").await;
}

/// Richer real music, when a corpus is staged at `/tmp/gp-music`.
///
/// Not committed: several of alphaTab's fixtures are copyrighted
/// transcriptions. These are public-domain compositions, rendered
/// locally for a look rather than checked in.
#[tokio::test]
#[ignore = "picture generator, not an assertion — `just ee-shots`"]
async fn staged_real_music() {
    let dir = std::path::Path::new("/tmp/gp-music");
    if !dir.exists() {
        return;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.to_string_lossy().to_string();
        if expression_editor_guitarpro::parse::Format::of_path(&name).is_none() {
            continue;
        }
        let Ok(imported) = expression_editor_guitarpro::import_file(&name) else {
            continue;
        };
        let stem = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .replace('.', "-");
        let mut ed = Editor::new(imported.doc, demo::default_viewport());
        ed.set_mode(expression_editor_core::Mode::Guitar);
        ed.reset_view();
        shoot(ed, Box::leak(format!("50-{stem}").into_boxed_str())).await;
    }
}
