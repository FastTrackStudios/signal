//! What every cue bus receives, and what one of them must never.

use std::collections::BTreeMap;

use patch_list::headphones::{Audience, bus_for, plan, sends};
use patch_list::{FIXTURE_ALBUM, PatchList};

fn fixture() -> PatchList {
    PatchList::from_styx(FIXTURE_ALBUM).expect("the fixture album parses")
}

/// Every bus the album declares becomes a track, `HP` prefixed so the
/// template's own classifier reads it as a monitor path — which is what
/// keeps a cue mix out of `MIX BUS`.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn every_declared_bus_becomes_an_hp_track() {
    let buses = plan(&fixture(), None);
    assert!(!buses.is_empty(), "the fixture declares buses");
    assert!(
        buses.iter().all(|bus| bus.track.starts_with("HP ")),
        "a cue bus without the HP prefix would classify into the mix"
    );
    assert!(buses.iter().any(|bus| bus.track == "HP Cody"));
    assert!(buses.iter().any(|bus| bus.track == "HP Engineer"));
}

/// **The negative control the decision asks for.** Broadcast leaves the
/// building, so it takes the mix and nothing else: no talkback, no
/// guide. Getting this wrong puts a click and the control room's
/// chatter on air.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn broadcast_takes_neither_talkback_nor_guide() {
    let buses = plan(&fixture(), None);
    let broadcast = buses
        .iter()
        .find(|bus| bus.audience == Audience::Broadcast)
        .expect("the fixture declares a broadcast feed");
    assert!(!broadcast.takes_talkback(), "chatter would go out");
    assert!(!broadcast.takes_guide(), "a click would go out");
}

/// The control room hears the room; a player does not need their own
/// talkback back in their ears.
///
/// r[verify flow.scenes.performer-headphones]
#[test]
fn the_control_room_takes_talkback_and_a_player_takes_the_guide() {
    let buses = plan(&fixture(), None);
    let engineer = bus_for(&buses, "engineer").expect("an engineer bus");
    assert!(engineer.takes_talkback());
    assert!(!engineer.takes_guide());

    let cody = bus_for(&buses, "cody").expect("Cody's bus");
    assert!(cody.takes_guide(), "a player needs the count");
    assert!(!cody.takes_talkback());
}

/// `flow.scenes.performer-identity`: a track goes to its own
/// performer's bus and nobody else's, which is what makes the send the
/// record of who played what.
///
/// r[verify flow.scenes.performer-identity]
#[test]
fn a_track_sends_only_to_its_own_performers_bus() {
    let buses = plan(&fixture(), None);
    let mut owned = BTreeMap::new();
    owned.insert("gtr-rhythm-l".to_owned(), "cody".to_owned());
    owned.insert("gtr-rhythm-r".to_owned(), "cody".to_owned());
    owned.insert("lead-vox".to_owned(), "ron".to_owned());

    let routed = sends(&buses, &owned);
    assert_eq!(routed.len(), 3, "every owned track is routed once");
    for (track, bus) in &routed {
        let wanted = if track.starts_with("gtr") {
            "HP Cody"
        } else {
            "HP Ron"
        };
        assert_eq!(*bus, wanted, "{track} went to the wrong cue mix");
    }
}

/// A performer nobody declared a bus for is **skipped and visible**,
/// not routed somewhere plausible. Guessing here would put a take in
/// the wrong person's ears and hide the missing entry.
#[test]
fn a_performer_with_no_bus_is_skipped_rather_than_guessed() {
    let buses = plan(&fixture(), None);
    let mut owned = BTreeMap::new();
    owned.insert("mystery".to_owned(), "nobody".to_owned());
    assert!(
        sends(&buses, &owned).is_empty(),
        "an unknown performer was routed anyway"
    );
}

/// A shared bus answers for each of its listeners, so a choir section
/// needs one bus and keeps every singer's own identity.
///
/// r[verify flow.scenes.performer-identity]
#[test]
fn a_shared_bus_answers_for_each_of_its_listeners() {
    let mut list = fixture();
    list.headphones.insert(
        "choir".to_owned(),
        patch_list::list::Bus {
            output: "HP 8".to_owned(),
            audience: vec!["soprano".to_owned(), "alto".to_owned()],
        },
    );
    let buses = plan(&list, None);
    let soprano = bus_for(&buses, "soprano").expect("soprano listens somewhere");
    let alto = bus_for(&buses, "alto").expect("alto listens somewhere");
    assert_eq!(soprano.track, "HP Choir");
    assert_eq!(soprano.track, alto.track, "one bus, two identities");
}
