//! Write the bass's shipped library (`default-config-bass/`): the guitar
//! rig's own system — the same chain, modules, presets and profiles —
//! tuned for bass.
//!
//!   cargo run -p signal-guitar --example bass_library -- <captures dir>
//!
//! The captures dir holds the bass `.nam` files (the Aguilar Tone Hammer,
//! the JHS Kilt, the JH Kilt 10, the Parallax); they are copied into
//! `default-config-bass/models/`. The Time, Delay, Reverb and Modulation
//! modules and every block preset come over from the guitar's library.
//!
//! The Worship profile, after the worship bassist's board — buffer, tuner,
//! Microsynth, Kilt, Tone Hammer, a DI — and his mix chain:
//!
//! | stack | patches |
//! |---|---|
//! | Clean | Amp (the Tone Hammer, AGS on), DI (flat, AGS off) |
//! | Crunch | the Kilt's boost into the Tone Hammer |
//! | Drive | the Kilt's drive |
//! | Synth | Moog (an octave down through a swept low-pass), Env (an envelope filter) |
//! | Fuzz | the Kilt with both sides up |

use std::path::{Path, PathBuf};

use signal_guitar::compose::{
    BlockChoiceDef, BlockLib, BlockPresetDef, ModuleLib, ModulePresetDef, ModuleSnapshotDef, ParamSetDef, PresetLib,
    PresetSnapshotDef, RigPresetDef, ToneLib,
};
use signal_guitar::library::DrivePresetLib;
use signal_guitar::profiles::{
    DriveOptionDef, DrivePresetDef, DriveSlotDef, ModuleChoiceDef, OverrideDef, PatchDef, ProfileDef, StackDef,
};

const AMP: &str = "Aguilar Tone Hammer";
const KILT: &str = "JHS Kilt";
const CORE: &str = "Tone Hammer";

/// A capture's library path, by the file name it is copied in as.
fn model(file: &str) -> String {
    format!("models/{file}")
}

fn params(ps: &[(&str, f32)]) -> Vec<ParamSetDef> {
    ps.iter().map(|(n, v)| ParamSetDef { param: (*n).to_string(), value: *v }).collect()
}

fn block_preset(ty: &str, name: &str, ps: &[(&str, f32)], target_gr_db: f32) -> BlockPresetDef {
    BlockPresetDef { block_type: ty.into(), name: name.into(), params: params(ps), bypass: false, target_gr_db, macros: Vec::new() }
}

fn bypass(block: &str, on: bool) -> OverrideDef {
    OverrideDef { module: "Drive".into(), block: block.into(), param: String::new(), op: "bypass".into(), value: if on { 1.0 } else { 0.0 }, text: String::new() }
}

fn choice(module: &str, preset: &str, snapshot: &str) -> ModuleChoiceDef {
    ModuleChoiceDef { module: module.into(), preset: preset.into(), snapshot: snapshot.into() }
}

fn block(b: &str, preset: &str) -> BlockChoiceDef {
    BlockChoiceDef { block: b.into(), preset: preset.into() }
}

fn read<T: for<'a> facet::Facet<'a>>(path: &Path) -> T {
    facet_styx::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .unwrap_or_else(|e| panic!("{} parses: {e}", path.display()))
}

fn write<T: for<'a> facet::Facet<'a>>(path: &Path, value: &T) {
    std::fs::write(path, facet_styx::to_string(value).expect("serialises")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!("wrote {}", path.display());
}

fn main() {
    let src = PathBuf::from(std::env::args().nth(1).expect("usage: bass_library <captures dir>"));
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let guitar = here.join("default-config");
    let out = here.join("default-config-bass");
    std::fs::create_dir_all(out.join("models")).unwrap();
    std::fs::create_dir_all(out.join("profiles")).unwrap();

    // ── Captures: copied in under their own names ─────────────────────
    let mut found = Vec::new();
    for e in walk(&src) {
        if e.extension().is_some_and(|x| x == "nam") {
            let name = e.file_name().unwrap().to_string_lossy().into_owned();
            std::fs::copy(&e, out.join("models").join(&name)).unwrap();
            found.push(name);
        }
    }
    found.sort();
    println!("{} captures", found.len());
    let has = |f: &str| assert!(found.iter().any(|n| n == f), "missing capture {f}");

    // ── Pedals ────────────────────────────────────────────────────────
    let opt = |name: &str, file: &str| {
        has(file);
        DriveOptionDef { name: name.into(), nam: model(file), hash: String::new(), level_db: 0.0 }
    };
    let pedals = vec![
        DrivePresetDef {
            name: KILT.into(),
            options: vec![
                opt("Boost", "JHS Kilt v2 OD gain 3.nam"),
                opt("Drive", "JHS Kilt v2 OD gain 5.nam"),
                opt("Drive Hot", "JHS Kilt v2 OD Gain 10.nam"),
                opt("Fuzz", "JHS Kilt v1 Fuzz gain 3.nam"),
            ],
        },
        DrivePresetDef {
            name: "JH Kilt 10".into(),
            options: vec![
                opt("Low Gain", "JH KILT 10 LOW GAIN.nam"),
                opt("Mid Gain", "JH KILT 10 MID GAIN.nam"),
                opt("Hi Gain", "JH KILT 10 HI GAIN.nam"),
                opt("Red Fuzz", "JH KILT 10 RED FUZZ.nam"),
                opt("G1 Low Gain", "JH KILT 10 G1 ON LOW GAIN.nam"),
                opt("G1 Mid Gain", "JH KILT 10 G1 ON MID GAIN.nam"),
                opt("G1 Hi Gain", "JH KILT 10 G1 ON HI GAIN.nam"),
                opt("G1 Red Fuzz", "JH KILT 10 G1 ON RED FUZZ.nam"),
            ],
        },
        DrivePresetDef {
            name: "Parallax".into(),
            options: vec![
                opt("Dist Smooth", "PARALLAX DIST SMOOTH.nam"),
                opt("Default", "PARALLAX DEFAULT (No Cab).nam"),
                opt("Default Raw", "PARALLAX DEFAULT (No Comp and No Cab).nam"),
            ],
        },
    ];
    write(&out.join("drive-presets.styx"), &DrivePresetLib { presets: pedals });

    // ── Modules: the amp, the board, and the guitar's Time/Modulation ──
    let mut modules: Vec<ModulePresetDef> = read::<ModuleLib>(&guitar.join("modules.styx"))
        .presets
        .into_iter()
        .filter(|m| matches!(m.module.as_str(), "Time" | "Delay" | "Reverb" | "Modulation"))
        .collect();
    let amp_snap = |name: &str, file: &str| {
        has(file);
        ModuleSnapshotDef { name: name.into(), nam: model(file), ..ModuleSnapshotDef::default() }
    };
    modules.push(ModulePresetDef {
        module: "Amp".into(),
        name: AMP.into(),
        snapshots: vec![
            amp_snap("AGS", "AGS Med- AGS. Gain 5, Bass 5, Mid 5, Treble 5.nam"),
            amp_snap("Flat", "Full Flat - Gain 5, Bass 5, Mid Level 5, Mid Freq 5, Treble 5.nam"),
            amp_snap("AGS Warm", "AGS Warm- AGS Gain 3, Bass 6, Mid 6, Mid Freq 4, Treble 4.nam"),
            amp_snap("AGS High", "AGS High- AGS, Gain 7, Bass 5, Mid 5, Treble 4.nam"),
            amp_snap("Warm Vintage", "Warm Vintage - Gain 5, Bass 6, Mid Level 4, Mid Freq 6, Treble 4.nam"),
            amp_snap("Mid Scoop", "Mid Scoop - Bass 6, Mid 3, Mid Freq 1, Treble 7.nam"),
            amp_snap("Low-Mid Boost", "Low-Mid Boost- Gain 5, Bass 6, Mid 7, Mid Freq 3, Treble 4.nam"),
            amp_snap("Mid-Hi Boost", "Mid-Hi Boost- Gain 5, Bass 3, Mid 7, Mid Freq 6, Treble 7.nam"),
        ],
    });
    // The board: one pedal in Drive 1, the other slots off.
    let all_off = || vec![bypass("Boost", true), bypass("Drive 1", true), bypass("Drive 2", true), bypass("Drive 3", true)];
    // A bass hits a drive far harder than a guitar (measured at a low E,
    // 82 Hz): the boost is the gain-3 Kilt with 9 dB less into it (≈14 %
    // THD, "a little teeth"); the drive the JH Kilt 10's low gain, a steady
    // 25 %; the fuzz the v1, 44 %.
    let kilt = |name: &str, pedal: &str, option: usize, drive: Option<f32>| ModuleSnapshotDef {
        name: name.into(),
        drives: vec![DriveSlotDef { block: "Drive 1".into(), preset: pedal.into(), option }],
        overrides: {
            let mut o = vec![bypass("Boost", true), bypass("Drive 1", false), bypass("Drive 2", true), bypass("Drive 3", true)];
            if let Some(d) = drive {
                o.push(OverrideDef::set("Drive", "Drive 1", "drive", d));
            }
            o
        },
        ..ModuleSnapshotDef::default()
    };
    modules.push(ModulePresetDef {
        module: "Drive".into(),
        name: "Off".into(),
        snapshots: vec![ModuleSnapshotDef {
            name: "Off".into(),
            drives: vec![DriveSlotDef { block: "Drive 1".into(), preset: KILT.into(), option: 0 }],
            overrides: all_off(),
            ..ModuleSnapshotDef::default()
        }],
    });
    modules.push(ModulePresetDef {
        module: "Drive".into(),
        name: KILT.into(),
        snapshots: vec![
            kilt("Boost", KILT, 0, Some(0.125)),
            kilt("Drive", "JH Kilt 10", 0, None),
            kilt("Drive Hot", "JH Kilt 10", 1, None),
            kilt("Fuzz", KILT, 3, None),
        ],
    });
    // The Parallax (Metal) as an amp: its full captures carry their own
    // compressor and cab — the smooth distortion (≈10 % THD) and the
    // default (≈25 %); the cab-less ones are near clean.
    modules.push(ModulePresetDef {
        module: "Amp".into(),
        name: "Parallax".into(),
        snapshots: vec![
            amp_snap("Dist Smooth", "PARALLAX DIST SMOOTH.nam"),
            // Its distortion peaks with less in (≈ −22 dB): the chain hits
            // it ~10 dB hotter, so the most trim the amp's drive gives.
            ModuleSnapshotDef {
                overrides: vec![OverrideDef::set("Amp", "Amp L", "drive", 0.0)],
                ..amp_snap("Default", "PARALLAX DEFAULT.nam")
            },
            amp_snap("Default No Cab", "PARALLAX DEFAULT (No Cab).nam"),
            amp_snap("Raw", "PARALLAX DEFAULT (No Comp and No Cab).nam"),
        ],
    });
    write(&out.join("modules.styx"), &ModuleLib { presets: modules });

    // ── Block presets: the guitar's, and the bass's own ───────────────
    let mut blocks = read::<BlockLib>(&guitar.join("blocks.styx")).presets;
    blocks.extend([
        // The Microsynth's octave: the note an octave down beside it.
        block_preset("pitch", "Microsynth Octave", &[("semitones", -12.0), ("b_semitones", -24.0), ("a_level", 0.85), ("b_level", 0.0), ("dry", 0.75), ("cents", 0.0), ("mix", 1.0)], 0.0),
        // A Moog-ish sweep: a resonant low-pass the pick opens, warmed.
        block_preset("filter", "Moog Sweep", &[("mode", 0.0), ("cutoff", 0.37), ("resonance", 0.55), ("drive", 0.3), ("env_octaves", 3.5), ("env_sens", 0.6), ("env_attack", 2.0), ("env_release", 260.0), ("mix", 1.0)], 0.0),
        // An envelope filter (Q-Tron): a band-pass the playing sweeps up,
        // some of the dry under it to keep the low end.
        block_preset("filter", "Envelope", &[("mode", 2.0), ("cutoff", 0.40), ("resonance", 0.5), ("env_octaves", 3.0), ("env_sens", 0.55), ("env_attack", 4.0), ("env_release", 120.0), ("mix", 0.75)], 0.0),
        // The mix chain's opto compressor (an LA-2A's job): 3 dB, slow.
        block_preset("compressor", "Bass Opto", &[("threshold", -24.0), ("ratio", 3.0), ("attack", 10.0), ("release", 300.0), ("knee", 8.0), ("style", 3.0), ("mix", 1.0), ("makeup", 2.0)], 3.0),
        // Harder, for the synth (5 dB).
        block_preset("compressor", "Bass Opto Heavy", &[("threshold", -28.0), ("ratio", 4.0), ("attack", 10.0), ("release", 300.0), ("knee", 8.0), ("style", 3.0), ("mix", 1.0), ("makeup", 3.0)], 5.0),
        // The mix EQ: rumble out, the woof at 350, the box at 900, string
        // noise at 2.5 k (narrow), the articulation back at 6 k, the top
        // rolled off.
        block_preset(
            "eq",
            "Bass Mix EQ",
            &[
                ("b1_used", 1.0), ("b1_on", 1.0), ("b1_freq", 32.0), ("b1_shape", 3.0),
                ("b2_used", 1.0), ("b2_on", 1.0), ("b2_freq", 350.0), ("b2_gain", -2.5), ("b2_q", 1.4),
                ("b3_used", 1.0), ("b3_on", 1.0), ("b3_freq", 900.0), ("b3_gain", -3.0), ("b3_q", 2.5),
                ("b4_used", 1.0), ("b4_on", 1.0), ("b4_freq", 2500.0), ("b4_gain", -3.0), ("b4_q", 3.0),
                ("b5_used", 1.0), ("b5_on", 1.0), ("b5_freq", 6000.0), ("b5_gain", 2.0), ("b5_q", 0.8),
                ("b6_used", 1.0), ("b6_on", 1.0), ("b6_freq", 9000.0), ("b6_shape", 4.0),
            ],
            0.0,
        ),
        // A bass's gate: lower, and slower to close on a held note.
        block_preset("gate", "Bass Gate", &[("threshold", -68.0), ("attack", 1.0), ("release", 200.0)], 0.0),
    ]);
    write(&out.join("blocks.styx"), &BlockLib { presets: blocks });

    // ── The Core: the Tone Hammer and its variations ──────────────────
    let studio = |comp: &str| vec![block("Gate", "Bass Gate"), block("Post Comp", comp), block("Amp EQ", "Bass Mix EQ")];
    // Each variation's level, so all of them land together (≈ −21 LUFS,
    // measured by `tone_audit`): the flat DI is quieter than the AGS, the
    // synths quieter still.
    let level = |name: &str| match name {
        "Amp" => 3.0,
        "DI" => 9.9,
        "Crunch" => -0.7,
        "Drive" => 1.2,
        "Moog" => 6.0,
        "Env" => 6.2,
        "Fuzz" => -0.8,
        "Ambient" => 7.9,
        _ => 0.0,
    };
    // Metal's levels (measured the same way).
    let metal_level = |name: &str| match name {
        "Clean" => 3.0,
        "Crunch" => 4.4,
        "Drive" => 2.6,
        _ => 0.0,
    };
    let snap = |name: &str, drive: (&str, &str), amp: &str, extra: &[(&str, &str)], comp: &str| {
        let mut blocks = studio(comp);
        blocks.extend(extra.iter().map(|(b, p)| block(b, p)));
        PresetSnapshotDef {
            name: name.into(),
            modules: vec![choice("Drive", drive.0, drive.1), choice("Amp", AMP, amp)],
            blocks,
            level_db: level(name),
            ..PresetSnapshotDef::default()
        }
    };
    let core = RigPresetDef {
        name: CORE.into(),
        snapshots: vec![
            snap("Amp", ("Off", "Off"), "AGS", &[], "Bass Opto"),
            snap("DI", ("Off", "Off"), "Flat", &[], "Bass Opto"),
            snap("Crunch", (KILT, "Boost"), "AGS", &[], "Bass Opto"),
            snap("Drive", (KILT, "Drive"), "AGS", &[], "Bass Opto"),
            snap("Fuzz", (KILT, "Fuzz"), "AGS", &[], "Bass Opto"),
            snap("Moog", ("Off", "Off"), "AGS", &[("Pitch", "Microsynth Octave"), ("Filter", "Moog Sweep")], "Bass Opto Heavy"),
            snap("Env", ("Off", "Off"), "AGS", &[("Filter", "Envelope")], "Bass Opto"),
            snap("Ambient", ("Off", "Off"), "AGS Warm", &[], "Bass Opto"),
        ],
    };
    // Metal's Core: the Tone Hammer clean, the Parallax for the dirt.
    let with_amp = |mut sn: PresetSnapshotDef, preset: &str, snapshot: &str, level: f32| {
        sn.modules = vec![choice("Drive", "Off", "Off"), choice("Amp", preset, snapshot)];
        sn.level_db = level;
        sn
    };
    let metal_core = RigPresetDef {
        name: "Parallax".into(),
        snapshots: vec![
            with_amp(snap("Clean", ("Off", "Off"), "AGS", &[], "Bass Opto"), AMP, "AGS", metal_level("Clean")),
            with_amp(snap("Crunch", ("Off", "Off"), "AGS", &[], "Bass Opto"), "Parallax", "Dist Smooth", metal_level("Crunch")),
            with_amp(snap("Drive", ("Off", "Off"), "AGS", &[], "Bass Opto"), "Parallax", "Default", metal_level("Drive")),
        ],
    };
    write(&out.join("presets.styx"), &PresetLib { presets: vec![core, metal_core] });
    write(&out.join("tones.styx"), &ToneLib { tones: Vec::new() });

    // ── Songs: the same songs and sections, the guitar's sounds left out
    //    (each section plays the bass profile's stacks until given one) ──
    let mut songs = read::<signal_guitar::library::SongLib>(&guitar.join("songs.styx"));
    for song in &mut songs.songs {
        song.patches.clear();
        song.stack_defaults.clear();
        song.patch_overrides.clear();
        song.patch_versions.clear();
        song.start_patch.clear();
        for r in &mut song.part_recalls {
            r.patch.clear();
            r.stack.clear();
            r.preset.clear();
            r.overrides.clear();
            r.stack_defaults.clear();
        }
    }
    write(&out.join("songs.styx"), &songs);
    std::fs::copy(guitar.join("setlists.styx"), out.join("setlists.styx")).unwrap();

    // ── The Worship profile ───────────────────────────────────────────
    let patch_in = |name: &str, snapshot: &str, time: (&str, &str)| PatchDef {
        name: name.into(),
        rig_preset: CORE.into(),
        snapshot: snapshot.into(),
        modules: vec![choice("Time", time.0, time.1)],
        ..PatchDef::default()
    };
    // Bass plays dry: the room is the mix's — but for the Ambient stack's
    // hall, for swells and pads.
    let patch = |name: &str, snapshot: &str| patch_in(name, snapshot, ("Dry", "Dry"));
    let stack = |name: &str, patches: &[&str]| StackDef { name: name.into(), patches: patches.iter().map(|p| (*p).to_string()).collect(), ..StackDef::default() };
    let worship = ProfileDef {
        name: "Worship".into(),
        patches: vec![
            patch("Amp", "Amp"),
            patch("DI", "DI"),
            patch("Crunch", "Crunch"),
            patch("Drive", "Drive"),
            patch("Moog", "Moog"),
            patch("Env", "Env"),
            patch("Fuzz", "Fuzz"),
            patch_in("Ambient", "Ambient", ("Ambience", "Hall")),
        ],
        stacks: vec![
            stack("Clean", &["Amp", "DI"]),
            stack("Crunch", &["Crunch"]),
            stack("Drive", &["Drive"]),
            stack("Synth", &["Moog", "Env"]),
            stack("Fuzz", &["Fuzz"]),
            // Switch 6: the hold layer's first, where the guitar has its
            // Ambient too.
            stack("Ambient", &["Ambient"]),
        ],
        ..ProfileDef::default()
    };
    write(&out.join("profiles/worship.styx"), &worship);
    // Metal: the Parallax — Clean, Crunch, Drive; it opens on Drive.
    let metal_patch = |name: &str| PatchDef { rig_preset: "Parallax".into(), ..patch(name, name) };
    let metal = ProfileDef {
        name: "Metal".into(),
        patches: vec![metal_patch("Clean"), metal_patch("Crunch"), metal_patch("Drive")],
        stacks: vec![stack("Clean", &["Clean"]), stack("Crunch", &["Crunch"]), stack("Drive", &["Drive"])],
        default_patch: "Drive".into(),
        ..ProfileDef::default()
    };
    write(&out.join("profiles/metal.styx"), &metal);
    // The legacy single profile a library seeds `profile.styx` from.
    write(&out.join("profile.styx"), &worship);
}

/// Every file under `dir`, recursively.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
