//! Tokens and bindings survive the text form.

use std::collections::BTreeMap;

use document_proto::{ResizeY, Scene};

const SRC: &str = r##"<Document id="00000000-0000-0000-0000-000000000001">
  <Variables modes="dark light">
    <Var name="strip.ground" value="#1c1c1c" light="#ececec" />
    <Var name="strip.ink" dark="#e0e0e0" light="#202020" />
    <Var name="pad" value="4" />
  </Variables>
  <Page name="mcp.strip" id="00000000-0000-0000-0000-000000000002">
    <Frame name="Strip" id="00000000-0000-0000-0000-000000000003" w=86 h=400 bind:fill="$strip.ground">
      <Text name="Name" id="00000000-0000-0000-0000-000000000004" y=380 w=86 h=16 resize-y="bottom" bind:visible="w >= 40" bind:text="track.name">Vocal</Text>
    </Frame>
  </Page>
</Document>
"##;

fn scene() -> Scene {
    document_text::parse(SRC, BTreeMap::new()).expect("parses")
}

#[test]
fn variables_are_read_with_their_modes() {
    let s = scene();
    assert_eq!(s.variables.modes, ["dark", "light"]);
    let ground = &s.variables.vars["strip.ground"];
    assert_eq!(ground.in_mode("dark"), "#1c1c1c");
    assert_eq!(ground.in_mode("light"), "#ececec");
    // No value=: the first mode's stands in.
    assert_eq!(s.variables.vars["strip.ink"].value, "#e0e0e0");
    assert_eq!(s.variables.vars["pad"].in_mode("light"), "4");
}

#[test]
fn bindings_and_vertical_resizing_are_read() {
    let s = scene();
    let name = s.nodes.values().find(|n| n.name == "Name").expect("Name");
    let binds: Vec<(&str, &str)> = name
        .bind
        .iter()
        .map(|b| (b.prop.as_str(), b.expr.as_str()))
        .collect();
    assert_eq!(binds, [("visible", "w >= 40"), ("text", "track.name")]);
    assert_eq!(name.layout_child.resize_y, Some(ResizeY::Bottom));
}

#[test]
fn the_text_form_round_trips() {
    let s = scene();
    let printed = document_text::print(&s);
    let again = document_text::parse(&printed, BTreeMap::new()).expect("reparses");
    assert_eq!(s, again, "{printed}");
    assert!(
        printed.contains("<Variables modes=\"dark light\">"),
        "{printed}"
    );
}

#[test]
fn variables_outside_the_document_are_refused() {
    let src = r#"<Document><Page><Variables modes="dark" /></Page></Document>"#;
    let err = document_text::parse(src, BTreeMap::new()).unwrap_err();
    assert!(
        err.to_string()
            .contains("belongs directly inside <Document>"),
        "{err}"
    );
}

#[test]
fn an_unknown_binding_is_refused() {
    let src = r#"<Document><Page><Frame bind:colour="$x" /></Page></Document>"#;
    assert!(document_text::parse(src, BTreeMap::new()).is_err());
}

const COMPONENTS: &str = r##"<Document>
  <Page name="controls">
    <Component name="button" w=21 h=20 arg:on="false" arg:label="\"M\"" arg:control="\"\"">
      <Rectangle w=21 h=20 bind:fill="arg.on ? $mute : $hardware" />
    </Component>
  </Page>
  <Page name="mcp.strip">
    <Frame name="Strip" w=86 h=400>
      <Instance name="@mute" of="button" x=30 y=60 w=21 h=20 arg:on="track.mute" arg:control="\"mute\"" />
    </Frame>
  </Page>
</Document>
"##;

#[test]
fn components_take_arguments_and_instances_name_them() {
    let s = document_text::parse(COMPONENTS, BTreeMap::new()).expect("parses");
    let button = s
        .nodes
        .values()
        .find(|n| n.name == "button")
        .expect("button");
    let args: Vec<(&str, &str)> = button
        .args
        .iter()
        .map(|a| (a.name.as_str(), a.expr.as_str()))
        .collect();
    // As declared, defaults and all.
    assert_eq!(
        args,
        [("on", "false"), ("label", "\"M\""), ("control", "\"\"")]
    );
    let mute = s.nodes.values().find(|n| n.name == "@mute").expect("@mute");
    assert_eq!(mute.of.as_deref(), Some("button"));
    let args: Vec<(&str, &str)> = mute
        .args
        .iter()
        .map(|a| (a.name.as_str(), a.expr.as_str()))
        .collect();
    assert_eq!(args, [("on", "track.mute"), ("control", "\"mute\"")]);

    let printed = document_text::print(&s);
    let again = document_text::parse(&printed, BTreeMap::new()).expect("reparses");
    assert_eq!(s, again, "{printed}");
}

#[test]
fn only_components_and_instances_take_arguments() {
    let src = r#"<Document><Page><Frame arg:on="true" /></Page></Document>"#;
    let err = document_text::parse(src, BTreeMap::new()).unwrap_err();
    assert!(
        err.to_string()
            .contains("only a <Component> or an <Instance> takes arguments"),
        "{err}"
    );
}
