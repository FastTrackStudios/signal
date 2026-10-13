//! The session override layers onto the album by key: it replaces an
//! entry that is there, it cannot delete one, and removing the
//! override (`over: None`) returns the session to the list exactly
//! (decision #29, `flow.patch-list.session-override`).
//!
//! r[verify flow.patch-list.session-override]

use patch_list::{Entry, PatchList, layer};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const ALBUM: &str = "performers {\n    cody {guitar {di \"DI 3\", pedalboard \"DI 4\"}}\n}\nheadphones {\n    cody {output \"HP 1\", for (cody)}\n}";

#[test]
fn with_no_override_the_session_is_exactly_the_album() -> Result {
    let album = PatchList::from_styx(ALBUM)?;
    let layered = layer(&album, None);
    assert_eq!(layered.list, album);
    assert!(layered.overridden_entries.is_empty());
    assert!(layered.overridden_buses.is_empty());
    Ok(())
}

#[test]
fn an_override_replaces_one_entry_by_key_and_is_marked() -> Result {
    let album = PatchList::from_styx(ALBUM)?;
    let over = PatchList::from_styx("performers {\n    cody {guitar {di \"DI 9\"}}\n}")?;

    let layered = layer(&album, Some(&over));

    let cody = layered.list.performers.get("cody").ok_or("cody")?;
    let guitar = cody.get("guitar").ok_or("cody's guitar rig")?;
    // Replaced.
    assert_eq!(guitar.get("di"), Some(&Entry::Role("DI 9".into())));
    // Untouched — the override cannot delete what it does not name.
    assert_eq!(guitar.get("pedalboard"), Some(&Entry::Role("DI 4".into())));

    assert!(layered.overridden_entries.contains(&(
        "cody".to_owned(),
        "guitar".to_owned(),
        "di".to_owned()
    )));
    assert!(!layered.overridden_entries.contains(&(
        "cody".to_owned(),
        "guitar".to_owned(),
        "pedalboard".to_owned()
    )));
    Ok(())
}

#[test]
fn an_override_can_add_a_bus_it_does_not_replace_anything() -> Result {
    let album = PatchList::from_styx(ALBUM)?;
    let over = PatchList::from_styx(
        "headphones {\n    engineer {output \"HP Engineer\", for (engineer)}\n}",
    )?;

    let layered = layer(&album, Some(&over));
    assert_eq!(
        layered
            .list
            .headphones
            .get("cody")
            .map(|b| b.output.clone()),
        Some("HP 1".into())
    );
    assert_eq!(
        layered
            .list
            .headphones
            .get("engineer")
            .map(|b| b.output.clone()),
        Some("HP Engineer".into())
    );
    assert!(layered.overridden_buses.contains("engineer"));
    assert!(!layered.overridden_buses.contains("cody"));
    Ok(())
}

#[test]
fn removing_the_override_returns_the_session_to_the_list() -> Result {
    // The negative control on the whole round trip: layering `Some`
    // then dropping back to `None` is not a no-op by accident — it is
    // the album, unchanged, because the override was never stored in
    // the album's own document.
    let album = PatchList::from_styx(ALBUM)?;
    let over = PatchList::from_styx("performers {\n    cody {guitar {di \"DI 9\"}}\n}")?;

    let overridden = layer(&album, Some(&over));
    assert_ne!(overridden.list, album);

    let restored = layer(&album, None);
    assert_eq!(restored.list, album);
    Ok(())
}
