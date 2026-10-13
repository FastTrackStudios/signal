//! Faces that move: the spring tank rings with the input level and dies
//! away by its params; the room redraws with its params; reels turn; the
//! valves flicker; the pedals' lamps pulse even without a stream.

use document_proto::{NodeId, Scene};
use plugin_faces::Face;
use surface_proto::{EntryId, ParamAddr, ParamValue, StreamFrame};
use surface_runtime::{Runtime, Surface};

fn face(file: &str) -> (Face, surface_proto::Catalog) {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    let f = all.into_iter().find(|f| f.file == file && (file != "02-tape" || f.plugin == "saturate")).unwrap();
    let c = catalogs[f.plugin].clone();
    (f, c)
}

fn named(scene: &Scene, name: &str) -> Vec<NodeId> {
    scene.walk(scene.root).into_iter().filter(|id| scene.node(*id).is_some_and(|n| n.name == name)).collect()
}

fn set(rt: &mut Runtime, addr: &str, v: f64) {
    rt.apply_values([ParamValue { addr: ParamAddr::new(addr), value: v }]);
}

fn level(rt: &mut Runtime, id: &str, db: f64, t: f64) {
    rt.apply_frame(StreamFrame { id: EntryId::new(id), values: vec![db, db] }, t);
}

/// How far the first spring's front coil is from its resting outline
/// (max |Δ| over the path's numbers).
fn deflection(rt: &Runtime, rest: &str, coil: NodeId, t: f64) -> f64 {
    let f = rt.frame(t);
    let now = &f.node(coil).unwrap().fill_geometry[0];
    let nums = |s: &str| s.split_whitespace().filter_map(|x| x.parse::<f64>().ok()).collect::<Vec<_>>();
    nums(now).iter().zip(nums(rest)).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)
}

fn ring(dwell: f64, damping: f64) -> Vec<f64> {
    let (f, c) = face("05-spring");
    let coil = named(&f.scene, "coil front")[0];
    let rest = f.scene.node(coil).unwrap().fill_geometry[0].clone();
    let mut rt = Runtime::new(Surface::from_scene(f.scene), c);
    set(&mut rt, "spring/dwell", dwell);
    set(&mut rt, "spring/damping", damping);
    let mut out = Vec::new();
    for k in 0..=120 {
        let t = f64::from(k) * 1000.0 / 30.0;
        // Silence, a burst at 0.2 s, silence again from 0.5 s.
        let db = if (0.2..0.5).contains(&(t / 1000.0)) { -6.0 } else { -90.0 };
        level(&mut rt, "spring/in", db, t);
        out.push(deflection(&rt, &rest, coil, t));
    }
    out
}

#[test]
fn the_spring_tank_rings_on_the_input_and_decays_by_its_params() {
    let long = ring(0.9, 0.1);
    let short = ring(0.2, 0.9);
    let at = |v: &[f64], s: f64| v[(s * 30.0) as usize];
    let peak = |v: &[f64], a: f64, b: f64| v[(a * 30.0) as usize..(b * 30.0) as usize].iter().copied().fold(0.0, f64::max);
    assert!(at(&long, 0.15) < 1e-6, "still before the burst");
    assert!(peak(&long, 0.2, 0.6) > 5.0, "struck: bends by px ({})", peak(&long, 0.2, 0.6));
    assert!(peak(&long, 0.2, 0.6) > 1.5 * peak(&short, 0.2, 0.6), "more dwell, harder strike");
    assert!(peak(&long, 1.2, 2.0) > 0.5, "a long dwell still rings a second later ({})", peak(&long, 1.2, 2.0));
    assert!(peak(&short, 1.5, 4.0) < 0.05, "a short one has died away ({})", peak(&short, 1.5, 4.0));
}

#[test]
fn the_room_redraws_with_its_params() {
    let (f, c) = face("04-room");
    let back = named(&f.scene, "back wall")[0];
    let rays = named(&f.scene, "rays")[0];
    let mut rt = Runtime::new(Surface::from_scene(f.scene.clone()), c);
    let draw = |rt: &Runtime, id: NodeId, t: f64| rt.frame(t).node(id).unwrap().fill_geometry[0].clone();
    let rest_back = draw(&rt, back, 0.0);
    assert_eq!(rest_back, f.scene.node(back).unwrap().fill_geometry[0], "at rest the room is the authored one");
    set(&mut rt, "room/size", 1.0);
    set(&mut rt, "room/diffusion", 0.0);
    draw(&rt, back, 16.0);
    let mid = draw(&rt, back, 100.0);
    assert_ne!(mid, rest_back, "the back wall is moving");
    assert!(rt.is_animating());
    for k in 1..=60 {
        let _ = rt.frame(100.0 + 33.0 * f64::from(k));
    }
    assert!(!rt.is_animating(), "and settles");
    assert_eq!(draw(&rt, rays, 2200.0).matches('M').count(), 2, "diffusion 0: two reflections");
}

#[test]
fn reels_turn_valves_flicker_and_lamps_pulse() {
    // Tape machine reels: turning with the speed switch.
    let (f, c) = face("02-tape");
    let reel = named(&f.scene, "reel turning")[0];
    let rt = Runtime::new(Surface::from_scene(f.scene), c);
    let a = |t: f64| rt.frame(t).node(reel).unwrap().transform.rotation();
    let (a0, a1) = (a(0.0), a(100.0));
    assert!((a1 - a0).abs() > 1e-3 && rt.is_animating(), "turning");

    // The valve preamp: the heaters waver.
    let (f, c) = face("01-tube");
    let heater = named(&f.scene, "heater")[0];
    let rt = Runtime::new(Surface::from_scene(f.scene), c);
    let o: Vec<f64> = (0..10).map(|k| rt.frame(f64::from(k) * 50.0).node(heater).unwrap().opacity).collect();
    assert!(o.iter().any(|x| (x - o[0]).abs() > 1e-3), "{o:?}");

    // A pedal's rate lamp pulses at its rate with no LFO stream.
    let (f, c) = face("01-chorus");
    let rt = Runtime::new(Surface::from_scene(f.scene.clone()), c);
    let lamp = f.scene.nodes.values().find(|n| n.name == "lamp" && !n.motion.is_empty()).map(|n| n.id).unwrap();
    let o: Vec<f64> = (0..20).map(|k| rt.frame(f64::from(k) * 50.0).node(lamp).unwrap().opacity).collect();
    let (lo, hi) = o.iter().fold((1.0_f64, 0.0_f64), |(l, h), x| (l.min(*x), h.max(*x)));
    assert!(hi - lo > 0.3, "pulsing: {o:?}");
}
