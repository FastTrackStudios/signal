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
    /// A cabinet impulse response (`.wav`) to convolve right after this
    /// amp's NAM, for a capture that is amp-only (`gear_type: amp`, no
    /// cab/mic baked in). Empty for a capture that already IS a full rig
    /// (`amp_cab`) — the chain always has a Cabinet block after the amp,
    /// but an empty `cab` makes it a no-op passthrough (`RigBlock` with no
    /// realization has no backend and is skipped at install), so pointing
    /// two different preset kinds at the same chain shape is free.
    #[facet(default)]
    pub cab: String,
    /// SHA-256 of the IR. See [`hash`](Self::hash).
    #[facet(default)]
    pub cab_hash: String,
    /// The amp's Output Level (dB), from its module snapshot (see
    /// `compose::ModuleSnapshotDef::level_db`).
    #[facet(default)]
    pub level_db: f32,
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
    /// The pedal's Output Level (dB) at its unity point — set by levelling
    /// (`signal rig level-modules`) so that engaging it at drive 0.5 does not
    /// change the loudness. The drive block's own output gain.
    #[facet(default)]
    pub level_db: f32,
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

/// The boost slot at the head of the board — a slot like the drives (a
/// profile or module snapshot assigns it a captured pedal), but an import
/// never claims it: a new capture is a drive until someone makes it the
/// boost.
pub const BOOST_SLOT: &str = "Boost";

/// Every slot of the drive board, in board order: the boost, then the
/// drives.
pub const BOARD_SLOTS: [&str; 4] = [BOOST_SLOT, "Drive 1", "Drive 2", "Drive 3"];

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
        level_db: 0.0,
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
        // The boost slot's pedal: the King of Tone's red side set as a
        // clean boost, as a pedal of its own.
        DrivePresetDef {
            name: "Clean Boost".to_string(),
            options: vec![opt(
                "King of Tone Red",
                "/home/cody/Downloads/King of Tone/red-boost/King of Tone ver4 Red channel set to Boost.nam",
            )],
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
    /// The song this patch belongs to — empty for the profile's own. A
    /// song's patches live in `songs.styx` with the song, not in the
    /// profile: they join the profile in memory (so their chains are
    /// preloaded and switching stays gapless), show only while their song
    /// is up, and are written back to the song when the profile is saved.
    #[facet(default)]
    pub song: String,
    pub preset: String,
    /// A second amp (Amp R), in parallel with the first: Amp L → Cab L and
    /// Amp R → Cab R both hear the guitar and are blended at the end (see
    /// `signal_sampler::amp_blend`). Independently bypassable. Empty
    /// = the slot is unloaded; the chain still has the block (so bypass
    /// grouping and the board layout stay constant) but it has no
    /// realization, so it is skipped at install and costs nothing.
    #[facet(default)]
    pub preset2: String,
    /// The preset this patch plays, and which of its snapshots — a
    /// composition of module snapshots (see [`crate::compose`]). Empty keeps
    /// the old shape: `preset`/`preset2`/`overrides` say it all directly.
    #[facet(default)]
    pub rig_preset: String,
    #[facet(default)]
    pub snapshot: String,
    /// This patch's own module picks, replacing the preset snapshot's for
    /// those modules — what choosing on the board saves.
    #[facet(default)]
    pub modules: Vec<ModuleChoiceDef>,
    /// This patch's own block presets — a single delay, a reverb — over
    /// the ones its preset snapshot and module snapshots pick: what picking
    /// a block preset on the board saves. Applied after them and before the
    /// patch's `overrides`, so a knob moved by hand still wins.
    #[facet(default)]
    pub blocks: Vec<crate::compose::BlockChoiceDef>,
    /// Drive-slot assignments for this patch alone, over the profile's
    /// `drives` slot by slot. Filled by a Drive module snapshot.
    #[facet(default)]
    pub drives: Vec<DriveSlotDef>,
    /// The player's own level for this patch, dB, relative to every other
    /// patch after normalisation.
    ///
    /// This is the knob that says "the lead is 3 dB up". It is never written
    /// by the levelling pass — normalisation puts every patch at the same
    /// loudness and this is what you want ON TOP of that, so a pass that
    /// clobbered it would erase the one thing the player set by ear.
    pub trim_db: f32,
    /// The loudness calibration, dB — what the levelling pass measured this
    /// patch needs to sit at the target. Written by the pass, not by hand.
    #[facet(default)]
    pub level_db: f32,
    /// Boost level recalled with the patch (0 = boost off).
    pub boost_db: f32,
    pub overrides: Vec<OverrideDef>,
    /// Where the macro bar's knobs sit on this patch — offsets from the
    /// patch as dialled, so a patch with none plays exactly as its values
    /// say (see `crate::macros`). Kept apart from `overrides` on purpose:
    /// a macro turned and turned back must leave the patch where it was.
    #[facet(default)]
    pub macros: Vec<MacroValueDef>,
}

/// One macro knob's position on a patch.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct MacroValueDef {
    /// The knob (`drive`, `delay-time1`, `width`).
    pub id: String,
    /// Offset from rest, −1..1: 0 changes nothing, 1 is the knob all the
    /// way up, −1 all the way down. For an absolute choice (none are
    /// stored today) it would be the position itself.
    pub value: f32,
    /// A drive stage's ON/OFF pad, pressed since its knob last moved:
    /// `on`, `off`, or empty (the knob decides).
    #[facet(default)]
    pub pad: String,
}

/// One module's pick: which module preset, and which of its snapshots.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ModuleChoiceDef {
    /// `Amp`, `Drive`, `Time`, `Modulation`, `Dynamics`.
    pub module: String,
    pub preset: String,
    #[facet(default)]
    pub snapshot: String,
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

/// The chain's delays and reverbs. Each runs in parallel with the dry — the
/// dry at the block's `dry`, the effect added on top — so each runs fully
/// wet (`mix` pinned at 1 in the chain) and how loud the effect sits is its
/// `level`, in dB.
pub const PARALLEL_FX: [&str; 6] = ["Pre Verb", "Pre Delay", "DLY 1", "DLY 2", "VERB 1", "VERB 2"];

#[must_use]
pub fn is_parallel_fx(block: &str) -> bool {
    PARALLEL_FX.iter().any(|b| b.eq_ignore_ascii_case(block))
}

/// A parallel effect's wet gain as a `level`: `20·log10(mix)`, floored at
/// the level's −60 dB (off).
#[must_use]
pub fn mix_to_level_db(mix: f32) -> f32 {
    if mix <= 0.001 { -60.0 } else { (20.0 * mix.log10()).max(-60.0) }
}

impl OverrideDef {
    /// A `mix` written to a delay or reverb, as the `level` that plays the
    /// same: before the effects ran fully wet, their amount was `mix`, and
    /// stored presets, patches and sections still say so.
    pub fn pin_parallel_mix(&mut self) {
        if self.op == "set" && self.param.eq_ignore_ascii_case("mix") && is_parallel_fx(&self.block) {
            self.param = "level".into();
            self.value = mix_to_level_db(self.value);
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
    /// The patch the profile lands on when it loads — its default scene.
    /// Empty means the first patch.
    ///
    /// What lets every profile keep the same slot convention (1 clean,
    /// 2 crunchier, 3 drive, 4 lead) and still start where it is played: a
    /// metal profile lands on the chug in slot 3 without moving the chug to
    /// slot 1.
    #[facet(default)]
    pub default_patch: String,
}

/// A footswitch stack: a name and its patch rotation.
#[derive(Clone, Debug, Facet)]
pub struct StackDef {
    pub name: String,
    pub patches: Vec<String>,
    /// How its switch behaves when no song says otherwise (see
    /// [`SwitchMode`]).
    #[facet(default)]
    pub momentary: bool,
    #[facet(default)]
    pub no_rotate: bool,
}

/// How a footswitch behaves: what a stack or a song's entry for it says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwitchMode {
    /// Active only while held: the rig goes back to where it was on release.
    pub momentary: bool,
    /// Always its landing patch — pressing it again does not rotate.
    pub no_rotate: bool,
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
        cab: String::new(),
        cab_hash: String::new(),
        level_db: 0.0,
    };
    let stack = |name: &str, patches: &[&str]| StackDef {
        name: name.to_string(),
        patches: patches
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        momentary: false,
        no_rotate: false,
    };
    let patch = |name: &str, preset: &str| PatchDef {
        song: String::new(),
        name: name.to_string(),
        preset: preset.to_string(),
        preset2: String::new(),
        rig_preset: String::new(),
        snapshot: String::new(),
        modules: Vec::new(),
        blocks: Vec::new(),
        drives: Vec::new(),
        trim_db: 0.0,
        level_db: 0.0,
        boost_db: 0.0,
        overrides: Vec::new(),
        macros: Vec::new(),
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
        default_patch: String::new(),
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
                vec![time_param("VERB 1", "level", -13.0)],
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
                    time_param("DLY 1", "level", -7.0),
                    time_param("DLY 1", "feedback", 0.4),
                ],
            ),
            // Drive
            patch("Drive", "AA Drive"),
            with_ovr(
                patch("Drive Edge", "AA Drive"),
                vec![time_param("DLY 1", "level", -18.5)],
            ),
            // Lead
            {
                let mut p = patch("Lead", "Arena Lead");
                p.boost_db = 3.0;
                p
            },
            with_ovr(
                patch("Lead POG", "Arena Lead"),
                vec![time_param("VERB 1", "level", -18.5)],
            ),
            // Ambient
            with_ovr(
                patch("Ambient", "AC30 Clean"),
                // Cloud verb, long and dark; delay a touch hotter.
                vec![
                    time_param("VERB 1", "algorithm", 4.0),
                    time_param("VERB 1", "level", -9.0),
                    time_param("VERB 1", "decay", 0.63),
                    time_param("VERB 1", "tone", -0.5),
                    time_param("DLY 1", "level", -12.0),
                ],
            ),
            with_ovr(
                patch("Ambient Swells", "AC30 Clean"),
                vec![
                    time_param("VERB 1", "level", -15.0),
                    time_param("VERB 1", "decay", 0.8),
                ],
            ),
            with_ovr(
                patch("Ambient Delay Craze", "AC30 Clean"),
                vec![
                    OverrideDef::bypass("Time", "DLY 2", false),
                    time_param("DLY 2", "level", -16.0),
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
fn drive_block(drives: &[DriveSlotDef], dps: &[DrivePresetDef], block: &str) -> RigBlock {
    let assigned = drives
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

/// The library's boost pedal — the first drive preset named for boosting
/// ("Clean Boost") — as `(preset, option)`: what the boost slot plays when
/// nothing is assigned to it. A pedal of its own, not an option of a drive
/// on the board: one pedal is one node, and the King of Tone cannot be both
/// the boost and Drive 1.
#[must_use]
pub fn default_boost(dps: &[DrivePresetDef]) -> Option<(String, usize)> {
    dps.iter()
        .find(|p| p.name.to_ascii_lowercase().contains("boost") && !p.options.is_empty())
        .map(|p| (p.name.clone(), 0))
}

/// What a board slot is playing, as the UI names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotPedal {
    /// The pedal (drive preset), e.g. "King of Tone"; "Clean Boost
    /// (built-in)" for the native boost; empty for an empty slot.
    pub pedal: String,
    /// Which of its captures, e.g. "Both Sides".
    pub option: String,
    /// The capture's file, for a tooltip.
    pub nam: String,
    /// Nothing assigned and no fallback: the slot does nothing.
    pub empty: bool,
}

/// What board slot `slot` plays for a patch whose effective drive slots are
/// `drives` — the pedal assigned to it (profile, module snapshot or the
/// patch's own), the library's boost pedal for an unassigned boost slot, the
/// native clean boost when the library has none, or nothing.
#[must_use]
pub fn slot_pedal(slot: &str, drives: &[DriveSlotDef], dps: &[DrivePresetDef]) -> SlotPedal {
    let assignment = drives
        .iter()
        .find(|d| d.block.eq_ignore_ascii_case(slot))
        .map(|d| (d.preset.clone(), d.option))
        .or_else(|| slot.eq_ignore_ascii_case(BOOST_SLOT).then(|| default_boost(dps)).flatten());
    let found = assignment.and_then(|(preset, i)| {
        let p = dps.iter().find(|p| p.name.eq_ignore_ascii_case(&preset))?;
        let o = p.options.get(i).or_else(|| p.options.first())?;
        Some(SlotPedal { pedal: p.name.clone(), option: o.name.clone(), nam: o.nam.clone(), empty: false })
    });
    match found {
        Some(s) => s,
        None if slot.eq_ignore_ascii_case(BOOST_SLOT) => SlotPedal {
            pedal: "Clean Boost (built-in)".to_string(),
            ..SlotPedal::default()
        },
        None => SlotPedal { empty: true, ..SlotPedal::default() },
    }
}

/// Build the boost slot: the captured pedal assigned to it; unassigned, the
/// library's boost capture (a drive preset option named for boosting, e.g.
/// King of Tone "Red = Boost"); with none in the library, the native clean
/// boost. Off by default, like every board slot.
fn boost_block(drives: &[DriveSlotDef], dps: &[DrivePresetDef]) -> RigBlock {
    let assigned = drives
        .iter()
        .find(|d| d.block.eq_ignore_ascii_case(BOOST_SLOT))
        .and_then(|d| {
            dps.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&d.preset))
                .and_then(|p| p.options.get(d.option).cloned())
        })
        .or_else(|| {
            let (preset, i) = default_boost(dps)?;
            dps.iter()
                .find(|p| p.name == preset)
                .and_then(|p| p.options.get(i).cloned())
        });
    // A captured boost is a pedal like the drives (a NAM Drive block, as
    // the library's pedal nodes resolve); only the native fallback is a
    // Boost block.
    let mut b = match assigned {
        Some(opt) => RigBlock::of_type(BlockType::Drive)
            .with_nam(opt.nam)
            .with_param("drive", "0.5"),
        None => RigBlock::of_type(BlockType::Boost).with_param("drive", "0.5"),
    };
    b = b.named(BOOST_SLOT);
    b.bypassed = true;
    b
}

/// Append the drive board to a patch under construction: the boost slot,
/// then every slot in [`DRIVE_SLOTS`] in board order. All off until the
/// control surface engages them, and a slot the profile has not assigned
/// builds as a transparent placeholder so every slot stays addressable
/// either way.
fn drive_board(drives: &[DriveSlotDef], dps: &[DrivePresetDef], patch: RigPatch) -> RigPatch {
    let boosted = patch.with_block(boost_block(drives, dps));
    DRIVE_SLOTS.iter().fold(boosted, |p, slot| {
        p.with_block(drive_block(drives, dps, slot))
    })
}

#[must_use]
pub fn build_profile(def: &ProfileDef, dps: &[DrivePresetDef]) -> RigProfile {
    // One amp + its cab — the same shape for "Amp L" and "Amp R"; with both
    // loaded the engine runs the two stages in parallel. A `.nam` that is
    // already a full rig (`amp_cab`) leaves `cab` empty, and an
    // empty-realization Cabinet block has no backend and is skipped at
    // install (pure passthrough) — so the second slot costs nothing while
    // unloaded, and an amp-only capture (`gear_type: amp`) pointing `cab`
    // at an IR is what makes it usable at all.
    let amp_stage = |suffix: &str, path: String, cab: String, bypassed: bool| {
        let mut amp_block = RigBlock::nam(path).named(format!("Amp {suffix}"));
        amp_block.bypassed = bypassed;
        let mut cab_block = if cab.is_empty() {
            RigBlock::effect(BlockType::Cabinet, format!("Cab {suffix}"))
        } else {
            RigBlock::cab_ir(cab).named(format!("Cab {suffix}"))
        };
        cab_block.bypassed = bypassed;
        [amp_block, cab_block]
    };
    let amp = |name: &str,
               drives: &[DriveSlotDef],
               path: String,
               cab: String,
               path2: String,
               cab2: String| {
        // The chain, in order — each block tagged with its module, because a
        // Reverb before the amp and one after it are different things:
        //   Pre Comp · Pitch · Volume · Boost + Drives · Pre FX · Amp ·
        //   Trim · Motion · Modulation · Time · Master
        let in_module = |mut b: RigBlock, module: &str| {
            b.module = module.to_string();
            b
        };
        let head = RigPatch::new(name)
            // Pedal-style squeeze before everything (off until a preset
            // engages it): slow-ish attack lets the pick through.
            // (No explicit module for these two single blocks: a module named
            // like its only block makes an override addressed to the block
            // find the module instead.)
            .with_block(off_fx(
                BlockType::Compressor,
                PRE_COMP,
                &[
                    ("threshold", "-30"),
                    ("ratio", "4"),
                    ("attack", "20"),
                    ("release", "200"),
                ],
            ))
            .with_block(off(BlockType::Pitch, "Pitch"))
            // Volume pedal (clean gain, unity default) — the Control view's
            // left pedal drives it.
            .with_block(on_fx(
                BlockType::Volume,
                "Volume Pedal",
                &[("gain_db", "0")],
            ));
        let [amp_l, cab_l] = amp_stage("L", path, cab, false);
        let has_amp_r = !path2.is_empty();
        let [amp_r, cab_r] = amp_stage("R", path2, cab2, !has_amp_r);
        drive_board(drives, dps, head)
            // Pre FX: what sits in front of the amp — a motion block, and a
            // reverb (a spring, like the tank in a Fender) and delay the amp
            // then colours. All off until a preset engages them.
            .with_block(in_module(off(BlockType::Trem, "Pre Motion"), PRE_FX))
            .with_block(in_module(
                off_fx(
                    BlockType::Reverb,
                    "Pre Verb",
                    &[("algorithm", "3"), ("mix", "1"), ("level", "-16.5"), ("decay", "0.35")],
                ),
                PRE_FX,
            ))
            .with_block(in_module(
                off_fx(
                    BlockType::Delay,
                    "Pre Delay",
                    &[
                        ("style", "0"),
                        ("tap_div_l", "7"),
                        ("tap_div_r", "7"),
                        ("time", "120"),
                        ("mix", "1"),
                        ("level", "-16.5"),
                        ("feedback", "0.15"),
                    ],
                ),
                PRE_FX,
            ))
            // The Amp module: the amp stage, then what shapes it — gate,
            // studio-style glue compression, and the amp EQ.
            .with_block(in_module(amp_l, "Amp"))
            .with_block(in_module(cab_l, "Amp"))
            .with_block(in_module(amp_r, "Amp"))
            .with_block(in_module(cab_r, "Amp"))
            .with_block(in_module(
                on_fx(BlockType::Gate, "Gate", &[("threshold", "-50")]),
                "Amp",
            ))
            .with_block(in_module(
                off_fx(
                    BlockType::Compressor,
                    POST_COMP,
                    &[
                        ("threshold", "-24"),
                        ("ratio", "3"),
                        ("attack", "20"),
                        ("release", "100"),
                    ],
                ),
                "Amp",
            ))
            // The amp EQ ships with the electric-guitar "magic frequencies"
            // preset (eq-ui cheatsheet zones): low cut at 80 Hz, then flat
            // named bells on body / character / honk / presence.
            .with_block(in_module(
                off_fx(
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
                ),
                "Amp",
            ))
            // Boost gain block the footswitch drives (0 dB until engaged).
            .with_block(on_fx(BlockType::Volume, "Boost", &[("gain_db", "0")]))
            // Motion, then Modulation — all off by default.
            .with_block(in_module(off(BlockType::Trem, "Tremolo"), "Motion"))
            .with_block(in_module(off(BlockType::Vibrato, "Vibrato"), "Motion"))
            .with_block(in_module(off(BlockType::Rotary, "Rotary"), "Motion"))
            .with_block(in_module(off(BlockType::Chorus, "Chorus"), "Modulation"))
            .with_block(in_module(off(BlockType::Flanger, "Flanger"), "Modulation"))
            .with_block(in_module(off(BlockType::Phaser, "Phaser"), "Modulation"))
            // The patch's own level and pan: the LAST thing before the time
            // section (see `set_patch_trim`). Everything before it hears the
            // same signal whatever the patch's level or pan — a compressor,
            // a drive, a chorus react the same — and the time effects take
            // the dry sound where it has been placed.
            .with_block(on_fx(BlockType::Volume, "Patch Trim", &[("gain_db", "0")]))
            // Time module — subtle pair on, extreme pair bypassed.
            .with_block(in_module(
                on_fx(
                    BlockType::Delay,
                    "DLY 1",
                    &[
                        ("mix", "1"),
                        ("level", "-14"),
                        ("style", "0"),
                        ("time", "350"),
                        ("feedback", "0.28"),
                        ("tap_div_l", "0"),
                        ("tap_div_r", "0"),
                    ],
                ),
                "Time",
            ))
            .with_block(in_module(
                off_fx(
                    BlockType::Delay,
                    "DLY 2",
                    &[
                        ("mix", "1"),
                        ("level", "-20"),
                        ("time", "600"),
                        ("feedback", "0.62"),
                        ("tap_div_l", "1"),
                        ("tap_div_r", "1"),
                    ],
                ),
                "Time",
            ))
            .with_block(in_module(
                on_fx(
                    BlockType::Reverb,
                    "VERB 1",
                    &[("mix", "1"), ("level", "-22"), ("decay", "0.42"), ("size", "0.45")],
                ),
                "Time",
            ))
            .with_block(in_module(
                off_fx(
                    BlockType::Reverb,
                    "VERB 2",
                    &[("mix", "1"), ("level", "-20"), ("decay", "0.85"), ("size", "0.92")],
                ),
                "Time",
            ))
            // Master: a final EQ (flat until a preset shapes it) and a
            // zero-latency brickwall — the compressor with no lookahead at
            // 20:1 and a 0.1 ms attack, catching peaks before the output.
            .with_block(in_module(off(BlockType::Eq, "Master EQ"), "Master"))
            .with_block(in_module(
                on_fx(
                    BlockType::Compressor,
                    LIMITER,
                    &[
                        ("threshold", "-1"),
                        ("ratio", "20"),
                        ("attack", "0.1"),
                        ("release", "50"),
                        ("knee", "0"),
                    ],
                ),
                "Master",
            ))
    };
    let nam_of = |preset: &str| {
        def.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(preset))
            .map(|p| p.nam.clone())
            .unwrap_or_default()
    };
    let cab_of = |preset: &str| {
        def.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(preset))
            .map(|p| p.cab.clone())
            .unwrap_or_default()
    };
    let mut profile = RigProfile::new(&def.name);
    for p in &def.patches {
        let mut patch = amp(
            &p.name,
            &crate::compose::drives_for(def, p),
            nam_of(&p.preset),
            cab_of(&p.preset),
            nam_of(&p.preset2),
            cab_of(&p.preset2),
        );
        set_patch_trim(&mut patch, p.level_db + p.trim_db);
        assign_meters(&mut patch);
        apply_overrides(&mut patch, &p.overrides);
        profile = profile.with_patch(patch);
    }
    for st in &def.stacks {
        profile = profile.with_stack(RigStack::new(&st.name, st.patches.clone()));
    }
    profile
}

/// The block a patch's level lands on.
pub const TRIM_BLOCK: &str = "Patch Trim";

/// The `comp_meter` channel each compressor block draws its panel's trace
/// on — one each, so the pre compressor, the post compressor and the limiter
/// each show their own input and gain reduction rather than one shared trace
/// every compressor in the chain wrote to. 0 = no meter.
#[must_use]
pub fn meter_channel(block_name: &str) -> usize {
    if block_name.eq_ignore_ascii_case(PRE_COMP) {
        1
    } else if block_name.eq_ignore_ascii_case(POST_COMP) {
        2
    } else if block_name.eq_ignore_ascii_case(LIMITER) {
        3
    } else {
        0
    }
}

/// Stamp every compressor block of `patch` with its meter channel (see
/// [`meter_channel`]). Applied where a patch's chain is finished, next to its
/// level, so no path that builds a chain can miss it.
pub fn assign_meters(patch: &mut RigPatch) {
    for block in &mut patch.chain {
        if block.block_type == BlockType::Compressor {
            let ch = meter_channel(&block.name).to_string();
            match block.params.iter_mut().find(|p| p.name == "meter") {
                Some(p) => p.value = ch,
                None => block.params.push(signal_sampler::rig_node::Param {
                    name: "meter".to_string(),
                    value: ch,
                }),
            }
        }
    }
}
/// The pedal-style compressor at the head of the chain.
pub const PRE_COMP: &str = "Pre Comp";
/// The studio-style compressor after the amp, in the Amp module.
pub const POST_COMP: &str = "Post Comp";
/// The zero-latency brickwall at the end, in the Master module.
pub const LIMITER: &str = "Limiter";
/// The module holding what sits in front of the amp.
pub const PRE_FX: &str = "Pre FX";

/// Put a patch's level on its trim block, INSIDE the chain and upstream of
/// the time effects.
///
/// Not on the scene's output. A patch's level used to be `output_trim_db`,
/// which the graph applies after everything in the scene — including the
/// delays and the reverbs. Switching patches then rescaled whatever was
/// still ringing: a tail that was decaying at one level jumped to another
/// mid-decay, which is audible and is exactly what you do not want at the
/// moment you change sound.
///
/// Upstream of the time section, a change only reaches signal that has not
/// been fed to them yet. What is already in the delay and reverb buffers
/// decays at the level it went in at, and the new patch arrives at its own
/// level behind it — which is what a real rig does and what the ear expects.
pub fn set_patch_trim(patch: &mut RigPatch, db: f32) {
    let Some(block) = patch
        .chain
        .iter_mut()
        .find(|b| b.name.eq_ignore_ascii_case(TRIM_BLOCK))
    else {
        // A profile built before the trim block existed, or one the player
        // has edited the block out of. The patch still plays; it just plays
        // uncalibrated, which is better than refusing to build it.
        tracing::debug!(patch = %patch.name, "no trim block — patch level not applied");
        return;
    };
    match block.params.iter_mut().find(|p| p.name == "gain_db") {
        Some(p) => p.value = db.to_string(),
        None => block.params.push(signal_sampler::rig_node::Param {
            name: "gain_db".to_string(),
            value: db.to_string(),
        }),
    }
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
    /// What each section recalls: selecting the part switches to this patch.
    ///
    /// Additive rather than folded into [`parts`](Self::parts), and
    /// deliberately: `parts` is a list of bare strings in `songs.styx`, and
    /// changing its shape would make every existing file fail to parse —
    /// which `read_or_seed` answers by silently reseeding the shipped
    /// defaults. A format change that can quietly replace a person's set
    /// list is not worth the tidier struct. Same idiom as
    /// [`stack_defaults`](Self::stack_defaults), one level down.
    ///
    /// Keyed by section **name**, not index: a section list gets reordered
    /// and renamed while a song is being worked out, and a name survives the
    /// first of those.
    #[facet(default)]
    pub part_recalls: Vec<PartRecallDef>,
    /// The profile the song is played on; empty keeps whatever is loaded.
    /// Its parts overlay this one unless a part names its own.
    #[facet(default)]
    pub profile: String,
    /// The part the song starts on — an intro lead, say. Empty lands on the
    /// profile's default patch.
    #[facet(default)]
    pub start_part: String,
    /// The patch the song opens on, when it has no start part (e.g.
    /// "Drive Dotted"). Empty lands on the profile's default patch, through
    /// the song's tuning of that stack.
    #[facet(default)]
    pub start_patch: String,
    /// Patches that belong to this song alone (a sound the song needs and
    /// the profile should not carry) — see [`PatchDef::song`].
    #[facet(default)]
    pub patches: Vec<PatchDef>,
    /// What the footswitches do for this song when it is not their usual
    /// job (see [`SwitchActionDef`]) — switch 5 stepping through the parts
    /// instead of tapping tempo, say.
    #[facet(default)]
    pub switch_actions: Vec<SwitchActionDef>,
    /// Changes made to the profile's patches while this song is up: dialled
    /// in the song, they stay the song's (the profile keeps its defaults)
    /// until saved back to the profile.
    #[facet(default)]
    pub patch_overrides: Vec<SongPatchOverridesDef>,
    /// The song's own versions of the profile's patches: the first change
    /// made to a profile patch while this song is up (a knob, a bypass, a
    /// module or preset pick, a macro position) copies the patch here, and
    /// the song plays its copy — until it is saved back to the profile or
    /// discarded.
    #[facet(default)]
    pub patch_versions: Vec<SongPatchVersionDef>,
}

/// A song's version of one profile patch.
#[derive(Clone, Debug, Facet)]
pub struct SongPatchVersionDef {
    /// The profile the patch belongs to.
    pub profile: String,
    pub patch: PatchDef,
}

/// One profile patch's changes within a song.
#[derive(Clone, Debug, Default, Facet)]
pub struct SongPatchOverridesDef {
    /// The profile patch, by name.
    pub patch: String,
    pub overrides: Vec<OverrideDef>,
}

impl SongDef {
    /// Record `ov` on `patch` for this song (replacing the same block /
    /// param / op).
    pub fn set_patch_override(&mut self, patch: &str, ov: OverrideDef) {
        let entry = match self.patch_overrides.iter().position(|e| e.patch.eq_ignore_ascii_case(patch)) {
            Some(i) => &mut self.patch_overrides[i],
            None => {
                self.patch_overrides.push(SongPatchOverridesDef { patch: patch.to_string(), overrides: Vec::new() });
                self.patch_overrides.last_mut().expect("just pushed")
            }
        };
        match entry.overrides.iter_mut().find(|o| {
            o.block.eq_ignore_ascii_case(&ov.block) && o.op == ov.op && o.param == ov.param
        }) {
            Some(o) => o.value = ov.value,
            None => entry.overrides.push(ov),
        }
    }

    /// The song's changes to `patch` (empty when it has none).
    #[must_use]
    pub fn patch_overrides_for(&self, patch: &str) -> Vec<OverrideDef> {
        self.patch_overrides
            .iter()
            .find(|e| e.patch.eq_ignore_ascii_case(patch))
            .map(|e| e.overrides.clone())
            .unwrap_or_default()
    }

    /// The song's version of profile `profile`'s patch `patch`.
    #[must_use]
    pub fn version_of(&self, profile: &str, patch: &str) -> Option<&PatchDef> {
        self.patch_versions
            .iter()
            .find(|v| v.profile.eq_ignore_ascii_case(profile) && v.patch.name.eq_ignore_ascii_case(patch))
            .map(|v| &v.patch)
    }

    /// The song's version of `base` (a profile patch of `profile`), made
    /// from it on first use — where a change in the song goes.
    pub fn version_mut(&mut self, profile: &str, base: &PatchDef) -> &mut PatchDef {
        let at = self.patch_versions.iter().position(|v| {
            v.profile.eq_ignore_ascii_case(profile) && v.patch.name.eq_ignore_ascii_case(&base.name)
        });
        let i = match at {
            Some(i) => i,
            None => {
                self.patch_versions.push(SongPatchVersionDef {
                    profile: profile.to_string(),
                    patch: base.clone(),
                });
                self.patch_versions.len() - 1
            }
        };
        &mut self.patch_versions[i].patch
    }

    /// Take the song's version of `patch` out of it.
    pub fn take_version(&mut self, profile: &str, patch: &str) -> Option<PatchDef> {
        let i = self.patch_versions.iter().position(|v| {
            v.profile.eq_ignore_ascii_case(profile) && v.patch.name.eq_ignore_ascii_case(patch)
        })?;
        Some(self.patch_versions.remove(i).patch)
    }

    /// `def` as this song plays it: its versions of the profile's patches in
    /// place of theirs, and its per-setting changes on top.
    #[must_use]
    pub fn apply_to(&self, def: &ProfileDef) -> ProfileDef {
        let mut out = def.clone();
        for p in out.patches.iter_mut().filter(|p| p.song.is_empty()) {
            if let Some(v) = self.version_of(&def.name, &p.name) {
                *p = v.clone();
            }
            for ov in self.patch_overrides_for(&p.name) {
                match p.overrides.iter_mut().find(|o| {
                    o.block.eq_ignore_ascii_case(&ov.block) && o.op == ov.op && o.param == ov.param
                }) {
                    Some(o) => o.value = ov.value,
                    None => p.overrides.push(ov),
                }
            }
        }
        out
    }

    /// Take the song's changes to `patch` out of it.
    pub fn take_patch_overrides(&mut self, patch: &str) -> Vec<OverrideDef> {
        match self.patch_overrides.iter().position(|e| e.patch.eq_ignore_ascii_case(patch)) {
            Some(i) => self.patch_overrides.remove(i).overrides,
            None => Vec::new(),
        }
    }
}

impl SongDef {
    /// The recall entry for part `name` — its source's, when it repeats
    /// another part (see [`PartRecallDef::repeat_of`]).
    #[must_use]
    pub fn part_recall(&self, name: &str) -> Option<&PartRecallDef> {
        let src = self.source_part(name);
        self.own_recall(&src)
    }

    /// Part `name`'s own entry (never its source's).
    #[must_use]
    pub fn own_recall(&self, name: &str) -> Option<&PartRecallDef> {
        self.part_recalls
            .iter()
            .find(|r| r.part.eq_ignore_ascii_case(name))
    }

    /// The part whose recall `name` plays: itself, or — following its
    /// repeats — the part it repeats (cycles and missing parts stop at the
    /// last good one).
    #[must_use]
    pub fn source_part(&self, name: &str) -> String {
        let mut cur = name.to_string();
        for _ in 0..8 {
            let next = self
                .own_recall(&cur)
                .map(|r| r.repeat_of.trim().to_string())
                .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(&cur) && self.parts.iter().any(|p| p.eq_ignore_ascii_case(n)));
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        cur
    }

    /// The part at `idx`'s section: its own `section`, else its name — a
    /// part nobody grouped is a section of one.
    #[must_use]
    pub fn section_of(&self, idx: usize) -> String {
        let Some(name) = self.parts.get(idx) else {
            return String::new();
        };
        self.own_recall(name)
            .map(|r| r.section.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| name.clone())
    }

    /// Where each section starts: the index of its first part. Sections are
    /// runs of consecutive parts with the same section name, so the same
    /// section name later in the song ("Chorus" again) is a new section.
    #[must_use]
    pub fn section_starts(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut last: Option<String> = None;
        for i in 0..self.parts.len() {
            let sec = self.section_of(i);
            if last.as_ref().is_none_or(|l| !l.eq_ignore_ascii_case(&sec)) {
                starts.push(i);
            }
            last = Some(sec);
        }
        starts
    }

    /// Each section paired with the patch it recalls, in section order.
    ///
    /// The empty string for a section nothing has been assigned to — which is
    /// every section until someone does, and is why this is a pair rather
    /// than an `Option`: a UI renders "recalls nothing" the same way it
    /// renders a name, and the wire has no room for absence.
    ///
    /// Matched by name, case-insensitively, because a section list gets
    /// renamed and reordered while a song is being worked out.
    #[must_use]
    pub fn parts_with_recalls(&self) -> Vec<(String, String)> {
        self.parts
            .iter()
            .map(|name| {
                let patch = self
                    .part_recall(name)
                    .map(|r| r.patch.clone())
                    .unwrap_or_default();
                (name.clone(), patch)
            })
            .collect()
    }

    /// Name a new section, appended. `false` if the name is taken or empty.
    ///
    /// Names must be unique because recalls are keyed by name: two sections
    /// called "Chorus" would share whatever either was given.
    pub fn add_part(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.parts.iter().any(|p| p.eq_ignore_ascii_case(name)) {
            return false;
        }
        self.parts.push(name.to_string());
        true
    }

    /// Rename a section, carrying what it recalls and changes.
    ///
    /// The carry is the whole subtlety. Recalls are keyed by NAME, so a
    /// rename that left them behind would silently empty the section — which
    /// reads as "the rename worked and it was always blank", and the player
    /// finds out mid-song.
    pub fn rename_part(&mut self, old: &str, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty()
            || old.eq_ignore_ascii_case(new_name)
            || self.parts.iter().any(|p| p.eq_ignore_ascii_case(new_name))
        {
            return false;
        }
        let Some(slot) = self.parts.iter_mut().find(|p| p.eq_ignore_ascii_case(old)) else {
            return false;
        };
        *slot = new_name.to_string();
        for r in &mut self.part_recalls {
            if r.part.eq_ignore_ascii_case(old) {
                r.part = new_name.to_string();
            }
            // Its repeats follow it.
            if r.repeat_of.eq_ignore_ascii_case(old) {
                r.repeat_of = new_name.to_string();
            }
        }
        true
    }

    /// Remove a section and whatever it recalled.
    pub fn remove_part(&mut self, name: &str) -> bool {
        // Its repeats keep its sound: each takes a copy of what it recalled.
        if let Some(src) = self.own_recall(name).cloned() {
            for r in self.part_recalls.iter_mut().filter(|r| r.repeat_of.eq_ignore_ascii_case(name)) {
                let (part, section) = (r.part.clone(), r.section.clone());
                *r = PartRecallDef { part, section, repeat_of: src.repeat_of.clone(), ..src.clone() };
            }
        }
        let before = self.parts.len();
        self.parts.retain(|p| !p.eq_ignore_ascii_case(name));
        self.part_recalls
            .retain(|r| !r.part.eq_ignore_ascii_case(name));
        self.parts.len() != before
    }

    /// Move a section, for arranging a song.
    ///
    /// `part_recalls` is keyed by name and order-independent, so nothing
    /// there needs touching — which is the reason it is keyed by name.
    pub fn move_part(&mut self, from: usize, to: usize) -> bool {
        if from >= self.parts.len() || to >= self.parts.len() || from == to {
            return false;
        }
        let name = self.parts.remove(from);
        self.parts.insert(to, name);
        true
    }

    /// Every section with the patch it recalls AND what it changes on top.
    ///
    /// The recall alone stopped being the whole story when sections gained
    /// overrides: a section with no patch and three overrides is doing more
    /// than a section with a patch and none, and
    /// [`parts_with_recalls`](Self::parts_with_recalls) shows it as empty.
    #[must_use]
    pub fn parts_with_changes(&self) -> Vec<(String, String, Vec<OverrideDef>)> {
        self.parts
            .iter()
            .map(|name| {
                let recall = self.part_recall(name);
                (
                    name.clone(),
                    recall.map(|r| r.patch.clone()).unwrap_or_default(),
                    recall.map(|r| r.overrides.clone()).unwrap_or_default(),
                )
            })
            .collect()
    }
}

/// One section's recall: the patch selecting it switches to.
#[derive(Clone, Debug, Default, Facet)]
pub struct PartRecallDef {
    /// The section's name, as it appears in [`SongDef::parts`].
    pub part: String,
    /// The profile this part is played on, when it is not the song's —
    /// a part is a base profile, then a patch in it, then changes on top.
    /// Empty means the song's profile.
    #[facet(default)]
    pub profile: String,
    /// The patch to switch to. Empty means the section stays on whatever
    /// patch is up — which, with [`overrides`](Self::overrides), is the
    /// common case: a chorus is usually the verse's sound with one or two
    /// things changed, not a different rig.
    pub patch: String,
    /// What this section changes on top of the patch.
    ///
    /// The point of a section. Recalling a whole patch is the blunt version
    /// and it forces a separate patch for every variation — a Verb-heavy
    /// chorus of an otherwise identical sound becomes a second patch to
    /// build, level and maintain. An override says the one thing that is
    /// different and leaves the rest of the profile alone.
    ///
    /// Applied AFTER the patch is re-established, so a section is the same
    /// sound every time it comes round regardless of which section preceded
    /// it. Without that, overrides would accumulate: a chorus that lifted
    /// the delay would leave it lifted in the verse that followed.
    #[facet(default)]
    pub overrides: Vec<OverrideDef>,
    /// The section this part belongs to (Verse 1, Bridge…): consecutive
    /// parts with the same section make one section. Empty = the part is a
    /// section of its own.
    #[facet(default)]
    pub section: String,
    /// The switches as this part tunes them, laid over the song's
    /// [`stack_defaults`](SongDef::stack_defaults) — an entry for a stack
    /// replaces the song's entry for it while the part is up.
    #[facet(default)]
    pub stack_defaults: Vec<StackDefaultDef>,
    /// The part plays the profile's own switches: the song's switch tuning
    /// steps aside (the part's own entries still apply).
    #[facet(default)]
    pub profile_switches: bool,
    /// The part's switch actions, over the song's.
    #[facet(default)]
    pub switch_actions: Vec<SwitchActionDef>,
    /// A repeat of another part (by name): this part plays — and edits —
    /// that part's recall (patch, changes, switch tuning, profile), so the
    /// two stay the same sound. Its own name and section stay its own.
    #[facet(default)]
    pub repeat_of: String,
}

/// What a footswitch does in place of its usual job, for a song or a part.
#[derive(Clone, Debug, Facet)]
pub struct SwitchActionDef {
    /// The footswitch, 1-based (1–5).
    pub switch: u32,
    /// One of [`SWITCH_ACTIONS`]' keys; empty = the switch's usual job.
    pub action: String,
}

/// The actions a footswitch can be given: `(key, label)`. `stack` is the
/// usual job of switches 1–4; `tap_tempo` switch 5's outside a song.
/// Stepping actions go forward on a tap and back on a hold.
pub const SWITCH_ACTIONS: &[(&str, &str)] = &[
    ("stack", "Its stack"),
    ("tap_tempo", "Tap tempo"),
    ("parts", "Next part (hold: previous)"),
    ("sections", "Next section (hold: previous)"),
    ("songs", "Next song (hold: previous)"),
    ("tuner", "Tuner"),
    ("boost", "Boost"),
    ("fx", "FX toggle"),
    ("none", "Nothing"),
];

/// One song-level stack override: which patch a stack lands on.
#[derive(Clone, Debug, Default, Facet)]
pub struct StackDefaultDef {
    pub stack: String,
    pub patch: String,
    /// The switch's rotation for this song, in place of the stack's —
    /// empty keeps the stack's own.
    #[facet(default)]
    pub patches: Vec<String>,
    /// The switch's behaviour for this song (see [`SwitchMode`]). An entry
    /// for a stack decides its mode for the song, whatever the profile says.
    #[facet(default)]
    pub momentary: bool,
    #[facet(default)]
    pub no_rotate: bool,
}

impl StackDefaultDef {
    #[must_use]
    pub const fn mode(&self) -> SwitchMode {
        SwitchMode {
            momentary: self.momentary,
            no_rotate: self.no_rotate,
        }
    }
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
            patches: Vec::new(),
            profile: String::new(),
            start_part: String::new(),
            start_patch: String::new(),
            name: name.to_string(),
            key: key.to_string(),
            bpm,
            stack: 0, // open on Clean; per-song stacks come with song editing
            parts: Vec::new(),
            stack_defaults: Vec::new(),
            part_recalls: Vec::new(),
            switch_actions: Vec::new(),
            patch_overrides: Vec::new(),
            patch_versions: Vec::new(),
        }
    }
    vec![
        song("What a God", "G", 79),
        song("No Other Name", "G", 74),
        song("Owe You Praise", "C", 122),
        song("WASHED", "E", 139),
        song("Who Else", "A", 68),
        song("Build My Life / With Everything", "A", 70),
        song("Thank God I'm Free", "E", 128),
        // Elevation Worship. Published at 68 — the half-time reading; some
        // charts list 136 for the same song counted double.
        song("Always on Time", "E", 68),
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
    /// The footswitches as notes (switch `i` = `tap_notes[i]`): Note On
    /// presses, Note Off releases, tap/hold from the timing — for pedals set
    /// to send a note per switch. Defaults to notes 1–5 so a `midi.styx`
    /// written before this field still maps a note pedal.
    #[facet(default = vec![1u32, 2, 3, 4, 5])]
    pub tap_notes: Vec<u32>,
    pub direct: Vec<DirectCcDef>,
    /// The pedal whose LEDs follow the rig (name contains this; empty =
    /// none): the switch whose stack is playing is lit, the rest are not.
    /// Without it a pedal lights its switches by its own toggling, and after
    /// a few presses — a momentary most of all — several are lit at once.
    #[facet(default = String::from("AIRSTEP"))]
    pub led_output: String,
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
        tap_notes: vec![1, 2, 3, 4, 5],
        led_output: "AIRSTEP".to_string(),
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
        level_db: 0.0,
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
            default_patch: String::new(),
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
        let second =
            import_drive_capture(&mut def, &mut dps, "Klon", "High", "/n/high.nam", "h-high");
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
        let other = import_drive_capture(
            &mut def,
            &mut dps,
            "Timmy",
            "Stock",
            "/n/timmy.nam",
            "h-timmy",
        );
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
        let again = import_drive_capture(
            &mut def,
            &mut dps,
            "Klon",
            "Low again",
            "/n/low.nam",
            "h-low",
        );
        assert_eq!(again, DriveImport::AlreadyPresent);
        assert_eq!(dps[0].options.len(), 1);
    }

    #[test]
    fn a_full_board_still_keeps_the_capture() {
        let mut def = empty_profile();
        let mut dps: Vec<DrivePresetDef> = Vec::new();
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let r = import_drive_capture(
                &mut def,
                &mut dps,
                name,
                "Stock",
                &format!("/n/{i}.nam"),
                &format!("h-{i}"),
            );
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

#[cfg(test)]
mod song_tests {
    use super::*;

    fn song() -> SongDef {
        SongDef {
            patches: Vec::new(),
            profile: String::new(),
            start_part: String::new(),
            start_patch: String::new(),
            name: "No Other Name".into(),
            key: "G".into(),
            bpm: 74,
            stack: 0,
            parts: vec![
                "Intro".into(),
                "Verse".into(),
                "Chorus".into(),
                "Bridge".into(),
            ],
            stack_defaults: Vec::new(),
            switch_actions: Vec::new(),
            patch_overrides: Vec::new(),
            patch_versions: Vec::new(),
            part_recalls: vec![
                PartRecallDef {
                    profile: String::new(),
                    part: "chorus".into(),
                    patch: "Ambient".into(),
                    overrides: Vec::new(),
                    ..Default::default()
                },
                PartRecallDef {
                    profile: String::new(),
                    part: "Bridge".into(),
                    patch: "Lead".into(),
                    overrides: Vec::new(),
                    ..Default::default()
                },
            ],
        }
    }

    /// A section recalls the patch assigned to it, and nothing when none is.
    /// Every section was a label before this, so "nothing" has to stay a
    /// first-class answer.
    #[test]
    fn sections_pair_with_what_they_recall() {
        let pairs = song().parts_with_recalls();
        assert_eq!(
            pairs,
            vec![
                ("Intro".to_string(), String::new()),
                ("Verse".to_string(), String::new()),
                ("Chorus".to_string(), "Ambient".to_string()),
                ("Bridge".to_string(), "Lead".to_string()),
            ],
            "in section order, with empty for the unassigned"
        );
    }

    /// Matched case-insensitively: the assignment above says "chorus" and the
    /// section is "Chorus". A section gets renamed while a song is being
    /// worked out, and a recall that only matches one capitalisation is a
    /// recall that stops firing without saying so.
    #[test]
    fn a_recall_matches_its_section_whatever_the_case() {
        let pairs = song().parts_with_recalls();
        assert_eq!(pairs[2].1, "Ambient");
    }

    /// A repeat plays its source's recall; a rename carries the link; the
    /// source removed, the repeat keeps a copy of the sound.
    #[test]
    fn a_repeat_is_linked_to_its_part() {
        let mut s = song();
        s.parts = ["Verse 2", "Chorus 2", "Bridge", "Dance! (V2)"].map(String::from).to_vec();
        s.part_recalls = vec![
            PartRecallDef { part: "Verse 2".into(), patch: "Dry Chorus Clean L".into(), ..Default::default() },
            PartRecallDef { part: "Dance! (V2)".into(), repeat_of: "Verse 2".into(), ..Default::default() },
        ];
        assert_eq!(s.source_part("Dance! (V2)"), "Verse 2");
        assert_eq!(s.part_recall("Dance! (V2)").unwrap().patch, "Dry Chorus Clean L");
        assert!(s.rename_part("Verse 2", "V2"));
        assert_eq!(s.source_part("Dance! (V2)"), "V2");
        assert!(s.remove_part("V2"));
        let own = s.own_recall("Dance! (V2)").unwrap();
        assert_eq!(own.patch, "Dry Chorus Clean L");
        assert!(own.repeat_of.is_empty());
    }

    /// A song's version of a profile patch replaces it in the song, and
    /// only in that profile.
    #[test]
    fn a_song_plays_its_version_of_a_patch() {
        let prof = worship_def();
        let base = prof.patches.iter().find(|p| p.name == "Lead").cloned().expect("Lead");
        let mut s = song();
        s.version_mut(&prof.name, &base).modules.push(ModuleChoiceDef {
            module: "Delay".into(),
            preset: "U2 Edge".into(),
            snapshot: "Streets".into(),
        });
        let played = s.apply_to(&prof);
        let lead = played.patches.iter().find(|p| p.name == "Lead").unwrap();
        assert!(lead.modules.iter().any(|m| m.snapshot == "Streets"));
        let other = ProfileDef { name: "Blues".into(), ..prof.clone() };
        let blues = s.apply_to(&other);
        assert!(!blues.patches.iter().find(|p| p.name == "Lead").unwrap().modules.iter().any(|m| m.snapshot == "Streets"));
        assert!(s.take_version(&prof.name, "Lead").is_some());
        assert!(s.version_of(&prof.name, "Lead").is_none());
    }

    /// A song keeps its changes to a profile patch per setting, the last
    /// value winning, and gives them up whole.
    #[test]
    fn song_changes_to_a_patch() {
        let mut s = song();
        let ov = |p: &str, v: f32| OverrideDef {
            module: "Time".into(),
            block: "DLY 1".into(),
            param: p.into(),
            op: "set".into(),
            value: v,
            text: String::new(),
        };
        s.set_patch_override("Drive", ov("feedback", 0.4));
        s.set_patch_override("Drive", ov("feedback", 0.5));
        s.set_patch_override("Drive", ov("level", -8.0));
        let got = s.patch_overrides_for("drive");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].value, 0.5);
        assert!(s.patch_overrides_for("Clean").is_empty());
        assert_eq!(s.take_patch_overrides("Drive").len(), 2);
        assert!(s.patch_overrides_for("Drive").is_empty());
    }

    /// Consecutive parts with one section name make one section.
    #[test]
    fn sections_are_runs_of_parts() {
        let mut s = song();
        s.parts = ["Verse 1", "Chorus 1", "Flute", "Build", "Chorus 2"]
            .map(String::from)
            .to_vec();
        for p in ["Flute", "Build"] {
            s.part_recalls.push(PartRecallDef {
                part: p.into(),
                section: "Bridge".into(),
                ..Default::default()
            });
        }
        assert_eq!(s.section_of(0), "Verse 1");
        assert_eq!(s.section_of(3), "Bridge");
        assert_eq!(s.section_starts(), vec![0, 1, 2, 4]);
    }

    /// A recall naming a section the song does not have is ignored rather
    /// than appearing as an extra section — the sections are the song's, and
    /// this list only says what they do.
    #[test]
    fn a_recall_for_a_missing_section_adds_nothing() {
        let mut s = song();
        s.part_recalls.push(PartRecallDef {
            profile: String::new(),
            part: "Outro".into(),
            patch: "Clean".into(),
            overrides: Vec::new(),
            ..Default::default()
        });
        assert_eq!(s.parts_with_recalls().len(), 4);
    }
}

#[cfg(test)]
mod trim_tests {
    use super::*;

    fn a_patch() -> RigPatch {
        let profile = build_profile(&worship_def(), &drive_presets());
        profile
            .patches
            .first()
            .cloned()
            .expect("the default profile has patches")
    }

    /// The trim block exists, and it is UPSTREAM of every time effect.
    ///
    /// This is the whole reason it exists. A level after the delays and
    /// reverbs rescales whatever is still ringing the moment a patch
    /// changes — the tail jumps mid-decay, which is audible and lands
    /// exactly when the player is changing sound. Upstream, a change only
    /// reaches signal that has not been fed to them yet.
    #[test]
    fn the_trim_sits_before_the_time_effects() {
        let patch = a_patch();
        let at = |name: &str| {
            patch
                .chain
                .iter()
                .position(|b| b.name.eq_ignore_ascii_case(name))
        };
        let trim = at(TRIM_BLOCK).expect("every patch has a trim block");

        for time in ["DLY 1", "DLY 2", "VERB 1", "VERB 2"] {
            let i = at(time).unwrap_or_else(|| panic!("{time} is in the chain"));
            assert!(
                trim < i,
                "the trim must come before {time} — a level applied after a \
                 reverb rescales the tail that is already ringing"
            );
        }
    }

    /// And after everything else: the motion and modulation, the boost,
    /// the amp. Its pan and level are the patch's placing of the finished
    /// dry sound, so everything upstream reacts the same whatever they are
    /// (a chorus fed a panned guitar was the WASHED verse in both ears).
    #[test]
    fn the_trim_is_the_last_block_before_the_time_module() {
        let patch = a_patch();
        let trim = patch
            .chain
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case(TRIM_BLOCK))
            .expect("trim block");
        for m in ["Chorus", "Flanger", "Phaser", "Tremolo", "Vibrato", "Rotary", "Boost", "Amp L", "Amp EQ"] {
            if let Some(i) = patch.chain.iter().position(|b| b.name.eq_ignore_ascii_case(m)) {
                assert!(i < trim, "{m} must come before the trim");
            }
        }
        let next = patch.chain.get(trim + 1).expect("the time module follows");
        assert!(next.is_time_module(), "the first block after the trim is the Time module's, got {}", next.name);
    }

    /// Calibration and the player's own level ADD. Normalisation puts every
    /// patch at the same loudness; the offset is what you want on top, and a
    /// scheme that used one or the other would lose whichever it dropped.
    #[test]
    fn calibration_and_the_players_level_add_up() {
        let mut def = worship_def();
        {
            let p = def.patches.first_mut().expect("a patch");
            p.level_db = -4.5;
            p.trim_db = 3.0;
        }
        let profile = build_profile(&def, &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        let gain: f32 = patch
            .chain
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(TRIM_BLOCK))
            .and_then(|b| b.params.iter().find(|p| p.name == "gain_db"))
            .and_then(|p| p.value.parse().ok())
            .expect("the trim block carries a gain");
        assert!((gain - (-1.5)).abs() < 1e-4, "got {gain}");
    }

    /// A patch's level no longer rides the scene's output. If it did, every
    /// word of the doc above would be false again.
    #[test]
    fn the_level_is_not_on_the_scene_output() {
        let mut def = worship_def();
        def.patches.first_mut().expect("a patch").trim_db = 6.0;
        let profile = build_profile(&def, &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        assert!(
            patch.output_trim_db.abs() < 1e-6,
            "the patch level leaked onto the scene output ({})",
            patch.output_trim_db
        );
    }
}

#[cfg(test)]
mod cab_tests {
    use super::*;

    fn block<'a>(patch: &'a RigPatch, name: &str) -> &'a RigBlock {
        patch
            .chain
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is in the chain"))
    }

    /// A patch always has a Cab block right after its amp — the chain shape
    /// never changes whether or not a cab is loaded, so switching between an
    /// amp-only and a full-rig capture never rebuilds the chain's structure.
    #[test]
    fn every_amp_has_a_cab_block_right_after_it() {
        let profile = build_profile(&worship_def(), &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        let amp_l = patch
            .chain
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case("Amp L"))
            .expect("Amp L");
        let cab_l = patch
            .chain
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case("Cab L"))
            .expect("Cab L");
        assert_eq!(cab_l, amp_l + 1, "the cab must sit right after the amp");
    }

    /// A preset with no `cab` set builds a Cab block with no realization —
    /// which has no backend and is skipped at install, so a full-rig
    /// capture (`amp_cab`) plays exactly as it did before this block existed.
    #[test]
    fn no_cab_on_the_preset_is_a_passthrough_block() {
        let profile = build_profile(&worship_def(), &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        let cab = block(patch, "Cab L");
        assert!(cab.ir.is_empty(), "no cab picked ⇒ no realization");
    }

    /// A preset with `cab` set builds a Cab block realized by that IR — what
    /// makes an amp-only capture usable at all.
    #[test]
    fn a_preset_cab_becomes_the_cab_blocks_ir() {
        let mut def = worship_def();
        let preset_name = def.presets.first().expect("a preset").name.clone();
        def.presets.first_mut().expect("a preset").cab = "/tmp/v30.wav".to_string();
        let patch_name = def
            .patches
            .iter()
            .find(|p| p.preset.eq_ignore_ascii_case(&preset_name))
            .expect("a patch on that preset")
            .name
            .clone();
        let profile = build_profile(&def, &drive_presets());
        let patch = profile
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&patch_name))
            .expect("the built patch");
        let cab = block(patch, "Cab L");
        assert_eq!(cab.ir, "/tmp/v30.wav");
    }

    /// A patch with no second amp still has the Amp R / Cab R blocks (so the
    /// board's shape is constant) but bypassed — an unloaded slot is silent,
    /// not broken.
    #[test]
    fn an_unloaded_second_amp_is_bypassed_not_missing() {
        let profile = build_profile(&worship_def(), &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        assert!(block(patch, "Amp R").bypassed);
        assert!(block(patch, "Cab R").bypassed);
    }

    /// Pointing a patch at a second preset loads Amp R for real, engaged by
    /// default — the player asked for it, so it should be audible.
    #[test]
    fn a_second_preset_loads_amp_r_engaged() {
        let mut def = worship_def();
        let second = def.presets.get(1).expect("a second preset").name.clone();
        def.patches.first_mut().expect("a patch").preset2 = second.clone();
        let profile = build_profile(&def, &drive_presets());
        let patch = profile.patches.first().expect("a patch");
        let amp_r = block(patch, "Amp R");
        assert!(!amp_r.bypassed);
        assert!(!amp_r.nam.is_empty());
    }
}

#[cfg(test)]
mod section_tests {
    use super::*;

    fn song_with_sections() -> SongDef {
        SongDef {
            patches: Vec::new(),
            profile: String::new(),
            start_part: String::new(),
            start_patch: String::new(),
            name: "Test Song".into(),
            key: "E".into(),
            bpm: 120,
            stack: 0,
            parts: vec!["Verse".into(), "Chorus".into(), "Bridge".into()],
            stack_defaults: Vec::new(),
            switch_actions: Vec::new(),
            patch_overrides: Vec::new(),
            patch_versions: Vec::new(),
            part_recalls: vec![
                // A section that only changes things — no patch of its own.
                PartRecallDef {
                    profile: String::new(),
                    part: "Chorus".into(),
                    patch: String::new(),
                    overrides: vec![OverrideDef::set("Time", "VERB 1", "mix", 0.35)],
                    ..Default::default()
                },
                // A section that recalls a patch AND changes something.
                PartRecallDef {
                    profile: String::new(),
                    part: "Bridge".into(),
                    patch: "Lead".into(),
                    overrides: vec![OverrideDef::set("Time", "DLY 1", "mix", 0.4)],
                    ..Default::default()
                },
            ],
        }
    }

    /// A section can change parameters without recalling a whole patch.
    ///
    /// The whole point. Recalling a patch for every variation forces a
    /// second patch to build, level and maintain for a chorus that is the
    /// verse's sound with the reverb up.
    #[test]
    fn a_section_can_change_things_without_recalling_a_patch() {
        let song = song_with_sections();
        let changes = song.parts_with_changes();
        let chorus = changes
            .iter()
            .find(|(name, ..)| name == "Chorus")
            .expect("Chorus");
        assert!(chorus.1.is_empty(), "no patch recall");
        assert_eq!(chorus.2.len(), 1, "but it changes one thing");
        assert_eq!(chorus.2[0].block, "VERB 1");
    }

    /// A section that does neither is still listed. Sections are the song's
    /// structure first and the rig's second — a verse with nothing dialed is
    /// still a verse, and dropping it would renumber every section after it.
    #[test]
    fn a_section_with_nothing_set_is_still_a_section() {
        let song = song_with_sections();
        let changes = song.parts_with_changes();
        assert_eq!(changes.len(), 3, "all three sections are listed");
        let verse = &changes[0];
        assert_eq!(verse.0, "Verse");
        assert!(verse.1.is_empty() && verse.2.is_empty());
    }

    /// Patch recall and overrides are independent halves of one section.
    #[test]
    fn a_section_can_do_both() {
        let song = song_with_sections();
        let bridge = song
            .parts_with_changes()
            .into_iter()
            .find(|(name, ..)| name == "Bridge")
            .expect("Bridge");
        assert_eq!(bridge.1, "Lead");
        assert_eq!(bridge.2.len(), 1);
    }

    /// Renaming a section keeps everything it was given.
    ///
    /// The subtle one. Recalls are keyed by name, so a rename that did not
    /// carry them would silently empty the section — and it reads as "the
    /// rename worked and it was always blank", which the player finds out
    /// mid-song.
    #[test]
    fn renaming_a_section_carries_what_it_recalls_and_changes() {
        let mut song = song_with_sections();
        assert!(song.rename_part("Bridge", "Instrumental"));
        let changes = song.parts_with_changes();
        assert!(
            changes.iter().all(|(n, ..)| n != "Bridge"),
            "the old name is gone"
        );
        let renamed = changes
            .iter()
            .find(|(n, ..)| n == "Instrumental")
            .expect("the new name is there");
        assert_eq!(renamed.1, "Lead", "it kept its patch");
        assert_eq!(renamed.2.len(), 1, "and what it changes");
    }

    /// Two sections cannot share a name, because recalls are keyed by it.
    #[test]
    fn section_names_are_unique() {
        let mut song = song_with_sections();
        assert!(!song.add_part("Chorus"), "duplicate refused");
        assert!(!song.add_part("  chorus "), "and case/space insensitively");
        assert!(
            !song.rename_part("Verse", "Chorus"),
            "rename cannot collide"
        );
        assert_eq!(song.parts.len(), 3);
    }

    /// Adding names a section at the end; empty names are not sections.
    #[test]
    fn adding_appends_and_refuses_nothing() {
        let mut song = song_with_sections();
        assert!(song.add_part("Outro"));
        assert_eq!(song.parts.last().map(String::as_str), Some("Outro"));
        assert!(!song.add_part("   "));
        assert_eq!(song.parts.len(), 4);
    }

    /// Removing a section takes its recall with it — otherwise a section
    /// added later under the same name would inherit a stranger's settings.
    #[test]
    fn removing_a_section_takes_its_recall() {
        let mut song = song_with_sections();
        assert!(song.remove_part("Bridge"));
        assert_eq!(song.parts.len(), 2);
        assert!(
            song.part_recalls
                .iter()
                .all(|r| !r.part.eq_ignore_ascii_case("Bridge")),
            "its recall went with it"
        );
        assert!(!song.remove_part("Bridge"), "and it is gone");
    }

    /// Reordering does not disturb what any section recalls. Recalls are
    /// keyed by name precisely so arranging a song is free.
    #[test]
    fn moving_a_section_keeps_every_recall() {
        let mut song = song_with_sections();
        let before = song.parts_with_changes();
        assert!(song.move_part(2, 0), "Bridge to the front");
        assert_eq!(song.parts[0], "Bridge");
        let after = song.parts_with_changes();
        for (name, patch, ovs) in &before {
            let now = after
                .iter()
                .find(|(n, ..)| n == name)
                .unwrap_or_else(|| panic!("{name} survived"));
            assert_eq!(&now.1, patch, "{name} kept its patch");
            assert_eq!(now.2.len(), ovs.len(), "{name} kept its changes");
        }
        assert!(!song.move_part(0, 9), "out of range is refused");
        assert!(!song.move_part(1, 1), "and a no-op is not a change");
    }

    /// `parts_with_recalls` still answers what it always answered, so the
    /// callers that only want the patch are unaffected.
    #[test]
    fn the_old_view_still_works() {
        let song = song_with_sections();
        let recalls = song.parts_with_recalls();
        assert_eq!(recalls.len(), 3);
        assert_eq!(recalls[2], ("Bridge".to_string(), "Lead".to_string()));
    }

    #[test]
    fn a_song_names_its_profile_and_where_it_starts_and_old_files_still_read() {
        let text = r#"name AMAZING!
key B
bpm 145
stack 0
parts ("Intro Lead" Verse)
stack_defaults ()
profile Worship
start_part "Intro Lead"
part_recalls ({part Verse, profile Rock, patch Crunch, overrides ()})
"#;
        let song: super::SongDef = facet_styx::from_str(text).expect("a song with profiles parses");
        assert_eq!(song.profile, "Worship");
        assert_eq!(song.start_part, "Intro Lead");
        assert_eq!(song.part_recalls[0].profile, "Rock");
        // A file from before songs named profiles reads with them empty.
        let old = "name X\nkey G\nbpm 70\nstack 0\nparts ()\nstack_defaults ()\n";
        let song: super::SongDef = facet_styx::from_str(old).expect("an old song parses");
        assert!(song.profile.is_empty() && song.start_part.is_empty());
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    fn pedal(name: &str, options: &[(&str, &str)]) -> DrivePresetDef {
        DrivePresetDef {
            name: name.into(),
            options: options
                .iter()
                .map(|(n, f)| DriveOptionDef { name: (*n).into(), nam: (*f).into(), hash: String::new(), level_db: 0.0 })
                .collect(),
        }
    }

    fn slot(block: &str, preset: &str, option: usize) -> DriveSlotDef {
        DriveSlotDef { block: block.into(), preset: preset.into(), option }
    }

    /// A slot names the pedal assigned to it and which capture; the boost
    /// slot unassigned plays the library's boost pedal, or the native boost
    /// when there is none; an unassigned drive slot is empty.
    #[test]
    fn a_slot_names_the_pedal_it_plays() {
        let dps = vec![
            pedal("King of Tone", &[("Red", "/m/kot red.nam"), ("Both Sides", "/m/kot both.nam")]),
            pedal("Clean Boost", &[("King of Tone Red", "/m/kot red.nam")]),
        ];
        let drives = vec![slot("Drive 1", "King of Tone", 1)];
        let d1 = slot_pedal("Drive 1", &drives, &dps);
        assert_eq!((d1.pedal.as_str(), d1.option.as_str(), d1.empty), ("King of Tone", "Both Sides", false));
        assert_eq!(d1.nam, "/m/kot both.nam");
        let boost = slot_pedal("Boost", &drives, &dps);
        assert_eq!((boost.pedal.as_str(), boost.option.as_str()), ("Clean Boost", "King of Tone Red"));
        let native = slot_pedal("Boost", &drives, &dps[..1]);
        assert_eq!(native.pedal, "Clean Boost (built-in)");
        assert!(!native.empty);
        let empty = slot_pedal("Drive 3", &drives, &dps);
        assert!(empty.empty && empty.pedal.is_empty());
        // A slot assigned a pedal the library no longer has is empty too.
        assert!(slot_pedal("Drive 2", &[slot("Drive 2", "Gone", 0)], &dps).empty);
    }
}
