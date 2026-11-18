//! A realistic library to measure and try the docs on: the shape of Task's
//! dev seed (`task-server admin seed`: 8 live songs, the album songs, the
//! `Sunday Setlist` and `Album Release Show` setlists) played on the rigs
//! Signal's own fixtures build (the JM-style guitar megarig and a keys
//! rig), with the guitar profiles those fixtures seed — Worship, Blues,
//! Rock, All-Around — and a keys profile.
//!
//! Every id is a `seed_id`, so the library is the same on every run.

use crate::default_profile::LibrarySettings;
use crate::engine::EngineId;
use crate::ids::seed_id;
use crate::metadata::Metadata;
use crate::overrides::{NodePath, Override};
use crate::profile::{Patch, PatchId, Profile, ProfileId};
use crate::rig::{EngineSelection, Rig, RigScene, RigType};
use crate::setlist::{Setlist, SetlistEntry};
use crate::slug::slugify;
use crate::song::{Scene, Song};

use super::library::Library;

const GUITAR_RIG: &str = "guitar-megarig";
const KEYS_RIG: &str = "keys-megarig";

fn guitar_param(block: &str, param: &str) -> NodePath {
    NodePath::engine("guitar-engine")
        .with_layer("guitar-layer-archetype-jm")
        .with_block(block)
        .with_parameter(param)
}

fn keys_param(block: &str, param: &str) -> NodePath {
    NodePath::engine("keys-engine")
        .with_layer("keys-layer")
        .with_block(block)
        .with_parameter(param)
}

fn guitar_rig() -> Rig {
    let scene = |seed: &str, name: &str, engine_scene: &str| {
        RigScene::new(seed_id(seed), name).with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id(engine_scene),
        ))
    };
    let default = scene("guitar-megarig-default", "Default", "guitar-engine-default");
    let mut rig = Rig::new(
        seed_id(GUITAR_RIG),
        "MegaRig",
        vec![EngineId::from(seed_id("guitar-engine"))],
        default,
    )
    .with_rig_type(RigType::Guitar)
    .with_input_track("Guitar DI")
    .with_metadata(Metadata::new().with_tag("megarig").with_tag("guitar"));
    rig.add_variant(
        scene("guitar-megarig-lead", "Lead", "guitar-engine-lead")
            .with_override(Override::set(guitar_param("amp", "gain"), 0.80)),
    );
    rig.add_variant(
        scene("guitar-megarig-ambient", "Ambient", "guitar-engine-lead")
            .with_override(Override::set(guitar_param("dream-delay", "mix"), 0.45))
            .with_override(Override::set(guitar_param("studio-verb", "mix"), 0.50)),
    );
    rig.add_variant(
        scene("guitar-megarig-crunch", "Crunch", "guitar-engine-default")
            .with_override(Override::set(guitar_param("amp", "gain"), 0.55))
            .with_override(Override::bypass(guitar_param("halfman-od", "on"), false)),
    );
    rig
}

fn keys_rig() -> Rig {
    let scene = |seed: &str, name: &str| {
        RigScene::new(seed_id(seed), name).with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id(&format!("{seed}-engine")),
        ))
    };
    let mut rig = Rig::new(
        seed_id(KEYS_RIG),
        "Keys MegaRig",
        vec![EngineId::from(seed_id("keys-engine"))],
        scene("keys-megarig-default", "Default"),
    )
    .with_rig_type(RigType::Keys)
    .with_input_track("Keys MIDI")
    .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));
    rig.add_variant(
        scene("keys-megarig-wide", "Wide")
            .with_override(Override::set(keys_param("chorus", "width"), 0.9)),
    );
    rig.add_variant(scene("keys-megarig-focus", "Focus"));
    rig.add_variant(
        scene("keys-megarig-air", "Air")
            .with_override(Override::set(keys_param("verb", "mix"), 0.6)),
    );
    rig
}

/// One patch definition: name, rig, scene, and amp/mix overrides.
struct PatchDef {
    name: &'static str,
    keys: bool,
    scene: &'static str,
    overrides: &'static [(&'static str, &'static str, f32)],
}

const fn def(
    name: &'static str,
    scene: &'static str,
    overrides: &'static [(&'static str, &'static str, f32)],
) -> PatchDef {
    PatchDef {
        name,
        keys: false,
        scene,
        overrides,
    }
}

const fn keys_def(
    name: &'static str,
    scene: &'static str,
    overrides: &'static [(&'static str, &'static str, f32)],
) -> PatchDef {
    PatchDef {
        name,
        keys: true,
        scene,
        overrides,
    }
}

const PROFILES: &[(&str, &str, &[PatchDef])] = &[
    (
        "worship",
        "Worship",
        &[
            def("Clean", "default", &[("amp", "gain", 0.18)]),
            def("Crunch", "default", &[("amp", "gain", 0.48)]),
            def("Drive", "default", &[("amp", "gain", 0.60)]),
            def("Lead", "lead", &[("amp", "gain", 0.75)]),
            def(
                "Ambient",
                "ambient",
                &[("dream-delay", "mix", 0.45), ("studio-verb", "mix", 0.55)],
            ),
            def(
                "Tremolo",
                "default",
                &[
                    ("harmonic-tremolo", "depth", 0.6),
                    ("harmonic-tremolo", "rate", 0.35),
                ],
            ),
            def("Delay", "default", &[("dream-delay", "mix", 0.35)]),
            def(
                "Solo",
                "lead",
                &[("amp", "gain", 0.72), ("dream-delay", "mix", 0.30)],
            ),
        ],
    ),
    (
        "blues",
        "Blues",
        &[
            def("Clean", "default", &[("amp", "gain", 0.22)]),
            def("Crunch", "crunch", &[("amp", "gain", 0.50)]),
            def(
                "Drive",
                "crunch",
                &[("amp", "gain", 0.65), ("tealbreaker", "drive", 0.4)],
            ),
            def("Lead", "lead", &[("amp", "gain", 0.80)]),
        ],
    ),
    (
        "rock",
        "Rock",
        &[
            def("Crunch", "crunch", &[("amp", "gain", 0.55)]),
            def("Drive", "crunch", &[("amp", "gain", 0.70)]),
            def("Lead", "lead", &[("amp", "gain", 0.85)]),
            def(
                "Solo",
                "lead",
                &[
                    ("amp", "gain", 0.90),
                    ("dream-delay", "mix", 0.25),
                    ("jm-eq", "mid", 0.65),
                ],
            ),
        ],
    ),
    (
        "allround",
        "All-Around",
        &[
            def("Clean", "default", &[("amp", "gain", 0.20)]),
            def(
                "Sparkle",
                "default",
                &[("amp", "gain", 0.15), ("jm-eq", "treble", 0.7)],
            ),
            def("Crunch", "crunch", &[("amp", "gain", 0.50)]),
            def("Drive", "crunch", &[("amp", "gain", 0.66)]),
            def("Lead", "lead", &[("amp", "gain", 0.78)]),
            def("Ambient", "ambient", &[("studio-verb", "mix", 0.6)]),
            def(
                "Fuzz",
                "crunch",
                &[("amp", "gain", 0.9), ("halfman-od", "drive", 0.85)],
            ),
            def(
                "Solo",
                "lead",
                &[("amp", "gain", 0.82), ("dream-delay", "mix", 0.3)],
            ),
        ],
    ),
    (
        "keys",
        "Keys",
        &[
            keys_def("Piano", "default", &[("piano", "tone", 0.5)]),
            keys_def(
                "Pad",
                "air",
                &[("pad", "attack", 0.7), ("verb", "mix", 0.5)],
            ),
            keys_def("Organ", "focus", &[("organ", "drawbar-8", 0.8)]),
            keys_def("Wide Pad", "wide", &[("pad", "attack", 0.6)]),
        ],
    ),
];

fn profiles() -> Vec<Profile> {
    PROFILES
        .iter()
        .filter_map(|(slug, name, defs)| {
            let mut patches = defs.iter().map(|d| {
                let patch_slug = format!("{slug}-{}", slugify(d.name));
                let rig = if d.keys { KEYS_RIG } else { GUITAR_RIG };
                let param: fn(&str, &str) -> NodePath =
                    if d.keys { keys_param } else { guitar_param };
                let mut patch = Patch::from_rig_scene(
                    seed_id(&patch_slug),
                    d.name,
                    seed_id(rig),
                    seed_id(&format!("{rig}-{}", d.scene)),
                )
                .with_slug(patch_slug)
                .with_metadata(Metadata::new().with_tag(*slug));
                for (block, p, value) in d.overrides {
                    patch = patch.with_override(Override::set(param(block, p), *value));
                }
                patch
            });
            let mut profile =
                Profile::new(seed_id(&format!("{slug}-profile")), *name, patches.next()?);
            for patch in patches {
                profile.add_patch(patch);
            }
            Some(profile)
        })
        .collect()
}

/// The songs: (title, album, scenes as (scene name, patch slug)).
const SONGS: &[(&str, Option<&str>, &[(&str, &str)])] = &[
    // ── the live song library ──
    (
        "Rise and Shine",
        None,
        &[
            ("Intro", "worship-ambient"),
            ("Verse", "worship-clean"),
            ("Chorus", "worship-drive"),
            ("Bridge", "worship-lead"),
            ("Outro", "worship-ambient"),
        ],
    ),
    (
        "Every Morning New",
        None,
        &[
            ("Intro", "worship-tremolo"),
            ("Verse", "worship-clean"),
            ("Chorus", "worship-crunch"),
            ("Bridge", "worship-solo"),
            ("Outro", "worship-tremolo"),
        ],
    ),
    (
        "Anchor Line",
        None,
        &[
            ("Intro", "worship-clean"),
            ("Verse", "worship-crunch"),
            ("Chorus", "worship-lead"),
            ("Solo", "worship-solo"),
            ("Outro", "worship-delay"),
        ],
    ),
    (
        "Wildfire Praise",
        None,
        &[
            ("Intro", "worship-drive"),
            ("Verse", "worship-crunch"),
            ("Chorus", "worship-lead"),
            ("Bridge", "worship-ambient"),
            ("Outro", "worship-drive"),
        ],
    ),
    (
        "Steady Ground",
        None,
        &[
            ("Intro", "worship-delay"),
            ("Verse", "worship-clean"),
            ("Chorus", "worship-drive"),
            ("Bridge", "worship-lead"),
            ("Outro", "worship-delay"),
        ],
    ),
    (
        "Open Doors",
        None,
        &[
            ("Intro", "worship-tremolo"),
            ("Verse", INHERIT),
            ("Chorus", INHERIT),
            ("Outro", "worship-tremolo"),
        ],
    ),
    (
        "Brighter Still",
        None,
        &[
            ("Intro", "allround-sparkle"),
            ("Verse", "allround-clean"),
            ("Chorus", "allround-crunch"),
            ("Bridge", "keys-pad"),
            ("Solo", "allround-solo"),
            ("Outro", "allround-sparkle"),
        ],
    ),
    (
        "Carry Me Home",
        None,
        &[
            ("Intro", "keys-piano"),
            ("Verse", INHERIT),
            ("Chorus", "worship-lead"),
            ("Bridge", "worship-ambient"),
            ("Outro", "keys-piano"),
        ],
    ),
    // ── album songs the release show plays ──
    (
        "Static Heart",
        Some("Midnight Static"),
        &[
            ("Intro", "rock-crunch"),
            ("Verse", "rock-crunch"),
            ("Chorus", "rock-drive"),
            ("Solo", "rock-solo"),
            ("Outro", "rock-drive"),
        ],
    ),
    (
        "Neon Run",
        Some("Midnight Static"),
        &[
            ("Intro", "allround-fuzz"),
            ("Verse", "rock-crunch"),
            ("Chorus", "rock-lead"),
            ("Bridge", "keys-wide-pad"),
            ("Outro", "allround-fuzz"),
        ],
    ),
    (
        "Afterglow",
        Some("Golden Hour"),
        &[
            ("Intro", "keys-pad"),
            ("Verse", "blues-clean"),
            ("Chorus", "blues-crunch"),
            ("Solo", "blues-lead"),
            ("Outro", "keys-pad"),
        ],
    ),
    (
        "Signal Fade",
        Some("Roots & Wires"),
        &[
            ("Intro", "allround-ambient"),
            ("Verse", INHERIT),
            ("Chorus", "rock-drive"),
            ("Bridge", "allround-lead"),
            ("Outro", "allround-ambient"),
        ],
    ),
    (
        "City Sleeps",
        Some("Roots & Wires"),
        &[
            ("Intro", "keys-organ"),
            ("Verse", "blues-crunch"),
            ("Chorus", "blues-drive"),
            ("Solo", "rock-solo"),
            ("Outro", "keys-organ"),
        ],
    ),
];

/// The two setlists of the dev seed, as song titles.
pub const SETLISTS: &[(&str, &str, &[&str])] = &[
    (
        "sunday",
        "Sunday Setlist",
        &[
            "Rise and Shine",
            "Every Morning New",
            "Anchor Line",
            "Steady Ground",
            "Carry Me Home",
        ],
    ),
    (
        "release",
        "Album Release Show",
        &[
            "Static Heart",
            "Neon Run",
            "Afterglow",
            "Signal Fade",
            "City Sleeps",
            "Brighter Still",
        ],
    ),
];

/// A song's id from its title.
#[must_use]
pub fn song_id(title: &str) -> crate::song::SongId {
    seed_id(&format!("song-{}", slugify(title))).into()
}

/// A setlist's id from its seed slug (`sunday`, `release`).
#[must_use]
pub fn setlist_id(slug: &str) -> crate::setlist::SetlistId {
    seed_id(&format!("setlist-{slug}")).into()
}

/// A song from the [`SONGS`] table shape, its scenes sourcing patches by slug.
#[must_use]
pub fn song(title: &str, album: Option<&str>, scenes: &[(&str, &str)]) -> Option<Song> {
    let slug = slugify(title);
    let scene = |n: usize, (name, patch): &(&str, &str)| {
        let id = seed_id(&format!("song-{slug}-{n}"));
        let scene = if *patch == INHERIT {
            Scene::inheriting_default(id, *name)
        } else {
            Scene::from_patch(id, *name, PatchId::from(seed_id(patch)))
        };
        if *name == "Solo" {
            scene.with_override(Override::set(guitar_param("dream-delay", "mix"), 0.4))
        } else {
            scene
        }
    };
    let mut parts = scenes.iter().enumerate();
    let (n, first) = parts.next()?;
    let mut song = Song::new(song_id(title), title, scene(n, first));
    for (n, s) in parts {
        song.add_scene(scene(n, s));
    }
    song.artist = Some("FTS Collective".to_owned());
    let mut metadata = Metadata::new().with_tag("live");
    if let Some(album) = album {
        metadata = metadata
            .with_tag("album")
            .with_folder(format!("Albums/{album}"));
    }
    Some(song.with_metadata(metadata))
}

/// A setlist entry for the song titled `title`.
#[must_use]
pub fn entry(setlist: &str, title: &str) -> SetlistEntry {
    SetlistEntry::new(
        seed_id(&format!("setlist-{setlist}-{}", slugify(title))),
        title,
        song_id(title),
    )
}

/// The whole demo library.
#[must_use]
pub fn demo_library() -> Library {
    let songs = SONGS
        .iter()
        .filter_map(|(title, album, scenes)| song(title, *album, scenes))
        .collect();
    let setlists = SETLISTS
        .iter()
        .filter_map(|(slug, name, titles)| {
            let mut titles = titles.iter();
            let mut setlist = Setlist::new(setlist_id(slug), *name, entry(slug, titles.next()?));
            for title in titles {
                setlist.add_entry(entry(slug, title));
            }
            Some(setlist)
        })
        .collect();
    Library::from_profiles(
        vec![guitar_rig(), keys_rig()],
        &profiles(),
        LibrarySettings::with_default_profile(all_around_id()),
        songs,
        setlists,
    )
}

/// The All-Around profile's id — the library's default profile.
#[must_use]
pub fn all_around_id() -> ProfileId {
    ProfileId::from(seed_id("allround-profile"))
}

/// A scene's patch in [`SONGS`] that is not a patch: the scene inherits the
/// library's default profile (All-Around) by reference.
const INHERIT: &str = "";
