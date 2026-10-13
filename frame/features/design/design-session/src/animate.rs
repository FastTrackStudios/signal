//! The `animate` tool: add, replace, remove or list a node's motions.

use design_lint::Severity;
use document_proto::motion::{Motion, Target};
use document_proto::{NodeId, Op, Prop};
use serde_json::{Value, json};
use surface_kit::motion as kit;

use super::{Args, Result, Session, ToolDef, ToolError, err};

pub(crate) fn def() -> ToolDef {
    let node = json!({"type": "string", "description": "node id (uuid) or unique node name"});
    ToolDef {
        name: "animate",
        description: "Animate a node: each motion drives one property (target) from an input expression through physical dynamics. \
target: none | x | y | rotate | scale | stretch_x | stretch_y | skew_x | skew_y | opacity | emission | dash_offset | {\"fill\":{\"colors\":[…]}} | {\"stroke\":{\"colors\":[…]}} | {\"morph\":{\"shapes\":[path…]}} | {\"shape\":{\"provider\":\"coil|reverb.room|wave\",\"part\":…,\"inputs\":{name: number|expr}}}. \
input (expression): norm(param) | param(param) | level(feed[,ch]) | gr(feed) | scalar(feed) | band(feed,lo,hi) | feed(feed,i) | motion(name) | time | hover | press, with + - * / and clamp min max lerp smoothstep curve or…. \
dynamics: \"follow\" | {\"spring\":{stiffness,damping,mass}} | {\"oscillator\":{freq,decay|damping,excite:expr,gain}} (a struck resonator: rings when excite rises, dies over decay s) | {\"lfo\":{rate,shape,phase}} | {\"noise\":{rate,amount}} | {\"ballistic\":{attack,release}} | {\"keyframes\":{times,values,easing,loop}} | \"spin\" (input = turns/s); any number may be an expression. \
from/to map output 0/1 onto the target (degrees, px, 0..1); lag: add only the dynamics' overshoot to a part a control already moves; pivot {x,y} in the node's box; name: for motion(name). \
Or a preset: spring_follow {param} | ring_on {feed, freq, decay} | spin {speed} | flicker {amount} | pulse {rate, low} — with optional target/from/to overrides. \
Pass `add` (a motion or list), or `set` (replace all), or `remove` (an index), or `clear`; with none of them, lists the node's motions.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "node": node,
                "add": {"description": "a motion object, or a list of them, to append"},
                "set": {"type": "array", "description": "replace every motion on the node"},
                "remove": {"type": "integer", "minimum": 0},
                "clear": {"type": "boolean"},
                "preset": {"type": "string", "enum": ["spring_follow", "ring_on", "spin", "flicker", "pulse"]},
                "args": {"type": "object", "description": "preset arguments: param | feed, freq, decay | speed | amount | rate, low"},
                "target": {"description": "preset: a target to use instead of its default"},
                "from": {"type": "number"},
                "to": {"type": "number"}
            },
            "required": ["node"]
        }),
        mutates: true,
    }
}

fn motions(v: &Value) -> Result<Vec<Motion>> {
    let list = match v {
        Value::Array(_) => v.clone(),
        other => Value::Array(vec![other.clone()]),
    };
    serde_json::from_value(list).map_err(|e| ToolError(format!("not a motion: {e}")))
}

fn preset(a: &Args<'_>) -> Result<Option<Motion>> {
    let Some(name) = a.opt_str("preset")? else { return Ok(None) };
    let args = a.0.get("args").cloned().unwrap_or(Value::Null);
    let s = |k: &str| -> Result<String> {
        match args.get(k) {
            Some(Value::String(s)) => Ok(s.clone()),
            Some(Value::Number(n)) => Ok(n.to_string()),
            _ => err(format!("preset `{name}` needs args.{k}")),
        }
    };
    let num = |k: &str, default: f64| -> document_proto::motion::Num {
        match args.get(k) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(default).into(),
            Some(Value::String(s)) => s.as_str().into(),
            _ => default.into(),
        }
    };
    let mut m = match name {
        "spring_follow" => kit::spring_follow(&s("param")?),
        "ring_on" => kit::ring_on(&s("feed")?, num("freq", 4.0), num("decay", 1.5)),
        "spin" => kit::spin(&s("speed")?),
        "flicker" => kit::flicker(num("amount", 0.1)),
        "pulse" => kit::pulse(num("rate", 1.0), args.get("low").and_then(Value::as_f64).unwrap_or(0.2)),
        other => return err(format!("unknown preset `{other}`; presets: spring_follow, ring_on, spin, flicker, pulse")),
    };
    if let Some(t) = a.0.get("target") {
        m.target = serde_json::from_value::<Target>(t.clone()).map_err(|e| ToolError(format!("target: {e}")))?;
    }
    if let Some(f) = a.0.get("from").and_then(Value::as_f64) {
        m.from = f;
    }
    if let Some(t) = a.0.get("to").and_then(Value::as_f64) {
        m.to = t;
    }
    Ok(Some(m))
}

impl Session {
    pub(crate) fn animate(&mut self, node: NodeId, a: &Args<'_>) -> Result<String> {
        let current = self.scene.node(node).map(|n| n.motion.clone()).unwrap_or_default();
        let next = if let Some(v) = a.0.get("set") {
            Some(motions(v)?)
        } else if a.0.get("clear").and_then(Value::as_bool) == Some(true) {
            Some(Vec::new())
        } else if let Some(i) = a.opt_u32("remove")? {
            let i = i as usize;
            if i >= current.len() {
                return err(format!("the node has {} motion(s); no index {i}", current.len()));
            }
            let mut v = current.clone();
            v.remove(i);
            Some(v)
        } else {
            let mut added = match a.0.get("add") {
                Some(v) => motions(v)?,
                None => Vec::new(),
            };
            added.extend(preset(a)?);
            (!added.is_empty()).then(|| current.iter().cloned().chain(added).collect())
        };
        if let Some(v) = next {
            for (i, m) in v.iter().enumerate() {
                if let Some(p) = m.problems().into_iter().next() {
                    return err(format!("motion[{i}]: {p}"));
                }
            }
            self.commit(vec![Op::Set { id: node, prop: Prop::Motion(v) }])?;
        }
        let now = self.scene.node(node).map(|n| n.motion.clone()).unwrap_or_default();
        let mut out = format!("{} motion(s) on {node}:\n", now.len());
        for (i, m) in now.iter().enumerate() {
            out.push_str(&format!("  [{i}] {}\n", serde_json::to_string(m).unwrap_or_default()));
        }
        let issues: Vec<String> =
            self.issues().into_iter().filter(|i| i.rule == "motion" && i.node == node && i.severity >= Severity::Warning).map(|i| i.to_string()).collect();
        if issues.is_empty() {
            out.push_str("motion lint: clean");
        } else {
            out.push_str(&format!("motion lint:\n  {}", issues.join("\n  ")));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use surface_memory::demo_catalog;

    use super::super::Session;

    #[test]
    fn animate_adds_presets_and_raw_motions_and_reports_lint() {
        let mut s = Session::blank(demo_catalog());
        let out = s.call("write", &json!({"parent": s.scene().pages()[0].to_string(), "markup": "<Ellipse name=\"Reel\" w=60 h=60 />"})).unwrap();
        assert!(out.contains("Reel") || !out.is_empty());
        let out = s.call("animate", &json!({"node": "Reel", "preset": "spin", "args": {"speed": "0.5"}})).unwrap();
        assert!(out.contains("1 motion(s)") && out.contains("\"spin\""), "{out}");
        let out = s.call("animate", &json!({"node": "Reel", "add": {"target": "opacity", "input": "norm(nope/nothing)"}})).unwrap();
        assert!(out.contains("2 motion(s)") && out.contains("no param `nope/nothing`"), "{out}");
        let e = s.call("animate", &json!({"node": "Reel", "add": {"target": "x", "input": "1 +"}})).unwrap_err();
        assert!(e.0.contains("input"), "{e}");
        s.call("animate", &json!({"node": "Reel", "remove": 1})).unwrap();
        let out = s.call("read", &json!({"node": "Reel"})).unwrap();
        assert!(out.contains("motion={[{\"dynamics\":\"spin\""), "{out}");
        s.call("undo", &json!({})).unwrap();
        assert_eq!(s.scene().nodes.values().find(|n| n.name == "Reel").unwrap().motion.len(), 2, "one undo per call");
    }
}
