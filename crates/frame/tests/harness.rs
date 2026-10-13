//! A headless host: what a mixer strip or REAPER does to an embed —
//! size it, paint it (record once, live over the top), feed it, drag it —
//! rendered to PNGs under the test target dir, with per-frame timings.
//!
//! `cargo test -p frame-embed --test harness -- --nocapture` prints the
//! timing table.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use frame::document::proto::{Node, NodeKind, Scene, Size};
use frame::embed::cpu::{Pixels, Rasterizer, snapshot};
use frame::embed::tone::{Which, chains, demo};
use frame::embed::{
    Button, DocumentView, Embed, EmbedHost, EngineCall, FeaturedView, Modifiers, Pointer,
    Recording, Tally, Tier, TonePanel, ToneRack,
};
use frame::surface::runtime::Surface;
use kurbo::Affine;

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("embed-shots");
    std::fs::create_dir_all(&d).expect("shots dir");
    d
}

fn fed(mut e: Embed, units: &[Which], index: usize, tick: usize) -> Embed {
    let values = demo::values(e.catalog(), units, index);
    e.apply_values(values);
    for f in demo::frames(e.catalog(), units, index, tick) {
        e.apply_frame(f, tick as f64 * 33.3);
    }
    e
}

fn panel(which: Which, w: f64, h: f64) -> Embed {
    let mut e = Embed::new(TonePanel::new(which), EmbedHost::Mcp);
    e.set_size(w, h);
    e
}

fn left(x: f64, y: f64) -> Pointer {
    Pointer::Down {
        x,
        y,
        button: Button::Left,
        mods: Modifiers::default(),
    }
}

fn set_calls(calls: &[EngineCall], addr: &str) -> usize {
    calls
        .iter()
        .filter(|c| matches!(c, EngineCall::SetParam { addr: a, .. } if a.0 == addr))
        .count()
}

/// How different two images are: the share of pixels that moved.
fn changed(a: &Pixels, b: &Pixels) -> f64 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let n = a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0.iter())
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count();
    n as f64 / f64::from(a.width * a.height)
}

/// Distinct colours: a render that is one flat fill has very few.
fn colours(p: &Pixels) -> usize {
    let mut seen: Vec<u32> = p
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0] & 0xf0, c[1] & 0xf0, c[2] & 0xf0, 0]))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

// ── REAPER's sizes ───────────────────────────────────────────────────────

#[test]
fn reaper_sizes_render_at_both_scales_with_timings() {
    let sizes = [
        (EmbedHost::Mcp, 60.0, 120.0),
        (EmbedHost::Mcp, 96.0, 200.0),
        (EmbedHost::Mcp, 160.0, 300.0),
        (EmbedHost::Tcp, 300.0, 40.0),
    ];
    let mut raster = Rasterizer::new();
    let mut rows = Vec::new();
    eprintln!(
        "\n{:<10} {:>9} {:>5} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "view", "size", "scale", "tier", "record", "live", "raster", "frame"
    );
    type Make = Box<dyn Fn(EmbedHost) -> Embed>;
    let views: Vec<(&str, Make)> = vec![
        (
            "comp",
            Box::new(|h| {
                fed(
                    Embed::new(TonePanel::new(Which::Comp), h),
                    &[Which::Comp],
                    2,
                    30,
                )
            }),
        ),
        (
            "eq",
            Box::new(|h| {
                fed(
                    Embed::new(TonePanel::new(Which::Eq), h),
                    &[Which::Eq],
                    2,
                    30,
                )
            }),
        ),
        (
            "reverb",
            Box::new(|h| {
                fed(
                    Embed::new(TonePanel::new(Which::Reverb), h),
                    &[Which::Reverb],
                    2,
                    30,
                )
            }),
        ),
        (
            "featured",
            Box::new(|h| Embed::new(FeaturedView::new(frame::surface::memory::demo_catalog()), h)),
        ),
    ];
    for (name, make) in &views {
        let mut shots = Vec::new();
        for (host, w, h) in sizes {
            for scale in [1.0, 2.0] {
                let mut e = make(host);
                e.set_size(w, h);
                // The host's loop: record when stale, live every frame,
                // rasterise both into its bitmap.
                let (mut rec_t, mut live_t, mut ras_t, mut frame_t) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new());
                let mut rec = Recording::new();
                let mut px = None;
                for i in 0..12 {
                    let t = 1000.0 + f64::from(i) * 33.3;
                    let t0 = Instant::now();
                    rec = e.recording();
                    rec_t.push(t0.elapsed());
                    let t1 = Instant::now();
                    let mut live = Recording::new();
                    e.live(&mut live, Affine::IDENTITY, t);
                    live_t.push(t1.elapsed());
                    let t2 = Instant::now();
                    let p = raster.render(
                        (w * scale) as u32,
                        (h * scale) as u32,
                        scale,
                        frame::embed::cpu::STRIP_GROUND,
                        |c, at| {
                            rec.replay(c, at);
                            live.replay(c, at);
                        },
                    );
                    ras_t.push(t2.elapsed());
                    frame_t.push(t0.elapsed());
                    px = Some(p);
                }
                let p = px.expect("rendered");
                assert_eq!(
                    (p.width, p.height),
                    ((w * scale) as u32, (h * scale) as u32)
                );
                assert!(
                    colours(&p) > 6,
                    "{name} at {w}x{h}@{scale} drew something ({} colours)",
                    colours(&p)
                );
                // Both passes replayed equal one paint.
                let whole = snapshot(&mut raster, &mut e, scale, 1000.0 + 11.0 * 33.3);
                assert!(
                    changed(&whole, &p) < 0.15,
                    "{name} {w}x{h}@{scale}: record + live ≈ paint (live is drawn on top) ({:.3})",
                    changed(&whole, &p)
                );
                eprintln!(
                    "{name:<10} {:>9} {scale:>5} {:>6} {:>10?} {:>10?} {:>10?} {:>10?}",
                    format!("{w}x{h}"),
                    e.tier().name(),
                    median(rec_t),
                    median(live_t),
                    median(ras_t),
                    median(frame_t)
                );
                if scale == 1.0 || w == 96.0 {
                    shots.push(p);
                }
                let _ = rec.len();
            }
        }
        rows.push(Pixels::row(&shots, 4, [9, 9, 11, 255]));
    }
    Pixels::column(&rows, 6, [9, 9, 11, 255])
        .save_png(out_dir().join("reaper-sizes.png"))
        .expect("png");
}

// ── the mixer ────────────────────────────────────────────────────────────

#[test]
fn a_mixer_row_at_sessions_widths_lines_up() {
    let mut raster = Rasterizer::new();
    let widths = [40.0, 86.0, 133.0, 300.0];
    let tiers = [Tier::Minimal, Tier::Curves, Tier::Full, Tier::Focus];
    let mut shots = Vec::new();
    let mut heights = Vec::new();
    for (i, (w, tier)) in widths.iter().zip(tiers).enumerate() {
        let mut e = Embed::new(ToneRack::channel(), EmbedHost::Strip);
        let h = e.height_for(*w).expect("a rack knows its height");
        e.set_size(*w, h);
        assert_eq!(e.tier(), tier, "{w} px is {tier:?}");
        let mut e = fed(e, &chains::CHANNEL, i + 1, 40);
        heights.push(h);
        let t = Instant::now();
        shots.push(snapshot(&mut raster, &mut e, 1.0, 1300.0));
        eprintln!("rack {w} x {h}: {:?}", t.elapsed());
    }
    // Height is a fact about the chain, not about the width: every strip's
    // compressor sits on one line across the mixer.
    assert!(
        heights.windows(2).all(|p| (p[0] - p[1]).abs() < 1e-9),
        "{heights:?}"
    );
    Pixels::row(&shots, 2, [9, 9, 11, 255])
        .save_png(out_dir().join("mixer-row.png"))
        .expect("png");
}

#[test]
fn folding_a_phase_shortens_the_rack_and_relayouts() {
    let mut e = Embed::new(ToneRack::channel(), EmbedHost::Strip);
    let rest = e.height_for(133.0).unwrap();
    e.set_size(133.0, rest);
    // Rescue rests shut (session's `Folded::rest`); its bar unfolds it.
    let builds = e.builds();
    let mut out = e.pointer(left(40.0, 7.0), 0.0);
    out.calls.extend(
        e.pointer(
            Pointer::Up {
                x: 40.0,
                y: 7.0,
                button: Button::Left,
            },
            10.0,
        )
        .calls,
    );
    assert_eq!(
        set_calls(&out.calls, "rack/fold_rescue"),
        1,
        "{:?}",
        out.calls
    );
    assert!(e.builds() > builds, "folding relayouts");
    let open = e.height_for(133.0).unwrap();
    assert!(
        open > rest + 300.0,
        "unfolding Rescue adds its units: {rest} → {open}"
    );
}

// ── interaction ──────────────────────────────────────────────────────────

/// A comp alone at 96 x 170: Full tier; its body starts 12 px down (2 px
/// inset + the 10 px header), and the display is the whole body.
fn threshold_y(db: f64) -> f64 {
    12.0 + (-db / 60.0) * 156.0
}

#[test]
fn dragging_the_threshold_moves_it_and_a_double_click_resets() {
    let mut e = panel(Which::Comp, 96.0, 170.0);
    assert_eq!(e.tier(), Tier::Full);
    let before = e.value("comp/threshold").unwrap();
    assert!((before + 18.0).abs() < 0.01, "default −18 dB, got {before}");
    let y = threshold_y(before);
    let down = e.pointer(left(60.0, y), 0.0);
    assert!(down.handled, "the line takes the click");
    let mut calls = down.calls;
    for i in 1..=10 {
        calls.extend(
            e.pointer(
                Pointer::Move {
                    x: 60.0,
                    y: y + 3.0 * f64::from(i),
                },
                f64::from(i) * 16.0,
            )
            .calls,
        );
    }
    calls.extend(
        e.pointer(
            Pointer::Up {
                x: 60.0,
                y: y + 30.0,
                button: Button::Left,
            },
            200.0,
        )
        .calls,
    );
    calls.extend(e.poll(400.0));
    assert!(
        set_calls(&calls, "comp/threshold") > 0,
        "the engine hears it: {calls:?}"
    );
    let after = e.value("comp/threshold").unwrap();
    assert!(
        after < -25.0,
        "30 px down a 156 px display lowers it ~11.5 dB: {after}"
    );
    assert!(e.needs_record(), "a moved param stales the record pass");

    let reset = e.pointer(
        Pointer::DoubleClick {
            x: 60.0,
            y: threshold_y(after),
            button: Button::Left,
        },
        600.0,
    );
    assert!(reset.handled);
    assert!(
        (e.value("comp/threshold").unwrap() + 18.0).abs() < 0.01,
        "double-click resets"
    );
}

#[test]
fn the_ratio_arrow_drags_down_for_more() {
    let mut e = panel(Which::Comp, 96.0, 170.0);
    let r0 = e.value("comp/ratio").unwrap();
    // The arrow's column: 14 px into the body, under the threshold.
    let (x, y) = (2.0 + 14.0, threshold_y(-18.0) + 10.0);
    e.pointer(left(x, y), 0.0);
    for i in 1..=8 {
        e.pointer(
            Pointer::Move {
                x,
                y: y + 5.0 * f64::from(i),
            },
            f64::from(i) * 16.0,
        );
    }
    e.pointer(
        Pointer::Up {
            x,
            y: y + 40.0,
            button: Button::Left,
        },
        200.0,
    );
    assert!(
        e.value("comp/ratio").unwrap() > r0,
        "{r0} → {:?}",
        e.value("comp/ratio")
    );
}

#[test]
fn wheel_steps_the_eq_zoom_and_the_graph_relayouts() {
    let mut e = panel(Which::Eq, 133.0, 175.0);
    let builds = e.builds();
    let r0 = e.value("eq/range").unwrap();
    // The zoom chip: centred at the top of the body.
    let out = e.pointer(
        Pointer::Wheel {
            x: 66.5,
            y: 19.0,
            notches: 1.0,
            mods: Modifiers::default(),
        },
        0.0,
    );
    assert!(out.handled && out.redraw, "{out:?}");
    assert_eq!(e.value("eq/range").unwrap(), r0 + 1.0);
    assert!(e.builds() > builds, "the zoom changes the graph's axes");
}

#[test]
fn an_eq_band_follows_the_pointer() {
    let mut e = fed(panel(Which::Eq, 133.0, 175.0), &[Which::Eq], 4, 0);
    let (f0, g0) = (
        e.value("eq/b3_freq").unwrap(),
        e.value("eq/b3_gain").unwrap(),
    );
    // Find band 3's handle by its hit box, then drag it up and right.
    let rt = e.runtime();
    let id = rt
        .surface()
        .scene
        .nodes
        .values()
        .find(|n| n.name == "band 3 handle")
        .map(|n| n.id)
        .expect("handle");
    let b = rt.hit_box(id).expect("placed");
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    assert_eq!(e.hit(x, y), Some(id));
    e.pointer(left(x, y), 0.0);
    e.pointer(
        Pointer::Move {
            x: x + 10.0,
            y: y - 10.0,
        },
        16.0,
    );
    e.pointer(
        Pointer::Up {
            x: x + 10.0,
            y: y - 10.0,
            button: Button::Left,
        },
        32.0,
    );
    let (f1, g1) = (
        e.value("eq/b3_freq").unwrap(),
        e.value("eq/b3_gain").unwrap(),
    );
    assert!(f1 > f0 && g1 > g0, "({f0}, {g0}) → ({f1}, {g1})");
}

#[test]
fn a_right_click_is_the_hosts() {
    let mut e = panel(Which::Comp, 96.0, 170.0);
    let out = e.pointer(
        Pointer::Down {
            x: 60.0,
            y: threshold_y(-18.0),
            button: Button::Right,
            mods: Modifiers::default(),
        },
        0.0,
    );
    assert!(out.passthrough && !out.handled && out.calls.is_empty());
}

#[test]
fn the_header_bypasses_and_the_scrim_says_so() {
    let mut raster = Rasterizer::new();
    let mut e = fed(panel(Which::Comp, 96.0, 170.0), &[Which::Comp], 1, 20);
    let on = snapshot(&mut raster, &mut e, 1.0, 700.0);
    let mut out = e.pointer(left(30.0, 6.0), 0.0);
    out.calls.extend(
        e.pointer(
            Pointer::Up {
                x: 30.0,
                y: 6.0,
                button: Button::Left,
            },
            20.0,
        )
        .calls,
    );
    assert_eq!(set_calls(&out.calls, "comp/bypass"), 1, "{:?}", out.calls);
    assert_eq!(e.value("comp/bypass"), Some(1.0));
    let off = snapshot(&mut raster, &mut e, 1.0, 700.0);
    let lum = |p: &Pixels| {
        p.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2]))
            .sum::<u32>()
    };
    assert!(
        lum(&off) * 10 < lum(&on) * 8,
        "the scrim greys the panel out"
    );
    off.save_png(out_dir().join("comp-bypassed.png"))
        .expect("png");
}

// ── passes and relayout ──────────────────────────────────────────────────

#[test]
fn record_once_live_every_frame() {
    let mut e = fed(panel(Which::Comp, 96.0, 170.0), &[Which::Comp], 1, 10);
    assert!(e.has_live());
    let mut all = Tally::default();
    e.paint(&mut all, Affine::IDENTITY, 0.0);
    let mut rec = Tally::default();
    e.record(&mut rec, Affine::IDENTITY);
    let mut live = Tally::default();
    e.live(&mut live, Affine::IDENTITY, 0.0);
    assert!(!e.needs_record(), "recorded");
    assert!(live.fills + live.strokes > 0, "the histories are live");
    assert_eq!(
        rec.fills + live.fills,
        all.fills,
        "the two passes partition the paint"
    );
    assert_eq!(rec.strokes + live.strokes, all.strokes);
    // A feed frame moves the live pass only.
    for f in demo::frames(e.catalog(), &[Which::Comp], 1, 11) {
        e.apply_frame(f, 400.0);
    }
    assert!(!e.needs_record(), "feeds do not touch the recording");
    assert!(e.animating(), "a history is playing");
    // A param moves the recording.
    let values = demo::values(e.catalog(), &[Which::Comp], 5);
    e.apply_values(values);
    assert!(e.needs_record());
}

#[test]
fn resizing_relayouts_and_keeps_values() {
    let mut e = fed(panel(Which::Comp, 96.0, 170.0), &[Which::Comp], 3, 0);
    let thr = e.value("comp/threshold").unwrap();
    let b = e.builds();
    assert!(!e.set_size(96.0, 170.0), "the same box is not a relayout");
    assert_eq!(e.builds(), b);
    for (w, tier) in [
        (300.0, Tier::Focus),
        (86.0, Tier::Curves),
        (40.0, Tier::Minimal),
    ] {
        assert!(e.set_size(w, 170.0));
        assert_eq!(e.tier(), tier);
        assert!(
            (e.value("comp/threshold").unwrap() - thr).abs() < 1e-9,
            "values survive the relayout"
        );
    }
}

#[test]
fn curve_overrides_reach_the_panel() {
    let mut raster = Rasterizer::new();
    let mut e = panel(Which::Sat, 133.0, Which::Sat.natural());
    let before = snapshot(&mut raster, &mut e, 1.0, 0.0);
    // A host registers the plugin's own transfer (here: a hard clip).
    e.register_curve("sat.transfer", |i| {
        let n = i.samples;
        vec![frame::surface::runtime::Series::new(
            "transfer",
            (0..n)
                .map(|k| {
                    let x = -1.0 + 2.0 * k as f64 / (n - 1) as f64;
                    (x, (3.0 * x).clamp(-1.0, 1.0))
                })
                .collect(),
        )]
    });
    assert!(e.needs_record());
    let after = snapshot(&mut raster, &mut e, 1.0, 0.0);
    assert!(
        changed(&before, &after) > 0.001,
        "the override is what is drawn"
    );
}

// ── authored documents ───────────────────────────────────────────────────

fn page(scene: &mut Scene, name: &str, w: f64, h: f64, fill: &str) {
    let mut p = Node::new(NodeKind::Page, name);
    p.parent = Some(scene.root);
    let pid = p.id;
    scene.nodes.insert(pid, p);
    scene.node_mut(scene.root).unwrap().children.push(pid);
    let mut root = Node::new(NodeKind::Frame, "root");
    root.size = Size {
        width: w,
        height: h,
    };
    root.fills = vec![frame::surface::kit::draw::solid(fill)];
    root.parent = Some(pid);
    let rid = root.id;
    scene.nodes.insert(rid, root);
    scene.node_mut(pid).unwrap().children.push(rid);
}

#[test]
fn a_document_embeds_by_its_breakpoint_pages() {
    let mut scene = Scene::new("Full");
    page(&mut scene, "embed:mcp", 60.0, 120.0, "#ff0000");
    page(&mut scene, "embed:mcp@120", 120.0, 200.0, "#00ff00");
    page(&mut scene, "embed:tcp", 300.0, 40.0, "#0000ff");
    let view = DocumentView::new(
        Surface::from_scene(scene.clone()),
        frame::surface::proto::Catalog::default(),
        EmbedHost::Mcp,
    );
    let name = |w: f64| {
        view.page_at(w)
            .and_then(|p| scene.node(p))
            .map(|n| n.name.clone())
    };
    assert_eq!(name(80.0).as_deref(), Some("embed:mcp"));
    assert_eq!(name(160.0).as_deref(), Some("embed:mcp@120"));
    assert_eq!(
        frame::embed::EmbedView::height_for(&view, 240.0, &Default::default()),
        Some(400.0),
        "the page scales to the width"
    );
    let mut e = Embed::new(view, EmbedHost::Mcp);
    e.set_size(160.0, 266.0);
    let mut raster = Rasterizer::new();
    let p = snapshot(&mut raster, &mut e, 1.0, 0.0);
    assert_eq!(
        p.at(150, 250),
        [0, 255, 0, 255],
        "the @120 page, scaled to 160 wide"
    );
}

#[cfg(feature = "anyrender-0-11")]
#[test]
fn paints_into_an_anyrender_scene() {
    use anyrender_0_11 as anyrender;
    let mut e = fed(panel(Which::Eq, 133.0, 175.0), &[Which::Eq], 1, 0);
    let mut scene = anyrender::Scene::new();
    e.record(
        &mut frame::embed::AnyRender(&mut scene),
        Affine::translate((200.0, 40.0)),
    );
    assert!(
        scene.commands.len() > 20,
        "{} commands",
        scene.commands.len()
    );
}
