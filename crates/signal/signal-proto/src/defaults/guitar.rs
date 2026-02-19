//! Guitar rig template — 11-module signal chain with 28 block slots.
//!
//! Signal chain order:
//! Source → Dynamics → Special → Drive → Volume Pedal → Pre-FX →
//! Amp → Modulation → Time → Motion → Mastering
//!
//! Two modules use parallel routing:
//! - **Amp**: dual-amp split (Amp L ∥ Amp R)
//! - **Time**: 3-lane split (Delay 1 → Reverb 1 ∥ Dry ∥ Delay 2 → Reverb 2)

use crate::template::{
    BlockTemplate, EngineTemplate, LayerTemplate, ModuleTemplate, RigTemplate, SignalChainTemplate,
    SignalNodeTemplate, TemplateMetadata,
};
use crate::{BlockType, ModuleType};

/// Standard guitar rig template with 11 modules in a single engine/layer.
///
/// All block preset slots are `Assignment::Unassigned` — this is a structural
/// blueprint awaiting plugin assignments from the user or a preset browser.
pub fn guitar_rig_template() -> RigTemplate {
    let layer = LayerTemplate::new("Main")
        .with_module(source())
        .with_module(dynamics())
        .with_module(special())
        .with_module(drive())
        .with_module(volume())
        .with_module(prefx())
        .with_module(amp())
        .with_module(modulation())
        .with_module(time())
        .with_module(motion())
        .with_module(mastering());

    let engine = EngineTemplate::new("Guitar Engine").with_layer(layer);

    RigTemplate::new("Guitar Rig Template")
        .with_rig_type("guitar")
        .with_engine(engine)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Standard guitar signal chain with 11 processing stages")
                .with_tag("guitar"),
        )
}

// ─── Module builders ────────────────────────────────────────────

fn source() -> ModuleTemplate {
    ModuleTemplate::new("Source", ModuleType::Source)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Input conditioning — gate and initial volume"),
        )
        .with_block(BlockTemplate::new("Input Gate", BlockType::Gate))
        .with_block(BlockTemplate::new("Input Volume", BlockType::Volume))
}

fn dynamics() -> ModuleTemplate {
    ModuleTemplate::new("Dynamics", ModuleType::Dynamics)
        .with_metadata(TemplateMetadata::new().with_description("Compression and dynamic control"))
        .with_block(BlockTemplate::new("Compressor", BlockType::Compressor))
}

fn special() -> ModuleTemplate {
    ModuleTemplate::new("Special", ModuleType::Special)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Special effects — wah, filter, pitch, doubler"),
        )
        .with_block(BlockTemplate::new("Envelope Filter", BlockType::Filter))
        .with_block(BlockTemplate::new("Wah Pedal", BlockType::Wah))
        .with_block(BlockTemplate::new("Pitch Octave FX", BlockType::Pitch))
        .with_block(BlockTemplate::new("Doubler", BlockType::Doubler))
}

fn drive() -> ModuleTemplate {
    ModuleTemplate::new("Drive", ModuleType::Drive)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Overdrive and distortion — boost into three drive stages"),
        )
        .with_block(BlockTemplate::new("Boost", BlockType::Boost))
        .with_block(BlockTemplate::new("Drive 1", BlockType::Drive))
        .with_block(BlockTemplate::new("Drive 2", BlockType::Drive))
        .with_block(BlockTemplate::new("Drive 3", BlockType::Drive))
}

fn volume() -> ModuleTemplate {
    ModuleTemplate::new("Volume", ModuleType::Volume)
        .with_metadata(
            TemplateMetadata::new().with_description("Volume pedal — expression-controlled level"),
        )
        .with_block(BlockTemplate::new("Volume Pedal", BlockType::Volume))
}

fn prefx() -> ModuleTemplate {
    ModuleTemplate::new("Pre-FX", ModuleType::PreFx)
        .with_metadata(
            TemplateMetadata::new().with_description("Effects before the amp — EQ and color"),
        )
        .with_block(BlockTemplate::new("Pre EQ", BlockType::Eq))
}

/// Dual-amp parallel split: input fans to two independent amps.
fn amp() -> ModuleTemplate {
    ModuleTemplate::new("Amp", ModuleType::Amp)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Parallel amplifier pair — input splits to both amps"),
        )
        .with_node(SignalNodeTemplate::Split {
            lanes: vec![
                SignalChainTemplate::serial(vec![BlockTemplate::new("Amp L", BlockType::Amp)]),
                SignalChainTemplate::serial(vec![BlockTemplate::new("Amp R", BlockType::Amp)]),
            ],
        })
}

fn modulation() -> ModuleTemplate {
    ModuleTemplate::new("Modulation", ModuleType::Modulation)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Modulation effects — chorus, flanger, phaser"),
        )
        .with_block(BlockTemplate::new("Chorus", BlockType::Chorus))
        .with_block(BlockTemplate::new("Flanger", BlockType::Flanger))
        .with_block(BlockTemplate::new("Phaser", BlockType::Phaser))
}

/// Two sequential parallel splits with dry pass-through:
///
/// ```text
///      ┌─ DLY 1 ─┐     ┌─ VERB 1 ─┐
/// in ──├─ DLY 2 ──┤─────├─ VERB 2 ──┤── out
///      └─ (dry) ──┘     └─ (dry) ───┘
/// ```
fn time() -> ModuleTemplate {
    ModuleTemplate::new("Time", ModuleType::Time)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Parallel time FX — delays then reverbs, each with dry path"),
        )
        // Split 1: two delays in parallel with dry
        .with_node(SignalNodeTemplate::Split {
            lanes: vec![
                SignalChainTemplate::serial(vec![BlockTemplate::new("DLY 1", BlockType::Delay)]),
                SignalChainTemplate::serial(vec![BlockTemplate::new("DLY 2", BlockType::Delay)]),
                SignalChainTemplate::new(), // dry pass-through
            ],
        })
        // Split 2: two reverbs in parallel with dry
        .with_node(SignalNodeTemplate::Split {
            lanes: vec![
                SignalChainTemplate::serial(vec![BlockTemplate::new("VERB 1", BlockType::Reverb)]),
                SignalChainTemplate::serial(vec![BlockTemplate::new("VERB 2", BlockType::Reverb)]),
                SignalChainTemplate::new(), // dry pass-through
            ],
        })
}

fn motion() -> ModuleTemplate {
    ModuleTemplate::new("Motion", ModuleType::Motion)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Rhythmic motion effects — tremolo, vibrato, rotary"),
        )
        .with_block(BlockTemplate::new("Tremolo", BlockType::Tremolo))
        .with_block(BlockTemplate::new("Vibrato", BlockType::Vibrato))
        .with_block(BlockTemplate::new("Rotary", BlockType::Rotary))
}

fn mastering() -> ModuleTemplate {
    ModuleTemplate::new("Mastering", ModuleType::Master)
        .with_metadata(
            TemplateMetadata::new()
                .with_description("Output processing — EQ, multiband comp, limiter, volume"),
        )
        .with_block(BlockTemplate::new("Mastering EQ", BlockType::Eq))
        .with_block(BlockTemplate::new(
            "Multiband Compressor",
            BlockType::Compressor,
        ))
        .with_block(BlockTemplate::new("Limiter", BlockType::Limiter))
        .with_block(BlockTemplate::new("Output Volume", BlockType::Volume))
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::AssignmentLevel;

    #[test]
    fn guitar_template_has_11_modules() {
        let t = guitar_rig_template();
        let engine = &t.engines[0];
        let layer = &engine.layers[0];
        assert_eq!(layer.modules.len(), 11);

        let names: Vec<&str> = layer.modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Source",
                "Dynamics",
                "Special",
                "Drive",
                "Volume",
                "Pre-FX",
                "Amp",
                "Modulation",
                "Time",
                "Motion",
                "Mastering",
            ]
        );
    }

    #[test]
    fn guitar_template_block_counts() {
        let t = guitar_rig_template();
        let modules = &t.engines[0].layers[0].modules;

        let counts: Vec<usize> = modules
            .iter()
            .map(|m| m.chain.missing_assignments().len())
            .collect();
        // Source(2), Dynamics(1), Special(4), Drive(4), Volume(1), Pre-FX(1),
        // Amp(2), Modulation(3), Time(4), Motion(3), Mastering(4)
        assert_eq!(counts, [2, 1, 4, 4, 1, 1, 2, 3, 4, 3, 4]);
    }

    #[test]
    fn guitar_template_total_29_blocks() {
        let t = guitar_rig_template();
        let modules = &t.engines[0].layers[0].modules;
        let total: usize = modules
            .iter()
            .map(|m| m.chain.missing_assignments().len())
            .sum();
        assert_eq!(total, 29);
    }

    #[test]
    fn guitar_template_all_blocks_unassigned() {
        let t = guitar_rig_template();
        let missing = t.missing_assignments();
        // rig_id + engine_id + layer_id + 29 blocks = 32
        let block_missing: Vec<_> = missing
            .iter()
            .filter(|m| m.level == AssignmentLevel::Block)
            .collect();
        assert_eq!(block_missing.len(), 29);
    }

    #[test]
    fn guitar_template_amp_is_parallel() {
        let t = guitar_rig_template();
        let amp_module = &t.engines[0].layers[0].modules[6];
        assert_eq!(amp_module.name, "Amp");

        // The amp module has a single Split node with 2 lanes
        assert_eq!(amp_module.chain.nodes.len(), 1);
        match &amp_module.chain.nodes[0] {
            SignalNodeTemplate::Split { lanes } => {
                assert_eq!(lanes.len(), 2);
                // Each lane has 1 block
                assert_eq!(lanes[0].nodes.len(), 1);
                assert_eq!(lanes[1].nodes.len(), 1);
            }
            _ => panic!("expected Split node in Amp module"),
        }
    }

    #[test]
    fn guitar_template_time_is_two_sequential_splits() {
        let t = guitar_rig_template();
        let time_module = &t.engines[0].layers[0].modules[8];
        assert_eq!(time_module.name, "Time");

        // Two sequential Split nodes
        assert_eq!(time_module.chain.nodes.len(), 2);

        // Split 1: DLY 1 ∥ DLY 2 ∥ dry
        match &time_module.chain.nodes[0] {
            SignalNodeTemplate::Split { lanes } => {
                assert_eq!(lanes.len(), 3);
                assert_eq!(lanes[0].nodes.len(), 1); // DLY 1
                assert_eq!(lanes[1].nodes.len(), 1); // DLY 2
                assert_eq!(lanes[2].nodes.len(), 0); // dry pass-through
            }
            _ => panic!("expected Split node for delays"),
        }

        // Split 2: VERB 1 ∥ VERB 2 ∥ dry
        match &time_module.chain.nodes[1] {
            SignalNodeTemplate::Split { lanes } => {
                assert_eq!(lanes.len(), 3);
                assert_eq!(lanes[0].nodes.len(), 1); // VERB 1
                assert_eq!(lanes[1].nodes.len(), 1); // VERB 2
                assert_eq!(lanes[2].nodes.len(), 0); // dry pass-through
            }
            _ => panic!("expected Split node for reverbs"),
        }
    }

    #[test]
    fn guitar_template_hierarchy() {
        let t = guitar_rig_template();
        assert_eq!(t.name, "Guitar Rig Template");
        assert_eq!(t.rig_type_id.as_ref().unwrap().as_str(), "guitar");
        assert_eq!(t.engines.len(), 1);
        assert_eq!(t.engines[0].name, "Guitar Engine");
        assert_eq!(t.engines[0].layers.len(), 1);
        assert_eq!(t.engines[0].layers[0].name, "Main");
    }

    #[test]
    fn guitar_template_metadata() {
        let t = guitar_rig_template();
        assert!(t.metadata.tags.contains("guitar"));
        assert!(t.metadata.description.as_ref().unwrap().contains("11"));
    }

    #[test]
    fn guitar_template_serial_modules_have_no_splits() {
        let t = guitar_rig_template();
        let modules = &t.engines[0].layers[0].modules;

        // Every module except Amp (6) and Time (8) should be purely serial
        let serial_indices = [0, 1, 2, 3, 4, 5, 7, 9, 10];
        for &i in &serial_indices {
            for node in &modules[i].chain.nodes {
                assert!(
                    matches!(node, SignalNodeTemplate::Block(_)),
                    "module '{}' should be serial but has a Split",
                    modules[i].name,
                );
            }
        }
    }
}
