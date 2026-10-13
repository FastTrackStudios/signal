//! Rule `motion`: broken motion sources are errors a model can fix.

use design_lint::{Config, Severity, lint};
use document_proto::motion::{Dynamics, Motion, Num, Target};
use document_proto::{Node, NodeKind, Op, Scene, Size};
use surface_proto::{Catalog, EntryId, ParamInfo, Range, StreamInfo, StreamKind, Unit};

fn catalog() -> Catalog {
    Catalog {
        params: vec![ParamInfo::new("room/size", "Size", Range::default())],
        streams: vec![
            StreamInfo { id: EntryId::new("room/in"), name: "In".into(), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 },
            StreamInfo { id: EntryId::new("room/lfo"), name: "LFO".into(), kind: StreamKind::Scalar { range: Range::linear(0.0, 1.0, Unit::None) }, rate_hz: 30.0 },
        ],
        ..Catalog::default()
    }
}

fn issues(kind: NodeKind, motion: Vec<Motion>) -> Vec<design_lint::Issue> {
    let mut scene = Scene::new("p");
    let page = scene.pages()[0];
    let mut n = Node::new(kind, "part");
    n.size = Size { width: 10.0, height: 10.0 };
    n.motion = motion;
    scene.apply(Op::Insert { node: n, parent: page, index: 0 }).unwrap();
    lint(&scene, &catalog(), &Config::default()).into_iter().filter(|i| i.rule == "motion").collect()
}

#[test]
fn good_motions_are_clean() {
    let ms = vec![
        Motion::new(Target::None).named("ring").dynamics(Dynamics::Oscillator { freq: Num::Value(4.0), decay: Some(Num::Expr("1 + norm(room/size)".into())), damping: None, excite: "level(room/in, 1)".into(), gain: Num::Value(1.0) }),
        Motion::new(Target::Y).input("motion(ring) * 4 + scalar(room/lfo)"),
        Motion::new(Target::Shape { provider: "reverb.room".into(), part: Some("rays".into()), inputs: Default::default() }),
    ];
    let found = issues(NodeKind::Frame, ms);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn broken_sources_are_errors_with_fixes() {
    let ms = vec![
        Motion::new(Target::Rotate).input("norm(room/sise)"),
        Motion::new(Target::Y).input("level(room/lfo)"),
        Motion::new(Target::X).input("motion(rng)"),
        Motion::new(Target::Opacity).input("clamp(1, 2"),
        Motion::new(Target::X).input("level(room/out)"),
    ];
    let found = issues(NodeKind::Frame, ms);
    let errors: Vec<String> = found.iter().filter(|i| i.severity == Severity::Error).map(ToString::to_string).collect();
    assert_eq!(errors.len(), 5, "{errors:#?}");
    assert!(errors[0].contains("room/sise") && errors[0].contains("did you mean `room/size`"), "{}", errors[0]);
    assert!(errors.iter().any(|e| e.contains("doesn't fit feed `room/lfo`")));
    assert!(errors.iter().any(|e| e.contains("motion `rng`")));
    assert!(errors.iter().any(|e| e.contains("no feed `room/out`") && e.contains("did you mean `room/in`")));
}

#[test]
fn unknown_providers_and_moved_groups_warn() {
    let found = issues(NodeKind::Group, vec![
        Motion::new(Target::Rotate).input("1"),
        Motion::new(Target::Shape { provider: "coyl".into(), part: None, inputs: Default::default() }),
    ]);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|i| i.severity == Severity::Warning));
    assert!(found.iter().any(|i| i.fix.contains("did you mean `coil`")));
}
