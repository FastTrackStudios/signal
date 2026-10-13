//! Write the lit visualizer example surfaces (processor's panel shaders as
//! kit components) and the catalogs they play against.
//!
//! ```text
//! make_lit_viz_examples [--out-dir examples/surfaces] [--bare DIR]
//! ```
//! `12-lit-comp.frame` (`viz.comp_scope`), `12-lit-delay.frame`
//! (`viz.delay_taps`), `12-lit-reverb.frame` (`viz.reverb_space`),
//! `12-lit-mod.frame` (`viz.mod_lfo`) and `12-lit-eq-glow.frame` (the
//! `viz.eq_glow` layer under an EQ response curve), each with knobs and a
//! `lit-*.catalog.json` beside it, so the desktop player's mock engine
//! serves their params and feeds:
//!
//! ```text
//! nix develop . -c cargo run -p frame-desktop -- examples/surfaces/12-lit-delay.frame
//! ```
//!
//! `--bare DIR` also writes each component alone, at its default size
//! (`bare-<kind>.frame` + catalog) — what the side-by-side comparison with
//! processor's own renders uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use frame::document::proto::{Node, NodeId, NodeKind, Scene, Size, TextAlign, TextData, Transform};
use frame::surface::kit::{Place, draw, viz};
use frame::surface::proto::{
    Axis, Binding, Catalog, Control, ControlKind, CurveInput, FeedInfo, ParamInfo, PlotRange,
    PlotStyle, SlotRole,
};
use frame::surface::runtime::Surface;
use frame::surface::widgets::{Builder, Theme};

const BG: &str = "#0a0c10";
const INK: &str = "#e9ecef";
const DIM: &str = "#8e98a8";
const KNOB: &str = "knob.soft_pointer";

/// One example: file stem, the surface, its catalog, the catalog's file.
pub struct Example {
    pub file: &'static str,
    pub catalog_file: &'static str,
    pub surface: Surface,
    pub catalog: Catalog,
}

fn main() {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/surfaces");
    let mut bare: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out-dir" => dir = args.next().map(PathBuf::from).unwrap_or(dir),
            "--bare" => bare = args.next().map(PathBuf::from),
            other => {
                eprintln!(
                    "make_lit_viz_examples: unknown argument {other}\nusage: make_lit_viz_examples [--out-dir DIR] [--bare DIR]"
                );
                std::process::exit(2);
            }
        }
    }
    for ex in examples() {
        write(&dir, &ex);
    }
    if let Some(bare_dir) = bare {
        let _ = std::fs::create_dir_all(&bare_dir);
        for ex in bare_examples() {
            write(&bare_dir, &ex);
        }
    }
}

fn write(dir: &Path, ex: &Example) {
    let fm = dir.join(ex.file);
    let cat = dir.join(ex.catalog_file);
    let text = frame::document::text::print(&ex.surface.to_scene());
    let json = serde_json::to_string_pretty(&ex.catalog).expect("catalogs serialize");
    if let Err(e) = std::fs::write(&fm, text).and_then(|()| std::fs::write(&cat, json + "\n")) {
        eprintln!("make_lit_viz_examples: writing {}: {e}", fm.display());
        std::process::exit(1);
    }
    println!(
        "{{\"ok\":true,\"surface\":\"{}\",\"catalog\":\"{}\",\"controls\":{}}}",
        fm.display(),
        cat.display(),
        ex.surface.controls.len()
    );
}

#[must_use]
pub fn examples() -> Vec<Example> {
    vec![comp(), delay(), reverb(), modulation(), eq_glow()]
}

/// Every lit component alone at its default size, on its own ground.
#[must_use]
pub fn bare_examples() -> Vec<Example> {
    let (comp_params, comp_feeds) = viz::lit_catalog::comp_scope("comp", 120);
    let (eq_params, eq_feeds) = viz::lit_catalog::eq_glow("eq", 6, 128);
    let mut graph_params = viz::catalog::eq_params("eq", 24);
    graph_params.extend(viz::catalog::eq_view_params("eq"));
    [
        (
            "eqgraph",
            "viz.eq_graph",
            Place::new("EQ")
                .size(820.0)
                .options(&["ns=eq", "h=340", "f_max=24000"]),
            graph_params,
            viz::catalog::eq_streams("eq", 128),
        ),
        (
            "comp",
            "viz.comp_scope",
            Place::new("Comp")
                .size(620.0)
                .options(&["ns=comp", "len=120", "h=520"]),
            comp_params,
            comp_feeds,
        ),
        (
            "delay",
            "viz.delay_taps",
            Place::new("Delay")
                .size(760.0)
                .options(&["ns=delay", "h=190"]),
            viz::lit_catalog::delay_params("delay"),
            Vec::new(),
        ),
        (
            "reverb",
            "viz.reverb_space",
            Place::new("Reverb")
                .size(760.0)
                .options(&["ns=reverb", "h=190"]),
            viz::lit_catalog::reverb_params("reverb"),
            Vec::new(),
        ),
        (
            "mod",
            "viz.mod_lfo",
            Place::new("Mod").size(760.0).options(&["ns=mod", "h=220"]),
            viz::lit_catalog::mod_params("mod"),
            Vec::new(),
        ),
        (
            "glow",
            "viz.eq_glow",
            Place::new("Glow")
                .size(800.0)
                .options(&["ns=eq", "bands=6", "h=336", "ground"]),
            eq_params,
            eq_feeds,
        ),
    ]
    .into_iter()
    .map(|(kind, id, place, params, feeds)| {
        let mut scene = Scene::new(kind);
        let page = scene.pages()[0];
        let mut controls = BTreeMap::new();
        let theme = Theme::stage_dark();
        let mut b = Builder {
            scene: &mut scene,
            controls: &mut controls,
            theme: &theme,
        };
        frame::surface::kit::place(&mut b, page, id, &place).unwrap_or_else(|e| panic!("{id}: {e}"));
        let file: &'static str = Box::leak(format!("bare-{kind}.frame").into_boxed_str());
        let catalog_file: &'static str =
            Box::leak(format!("bare-{kind}.catalog.json").into_boxed_str());
        Example {
            file,
            catalog_file,
            surface: Surface { scene, controls },
            catalog: catalog(id, params, feeds),
        }
    })
    .collect()
}

/// A new surface: a page with a dark root frame `w` wide, titled.
struct Doc {
    scene: Scene,
    controls: BTreeMap<NodeId, Control>,
    root: NodeId,
}

impl Doc {
    fn new(name: &str, w: f64, title: &str, subtitle: &str) -> Self {
        let mut scene = Scene::new(name);
        let page = scene.pages()[0];
        let mut root = draw::frame(name, 0.0, 0.0, w, 100.0);
        root.fills = vec![draw::solid(BG)];
        let root_id = root.id;
        scene
            .apply(frame::document::proto::Op::Insert {
                node: root,
                parent: page,
                index: 0,
            })
            .expect("page exists");
        let mut d = Self {
            scene,
            controls: BTreeMap::new(),
            root: root_id,
        };
        let mut t = draw::text(
            "title",
            title,
            24.0,
            16.0,
            w - 48.0,
            13.0,
            INK,
            TextAlign::Left,
        );
        if let NodeKind::Text(TextData {
            font_weight,
            letter_spacing,
            ..
        }) = &mut t.kind
        {
            *font_weight = Some(700.0);
            *letter_spacing = 2.0;
        }
        d.add(root_id, t);
        d.add(
            root_id,
            draw::text(
                "subtitle",
                subtitle,
                24.0,
                34.0,
                w - 48.0,
                10.0,
                DIM,
                TextAlign::Left,
            ),
        );
        d
    }

    fn add(&mut self, parent: NodeId, node: Node) -> NodeId {
        let theme = Theme::stage_dark();
        Builder {
            scene: &mut self.scene,
            controls: &mut self.controls,
            theme: &theme,
        }
        .add(parent, node)
    }

    /// Place kit component `id` at (`x`, `y`) of the root.
    fn place(&mut self, id: &str, (x, y): (f64, f64), p: &Place) -> NodeId {
        let theme = Theme::stage_dark();
        let mut b = Builder {
            scene: &mut self.scene,
            controls: &mut self.controls,
            theme: &theme,
        };
        let node =
            frame::surface::kit::place(&mut b, self.root, id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
        self.scene.node_mut(node).expect("placed").transform = Transform::translate(x, y);
        node
    }

    /// A row of knobs centred in `(x, w)` at `y`, one per `(legend, addr)`.
    fn knobs(&mut self, (x, w): (f64, f64), y: f64, knobs: &[(&str, &str)]) {
        let theme = Theme::stage_dark();
        let cell = w / knobs.len() as f64;
        for (i, (legend, addr)) in knobs.iter().enumerate() {
            let mut b = Builder {
                scene: &mut self.scene,
                controls: &mut self.controls,
                theme: &theme,
            };
            let p = Place::new(*legend)
                .size(46.0)
                .bind(Binding::param(*addr))
                .legend(*legend);
            let knob =
                frame::surface::kit::place_in_cell(&mut b, self.root, KNOB, &p).expect("knob places");
            let holder = self
                .scene
                .node(knob)
                .and_then(|n| n.parent)
                .expect("in its cell");
            let hw = self.scene.node(holder).map_or(0.0, |n| n.size.width);
            self.scene.node_mut(holder).expect("cell").transform =
                Transform::translate(x + cell * (i as f64 + 0.5) - hw / 2.0, y);
        }
    }

    /// Size the root to what it holds, plus a margin.
    fn finish(mut self, margin: f64) -> Surface {
        let (_, _, _, h) = frame::surface::kit::drawn_bounds(&self.scene, self.root);
        if let Some(n) = self.scene.node_mut(self.root) {
            n.size = Size {
                width: n.size.width,
                height: (h + margin).ceil(),
            };
        }
        Surface {
            scene: self.scene,
            controls: self.controls,
        }
    }
}

fn catalog(id: &str, params: Vec<ParamInfo>, streams: Vec<FeedInfo>) -> Catalog {
    Catalog {
        id: id.into(),
        version: 1,
        params,
        actions: Vec::new(),
        streams,
        collections: Vec::new(),
    }
}

fn comp() -> Example {
    let mut d = Doc::new(
        "Lit Compressor",
        660.0,
        "COMPRESSOR",
        "viz.comp_scope — processor's compressor panel: input, gain reduction, transfer curve; drag the threshold (wheel: ratio)",
    );
    d.place(
        "viz.comp_scope",
        (20.0, 54.0),
        &Place::new("Compressor")
            .size(620.0)
            .options(&["ns=comp", "h=360", "len=128"]),
    );
    d.knobs(
        (20.0, 620.0),
        432.0,
        &[
            ("Threshold", "comp/threshold"),
            ("Ratio", "comp/ratio"),
            ("Knee", "comp/knee"),
        ],
    );
    let (params, feeds) = viz::lit_catalog::comp_scope("comp", 128);
    Example {
        file: "12-lit-comp.frame",
        catalog_file: "lit-comp.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog("viz.comp_scope", params, feeds),
    }
}

fn delay() -> Example {
    let mut d = Doc::new(
        "Lit Delay",
        800.0,
        "DELAY",
        "viz.delay_taps — processor's delay lane: repeats up (left) and down (right), the machine's marks, the sweep",
    );
    d.place(
        "viz.delay_taps",
        (20.0, 54.0),
        &Place::new("Delay")
            .size(760.0)
            .options(&["ns=delay", "h=190"]),
    );
    d.knobs(
        (20.0, 760.0),
        262.0,
        &[
            ("Time", "delay/time"),
            ("Feedback", "delay/feedback"),
            ("Left", "delay/tap_div_l"),
            ("Right", "delay/tap_div_r"),
            ("Machine", "delay/family"),
            ("Tempo", "delay/bpm"),
        ],
    );
    Example {
        file: "12-lit-delay.frame",
        catalog_file: "lit-delay.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog(
            "viz.delay_taps",
            viz::lit_catalog::delay_params("delay"),
            Vec::new(),
        ),
    }
}

fn reverb() -> Example {
    let mut d = Doc::new(
        "Lit Reverb",
        800.0,
        "REVERB",
        "viz.reverb_space — processor's reverb lane: pre-delay, early reflections, the tail and its damped core",
    );
    d.place(
        "viz.reverb_space",
        (20.0, 54.0),
        &Place::new("Reverb")
            .size(760.0)
            .options(&["ns=reverb", "h=190"]),
    );
    d.knobs(
        (20.0, 760.0),
        262.0,
        &[
            ("Decay", "reverb/decay"),
            ("Pre-delay", "reverb/predelay"),
            ("Density", "reverb/density"),
            ("Damping", "reverb/damp"),
            ("Space", "reverb/family"),
            ("Tempo", "reverb/bpm"),
        ],
    );
    Example {
        file: "12-lit-reverb.frame",
        catalog_file: "lit-reverb.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog(
            "viz.reverb_space",
            viz::lit_catalog::reverb_params("reverb"),
            Vec::new(),
        ),
    }
}

fn modulation() -> Example {
    let mut d = Doc::new(
        "Lit Modulation",
        800.0,
        "MODULATION",
        "viz.mod_lfo — processor's modulation panel: a picture per engine, moving at the LFO rate",
    );
    d.place(
        "viz.mod_lfo",
        (20.0, 54.0),
        &Place::new("Modulation")
            .size(760.0)
            .options(&["ns=mod", "h=220"]),
    );
    d.knobs(
        (20.0, 760.0),
        292.0,
        &[
            ("Rate", "mod/rate"),
            ("Depth", "mod/depth"),
            ("Mix", "mod/mix"),
            ("Engine", "mod/engine"),
        ],
    );
    Example {
        file: "12-lit-mod.frame",
        catalog_file: "lit-mod.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog(
            "viz.mod_lfo",
            viz::lit_catalog::mod_params("mod"),
            Vec::new(),
        ),
    }
}

/// The glow layer, with the EQ's combined response drawn over it.
fn eq_glow() -> Example {
    const BANDS: usize = 6;
    let (w, h) = (800.0, 336.0);
    let mut d = Doc::new(
        "Lit EQ",
        840.0,
        "EQ GLOW",
        "viz.eq_glow — processor's EQ light: the analyser as a field, a bloom under each band; the response curve drawn over it",
    );
    let glow = d.place(
        "viz.eq_glow",
        (20.0, 54.0),
        &Place::new("Glow")
            .size(w)
            .options(&["ns=eq", "bands=6", "h=336", "ground"]),
    );
    // The response over the light, as a graph would draw it.
    let inputs: Vec<CurveInput> = (1..=BANDS)
        .flat_map(|n| {
            ["freq", "gain", "q", "shape", "on"]
                .map(|k| CurveInput::param(format!("b{n}_{k}"), format!("eq/b{n}_{k}")))
        })
        .collect();
    let frame = d.add(glow, draw::frame("response", 0.0, 0.0, w, h));
    let mut line = Node::new(NodeKind::Vector, "line");
    line.size = Size {
        width: w,
        height: h,
    };
    line.stroke = frame::document::proto::Stroke {
        paints: vec![draw::solid("#ffffffe6")],
        weight: 2.0,
        join: frame::document::proto::StrokeJoin::Round,
        align: frame::document::proto::StrokeAlign::Center,
        ..frame::document::proto::Stroke::default()
    };
    let line = d.add(frame, line);
    let kind = ControlKind::Curve {
        provider: "eq.response".into(),
        inputs,
        series: Some("sum".into()),
        style: PlotStyle::Line,
        x: Axis::Log {
            f_min: 20.0,
            f_max: 20_000.0,
        },
        y: PlotRange::linear(-24.0, 24.0),
    };
    d.controls
        .insert(frame, Control::new(kind).slot(SlotRole::Trace, line.0));
    let knobs: Vec<(String, String)> = (1..=BANDS)
        .map(|n| (format!("Band {n}"), format!("eq/b{n}_gain")))
        .collect();
    let refs: Vec<(&str, &str)> = knobs
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    d.knobs((20.0, w), 54.0 + h + 18.0, &refs);
    let (params, feeds) = viz::lit_catalog::eq_glow("eq", BANDS, 128);
    Example {
        file: "12-lit-eq-glow.frame",
        catalog_file: "lit-eq-glow.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog("viz.eq_glow", params, feeds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/surfaces")
    }

    #[test]
    fn examples_round_trip_through_markup_and_lint_clean() {
        for ex in examples().into_iter().chain(bare_examples()) {
            let scene = ex.surface.to_scene();
            let text = frame::document::text::print(&scene);
            let back = frame::document::text::parse(&text, scene.assets.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", ex.file));
            assert_eq!(back, scene, "{}: parse(print(s)) == s", ex.file);
            let issues = frame::design::lint::lint(&scene, &ex.catalog, &frame::design::lint::Config::default());
            let errors: Vec<String> = issues
                .iter()
                .filter(|i| i.severity == frame::design::lint::Severity::Error)
                .map(ToString::to_string)
                .collect();
            assert!(errors.is_empty(), "{}:\n{}", ex.file, errors.join("\n"));
        }
    }

    /// One bare component rendered at `t` s: `(pixels, floats uploaded)`.
    fn render_bare(
        o: &mut frame::render::offscreen::Offscreen,
        ex: &Example,
        mode: frame::render::fx::Mode,
        t: f64,
    ) -> (frame::render::offscreen::Pixels, usize) {
        let mut rt = frame::surface::runtime::Runtime::new(ex.surface.clone(), ex.catalog.clone());
        for f in frame::feed::memory::frames(&ex.catalog.streams, t) {
            rt.apply_frame(f, 0.0);
        }
        rt.set_clock(t);
        let frame = rt.frame(0.0);
        let page = frame.pages()[0];
        let root = frame
            .node(page)
            .and_then(|p| p.children.first().copied())
            .expect("a root");
        let size = frame.node(root).expect("root").size;
        let values: std::collections::HashMap<NodeId, f64> = rt
            .surface()
            .controls
            .iter()
            .map(|(id, c)| (*id, rt.position(*id, c)))
            .collect();
        let inputs = rt.shader_inputs();
        let floats = inputs
            .values()
            .flat_map(BTreeMap::values)
            .map(Vec::len)
            .sum();
        let px = o
            .render_with_effects(&frame::render::fx::Frame {
                mode,
                time_s: t,
                values: Some(&values),
                inputs: Some(&inputs),
                base: peniko::Color::from_rgba8(8, 8, 8, 255),
                ..frame::render::fx::Frame::new(
                    &frame,
                    page,
                    size.width.ceil() as u32,
                    size.height.ceil() as u32,
                )
            })
            .expect("render");
        (px, floats)
    }

    /// On a GPU the shader draws the picture — and it is not the fallback.
    #[test]
    fn lit_components_light_up_on_the_gpu() {
        let Ok(mut o) = frame::render::offscreen::Offscreen::new() else {
            eprintln!("skipping: no GPU adapter");
            return;
        };
        for ex in bare_examples() {
            let (gpu, _) = render_bare(&mut o, &ex, frame::render::fx::Mode::Gpu, 1.3);
            let (off, _) = render_bare(&mut o, &ex, frame::render::fx::Mode::Off, 1.3);
            let lit = gpu
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0].max(p[1]).max(p[2]) > 40)
                .count();
            let differ = gpu
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .zip(off.rgba.as_chunks::<4>().0)
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
                .count();
            let n = gpu.rgba.len() / 4;
            assert!(
                lit > n / 200,
                "{}: the shader lit only {lit} of {n} px",
                ex.file
            );
            assert!(
                differ > n / 20,
                "{}: GPU and fallback differ in only {differ} of {n} px",
                ex.file
            );
        }
    }

    /// What a lit frame costs: the runtime's frame + inputs, the upload, and
    /// the effects stage's wall time (render + readback) with the shader on
    /// and off. `cargo test --release -p frame-cli --bin make_lit_viz_examples -- --ignored --nocapture lit_frame_cost`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn lit_frame_cost() {
        let Ok(mut o) = frame::render::offscreen::Offscreen::new() else {
            return;
        };
        for ex in bare_examples() {
            let n = 60;
            let mut rt = frame::surface::runtime::Runtime::new(ex.surface.clone(), ex.catalog.clone());
            let started = std::time::Instant::now();
            for i in 0..n {
                let t = f64::from(i) / 60.0;
                for f in frame::feed::memory::frames(&ex.catalog.streams, t) {
                    rt.apply_frame(f, t * 1000.0);
                }
                rt.set_clock(t);
                let _ = rt.frame(t * 1000.0);
                let _ = rt.shader_inputs();
            }
            let runtime_ms = started.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
            let mut time = |mode| {
                let _ = render_bare(&mut o, &ex, mode, 0.0);
                let started = std::time::Instant::now();
                let mut floats = 0;
                for i in 0..n {
                    floats = render_bare(&mut o, &ex, mode, f64::from(i) / 60.0).1;
                }
                (
                    started.elapsed().as_secs_f64() * 1000.0 / f64::from(n),
                    floats,
                )
            };
            let (gpu_ms, floats) = time(frame::render::fx::Mode::Gpu);
            let (off_ms, _) = time(frame::render::fx::Mode::Off);
            println!(
                "{}: runtime {runtime_ms:.3} ms/frame; upload {} B/frame; effects stage {gpu_ms:.2} ms/frame on, {off_ms:.2} off (incl. setup + readback)",
                ex.file,
                floats * 4 + 288
            );
        }
    }

    /// The checked-in files parse, and every control and shader input binds
    /// to their catalog.
    #[test]
    fn checked_in_examples_play_against_their_catalogs() {
        for ex in examples() {
            let text = std::fs::read_to_string(dir().join(ex.file))
                .unwrap_or_else(|e| panic!("{}: {e} (run make_lit_viz_examples)", ex.file));
            let json = std::fs::read_to_string(dir().join(ex.catalog_file)).unwrap();
            let catalog: Catalog = serde_json::from_str(&json).unwrap();
            let scene = frame::document::text::parse(&text, BTreeMap::new()).unwrap();
            let surface = Surface::from_scene(scene);
            assert_eq!(
                surface.controls.len(),
                ex.surface.controls.len(),
                "{} is stale: run make_lit_viz_examples",
                ex.file
            );
            for (id, c) in &surface.controls {
                let issues = c.validate(&catalog);
                assert!(
                    issues.is_empty(),
                    "{} {}: {issues:?}",
                    ex.file,
                    surface.scene.node(*id).map_or("?", |n| n.name.as_str())
                );
            }
            // Every shader input resolves once the mock engine has sent a frame.
            let mut rt = frame::surface::runtime::Runtime::new(surface, catalog.clone());
            for f in frame::feed::memory::frames(&catalog.streams, 1.0) {
                rt.apply_frame(f, 0.0);
            }
            rt.set_clock(1.0);
            let inputs = rt.shader_inputs();
            let shaders: Vec<_> = rt
                .surface()
                .scene
                .nodes
                .values()
                .filter_map(|n| n.shader.as_ref().map(|s| (n.id, s)))
                .filter(|(_, s)| !s.inputs.is_empty())
                .collect();
            assert!(!shaders.is_empty(), "{}: no lit shader", ex.file);
            for (id, fx) in shaders {
                let got = &inputs[&id];
                for i in &fx.inputs {
                    assert!(
                        got.contains_key(&i.name),
                        "{}: input `{}` has no data",
                        ex.file,
                        i.name
                    );
                }
                let floats: usize = got.values().map(Vec::len).sum();
                assert!(floats * 4 <= 4096, "{}: {floats} floats a frame", ex.file);
            }
        }
    }
}
