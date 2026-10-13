//! On load the list lowers to flat `(selector, entry)` pairs in the
//! scene engine's own `Selector` vocabulary (decision #29: "nothing new
//! is invented for matching").
//!
//! r[verify flow.patch-list.plan]

use dynamic_template::scenes::Selector;
use patch_list::{Entry, FIXTURE_ALBUM as ALBUM, Lowered, PatchList};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn find<'a>(lowered: &'a [Lowered], performer: &str, key: &str) -> Option<&'a Lowered> {
    lowered
        .iter()
        .find(|e| e.performer == performer && e.key == key)
}

#[test]
fn every_entry_lowers_to_one_selector_in_file_order() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let lowered = list.entries();
    // 6 + 4 + 1 + 1 + 1 + 1 + 1 + 14 + 2 + 1 + 1 rig entries in the fixture.
    assert_eq!(lowered.len(), 33);

    let first = lowered.first().ok_or("the first entry")?;
    assert_eq!(first.performer, "cody");
    assert_eq!(first.kind, "guitar");
    assert_eq!(first.key, "di");
    assert_eq!(first.entry, Entry::Role("DI 3".into()));
    assert_eq!(
        first.selector,
        Selector {
            performer: Some("cody".into()),
            kind: Some("guitar".into()),
            multi_mic: Some("di".into()),
            ..Selector::default()
        }
    );
    Ok(())
}

#[test]
fn a_slash_in_the_key_is_a_group_path_over_the_mic() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let lowered = list.entries();

    let amp = find(&lowered, "cody", "amp-a/57").ok_or("cody's amp A 57")?;
    assert_eq!(amp.selector.group, ["amp-a"]);
    assert_eq!(amp.selector.multi_mic.as_deref(), Some("57"));
    assert_eq!(amp.entry, Entry::Role("Mic 5".into()));

    let kick = find(&lowered, "drummer", "kick/in").ok_or("the kick in mic")?;
    assert_eq!(kick.selector.performer.as_deref(), Some("drummer"));
    assert_eq!(kick.selector.kind.as_deref(), Some("drums"));
    assert_eq!(kick.selector.group, ["kick"]);
    assert_eq!(kick.selector.multi_mic.as_deref(), Some("in"));
    Ok(())
}

#[test]
fn a_midi_entry_lowers_like_any_other() -> Result {
    let list = PatchList::from_styx(ALBUM)?;
    let lowered = list.entries();
    let synth = find(&lowered, "john", "synth").ok_or("john's synth")?;
    assert_eq!(synth.kind, "keys");
    assert_eq!(synth.selector.multi_mic.as_deref(), Some("synth"));
    assert!(matches!(synth.entry, Entry::Midi { .. }));
    Ok(())
}

#[test]
fn a_key_that_names_a_channel_lowers_to_the_channel_dimension() -> Result {
    // A double-tracked lead: the guitar's two halves are the Channel
    // dimension, not two mics of one source (decision #29 — a rig may
    // be keyed by channel).
    let list = PatchList::from_styx(
        "performers {\n    john {guitar {lead/l \"DI 7\", lead/r \"DI 8\"}}\n}",
    )?;
    let lowered = list.entries();
    let left = find(&lowered, "john", "lead/l").ok_or("the left channel")?;
    assert_eq!(left.selector.group, ["lead"]);
    assert_eq!(left.selector.channel.as_deref(), Some("l"));
    assert_eq!(left.selector.multi_mic, None);

    // The negative control: a mic key on the same rig stays a mic.
    let mics = PatchList::from_styx("performers {\n    john {guitar {lead/di \"DI 7\"}}\n}")?;
    let di = mics.entries().into_iter().next().ok_or("the DI")?;
    assert_eq!(di.selector.multi_mic.as_deref(), Some("di"));
    assert_eq!(di.selector.channel, None);
    Ok(())
}

#[test]
fn the_role_of_an_entry_is_only_a_role() {
    assert_eq!(Entry::Role("DI 3".into()).role(), Some("DI 3"));
    assert_eq!(
        Entry::Midi {
            device: "Nord".into(),
            channel: None
        }
        .role(),
        None
    );
}
