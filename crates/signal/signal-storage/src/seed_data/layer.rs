//! Layer seed data — default layer collections for development/demo.

use signal_proto::layer::{BlockRef, Layer, LayerRef, LayerSnapshot, ModuleRef};
use signal_proto::metadata::Metadata;
use signal_proto::overrides::{NodePath, Override};
use signal_proto::{seed_id, EngineType};
use signal_proto::defaults::archetype_ndsp::{archetype_label, archetype_module_seed_keys, NDSP_ARCHETYPE_X_PLUGIN_NAMES};

/// All default layer collections.
pub fn layers() -> Vec<Layer> {
    let mut out = vec![
        keys_layer_core(),
        keys_layer_space(),
        guitar_layer_main(),
        guitar_layer_archetype_jm(),
        synth_layer_osc(),
        synth_layer_motion(),
        synth_layer_texture(),
        organ_layer_body(),
        organ_layer_air(),
        pad_layer_foundation(),
        pad_layer_shimmer(),
        vocal_layer_main(),
    ];
    out.extend(guitar_layers_archetype_ndsp());
    out
}

fn keys_layer_core() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("keys-layer-core-default"), "Default")
        .with_block(BlockRef::new(seed_id("compressor-cp1x")))
        .with_block(BlockRef::new(seed_id("eq-proq4")))
        .with_block(BlockRef::new(seed_id("volume-utility")))
        .with_override(Override::set(
            NodePath::block("keys-core-comp").with_parameter("threshold"),
            0.46,
        ));

    let bright_variant = LayerSnapshot::new(seed_id("keys-layer-core-bright"), "Bright")
        .with_block(BlockRef::new(seed_id("compressor-cp1x")))
        .with_block(BlockRef::new(seed_id("eq-proq4")).with_variant(seed_id("eq-proq4-hifi")))
        .with_block(BlockRef::new(seed_id("volume-utility")))
        .with_override(Override::set(
            NodePath::block("keys-core-eq").with_parameter("high_shelf"),
            0.62,
        ));

    let mut layer = Layer::new(
        seed_id("keys-layer-core"),
        "Keys Core",
        EngineType::Keys,
        default_variant,
    );
    layer.add_variant(bright_variant);
    layer
}

fn keys_layer_space() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("keys-layer-space-default"), "Default")
        .with_layer(
            LayerRef::new(seed_id("guitar-layer-main"))
                .with_variant(seed_id("guitar-layer-main-default")),
        )
        .with_module(
            ModuleRef::new(seed_id("__phantom__keys-megarig-time"))
                .with_variant(seed_id("__phantom__keys-megarig-time-default")),
        )
        .with_block(
            BlockRef::new(seed_id("__phantom__keys-megarig-space-verb"))
                .with_variant(seed_id("__phantom__keys-megarig-space-verb-default")),
        )
        .with_override(Override::set(
            NodePath::module("__phantom__keys-megarig-time")
                .with_block("verb-1")
                .with_parameter("mix"),
            0.58,
        ));

    let private_variant = LayerSnapshot::new(
        seed_id("__phantom__keys-megarig-keys-layer-space"),
        "__phantom__ MegaRig Private",
    )
    .with_layer(
        LayerRef::new(seed_id("guitar-layer-main")).with_variant(seed_id("guitar-layer-main-lead")),
    )
    .with_module(
        ModuleRef::new(seed_id("__phantom__keys-megarig-time"))
            .with_variant(seed_id("__phantom__keys-megarig-time-default")),
    )
    .with_block(
        BlockRef::new(seed_id("__phantom__keys-megarig-space-verb"))
            .with_variant(seed_id("__phantom__keys-megarig-space-verb-wide")),
    )
    .with_metadata(
        Metadata::new()
            .with_tag("hidden")
            .with_tag("keys-megarig")
            .with_description("Private layer snapshot scoped to Keys MegaRig"),
    );

    let mut layer = Layer::new(
        seed_id("keys-layer-space"),
        "Keys Space",
        EngineType::Keys,
        default_variant,
    );
    layer.add_variant(private_variant);
    layer
}

fn guitar_layer_main() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("guitar-layer-main-default"), "Default")
        .with_module(ModuleRef::new(seed_id("gtr-source")))
        .with_module(ModuleRef::new(seed_id("gtr-dynamics")))
        .with_module(ModuleRef::new(seed_id("gtr-special")))
        .with_module(ModuleRef::new(seed_id("drive-full-stack")))
        .with_module(ModuleRef::new(seed_id("gtr-volume")))
        .with_module(ModuleRef::new(seed_id("gtr-pre-fx")))
        .with_module(ModuleRef::new(seed_id("gtr-amp")))
        .with_module(ModuleRef::new(seed_id("gtr-modulation")))
        .with_module(ModuleRef::new(seed_id("time-parallel")))
        .with_module(ModuleRef::new(seed_id("gtr-motion")))
        .with_module(ModuleRef::new(seed_id("gtr-master")))
        .with_override(Override::set(
            NodePath::module("drive-full-stack")
                .with_block("drive-1")
                .with_parameter("drive"),
            0.48,
        ));

    let lead_variant = LayerSnapshot::new(seed_id("guitar-layer-main-lead"), "Lead")
        .with_module(ModuleRef::new(seed_id("gtr-source")))
        .with_module(ModuleRef::new(seed_id("gtr-dynamics")))
        .with_module(ModuleRef::new(seed_id("gtr-special")))
        .with_module(
            ModuleRef::new(seed_id("drive-full-stack"))
                .with_variant(seed_id("drive-full-stack-push")),
        )
        .with_module(ModuleRef::new(seed_id("gtr-volume")))
        .with_module(ModuleRef::new(seed_id("gtr-pre-fx")))
        .with_module(ModuleRef::new(seed_id("gtr-amp")))
        .with_module(ModuleRef::new(seed_id("gtr-modulation")))
        .with_module(
            ModuleRef::new(seed_id("time-parallel")).with_variant(seed_id("time-parallel-ambient")),
        )
        .with_module(ModuleRef::new(seed_id("gtr-motion")))
        .with_module(ModuleRef::new(seed_id("gtr-master")))
        .with_override(Override::set(
            NodePath::module("time-parallel")
                .with_block("verb-1")
                .with_parameter("mix"),
            0.67,
        ));

    let mut layer = Layer::new(
        seed_id("guitar-layer-main"),
        "Guitar Main",
        EngineType::Guitar,
        default_variant,
    );
    layer.add_variant(lead_variant);
    layer
}

fn synth_layer_osc() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("synth-layer-osc-default"), "Default")
        .with_block(BlockRef::new(seed_id("pitch-pog2")))
        .with_block(BlockRef::new(seed_id("filter-volcano")))
        .with_block(BlockRef::new(seed_id("compressor-cp1x")))
        .with_override(Override::set(
            NodePath::block("synth-osc-filter").with_parameter("cutoff"),
            0.42,
        ));

    let alt_variant = LayerSnapshot::new(seed_id("synth-layer-osc-alt"), "Alt")
        .with_block(BlockRef::new(seed_id("pitch-whammy")))
        .with_block(
            BlockRef::new(seed_id("filter-volcano"))
                .with_variant(seed_id("filter-volcano-fixed-filter")),
        )
        .with_block(BlockRef::new(seed_id("compressor-cp1x")))
        .with_override(Override::set(
            NodePath::block("synth-osc-filter").with_parameter("resonance"),
            0.64,
        ));

    let mut layer = Layer::new(
        seed_id("synth-layer-osc"),
        "Synth Osc",
        EngineType::Synth,
        default_variant,
    );
    layer.add_variant(alt_variant);
    layer
}

fn synth_layer_motion() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("synth-layer-motion-default"), "Default")
        .with_module(ModuleRef::new(seed_id("time-parallel")))
        .with_block(BlockRef::new(seed_id("phaser-phase90")))
        .with_override(Override::set(
            NodePath::module("time-parallel")
                .with_block("dly-1")
                .with_parameter("feedback"),
            0.52,
        ));

    Layer::new(
        seed_id("synth-layer-motion"),
        "Synth Motion",
        EngineType::Synth,
        default_variant,
    )
}

fn synth_layer_texture() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("synth-layer-texture-default"), "Default")
        .with_block(BlockRef::new(seed_id("reverb-slo")))
        .with_block(BlockRef::new(seed_id("delay-dd8")))
        .with_block(BlockRef::new(seed_id("tremolo-harmonic")))
        .with_override(Override::set(
            NodePath::block("texture-verb").with_parameter("mix"),
            0.66,
        ));

    Layer::new(
        seed_id("synth-layer-texture"),
        "Synth Texture",
        EngineType::Synth,
        default_variant,
    )
}

fn organ_layer_body() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("organ-layer-body-default"), "Default")
        .with_block(BlockRef::new(seed_id("rotary-leslie")))
        .with_block(BlockRef::new(seed_id("eq-reaeq")))
        .with_override(Override::set(
            NodePath::block("organ-rotary").with_parameter("speed"),
            0.50,
        ));

    Layer::new(
        seed_id("organ-layer-body"),
        "Organ Body",
        EngineType::Organ,
        default_variant,
    )
}

fn organ_layer_air() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("organ-layer-air-default"), "Default")
        .with_block(BlockRef::new(seed_id("chorus-tal")))
        .with_block(BlockRef::new(seed_id("reverb-bigsky")))
        .with_override(Override::set(
            NodePath::block("organ-air-verb").with_parameter("mix"),
            0.54,
        ));

    Layer::new(
        seed_id("organ-layer-air"),
        "Organ Air",
        EngineType::Organ,
        default_variant,
    )
}

fn pad_layer_foundation() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("pad-layer-foundation-default"), "Default")
        .with_block(BlockRef::new(seed_id("filter-volcano")))
        .with_block(BlockRef::new(seed_id("volume-pedal")))
        .with_override(Override::set(
            NodePath::block("pad-foundation-filter").with_parameter("cutoff"),
            0.33,
        ));

    Layer::new(
        seed_id("pad-layer-foundation"),
        "Pad Foundation",
        EngineType::Pad,
        default_variant,
    )
}

fn pad_layer_shimmer() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("pad-layer-shimmer-default"), "Default")
        .with_block(
            BlockRef::new(seed_id("reverb-bigsky")).with_variant(seed_id("reverb-bigsky-shimmer")),
        )
        .with_block(BlockRef::new(seed_id("delay-timeless")))
        .with_override(Override::set(
            NodePath::block("pad-shimmer-delay").with_parameter("mix"),
            0.49,
        ));

    Layer::new(
        seed_id("pad-layer-shimmer"),
        "Pad Shimmer",
        EngineType::Pad,
        default_variant,
    )
}

fn guitar_layer_archetype_jm() -> Layer {
    let default_variant =
        LayerSnapshot::new(seed_id("guitar-layer-archetype-jm-default"), "Default")
            .with_module(ModuleRef::new(seed_id("jm-pedals")))
            .with_module(ModuleRef::new(seed_id("jm-pre-fx")))
            .with_module(ModuleRef::new(seed_id("jm-amp-module")))
            .with_module(ModuleRef::new(seed_id("jm-cab-module")))
            .with_module(ModuleRef::new(seed_id("jm-eq-module")))
            .with_module(ModuleRef::new(seed_id("jm-post-fx")));

    let lead_variant = LayerSnapshot::new(seed_id("guitar-layer-archetype-jm-lead"), "Lead")
        .with_module(ModuleRef::new(seed_id("jm-pedals")).with_variant(seed_id("jm-pedals-lead")))
        .with_module(ModuleRef::new(seed_id("jm-pre-fx")))
        .with_module(
            ModuleRef::new(seed_id("jm-amp-module")).with_variant(seed_id("jm-amp-module-crunch")),
        )
        .with_module(ModuleRef::new(seed_id("jm-cab-module")))
        .with_module(ModuleRef::new(seed_id("jm-eq-module")))
        .with_module(
            ModuleRef::new(seed_id("jm-post-fx")).with_variant(seed_id("jm-post-fx-ambient")),
        );

    let mut layer = Layer::new(
        seed_id("guitar-layer-archetype-jm"),
        "Archetype JM",
        EngineType::Guitar,
        default_variant,
    );
    layer.add_variant(lead_variant);
    layer
}

fn guitar_layers_archetype_ndsp() -> Vec<Layer> {
    NDSP_ARCHETYPE_X_PLUGIN_NAMES
        .iter()
        .filter_map(|plugin_name| {
            let module_keys = archetype_module_seed_keys(plugin_name)?;
            let slug = signal_proto::defaults::archetype_ndsp::archetype_seed_slug(plugin_name);
            let short_name = archetype_label(plugin_name).replace("Archetype ", "");
            let layer_seed = format!("guitar-layer-archetype-{}", slug);
            let layer_default_seed = format!("{layer_seed}-default");

            let mut default_variant = LayerSnapshot::new(seed_id(&layer_default_seed), "Default");
            for key in &module_keys {
                default_variant = default_variant.with_module(ModuleRef::new(seed_id(key)));
            }

            Some(Layer::new(
                seed_id(&layer_seed),
                format!("Archetype {short_name}"),
                EngineType::Guitar,
                default_variant,
            ))
        })
        .collect()
}

fn vocal_layer_main() -> Layer {
    let default_variant = LayerSnapshot::new(seed_id("vocal-layer-main-default"), "Default")
        .with_module(ModuleRef::new(seed_id("vox-rescue")))
        .with_module(ModuleRef::new(seed_id("vox-correction")))
        .with_module(ModuleRef::new(seed_id("vox-tonal")))
        .with_module(ModuleRef::new(seed_id("vox-modulation")))
        .with_module(ModuleRef::new(seed_id("vox-time")))
        .with_override(Override::set(
            NodePath::module("vox-tonal")
                .with_block("saturator")
                .with_parameter("mix"),
            0.62,
        ));

    Layer::new(
        seed_id("vocal-layer-main"),
        "Vocal Main",
        EngineType::Vocal,
        default_variant,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_count() {
        assert_eq!(layers().len(), 19);
    }

    #[test]
    fn includes_required_engine_types() {
        let layers = layers();
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Keys)
                .count(),
            2
        );
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Synth)
                .count(),
            3
        );
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Organ)
                .count(),
            2
        );
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Pad)
                .count(),
            2
        );
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Guitar)
                .count(),
            9
        );
        assert_eq!(
            layers
                .iter()
                .filter(|l| l.engine_type == EngineType::Vocal)
                .count(),
            1
        );
    }
}
