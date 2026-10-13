//! The canonical control library.
//!
//! - [`theme`]: one set of semantic tokens (colors, sizes, spacing).
//! - [`widgets`]: knob, fader, button, footswitch, meter, readout — each a
//!   plain design subtree plus a [`Control`](crate::surface::proto::Control) with
//!   its slots wired.
//! - [`templates`]: whole surfaces from an engine [`Catalog`](crate::surface::proto::Catalog).

pub mod templates;
pub mod theme;
pub mod widgets;

pub use templates::{Options, hug, surface_for_catalog, wrap_rows};
pub use theme::{ControlSize, Theme};
pub use widgets::{Builder, sector_path};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::memory::demo_catalog;
    use crate::surface::proto::{ControlKind, SlotRole};
    use crate::surface::runtime::Runtime;

    #[test]
    fn demo_surface_is_fully_bound_and_valid() {
        let catalog = demo_catalog();
        let surface = surface_for_catalog(&catalog, &Options::default());
        assert!(
            surface.controls.len() >= 20,
            "got {}",
            surface.controls.len()
        );
        for (id, control) in &surface.controls {
            let issues = control.validate(&catalog);
            assert!(
                issues.is_empty(),
                "{:?} on {}: {issues:?}",
                control.kind.name(),
                surface.scene.node(*id).unwrap().name
            );
            for slot in &control.slots {
                assert!(
                    surface
                        .scene
                        .node(crate::document::proto::NodeId(slot.node))
                        .is_some(),
                    "dangling slot"
                );
            }
        }
        // Every continuous param got a knob or fader, every toggle a footswitch.
        let knobs = surface
            .controls
            .values()
            .filter(|c| matches!(c.kind, ControlKind::Knob { .. }))
            .count();
        let switches = surface
            .controls
            .values()
            .filter(|c| matches!(c.kind, ControlKind::Footswitch { .. }))
            .count();
        assert!(
            knobs >= 14 && switches == 3 + catalog.actions.len(),
            "knobs {knobs}, switches {switches}"
        );
    }

    #[test]
    fn containers_hug_their_content() {
        let surface = surface_for_catalog(&demo_catalog(), &Options::default());
        let root = surface
            .scene
            .pages()
            .first()
            .and_then(|p| surface.scene.node(*p))
            .unwrap()
            .children[0];
        let size = surface.scene.node(root).unwrap().size;
        assert!(size.width > 600.0 && size.height > 150.0, "{size:?}");
        for n in surface.scene.nodes.values() {
            if n.layout.is_some() {
                assert!(
                    n.size.width > 0.0 && n.size.height > 0.0,
                    "{} unsized",
                    n.name
                );
            }
        }
    }

    #[test]
    fn frames_render_live_values() {
        let catalog = demo_catalog();
        let surface = surface_for_catalog(&catalog, &Options::default());
        let rt = Runtime::new(surface, catalog);
        let frame = rt.frame(0.0);
        let texts: Vec<String> = frame
            .nodes
            .values()
            .filter_map(|n| n.text().map(|t| t.characters.clone()))
            .collect();
        assert!(
            texts.iter().any(|t| t == "Gain"),
            "labels come from the catalog"
        );
        assert!(
            texts.iter().any(|t| t == "350 ms"),
            "readouts show defaults: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "Clean"),
            "choices show option names"
        );
    }

    #[test]
    fn featured_params_limit_generated_surfaces() {
        let mut catalog = demo_catalog();
        for p in &mut catalog.params {
            p.featured = matches!(p.addr.0.as_str(), "amp/gain" | "delay/mix" | "delay/on");
        }
        let surface = surface_for_catalog(&catalog, &Options::default());
        let bound: Vec<String> = surface
            .controls
            .values()
            .filter_map(|c| match c.binding.as_ref().map(|b| &b.source) {
                Some(crate::surface::proto::Source::Param { addr }) => Some(addr.0.clone()),
                _ => None,
            })
            .collect();
        for addr in ["amp/gain", "delay/mix", "delay/on"] {
            assert!(bound.iter().any(|b| b == addr), "{addr} missing: {bound:?}");
        }
        assert!(
            !bound.iter().any(|b| b == "amp/bass"),
            "unfeatured params are left out"
        );
    }

    #[test]
    fn wide_surfaces_wrap_into_rows() {
        let surface = surface_for_catalog(
            &demo_catalog(),
            &Options {
                max_width: 700.0,
                ..Options::default()
            },
        );
        let root = surface
            .scene
            .node(surface.scene.pages()[0])
            .unwrap()
            .children[0];
        let w = surface.scene.node(root).unwrap().size.width;
        assert!(w <= 700.0 + 1e-9, "root is {w} wide");
        assert!(
            surface.scene.nodes.values().any(|n| n.name == "panels 2"),
            "panels wrapped"
        );
    }

    #[test]
    fn knob_slots_are_complete() {
        let surface = surface_for_catalog(&demo_catalog(), &Options::default());
        let knob = surface
            .controls
            .values()
            .find(|c| matches!(c.kind, ControlKind::Knob { .. }))
            .unwrap();
        for role in [
            SlotRole::Label,
            SlotRole::ValueArc,
            SlotRole::Needle,
            SlotRole::Value,
        ] {
            assert_eq!(knob.slots_of(role).count(), 1, "{role:?}");
        }
    }
}
