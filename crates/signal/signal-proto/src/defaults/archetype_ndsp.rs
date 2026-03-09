//! Neural DSP Archetype X virtual controller templates.
//!
//! Each template is custom per plugin and maps all non-MIDI parameters captured
//! from live REAPER enumeration into virtual modules/blocks.

use std::collections::HashSet;

use crate::plugin_block::{ParamMapping, PluginBlockDef, VirtualBlock, VirtualModule};
use crate::{BlockType, ModuleType};

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[derive(Clone, Copy)]
struct BlockSpec {
    id: &'static str,
    label: &'static str,
    block_type: BlockType,
    exact: &'static [&'static str],
    prefixes: &'static [&'static str],
}

impl BlockSpec {
    fn matches(self, name: &str) -> bool {
        self.exact.contains(&name) || self.prefixes.iter().any(|p| name.starts_with(p))
    }
}

#[derive(Clone, Copy)]
struct ModuleSpec {
    id: &'static str,
    label: &'static str,
    module_type: ModuleType,
    blocks: &'static [BlockSpec],
}

#[derive(Clone)]
struct ParamRow {
    index: u32,
    name: String,
    default_value: f32,
}

fn parse_inventory(raw: &str) -> Vec<ParamRow> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let idx = parts.next()?.parse::<u32>().ok()?;
            let name = parts.next()?.trim().to_string();
            let default_value = parts
                .next()
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);
            Some(ParamRow {
                index: idx,
                name,
                default_value,
            })
        })
        .collect()
}

fn build_template(
    plugin_name: &'static str,
    param_count: u32,
    inventory_raw: &'static str,
    modules: &'static [ModuleSpec],
) -> PluginBlockDef {
    let params = parse_inventory(inventory_raw);
    let mut used_indices = HashSet::new();

    let mut def = PluginBlockDef::new(plugin_name, param_count).with_vendor("Neural DSP");

    for module in modules {
        let mut vm = VirtualModule::new(module.id, module.label, module.module_type);

        for block in module.blocks {
            let mut mappings = Vec::new();
            for param in params.iter().filter(|p| block.matches(&p.name)) {
                if used_indices.insert(param.index) {
                    mappings.push(ParamMapping::new(
                        &param.name,
                        param.index,
                        param.default_value,
                    ));
                }
            }

            if !mappings.is_empty() {
                vm = vm.with_block(
                    VirtualBlock::new(block.id, block.label, block.block_type).with_params(mappings),
                );
            }
        }

        def = def.with_module(vm);
    }

    let unmapped: Vec<&ParamRow> = params
        .iter()
        .filter(|p| !used_indices.contains(&p.index))
        .collect();

    assert!(
        unmapped.is_empty(),
        "{plugin_name}: {} unmapped params (first: {:?})",
        unmapped.len(),
        unmapped.first().map(|p| (&p.index, &p.name))
    );

    def
}

const CORY_WONG_RAW: &str =
    include_str!("ndsp_inventory/vst3-archetype-cory-wong-x-neural-dsp.txt");
const JOHN_MAYER_RAW: &str =
    include_str!("ndsp_inventory/vst3-archetype-john-mayer-x-neural-dsp.txt");
const MISHA_MANSOOR_RAW: &str =
    include_str!("ndsp_inventory/vst3-archetype-misha-mansoor-x-neural-dsp.txt");
const NOLLY_RAW: &str = include_str!("ndsp_inventory/vst3-archetype-nolly-x-neural-dsp.txt");
const PETRUCCI_RAW: &str =
    include_str!("ndsp_inventory/vst3-archetype-petrucci-x-neural-dsp.txt");
const RABEA_RAW: &str = include_str!("ndsp_inventory/vst3-archetype-rabea-x-neural-dsp.txt");
const TIM_HENSON_RAW: &str =
    include_str!("ndsp_inventory/vst3-archetype-tim-henson-x-neural-dsp.txt");

const SOURCE_BLOCK: BlockSpec = BlockSpec {
    id: "source-controls",
    label: "Source Controls",
    block_type: BlockType::Input,
    exact: &["Input Gain", "Output Gain", "Transpose", "Preset Previous", "Preset Next", "Bypass"],
    prefixes: &["Gate ", "Doubler "],
};

const CORY_MODULES: &[ModuleSpec] = &[
    ModuleSpec {
        id: "source",
        label: "Source",
        module_type: ModuleType::Input,
        blocks: &[SOURCE_BLOCK],
    },
    ModuleSpec {
        id: "wah",
        label: "Wah",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec {
                id: "wah-section",
                label: "Wah Section",
                block_type: BlockType::Special,
                exact: &["Active Wah Section"],
                prefixes: &[],
            },
            BlockSpec {
                id: "wah",
                label: "Wah",
                block_type: BlockType::Wah,
                exact: &[],
                prefixes: &["Wah "],
            },
            BlockSpec {
                id: "auto-wah",
                label: "Auto Wah",
                block_type: BlockType::Filter,
                exact: &[],
                prefixes: &["Auto Wah "],
            },
        ],
    },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec {
                id: "pre-fx-section",
                label: "Pre-FX Section",
                block_type: BlockType::Special,
                exact: &["Active Pre FX Section"],
                prefixes: &[],
            },
            BlockSpec {
                id: "postal-service",
                label: "The Postal Service",
                block_type: BlockType::Filter,
                exact: &[],
                prefixes: &["The Postal Service "],
            },
            BlockSpec {
                id: "4th-position-comp",
                label: "The 4th Position Comp",
                block_type: BlockType::Compressor,
                exact: &[],
                prefixes: &["The 4th Position Comp "],
            },
            BlockSpec {
                id: "the-tuber",
                label: "The Tuber",
                block_type: BlockType::Drive,
                exact: &[],
                prefixes: &["The Tuber "],
            },
            BlockSpec {
                id: "big-rig-overdrive",
                label: "The Big Rig Overdrive",
                block_type: BlockType::Drive,
                exact: &[],
                prefixes: &["The Big Rig Overdrive "],
            },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec {
                id: "amp-section",
                label: "Amp Section",
                block_type: BlockType::Special,
                exact: &["Active Amp Section", "Amp/Cab Linked", "Amp Type"],
                prefixes: &[],
            },
            BlockSpec {
                id: "di-funk-console",
                label: "D.I. Funk Console",
                block_type: BlockType::Amp,
                exact: &[],
                prefixes: &["D.I. Funk Console "],
            },
            BlockSpec {
                id: "clean-machine",
                label: "The Clean Machine",
                block_type: BlockType::Amp,
                exact: &[],
                prefixes: &["The Clean Machine "],
            },
            BlockSpec {
                id: "amp-snob",
                label: "The Amp Snob",
                block_type: BlockType::Amp,
                exact: &[],
                prefixes: &["The Amp Snob "],
            },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec {
                id: "cab-section",
                label: "Cab Section",
                block_type: BlockType::Special,
                exact: &["Active Cab Section", "Cab Type (Unlinked)"],
                prefixes: &[],
            },
            BlockSpec {
                id: "cab-left",
                label: "Cab Left",
                block_type: BlockType::Cabinet,
                exact: &[],
                prefixes: &["Cab L "],
            },
            BlockSpec {
                id: "room-left",
                label: "Room Left",
                block_type: BlockType::Cabinet,
                exact: &[],
                prefixes: &["Room L "],
            },
            BlockSpec {
                id: "cab-right",
                label: "Cab Right",
                block_type: BlockType::Cabinet,
                exact: &[],
                prefixes: &["Cab R "],
            },
            BlockSpec {
                id: "room-right",
                label: "Room Right",
                block_type: BlockType::Cabinet,
                exact: &[],
                prefixes: &["Room R "],
            },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec {
                id: "eq-section",
                label: "EQ Section",
                block_type: BlockType::Special,
                exact: &["Active EQ Section"],
                prefixes: &[],
            },
            BlockSpec {
                id: "di-funk-console-eq",
                label: "D.I. Funk Console EQ",
                block_type: BlockType::Eq,
                exact: &[],
                prefixes: &["D.I. Funk Console EQ "],
            },
            BlockSpec {
                id: "clean-machine-eq",
                label: "The Clean Machine EQ",
                block_type: BlockType::Eq,
                exact: &[],
                prefixes: &["The Clean Machine EQ "],
            },
            BlockSpec {
                id: "amp-snob-eq",
                label: "The Amp Snob EQ",
                block_type: BlockType::Eq,
                exact: &[],
                prefixes: &["The Amp Snob EQ "],
            },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec {
                id: "post-fx-section",
                label: "Post-FX Section",
                block_type: BlockType::Special,
                exact: &["Active Post FX Section"],
                prefixes: &[],
            },
            BlockSpec {
                id: "80s-chorus",
                label: "The 80s Chorus",
                block_type: BlockType::Chorus,
                exact: &[],
                prefixes: &["The 80s Chorus "],
            },
            BlockSpec {
                id: "delay-y-y",
                label: "Delay-y-y",
                block_type: BlockType::Delay,
                exact: &[],
                prefixes: &["Delay-y-y "],
            },
            BlockSpec {
                id: "the-wash",
                label: "The Wash",
                block_type: BlockType::Reverb,
                exact: &[],
                prefixes: &["The Wash "],
            },
        ],
    },
];

const JOHN_MAYER_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "justa-boost", label: "Justa Boost", block_type: BlockType::Boost, exact: &[], prefixes: &["Justa Boost "] },
            BlockSpec { id: "antelope-filter", label: "Antelope Filter", block_type: BlockType::Filter, exact: &[], prefixes: &["Antelope Filter "] },
            BlockSpec { id: "halfman-od", label: "Halfman OD", block_type: BlockType::Drive, exact: &[], prefixes: &["Halfman OD "] },
            BlockSpec { id: "tealbreaker", label: "Tealbreaker", block_type: BlockType::Drive, exact: &[], prefixes: &["Tealbreaker "] },
            BlockSpec { id: "millipede-delay", label: "Millipede Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Millipede Delay "] },
        ],
    },
    ModuleSpec {
        id: "gravity-tank",
        label: "Gravity Tank",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "gravity-tank-section", label: "Gravity Tank Section", block_type: BlockType::Special, exact: &["Active Gravity Tank Section"], prefixes: &[] },
            BlockSpec { id: "spring-reverb", label: "Spring Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Spring Reverb "] },
            BlockSpec { id: "harmonic-tremolo", label: "Harmonic Tremolo", block_type: BlockType::Trem, exact: &[], prefixes: &["Harmonic Tremolo "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp/Cab Linked", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "smooth-operator-amp", label: "Smooth Operator", block_type: BlockType::Amp, exact: &[], prefixes: &["Smooth Operator "] },
            BlockSpec { id: "headroom-hero-amp", label: "Headroom Hero", block_type: BlockType::Amp, exact: &[], prefixes: &["Headroom Hero "] },
            BlockSpec { id: "signature-83-amp", label: "Signature 83", block_type: BlockType::Amp, exact: &[], prefixes: &["Signature 83 "] },
            BlockSpec { id: "three-in-one-amp", label: "Three-In-One Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Three-In-One Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section", "Cab Type (Unlinked)"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L ", "Cab Smooth Operator L Mic Type", "Cab Headroom Hero L Mic Type", "Cab Signature 83 L Mic Type"] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R ", "Cab Smooth Operator R Mic Type", "Cab Headroom Hero R Mic Type", "Cab Signature 83 R Mic Type"] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "eq-comp",
        label: "EQ & Comp",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-comp-section", label: "EQ & Comp Section", block_type: BlockType::Special, exact: &["Active EQ & Comp Section"], prefixes: &[] },
            BlockSpec { id: "smooth-operator-eq", label: "Smooth Operator EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Smooth Operator EQ "] },
            BlockSpec { id: "smooth-operator-comp", label: "Smooth Operator Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Smooth Operator Compressor "] },
            BlockSpec { id: "headroom-hero-eq", label: "Headroom Hero EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Headroom Hero EQ "] },
            BlockSpec { id: "headroom-hero-comp", label: "Headroom Hero Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Headroom Hero Compressor "] },
            BlockSpec { id: "signature-83-eq", label: "Signature 83 EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Signature 83 EQ "] },
            BlockSpec { id: "signature-83-comp", label: "Signature 83 Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Signature 83 Compressor "] },
            BlockSpec { id: "three-in-one-eq", label: "Three-In-One EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Three-In-One EQ "] },
            BlockSpec { id: "three-in-one-comp", label: "Three-In-One Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Three-In-One Compressor "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "dream-delay", label: "Dream Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Dream Delay "] },
            BlockSpec { id: "studio-verb", label: "Studio Verb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Studio Verb "] },
        ],
    },
];

const MISHA_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "special-fx",
        label: "Special FX",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec { id: "special-fx-section", label: "Special FX Section", block_type: BlockType::Special, exact: &["Active Special FX Section"], prefixes: &[] },
            BlockSpec { id: "laser", label: "Laser", block_type: BlockType::Special, exact: &[], prefixes: &["Laser "] },
            BlockSpec { id: "glitch", label: "Glitch", block_type: BlockType::Special, exact: &[], prefixes: &["Glitch "] },
        ],
    },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "horizon-compressor", label: "Horizon Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Horizon Devices Compressor "] },
            BlockSpec { id: "tape-echo", label: "Tape Echo", block_type: BlockType::Delay, exact: &[], prefixes: &["Tape Echo "] },
            BlockSpec { id: "dual-octaver", label: "Dual Octaver", block_type: BlockType::Pitch, exact: &[], prefixes: &["Dual Octaver "] },
            BlockSpec { id: "precision-drive", label: "Precision Drive", block_type: BlockType::Drive, exact: &[], prefixes: &["Horizon Devices Precision Drive "] },
            BlockSpec { id: "chaos", label: "Chaos", block_type: BlockType::Special, exact: &[], prefixes: &["Chaos "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp/Cab Linked", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "clean-amp", label: "Clean Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Clean Amp "] },
            BlockSpec { id: "rhythm-amp", label: "Rhythm Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Rhythm Amp "] },
            BlockSpec { id: "lead-amp", label: "Lead Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Lead Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section", "Cab Type (Unlinked)"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L ", "Cab Mic L Type"] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R ", "Cab Mic R Type"] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-section", label: "EQ Section", block_type: BlockType::Special, exact: &["Active EQ Section"], prefixes: &[] },
            BlockSpec { id: "clean-amp-eq", label: "Clean Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Clean Amp EQ "] },
            BlockSpec { id: "rhythm-amp-eq", label: "Rhythm Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Rhythm Amp EQ "] },
            BlockSpec { id: "lead-amp-eq", label: "Lead Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Lead Amp EQ "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "modulator", label: "Modulator", block_type: BlockType::Modulation, exact: &[], prefixes: &["Modulator "] },
            BlockSpec { id: "stereo-delay", label: "Stereo Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Stereo Delay "] },
            BlockSpec { id: "reverb", label: "Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Reverb "] },
        ],
    },
];

const NOLLY_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "compressor", label: "Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Compressor "] },
            BlockSpec { id: "overdrive-1", label: "Overdrive 1", block_type: BlockType::Drive, exact: &[], prefixes: &["Overdrive-1 "] },
            BlockSpec { id: "delay-1", label: "Delay 1", block_type: BlockType::Delay, exact: &[], prefixes: &["Delay-1 "] },
            BlockSpec { id: "overdrive-2", label: "Overdrive 2", block_type: BlockType::Drive, exact: &[], prefixes: &["Overdrive-2 "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp/Cab Linked", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "clean-amp", label: "Clean Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Clean Amp "] },
            BlockSpec { id: "crunch-amp", label: "Crunch Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Crunch Amp "] },
            BlockSpec { id: "rhythm-amp", label: "Rhythm Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Rhythm Amp "] },
            BlockSpec { id: "lead-amp", label: "Lead Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Lead Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section", "Cab Type (Unlinked)"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L "] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R "] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-section", label: "EQ Section", block_type: BlockType::Special, exact: &["Active EQ Section"], prefixes: &[] },
            BlockSpec { id: "clean-amp-eq", label: "Clean Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Clean Amp EQ "] },
            BlockSpec { id: "crunch-amp-eq", label: "Crunch Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Crunch Amp EQ "] },
            BlockSpec { id: "rhythm-amp-eq", label: "Rhythm Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Rhythm Amp EQ "] },
            BlockSpec { id: "lead-amp-eq", label: "Lead Amp EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Lead Amp EQ "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "delay-2", label: "Delay 2", block_type: BlockType::Delay, exact: &[], prefixes: &["Delay-2 "] },
            BlockSpec { id: "reverb", label: "Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Reverb "] },
        ],
    },
];

const PETRUCCI_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "wah-comp",
        label: "Wah/Comp",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec { id: "wah-comp-section", label: "Wah/Comp Section", block_type: BlockType::Special, exact: &["Active Wah Comp Section"], prefixes: &[] },
            BlockSpec { id: "wah", label: "Wah", block_type: BlockType::Wah, exact: &[], prefixes: &["Wah "] },
            BlockSpec { id: "compressor", label: "Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Compressor "] },
        ],
    },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "overdrive", label: "Overdrive", block_type: BlockType::Drive, exact: &[], prefixes: &["Overdrive "] },
            BlockSpec { id: "phaser", label: "Phaser", block_type: BlockType::Phaser, exact: &[], prefixes: &["Phaser "] },
            BlockSpec { id: "chorus", label: "Chorus", block_type: BlockType::Chorus, exact: &[], prefixes: &["Chorus "] },
            BlockSpec { id: "flanger", label: "Flanger", block_type: BlockType::Flanger, exact: &[], prefixes: &["Flanger "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "piezo-amp", label: "Piezo Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Piezo Amp "] },
            BlockSpec { id: "clean-amp", label: "Clean Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Clean Amp "] },
            BlockSpec { id: "rhythm-amp", label: "Rhythm Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Rhythm Amp "] },
            BlockSpec { id: "lead-amp", label: "Lead Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Lead Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L ", "Cab Mic L Type"] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R ", "Cab Mic R Type"] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-section", label: "EQ Section", block_type: BlockType::Special, exact: &["Active EQ Section"], prefixes: &[] },
            BlockSpec { id: "clean-eq", label: "Clean EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Clean EQ "] },
            BlockSpec { id: "rhythm-eq", label: "Rhythm EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Rhythm EQ "] },
            BlockSpec { id: "lead-eq", label: "Lead EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Lead EQ "] },
        ],
    },
    ModuleSpec {
        id: "volume",
        label: "Volume",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec { id: "volume-section", label: "Volume Section", block_type: BlockType::Special, exact: &["Active Volume Section"], prefixes: &[] },
            BlockSpec { id: "volume", label: "Volume", block_type: BlockType::Volume, exact: &[], prefixes: &["Volume "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "chorus-2", label: "Chorus 2", block_type: BlockType::Chorus, exact: &[], prefixes: &["Chorus 2 "] },
            BlockSpec { id: "delay", label: "Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Delay "] },
            BlockSpec { id: "reverb", label: "Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Reverb "] },
        ],
    },
];

const RABEA_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "synth",
        label: "Synth",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec { id: "synth-section", label: "Synth Section", block_type: BlockType::Special, exact: &["Active Synth Section"], prefixes: &[] },
            BlockSpec { id: "synth-core", label: "Synth Core", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Mix", "Synth Sensitivity", "Synth Env ", "Synth Tuning", "Synth Pre Post", "Synth Gate", "Synth Output", "Synth Glide"] },
            BlockSpec { id: "synth-arp", label: "Synth Arp", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Arp "] },
            BlockSpec { id: "synth-amp", label: "Synth Amplifier", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Amplifier "] },
            BlockSpec { id: "synth-osc-1", label: "Synth OSC 1", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Osc 1 "] },
            BlockSpec { id: "synth-osc-2", label: "Synth OSC 2", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Osc 2 "] },
            BlockSpec { id: "synth-osc-unison", label: "Synth OSC Unison", block_type: BlockType::Special, exact: &[], prefixes: &["Synth Osc Unison", "Synth Osc Unisons"] },
            BlockSpec { id: "synth-filter", label: "Synth Filter", block_type: BlockType::Filter, exact: &[], prefixes: &["Synth Filter "] },
        ],
    },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "twin-blade-comp", label: "Twin Blade Dual Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Twin Blade Dual Compressor "] },
            BlockSpec { id: "chaos-bed-octaver", label: "Chaos Bed Octaver", block_type: BlockType::Pitch, exact: &[], prefixes: &["Chaos Bed Octaver "] },
            BlockSpec { id: "colossus-fuzz", label: "Colossus Fuzz", block_type: BlockType::Drive, exact: &[], prefixes: &["Colossus Fuzz "] },
            BlockSpec { id: "paragon-overdrive", label: "Paragon Overdrive", block_type: BlockType::Drive, exact: &[], prefixes: &["Paragon Overdrive "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp/Cab Linked", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "clean-amp", label: "Clean Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Clean Amp "] },
            BlockSpec { id: "crunch-amp", label: "Crunch Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Crunch Amp "] },
            BlockSpec { id: "lead-amp", label: "Lead Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Lead Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section", "Cab Type (Unlinked)"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L "] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R "] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-section", label: "EQ Section", block_type: BlockType::Special, exact: &["Active EQ Section"], prefixes: &[] },
            BlockSpec { id: "clean-eq", label: "Clean EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Clean EQ "] },
            BlockSpec { id: "crunch-eq", label: "Crunch EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Crunch EQ "] },
            BlockSpec { id: "lead-eq", label: "Lead EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Lead EQ "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "atlas-delay", label: "Atlas Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Atlas Delay "] },
            BlockSpec { id: "aeons-reverb", label: "Aeons Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Aeons Reverb "] },
        ],
    },
];

const TIM_HENSON_MODULES: &[ModuleSpec] = &[
    ModuleSpec { id: "source", label: "Source", module_type: ModuleType::Input, blocks: &[SOURCE_BLOCK] },
    ModuleSpec {
        id: "pre-fx",
        label: "Pre-FX",
        module_type: ModuleType::PreFx,
        blocks: &[
            BlockSpec { id: "pre-fx-section", label: "Pre-FX Section", block_type: BlockType::Special, exact: &["Active Pre FX Section"], prefixes: &[] },
            BlockSpec { id: "boost", label: "Boost", block_type: BlockType::Boost, exact: &[], prefixes: &["Boost "] },
            BlockSpec { id: "compressor", label: "Compressor", block_type: BlockType::Compressor, exact: &[], prefixes: &["Compressor "] },
            BlockSpec { id: "overdrive", label: "Overdrive", block_type: BlockType::Drive, exact: &[], prefixes: &["Overdrive "] },
        ],
    },
    ModuleSpec {
        id: "amp",
        label: "Amp",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "amp-section", label: "Amp Section", block_type: BlockType::Special, exact: &["Active Amp Section", "Amp Type"], prefixes: &[] },
            BlockSpec { id: "roses-amp", label: "Roses Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Roses Amp "] },
            BlockSpec { id: "cherubs-amp", label: "Cherubs Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Cherubs Amp "] },
            BlockSpec { id: "pink-amp", label: "Pink Amp", block_type: BlockType::Amp, exact: &[], prefixes: &["Pink Amp "] },
        ],
    },
    ModuleSpec {
        id: "cab",
        label: "Cab",
        module_type: ModuleType::Amp,
        blocks: &[
            BlockSpec { id: "cab-section", label: "Cab Section", block_type: BlockType::Special, exact: &["Active Cab Section"], prefixes: &[] },
            BlockSpec { id: "cab-left", label: "Cab Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab L ", "Cherubs Amp Cab Mic L Type", "Pink Amp Cab Mic L Type"] },
            BlockSpec { id: "room-left", label: "Room Left", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room L "] },
            BlockSpec { id: "cab-right", label: "Cab Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Cab R ", "Cherubs Amp Cab Mic R Type", "Pink Amp Cab Mic R Type"] },
            BlockSpec { id: "room-right", label: "Room Right", block_type: BlockType::Cabinet, exact: &[], prefixes: &["Room R "] },
        ],
    },
    ModuleSpec {
        id: "multivoicer",
        label: "Multivoicer",
        module_type: ModuleType::Special,
        blocks: &[
            BlockSpec { id: "multivoicer-section", label: "Multivoicer Section", block_type: BlockType::Special, exact: &["Active Multivoicer Section"], prefixes: &[] },
            BlockSpec { id: "multivoicer-global", label: "Multivoicer Global", block_type: BlockType::Pitch, exact: &[], prefixes: &["Multivoicer Tuning", "Multivoicer Root", "Multivoicer Mode", "Multivoicer Quantize", "Multivoicer Level Adder", "Multivoicer Pan Multiplier", "Multivoicer Detune Multiplier", "Multivoicer Delay Multiplier", "Multivoicer Unisons", "Multivoicer Width", "Multivoicer Tone", "Multivoicer Output", "Multivoicer Midi Enabled", "Multivoicer DI Level"] },
            BlockSpec { id: "multivoicer-v1", label: "Multivoicer Voice 1", block_type: BlockType::Pitch, exact: &[], prefixes: &["Multivoicer Voice 1 "] },
            BlockSpec { id: "multivoicer-v2", label: "Multivoicer Voice 2", block_type: BlockType::Pitch, exact: &[], prefixes: &["Multivoicer Voice 2 "] },
            BlockSpec { id: "multivoicer-v3", label: "Multivoicer Voice 3", block_type: BlockType::Pitch, exact: &[], prefixes: &["Multivoicer Voice 3 "] },
            BlockSpec { id: "multivoicer-v4", label: "Multivoicer Voice 4", block_type: BlockType::Pitch, exact: &[], prefixes: &["Multivoicer Voice 4 "] },
        ],
    },
    ModuleSpec {
        id: "eq",
        label: "EQ",
        module_type: ModuleType::Eq,
        blocks: &[
            BlockSpec { id: "eq-section", label: "EQ Section", block_type: BlockType::Special, exact: &["Active EQ Section"], prefixes: &[] },
            BlockSpec { id: "roses-eq", label: "Roses EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Roses EQ "] },
            BlockSpec { id: "cherubs-eq", label: "Cherubs EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Cherubs EQ "] },
            BlockSpec { id: "pink-eq", label: "Pink EQ", block_type: BlockType::Eq, exact: &[], prefixes: &["Pink EQ "] },
        ],
    },
    ModuleSpec {
        id: "post-fx",
        label: "Post-FX",
        module_type: ModuleType::Time,
        blocks: &[
            BlockSpec { id: "post-fx-section", label: "Post-FX Section", block_type: BlockType::Special, exact: &["Active Post FX Section"], prefixes: &[] },
            BlockSpec { id: "chorus", label: "Chorus", block_type: BlockType::Chorus, exact: &[], prefixes: &["Chorus "] },
            BlockSpec { id: "delay", label: "Delay", block_type: BlockType::Delay, exact: &[], prefixes: &["Delay "] },
            BlockSpec { id: "reverb", label: "Reverb", block_type: BlockType::Reverb, exact: &[], prefixes: &["Reverb "] },
        ],
    },
];

pub fn archetype_cory_wong_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Cory Wong X (Neural DSP)",
        2233,
        CORY_WONG_RAW,
        CORY_MODULES,
    )
}

pub fn archetype_john_mayer_x_full() -> PluginBlockDef {
    build_template(
        "VST3: Archetype John Mayer X (Neural DSP)",
        2256,
        JOHN_MAYER_RAW,
        JOHN_MAYER_MODULES,
    )
}

pub fn archetype_misha_mansoor_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Misha Mansoor X (Neural DSP)",
        2264,
        MISHA_MANSOOR_RAW,
        MISHA_MODULES,
    )
}

pub fn archetype_nolly_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Nolly X (Neural DSP)",
        2244,
        NOLLY_RAW,
        NOLLY_MODULES,
    )
}

pub fn archetype_petrucci_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Petrucci X (Neural DSP)",
        2239,
        PETRUCCI_RAW,
        PETRUCCI_MODULES,
    )
}

pub fn archetype_rabea_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Rabea X (Neural DSP)",
        2278,
        RABEA_RAW,
        RABEA_MODULES,
    )
}

pub fn archetype_tim_henson_x() -> PluginBlockDef {
    build_template(
        "VST3: Archetype Tim Henson X (Neural DSP)",
        2252,
        TIM_HENSON_RAW,
        TIM_HENSON_MODULES,
    )
}

pub fn archetype_x_templates() -> Vec<PluginBlockDef> {
    vec![
        archetype_cory_wong_x(),
        archetype_john_mayer_x_full(),
        archetype_misha_mansoor_x(),
        archetype_nolly_x(),
        archetype_petrucci_x(),
        archetype_rabea_x(),
        archetype_tim_henson_x(),
    ]
}

/// Full plugin names for supported Archetype X templates.
pub const NDSP_ARCHETYPE_X_PLUGIN_NAMES: &[&str] = &[
    "VST3: Archetype Cory Wong X (Neural DSP)",
    "VST3: Archetype John Mayer X (Neural DSP)",
    "VST3: Archetype Misha Mansoor X (Neural DSP)",
    "VST3: Archetype Nolly X (Neural DSP)",
    "VST3: Archetype Petrucci X (Neural DSP)",
    "VST3: Archetype Rabea X (Neural DSP)",
    "VST3: Archetype Tim Henson X (Neural DSP)",
];

/// User-facing short label (e.g. "Archetype Cory Wong X").
pub fn archetype_label(plugin_name: &str) -> String {
    plugin_name
        .trim_start_matches("VST3: ")
        .trim_end_matches(" (Neural DSP)")
        .to_string()
}

/// Stable seed slug used to derive deterministic IDs.
pub fn archetype_seed_slug(plugin_name: &str) -> String {
    slugify(&archetype_label(plugin_name))
}

/// Find which Archetype X plugin appears in an rfxchain text payload.
pub fn detect_archetype_plugin_in_rfxchain(rfxchain: &str) -> Option<String> {
    NDSP_ARCHETYPE_X_PLUGIN_NAMES
        .iter()
        .find(|name| rfxchain.contains(**name))
        .map(|s| s.to_string())
}

/// Deterministic module preset seed keys for a plugin, in virtual module order.
///
/// Keys are suitable for `seed_id(key)` and match `signal-storage` NDSP module seeding.
pub fn archetype_module_seed_keys(plugin_name: &str) -> Option<Vec<String>> {
    let def = archetype_x_templates()
        .into_iter()
        .find(|d| d.plugin_name == plugin_name)?;
    let prefix = format!("ndsp-{}", archetype_seed_slug(plugin_name));
    Some(
        def.modules
            .iter()
            .map(|m| format!("{prefix}-module-{}", m.id))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_build_and_validate() {
        let defs = archetype_x_templates();
        assert_eq!(defs.len(), 7);
        for def in defs {
            def.validate().unwrap();
            assert!(def.modules.iter().any(|m| !m.blocks.is_empty()));
            assert!(
                def.all_blocks()
                    .iter()
                    .flat_map(|b| b.params.iter())
                    .all(|p| !p.name.starts_with("MIDI CC "))
            );
        }
    }
}
