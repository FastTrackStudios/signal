//! Scene → markup. Deterministic: same scene, same bytes.

use crate::document::proto::{
    BlendMode, CornerRadii, Node, NodeId, NodeKind, Scene, SourceRef, StrokeAlign, StrokeCap,
    StrokeJoin, TextAlign, TextCase, Transform,
};
use serde_json::Value;

use crate::document::text::schema::{default_name, tag_of};
use crate::document::text::values::{
    Keyword, effects_json, num, num_list, paints_json, plain_solid_hex, quote, rotation_of,
    runs_json, to_json,
};

/// Print the whole scene, rooted at `scene.root`.
#[must_use]
pub fn print(scene: &Scene) -> String {
    let mut out = String::new();
    print_node(scene, scene.root, 0, &mut out);
    if !scene.variables.is_empty() {
        // The block is the first child of <Document>: after its opening
        // line, before the pages.
        let block = variables(&scene.variables);
        match out.find(">\n") {
            Some(i) if !out[..i].ends_with(" /") => out.insert_str(i + 2, &block),
            _ => {
                // A document with no pages printed self-closing.
                let close = out.rfind(" />").unwrap_or(out.len());
                out.replace_range(close.., &format!(">\n{block}</Document>\n"));
            }
        }
    }
    out
}

fn variables(v: &crate::document::proto::Variables) -> String {
    let mut out = format!("  <Variables modes={}>\n", quote(&v.modes.join(" ")));
    for (name, var) in &v.vars {
        out.push_str(&format!(
            "    <Var name={} value={}",
            quote(name),
            quote(&var.value)
        ));
        for (mode, value) in &var.modes {
            out.push_str(&format!(" {mode}={}", quote(value)));
        }
        out.push_str(" />\n");
    }
    out.push_str("  </Variables>\n");
    out
}

/// Print one node and its descendants (a fragment `parse_fragment` reads
/// back). `None` when `id` isn't in the scene.
#[must_use]
pub fn print_subtree(scene: &Scene, id: NodeId) -> Option<String> {
    scene.node(id)?;
    let mut out = String::new();
    print_node(scene, id, 0, &mut out);
    Some(out)
}

fn print_node(scene: &Scene, id: NodeId, depth: usize, out: &mut String) {
    let Some(node) = scene.node(id) else { return };
    let indent = "  ".repeat(depth);
    let tag = tag_of(&node.kind);
    let children: Vec<NodeId> = node
        .children
        .iter()
        .copied()
        .filter(|c| scene.nodes.contains_key(c))
        .collect();
    let (attrs, body) = attributes(node, !children.is_empty());

    out.push_str(&indent);
    out.push('<');
    out.push_str(tag);
    for (k, v) in &attrs {
        out.push(' ');
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    if let Some(of) = &node.of {
        out.push_str(" of=");
        out.push_str(&quote(of));
    }
    for arg in &node.args {
        out.push(' ');
        out.push_str(crate::document::text::schema::ARG_PREFIX);
        out.push_str(&arg.name);
        out.push('=');
        out.push_str(&quote(&arg.expr));
    }
    if let Some(body) = body {
        out.push('>');
        out.push_str(&body);
        out.push_str("</");
        out.push_str(tag);
        out.push_str(">\n");
    } else if children.is_empty() {
        out.push_str(" />\n");
    } else {
        out.push_str(">\n");
        for c in children {
            print_node(scene, c, depth + 1, out);
        }
        out.push_str(&indent);
        out.push_str("</");
        out.push_str(tag);
        out.push_str(">\n");
    }
}

fn braced(v: &Value) -> String {
    format!("{{{}}}", serde_json::to_string(v).unwrap_or_default())
}

/// `(attribute, rendered value)` pairs in canonical order, plus the element
/// body (a text node's characters, when they fit in a body losslessly).
fn attributes(n: &Node, has_children: bool) -> (Vec<(&'static str, String)>, Option<String>) {
    let mut a: Vec<(&'static str, String)> = Vec::new();
    let mut body = None;

    if n.name != default_name(&n.kind) {
        a.push(("name", quote(&n.name)));
    }
    a.push(("id", quote(&n.id.to_string())));
    match &n.kind {
        NodeKind::Instance { component: Some(c) } => a.push(("component", quote(&c.to_string()))),
        NodeKind::BooleanOperation { op: Some(op) } => a.push(("op", quote(&op.keyword()))),
        NodeKind::Other { tag } => a.push(("tag", quote(tag))),
        _ => {}
    }
    if !n.visible {
        a.push(("visible", "false".into()));
    }
    if n.locked {
        a.push(("locked", "true".into()));
    }
    transform(&n.transform, &mut a);
    if n.size.width != 0.0 {
        a.push(("w", num(n.size.width)));
    }
    if n.size.height != 0.0 {
        a.push(("h", num(n.size.height)));
    }
    if n.opacity != 1.0 {
        a.push(("opacity", num(n.opacity)));
    }
    if n.blend != BlendMode::Normal {
        a.push(("blend", quote(&n.blend.keyword())));
    }
    if let Some(r) = radius(&n.corner_radii) {
        a.push(("radius", r));
    }
    if n.clips_content {
        a.push(("clip", "true".into()));
    }
    if let Some(m) = n.mask {
        a.push(("mask", quote(&m.keyword())));
    }
    if let Some(f) = paints_attr(&n.fills) {
        a.push(("fill", f));
    }

    let s = &n.stroke;
    if let Some(f) = paints_attr(&s.paints) {
        a.push(("stroke", f));
    }
    if s.weight != 1.0 {
        a.push(("stroke-weight", num(s.weight)));
    }
    if s.align != StrokeAlign::Center {
        a.push(("stroke-align", quote(&s.align.keyword())));
    }
    if s.cap != StrokeCap::None {
        a.push(("stroke-cap", quote(&s.cap.keyword())));
    }
    if s.join != StrokeJoin::Miter {
        a.push(("stroke-join", quote(&s.join.keyword())));
    }
    if s.miter_limit != 4.0 {
        a.push(("stroke-miter", num(s.miter_limit)));
    }
    if !s.dash_pattern.is_empty() {
        a.push(("stroke-dash", quote(&num_list(&s.dash_pattern))));
    }
    if s.dash_offset != 0.0 {
        a.push(("stroke-dash-offset", num(s.dash_offset)));
    }
    if !n.effects.is_empty() {
        a.push(("effects", braced(&effects_json(&n.effects))));
    }

    if let Some(l) = &n.layout {
        let dir = match l.direction {
            crate::document::proto::LayoutDirection::Horizontal => "row",
            crate::document::proto::LayoutDirection::Vertical => "column",
        };
        a.push(("layout", quote(dir)));
        if l.gap != 0.0 {
            a.push(("gap", num(l.gap)));
        }
        let (t, r, b, lft) = (
            l.padding_top,
            l.padding_right,
            l.padding_bottom,
            l.padding_left,
        );
        if !(t == 0.0 && r == 0.0 && b == 0.0 && lft == 0.0) {
            let v = if t == r && r == b && b == lft {
                num(t)
            } else if t == b && r == lft {
                quote(&num_list(&[t, r]))
            } else {
                quote(&num_list(&[t, r, b, lft]))
            };
            a.push(("padding", v));
        }
        if l.wrap {
            a.push(("wrap", "true".into()));
        }
        match (l.hug_main, l.hug_cross) {
            (true, true) => a.push(("hug", "true".into())),
            (true, false) => a.push(("hug", quote("main"))),
            (false, true) => a.push(("hug", quote("cross"))),
            (false, false) => {}
        }
        if let Some(m) = l.main_align {
            a.push(("justify", quote(&m.keyword())));
        }
        if let Some(c) = l.cross_align {
            a.push(("align", quote(&c.keyword())));
        }
    }

    let lc = &n.layout_child;
    if let Some(g) = lc.grow {
        a.push(("grow", num(g)));
    }
    if let Some(s) = lc.align_self {
        a.push(("align-self", quote(&s.keyword())));
    }
    if lc.absolute {
        a.push(("absolute", "true".into()));
    }
    if let Some(r) = lc.resize_x {
        a.push(("resize-x", quote(&r.keyword())));
    }
    if let Some(r) = lc.resize_y {
        a.push(("resize-y", quote(&r.keyword())));
    }
    for (k, v) in [
        ("min-w", lc.min_width),
        ("min-h", lc.min_height),
        ("max-w", lc.max_width),
        ("max-h", lc.max_height),
    ] {
        if let Some(v) = v {
            a.push((k, num(v)));
        }
    }

    if let NodeKind::Text(t) = &n.kind {
        if let Some(f) = &t.font_family {
            a.push(("font", quote(f)));
        }
        if t.font_size != 14.0 {
            a.push(("size", num(t.font_size)));
        }
        if let Some(w) = t.font_weight {
            a.push(("weight", num(w)));
        }
        if let Some(st) = &t.font_style {
            a.push(("style", quote(st)));
        }
        if let Some(w) = t.font_width {
            a.push(("stretch", num(w)));
        }
        if let Some(lh) = t.line_height {
            a.push(("line-height", num(lh)));
        }
        if t.letter_spacing != 0.0 {
            a.push(("letter-spacing", num(t.letter_spacing)));
        }
        if t.text_case != TextCase::Original {
            a.push(("case", quote(&t.text_case.keyword())));
        }
        if t.align != TextAlign::Left {
            a.push(("text-align", quote(&t.align.keyword())));
        }
        if !t.runs.is_empty() {
            a.push(("runs", braced(&runs_json(&t.runs))));
        }
        if !t.characters.is_empty() {
            if has_children || t.characters.contains('\n') {
                a.push(("text", quote(&t.characters)));
            } else {
                body = Some(escape_body(&t.characters));
            }
        }
    }

    geometry("geometry", &n.fill_geometry, &mut a);
    geometry("stroke-geometry", &n.stroke_geometry, &mut a);
    if let Some(s) = &n.svg_snapshot {
        a.push(("svg-snapshot", quote(&s.0)));
    }
    if let Some(c) = &n.control {
        a.push(("control", braced(&to_json(c))));
    }
    if let Some(e) = &n.emission {
        a.push(("emission", braced(&to_json(e))));
    }
    if let Some(s) = &n.shader {
        a.push(("shader", braced(&to_json(s))));
    }
    if !n.motion.is_empty() {
        a.push(("motion", braced(&to_json(&n.motion))));
    }
    if let Some(s) = &n.source {
        a.push(("source", source(s)));
    }
    for name in crate::document::text::schema::BIND_ATTRS {
        let prop = name.trim_start_matches("bind:");
        for b in n.bind.iter().filter(|b| b.prop == prop) {
            a.push((name, quote(&b.expr)));
        }
    }
    (a, body)
}

fn transform(t: &Transform, a: &mut Vec<(&'static str, String)>) {
    let translate_only = t.a == 1.0 && t.b == 0.0 && t.c == 0.0 && t.d == 1.0;
    let rotation = if translate_only { None } else { rotation_of(t) };
    if translate_only || rotation.is_some() {
        if t.tx != 0.0 {
            a.push(("x", num(t.tx)));
        }
        if t.ty != 0.0 {
            a.push(("y", num(t.ty)));
        }
        if let Some(deg) = rotation {
            a.push(("rotation", num(deg)));
        }
    } else {
        a.push(("transform", braced(&to_json(t))));
    }
}

fn radius(r: &CornerRadii) -> Option<String> {
    if r.is_zero() {
        return None;
    }
    let all = [r.top_left, r.top_right, r.bottom_right, r.bottom_left];
    Some(if all.iter().all(|v| *v == all[0]) {
        num(all[0])
    } else {
        quote(&num_list(&all))
    })
}

fn paints_attr(ps: &[crate::document::proto::Paint]) -> Option<String> {
    match ps {
        [] => None,
        [p] if plain_solid_hex(p).is_some() => plain_solid_hex(p).map(|h| quote(&h)),
        _ => Some(braced(&paints_json(ps))),
    }
}

fn geometry(key: &'static str, paths: &[String], a: &mut Vec<(&'static str, String)>) {
    match paths {
        [] => {}
        [one] => a.push((key, quote(one))),
        many => a.push((key, braced(&Value::from(many.to_vec())))),
    }
}

fn source(s: &SourceRef) -> String {
    // The compact form can't carry import bounds; fall back to JSON then.
    if s.tool.contains('/') || s.node_type.contains('/') || s.bounds.is_some() {
        braced(&to_json(s))
    } else {
        quote(&format!("{}/{}/{}", s.tool, s.node_type, s.id))
    }
}

fn escape_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '{' => out.push_str("&lbrace;"),
            c => out.push(c),
        }
    }
    out
}
