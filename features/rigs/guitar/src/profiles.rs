//! Built-in rig profiles — hardcoded for now, Profile/Stack/Patch entities
//! later. Moved out of the desktop app: profiles are rig domain data, not
//! front-end concern.

use signal_proto::block::BlockType;
use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};
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

/// One preset in the pool — a complete amp tone (NAM capture). Patches
/// *point at* presets; several patches can share one (scene/override
/// differences layer on top later).
#[derive(Clone)]
pub struct PresetDef {
    pub name: String,
    pub nam: String,
}

/// One patch: a name in the profile + the preset it points at + the
/// overrides that make it different from the preset (the domain's
/// `Patch { target, overrides }` — see `signal_proto::overrides`).
#[derive(Clone)]
pub struct PatchDef {
    pub name: String,
    pub preset: String,
    pub overrides: Vec<Override>,
}

impl PatchDef {
    /// The unique module names this patch overrides (for the UI's
    /// override badges).
    pub fn override_modules(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for ov in &self.overrides {
            for seg in ov.path.segments() {
                if let NodePathSegment::Module(m) = seg {
                    if !out.iter().any(|x| x.eq_ignore_ascii_case(m)) {
                        out.push(m.clone());
                    }
                }
            }
        }
        out
    }
}

/// The editable profile definition: the preset pool, the patches pointing
/// into it, and the footswitch stacks grouping the patches.
#[derive(Clone)]
pub struct ProfileDef {
    pub name: String,
    pub presets: Vec<PresetDef>,
    pub patches: Vec<PatchDef>,
    pub stacks: Vec<(String, Vec<String>)>,
}

/// The Worship profile definition: a small preset pool, twelve patches
/// pointing into it (several share a preset — the override system will
/// carry their differences), five footswitch stacks.
pub fn worship_def() -> ProfileDef {
    let preset = |name: &str, nam: String| PresetDef { name: name.to_string(), nam };
    let patch = |name: &str, preset: &str| PatchDef {
        name: name.to_string(),
        preset: preset.to_string(),
        overrides: Vec::new(),
    };
    let with_ovr = |mut p: PatchDef, ovr: Vec<Override>| {
        p.overrides = ovr;
        p
    };
    // Override paths address module → block → parameter, exactly the
    // domain's NodePath. Values are normalized 0–1 (ParameterValue clamps).
    let time_param = |block: &str, param: &str, v: f32| {
        Override::set(
            NodePath::module("Time").with_block(block).with_parameter(param),
            v,
        )
    };
    ProfileDef {
        name: "Worship".to_string(),
        presets: vec![
            preset("Fender Clean", format!("{FENDER_DIR}/Fender DRRI _ Clean _ SM57 + Royer R-121 + Room _ Full Rig.nam")),
            preset("Fender DI", format!("{FENDER_DIR}/Fender DRRI _ Clean _ DI Capture (No Cab).nam")),
            preset("AA Crunch", format!("{CUSTOM_DIR}/Vibrato Verb AA Crunch.nam")),
            preset("AA Drive", format!("{CUSTOM_DIR}/Vibrato Verb AA Driven.nam")),
            preset("Arena Lead", format!("{CUSTOM_DIR}/Vib Arena Lead LT.nam")),
            preset("AC30 Clean", AC30_MODEL.to_string()),
        ],
        patches: vec![
            // Clean
            patch("Clean", "Fender Clean"),
            patch("Clean Dry", "Fender DI"),
            with_ovr(
                patch("Clean Verb", "Fender Clean"),
                vec![time_param("VERB 1", "mix", 0.22)],
            ),
            // Crunch
            patch("Crunch", "AA Crunch"),
            with_ovr(
                patch("Crunch Edge", "AA Crunch"),
                // "Edge": gate off so every rattle rings.
                vec![Override::bypass(
                    NodePath::module("Utility").with_block("Gate"),
                    true,
                )],
            ),
            // Drive
            patch("Drive", "AA Drive"),
            with_ovr(
                patch("Drive Edge", "AA Drive"),
                vec![time_param("DLY 1", "mix", 0.12)],
            ),
            // Lead
            patch("Lead", "Arena Lead"),
            with_ovr(
                patch("Lead POG", "Arena Lead"),
                vec![time_param("VERB 1", "mix", 0.12)],
            ),
            // Ambient
            patch("Ambient", "AC30 Clean"),
            with_ovr(
                patch("Ambient Swells", "AC30 Clean"),
                vec![
                    time_param("VERB 1", "mix", 0.18),
                    time_param("VERB 1", "decay", 0.8),
                ],
            ),
            with_ovr(
                patch("Ambient Delay Craze", "AC30 Clean"),
                vec![
                    Override::bypass(NodePath::module("Time").with_block("DLY 2"), false),
                    time_param("DLY 2", "mix", 0.16),
                ],
            ),
        ],
        stacks: vec![
            ("Clean".to_string(), vec!["Clean".into(), "Clean Dry".into(), "Clean Verb".into()]),
            ("Crunch".to_string(), vec!["Crunch".into(), "Crunch Edge".into()]),
            ("Drive".to_string(), vec!["Drive".into(), "Drive Edge".into()]),
            ("Lead".to_string(), vec!["Lead".into(), "Lead POG".into()]),
            ("Ambient".to_string(), vec!["Ambient".into(), "Ambient Swells".into(), "Ambient Delay Craze".into()]),
        ],
    }
}

/// Build the runtime [`RigProfile`] from a definition: every patch gets the
/// standard chain (Comp → its preset's NAM → Gate/Boost → mod/motion →
/// Time), so pointing a patch at a different preset swaps the amp capture.
pub fn build_profile(def: &ProfileDef) -> RigProfile {
    // The standard full chain around one NAM capture — see the block-name
    // comments in the module docs (names match the guitar-rig-template slots).
    let amp = |name: &str, path: String| {
        RigPatch::new(name)
            .with_block(RigBlock::of_type(BlockType::Compressor).named("Compressor"))
            // Volume pedal (clean gain, unity default) — the Control view's
            // left pedal drives it.
            .with_block(on_fx(BlockType::Volume, "Volume Pedal", &[("gain_db", "0")]))
            .with_block(RigBlock::nam(path).named("Amp L"))
            // Post-amp shaping, part of the Amp module: gate into the amp
            // EQ — both dialed against the amp's character.
            .with_block(on_fx(BlockType::Gate, "Gate", &[("threshold", "-50")]))
            // The amp EQ ships with the electric-guitar "magic frequencies"
            // preset (eq-ui cheatsheet zones): low cut at 80 Hz, then flat
            // named bells on body / character / honk / presence.
            .with_block(on_fx(
                BlockType::Eq,
                "Amp EQ",
                &[
                    ("b1_used", "1"), ("b1_on", "1"), ("b1_freq", "80"), ("b1_shape", "3"),
                    ("b2_used", "1"), ("b2_on", "1"), ("b2_freq", "212"),
                    ("b3_used", "1"), ("b3_on", "1"), ("b3_freq", "560"),
                    ("b4_used", "1"), ("b4_on", "1"), ("b4_freq", "1400"),
                    ("b5_used", "1"), ("b5_on", "1"), ("b5_freq", "5500"),
                ],
            ))
            // Boost gain block the footswitch drives (0 dB until engaged).
            .with_block(on_fx(BlockType::Volume, "Boost", &[("gain_db", "0")]))
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
    let nam_of = |preset: &str| {
        def.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(preset))
            .map(|p| p.nam.clone())
            .unwrap_or_default()
    };
    let mut profile = RigProfile::new(&def.name);
    for p in &def.patches {
        let mut patch = amp(&p.name, nam_of(&p.preset));
        apply_overrides(&mut patch, &p.overrides);
        profile = profile.with_patch(patch);
    }
    for (name, patches) in &def.stacks {
        profile = profile.with_stack(RigStack::new(name, patches.clone()));
    }
    profile
}

/// Apply a patch's overrides onto its built chain: block-addressed `Set`
/// writes the param (normalized value as the block's build-time param) and
/// `Bypass` flips the block's configured bypass. Module segments scope the
/// path (and drive the UI badges); application resolves by block name.
fn apply_overrides(patch: &mut RigPatch, overrides: &[Override]) {
    for ov in overrides {
        let block_name = ov.path.segments().iter().find_map(|s| match s {
            NodePathSegment::Block(b) => Some(b.clone()),
            _ => None,
        });
        let param_name = ov.path.segments().iter().find_map(|s| match s {
            NodePathSegment::Parameter(p) => Some(p.clone()),
            _ => None,
        });
        let Some(block_name) = block_name else { continue };
        let Some(block) = patch
            .chain
            .iter_mut()
            .find(|b| b.name.eq_ignore_ascii_case(&block_name))
        else {
            continue;
        };
        match &ov.op {
            NodeOverrideOp::Set(v) => {
                if let Some(param) = param_name {
                    // Replace an existing build-time param or append one.
                    match block.params.iter_mut().find(|p| p.name == param) {
                        Some(p) => p.value = v.get().to_string(),
                        None => block.params.push(signal_sampler::rig_node::Param {
                            name: param,
                            value: v.get().to_string(),
                        }),
                    }
                }
            }
            NodeOverrideOp::Bypass(b) => block.bypassed = *b,
            _ => {} // ReplaceRef / Insert / Remove / Enable — later.
        }
    }
}

/// The Worship runtime profile (kept for existing callers).
pub fn worship_profile() -> RigProfile {
    build_profile(&worship_def())
}

/// One setlist entry: the song, its starting stack, and its section map.
#[derive(Clone)]
pub struct SetlistSong {
    pub name: String,
    /// The stack (footswitch folder) whose current patch the song opens on.
    pub stack: usize,
    /// Section names, in order (Intro, V1, Chorus, …).
    pub sections: Vec<String>,
}

/// The demo worship setlist — hardcoded next to the profile until setlists
/// become entities driven over the Setlist service.
pub fn worship_setlist() -> Vec<SetlistSong> {
    fn song(name: &str, stack: usize, sections: &[&str]) -> SetlistSong {
        SetlistSong {
            name: name.to_string(),
            stack,
            sections: sections.iter().map(|s| s.to_string()).collect(),
        }
    }
    vec![
        song("Great Are You Lord", 0, &["Intro", "V1", "Chorus", "V2", "Chorus", "Bridge", "Outro"]),
        song("What A Beautiful Name", 1, &["Intro", "V1", "Chorus 1", "V2", "Chorus 2", "Bridge", "Outro"]),
        song("Graves Into Gardens", 2, &["Intro", "V1", "Chorus", "V2", "Bridge", "Vamp", "Outro"]),
        song("How Great Thou Art", 0, &["Intro", "V1", "Chorus", "V2", "V3", "Outro"]),
        song("See A Victory", 3, &["Intro", "V1", "Chorus", "V2", "Bridge", "Outro"]),
        song("Goodness Of God", 4, &["Intro", "V1", "Chorus", "V2", "Bridge", "Outro"]),
        song("Firm Foundation", 1, &["Intro", "V1", "Chorus", "V2", "Bridge", "Vamp"]),
        song("Praise", 2, &["Intro", "V1", "Chorus", "V2", "Bridge", "Outro"]),
    ]
}
