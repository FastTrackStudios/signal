//! Built-in rig profiles — hardcoded for now, Profile/Stack/Patch entities
//! later. Moved out of the desktop app: profiles are rig domain data, not
//! front-end concern.

use signal_proto::block::BlockType;
use signal_sampler::rig_profile::RigStack;
use signal_sampler::{RigBlock, RigPatch, RigProfile};

// ── Worship profile (hardcoded, one NAM per patch from ~/Downloads) ──────────

const FENDER_DIR: &str =
    "/home/cody/Downloads/Fender Deluxe Reverb '65 Reissue _ Clean _ SM57 + Royer R-121 + Room";
const CUSTOM_DIR: &str = "/home/cody/Downloads/Fender Style Custom Patches Made with Custom IR";
const AC30_MODEL: &str =
    "/home/cody/Downloads/1965 VOX AC30 Top Boost/'65 AC30_6 - The Iconic Cleanish.nam";

/// A bypassed (off-by-default) native FX block of the given type.
fn off(block_type: BlockType, name: &str) -> RigBlock {
    let mut b = RigBlock::of_type(block_type).named(name);
    b.bypassed = true;
    b
}

/// An active native FX block with build-time params (`(name, value)`).
fn on_fx(block_type: BlockType, name: &str, params: &[(&str, &str)]) -> RigBlock {
    let mut b = RigBlock::of_type(block_type).named(name);
    for (k, v) in params {
        b = b.with_param(*k, *v);
    }
    b
}

/// A bypassed native FX block carrying params (so it's ready when un-bypassed).
fn off_fx(block_type: BlockType, name: &str, params: &[(&str, &str)]) -> RigBlock {
    let mut b = on_fx(block_type, name, params);
    b.bypassed = true;
    b
}

/// The Worship guitar profile: 5 footswitch folders (stacks), each an ordered
/// rotation of patches, every patch a distinct NAM amp for now. Patch names are
/// globally unique (the backend addresses patches by name) — the UI strips the
/// folder prefix for display ("Clean Verb" → "Verb", the folder default → its
/// own name shows as "Default").
pub fn worship_profile() -> RigProfile {
    // Each patch is a full built-in-FX chain (all native, from features/fx/):
    //   Comp → NAM amp → [Modulation module: all off] → [Time module]
    // Time module: DLY 1 + Reverb 1 are subtle and ON; DLY 2 + Reverb 2 are
    // more extreme and BYPASSED (ready to kick in). The live rig chain is serial,
    // so this is the serial form of the template's parallel Time splits — with
    // only lane 1 active it's equivalent; true parallel routing is a later rig
    // feature. Delay/Reverb are Time-category, so the FX-Toggle footswitch
    // bypasses them.
    // Block names match the guitar-rig-template slot names so the UI grid
    // resolves each into its template slot (Dynamics→Compressor, Amp→Amp L,
    // Modulation→Chorus/Flanger/Phaser, Motion→Tremolo/Vibrato/Rotary,
    // Time→DLY 1/DLY 2/VERB 1/VERB 2).
    let amp = |name: &str, path: String| {
        RigPatch::new(name)
            .with_block(RigBlock::of_type(BlockType::Compressor).named("Compressor"))
            .with_block(RigBlock::nam(path).named("Amp L"))
            // Modulation + Motion modules — all off by default.
            .with_block(off(BlockType::Chorus, "Chorus"))
            .with_block(off(BlockType::Flanger, "Flanger"))
            .with_block(off(BlockType::Phaser, "Phaser"))
            .with_block(off(BlockType::Trem, "Tremolo"))
            .with_block(off(BlockType::Vibrato, "Vibrato"))
            .with_block(off(BlockType::Rotary, "Rotary"))
            // Time module — subtle pair on, extreme pair bypassed.
            .with_block(on_fx(BlockType::Delay, "DLY 1", &[("mix", "0.08"), ("time", "350"), ("feedback", "0.28")]))
            .with_block(off_fx(BlockType::Delay, "DLY 2", &[("mix", "0.10"), ("time", "600"), ("feedback", "0.62")]))
            .with_block(on_fx(BlockType::Reverb, "VERB 1", &[("mix", "0.08"), ("decay", "0.42"), ("size", "0.45")]))
            .with_block(off_fx(BlockType::Reverb, "VERB 2", &[("mix", "0.10"), ("decay", "0.85"), ("size", "0.92")]))
    };
    RigProfile::new("Worship")
        // Clean
        .with_patch(amp("Clean", format!("{FENDER_DIR}/Fender DRRI _ Clean _ SM57 + Royer R-121 + Room _ Full Rig.nam")))
        .with_patch(amp("Clean Dry", format!("{FENDER_DIR}/Fender DRRI _ Clean _ DI Capture (No Cab).nam")))
        .with_patch(amp("Clean Verb", format!("{FENDER_DIR}/Fender DRRI _ Clean _ Room Only _ Full Rig.nam")))
        // Crunch
        .with_patch(amp("Crunch", format!("{CUSTOM_DIR}/Vibrato Verb AA Crunch.nam")))
        .with_patch(amp("Crunch Edge", format!("{CUSTOM_DIR}/Vibrato Verb AA More.nam")))
        // Drive
        .with_patch(amp("Drive", format!("{CUSTOM_DIR}/Vibrato Verb AA Driven.nam")))
        .with_patch(amp("Drive Edge", format!("{CUSTOM_DIR}/Vibrato Verb AA Cranked.nam")))
        // Lead
        .with_patch(amp("Lead", format!("{CUSTOM_DIR}/Vib Arena Lead LT.nam")))
        .with_patch(amp("Lead POG", format!("{CUSTOM_DIR}/Vibrato Verb Octaver dist.nam")))
        // Ambient
        .with_patch(amp("Ambient", AC30_MODEL.to_string()))
        .with_patch(amp("Ambient Swells", format!("{CUSTOM_DIR}/Vibrato Verb SRV.nam")))
        .with_patch(amp("Ambient Delay Craze", format!("{CUSTOM_DIR}/Vibrato Spaghetti W.nam")))
        // Footswitch stacks (folders)
        .with_stack(RigStack::new("Clean", ["Clean", "Clean Dry", "Clean Verb"]))
        .with_stack(RigStack::new("Crunch", ["Crunch", "Crunch Edge"]))
        .with_stack(RigStack::new("Drive", ["Drive", "Drive Edge"]))
        .with_stack(RigStack::new("Lead", ["Lead", "Lead POG"]))
        .with_stack(RigStack::new("Ambient", ["Ambient", "Ambient Swells", "Ambient Delay Craze"]))
}
