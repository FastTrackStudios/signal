//! The gangs, driven through the facade against daw-standalone.
//!
//! The unit tests beside the gangs prove what a gang *is*; these prove
//! that arming one track actually arms the others in a real project and
//! that the project reads back that way — the map's standard that a
//! scenario drives the real gesture and reads the project back, rather
//! than asserting on the thing that computed it.

use std::collections::HashMap;

use daw_proto::{ProjectContext, ProjectInfo, TrackRef, Tracks};
use daw_standalone::sync::Standalone;
use dynamic_template::golden_session::Kind;
use dynamic_template::grouping::follow_arm;
use dynamic_template::scenes::{Fact, Segment};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

/// Cody on a DI and an amp mic across two parts, one of them doubled,
/// and Ron beside him — so a track sits in a rig gang and a layer gang
/// at once, and there is someone else's rig to leave alone.
const RIG: [(&str, &str, Option<&str>); 4] = [
    ("Cody", "Rhythm", Some("L")),
    ("Cody", "Rhythm", Some("R")),
    ("Cody", "Lead", None),
    ("Ron", "Rhythm", None),
];
const MICS: [&str; 2] = ["DI", "SM57"];

struct Rig {
    daw: Standalone,
    project: ProjectContext,
    facts: Vec<Fact>,
    /// Track name to the guid the backend gave it — the test names
    /// tracks, the project owns the guids.
    guids: HashMap<String, String>,
}

impl Rig {
    fn new() -> Self {
        let daw = Standalone::new();
        let guid = daw.seed_project(ProjectInfo {
            guid: "reference".into(),
            name: "reference".into(),
            path: String::new(),
        });
        let project = ProjectContext::Project(guid);
        let electric = Segment::named("Electric").of(Kind::Group);
        let mut facts = Vec::new();
        let mut guids = HashMap::new();
        let mut index = 0;

        for (performer, part, channel) in RIG {
            for mic in MICS {
                let name = channel.map_or_else(
                    || format!("{performer} {part} {mic}"),
                    |c| format!("{performer} {part} {c} {mic}"),
                );
                let guid = Tracks::add(&daw, project.clone(), &name, None)
                    .expect("the standalone backend adds a track");
                let mut fact = Fact::leaf(&guid, &name, index, 2)
                    .of(Kind::Source)
                    .at(vec![electric.clone()]);
                fact.performer = Some((*performer).to_owned());
                fact.arrangement = Some((*part).to_owned());
                fact.multi_mic = Some((*mic).to_owned());
                fact.layer = Some((*part).to_owned());
                fact.channel = channel.map(str::to_owned);
                facts.push(fact);
                guids.insert(name, guid);
                index += 1;
            }
        }
        Self {
            daw,
            project,
            facts,
            guids,
        }
    }

    fn guid(&self, name: &str) -> &str {
        self.guids
            .get(name)
            .unwrap_or_else(|| panic!("no track named {name}"))
    }

    fn armed(&self, name: &str) -> bool {
        Tracks::get(
            &self.daw,
            self.project.clone(),
            TrackRef::Guid(self.guid(name).to_owned()),
        )
        .is_some_and(|track| track.armed)
    }

    fn arm(&self, name: &str, armed: bool) -> Result {
        let out = follow_arm(
            &self.daw,
            &self.project,
            &self.facts,
            self.guid(name),
            armed,
        )?;
        assert!(out.gangs > 0, "{name} belongs to no gang");
        Ok(())
    }
}

/// `flow.scenes.performer-rig`: arming one of Cody's DI tracks arms
/// every DI of Cody's, in every part — and leaves Ron's alone, and
/// leaves Cody's amp mics alone.
///
/// r[verify flow.scenes.performer-rig]
#[test]
fn arming_one_di_arms_that_performers_dis_and_no_one_elses() -> Result {
    let rig = Rig::new();
    rig.arm("Cody Rhythm L DI", true)?;

    assert!(rig.armed("Cody Rhythm R DI"), "the double's DI");
    assert!(rig.armed("Cody Lead DI"), "the other part's DI");
    assert!(!rig.armed("Ron Rhythm DI"), "Ron is not in Cody's gang");
    assert!(
        !rig.armed("Cody Lead SM57"),
        "a different source kind is a different gang"
    );
    Ok(())
}

/// The layer gang: arming one channel arms its siblings, so a double is
/// never half-recorded.
///
/// r[verify flow.scenes.groups]
#[test]
fn arming_one_channel_arms_the_layers_others() -> Result {
    let rig = Rig::new();
    rig.arm("Cody Rhythm L SM57", true)?;
    assert!(
        rig.armed("Cody Rhythm R SM57"),
        "the other channel of the layer"
    );
    Ok(())
}

/// Disarm mirrors arm — a gang is not a one-way latch.
///
/// r[verify flow.scenes.groups]
#[test]
fn disarm_mirrors_arm() -> Result {
    let rig = Rig::new();
    rig.arm("Cody Rhythm L DI", true)?;
    assert!(rig.armed("Cody Lead DI"));
    rig.arm("Cody Rhythm L DI", false)?;
    assert!(!rig.armed("Cody Lead DI"), "disarm did not follow");
    Ok(())
}

/// The echo, end to end. The watcher's own writes come back through the
/// same 30 Hz poll with no origin on them, so running the same gesture
/// twice must write nothing the second time — otherwise it loops.
///
/// r[verify flow.scenes.groups]
#[test]
fn the_second_pass_over_the_same_gesture_writes_nothing() -> Result {
    let rig = Rig::new();
    let guid = rig.guid("Cody Rhythm L DI").to_owned();
    let first = follow_arm(&rig.daw, &rig.project, &rig.facts, &guid, true)?;
    assert!(first.wrote > 0, "nothing propagated");
    let echo = follow_arm(&rig.daw, &rig.project, &rig.facts, &guid, true)?;
    assert_eq!(echo.wrote, 0, "an echo wrote again — the watcher loops");
    Ok(())
}

// ── the active-language switch ───────────────────────────────────────

use dynamic_template::grouping::follow_language;
use dynamic_template::scenes::Language;

/// Ron sings all three languages on his Main layer, plus one wordless
/// `All` part that belongs to every version — and his `Ron` and `Main`
/// mix tracks carry no language at all.
struct Vocals {
    daw: Standalone,
    project: ProjectContext,
    facts: Vec<Fact>,
    guids: HashMap<String, String>,
}

impl Vocals {
    fn new() -> Self {
        let daw = Standalone::new();
        let guid = daw.seed_project(ProjectInfo {
            guid: "vocals".into(),
            name: "vocals".into(),
            path: String::new(),
        });
        let project = ProjectContext::Project(guid);
        let vox = Segment::named("Vocals").of(Kind::Group);
        let mut facts = Vec::new();
        let mut guids = HashMap::new();
        let mut index = 0;

        let mut push = |name: &str, language: Option<Language>, is_folder: bool| {
            let guid = Tracks::add(&daw, project.clone(), name, None).expect("a track");
            let mut fact = if is_folder {
                Fact::folder(&guid, name, index, 1)
            } else {
                Fact::leaf(&guid, name, index, 2).of(Kind::Source)
            }
            .at(vec![vox.clone()]);
            fact.language = language;
            facts.push(fact);
            guids.insert(name.to_owned(), guid);
            index += 1;
        };

        push("Ron", None, true);
        push("Main", None, true);
        push("EN", Some(Language::En), false);
        push("ES", Some(Language::Es), false);
        push("PT", Some(Language::Pt), false);
        push("Hey", Some(Language::All), false);

        Self {
            daw,
            project,
            facts,
            guids,
        }
    }

    fn muted(&self, name: &str) -> bool {
        let guid = self.guids.get(name).expect("a seeded track").clone();
        Tracks::get(&self.daw, self.project.clone(), TrackRef::Guid(guid))
            .is_some_and(|track| track.muted)
    }
}

/// `flow.vocals.language.active`: switching is one write, and after it
/// **nothing of another language is audible**. This is the negative
/// control the whole rule exists for — "look at the English version"
/// that leaves a Spanish double under it is worse than not switching,
/// because the mix sounds wrong and nothing on screen says why.
///
/// r[verify flow.vocals.language.active]
#[test]
fn switching_leaves_no_other_language_audible() -> Result {
    let vox = Vocals::new();
    follow_language(&vox.daw, &vox.project, &vox.facts, Language::En)?;

    assert!(!vox.muted("EN"), "the active language must be heard");
    assert!(vox.muted("ES"), "a Spanish source stayed audible");
    assert!(vox.muted("PT"), "a Portuguese source stayed audible");
    Ok(())
}

/// A language-free source belongs to every version, so it is never
/// muted — muting it with a language would thin every render but one.
///
/// r[verify flow.vocals.language.active]
#[test]
fn a_language_free_source_is_never_muted() -> Result {
    let vox = Vocals::new();
    for active in [Language::En, Language::Es, Language::Pt] {
        follow_language(&vox.daw, &vox.project, &vox.facts, active)?;
        assert!(!vox.muted("Hey"), "All was muted with {active:?}");
    }
    Ok(())
}

/// The mix tracks above the language dimension carry no language, so
/// the switch never touches them — which is what keeps one chain
/// mixing every version.
///
/// r[verify flow.vocals.language.active]
#[test]
fn the_mix_tracks_are_untouched_by_the_switch() -> Result {
    let vox = Vocals::new();
    follow_language(&vox.daw, &vox.project, &vox.facts, Language::Es)?;
    assert!(!vox.muted("Ron"), "the performer's mix track was muted");
    assert!(!vox.muted("Main"), "the layer's mix track was muted");
    Ok(())
}

/// Switching back restores what it muted: the switch is a projection of
/// the active language, not a one-way latch.
///
/// r[verify flow.vocals.language.active]
#[test]
fn switching_back_restores_the_other_language() -> Result {
    let vox = Vocals::new();
    follow_language(&vox.daw, &vox.project, &vox.facts, Language::En)?;
    assert!(vox.muted("ES"));
    follow_language(&vox.daw, &vox.project, &vox.facts, Language::Es)?;
    assert!(!vox.muted("ES"), "switching back left it muted");
    assert!(vox.muted("EN"), "the old language stayed audible");
    Ok(())
}

/// The same echo guard: a second pass over an unchanged switch writes
/// nothing.
///
/// r[verify flow.vocals.language.active]
#[test]
fn a_second_switch_to_the_same_language_writes_nothing() -> Result {
    let vox = Vocals::new();
    let first = follow_language(&vox.daw, &vox.project, &vox.facts, Language::En)?;
    assert!(first.muted > 0, "nothing was muted");
    let echo = follow_language(&vox.daw, &vox.project, &vox.facts, Language::En)?;
    assert_eq!(echo.muted, 0);
    assert_eq!(echo.unmuted, 0, "an echo rewrote the mutes");
    Ok(())
}
