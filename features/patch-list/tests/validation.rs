//! One role, one entry: two entries naming the same role is a
//! validation error unless the profile marks the role shared. A role
//! the profile lacks is unresolved, never an error (decision #29).
//!
//! r[verify flow.patch-list.plan]
//! r[verify flow.patch-list.studio-profiles]

use patch_list::{
    FIXTURE_ALBUM as ALBUM, FIXTURE_STUDIO as ROOM, PatchList, Resolved, StudioProfile,
    ValidationError, validate,
};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

/// Two performers on one DI, for the duplicate-role cases.
const DOUBLE_BOOKED: &str =
    "performers {\n    cody {guitar {di \"DI 3\"}}\n    john {guitar {di \"DI 3\"}}\n}";

#[test]
fn the_fixture_album_is_valid_in_the_fixture_room() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let room = StudioProfile::from_styx(ROOM)?;
    // Two talkback mics name one role; the room shares it.
    let plan = validate(&list, &room)?;
    assert_eq!(plan.entries.len(), 33);
    assert_eq!(plan.buses.len(), 10);
    Ok(())
}

#[test]
fn a_role_the_room_lacks_is_unresolved_in_the_plan_not_an_error() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let room = StudioProfile::from_styx(ROOM)?;
    let plan = validate(&list, &room)?;

    let bass = |key: &str| {
        plan.entries
            .iter()
            .find(|e| e.lowered.performer == "bassist" && e.lowered.key == key)
            .map(|e| e.resolved.clone())
    };
    assert_eq!(bass("amp"), Some(Resolved::Unresolved));
    // The negative control, on the same rig.
    assert_eq!(bass("di"), Some(Resolved::Audio { channel: 0 }));

    let bus = plan
        .buses
        .iter()
        .find(|b| b.name == "bassist")
        .ok_or("the bassist's bus")?;
    assert_eq!(bus.resolved, Resolved::Unresolved);
    assert_eq!(plan.unresolved(), 2);
    Ok(())
}

#[test]
fn a_midi_entry_resolves_by_itself() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    // A room with no Prophet in it: the entry carries its own device.
    let room = StudioProfile::from_styx("shared (\"Talkback\")")?;
    let plan = validate(&list, &room)?;
    let synth = plan
        .entries
        .iter()
        .find(|e| e.lowered.performer == "john" && e.lowered.key == "synth")
        .ok_or("john's synth")?;
    assert_eq!(
        synth.resolved,
        Resolved::Midi {
            device: "Prophet-6".into(),
            channel: Some(1)
        }
    );
    Ok(())
}

#[test]
fn two_entries_on_one_role_is_an_error() -> Result {
    let list = PatchList::from_styx(DOUBLE_BOOKED)?;
    let Err(err) = validate(&list, &StudioProfile::default()) else {
        return Err("a double-booked DI 3 validated".into());
    };
    let ValidationError::DuplicateRole { role, entries } = &err else {
        return Err(format!("wrong refusal: {err}").into());
    };
    assert_eq!(role, "DI 3");
    assert_eq!(entries, &["cody/guitar/di", "john/guitar/di"]);
    assert!(err.to_string().contains("DI 3"), "{err}");
    Ok(())
}

#[test]
fn a_shared_role_may_carry_two_entries() -> Result {
    let list = PatchList::from_styx(DOUBLE_BOOKED)?;
    let room = StudioProfile::from_styx("shared (\"DI 3\")")?;
    validate(&list, &room)?;
    Ok(())
}

#[test]
fn two_buses_on_one_output_is_an_error_too() -> Result {
    let list = PatchList::from_styx(
        "headphones {\n    cody {output \"HP 1\", for (cody)}\n    john {output \"HP 1\", for (john)}\n}",
    )?;
    let Err(ValidationError::DuplicateRole { role, .. }) =
        validate(&list, &StudioProfile::default())
    else {
        return Err("a double-booked HP 1 validated".into());
    };
    assert_eq!(role, "HP 1");

    // The negative control: a bus for two people on one output is one
    // entry, not two.
    let shared =
        PatchList::from_styx("headphones {\n    band {output \"HP 1\", for (cody john)}\n}")?;
    validate(&shared, &StudioProfile::default())?;
    Ok(())
}

#[test]
fn a_midi_channel_outside_one_to_sixteen_is_an_error() -> Result {
    let list = PatchList::from_styx(
        "performers {\n    ron {keys {nord @midi{device \"Nord\", channel 200}}}\n}",
    )?;
    let Err(ValidationError::MidiChannel { entry, channel }) =
        validate(&list, &StudioProfile::default())
    else {
        return Err("MIDI channel 200 validated".into());
    };
    assert_eq!(entry, "ron/keys/nord");
    assert_eq!(channel, 200);

    // The negative control: the same entry on a channel MIDI has.
    let ok = PatchList::from_styx(
        "performers {\n    ron {keys {nord @midi{device \"Nord\", channel 16}}}\n}",
    )?;
    validate(&ok, &StudioProfile::default())?;
    Ok(())
}
