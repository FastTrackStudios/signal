//! Guitar rig template — 12-module signal chain with 31 block slots.
//!
//! Signal chain order:
//! Source → Dynamics → Special → Drive → Volume Pedal → Pre-FX →
//! Amp → Utility → Modulation → Time → Motion → Mastering
//!
//! Two modules use parallel routing:
//! - **Amp**: dual-amp split (Amp L ∥ Amp R)
//! - **Time**: 3-lane split (Delay 1 → Reverb 1 ∥ Dry ∥ Delay 2 → Reverb 2)

use crate::template::{
    BlockTemplate, EngineTemplate, LayerTemplate, ModuleTemplate, RigTemplate, SignalChainTemplate,
    SignalNodeTemplate, TemplateMetadata,
};
use crate::{BlockType, ModuleType};

/// Standard guitar rig template with 12 modules in a single engine/layer.
///
/// All block preset slots are `Assignment::Unassigned` — this is a structural
/// blueprint awaiting plugin assignments from the user or a preset browser.
#[must_use]
pub fn guitar_rig_template() -> RigTemplate {
    let layer = LayerTemplate::new("Main")
        .with_module(source())
        .with_module(dynamics())
        .with_module(special())
        .with_module(drive())
        .with_module(volume())
        .with_module(prefx())
        .with_module(amp())
        .with_module(utility())
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
    ModuleTemplate::new("Source", ModuleType::Input)
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
                .with_description("Amplifier block — parallel amp pair into gate and amp EQ"),
        )
        .with_node(SignalNodeTemplate::Split {
            lanes: vec![
                SignalChainTemplate::serial(vec![BlockTemplate::new("Amp L", BlockType::Amp)]),
                SignalChainTemplate::serial(vec![BlockTemplate::new("Amp R", BlockType::Amp)]),
            ],
        })
        // Post-amp shaping lives with the amp — the gate and EQ are dialed
        // against the amp's character, so they're part of the module (2×2:
        // amps left, gate → EQ right).
        .with_block(BlockTemplate::new("Gate", BlockType::Gate))
        .with_block(BlockTemplate::new("Amp EQ", BlockType::Eq))
}

/// The Boost gain block the boost footswitch drives.
fn utility() -> ModuleTemplate {
    ModuleTemplate::new("Utility", ModuleType::Volume)
        .with_metadata(TemplateMetadata::new().with_description("Post-amp utilities — boost gain"))
        .with_block(BlockTemplate::new("Boost", BlockType::Volume))
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
        .with_block(BlockTemplate::new("Tremolo", BlockType::Trem))
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
