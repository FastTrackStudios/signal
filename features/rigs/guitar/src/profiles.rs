//! Built-in rig profiles — hardcoded for now, Profile/Stack/Patch entities
//! later. Moved out of the desktop app: profiles are rig domain data, not
//! front-end concern.

use facet::Facet;
use signal_proto::block::BlockType;
use signal_sampler::rig_profile::RigStack;
use signal_sampler::{RigBlock, RigPatch, RigProfile};

// ── Worship profile (hardcoded, one NAM per patch from ~/Downloads) ──────────

const FENDER_DIR: &str =
    "/home/cody/Downloads/Fender Deluxe Reverb '65 Reissue _ Clean _ SM57 + Royer R-121 + Room";
const CUSTOM_DIR: &str = "/home/cody/Downloads/Fender Style Custom Patches Made with Custom IR";
// TONE3000 tone 82521 — "1964 VOX AC30 Top Boost Super Twin - Edge of
// Breakup - A2" by amalgamaudio, the most-downloaded AC30 on the catalog
// (35.9K). A JMI-era Top Boost Super Twin into a VOX 2x12 with Alnico
// Silvers, mic'd R121/R160/U87, so it is a complete rig in one capture —
// which is what this chain needs, since it has no cab IR block after the
// amp. Fetch it with `signal tone3000 fetch 82521`.
const AC30_MODEL: &str = "models/VX TB30 BR Edge0 BAL2 CAB FREE.nam";

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
#[derive(Clone, Debug, Facet)]
pub struct PresetDef {
    pub name: String,
    pub nam: String,
    /// SHA-256 of the capture — the key the NAM catalog indexes by, and so
    /// the way this preset finds out what it *is*: creator, licence, the
    /// tone it came from, its cover art.
    ///
    /// Content-addressed rather than path-addressed on purpose. A library
    /// reorganised on disk, or copied to another machine, keeps its
    /// attribution; a path would not. Empty for a capture added by hand that
    /// the catalog has never seen.
    #[facet(default)]
    pub hash: String,
}

/// One NAM option inside a drive block preset — pedals are commonly
/// captured at several settings (gain stages, sides, channels).
#[derive(Clone, Debug, Facet)]
pub struct DriveOptionDef {
    pub name: String,
    pub nam: String,
    /// SHA-256 of the capture. See [`PresetDef::hash`].
    #[facet(default)]
    pub hash: String,
}

/// A **Drive Block Preset**: the thing a drive slot loads. Wraps one or
/// more NAM captures of the pedal with a quick option switch.
#[derive(Clone, Debug, Facet)]
pub struct DrivePresetDef {
    pub name: String,
    pub options: Vec<DriveOptionDef>,
}

/// The drive slots the standard chain builds, in board order.
///
/// One definition, because two things walk this list and they must not
/// drift: the chain builder, which creates the blocks, and an import,
/// which claims the first slot no profile has assigned yet.
pub const DRIVE_SLOTS: [&str; 3] = ["Drive 1", "Drive 2", "Drive 3"];

/// A drive slot in the chain: which preset it runs and which of the
/// preset's NAM options is selected.
#[derive(Clone, Debug, Facet)]
pub struct DriveSlotDef {
    /// Chain block name ("Drive 1", …).
    pub block: String,
    pub preset: String,
    pub option: usize,
}

/// The drive block preset library.
#[must_use]
pub fn drive_presets() -> Vec<DrivePresetDef> {
    // The seeds carry no hash: they are files shipped with the binary, not
    // catalog entries. An import fills it in; a seeded one simply has no
    // provenance to show until the same capture is fetched from its source.
    let opt = |name: &str, nam: &str| DriveOptionDef {
        name: name.to_string(),
        nam: nam.to_string(),
        hash: String::new(),
    };
    vec![
        DrivePresetDef {
            name: "King of Tone".to_string(),
            options: vec![
                opt(
                    "Both Sides",
                    "/home/cody/Downloads/King of Tone/both-sides/King of Tone both sides.nam",
                ),
                opt(
                    "Red = Boost",
                    "/home/cody/Downloads/King of Tone/red-boost/King of Tone ver4 Red channel set to Boost.nam",
                ),
            ],
        },
        DrivePresetDef {
            name: "JHS Morning Glory".to_string(),
            options: vec![
                opt(
                    "Low Gain",
                    "/home/cody/Downloads/JHS Morning Glory/JHS Morning Glory V4 - Low Gain Blue.nam",
                ),
                opt(
                    "Medium Gain",
                    "/home/cody/Downloads/JHS Morning Glory/JHS Morning Glory V4 - Medium Gain Blue.nam",
                ),
                opt(
                    "High Gain",
                    "/home/cody/Downloads/JHS Morning Glory/JHS Morning Glory V4 - High Gain Blue.nam",
                ),
            ],
        },
    ]
}

/// One patch: a name in the profile + the preset it points at + the
/// overrides that make it different from the preset (the domain's
/// `Patch { target, overrides }` — see `signal_proto::overrides`).
#[derive(Clone, Debug, Facet)]
pub struct PatchDef {
    pub name: String,
    pub preset: String,
    /// Manual output trim (dB) on top of the loudness calibration —
    /// patch-level levelling when the ears disagree with the meter.
    pub trim_db: f32,
    /// Boost level recalled with the patch (0 = boost off).
    pub boost_db: f32,
    pub overrides: Vec<OverrideDef>,
}

/// One patch-level override, flat and text-friendly: which module/block it
/// touches, the op, and the value. Examples (styx):
///
/// ```text
/// {module Time, block "VERB 1", param mix, op set, value 0.35}
/// {module Time, block "DLY 2", param "", op bypass, value 0}   // 0 = engage
/// ```
///
/// Ops: `set` writes `param` = `value`; `bypass` sets the block's bypass
/// (value ≥ 0.5 = bypassed, < 0.5 = engaged).
#[derive(Clone, Debug, Facet)]
pub struct OverrideDef {
    pub module: String,
    pub block: String,
    pub param: String,
    pub op: String,
    pub value: f32,
    /// String payload for `set_text` (e.g. an IR wav path); empty otherwise.
    pub text: String,
}

impl OverrideDef {
    #[must_use]
    pub fn set(module: &str, block: &str, param: &str, value: f32) -> Self {
        Self {
            module: module.to_string(),
            block: block.to_string(),
            param: param.to_string(),
            op: "set".to_string(),
            value,
            text: String::new(),
        }
    }

    #[must_use]
    pub fn set_text(module: &str, block: &str, param: &str, text: &str) -> Self {
        Self {
            module: module.to_string(),
            block: block.to_string(),
            param: param.to_string(),
            op: "set_text".to_string(),
            value: 0.0,
            text: text.to_string(),
        }
    }

    #[must_use]
    pub fn bypass(module: &str, block: &str, bypassed: bool) -> Self {
        Self {
            module: module.to_string(),
            block: block.to_string(),
            param: String::new(),
            op: "bypass".to_string(),
            value: if bypassed { 1.0 } else { 0.0 },
            text: String::new(),
        }
    }
}

impl PatchDef {
    /// The unique module names this patch overrides (for the UI's
    /// override badges).
    #[must_use]
    pub fn override_modules(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for ov in &self.overrides {
            if !ov.module.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&ov.module)) {
                out.push(ov.module.clone());
            }
        }
        out
    }
}

/// The editable profile definition: the preset pool, the patches pointing
/// into it, and the footswitch stacks grouping the patches.
#[derive(Clone, Debug, Facet)]
pub struct ProfileDef {
    /// Drive-slot assignments (block → drive preset + selected option).
    pub drives: Vec<DriveSlotDef>,
    pub name: String,
    pub presets: Vec<PresetDef>,
    pub patches: Vec<PatchDef>,
    pub stacks: Vec<StackDef>,
}

/// A footswitch stack: a name and its patch rotation.
#[derive(Clone, Debug, Facet)]
pub struct StackDef {
    pub name: String,
    pub patches: Vec<String>,
}

/// The Worship profile definition: a small preset pool, twelve patches
/// pointing into it (several share a preset — the override system will
/// carry their differences), five footswitch stacks.
#[must_use]
pub fn worship_def() -> ProfileDef {
    let preset = |name: &str, nam: String| PresetDef {
        name: name.to_string(),
        nam,
        hash: String::new(),
    };
    let stack = |name: &str, patches: &[&str]| StackDef {
        name: name.to_string(),
        patches: patches
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    };
    let patch = |name: &str, preset: &str| PatchDef {
        name: name.to_string(),
        preset: preset.to_string(),
        trim_db: 0.0,
        boost_db: 0.0,
        overrides: Vec::new(),
    };
    let with_ovr = |mut p: PatchDef, ovr: Vec<OverrideDef>| {
        p.overrides = ovr;
        p
    };
    let time_param = |block: &str, param: &str, v: f32| OverrideDef::set("Time", block, param, v);
    let drives = vec![
        DriveSlotDef {
            block: "Drive 1".to_string(),
            preset: "King of Tone".to_string(),
            option: 0,
        },
        DriveSlotDef {
            block: "Drive 2".to_string(),
            preset: "JHS Morning Glory".to_string(),
            option: 1,
        },
    ];
    ProfileDef {
        drives,
        name: "Worship".to_string(),
        presets: vec![
            preset(
                "Fender Clean",
                format!(
                    "{FENDER_DIR}/Fender DRRI _ Clean _ SM57 + Royer R-121 + Room _ Full Rig.nam"
                ),
            ),
            preset(
                "Fender DI",
                format!("{FENDER_DIR}/Fender DRRI _ Clean _ DI Capture (No Cab).nam"),
            ),
            preset(
                "AA Crunch",
                format!("{CUSTOM_DIR}/Vibrato Verb AA Crunch.nam"),
            ),
            preset(
                "AA Drive",
                format!("{CUSTOM_DIR}/Vibrato Verb AA Driven.nam"),
            ),
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
                // "Edge": gate off so every rattle rings; dotted-8th tape
                // delay, hotter and regenerating.
                vec![
                    OverrideDef::bypass("Utility", "Gate", true),
                    time_param("DLY 1", "tap_div_l", 1.0),
                    time_param("DLY 1", "tap_div_r", 1.0),
                    time_param("DLY 1", "mix", 0.45),
                    time_param("DLY 1", "feedback", 0.4),
                ],
            ),
            // Drive
            patch("Drive", "AA Drive"),
            with_ovr(
                patch("Drive Edge", "AA Drive"),
                vec![time_param("DLY 1", "mix", 0.12)],
            ),
            // Lead
            {
                let mut p = patch("Lead", "Arena Lead");
                p.boost_db = 3.0;
                p
            },
            with_ovr(
                patch("Lead POG", "Arena Lead"),
                vec![time_param("VERB 1", "mix", 0.12)],
            ),
            // Ambient
            with_ovr(
                patch("Ambient", "AC30 Clean"),
                // Cloud verb, long and dark; delay a touch hotter.
                vec![
                    time_param("VERB 1", "algorithm", 4.0),
                    time_param("VERB 1", "mix", 0.35),
                    time_param("VERB 1", "decay", 0.63),
                    time_param("VERB 1", "tone", -0.5),
                    time_param("DLY 1", "mix", 0.25),
                ],
            ),
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
                    OverrideDef::bypass("Time", "DLY 2", false),
                    time_param("DLY 2", "mix", 0.16),
                ],
            ),
        ],
        stacks: vec![
            stack("Clean", &["Clean", "Clean Dry", "Clean Verb"]),
            stack("Crunch", &["Crunch", "Crunch Edge"]),
            stack("Drive", &["Drive", "Drive Edge"]),
            stack("Lead", &["Lead", "Lead POG"]),
            stack(
                "Ambient",
                &["Ambient", "Ambient Swells", "Ambient Delay Craze"],
            ),
        ],
    }
}

/// Build the runtime [`RigProfile`] from a definition: every patch gets the
/// standard chain (Comp → its preset's NAM → Gate/Boost → mod/motion →
/// Time), so pointing a patch at a different preset swaps the amp capture.
/// Build a drive slot's block: NAM-backed when the profile assigns a
/// drive preset to it, a transparent placeholder otherwise. Off by
/// default either way — the board engages them.
fn drive_block(def: &ProfileDef, dps: &[DrivePresetDef], block: &str) -> RigBlock {
    let assigned = def
        .drives
        .iter()
        .find(|d| d.block.eq_ignore_ascii_case(block))
        .and_then(|d| {
            dps.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&d.preset))
                .and_then(|p| p.options.get(d.option).cloned())
        });
    let mut b = match assigned {
        Some(opt) => RigBlock::of_type(BlockType::Drive)
            .with_nam(opt.nam)
            .with_param("drive", "0.5"),
        None => RigBlock::of_type(BlockType::Drive).with_param("drive", "0.5"),
    };
    b = b.named(block);
    b.bypassed = true;
    b
}

/// Append the drive board to a patch under construction: a boost, then every
/// slot in [`DRIVE_SLOTS`] in board order. All off until the control surface
/// engages them, and a slot the profile has not assigned builds as a
/// transparent placeholder so every slot stays addressable either way.
fn drive_board(def: &ProfileDef, dps: &[DrivePresetDef], patch: RigPatch) -> RigPatch {
    let boosted = patch.with_block(off_fx(BlockType::Boost, "Boost", &[("drive", "0.5")]));
    DRIVE_SLOTS
        .iter()
        .fold(boosted, |p, slot| p.with_block(drive_block(def, dps, slot)))
}

#[must_use]
pub fn build_profile(def: &ProfileDef, dps: &[DrivePresetDef]) -> RigProfile {
    // The standard full chain around one NAM capture — see the block-name
    // comments in the module docs (names match the guitar-rig-template slots).
    let amp = |name: &str, path: String| {
        // The head of the chain, up to and including the drive board. Split
        // out because the board is a fold over `DRIVE_SLOTS` rather than a
        // fixed run of `.with_block` calls.
        let head = RigPatch::new(name)
            .with_block(on_fx(
                BlockType::Compressor,
                "Compressor",
                &[("threshold", "-40")],
            ))
            // Volume pedal (clean gain, unity default) — the Control view's
            // left pedal drives it.
            .with_block(on_fx(
                BlockType::Volume,
                "Volume Pedal",
                &[("gain_db", "0")],
            ));
        drive_board(def, dps, head)
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
                    ("b1_used", "1"),
                    ("b1_on", "1"),
                    ("b1_freq", "80"),
                    ("b1_shape", "3"),
                    ("b2_used", "1"),
                    ("b2_on", "1"),
                    ("b2_freq", "212"),
                    ("b3_used", "1"),
                    ("b3_on", "1"),
                    ("b3_freq", "560"),
                    ("b4_used", "1"),
                    ("b4_on", "1"),
                    ("b4_freq", "1400"),
                    ("b5_used", "1"),
                    ("b5_on", "1"),
                    ("b5_freq", "5500"),
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
            .with_block(on_fx(
                BlockType::Delay,
                "DLY 1",
                &[
                    ("mix", "0.2"),
                    ("style", "0"),
                    ("time", "350"),
                    ("feedback", "0.28"),
                    ("tap_div_l", "0"),
                    ("tap_div_r", "0"),
                ],
            ))
            .with_block(off_fx(
                BlockType::Delay,
                "DLY 2",
                &[
                    ("mix", "0.10"),
                    ("time", "600"),
                    ("feedback", "0.62"),
                    ("tap_div_l", "1"),
                    ("tap_div_r", "1"),
                ],
            ))
            .with_block(on_fx(
                BlockType::Reverb,
                "VERB 1",
                &[("mix", "0.08"), ("decay", "0.42"), ("size", "0.45")],
            ))
            .with_block(off_fx(
                BlockType::Reverb,
                "VERB 2",
                &[("mix", "0.10"), ("decay", "0.85"), ("size", "0.92")],
            ))
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
        patch.output_trim_db += p.trim_db;
        apply_overrides(&mut patch, &p.overrides);
        profile = profile.with_patch(patch);
    }
    for st in &def.stacks {
        profile = profile.with_stack(RigStack::new(&st.name, st.patches.clone()));
    }
    profile
}

/// Apply a patch's overrides onto its built chain: `set` writes the param
/// on the named block, `bypass` flips its configured bypass. Blocks resolve
/// by name; the module field scopes the path (and drives the UI badges).
fn apply_overrides(patch: &mut RigPatch, overrides: &[OverrideDef]) {
    for ov in overrides {
        let Some(block) = patch
            .chain
            .iter_mut()
            .find(|b| b.name.eq_ignore_ascii_case(&ov.block))
        else {
            continue;
        };
        match ov.op.to_ascii_lowercase().as_str() {
            "set" if !ov.param.is_empty() => {
                match block.params.iter_mut().find(|p| p.name == ov.param) {
                    Some(p) => p.value = ov.value.to_string(),
                    None => block.params.push(signal_sampler::rig_node::Param {
                        name: ov.param.clone(),
                        value: ov.value.to_string(),
                    }),
                }
            }
            "set_text" if !ov.param.is_empty() => {
                match block.params.iter_mut().find(|p| p.name == ov.param) {
                    Some(p) => p.value = ov.text.clone(),
                    None => block.params.push(signal_sampler::rig_node::Param {
                        name: ov.param.clone(),
                        value: ov.text.clone(),
                    }),
                }
            }
            "bypass" => block.bypassed = ov.value >= 0.5,
            other => tracing::warn!("override op '{other}' not understood (set|bypass)"),
        }
    }
}

/// The Worship runtime profile (kept for existing callers).
#[must_use]
pub fn worship_profile() -> RigProfile {
    build_profile(&worship_def(), &drive_presets())
}

/// A song in the library: name plus its default key and tempo. Setlist
/// entries reference songs and may override key/tempo per set.
#[derive(Clone, Debug, Facet)]
pub struct SongDef {
    pub name: String,
    /// Default key (e.g. "G", "C", "E", "A").
    pub key: String,
    /// Default tempo.
    pub bpm: u32,
    /// The stack (footswitch folder) the song opens on.
    pub stack: usize,
    /// Section names, in order (empty until section maps come back).
    /// Named song parts (Verse / Chorus / Bridge…), selectable live.
    pub parts: Vec<String>,
    /// Per-song switch tuning: each entry re-points a stack's landing
    /// patch while this song is up (e.g. Clean → "Clean Verb"), so the
    /// footswitches are dialed for the song. Cursors reset on recall.
    pub stack_defaults: Vec<StackDefaultDef>,
}

/// One song-level stack override: which patch a stack lands on.
#[derive(Clone, Debug, Facet)]
pub struct StackDefaultDef {
    pub stack: String,
    pub patch: String,
}

/// One setlist entry: a song reference with optional per-set key/tempo
/// overrides (the same song can sit in different keys on different sets).
#[derive(Clone, Debug, Facet)]
pub struct SetlistEntryDef {
    pub song: String,
    /// Per-set key override; empty = the song's default.
    pub key: String,
    /// Per-set tempo override; 0 = the song's default.
    pub bpm: u32,
}

/// A named setlist (e.g. "XR Wednesday 7-8-26").
#[derive(Clone, Debug, Facet)]
pub struct SetlistDef {
    pub name: String,
    pub entries: Vec<SetlistEntryDef>,
}

/// The song library — defaults live here; sets override per entry.
#[must_use]
pub fn song_library() -> Vec<SongDef> {
    fn song(name: &str, key: &str, bpm: u32) -> SongDef {
        SongDef {
            name: name.to_string(),
            key: key.to_string(),
            bpm,
            stack: 0, // open on Clean; per-song stacks come with song editing
            parts: Vec::new(),
            stack_defaults: Vec::new(),
        }
    }
    vec![
        song("What a God", "G", 79),
        song("No Other Name", "G", 74),
        song("Owe You Praise", "C", 122),
        song("WASHED", "E", 139),
        song("Who Else", "A", 68),
        song("Build My Life / With Everything", "A", 70),
    ]
}

/// The default setlists — XR + CYA, dated.
#[must_use]
pub fn default_setlists() -> Vec<SetlistDef> {
    fn entry(song: &str) -> SetlistEntryDef {
        SetlistEntryDef {
            song: song.to_string(),
            key: String::new(),
            bpm: 0,
        }
    }
    vec![
        SetlistDef {
            name: "XR Wednesday 7-8-26".to_string(),
            entries: vec![
                entry("What a God"),
                entry("No Other Name"),
                entry("Owe You Praise"),
            ],
        },
        SetlistDef {
            name: "CYA 7-9-26".to_string(),
            entries: vec![
                entry("WASHED"),
                entry("Who Else"),
                entry("Build My Life / With Everything"),
            ],
        },
    ]
}

/// MIDI footswitch mapping — `midi.styx`.
///
/// `tap_ccs` are the five gesture switches in order (stacks 1–4 + tap tempo; hold = the hold layer);
/// `direct` maps extra CCs straight onto hold-layer slots
/// (0 Ambient / 1 FX toggle / 2 next song / 3 boost / 4 tuner).
#[derive(Clone, Debug, Facet)]
pub struct MidiMapDef {
    pub tap_ccs: Vec<u32>,
    pub direct: Vec<DirectCcDef>,
}

#[derive(Clone, Debug, Facet)]
pub struct DirectCcDef {
    pub cc: u32,
    pub slot: u32,
}

#[must_use]
pub fn default_midi_map() -> MidiMapDef {
    MidiMapDef {
        tap_ccs: vec![101, 102, 103, 104, 105],
        direct: (0..5)
            .map(|i| DirectCcDef {
                cc: 106 + i,
                slot: i,
            })
            .collect(),
    }
}

/// One keyboard binding — `keys` is "ctrl+1" / "meta+shift+t" style.
///
/// Modifiers: ctrl/meta/shift/alt + a key name. `action` uses the rig
/// action vocabulary: stack:N, patch:N, preset:N, song:next|prev|N,
/// setlist:N, mode:pedals|profile|setlist, `toggle_fx`, boost, tuner,
/// mute, tap, reload.
#[derive(Clone, Debug, Facet)]
pub struct KeyBindingDef {
    pub keys: String,
    pub action: String,
}

#[must_use]
pub fn default_keymap() -> Vec<KeyBindingDef> {
    let b = |keys: &str, action: &str| KeyBindingDef {
        keys: keys.to_string(),
        action: action.to_string(),
    };
    vec![
        // Stacks mirror the footswitches on ctrl+1..5.
        b("ctrl+1", "stack:0"),
        b("ctrl+2", "stack:1"),
        b("ctrl+3", "stack:2"),
        b("ctrl+4", "stack:3"),
        b("ctrl+5", "stack:4"),
        b("ctrl+arrowright", "song:next"),
        b("ctrl+arrowleft", "song:prev"),
        b("ctrl+f", "toggle_fx"),
        b("ctrl+b", "boost"),
        b("ctrl+t", "tuner"),
        b("ctrl+m", "mute"),
        b("ctrl+space", "tap"),
        b("ctrl+r", "reload"),
    ]
}

// ── Importing a downloaded capture ───────────────────────────────────────────

/// What importing a pedal capture did to the library.
///
/// Returned rather than logged in place so the caller decides whether the
/// change is worth persisting and rebuilding for, and so a test can assert
/// the routing without a live rig behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveImport {
    /// The preset already held this exact capture — nothing changed. A
    /// download the user repeats is the same file, not a second option.
    AlreadyPresent,
    /// Added as another option of a pedal already on the board.
    Option { preset: String },
    /// A pedal new to the board, which claimed the named drive slot.
    Slot { preset: String, block: String },
    /// A pedal new to the board, but every drive slot was already
    /// assigned. The capture is in the library and a slot can be pointed
    /// at it by editing the styx.
    NoFreeSlot { preset: String },
}

/// Route a downloaded pedal capture into the drive library.
///
/// Captures group by the tone they came from: the first one creates the
/// preset and claims the first drive slot nothing is assigned to, and later
/// ones become further options of it — which is how a pedal captured at
/// three gain settings ends up as one pedal with three options rather than
/// three pedals.
///
/// Pure, so the caller persists and rebuilds; it mutates only what it is
/// handed.
pub fn import_drive_capture(
    def: &mut ProfileDef,
    dps: &mut Vec<DrivePresetDef>,
    group: &str,
    option: &str,
    nam_path: &str,
    hash: &str,
) -> DriveImport {
    let fresh = !dps.iter().any(|p| p.name.eq_ignore_ascii_case(group));
    if fresh {
        dps.push(DrivePresetDef {
            name: group.to_string(),
            options: Vec::new(),
        });
    }
    let Some(preset) = dps.iter_mut().find(|p| p.name.eq_ignore_ascii_case(group)) else {
        return DriveImport::AlreadyPresent;
    };
    if preset.options.iter().any(|o| o.nam == nam_path) {
        return DriveImport::AlreadyPresent;
    }
    preset.options.push(DriveOptionDef {
        name: option.to_string(),
        nam: nam_path.to_string(),
        hash: hash.to_string(),
    });
    if !fresh {
        return DriveImport::Option {
            preset: group.to_string(),
        };
    }
    let free = DRIVE_SLOTS
        .iter()
        .find(|s| !def.drives.iter().any(|d| d.block.eq_ignore_ascii_case(s)));
    let Some(block) = free else {
        return DriveImport::NoFreeSlot {
            preset: group.to_string(),
        };
    };
    def.drives.push(DriveSlotDef {
        block: (*block).to_string(),
        preset: group.to_string(),
        option: 0,
    });
    DriveImport::Slot {
        preset: group.to_string(),
        block: (*block).to_string(),
    }
}

#[cfg(test)]
mod import_tests {
    use super::{DriveImport, DrivePresetDef, ProfileDef, import_drive_capture};

    /// A profile with no drive slots assigned yet.
    fn empty_profile() -> ProfileDef {
        ProfileDef {
            drives: Vec::new(),
            name: "Test".to_string(),
            presets: Vec::new(),
            patches: Vec::new(),
            stacks: Vec::new(),
        }
    }

    #[test]
    fn a_pedals_captures_group_into_one_preset_on_one_slot() {
        let mut def = empty_profile();
        let mut dps: Vec<DrivePresetDef> = Vec::new();

        // The first capture of a tone creates the pedal and puts it on the
        // board.
        let first = import_drive_capture(&mut def, &mut dps, "Klon", "Low", "/n/low.nam", "h-low");
        assert_eq!(
            first,
            DriveImport::Slot {
                preset: "Klon".to_string(),
                block: "Drive 1".to_string(),
            }
        );

        // A second capture of the SAME tone is another option of it, not a
        // second pedal, and does not claim a second slot.
        let second = import_drive_capture(&mut def, &mut dps, "Klon", "High", "/n/high.nam", "h-high");
        assert_eq!(
            second,
            DriveImport::Option {
                preset: "Klon".to_string()
            }
        );
        assert_eq!(dps.len(), 1);
        assert_eq!(dps[0].options.len(), 2);
        assert_eq!(def.drives.len(), 1);
        assert_eq!(def.drives[0].block, "Drive 1");

        // A different tone takes the next free slot.
        let other = import_drive_capture(&mut def, &mut dps, "Timmy", "Stock", "/n/timmy.nam", "h-timmy");
        assert_eq!(
            other,
            DriveImport::Slot {
                preset: "Timmy".to_string(),
                block: "Drive 2".to_string(),
            }
        );
    }

    #[test]
    fn the_same_file_twice_is_the_same_capture() {
        let mut def = empty_profile();
        let mut dps: Vec<DrivePresetDef> = Vec::new();
        import_drive_capture(&mut def, &mut dps, "Klon", "Low", "/n/low.nam", "h-low");
        let again = import_drive_capture(&mut def, &mut dps, "Klon", "Low again", "/n/low.nam", "h-low");
        assert_eq!(again, DriveImport::AlreadyPresent);
        assert_eq!(dps[0].options.len(), 1);
    }

    #[test]
    fn a_full_board_still_keeps_the_capture() {
        let mut def = empty_profile();
        let mut dps: Vec<DrivePresetDef> = Vec::new();
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let r = import_drive_capture(&mut def, &mut dps, name, "Stock", &format!("/n/{i}.nam"), &format!("h-{i}"));
            assert!(matches!(r, DriveImport::Slot { .. }), "{name} took a slot");
        }
        // Every slot is taken, so the fourth pedal lands in the library
        // without one rather than being dropped.
        let overflow = import_drive_capture(&mut def, &mut dps, "D", "Stock", "/n/3.nam", "h-3");
        assert_eq!(
            overflow,
            DriveImport::NoFreeSlot {
                preset: "D".to_string()
            }
        );
        assert_eq!(dps.len(), 4);
        assert_eq!(def.drives.len(), 3);
    }
}
