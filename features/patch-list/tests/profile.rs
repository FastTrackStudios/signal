//! The studio profile round-trips through styx and resolves roles: a
//! role the room has to its device channel, a role it lacks to
//! `Unresolved` (spec #48: "an unknown role shows as unresolved").
//!
//! r[verify flow.patch-list.studio-profiles]

use patch_list::FIXTURE_STUDIO as ROOM;
use patch_list::profile::{Endpoint, Input, Output, Resolved, StudioProfile};

#[test]
fn the_fixture_profile_parses_with_its_tables_intact() {
    let room = StudioProfile::from_styx(ROOM).expect("the fixture profile parses");

    assert_eq!(
        room.inputs["Kick In"],
        Input::Audio {
            channel: 24,
            patchbay: None
        }
    );
    assert_eq!(
        room.inputs["Mic 5"],
        Input::Audio {
            channel: 8,
            patchbay: Some(Endpoint {
                node: "Scarlett 18i20".into(),
                port: "capture_AUX8".into()
            })
        }
    );
    assert_eq!(
        room.inputs["Nord"],
        Input::Midi {
            device: "Nord Stage 3".into(),
            channel: Some(1),
            patchbay: None
        }
    );
    assert_eq!(
        room.outputs["HP 1"],
        Output {
            left: 2,
            right: 3,
            patchbay: None
        }
    );
    assert_eq!(room.shared, ["Talkback"]);
}

#[test]
fn the_fixture_profile_round_trips() {
    let room = StudioProfile::from_styx(ROOM).expect("parse");
    let text = room.to_styx().expect("serialize");
    let back = StudioProfile::from_styx(&text).expect("parse what we wrote");
    assert_eq!(back, room, "written text was:\n{text}");
}

#[test]
fn a_role_the_room_has_resolves_to_its_channel() {
    let room = StudioProfile::from_styx(ROOM).expect("parse");
    assert_eq!(room.resolve_input("DI 3"), Resolved::Audio { channel: 2 });
    assert_eq!(
        room.resolve_input("Nord"),
        Resolved::Midi {
            device: "Nord Stage 3".into(),
            channel: Some(1)
        }
    );
    assert_eq!(
        room.resolve_output("HP 6"),
        Resolved::Pair {
            left: 12,
            right: 13
        }
    );
}

#[test]
fn a_role_the_room_lacks_is_unresolved_not_an_error() {
    let room = StudioProfile::from_styx(ROOM).expect("parse");
    assert_eq!(room.resolve_input("Mic 11"), Resolved::Unresolved);
    assert_eq!(room.resolve_output("HP 7"), Resolved::Unresolved);
    // The negative control: the same call on a role the room has.
    assert_ne!(room.resolve_input("Mic 10"), Resolved::Unresolved);
}

#[test]
fn shared_is_a_property_of_the_profile() {
    let room = StudioProfile::from_styx(ROOM).expect("parse");
    assert!(room.is_shared("Talkback"));
    assert!(!room.is_shared("Kick In"));
    // An empty profile shares nothing and resolves nothing.
    let empty = StudioProfile::from_styx("").expect("an empty profile");
    assert!(!empty.is_shared("Talkback"));
    assert_eq!(empty.resolve_input("Kick In"), Resolved::Unresolved);
}
