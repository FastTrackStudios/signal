use std::collections::HashMap;

use crate::document::proto::{Node, NodeId, NodeKind, Op, Scene, Size};
use crate::motion::proto::{Dynamics, FeedRead, Motion, Num, Source, Target, Wave};

use super::*;

fn scene_with(motions: Vec<Motion>) -> (Scene, NodeId) {
    let mut scene = Scene::new("p");
    let page = scene.pages()[0];
    let mut n = Node::new(NodeKind::Rectangle, "part");
    n.size = Size {
        width: 20.0,
        height: 40.0,
    };
    n.motion = motions;
    let id = n.id;
    scene
        .apply(Op::Insert {
            node: n,
            parent: page,
            index: 0,
        })
        .unwrap();
    (scene, id)
}

fn param(addr: &str) -> Source {
    Source::Param {
        addr: addr.into(),
        normalized: true,
    }
}

fn level(id: &str) -> Source {
    Source::Feed {
        id: id.into(),
        read: FeedRead::Level(None),
    }
}

/// Run from `t0` to `t1` at `fps`, returning (t, output) per frame.
fn run(
    a: &mut Animator,
    inputs: &HashMap<Source, f64>,
    node: NodeId,
    t0: f64,
    t1: f64,
    fps: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let n = ((t1 - t0) * fps).round() as usize;
    for k in 0..=n {
        let t = t0 + k as f64 / fps;
        a.advance(t, inputs);
        out.push((t, a.output(node, 0).unwrap_or(f64::NAN)));
    }
    out
}

#[test]
fn a_spring_follows_its_param_and_settles() {
    let (scene, id) = scene_with(vec![
        Motion::new(Target::Rotate)
            .input("norm(k/pos)")
            .dynamics(Dynamics::spring(200.0, 6.0))
            .range(0.0, 100.0),
    ]);
    let mut a = Animator::new(&scene);
    let mut inputs = HashMap::from([(param("k/pos"), 0.0)]);
    a.advance(0.0, &inputs);
    assert_eq!(a.output(id, 0), Some(0.0), "starts at rest on its input");
    assert!(!a.is_animating());
    inputs.insert(param("k/pos"), 1.0);
    let trace = run(&mut a, &inputs, id, 1.0 / 60.0, 3.0, 60.0);
    let peak = trace.iter().map(|(_, v)| *v).fold(f64::MIN, f64::max);
    assert!(peak > 105.0, "under-damped: overshoots ({peak})");
    let last = trace.last().unwrap().1;
    assert!((last - 100.0).abs() < 0.5, "settles on the target: {last}");
    for t in [4.0, 6.0, 8.0] {
        a.advance(t, &inputs);
    }
    assert!(!a.is_animating(), "a settled spring lets the host sleep");
    assert_eq!(a.output(id, 0), Some(100.0));
}

#[test]
fn integration_is_frame_rate_independent() {
    let motion = Motion::new(Target::X)
        .input("norm(k/pos)")
        .dynamics(Dynamics::spring(300.0, 8.0));
    let at = |fps: f64| {
        let (scene, id) = scene_with(vec![motion.clone()]);
        let mut a = Animator::new(&scene);
        let mut inputs = HashMap::from([(param("k/pos"), 0.0)]);
        a.advance(0.0, &inputs);
        // Keep it moving so no frame is skipped as "at rest".
        inputs.insert(param("k/pos"), 1.0);
        a.advance(0.0, &inputs);
        let mut last = 0.0;
        // 0.35 s lands on a whole frame at every rate tried.
        let n = (0.35 * fps).round() as usize;
        for k in 1..=n {
            a.advance(k as f64 / fps, &inputs);
            last = a.output(id, 0).unwrap();
        }
        last
    };
    let (a30, a60, a240) = (at(40.0), at(60.0), at(240.0));
    assert!(
        (a30 - a240).abs() < 0.02 && (a60 - a240).abs() < 0.02,
        "{a30} {a60} {a240}"
    );
}

#[test]
fn a_stall_is_capped_and_stays_stable() {
    let (scene, id) = scene_with(vec![
        Motion::new(Target::X)
            .input("norm(k/pos)")
            .dynamics(Dynamics::spring(5000.0, 5.0)),
    ]);
    let mut a = Animator::new(&scene);
    let mut inputs = HashMap::from([(param("k/pos"), 0.0)]);
    a.advance(0.0, &inputs);
    inputs.insert(param("k/pos"), 1.0);
    a.advance(0.001, &inputs);
    a.advance(0.017, &inputs);
    a.advance(30.0, &inputs); // a 30 s stall
    let v = a.output(id, 0).unwrap();
    assert!(v.is_finite() && v.abs() < 3.0, "{v}");
}

/// The reverb spring: rest at 0, struck by rises in the input level, ringing
/// at `freq` and dying away over `decay` seconds (to −60 dB).
fn ring(decay: &str) -> Motion {
    Motion::new(Target::Y).dynamics(Dynamics::Oscillator {
        freq: Num::Value(5.0),
        decay: Some(Num::Expr(decay.into())),
        damping: None,
        excite: "level(spring/in)".into(),
        gain: Num::Value(1.0),
    })
}

/// Peak |output| in `[t0, t1)` of a trace.
fn peak(trace: &[(f64, f64)], t0: f64, t1: f64) -> f64 {
    trace
        .iter()
        .filter(|(t, _)| *t >= t0 && *t < t1)
        .map(|(_, v)| v.abs())
        .fold(0.0, f64::max)
}

fn strike(decay_param: f64) -> Vec<(f64, f64)> {
    let (scene, id) = scene_with(vec![ring("0.2 + 3.8*norm(spring/decay)")]);
    let mut a = Animator::new(&scene);
    let mut inputs = HashMap::from([
        (level("spring/in"), 0.0),
        (param("spring/decay"), decay_param),
    ]);
    let mut trace = run(&mut a, &inputs, id, 0.0, 0.5, 60.0);
    assert!(peak(&trace, 0.0, 0.5) < 1e-9, "silent input: still");
    assert!(!a.is_animating());
    // A level step: struck once, then the level holds (no further rises).
    inputs.insert(level("spring/in"), 0.8);
    trace.extend(run(&mut a, &inputs, id, 0.5 + 1.0 / 60.0, 4.0, 60.0));
    trace
}

#[test]
fn a_spring_rings_on_a_level_step_and_decays_per_the_decay_param() {
    let short = strike(0.1); // T60 = 0.58 s
    let long = strike(0.6); // T60 = 2.48 s
    // It rings: the output crosses zero repeatedly at about 5 Hz.
    let after: Vec<f64> = long
        .iter()
        .filter(|(t, _)| *t > 0.5 && *t < 1.5)
        .map(|(_, v)| *v)
        .collect();
    let crossings = after
        .windows(2)
        .filter(|w| w[0].signum() != w[1].signum() && w[0] != 0.0)
        .count();
    assert!(
        (9..=11).contains(&crossings),
        "5 Hz: ~10 zero crossings a second: {crossings}"
    );
    // Amplitude ≈ gain × the rise.
    let first = peak(&long, 0.5, 0.8);
    assert!(
        (0.55..=0.85).contains(&first),
        "struck by the 0.8 rise: {first}"
    );
    // Decay: amplitude falls by 60 dB over T60 — measured on the envelope.
    let t60 = |p: f64| 0.2 + 3.8 * p;
    for (trace, p) in [(&short, 0.1), (&long, 0.6)] {
        let a0 = peak(trace, 0.5, 0.75);
        let later = 0.5 + t60(p) / 3.0; // one third of T60 → −20 dB
        let a1 = peak(trace, later, later + 0.25);
        let db = 20.0 * (a1 / a0).log10();
        assert!(
            (-26.0..=-14.0).contains(&db),
            "decay param {p}: {db:.1} dB at T60/3"
        );
    }
    assert!(
        peak(&short, 1.5, 2.0) < 0.2 * peak(&long, 1.5, 2.0),
        "a shorter decay dies sooner"
    );
    assert!(peak(&short, 3.5, 4.0) < 1e-3, "and settles");
}

#[test]
fn a_named_motion_feeds_another_and_order_does_not_matter() {
    // The reader is listed before the signal it reads.
    let reader = Motion::new(Target::X).input("motion(sig) * 10");
    let sig = Motion::new(Target::None).named("sig").input("norm(a/b)");
    let (scene, id) = scene_with(vec![reader, sig]);
    let mut a = Animator::new(&scene);
    a.advance(0.0, &HashMap::from([(param("a/b"), 0.25)]));
    assert_eq!(a.output(id, 0), Some(2.5));
}

#[test]
fn time_driven_dynamics_are_deterministic_from_any_start() {
    let m = Motion::new(Target::Opacity).dynamics(Dynamics::Lfo {
        rate: Num::Value(1.5),
        shape: Wave::Sine,
        phase: 0.0,
    });
    let (scene, id) = scene_with(vec![m]);
    let mut stepped = Animator::new(&scene);
    run(&mut stepped, &HashMap::new(), id, 0.0, 1.3, 50.0);
    let mut single = Animator::new(&scene);
    single.advance(1.3, &());
    let (x, y) = (
        stepped.output(id, 0).unwrap(),
        single.output(id, 0).unwrap(),
    );
    assert!((x - y).abs() < 1e-9, "{x} vs {y}");
    assert!(stepped.is_animating());
    let spin = Motion::new(Target::Rotate)
        .input("2")
        .dynamics(Dynamics::Spin)
        .range(0.0, 360.0);
    let (scene, id) = scene_with(vec![spin]);
    let mut a = Animator::new(&scene);
    a.advance(0.1, &());
    assert!(
        (a.output(id, 0).unwrap() - 72.0).abs() < 1e-9,
        "2 turns/s × 0.1 s"
    );
}

#[test]
fn state_survives_a_swap_for_motions_still_there() {
    let (scene, id) = scene_with(vec![ring("2")]);
    let mut a = Animator::new(&scene);
    let mut inputs = HashMap::from([(level("spring/in"), 0.0)]);
    a.advance(0.0, &inputs);
    inputs.insert(level("spring/in"), 1.0);
    let trace = run(&mut a, &inputs, id, 0.02, 0.3, 50.0);
    let (t, before) = *trace.last().unwrap();
    assert!(before.abs() > 1e-3, "ringing");
    // The document is edited: the node gains a second motion and moves.
    let mut edited = scene.clone();
    let n = edited.node_mut(id).unwrap();
    n.transform.tx = 40.0;
    n.motion.push(Motion::new(Target::Opacity).input("0.5"));
    a.swap(&edited);
    a.advance(t, &inputs);
    assert_eq!(a.output(id, 0), Some(before), "same time, same state");
    a.advance(t + 0.02, &inputs);
    assert!(a.is_animating(), "still ringing after the swap");
    // A motion whose dynamics changed kind starts fresh.
    let mut changed = edited.clone();
    changed.node_mut(id).unwrap().motion[0].dynamics = Dynamics::Follow;
    a.swap(&changed);
    a.advance(t + 0.04, &inputs);
    assert_eq!(a.output(id, 0), Some(0.0));
}

#[test]
fn a_source_that_appears_snaps_to_rest() {
    let (scene, id) = scene_with(vec![
        Motion::new(Target::Opacity)
            .input("scalar(x/lfo)")
            .dynamics(Dynamics::spring(100.0, 20.0)),
    ]);
    let mut a = Animator::new(&scene);
    let src = Source::Feed {
        id: "x/lfo".into(),
        read: FeedRead::Scalar,
    };
    a.advance(0.0, &());
    assert_eq!(a.output(id, 0), None, "no data yet: nothing to show");
    a.advance(0.0, &HashMap::from([(src.clone(), 0.5)]));
    assert_eq!(a.output(id, 0), Some(0.5));
    let mut s = scene.clone();
    a.apply(&mut s);
    assert_eq!(s.node(id).unwrap().opacity, 0.5);
}

#[test]
fn targets_write_the_render_copy() {
    let (mut scene, id) = scene_with(vec![
        Motion::new(Target::Rotate).input("90").pivot(0.0, 0.0),
        Motion::new(Target::X).input("5"),
        Motion::new(Target::Fill {
            colors: vec!["#000000".into(), "#ffffff".into()],
        })
        .input("0.5"),
        Motion::new(Target::Morph {
            shapes: vec!["M0 0 L10 0".into(), "M0 10 L10 10".into()],
        })
        .input("0.5"),
    ]);
    let mut a = Animator::new(&scene);
    a.advance(0.0, &());
    a.apply(&mut scene);
    let n = scene.node(id).unwrap();
    assert!((n.transform.rotation().to_degrees() - 90.0).abs() < 1e-9);
    assert!(
        (n.transform.tx - 5.0).abs() < 1e-9 && n.transform.ty.abs() < 1e-9,
        "turned about its corner, then moved"
    );
    let crate::document::proto::PaintKind::Solid { color } = &n.fills[0].kind else {
        panic!()
    };
    assert!((color.r - 0.5).abs() < 1e-3);
    assert_eq!(n.fill_geometry, ["M0.000 5.000 L10.000 5.000"]);
    assert!(matches!(n.kind, NodeKind::Vector));
}

#[test]
fn a_shape_provider_redraws_with_its_inputs() {
    let m = Motion::new(Target::Shape {
        provider: "reverb.room".into(),
        part: Some("rays".into()),
        inputs: [(
            "diffusion".to_string(),
            Num::Expr("norm(room/diffusion)".into()),
        )]
        .into_iter()
        .collect(),
    })
    .input("0");
    let (scene, id) = scene_with(vec![m]);
    let mut a = Animator::new(&scene);
    let draw = |a: &mut Animator, d: f64| {
        a.advance(0.0, &HashMap::from([(param("room/diffusion"), d)]));
        let mut s = scene.clone();
        a.apply(&mut s);
        s.node(id).unwrap().fill_geometry[0].matches('M').count()
    };
    assert_eq!(draw(&mut a, 0.0), 2);
    assert_eq!(draw(&mut a, 1.0), 8);
}

#[test]
fn two_hundred_motions_are_cheap() {
    let mut scene = Scene::new("p");
    let page = scene.pages()[0];
    let mut ids = Vec::new();
    for i in 0..50 {
        let mut n = Node::new(NodeKind::Vector, format!("n{i}"));
        n.size = Size {
            width: 20.0,
            height: 20.0,
        };
        n.emission = Some(crate::document::proto::Emission {
            color: crate::document::proto::Color::rgba(1.0, 0.5, 0.2, 1.0),
            intensity: 1.0,
            radius: 4.0,
        });
        n.motion = vec![
            Motion::new(Target::Rotate)
                .input("norm(p/a)")
                .dynamics(Dynamics::spring(200.0, 8.0))
                .range(-30.0, 30.0),
            Motion::new(Target::Y).dynamics(Dynamics::Oscillator {
                freq: Num::Expr("3 + 4*norm(p/b)".into()),
                decay: Some(Num::Value(2.0)),
                damping: None,
                excite: "level(p/in)".into(),
                gain: Num::Value(4.0),
            }),
            Motion::new(Target::Emission)
                .input("0.8 + 0.4*norm(p/c)")
                .dynamics(Dynamics::Noise {
                    rate: Num::Value(9.0),
                    amount: Num::Value(0.2),
                }),
            Motion::new(Target::Opacity).dynamics(Dynamics::Lfo {
                rate: Num::Expr("param(p/rate)".into()),
                shape: Wave::Triangle,
                phase: 0.0,
            }),
        ];
        ids.push(n.id);
        scene
            .apply(Op::Insert {
                node: n,
                parent: page,
                index: u32::MAX,
            })
            .unwrap();
    }
    let mut a = Animator::new(&scene);
    assert_eq!(a.len(), 200);
    let mut inputs = HashMap::from([
        (param("p/a"), 0.3),
        (param("p/b"), 0.5),
        (param("p/c"), 0.5),
        (
            Source::Param {
                addr: "p/rate".into(),
                normalized: false,
            },
            2.0,
        ),
        (level("p/in"), 0.0),
    ]);
    a.advance(0.0, &inputs);
    let mut render = scene.clone();
    let frames = 600;
    let start = std::time::Instant::now();
    for k in 1..=frames {
        let t = f64::from(k) / 60.0;
        inputs.insert(level("p/in"), if k % 30 < 5 { 0.9 } else { 0.1 });
        inputs.insert(param("p/a"), if k % 60 < 30 { 0.2 } else { 0.8 });
        a.advance(t, &inputs);
        a.apply(&mut render);
    }
    let per = start.elapsed().as_secs_f64() / f64::from(frames) * 1e3;
    println!("200 motions: {per:.4} ms per frame (advance + apply, 60 Hz)");
    // Debug builds are ~10× slower than release; the release figure is
    // what the ≤ 0.1 ms budget is about (see the docs for measurements).
    assert!(
        per < if cfg!(debug_assertions) { 3.0 } else { 0.1 },
        "{per} ms"
    );
}
