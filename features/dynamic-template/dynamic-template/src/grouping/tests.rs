//! What the gangs, the slots and the VCA leads are for, proven against
//! facts rather than against a DAW.

use std::collections::HashMap;

use super::{diff, gangs, leads, slots, GangKey};
use crate::golden_session::Kind;
use crate::scenes::{Fact, Segment};

/// Cody and Ron each play a Rhythm and a Lead part, each captured on a
/// DI and an SM57 — so two rig gangs of two, one per performer per
/// source kind, crossing the parts.
fn two_rigs() -> Vec<Fact> {
    let electric = Segment::named("Electric").of(Kind::Group);
    let mut out = vec![Fact::folder("electric", "Electric", 0, 0)
        .of(Kind::Group)
        .at(vec![electric.clone()])];
    let mut index = 1;
    for (performer, part) in [
        ("Cody", "Rhythm"),
        ("Cody", "Lead"),
        ("Ron", "Rhythm"),
        ("Ron", "Lead"),
    ] {
        for mic in ["DI", "SM57"] {
            let mut fact = Fact::leaf(
                Box::leak(format!("{performer}-{part}-{mic}").into_boxed_str()),
                mic,
                index,
                2,
            )
            .of(Kind::Source)
            .at(vec![electric.clone()]);
            fact.performer = Some(performer.to_owned());
            fact.arrangement = Some(part.to_owned());
            fact.multi_mic = Some(mic.to_owned());
            out.push(fact);
            index += 1;
        }
    }
    out
}

/// `flow.scenes.performer-rig`: one performer's tracks of one source
/// kind are a gang across every part they play — change the DI on
/// Cody's row and both of Cody's DI tracks take it, while Ron's do not.
///
/// r[verify flow.scenes.performer-rig]
#[test]
fn a_rig_gang_crosses_parts_but_never_performers() {
    let all = gangs(&two_rigs());
    let cody_di = all
        .iter()
        .find(|g| {
            g.key
                == GangKey::Rig {
                    performer: "Cody".to_owned(),
                    source_kind: "DI".to_owned(),
                }
        })
        .expect("Cody's DI gang");
    assert_eq!(cody_di.members, ["Cody-Rhythm-DI", "Cody-Lead-DI"]);
    assert!(
        !cody_di.members.iter().any(|m| m.starts_with("Ron")),
        "a rig gang must never cross performers: {:?}",
        cody_di.members
    );
}

/// A source kind is its own gang: changing the DI must not move the
/// amp mic of the same performer.
///
/// r[verify flow.scenes.performer-rig]
#[test]
fn source_kinds_are_separate_gangs() {
    let all = gangs(&two_rigs());
    let di: Vec<_> = all
        .iter()
        .filter(|g| matches!(&g.key, GangKey::Rig { source_kind, .. } if source_kind == "DI"))
        .flat_map(|g| g.members.clone())
        .collect();
    assert!(
        !di.iter().any(|m| m.ends_with("SM57")),
        "the DI gang swallowed an amp mic: {di:?}"
    );
}

/// The channels of one layer arm together, so a double is never
/// half-recorded. A layer with no channel dimension is not a gang: it
/// is undoubled and has nothing to move with.
///
/// r[verify flow.scenes.groups]
#[test]
fn a_layer_gangs_its_channels_and_an_undoubled_layer_is_no_gang() {
    let group = Segment::named("Electric").of(Kind::Group);
    let mut facts = Vec::new();
    for (i, (layer, channel)) in [("Main", Some("L")), ("Main", Some("R")), ("Solo", None)]
        .into_iter()
        .enumerate()
    {
        let guid = Box::leak(format!("{layer}-{}", channel.unwrap_or("only")).into_boxed_str());
        let mut fact = Fact::leaf(guid, layer, u32::try_from(i).unwrap_or(0), 1)
            .of(Kind::Source)
            .at(vec![group.clone()]);
        fact.layer = Some(layer.to_owned());
        fact.channel = channel.map(str::to_owned);
        facts.push(fact);
    }
    let layers: Vec<_> = gangs(&facts)
        .into_iter()
        .filter(|g| matches!(g.key, GangKey::Layer { .. }))
        .collect();
    assert_eq!(layers.len(), 1, "only the doubled layer is a gang");
    assert_eq!(layers[0].members, ["Main-L", "Main-R"]);
}

/// The echo guard: the watcher's own write comes back through the same
/// poll diff with no origin on it, so `diff` must answer nothing once
/// the gang already agrees. Without this the watcher loops.
///
/// r[verify flow.scenes.groups]
#[test]
fn an_echo_produces_no_second_write() {
    let all = gangs(&two_rigs());
    let gang = all.first().expect("a gang");
    let armed = |_: &str| true;
    assert!(
        diff(gang, true, &armed).is_empty(),
        "a gang already at the wanted state must write nothing"
    );
    assert_eq!(
        diff(gang, false, &armed).len(),
        gang.members.len(),
        "and every member that differs must be written"
    );
}

/// Slots are taken from the top downward, languages first, and the same
/// template always lands on the same slots — which is what makes
/// re-deriving on open equal to what was there before.
///
/// r[verify flow.scenes.groups]
#[test]
fn slots_are_deterministic_from_the_top_down() {
    let languages = ["EN".to_owned(), "ES".to_owned(), "PT".to_owned()];
    let folders = ["Rhythm".to_owned(), "Lead".to_owned()];
    let first = slots::assign(&languages, &folders);
    assert_eq!(first, slots::assign(&languages, &folders), "not stable");
    assert_eq!(first[0].0, 128, "FTS takes the top of the range");
    assert_eq!(first[0].1.slot_name(), "FTS LANG EN");
    assert_eq!(first[3].1.slot_name(), "FTS VCA Rhythm");
    let descending: Vec<u32> = first.iter().map(|(slot, _)| *slot).collect();
    assert_eq!(descending, [128, 127, 126, 125, 124]);
}

/// A song with no guitars must not shift the language slots — that is
/// why languages are assigned first.
///
/// r[verify flow.scenes.groups]
#[test]
fn a_song_without_guitars_keeps_the_language_slots() {
    let languages = ["EN".to_owned(), "ES".to_owned()];
    let with = slots::assign(&languages, &["Rhythm".to_owned()]);
    let without = slots::assign(&languages, &[]);
    assert_eq!(with[..2], without[..], "language slots moved with the song");
}

/// The top of the range does not survive a save, and the code says so
/// rather than letting a reopen discover it.
#[test]
fn the_top_of_the_range_is_session_only() {
    assert!(!slots::survives_save(128));
    assert!(slots::survives_save(64));
}

/// `flow.guitars.mixing`: a folder whose children route past it to a
/// bus needs a VCA lead over them, and a folder that sums its children
/// needs none.
///
/// r[verify flow.guitars.mixing]
#[test]
fn only_a_folder_its_children_bypass_needs_a_vca() {
    let group = Segment::named("Electric").of(Kind::Group);
    let facts = vec![
        Fact::folder("rhythm", "Rhythm", 0, 0).at(vec![group.clone()]),
        Fact::leaf("rhythm-l", "L", 1, 1).at(vec![group.clone()]),
        Fact::leaf("rhythm-r", "R", 2, 1).at(vec![group.clone()]),
        Fact::folder("kit", "Drum Kit", 3, 0).at(vec![group.clone()]),
        Fact::leaf("kick", "Kick", 4, 1).at(vec![group]),
    ];
    let mut sends = HashMap::new();
    // The guitar part's channels go to GTR RHYTHM, not up through the
    // folder; the kit's mics sum into their folder as usual.
    sends.insert("rhythm-l".to_owned(), false);
    sends.insert("rhythm-r".to_owned(), false);
    sends.insert("kick".to_owned(), true);

    let found = leads(&facts, &sends);
    assert_eq!(found.len(), 1, "only the bypassed folder leads: {found:?}");
    assert_eq!(found[0].folder_name, "Rhythm");
    assert_eq!(found[0].members, ["rhythm-l", "rhythm-r"]);
}
