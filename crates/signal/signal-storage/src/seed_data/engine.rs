//! Engine seed data — default engine collections for development/demo.

use signal_proto::engine::{Engine, EngineScene, LayerSelection};
use signal_proto::layer::LayerId;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::{NodePath, Override};
use signal_proto::{seed_id, EngineType};

/// All default engine collections.
pub fn engines() -> Vec<Engine> {
    vec![
        keys_engine(),
        synth_engine(),
        organ_engine(),
        pad_engine(),
        guitar_engine(),
        worship_guitar_engine(),
        vocal_engine(),
    ]
}

fn keys_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("keys-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("keys-layer-core"),
            seed_id("keys-layer-core-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("keys-layer-space"),
            seed_id("keys-layer-space-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("keys-layer-space")
                .with_module("time-parallel")
                .with_block("verb-1")
                .with_parameter("mix"),
            0.56,
        ));

    let bright_scene = EngineScene::new(seed_id("keys-engine-bright"), "Bright")
        .with_layer(LayerSelection::new(
            seed_id("keys-layer-core"),
            seed_id("keys-layer-core-bright"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("keys-layer-space"),
            seed_id("keys-layer-space-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("keys-layer-core")
                .with_block("keys-core-eq")
                .with_parameter("high_shelf"),
            0.68,
        ));

    let mut engine = Engine::new(
        seed_id("keys-engine"),
        "Keys Engine",
        EngineType::Keys,
        vec![
            LayerId::from(seed_id("keys-layer-core")),
            LayerId::from(seed_id("keys-layer-space")),
        ],
        default_scene,
    )
    .with_metadata(Metadata::new().with_tag("keys"));
    engine.add_variant(bright_scene);
    engine
}

fn synth_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("synth-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-osc"),
            seed_id("synth-layer-osc-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-motion"),
            seed_id("synth-layer-motion-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-texture"),
            seed_id("synth-layer-texture-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("synth-layer-motion")
                .with_module("time-parallel")
                .with_block("dly-1")
                .with_parameter("feedback"),
            0.57,
        ));

    let scene_b = EngineScene::new(seed_id("synth-engine-scene-b"), "Scene B")
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-osc"),
            seed_id("synth-layer-osc-alt"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-motion"),
            seed_id("synth-layer-motion-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("synth-layer-texture"),
            seed_id("synth-layer-texture-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("synth-layer-texture")
                .with_block("texture-verb")
                .with_parameter("mix"),
            0.72,
        ));

    let mut engine = Engine::new(
        seed_id("synth-engine"),
        "Synth Engine",
        EngineType::Synth,
        vec![
            LayerId::from(seed_id("synth-layer-osc")),
            LayerId::from(seed_id("synth-layer-motion")),
            LayerId::from(seed_id("synth-layer-texture")),
        ],
        default_scene,
    )
    .with_metadata(Metadata::new().with_tag("synth"));
    engine.add_variant(scene_b);
    engine
}

fn organ_engine() -> Engine {
    let scene = EngineScene::new(seed_id("organ-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("organ-layer-body"),
            seed_id("organ-layer-body-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("organ-layer-air"),
            seed_id("organ-layer-air-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("organ-layer-air")
                .with_block("organ-air-verb")
                .with_parameter("mix"),
            0.50,
        ));

    Engine::new(
        seed_id("organ-engine"),
        "Organ Engine",
        EngineType::Organ,
        vec![
            LayerId::from(seed_id("organ-layer-body")),
            LayerId::from(seed_id("organ-layer-air")),
        ],
        scene,
    )
}

fn pad_engine() -> Engine {
    let scene = EngineScene::new(seed_id("pad-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("pad-layer-foundation"),
            seed_id("pad-layer-foundation-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("pad-layer-shimmer"),
            seed_id("pad-layer-shimmer-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("pad-layer-shimmer")
                .with_block("pad-shimmer-delay")
                .with_parameter("mix"),
            0.55,
        ));

    Engine::new(
        seed_id("pad-engine"),
        "Pad Engine",
        EngineType::Pad,
        vec![
            LayerId::from(seed_id("pad-layer-foundation")),
            LayerId::from(seed_id("pad-layer-shimmer")),
        ],
        scene,
    )
}

fn guitar_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("guitar-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("guitar-layer-main"),
            seed_id("guitar-layer-main-default"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("guitar-layer-main")
                .with_module("gtr-amp")
                .with_block("amp-l")
                .with_parameter("gain"),
            0.47,
        ));

    let lead_scene = EngineScene::new(seed_id("guitar-engine-lead"), "Lead")
        .with_layer(LayerSelection::new(
            seed_id("guitar-layer-main"),
            seed_id("guitar-layer-main-lead"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-lead"),
        ))
        .with_override(Override::set(
            NodePath::layer("guitar-layer-main")
                .with_module("drive-full-stack")
                .with_block("drive-3")
                .with_parameter("drive"),
            0.77,
        ));

    let mut engine = Engine::new(
        seed_id("guitar-engine"),
        "Guitar Engine",
        EngineType::Guitar,
        vec![
            LayerId::from(seed_id("guitar-layer-main")),
            LayerId::from(seed_id("guitar-layer-archetype-jm")),
        ],
        default_scene,
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("template"));
    engine.add_variant(lead_scene);
    engine
}

fn worship_guitar_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("worship-gtr-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("worship-gtr-layer"),
            seed_id("worship-gtr-layer-default"),
        ));

    Engine::new(
        seed_id("worship-gtr-engine"),
        "Worship Guitar Engine",
        EngineType::Guitar,
        vec![LayerId::from(seed_id("worship-gtr-layer"))],
        default_scene,
    )
    .with_metadata(Metadata::new().with_tag("guitar").with_tag("worship"))
}

fn vocal_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("vocal-engine-default"), "Default")
        .with_layer(LayerSelection::new(
            seed_id("vocal-layer-main"),
            seed_id("vocal-layer-main-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("vocal-layer-main")
                .with_module("vox-time")
                .with_block("reverb")
                .with_parameter("mix"),
            0.44,
        ));

    Engine::new(
        seed_id("vocal-engine"),
        "Vocal Engine",
        EngineType::Vocal,
        vec![LayerId::from(seed_id("vocal-layer-main"))],
        default_scene,
    )
    .with_metadata(Metadata::new().with_tag("vocal"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_count() {
        assert_eq!(engines().len(), 7);
    }

    #[test]
    fn includes_required_engine_types() {
        let engines = engines();
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Keys));
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Synth));
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Organ));
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Pad));
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Guitar));
        assert!(engines.iter().any(|e| e.engine_type == EngineType::Vocal));
    }
}
