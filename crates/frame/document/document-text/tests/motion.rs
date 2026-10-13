//! `motion` round-trips through markup: parse(print(s)) == s, and the
//! printed text is stable (print(parse(text)) == text).

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};

#[test]
fn motion_round_trips() {
    let src = r##"<Frame name="Tank" w=200 h=80>
  <Vector name="Coil" w=200 h=80 geometry="M0 40 L200 40" motion={[{"name":"ring","target":"none","dynamics":{"oscillator":{"freq":"4 + 6*norm(spring/tension)","decay":"0.3 + 3*norm(spring/dwell)","excite":"level(spring/in)","gain":2}}},{"target":{"shape":{"provider":"coil","part":"front","inputs":{"bend":"motion(ring)*8","loops":30}}}}]} />
  <Ellipse name="Reel" w=60 h=60 motion={{"target":"rotate","input":"0.4 + norm(tape/speed)","dynamics":"spin","to":360}} />
  <Ellipse name="Lamp" w=10 h=10 motion={[{"target":"opacity","dynamics":{"lfo":{"rate":"param(chorus/rate)","shape":"triangle"}},"from":0.2}]} />
</Frame>"##;
    let nodes = document_text::parse_fragment(src).unwrap();
    let coil = nodes.iter().find(|n| n.name == "Coil").unwrap();
    assert_eq!(coil.motion.len(), 2);
    assert_eq!(coil.motion[0].name.as_deref(), Some("ring"));
    assert!(matches!(&coil.motion[1].target, Target::Shape { provider, .. } if provider == "coil"));
    let reel = nodes.iter().find(|n| n.name == "Reel").unwrap();
    assert_eq!(reel.motion, vec![Motion::new(Target::Rotate).input("0.4 + norm(tape/speed)").dynamics(Dynamics::Spin).range(0.0, 360.0)]);
    let lamp = nodes.iter().find(|n| n.name == "Lamp").unwrap();
    assert_eq!(lamp.motion[0].dynamics, Dynamics::Lfo { rate: Num::Expr("param(chorus/rate)".into()), shape: Wave::Triangle, phase: 0.0 });

    let mut scene = document_proto::Scene::new("p");
    let page = scene.pages()[0];
    let ops: Vec<document_proto::Op> = nodes.into_iter().map(|n| {
        let parent = n.parent.unwrap_or(page);
        document_proto::Op::Insert { node: n, parent, index: u32::MAX }
    }).collect();
    scene.transact(ops).unwrap();
    let text = document_text::print(&scene);
    let back = document_text::parse(&text, scene.assets.clone()).unwrap();
    assert_eq!(back, scene);
    assert_eq!(document_text::print(&back), text);
    // Nodes without motion print no attribute.
    assert_eq!(text.matches("motion=").count(), 3);
}
