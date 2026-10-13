//! `emission` and `shader` round-trip through markup.

#[test]
fn emission_and_shader_round_trip() {
    let src = r##"<Frame name="Lamp" w=40 h=40>
  <Ellipse name="Jewel" w=24 h=24 fill="#e33a2a" emission={{"color":{"r":1,"g":0.3,"b":0.2,"a":1},"intensity":1.6,"radius":14}} />
  <Frame name="Glass" w=40 h=40 shader={{"shader":"glass","params":{"strength":0.6}}} />
</Frame>"##;
    let nodes = document_text::parse_fragment(src).unwrap();
    let jewel = nodes.iter().find(|n| n.name == "Jewel").unwrap();
    let e = jewel.emission.as_ref().unwrap();
    assert_eq!((e.intensity, e.radius), (1.6, 14.0));
    let glass = nodes.iter().find(|n| n.name == "Glass").unwrap();
    assert_eq!(glass.shader.as_ref().unwrap().params["strength"], 0.6);

    let mut scene = document_proto::Scene::new("p");
    let page = scene.pages()[0];
    let mut ops = Vec::new();
    for n in nodes {
        let parent = n.parent.unwrap_or(page);
        ops.push(document_proto::Op::Insert { node: n, parent, index: u32::MAX });
    }
    scene.transact(ops).unwrap();
    let back = document_text::parse(&document_text::print(&scene), scene.assets.clone()).unwrap();
    assert_eq!(back, scene);
}
