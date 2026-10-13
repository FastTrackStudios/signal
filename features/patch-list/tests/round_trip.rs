//! The patch list round-trips through styx (decision #29: "styx from
//! day one, Facet types, a schema, a round-trip test").
//!
//! r[verify flow.patch-list.plan]
//! r[verify flow.patch-list.project-level]

use patch_list::{Entry, FIXTURE_ALBUM as ALBUM, PatchList};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn the_fixture_album_parses_with_its_shape_intact() -> Result {
    let list = PatchList::from_styx(ALBUM)?;

    // Performers in the order the file lists them — the view and the
    // performer rows sort by it.
    let performers: Vec<&str> = list.performers.keys().map(String::as_str).collect();
    assert_eq!(
        performers,
        [
            "cody", "john", "ron", "belen", "aline", "drummer", "bassist", "engineer", "producer"
        ]
    );

    // performer → source kind → channel or mic → input role.
    let cody = list.performers.get("cody").ok_or("cody")?;
    let guitar = cody.get("guitar").ok_or("cody's guitar rig")?;
    assert_eq!(guitar.get("di"), Some(&Entry::Role("DI 3".into())));
    assert_eq!(guitar.get("amp-a/57"), Some(&Entry::Role("Mic 5".into())));

    // A MIDI source is an entry giving the device and channel.
    let john_keys = list
        .performers
        .get("john")
        .and_then(|rigs| rigs.get("keys"))
        .ok_or("john's keys")?;
    assert_eq!(
        john_keys.get("synth"),
        Some(&Entry::Midi {
            device: "Prophet-6".into(),
            channel: Some(1),
        })
    );

    // The kit: the drummer's rig of kind drums, keyed piece/mic.
    let kit = list
        .performers
        .get("drummer")
        .and_then(|rigs| rigs.get("drums"))
        .ok_or("the kit")?;
    assert_eq!(kit.len(), 14);
    assert_eq!(kit.get("snare/top"), Some(&Entry::Role("Snare Top".into())));

    // Headphone buses, with who each is for.
    let cody_bus = list.headphones.get("cody").ok_or("cody's bus")?;
    assert_eq!(cody_bus.output, "HP 1");
    assert_eq!(cody_bus.audience, ["cody"]);
    assert_eq!(
        list.headphones.get("broadcast").map(|b| b.output.as_str()),
        Some("Broadcast")
    );
    assert_eq!(list.headphones.len(), 10);
    Ok(())
}

#[test]
fn the_fixture_album_round_trips() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let text = list.to_styx()?;
    let back = PatchList::from_styx(&text)?;
    assert_eq!(back, list, "written text was:\n{text}");
    Ok(())
}

#[test]
fn a_bus_for_several_people_is_one_bus() -> Result {
    let text = "headphones {\n    choir {output \"HP 8\", for (soprano-1 soprano-2 alto)}\n}";
    let list = PatchList::from_styx(text)?;
    let choir = list.headphones.get("choir").ok_or("the choir's bus")?;
    assert_eq!(choir.audience, ["soprano-1", "soprano-2", "alto"]);
    assert!(list.performers.is_empty());
    Ok(())
}

#[test]
fn a_midi_entry_without_a_channel_means_every_channel() -> Result {
    let text = "performers {\n    ron {keys {nord @midi{device \"Nord Stage 3\"}}}\n}";
    let list = PatchList::from_styx(text)?;
    let nord = list
        .performers
        .get("ron")
        .and_then(|rigs| rigs.get("keys"))
        .and_then(|rig| rig.get("nord"))
        .ok_or("ron's nord")?;
    assert_eq!(
        nord,
        &Entry::Midi {
            device: "Nord Stage 3".into(),
            channel: None,
        }
    );
    Ok(())
}

#[test]
fn a_document_that_is_not_a_patch_list_is_an_error() -> Result {
    let Err(err) = PatchList::from_styx("performers (a b c)") else {
        return Err("a sequence parsed as a map of performers".into());
    };
    assert!(!err.to_string().is_empty());
    Ok(())
}
