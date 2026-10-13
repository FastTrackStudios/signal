use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn single_item_creates_track_at_deepest_group_level() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec!["Kick In"];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Philosophy: only create folders when needed to organize multiple things
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .track("Kick")
        .item("Kick In")
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}

#[test]
fn multiple_items_of_same_subgroup_create_folder_with_subtracks() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec!["Kick In", "Kick Out"];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Kick (folder)
    // - In: [Kick In]
    // - Out: [Kick Out]
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .folder("Kick")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}

#[test]
fn multiple_subgroups_create_parent_folder_with_nested_structure() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec!["Kick In", "Kick Out", "Snare Top"];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Drums
    // -Kick
    //   --In [Kick In]
    //   --Out [Kick Out]
    // -Snare [Snare Top]
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .track("Snare")
        .item("Snare Top")
        .end()
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}

#[test]
fn items_in_nested_groups_create_nested_track_structure() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec!["Kick In", "Kick Out", "Snare Top", "Snare Bottom"];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Drums
    // -Kick
    //   --In [Kick In]
    //   --Out [Kick Out]
    // -Snare
    //   --Top [Snare Top]
    //   --Bottom [Snare Bottom]
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .folder("Snare")
        .track("Top")
        .item("Snare Top")
        .track("Bottom")
        .item("Snare Bottom")
        .end()
        .end()
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}

#[test]
fn group_separator_only_created_when_multiple_sibling_groups_exist() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec![
        "Kick In",
        "Kick Out",
        "Snare Top",
        "Snare Bottom",
        "808 Kick",
        "Electronic Snare",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Drums
    // -Drum Kit
    //   --Kick
    //     ---In [Kick In]
    //     ---Out [Kick Out]
    //   --Snare
    //     ---Top [Snare Top]
    //     ---Bottom [Snare Bottom]
    // -Electronic Kit
    //   --Kick [808 Kick]
    //   --Snare [Electronic Snare]
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Drum Kit")
        .folder("Kick")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .folder("Snare")
        .track("Top")
        .item("Snare Top")
        .track("Bottom")
        .item("Snare Bottom")
        .end()
        .end()
        .folder("Electronic Kit")
        .track("Kick")
        .item("808 Kick")
        .track("Snare")
        .item("Electronic Snare")
        .end()
        .end()
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}

#[test]
fn multiple_top_level_groups_create_separate_sections() -> Result<()> {
    // -- Setup & Fixtures
    let items = vec![
        "Kick In",
        "Kick Out",
        "Snare Top",
        "Snare Bottom",
        "808 Kick",
        "Electronic Snare",
        "Bass Guitar",
        "Bass Synth",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None)?;

    // -- Check
    // Drums
    // -Drum Kit
    //   --Kick
    //     ---In [Kick In]
    //     ---Out [Kick Out]
    //   --Snare
    //     ---Top [Snare Top]
    //     ---Bottom [Snare Bottom]
    // -Electronic Kit
    //   --Kick [808 Kick]
    //   --Snare [Electronic Snare]
    // Bass
    // -Guitar [Bass Guitar]
    // -Synth [Bass Synth]
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Drum Kit")
        .folder("Kick")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .folder("Snare")
        .track("Top")
        .item("Snare Top")
        .track("Bottom")
        .item("Snare Bottom")
        .end()
        .end()
        .folder("Electronic Kit")
        .track("Kick")
        .item("808 Kick")
        .track("Snare")
        .item("Electronic Snare")
        .end()
        .end()
        .folder("Bass")
        .track("Guitar")
        .item("Bass Guitar")
        .track("Synth")
        .item("Bass Synth")
        .end()
        .build();

    assert_tracks_equal(&tracks, &expected)?;

    Ok(())
}
