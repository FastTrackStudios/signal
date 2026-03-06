//! Rig seed data — default rig collections for development/demo.

use signal_proto::engine::EngineId;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
use signal_proto::rig::{EngineSelection, Rig, RigScene, RigType};
use signal_proto::seed_id;

/// All default rig collections.
pub fn rigs() -> Vec<Rig> {
    vec![
        keys_mega_rig(),
        guitar_mega_rig(),
        vocal_mega_rig(),
        worship_guitar_rig(),
    ]
}

fn keys_mega_rig() -> Rig {
    let default_scene = RigScene::new(seed_id("keys-megarig-default"), "Default")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("pad-engine"),
            seed_id("pad-engine-default"),
        ))
        .with_override(Override {
            path: NodePath::engine("synth-engine").with_layer("synth-layer-osc"),
            op: NodeOverrideOp::ReplaceRef("synth-layer-osc-alt".to_string()),
        })
        .with_override(Override {
            path: NodePath::engine("synth-engine")
                .with_layer("synth-layer-motion")
                .with_module("time-parallel"),
            op: NodeOverrideOp::ReplaceRef("time-parallel-ambient".to_string()),
        })
        .with_override(Override {
            path: NodePath::engine("synth-engine")
                .with_layer("synth-layer-texture")
                .with_block("texture-verb"),
            op: NodeOverrideOp::ReplaceRef("reverb-space-blackhole".to_string()),
        })
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-space")
                .with_block("keys-space-verb")
                .with_parameter("mix"),
            0.63,
        ))
        .with_override(Override {
            path: NodePath::engine("keys-engine").with_layer("keys-layer-space"),
            op: NodeOverrideOp::ReplaceRef(
                seed_id("__phantom__keys-megarig-keys-layer-space").to_string(),
            ),
        })
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));

    let wide_scene = RigScene::new(seed_id("keys-megarig-wide"), "Wide")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-bright"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-scene-b"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("pad-engine"),
            seed_id("pad-engine-default"),
        ))
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-core")
                .with_block("keys-core-eq")
                .with_parameter("high_shelf"),
            0.74,
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));

    let focus_scene = RigScene::new(seed_id("keys-megarig-focus"), "Focus")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("pad-engine"),
            seed_id("pad-engine-default"),
        ))
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-core")
                .with_block("keys-core-comp")
                .with_parameter("threshold"),
            0.38,
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));

    let air_scene = RigScene::new(seed_id("keys-megarig-air"), "Air")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-bright"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("pad-engine"),
            seed_id("pad-engine-default"),
        ))
        .with_override(Override::set(
            NodePath::engine("pad-engine")
                .with_layer("pad-layer-shimmer")
                .with_block("pad-shimmer-delay")
                .with_parameter("mix"),
            0.58,
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));

    let mut rig = Rig::new(
        seed_id("keys-megarig"),
        "MegaRig",
        vec![
            EngineId::from(seed_id("keys-engine")),
            EngineId::from(seed_id("synth-engine")),
            EngineId::from(seed_id("organ-engine")),
            EngineId::from(seed_id("pad-engine")),
        ],
        default_scene,
    )
    .with_rig_type(RigType::Keys)
    .with_metadata(
        Metadata::new()
            .with_tag("megarig")
            .with_tag("keys")
            .with_description(
                "Keys MegaRig showcasing engine/layer/module/block/parameter overrides",
            ),
    );
    rig.add_variant(wide_scene);
    rig.add_variant(focus_scene);
    rig.add_variant(air_scene);
    rig
}

fn guitar_mega_rig() -> Rig {
    let default_scene = RigScene::new(seed_id("guitar-megarig-default"), "Default")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-default"),
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("guitar"));

    let lead_scene = RigScene::new(seed_id("guitar-megarig-lead"), "Lead")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-lead"),
        ))
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("time-parallel")
                .with_block("dly-1")
                .with_parameter("mix"),
            0.49,
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("guitar"));

    let mut rig = Rig::new(
        seed_id("guitar-megarig"),
        "MegaRig",
        vec![EngineId::from(seed_id("guitar-engine"))],
        default_scene,
    )
    .with_rig_type(RigType::Guitar)
    .with_metadata(
        Metadata::new()
            .with_tag("megarig")
            .with_tag("guitar")
            .with_description("Guitar MegaRig implemented from the guitar template module layout"),
    );
    rig.add_variant(lead_scene);
    rig
}

/// Worship guitar rig — single-engine rig with Dry and Ambient scene states.
///
/// Scene layout:
/// - Default: baseline worship tone (Klone drive, parallel time, full signal chain)
/// - Dry: Input + Drive + Amp active; Modulation, Time, Motion bypassed
/// - Ambient: full chain active; lush reverb/delay mix, chorus, tremolo engaged
fn worship_guitar_rig() -> Rig {
    let default_scene = RigScene::new(seed_id("worship-rig-default"), "Default")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-default"),
        ))
        .with_metadata(Metadata::new().with_tag("worship").with_tag("guitar"));

    // Scene 5 — Dry
    // Input, Drive, Amp, Master enabled; Modulation, Time, Motion bypassed.
    // Spring reverb on JM Pre-FX provides minimal room character.
    let dry_scene = RigScene::new(seed_id("worship-rig-dry"), "Dry")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-default"),
        ))
        // Drive: Klone (drive-2) engaged via sweetener snapshot
        .with_override(Override {
            path: NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("drive-full-stack")
                .with_block("drive-2"),
            op: NodeOverrideOp::ReplaceRef(seed_id("drive-klon-sweetener").to_string()),
        })
        // Amp: Room reverb at low mix via JM spring reverb (0.15)
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_module("jm-pre-fx")
                .with_block("spring-reverb")
                .with_parameter("mix"),
            0.15,
        ))
        // Modulation Module: bypassed
        .with_override(Override::bypass(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("gtr-modulation"),
            true,
        ))
        // Time Module: bypassed (all delays + reverbs off)
        .with_override(Override::bypass(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("time-parallel"),
            true,
        ))
        // Motion Module: bypassed
        .with_override(Override::bypass(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("gtr-motion"),
            true,
        ))
        .with_metadata(
            Metadata::new()
                .with_tag("worship")
                .with_tag("guitar")
                .with_tag("dry"),
        );

    // Scene 6 — Ambient
    // All modules enabled; lush reverb/delay/modulation settings engaged.
    let ambient_scene = RigScene::new(seed_id("worship-rig-ambient"), "Ambient")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-default"),
        ))
        // Drive: lighter drive settings (lower drive-1 drive amount)
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("drive-full-stack")
                .with_block("drive-1")
                .with_parameter("drive"),
            0.35,
        ))
        // Amp: Room reverb at higher mix via JM spring reverb (0.42)
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_module("jm-pre-fx")
                .with_block("spring-reverb")
                .with_parameter("mix"),
            0.42,
        ))
        // Modulation: chorus active at moderate mix
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("gtr-modulation")
                .with_block("chorus")
                .with_parameter("mix"),
            0.55,
        ))
        // Time: delay mix (dly-1) at ambient level
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("time-parallel")
                .with_block("dly-1")
                .with_parameter("mix"),
            0.48,
        ))
        // Time: reverb mix (verb-1) at ambient level
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("time-parallel")
                .with_block("verb-1")
                .with_parameter("mix"),
            0.52,
        ))
        // Motion: tremolo active with worshipful depth and rate
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("gtr-motion")
                .with_block("tremolo")
                .with_parameter("depth"),
            0.65,
        ))
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-main")
                .with_module("gtr-motion")
                .with_block("tremolo")
                .with_parameter("rate"),
            0.40,
        ))
        .with_metadata(
            Metadata::new()
                .with_tag("worship")
                .with_tag("guitar")
                .with_tag("ambient"),
        );

    let mut rig = Rig::new(
        seed_id("worship-guitar-rig"),
        "Worship Rig",
        vec![EngineId::from(seed_id("guitar-engine"))],
        default_scene,
    )
    .with_rig_type(RigType::Guitar)
    .with_metadata(
        Metadata::new()
            .with_tag("worship")
            .with_tag("guitar")
            .with_description(
            "Worship guitar rig with Dry (bypass mod/time/motion) and Ambient (full chain) scenes",
        ),
    );
    rig.add_variant(dry_scene);
    rig.add_variant(ambient_scene);
    rig
}

fn vocal_mega_rig() -> Rig {
    let default_scene = RigScene::new(seed_id("vocal-megarig-default"), "Default")
        .with_engine(EngineSelection::new(
            seed_id("vocal-engine"),
            seed_id("vocal-engine-default"),
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("vocal"));

    Rig::new(
        seed_id("vocal-megarig"),
        "MegaRig",
        vec![EngineId::from(seed_id("vocal-engine"))],
        default_scene,
    )
    .with_rig_type(RigType::Vocals)
    .with_metadata(
        Metadata::new()
            .with_tag("megarig")
            .with_tag("vocal")
            .with_description(
                "Vocal MegaRig with Rescue, Correction, Tonal, Modulation, and Time modules",
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed_data::{engine, layer, module};
    use signal_proto::rig::RigType;

    #[test]
    fn rig_count() {
        assert_eq!(rigs().len(), 4);
    }

    #[test]
    fn has_three_megarigs_with_types() {
        let rigs = rigs();
        let megarigs: Vec<_> = rigs.iter().filter(|r| r.name == "MegaRig").collect();
        assert_eq!(megarigs.len(), 3);
        assert!(megarigs.iter().any(|r| r.rig_type == Some(RigType::Keys)));
        assert!(megarigs.iter().any(|r| r.rig_type == Some(RigType::Guitar)));
        assert!(megarigs.iter().any(|r| r.rig_type == Some(RigType::Vocals)));
    }

    #[test]
    fn worship_rig_has_dry_and_ambient_scenes() {
        use signal_proto::overrides::NodeOverrideOp;

        let worship = rigs()
            .into_iter()
            .find(|r| r.name == "Worship Rig")
            .expect("worship rig not found");

        assert_eq!(worship.rig_type, Some(RigType::Guitar));

        let dry = worship
            .variants
            .iter()
            .find(|v| v.name == "Dry")
            .expect("Dry scene missing");

        // Modulation, Time, Motion should be bypassed in Dry
        let bypass_paths: Vec<_> = dry
            .overrides
            .iter()
            .filter_map(|ov| match &ov.op {
                NodeOverrideOp::Bypass(true) => Some(ov.path.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            bypass_paths.iter().any(|p| p.contains("gtr-modulation")),
            "Dry scene must bypass gtr-modulation"
        );
        assert!(
            bypass_paths.iter().any(|p| p.contains("time-parallel")),
            "Dry scene must bypass time-parallel"
        );
        assert!(
            bypass_paths.iter().any(|p| p.contains("gtr-motion")),
            "Dry scene must bypass gtr-motion"
        );

        let ambient = worship
            .variants
            .iter()
            .find(|v| v.name == "Ambient")
            .expect("Ambient scene missing");

        // Ambient should NOT bypass modulation/time/motion
        let ambient_bypasses: Vec<_> = ambient
            .overrides
            .iter()
            .filter(|ov| matches!(&ov.op, NodeOverrideOp::Bypass(true)))
            .collect();
        assert!(
            ambient_bypasses.is_empty(),
            "Ambient scene must not bypass any modules"
        );

        // Ambient should set reverb and delay parameters
        let set_paths: Vec<_> = ambient
            .overrides
            .iter()
            .filter_map(|ov| match &ov.op {
                NodeOverrideOp::Set(_) => Some(ov.path.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            set_paths.iter().any(|p| p.contains("spring-reverb")),
            "Ambient scene must set spring-reverb mix"
        );
        assert!(
            set_paths.iter().any(|p| p.contains("time-parallel")),
            "Ambient scene must set time-parallel parameters"
        );
        assert!(
            set_paths.iter().any(|p| p.contains("gtr-motion")),
            "Ambient scene must set tremolo parameters"
        );
    }

    #[test]
    fn keys_megarig_touches_all_levels_except_profile_song() {
        let keys_rig = rigs()
            .into_iter()
            .find(|r| r.rig_type == Some(RigType::Keys))
            .expect("expected keys megarig");

        // Rig level: multiple engines and scene variants.
        assert_eq!(keys_rig.engine_ids.len(), 4);
        assert!(keys_rig.variants.len() >= 2);

        let default_scene = keys_rig
            .variants
            .iter()
            .find(|v| v.name == "Default")
            .expect("keys megarig missing default scene");
        assert_eq!(default_scene.engine_selections.len(), 4);

        // Rig overrides: layer/module/block/parameter coverage.
        let mut has_layer_replace = false;
        let mut has_module_replace = false;
        let mut has_block_replace = false;
        let mut has_param_set = false;
        for ov in &default_scene.overrides {
            let path = ov.path.as_str();
            match &ov.op {
                NodeOverrideOp::ReplaceRef(_)
                    if path.contains(".layer.")
                        && !path.contains(".module.")
                        && !path.contains(".block.") =>
                {
                    has_layer_replace = true;
                }
                NodeOverrideOp::ReplaceRef(_) if path.contains(".module.") => {
                    has_module_replace = true;
                }
                NodeOverrideOp::ReplaceRef(_) if path.contains(".block.") => {
                    has_block_replace = true;
                }
                NodeOverrideOp::Set(_) if path.contains(".param.") => {
                    has_param_set = true;
                }
                _ => {}
            }
        }
        assert!(has_layer_replace, "expected rig-level layer replacement");
        assert!(has_module_replace, "expected rig-level module replacement");
        assert!(has_block_replace, "expected rig-level block replacement");
        assert!(has_param_set, "expected rig-level parameter set");

        // Engine level exists for each selected engine.
        let seeded_engines = engine::engines();
        for sel in &default_scene.engine_selections {
            let engine = seeded_engines
                .iter()
                .find(|e| e.id == sel.engine_id)
                .expect("rig references unknown engine");
            assert!(!engine.layer_ids.is_empty());
            assert!(engine.variant(&sel.variant_id).is_some());
        }

        // Layer level includes composable refs (layer/module/block).
        let seeded_layers = layer::layers();
        let keys_space = seeded_layers
            .iter()
            .find(|l| l.name == "Keys Space")
            .expect("missing Keys Space layer");
        let keys_space_default = keys_space
            .default_variant()
            .expect("Keys Space missing default variant");
        assert!(!keys_space_default.layer_refs.is_empty());
        assert!(!keys_space_default.module_refs.is_empty());
        assert!(!keys_space_default.block_refs.is_empty());

        // Module level exists independently of profiles/songs.
        let modules = module::presets();
        assert!(modules.iter().any(|m| m.name() == "Source"));
        assert!(modules.iter().any(|m| m.name() == "Parallel Time"));
        assert!(modules.iter().any(|m| m.name() == "Rescue"));
    }
}
