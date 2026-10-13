//! Live reload: replace the design under a running surface.

use std::collections::HashSet;
use std::mem::take;

use document_proto::NodeId;

use super::{Runtime, Surface};

impl Runtime {
    /// Swap in a new design (an edited document) and keep what's running:
    /// parameter values (incl. optimistic edits and edits still
    /// coalescing), the latest stream frames, meter ballistics, local
    /// positions, and a gesture in flight — for every control whose node
    /// is still there with the same kind (node ids are stable across
    /// edits). Controls that are gone drop their state. Motions keep
    /// theirs for every node (and motion slot) still there.
    ///
    /// The host lays the new design out and passes its boxes again
    /// ([`Runtime::set_boxes`]).
    pub fn swap_surface(&mut self, surface: Surface) {
        let mut next = Runtime::new(surface, self.catalog.clone());
        let same =
            |id: &NodeId| match (self.surface.controls.get(id), next.surface.controls.get(id)) {
                (Some(a), Some(b)) => a.kind == b.kind,
                _ => false,
            };
        let kept: HashSet<NodeId> = next
            .surface
            .controls
            .keys()
            .copied()
            .filter(|id| same(id))
            .collect();
        next.values = take(&mut self.values);
        next.streams = take(&mut self.streams);
        next.coalescer = take(&mut self.coalescer);
        next.meter_scale = self.meter_scale.clone();
        next.meters = take(&mut self.meters)
            .into_iter()
            .filter(|((id, _, _), _)| kept.contains(id))
            .collect();
        next.local_positions = take(&mut self.local_positions)
            .into_iter()
            .filter(|(id, _)| kept.contains(id))
            .collect();
        for (id, g) in take(&mut self.gestures) {
            if kept.contains(&id) {
                next.gestures.insert(id, g);
            }
        }
        next.captured = self.captured.filter(|id| kept.contains(id));
        next.hovered = self.hovered.filter(|id| kept.contains(id));
        // Motions keep their state (a ringing spring, a turning reel) for
        // every node still there, and the clock runs on.
        if let (Ok(motion), Ok(prev)) = (next.motion.get_mut(), self.motion.get_mut()) {
            std::mem::swap(motion, prev);
            motion.swap(&next.surface.scene);
        }
        next.texts = take(&mut self.texts);
        next.text_measure = self.text_measure.take();
        next.curves = std::mem::take(&mut self.curves);
        // Selection, focus, an edit in progress: kept for what's still there
        // (instances keep their ids across rebuilds).
        let mut ui = take(&mut self.ui);
        ui.retain(|id| kept.contains(id));
        next.ui = ui;
        *self = next;
    }
}

#[cfg(test)]
mod tests {
    use document_proto::{Node, NodeKind, Op, Scene, Size};
    use surface_proto::{
        Binding, Catalog, Control, ControlKind, EntryId, ParamAddr, ParamInfo, ParamValue, Range,
        StreamFrame, StreamInfo, StreamKind,
    };

    use super::*;
    use crate::{Bounds, EngineCall, Input, Modifiers};

    fn catalog() -> Catalog {
        Catalog {
            id: "t".into(),
            version: 1,
            params: vec![
                ParamInfo::new("gain", "Gain", Range::default()),
                ParamInfo::new("mix", "Mix", Range::default()),
            ],
            streams: vec![StreamInfo {
                id: EntryId::new("out"),
                name: "Out".into(),
                kind: StreamKind::Level { channels: 1 },
                rate_hz: 30.0,
            }],
            ..Catalog::default()
        }
    }

    /// A page with a knob on `gain`, a meter on `out`, and (with `extra`) a
    /// second knob on `mix`; `knob_x` moves the first knob (an edit).
    fn design(knob_x: f64, extra: bool, ids: &mut Vec<NodeId>) -> Surface {
        let mut scene = Scene::new("S");
        let page = scene.pages()[0];
        let mut add = |scene: &mut Scene, i: usize, name: &str, x: f64, control: Control| {
            let mut n = Node::new(NodeKind::Frame, name);
            if let Some(id) = ids.get(i) {
                n.id = *id;
            } else {
                ids.push(n.id);
            }
            n.size = Size {
                width: 40.0,
                height: 40.0,
            };
            n.transform.tx = x;
            n.control = Some(control);
            scene
                .apply(Op::Insert {
                    node: n,
                    parent: page,
                    index: u32::MAX,
                })
                .unwrap();
        };
        let with = |kind: ControlKind, b: Binding| Control {
            binding: Some(b),
            ..Control::new(kind)
        };
        add(
            &mut scene,
            0,
            "knob",
            knob_x,
            with(ControlKind::knob(), Binding::param("gain")),
        );
        add(
            &mut scene,
            1,
            "meter",
            100.0,
            with(
                ControlKind::Meter {
                    orientation: surface_proto::Orientation::Vertical,
                },
                Binding::stream("out"),
            ),
        );
        if extra {
            add(
                &mut scene,
                2,
                "mix",
                200.0,
                with(ControlKind::knob(), Binding::param("mix")),
            );
        }
        Surface::from_scene(scene)
    }

    fn boxes(rt: &mut Runtime) {
        let b: Vec<(NodeId, Bounds)> = rt
            .surface()
            .scene
            .nodes
            .values()
            .map(|n| {
                (
                    n.id,
                    Bounds {
                        x: n.transform.tx,
                        y: 0.0,
                        width: n.size.width,
                        height: n.size.height,
                    },
                )
            })
            .collect();
        rt.set_boxes(b);
    }

    #[test]
    fn swap_keeps_values_meters_and_the_gesture_in_flight() {
        let mut ids = Vec::new();
        let mut rt = Runtime::new(design(0.0, true, &mut ids), catalog());
        boxes(&mut rt);
        rt.apply_values([
            ParamValue {
                addr: ParamAddr::new("gain"),
                value: 0.25,
            },
            ParamValue {
                addr: ParamAddr::new("mix"),
                value: 0.75,
            },
        ]);
        rt.apply_frame(
            StreamFrame {
                id: EntryId::new("out"),
                values: vec![-6.0],
            },
            0.0,
        );
        let meter = ids[1];
        let reading = rt.state(meter, &rt.surface().controls[&meter].clone(), 50.0);

        // Grab the knob and start dragging…
        let m = Modifiers::default();
        rt.pointer(
            Input::Down {
                x: 20.0,
                y: 20.0,
                t: 0.0,
                mods: m,
            },
            0.0,
        );
        rt.pointer(
            Input::Move {
                x: 20.0,
                y: 10.0,
                t: 16.0,
                mods: m,
            },
            16.0,
        );
        let mid = rt
            .values()
            .into_iter()
            .find(|v| v.addr.0 == "gain")
            .unwrap()
            .value;
        assert!(mid > 0.25, "dragging up raised it: {mid}");

        // …the document changes under the hand: the knob moved, `mix` is gone.
        let mut ids2 = ids.clone();
        ids2.truncate(2);
        rt.swap_surface(design(10.0, false, &mut ids2));
        boxes(&mut rt);
        assert!(rt.animating(), "the drag is still captured");
        let after = rt.state(meter, &rt.surface().controls[&meter].clone(), 50.0);
        assert_eq!(after, reading, "meter ballistics carried over");
        assert!(
            rt.values()
                .iter()
                .any(|v| v.addr.0 == "mix" && v.value == 0.75),
            "engine values stay"
        );

        // The same drag continues from where it was, then ends.
        let mut calls = rt.pointer(
            Input::Move {
                x: 30.0,
                y: 0.0,
                t: 32.0,
                mods: m,
            },
            32.0,
        );
        calls.extend(rt.pointer(
            Input::Up {
                x: 30.0,
                y: 0.0,
                t: 48.0,
            },
            48.0,
        ));
        calls.extend(rt.poll(200.0));
        let end = rt
            .values()
            .into_iter()
            .find(|v| v.addr.0 == "gain")
            .unwrap()
            .value;
        assert!(end > mid, "{mid} → {end}");
        assert!(
            calls.iter().any(|c| matches!(
                c,
                EngineCall::SetParam {
                    phase: surface_proto::EditPhase::End,
                    ..
                }
            )),
            "the gesture ends properly: {calls:?}"
        );
        assert!(!rt.surface().controls.contains_key(&ids[2]));
    }

    #[test]
    fn a_control_that_changed_kind_starts_fresh() {
        let mut ids = Vec::new();
        let mut rt = Runtime::new(design(0.0, false, &mut ids), catalog());
        boxes(&mut rt);
        rt.pointer(
            Input::Down {
                x: 20.0,
                y: 20.0,
                t: 0.0,
                mods: Modifiers::default(),
            },
            0.0,
        );
        let mut s = design(0.0, false, &mut ids);
        s.controls.get_mut(&ids[0]).unwrap().kind = ControlKind::Fader {
            orientation: surface_proto::Orientation::Vertical,
        };
        rt.swap_surface(s);
        assert!(
            rt.pointer(
                Input::Move {
                    x: 20.0,
                    y: 0.0,
                    t: 16.0,
                    mods: Modifiers::default()
                },
                16.0
            )
            .is_empty()
        );
    }
}
