//! `viz.eq_graph` as a user drives it through the runtime: add a band by
//! double-clicking, rename it, marquee-select and group-drag, drag one out
//! to delete it, the band panel following its band, the context menu, the
//! keyboard, the range switch.

use std::collections::BTreeMap;

use document_proto::{NodeId, Scene};
use surface_kit::{Place, place, viz};
use surface_proto::{Binding, Catalog, ControlKind, ParamAddr};
use surface_runtime::{Bounds, EngineCall, Input, Key, Modifiers, Runtime, Surface};
use surface_widgets::{Builder, Theme};

fn catalog() -> Catalog {
    let mut params = viz::catalog::eq_params("eq", 24);
    params.extend(viz::catalog::eq_view_params("eq"));
    Catalog { id: "viz.eq".into(), version: 1, params, streams: viz::catalog::eq_streams("eq", 64), ..Catalog::default() }
}

fn eq() -> Runtime {
    let mut scene = Scene::new("eq");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let p = Place::new("EQ").size(820.0).options(&["ns=eq", "bands=24"]).bind(Binding::stream("eq/spectrum"));
        place(&mut b, page, "viz.eq_graph", &p).unwrap();
    }
    let mut rt = Runtime::new(Surface { scene, controls }, catalog());
    let boxes = render::build_scene_layout_boxes(&rt.surface().scene, page);
    rt.set_boxes(boxes.iter().map(|(id, b)| (*id, Bounds { x: b.x, y: b.y, width: b.width, height: b.height })).collect::<Vec<_>>());
    rt
}

fn named(rt: &Runtime, name: &str) -> NodeId {
    let s = &rt.surface().scene;
    *rt.surface().controls.keys().find(|id| s.node(**id).is_some_and(|n| n.name == name)).unwrap_or_else(|| panic!("no control `{name}`"))
}

fn real(rt: &Runtime, addr: &str) -> f64 {
    rt.real(&ParamAddr::new(addr)).unwrap()
}

fn used(rt: &Runtime, n: u32) -> bool {
    real(rt, &format!("eq/b{n}_used")) >= 0.5
}

fn centre(b: Bounds) -> (f64, f64) {
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

fn handle(rt: &Runtime, n: u32) -> NodeId {
    named(rt, &format!("band {n} handle"))
}

fn at_handle(rt: &Runtime, n: u32) -> (f64, f64) {
    centre(rt.hit_box(handle(rt, n)).unwrap())
}

/// The plot area (the canvas) and its box.
fn canvas(rt: &Runtime) -> (NodeId, Bounds) {
    let id = *rt.surface().controls.iter().find(|(_, c)| matches!(c.kind, ControlKind::Canvas { .. })).unwrap().0;
    (id, rt.hit_box(id).unwrap())
}

/// The canvas point at `hz` and `db` (±12 dB range).
fn point(rt: &Runtime, hz: f64, db: f64) -> (f64, f64) {
    let (_, b) = canvas(rt);
    let u = (hz / 20.0).ln() / 1000f64.ln();
    let v = (db + 12.0) / 24.0;
    (b.x + u * b.width, b.y + (1.0 - v) * b.height)
}

const M: Modifiers = Modifiers { fine: false, alt: false, option: false };

fn click(rt: &mut Runtime, (x, y): (f64, f64), mods: Modifiers, t: f64) -> Vec<EngineCall> {
    let mut c = rt.pointer(Input::Down { x, y, t, mods }, t);
    c.extend(rt.pointer(Input::Up { x, y, t }, t + 10.0));
    c
}

fn double_click(rt: &mut Runtime, (x, y): (f64, f64), t: f64) -> Vec<EngineCall> {
    let mut c = click(rt, (x, y), M, t);
    c.extend(rt.pointer(Input::Down { x, y, t: t + 100.0, mods: M }, t + 100.0));
    c.extend(rt.pointer(Input::DoubleClick, t + 100.0));
    c.extend(rt.pointer(Input::Up { x, y, t: t + 110.0 }, t + 110.0));
    c
}

fn drag(rt: &mut Runtime, from: (f64, f64), to: (f64, f64), mods: Modifiers, t: f64) -> Vec<EngineCall> {
    let mut c = rt.pointer(Input::Down { x: from.0, y: from.1, t, mods }, t);
    for i in 1..=4 {
        let k = f64::from(i) / 4.0;
        let (x, y) = (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
        c.extend(rt.pointer(Input::Move { x, y, t: t + 16.0 * f64::from(i), mods }, t + 16.0 * f64::from(i)));
    }
    c.extend(rt.pointer(Input::Up { x: to.0, y: to.1, t: t + 100.0 }, t + 100.0));
    c.extend(rt.poll(t + 1000.0));
    c
}

#[test]
fn five_bands_start_in_use_and_the_rest_are_hidden() {
    let rt = eq();
    assert!((1..=5).all(|n| used(&rt, n)) && (6..=24).all(|n| !used(&rt, n)));
    let frame = rt.frame(0.0);
    let shown = |n: u32| {
        let root = frame.node(handle(&rt, n)).and_then(|h| h.parent).unwrap();
        frame.node(root).unwrap().visible
    };
    assert!(shown(1) && !shown(6), "an unused band is not drawn");
    assert_eq!(rt.hit(at_handle(&rt, 6).0, at_handle(&rt, 6).1), Some(canvas(&rt).0), "nor hit");
}

#[test]
fn double_clicking_empty_space_adds_a_band_shaped_by_where() {
    let mut rt = eq();
    // A bell at 1 kHz, +6 dB.
    let calls = { let a0 = point(&rt, 1000.0, 6.0); double_click(&mut rt, a0, 0.0) };
    assert!(used(&rt, 6), "the first free slot");
    assert!((real(&rt, "eq/b6_freq") - 1000.0).abs() < 15.0, "{}", real(&rt, "eq/b6_freq"));
    assert!((real(&rt, "eq/b6_gain") - 6.0).abs() < 0.3);
    assert_eq!(real(&rt, "eq/b6_shape"), 0.0, "a bell");
    assert!(calls.iter().any(|c| matches!(c, EngineCall::SetParam { addr, value, .. } if addr.0 == "eq/b6_used" && *value == 1.0)));
    assert_eq!(rt.selection(), [handle(&rt, 6)], "the new band is selected");
    // Its panel is up, under it.
    let panel = named(&rt, "band panel");
    let (px, py) = rt.popover_origin(panel).expect("the band panel follows the new band");
    let hb = rt.hit_box(handle(&rt, 6)).unwrap();
    assert!(py >= hb.y + hb.height - 1.0 && (px + 150.0 - centre(hb).0).abs() < 1.0, "below, centred: {px},{py} vs {hb:?}");

    // Near the left edge at 0 dB: a low cut with no gain.
    { let a0 = point(&rt, 24.0, 0.3); double_click(&mut rt, a0, 1000.0); }
    assert_eq!(real(&rt, "eq/b7_shape"), 3.0, "low cut");
    assert_eq!(real(&rt, "eq/b7_gain"), 0.0);
    // Top right: a high shelf keeps its gain.
    { let a0 = point(&rt, 12_000.0, 8.0); double_click(&mut rt, a0, 2000.0); }
    assert_eq!(real(&rt, "eq/b8_shape"), 2.0, "high shelf");
    assert!(real(&rt, "eq/b8_gain") > 7.0);
    // Deep down: a notch. (Clear of the band panel, docked on the floor
    // under the 12 kHz band just added — a double-click on it doesn't add.)
    { let a0 = point(&rt, 300.0, -11.5); double_click(&mut rt, a0, 3000.0); }
    assert_eq!(real(&rt, "eq/b9_shape"), 5.0, "notch");
}

#[test]
fn a_new_band_is_under_the_hand() {
    let mut rt = eq();
    let at = point(&rt, 500.0, 0.0);
    click(&mut rt, at, M, 0.0);
    rt.pointer(Input::Down { x: at.0, y: at.1, t: 100.0, mods: M }, 100.0);
    rt.pointer(Input::DoubleClick, 100.0);
    rt.pointer(Input::Move { x: at.0 + 60.0, y: at.1 - 40.0, t: 116.0, mods: M }, 116.0);
    rt.pointer(Input::Up { x: at.0 + 60.0, y: at.1 - 40.0, t: 132.0 }, 132.0);
    assert!(real(&rt, "eq/b6_freq") > 600.0 && real(&rt, "eq/b6_gain") > 2.0, "dragged straight after adding");
}

#[test]
fn double_clicking_a_band_renames_it_in_place() {
    let mut rt = eq();
    let h = handle(&rt, 3);
    let before = real(&rt, "eq/b3_gain");
    { let a0 = at_handle(&rt, 3); double_click(&mut rt, a0, 0.0); }
    let (field, text) = rt.editing().expect("editing the name label");
    assert_eq!(text, "");
    assert_eq!(rt.surface().scene.node(field).unwrap().name, "name");
    let mut calls = rt.type_text("Kick").unwrap();
    calls.extend(rt.key(Key::Backspace, M, 0.0).unwrap());
    calls.extend(rt.type_text("k").unwrap());
    calls.extend(rt.key(Key::Enter, M, 0.0).unwrap());
    assert!(rt.editing().is_none());
    assert_eq!(rt.text(&ParamAddr::new("eq/b3_name")), Some("Kick"));
    assert!(calls.iter().any(|c| matches!(c, EngineCall::SetText { addr, text } if addr.0 == "eq/b3_name" && text == "Kick")));
    assert_eq!(real(&rt, "eq/b3_gain"), before, "a double-click doesn't reset the band");
    // The label shows it, riding above the handle.
    let frame = rt.frame(0.0);
    let label = frame.nodes.values().find(|n| n.name == "value" && n.text().is_some_and(|t| t.characters == "Kick")).expect("label drawn");
    assert!(label.visible);
    let label_pop = named(&rt, "band 3 label");
    let (_, ly) = rt.popover_origin(label_pop).unwrap();
    assert!(ly < rt.hit_box(h).unwrap().y + 8.0, "above the node");
    // Escape cancels a rename.
    { let a0 = at_handle(&rt, 3); double_click(&mut rt, a0, 1000.0); }
    rt.type_text("Snare");
    rt.key(Key::Escape, M, 1100.0);
    assert_eq!(rt.text(&ParamAddr::new("eq/b3_name")), Some("Kick"));
}

#[test]
fn the_panel_name_field_edits_with_caret_and_selection() {
    let mut rt = eq();
    { let a0 = at_handle(&rt, 2); click(&mut rt, a0, M, 0.0); }
    let field = named(&rt, "name field");
    let (dx, dy) = rt.popover_origin(named(&rt, "band panel")).unwrap();
    let panel_box = rt.hit_box(named(&rt, "band panel")).unwrap();
    let fb = rt.hit_box(field).unwrap();
    let at = (fb.x - panel_box.x + dx + 10.0, fb.y - panel_box.y + dy + 10.0);
    assert_eq!(rt.hit(at.0, at.1), Some(field), "the panel's field is hit where the panel is drawn");
    click(&mut rt, at, M, 100.0);
    rt.type_text("Low end");
    rt.key(Key::Char('a'), Modifiers { alt: true, ..M }, 200.0);
    rt.type_text("Warmth");
    rt.key(Key::Enter, M, 300.0);
    assert_eq!(rt.text(&ParamAddr::new("eq/b2_name")), Some("Warmth"), "select-all then type replaces");
}

#[test]
fn marquee_selects_and_the_group_drags_together() {
    let mut rt = eq();
    // Bands 3 (420 Hz) and 4 (2.4 kHz) inside; band 2 (120 Hz) outside.
    let (_, cb) = canvas(&rt);
    let from = (point(&rt, 250.0, 0.0).0, cb.y + 2.0);
    let to = (point(&rt, 4000.0, 0.0).0, cb.y + cb.height - 2.0);
    drag(&mut rt, from, to, M, 0.0);
    assert_eq!(rt.selection().len(), 2, "{:?}", rt.selection());
    assert!(rt.selection().contains(&handle(&rt, 3)) && rt.selection().contains(&handle(&rt, 4)));
    // Drawn while dragging: the marquee node.
    let (f3, f4, g3, g4) = (real(&rt, "eq/b3_freq"), real(&rt, "eq/b4_freq"), real(&rt, "eq/b3_gain"), real(&rt, "eq/b4_gain"));
    let h3 = at_handle(&rt, 3);
    drag(&mut rt, h3, (h3.0 + 60.0, h3.1 - 20.0), M, 1000.0);
    let (r3, r4) = (real(&rt, "eq/b3_freq") / f3, real(&rt, "eq/b4_freq") / f4);
    assert!(r3 > 1.3 && (r3 - r4).abs() < 0.02, "the same frequency ratio: {r3} {r4}");
    assert!((real(&rt, "eq/b3_gain") - g3 - (real(&rt, "eq/b4_gain") - g4)).abs() < 0.05, "the same gain travel");
    // A click on empty space clears the selection.
    { let a0 = point(&rt, 60.0, -6.0); click(&mut rt, a0, M, 3000.0); }
    assert!(rt.selection().is_empty());
}

#[test]
fn chord_clicks_select_bypass_and_cycle() {
    let mut rt = eq();
    { let a0 = at_handle(&rt, 2); click(&mut rt, a0, M, 0.0); }
    { let a0 = at_handle(&rt, 4); click(&mut rt, a0, Modifiers { alt: true, ..M }, 100.0); }
    assert_eq!(rt.selection(), [handle(&rt, 2), handle(&rt, 4)], "Cmd adds");
    { let a0 = at_handle(&rt, 5); click(&mut rt, a0, Modifiers { fine: true, ..M }, 200.0); }
    assert_eq!(rt.selection().len(), 2, "Shift selects the range 4..5 from the last: {:?}", rt.selection());
    // Alt-click bypasses (no drag); Alt+Cmd cycles the shape.
    let g = real(&rt, "eq/b3_gain");
    { let a0 = at_handle(&rt, 3); click(&mut rt, a0, Modifiers { option: true, ..M }, 300.0); }
    assert_eq!(real(&rt, "eq/b3_on"), 0.0);
    assert_eq!(real(&rt, "eq/b3_gain"), g);
    { let a0 = at_handle(&rt, 3); click(&mut rt, a0, Modifiers { option: true, alt: true, ..M }, 400.0); }
    assert_eq!(real(&rt, "eq/b3_shape"), 1.0, "bell → low shelf");
    // A bypassed band's node is grey, its curve gone.
    let frame = rt.frame(0.0);
    let grey = frame.nodes.values().filter(|n| n.name == "bypassed" && n.visible).count();
    assert!(grey >= 1);
}

#[test]
fn drag_modes_and_wheel_targets() {
    let mut rt = eq();
    let (f, g, q) = (real(&rt, "eq/b3_freq"), real(&rt, "eq/b3_gain"), real(&rt, "eq/b3_q"));
    let at = at_handle(&rt, 3);
    // Alt: frequency only.
    drag(&mut rt, at, (at.0 + 50.0, at.1 - 50.0), Modifiers { option: true, ..M }, 0.0);
    let (f1, g1) = (real(&rt, "eq/b3_freq"), real(&rt, "eq/b3_gain"));
    assert!(f1 > f * 1.2 && (g1 - g).abs() < 1e-3, "{f} → {f1}, {g} → {g1}");
    assert_eq!(real(&rt, "eq/b3_on"), 1.0, "an Alt-drag is not an Alt-click");
    // Cmd: vertical travel is resonance; the node stays.
    let at = at_handle(&rt, 3);
    let f = real(&rt, "eq/b3_freq");
    drag(&mut rt, at, (at.0, at.1 - 60.0), Modifiers { alt: true, ..M }, 1000.0);
    assert!((real(&rt, "eq/b3_q") / q - 2.0).abs() < 0.15, "60 px up doubles Q: {q} → {}", real(&rt, "eq/b3_q"));
    assert!((real(&rt, "eq/b3_freq") - f).abs() < 1e-6);
    // Wheel: Q; Cmd-wheel: gain; a cut's wheel: its slope.
    let h = handle(&rt, 3);
    let q = real(&rt, "eq/b3_q");
    rt.feed(h, Input::Wheel { notches: 1.0, mods: M }, 2000.0);
    assert!((real(&rt, "eq/b3_q") / q - 1.15).abs() < 0.01);
    let g = real(&rt, "eq/b3_gain");
    rt.feed(h, Input::Wheel { notches: 2.0, mods: Modifiers { alt: true, ..M } }, 2100.0);
    assert!((real(&rt, "eq/b3_gain") - g - 2.0).abs() < 0.05, "{g} → {}", real(&rt, "eq/b3_gain"));
    rt.feed(handle(&rt, 1), Input::Wheel { notches: 1.0, mods: M }, 2200.0);
    assert_eq!(real(&rt, "eq/b1_slope"), 3.0, "a low cut's wheel steps its slope");
}

#[test]
fn dragging_a_band_out_of_the_graph_deletes_it() {
    let mut rt = eq();
    let (_, cb) = canvas(&rt);
    let at = at_handle(&rt, 4);
    rt.pointer(Input::Down { x: at.0, y: at.1, t: 0.0, mods: M }, 0.0);
    rt.pointer(Input::Move { x: at.0, y: cb.y + cb.height + 40.0, t: 16.0, mods: M }, 16.0);
    let frame = rt.frame(16.0);
    assert!(frame.node(handle(&rt, 4)).unwrap().opacity < 0.5, "fades while out");
    let calls = rt.pointer(Input::Up { x: at.0, y: cb.y + cb.height + 40.0, t: 32.0 }, 32.0);
    assert!(!used(&rt, 4));
    assert!(calls.iter().any(|c| matches!(c, EngineCall::SetParam { addr, value, .. } if addr.0 == "eq/b4_used" && *value == 0.0)));
    // Dragged back in before letting go: kept.
    let at = at_handle(&rt, 3);
    rt.pointer(Input::Down { x: at.0, y: at.1, t: 100.0, mods: M }, 100.0);
    rt.pointer(Input::Move { x: at.0, y: cb.y + cb.height + 40.0, t: 116.0, mods: M }, 116.0);
    rt.pointer(Input::Move { x: at.0, y: at.1, t: 132.0, mods: M }, 132.0);
    rt.pointer(Input::Up { x: at.0, y: at.1, t: 148.0 }, 148.0);
    assert!(used(&rt, 3));
}

#[test]
fn delete_and_escape_act_on_the_selection() {
    let mut rt = eq();
    { let a0 = at_handle(&rt, 2); click(&mut rt, a0, M, 0.0); }
    { let a0 = at_handle(&rt, 5); click(&mut rt, a0, Modifiers { alt: true, ..M }, 100.0); }
    rt.key(Key::Delete, M, 200.0).expect("taken");
    assert!(!used(&rt, 2) && !used(&rt, 5) && used(&rt, 3));
    { let a0 = at_handle(&rt, 3); click(&mut rt, a0, M, 300.0); }
    rt.key(Key::Char('m'), M, 350.0).expect("a placement key");
    assert_eq!(real(&rt, "eq/b3_placement"), 3.0, "m places the band mid");
    rt.key(Key::Escape, M, 400.0).expect("taken");
    assert!(rt.selection().is_empty());
    assert!(rt.key(Key::Escape, M, 500.0).is_none(), "nothing to take: the host's");
}

#[test]
fn the_band_panel_follows_its_band_and_edits_it() {
    let mut rt = eq();
    let panel = named(&rt, "band panel");
    assert!(rt.popover_origin(panel).is_none(), "down with nothing active");
    { let a0 = at_handle(&rt, 3); click(&mut rt, a0, M, 0.0); }
    let before = rt.popover_origin(panel).unwrap();
    let at = at_handle(&rt, 3);
    drag(&mut rt, at, (at.0 + 80.0, at.1 - 30.0), M, 100.0);
    let after = rt.popover_origin(panel).unwrap();
    assert!((after.0 - before.0 - 80.0).abs() < 1.0, "follows across: {before:?} → {after:?}");
    // Docked: the band moved up 30 px, the panel did not move vertically,
    // and it sits on the plot's floor.
    assert_eq!(after.1, before.1, "locked vertically");
    let (_, cb) = canvas(&rt);
    assert!((after.1 + 104.0 - (cb.y + cb.height)).abs() <= 18.5, "docked to the plot's bottom: {after:?} {cb:?}");
    // Its controls act on band 3, where the panel is drawn.
    let bypass = named(&rt, "bypass");
    let pb = rt.hit_box(panel).unwrap();
    let bb = rt.hit_box(bypass).unwrap();
    let at = (after.0 + bb.x - pb.x + 5.0, after.1 + bb.y - pb.y + 5.0);
    assert_eq!(rt.hit(at.0, at.1), Some(bypass));
    click(&mut rt, at, M, 1000.0);
    assert_eq!(real(&rt, "eq/b3_on"), 0.0, "bypassed from the panel");
    // Selecting band 5 moves it there, and it edits band 5.
    { let a0 = at_handle(&rt, 5); click(&mut rt, a0, M, 2000.0); }
    let there = rt.popover_origin(panel).unwrap();
    let hb = rt.hit_box(handle(&rt, 5)).unwrap();
    assert!((there.0 + 150.0 - centre(hb).0).abs() < 1.0 || there.0 <= 12.0 || there.0 >= 400.0, "follows band 5");
    let del = named(&rt, "delete");
    let db = rt.hit_box(del).unwrap();
    click(&mut rt, (there.0 + db.x - pb.x + 5.0, there.1 + db.y - pb.y + 5.0), M, 3000.0);
    assert!(!used(&rt, 5) && used(&rt, 3), "the panel's delete removed band 5");
    // Near the bottom edge it still docks there: it never flips or jumps.
    let at = at_handle(&rt, 2);
    drag(&mut rt, at, (at.0, cb.y + cb.height - 3.0), M, 8000.0);
    let (_, y) = rt.popover_origin(panel).unwrap();
    assert_eq!(y, there.1, "same height for every band, wherever it is");
}

#[test]
fn the_docked_panel_holds_still_while_it_is_used() {
    let mut rt = eq();
    let panel = named(&rt, "band panel");
    { let a0 = at_handle(&rt, 3); click(&mut rt, a0, M, 0.0); }
    let (x0, y0) = rt.popover_origin(panel).unwrap();
    // Touch the panel (hover over it), then move the band from elsewhere:
    // the panel stays under the hand.
    rt.pointer(Input::Move { x: x0 + 20.0, y: y0 + 20.0, t: 100.0, mods: M }, 100.0);
    rt.apply_values([surface_proto::ParamValue { addr: ParamAddr::new("eq/b3_freq"), value: 0.8 }]);
    assert_eq!(rt.popover_origin(panel).unwrap().0, x0, "held while the pointer is on it");
    // Leave it: held for the hold time, then it catches up with its band.
    rt.pointer(Input::Move { x: 1.0, y: 1.0, t: 200.0, mods: M }, 200.0);
    assert_eq!(rt.popover_origin(panel).unwrap().0, x0, "still held just after leaving");
    rt.pointer(Input::Move { x: 2.0, y: 1.0, t: 2300.0, mods: M }, 2300.0);
    assert_ne!(rt.popover_origin(panel).unwrap().0, x0, "follows again once left alone");
}

#[test]
fn the_context_menu_runs_commands_for_its_band() {
    let mut rt = eq();
    let at = at_handle(&rt, 3);
    rt.context_click(at.0, at.1, 0.0);
    let menu = named(&rt, "band menu");
    assert_eq!(rt.open_menus(), [menu]);
    assert_eq!(rt.selection(), [handle(&rt, 3)]);
    // Keyboard: down to "Invert Gain" (Bypass, Focus, Invert), Enter.
    let g = real(&rt, "eq/b3_gain");
    for _ in 0..3 {
        rt.key(Key::Down, M, 10.0);
    }
    rt.key(Key::Enter, M, 20.0);
    assert!((real(&rt, "eq/b3_gain") + g).abs() < 1e-6, "inverted: {g}");
    assert!(rt.open_menus().is_empty());
    // Shape › submenu by keyboard: down to Shape (4th action), Right, Down ×3 (Low Cut), Enter.
    let at = at_handle(&rt, 3);
    rt.context_click(at.0, at.1, 100.0);
    for _ in 0..4 {
        rt.key(Key::Down, M, 110.0);
    }
    rt.key(Key::Right, M, 120.0);
    assert_eq!(rt.open_menus().len(), 2, "the shape submenu opened");
    for _ in 0..3 {
        rt.key(Key::Down, M, 130.0);
    }
    rt.key(Key::Enter, M, 140.0);
    assert_eq!(real(&rt, "eq/b3_shape"), 3.0, "low cut from the submenu");
    assert_eq!(real(&rt, "eq/b3_gain"), 0.0, "a cut has no gain");
    // A press outside closes it without acting.
    let at = at_handle(&rt, 3);
    rt.context_click(at.0, at.1, 200.0);
    { let a0 = point(&rt, 50.0, -10.0); click(&mut rt, a0, M, 210.0); }
    assert!(rt.open_menus().is_empty() && used(&rt, 3));
    // By pointer: "Delete Band", the last row, where the menu is drawn
    // (at the pointer + 10, clamped inside the surface).
    rt.context_click(at.0, at.1, 300.0);
    let (ox, oy) = rt.floating_origin(menu).unwrap();
    assert!((ox - at.0 - 10.0).abs() < 1e-6 || ox < at.0, "{ox} vs {}", at.0);
    let row = named_node(&rt, "Delete Band");
    let (rb, mb) = (box_of(&rt, row), rt.hit_box(menu).unwrap());
    let target = (ox + rb.x - mb.x + 30.0, oy + rb.y - mb.y + rb.height / 2.0);
    // Hovering highlights it.
    rt.pointer(Input::Move { x: target.0, y: target.1, t: 305.0, mods: M }, 305.0);
    let frame = rt.frame(305.0);
    let hl = frame.nodes.values().find(|n| n.name == "highlight" && n.parent == Some(menu)).expect("a highlight");
    assert!(hl.visible);
    assert!((hl.transform.ty - (rb.y - mb.y)).abs() < 1e-6, "on the hovered row");
    click(&mut rt, target, M, 310.0);
    assert!(!used(&rt, 3), "deleted from the menu");
}

#[test]
fn the_graph_menu_adds_a_band_where_it_opened() {
    let mut rt = eq();
    let at = point(&rt, 700.0, -3.0);
    rt.context_click(at.0, at.1, 0.0);
    assert_eq!(rt.open_menus(), [named(&rt, "graph menu")]);
    rt.key(Key::Down, M, 10.0);
    rt.key(Key::Enter, M, 20.0);
    assert!(used(&rt, 6));
    assert!((real(&rt, "eq/b6_freq") - 700.0).abs() < 15.0 && (real(&rt, "eq/b6_gain") + 3.0).abs() < 0.3);
}

#[test]
fn the_range_switch_zooms_handles_and_curves() {
    let mut rt = eq();
    let y0 = at_handle(&rt, 3).1;
    let range = named(&rt, "range");
    let rb = rt.hit_box(range).unwrap();
    click(&mut rt, centre(rb), M, 0.0);
    assert_eq!(real(&rt, "eq/range"), 3.0, "±12 → ±18");
    let (_, cb) = canvas(&rt);
    let mid = cb.y + cb.height / 2.0;
    let y1 = at_handle(&rt, 3).1;
    assert!(((y1 - mid) / (y0 - mid) - 12.0 / 18.0).abs() < 0.02, "the node moves toward 0 dB: {y0} → {y1}");
    let frame = rt.frame(0.0);
    let label = frame.nodes.values().filter_map(|n| n.text()).any(|t| t.characters == "+18");
    assert!(label, "the scale reads ±18");
    // Double-click resets the range.
    double_click(&mut rt, centre(rb), 1000.0);
    assert_eq!(real(&rt, "eq/range"), 2.0);
}

#[test]
fn hover_shows_the_panel_and_a_halo() {
    let mut rt = eq();
    let at = at_handle(&rt, 2);
    rt.pointer(Input::Move { x: at.0, y: at.1, t: 0.0, mods: M }, 0.0);
    assert_eq!(rt.hovered(), Some(handle(&rt, 2)));
    assert!(rt.popover_origin(named(&rt, "band panel")).is_some(), "hovering a band shows its panel");
    let frame = rt.frame(0.0);
    let halo = frame.nodes.values().filter(|n| n.name == "hover" && n.visible && n.parent.is_some_and(|p| p == handle(&rt, 2))).count();
    assert_eq!(halo, 1);
    rt.pointer(Input::Move { x: 5.0, y: 5.0, t: 16.0, mods: M }, 16.0);
    assert!(rt.popover_origin(named(&rt, "band panel")).is_none());
}

#[test]
fn the_graph_lints_clean_and_round_trips_through_markup() {
    let rt = eq();
    // The authored design (templates, not instances).
    let mut scene = Scene::new("eq");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        place(&mut b, page, "viz.eq_graph", &Place::new("EQ").size(820.0).options(&["ns=eq"])).unwrap();
    }
    let authored = Surface { scene, controls }.to_scene();
    let text = document_text::print(&authored);
    let back = document_text::parse(&text, authored.assets.clone()).unwrap();
    assert_eq!(back, authored, "parse(print(s)) == s");
    let issues = design_lint::lint(&authored, rt.catalog(), &design_lint::Config::default());
    let errors: Vec<String> = issues.iter().filter(|i| i.severity == design_lint::Severity::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let sig = library_proto_sig(&authored);
    assert!(sig.contains(&"eq/b24_gain".to_string()) && !sig.iter().any(|a| a.contains("{n}")), "a repeat binds every item");
}

fn named_node(rt: &Runtime, name: &str) -> NodeId {
    rt.surface().scene.nodes.values().find(|n| n.name == name).map(|n| n.id).unwrap_or_else(|| panic!("no node `{name}`"))
}

/// A node's layout box (from a fresh layout of the runtime's scene).
fn box_of(rt: &Runtime, id: NodeId) -> Bounds {
    let s = &rt.surface().scene;
    let b = render::build_scene_layout_boxes(s, s.pages()[0])[&id];
    Bounds { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn library_proto_sig(scene: &Scene) -> Vec<String> {
    library_proto::Signature::of_scene(scene).params
}

/// A fixed EQ (`fixed`, `shapes=3`, `db_range=12`, `tint=`): its bands are
/// always there — nothing adds, removes, bypasses or renames one — and it
/// draws in one colour at a fixed ±12 dB with no range picker.
#[test]
fn a_fixed_eq_has_its_bands_and_nothing_to_add_or_remove() {
    let mut scene = Scene::new("eq");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let p = Place::new("EQ").size(480.0).options(&["ns=sat/emph", "bands=6", "fixed", "shapes=3", "db_range=12", "tint=#ff9a3c", "h=150"]);
        place(&mut b, page, "viz.eq_graph", &p).unwrap();
    }
    let named = |name: &str| scene.walk(page).into_iter().find(|id| scene.node(*id).is_some_and(|n| n.name == name));
    for c in controls.values() {
        match &c.kind {
            ControlKind::Repeat { count, used, .. } => {
                assert_eq!(*count, 6);
                assert!(used.is_none(), "a fixed band is always used");
            }
            ControlKind::Canvas { add, .. } => assert!(add.is_none(), "nothing adds a band"),
            ControlKind::Handle { .. } => {
                let i = c.interact.as_ref().unwrap();
                assert!(!i.removable, "nothing removes a band");
                assert!(c.zoom.is_none(), "a fixed span does not zoom");
                assert_eq!(c.tint, vec!["#ff9a3c".to_string(); 2]);
            }
            _ => {}
        }
        if let Some(b) = &c.binding {
            let addr = format!("{b:?}");
            assert!(!["_used", "_on", "_name", "_slope", "_placement", "_focus", "_dyn_range", "/range"].iter().any(|k| addr.contains(k)), "binds {addr}");
        }
    }
    assert!(named("graph menu").is_none() && named("band {n} label").is_none());
    let range = named("range").unwrap();
    assert!(!scene.node(range).unwrap().visible, "no range picker");
}
