//! Neural DSP Archetype X seeded block/module presets.
//!
//! Converts virtual controller templates into real seed-time `Preset` and
//! `ModulePreset` entries, mirroring the Archetype JM structure:
//! - one block preset per virtual block
//! - one module preset per virtual module

use std::collections::HashMap;

use signal_proto::defaults::archetype_x_templates;
use signal_proto::{
    seed_id, Block, BlockParameter, Module, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModuleSnapshot, Preset, PresetId, Snapshot,
};

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric();
        if keep {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn archetype_label(plugin_name: &str) -> String {
    plugin_name
        .trim_start_matches("VST3: ")
        .trim_end_matches(" (Neural DSP)")
        .to_string()
}

fn archetype_seed_prefix(plugin_name: &str) -> String {
    format!("ndsp-{}", slugify(&archetype_label(plugin_name)))
}

/// One seeded block preset per virtual block across all NDSP Archetype X plugins.
pub fn block_presets() -> Vec<Preset> {
    let mut out = Vec::new();

    for def in archetype_x_templates() {
        let prefix = archetype_seed_prefix(&def.plugin_name);
        let label = archetype_label(&def.plugin_name);

        for module in def.modules {
            for block in module.blocks {
                let seed_key = format!("{prefix}-{module_id}-{block_id}", module_id = module.id, block_id = block.id);
                let default_snapshot_key = format!("{seed_key}-default");

                let params = block
                    .params
                    .iter()
                    .map(|p| {
                        BlockParameter::new(
                            format!("p{}", p.plugin_param_index),
                            p.name.clone(),
                            p.default_value,
                        )
                    })
                    .collect();

                out.push(Preset::new(
                    seed_id(&seed_key),
                    format!("{label}: {} / {}", module.label, block.label),
                    block.block_type,
                    Snapshot::new(
                        seed_id(&default_snapshot_key),
                        "Default",
                        Block::from_parameters(params),
                    ),
                    vec![],
                ));
            }
        }
    }

    out
}

/// One seeded module preset per virtual module across all NDSP Archetype X plugins.
pub fn module_presets() -> Vec<ModulePreset> {
    let mut out = Vec::new();

    for def in archetype_x_templates() {
        let prefix = archetype_seed_prefix(&def.plugin_name);
        let label = archetype_label(&def.plugin_name);

        let mut block_id_map: HashMap<(String, String), PresetId> = HashMap::new();
        for module in &def.modules {
            for block in &module.blocks {
                let seed_key = format!("{prefix}-{module_id}-{block_id}", module_id = module.id, block_id = block.id);
                block_id_map.insert(
                    (module.id.clone(), block.id.clone()),
                    PresetId::from(seed_id(&seed_key)),
                );
            }
        }

        for module in def.modules {
            let module_seed_key = format!("{prefix}-module-{}", module.id);
            let module_default_key = format!("{module_seed_key}-default");

            let blocks: Vec<ModuleBlock> = module
                .blocks
                .iter()
                .map(|block| {
                    let preset_id = block_id_map
                        .get(&(module.id.clone(), block.id.clone()))
                        .expect("NDSP module block preset id must exist")
                        .clone();

                    ModuleBlock::new(
                        block.id.clone(),
                        block.label.clone(),
                        block.block_type,
                        ModuleBlockSource::PresetDefault {
                            preset_id,
                            saved_at_version: None,
                        },
                    )
                })
                .collect();

            out.push(ModulePreset::new(
                seed_id(&module_seed_key),
                format!("{label}: {}", module.label),
                module.module_type,
                ModuleSnapshot::new(seed_id(&module_default_key), "Default", Module::from_blocks(blocks)),
                vec![],
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_block_and_module_presets_for_all_archetypes() {
        let blocks = block_presets();
        let modules = module_presets();

        assert!(blocks.len() > 50);
        assert!(modules.len() >= 40);

        // Spot-check naming and wiring style.
        assert!(
            blocks
                .iter()
                .any(|p| p.name().contains("Archetype Cory Wong X") && p.name().contains("The Tuber"))
        );
        assert!(
            modules
                .iter()
                .any(|m| m.name().contains("Archetype Tim Henson X") && m.name().contains("Multivoicer"))
        );
    }
}
