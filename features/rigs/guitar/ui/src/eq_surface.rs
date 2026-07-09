//! The full FTS-EQ surface, detached — the Pro-Q editor over the wire.
//!
//! Everything that *is* the EQ comes from `eq-ui`'s portable core: the band
//! model ([`EqBand`], all ten shapes), the response math, the SVG curve/grid
//! generators, and the interaction rules (hit-testing, shape-from-position,
//! shape-aware gain drags, Q wheel steps). This module only re-hosts them in
//! plain Dioxus SVG and swaps the nice-plug `ParamPtr` writes for
//! `set_block_param` over the vox link — same graph, any transport, wasm
//! included. The vello painter remains the native plugin editor.
//!
//! Wire scheme: 24 bands × `b{i}_{used,on,freq,gain,q,shape}` on the "Pre EQ"
//! block (shape = `EqBandShape` ordinal).

use dioxus::prelude::*;

use eq_ui::cheatsheet::GUITAR_ELECTRIC;
use eq_ui::eq_graph_interaction::{
    GraphMapper, drag_gain_for_shape, filter_type_for_position, nearest_band, wheel_q_for_shape,
};
use eq_ui::eq_graph_model::{EqBand, EqBandShape, freq_to_color};
use eq_ui::eq_graph_svg::{generate_all_eq_curves, generate_freq_labels, generate_grid_elements};

use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::rig::RigClient;

const NUM_BANDS: usize = 24;
const H: f64 = 270.0; // viewBox height; width follows the panel's aspect
const MIN_FREQ: f64 = 10.0; // "0 Hz" — log axis floor
const MAX_FREQ: f64 = 22_050.0;

/// Vertical resolution options (± dB) — a view feature, default ±6.
const DB_RANGES: [f64; 3] = [3.0, 6.0, 12.0];
const SAMPLE_RATE: f64 = 48000.0;
const HIT_RADIUS: f64 = 16.0;

/// The electric-guitar "magic frequency" zone containing `freq`, if any —
/// its role names the band node ("body / thickness", "honk", …).
fn magic_label(freq: f32) -> Option<&'static str> {
    GUITAR_ELECTRIC
        .zones
        .iter()
        .find(|z| freq >= z.lo_hz && freq <= z.hi_hz)
        .map(|z| z.role)
}

fn shape_index(s: EqBandShape) -> f32 {
    EqBandShape::all().iter().position(|x| *x == s).unwrap_or(0) as f32
}

fn shape_from_index(i: usize) -> EqBandShape {
    EqBandShape::all().get(i).copied().unwrap_or_default()
}

/// Decode the wire params into the eq-ui band model.
fn bands_of(block: &LiveBlock) -> Vec<EqBand> {
    let get = |name: &str, dflt: f32| -> f32 {
        block
            .params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value)
            .unwrap_or(dflt)
    };
    (0..NUM_BANDS)
        .map(|i| {
            let b = i + 1;
            EqBand {
                index: i,
                used: get(&format!("b{b}_used"), 0.0) >= 0.5,
                enabled: get(&format!("b{b}_on"), 0.0) >= 0.5,
                frequency: get(&format!("b{b}_freq"), 1000.0),
                gain: get(&format!("b{b}_gain"), 0.0),
                q: get(&format!("b{b}_q"), 0.707),
                shape: shape_from_index(get(&format!("b{b}_shape"), 0.0) as usize),
                solo: false,
                stereo_mode: Default::default(),
                name: String::new(),
            }
        })
        .collect()
}

/// Fire a band-param write over the rig service.
fn send(rig: &Option<RigClient>, block_id: &str, band: usize, field: &str, value: f32) {
    if let Some(r) = rig.clone() {
        let id = block_id.to_string();
        let name = format!("b{}_{}", band + 1, field);
        spawn(async move {
            let _ = r.set_block_param(id, name, value).await;
        });
    }
}

/// The detached Pro-Q surface. Drag nodes (shape-aware gain), wheel for Q,
/// double-click to add a band (shape inferred from position), band rail for
/// shape/enable/delete on the selection.
#[component]
pub fn EqProSurface(block: LiveBlock, spectrum: Vec<f32>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut selected = use_signal(|| None::<usize>);
    let mut dragging = use_signal(|| None::<usize>);
    // Svg client rect (origin_x, origin_y, w, h), cached at drag start so
    // the shield can map global pointer coords into graph space.
    let mut drag_rect = use_signal(|| None::<(f64, f64, f64, f64)>);
    let mut svg_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    // View vertical resolution: ±3 / ±6 / ±12 dB (default ±6).
    let mut range_db = use_signal(|| 6.0f64);
    // The viewBox width follows the panel's real aspect so drawing units
    // stay square (nodes are circles, no stretch) while the graph fills
    // every horizontal pixel: 10 Hz at the far left, 22 kHz at the right.
    let mut vb_w = use_signal(|| 480.0f64);

    let bands = bands_of(&block);
    let db_range = range_db();
    let w = vb_w();
    let mapper = GraphMapper::new(MIN_FREQ, MAX_FREQ, db_range, w, H, 0.0);

    // Curves + grid + labels — straight from eq-ui's generators.
    let curves = generate_all_eq_curves(
        &bands,
        SAMPLE_RATE,
        MIN_FREQ,
        MAX_FREQ,
        db_range,
        0.0,
        w,
        H,
        128,
    );
    let grid = generate_grid_elements(0.0, w, H, MIN_FREQ, MAX_FREQ, db_range);
    let freq_labels = generate_freq_labels(0.0, w, H, MIN_FREQ, MAX_FREQ);

    // Input spectrum fill (20 Hz–20 kHz log bins → mapper space).
    let spec_poly = if spectrum.is_empty() {
        String::new()
    } else {
        let n = spectrum.len();
        let mut pts = format!("{:.1},{H} ", mapper.freq_to_x(20.0));
        for (i, db) in spectrum.iter().enumerate() {
            let f = 20.0 * (1000.0f64).powf(i as f64 / (n - 1).max(1) as f64);
            let x = mapper.freq_to_x(f);
            let y = H - ((*db + 90.0) as f64 / 90.0 * H).clamp(0.0, H);
            pts.push_str(&format!("{x:.1},{y:.1} "));
        }
        pts.push_str(&format!("{:.1},{H}", mapper.freq_to_x(20000.0)));
        pts
    };

    // Pointer → graph coordinates (svg is scaled; measure the element).
    let gw = w;
    let to_graph = move |coords: dioxus::html::geometry::ElementPoint,
                         el: Option<std::rc::Rc<MountedData>>|
          -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<(f64, f64)>>>> {
        Box::pin(async move {
            let el = el?;
            let rect = el.get_client_rect().await.ok()?;
            Some((coords.x / rect.width() * gw, coords.y / rect.height() * H))
        })
    };

    let sel = selected();
    let sel_band = sel.and_then(|i| bands.get(i).cloned()).filter(|b| b.used);
    let block_id = block.id.clone();
    let bands_for_move = bands.clone();
    let bands_for_down = bands.clone();
    let bands_for_wheel = bands.clone();

    rsx! {
        div { class: "relative flex flex-col h-full w-full min-h-0",
            svg {
                class: "w-full flex-1 min-h-0 touch-none select-none",
                view_box: "0 0 {w:.0} 270",
                preserve_aspect_ratio: "none",
                onmounted: move |e| {
                    let data = e.data();
                    svg_el.set(Some(data.clone()));
                    spawn(async move {
                        if let Ok(rect) = data.get_client_rect().await {
                            if rect.height() > 1.0 {
                                vb_w.set((H * rect.width() / rect.height()).max(120.0));
                            }
                        }
                    });
                },

                // Add a band where the user double-clicks — shape inferred
                // from position (eq-ui's rule).
                ondoubleclick: {
                    let rig = rig.clone();
                    let block_id = block.id.clone();
                    let bands = bands.clone();
                    move |e: MouseEvent| {
                        let free = bands.iter().position(|b| !b.used);
                        let (rig, block_id) = (rig.clone(), block_id.clone());
                        let coords = e.element_coordinates();
                        let el = svg_el();
                        spawn(async move {
                            let Some((x, y)) = to_graph(coords, el).await else { return };
                            let Some(i) = free else { return };
                            let freq = mapper.x_to_freq(x).clamp(MIN_FREQ, MAX_FREQ);
                            let gain = mapper.y_to_db(y);
                            let shape = filter_type_for_position(freq, gain, db_range);
                            if let Some(r) = rig {
                                let id = |f: &str| (block_id.clone(), format!("b{}_{f}", i + 1));
                                let (bid, n) = id("freq");
                                let _ = r.set_block_param(bid, n, freq as f32).await;
                                let (bid, n) = id("gain");
                                let _ = r.set_block_param(bid, n, gain as f32).await;
                                let (bid, n) = id("shape");
                                let _ = r.set_block_param(bid, n, shape_index(shape)).await;
                                let (bid, n) = id("on");
                                let _ = r.set_block_param(bid, n, 1.0).await;
                                let (bid, n) = id("used");
                                let _ = r.set_block_param(bid, n, 1.0).await;
                            }
                        });
                        // Select it optimistically.
                        if let Some(i) = free {
                            selected.set(Some(i));
                        }
                    }
                },
                onpointerdown: {
                    move |e: PointerEvent| {
                        let coords = e.element_coordinates();
                        let el = svg_el();
                        let bands = bands_for_down.clone();
                        spawn(async move {
                            let Some(el2) = el.clone() else { return };
                            let Some((x, y)) = to_graph(coords, el).await else { return };
                            if let Ok(rect) = el2.get_client_rect().await {
                                drag_rect.set(Some((
                                    rect.origin.x,
                                    rect.origin.y,
                                    rect.width(),
                                    rect.height(),
                                )));
                            }
                            match nearest_band(&bands, mapper, x, y, HIT_RADIUS) {
                                Some((i, _)) => {
                                    selected.set(Some(i));
                                    dragging.set(Some(i));
                                }
                                None => selected.set(None),
                            }
                        });
                    }
                },

                // Q on the wheel — shape-aware steps (slope for cuts).
                onwheel: {
                    let rig = rig.clone();
                    let block_id = block.id.clone();
                    move |e: WheelEvent| {
                        let target = dragging().or(selected());
                        let Some(i) = target else { return };
                        let Some(b) = bands_for_wheel.get(i) else { return };
                        if !b.used {
                            return;
                        }
                        let q = wheel_q_for_shape(
                            b.shape,
                            b.q,
                            e.delta().strip_units().y,
                            false,
                        );
                        send(&rig, &block_id, i, "q", q);
                    }
                },

                // Spectrum behind everything.
                if !spec_poly.is_empty() {
                    polygon { points: "{spec_poly}", fill: "#7dd3fc14", stroke: "#7dd3fc38", stroke_width: "1" }
                }
                // Grid.
                for (i, (x1, y1, x2, y2, major)) in grid.iter().enumerate() {
                    line {
                        key: "g{i}",
                        x1: "{x1:.1}", y1: "{y1:.1}", x2: "{x2:.1}", y2: "{y2:.1}",
                        stroke: if *major { "#3f3f46" } else { "#27272a" },
                        stroke_width: "1",
                    }
                }
                for (i, (x, _y, label)) in freq_labels.iter().enumerate() {
                    text {
                        key: "f{i}",
                        x: "{x:.0}", y: "{H - 5.0}",
                        fill: "#52525b", font_size: "9", text_anchor: "middle",
                        "{label}"
                    }
                }
                // Per-band curves (selected band gets its fill).
                for (bi, stroke, fill) in curves.band_curves.iter() {
                    {
                        let color = freq_to_color(bands.get(*bi).map(|b| b.frequency as f64).unwrap_or(1000.0));
                        let is_sel = sel == Some(*bi);
                        rsx! {
                            if is_sel {
                                path { key: "bf{bi}", d: "{fill}", fill: "{color}22", stroke: "none" }
                            }
                            path {
                                key: "bs{bi}",
                                d: "{stroke}",
                                fill: "none",
                                stroke: "{color}",
                                stroke_width: if is_sel { "1.5" } else { "1" },
                                opacity: if is_sel { "0.9" } else { "0.45" },
                            }
                        }
                    }
                }
                // Combined response.
                path { d: "{curves.combined_fill}", fill: "#fafafa10", stroke: "none" }
                path { d: "{curves.combined_stroke}", fill: "none", stroke: "#fafafa", stroke_width: "2" }
                // Band nodes.
                for b in bands.iter().filter(|b| b.used) {
                    {
                        let color = freq_to_color(b.frequency as f64);
                        let cx = mapper.freq_to_x(b.frequency as f64);
                        let cy = mapper.db_to_y(if b.shape.uses_gain() { b.gain as f64 } else { 0.0 });
                        let is_sel = sel == Some(b.index);
                        rsx! {
                            circle {
                                key: "n{b.index}",
                                cx: "{cx:.1}", cy: "{cy:.1}",
                                r: if is_sel { "9" } else { "7" },
                                fill: if b.enabled { "{color}" } else { "#3f3f46" },
                                fill_opacity: "0.85",
                                stroke: if is_sel { "#ffffff" } else { "{color}" },
                                stroke_width: if is_sel { "2" } else { "1" },
                                class: "cursor-grab",
                            }
                            text {
                                key: "t{b.index}",
                                x: "{cx:.1}", y: "{cy + 3.0:.1}",
                                fill: "#09090b", font_size: "9", font_weight: "700",
                                text_anchor: "middle", pointer_events: "none",
                                "{b.index + 1}"
                            }
                            // Magic-frequency zone label above the node.
                            if let Some(role) = magic_label(b.frequency) {
                                // Fixed top lane — labels never collide with
                                // the curve; a hairline ties label to node.
                                text {
                                    key: "l{b.index}",
                                    x: "{cx:.1}",
                                    y: "10",
                                    fill: "{color}",
                                    fill_opacity: "0.7",
                                    font_size: "7",
                                    text_anchor: "middle",
                                    pointer_events: "none",
                                    "{role}"
                                }
                                line {
                                    key: "ll{b.index}",
                                    x1: "{cx:.1}",
                                    y1: "13",
                                    x2: "{cx:.1}",
                                    y2: "{(cy - 8.0).max(14.0):.1}",
                                    stroke: "{color}",
                                    stroke_opacity: "0.15",
                                    stroke_width: "0.5",
                                    pointer_events: "none",
                                }
                            }
                        }
                    }
                }
            }

            // Drag shield: node drags keep tracking outside the graph.
            if dragging().is_some() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: grabbing;",
                    onpointermove: {
                        let rig = rig.clone();
                        let block_id = block.id.clone();
                        move |e: PointerEvent| {
                            let Some(i) = dragging() else { return };
                            let Some((ox, oy, w_px, h_px)) = drag_rect() else { return };
                            let shape = bands_for_move.get(i).map(|b| (b.shape, b.gain));
                            let c = e.client_coordinates();
                            let x = (c.x - ox) / w_px * gw;
                            let y = (c.y - oy) / h_px * H;
                            let (rig, block_id) = (rig.clone(), block_id.clone());
                            spawn(async move {
                                let Some((shape, cur_gain)) = shape else { return };
                                let freq = mapper.x_to_freq(x).clamp(MIN_FREQ, MAX_FREQ) as f32;
                                let gain =
                                    drag_gain_for_shape(shape, cur_gain, mapper.y_to_db(y));
                                if let Some(r) = rig {
                                    let n = format!("b{}_freq", i + 1);
                                    let _ = r.set_block_param(block_id.clone(), n, freq).await;
                                    let n = format!("b{}_gain", i + 1);
                                    let _ = r.set_block_param(block_id, n, gain).await;
                                }
                            });
                        }
                    },
                    onpointerup: move |_| dragging.set(None),
                }
            }

            // Vertical-resolution selector (±3 / ±6 / ±12 dB), top-left.
            div {
                class: "absolute top-1 left-1 z-10 flex gap-0.5",
                for r in DB_RANGES {
                    button {
                        class: if range_db() == r {
                            "rounded-sm px-1 py-0 text-[8px] font-bold bg-accent text-accent-foreground"
                        } else {
                            "rounded-sm px-1 py-0 text-[8px] text-muted-foreground/70 hover:text-foreground border border-border/50"
                        },
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            range_db.set(r);
                        },
                        {format!("±{}", r as u32)}
                    }
                }
            }

            // Selection rail overlays the graph bottom, only while a band
            // is selected — the graph gets the full panel otherwise.
            if sel_band.is_some() {
                div {
                    class: "absolute bottom-0 inset-x-0 z-10 flex items-center gap-2 px-2 py-1",
                    style: "background: linear-gradient(to top, rgba(8,8,8,0.92), transparent);",
                if let Some(b) = sel_band {
                    {
                        let i = b.index;
                        let color = freq_to_color(b.frequency as f64);
                        let rig1 = rig.clone();
                        let rig2 = rig.clone();
                        let rig3 = rig.clone();
                        let rig4 = rig.clone();
                        let id1 = block.id.clone();
                        let id2 = block.id.clone();
                        let id3 = block.id.clone();
                        let id4 = block.id.clone();
                        let enabled = b.enabled;
                        let q = b.q;
                        rsx! {
                            span {
                                class: "w-3 h-3 rounded-full flex-shrink-0",
                                style: "background-color: {color};",
                            }
                            span { class: "text-[10px] font-mono", "{b.frequency:.0} Hz · {b.gain:+.1} dB" }
                            select {
                                class: "bg-transparent border border-border rounded text-[10px] px-1",
                                value: "{shape_index(b.shape) as usize}",
                                onchange: move |e: FormEvent| {
                                    if let Ok(v) = e.value().parse::<usize>() {
                                        send(&rig1, &id1, i, "shape", v as f32);
                                    }
                                },
                                for (si, s) in EqBandShape::all().iter().enumerate() {
                                    option { key: "{si}", value: "{si}", selected: *s == b.shape, "{s.label()}" }
                                }
                            }
                            span { class: "text-[9px] font-mono text-muted-foreground", "Q" }
                            input {
                                r#type: "range",
                                class: "w-24 h-1 accent-primary",
                                min: "0.025", max: "40", step: "any", value: "{q}",
                                oninput: move |e: FormEvent| {
                                    if let Ok(v) = e.value().parse::<f32>() {
                                        send(&rig2, &id2, i, "q", v);
                                    }
                                },
                            }
                            span { class: "text-[9px] font-mono w-10", "{q:.2}" }
                            button {
                                class: "text-[10px] px-1.5 py-0.5 rounded border border-border hover:bg-accent/40",
                                onclick: move |_| send(&rig3, &id3, i, "on", if enabled { 0.0 } else { 1.0 }),
                                if enabled { "On" } else { "Off" }
                            }
                            button {
                                class: "text-[10px] px-1.5 py-0.5 rounded border border-border text-red-400 hover:bg-red-500/20 ml-auto",
                                onclick: move |_| {
                                    send(&rig4, &id4, i, "used", 0.0);
                                    selected.set(None);
                                },
                                "Delete"
                            }
                        }
                    }
                }
                }
            }
        }
    }
}
