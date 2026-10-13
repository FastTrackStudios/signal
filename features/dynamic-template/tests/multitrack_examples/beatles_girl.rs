use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;
use monarchy::{
    cleanup_display_names, collapse_single_child_folders, expand_items_to_children, monarchy_sort,
    move_items_matching_any_to_group, reapply_collapse,
};

fn beatles_girl_expected() -> daw_proto::TrackHierarchy {
    let kick = TrackGroup::folder("Kick")
        .track("Kick 1")
        .item("boomK__KickRe20_02.wav")
        .track("Kick 2")
        .item("kitA_KickRe20_02.wav")
        .end();

    // Cymbals folder collapsed to OH (single child)
    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .track("Snare")
        .item("Snare DRUMS _01.R.wav")
        .track("Toms")
        .item("boomK__FloorOmni414_02.wav")
        .track("OH")
        .item("OverHead DRUMS_01.wav")
        .track("Drum Kit")
        .item("Drums_TAPE_2.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::folder("Bass")
        .track("Bass 1")
        .item("Bass_TAPE .wav")
        .track("Bass 2")
        .item("BASS.03_01.wav")
        .end();

    // --- Guitars ---
    // "AC GTR 1" has "GTR" → ElectricGuitar → gets "Electric" display name, comes before Acoustic.
    // Sam and Perry are grouped under Acoustic (from J160 and Framus patterns).
    let acoustic = TrackGroup::folder("Acoustic")
        .track("Acoustic")
        .item("Acoustic Guitar_TAPE .wav")
        .track("Perry")
        .item("Framus end track Perry_01.wav")
        .track("Sam 1")
        .item("Girl Sam J160 1_01.wav")
        .track("Sam 2")
        .item("Girl Sam J160 2_01.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric")
        .item("AC GTR 1_01.wav")
        .group(acoustic)
        .end();

    // --- Vocals ---
    // BGVs are organized by performer (moved from Unsorted)
    let chad = TrackGroup::folder("Chad")
        .track("Chad 1")
        .item("Chad1_01.wav")
        .track("Chad 2")
        .item("Chad2_01.wav")
        .end();

    let lou = TrackGroup::folder("Lou")
        .track("Lou 1")
        .item("Lou1_01.wav")
        .track("Lou 2")
        .item("Lou2_01.wav")
        .end();

    let tit = TrackGroup::folder("Tit")
        .track("Tit 1")
        .item("Tit1_01.wav")
        .track("Tit 2")
        .item("Tit2_01.wav")
        .track("Tit 3")
        .item("Tit3_01.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .group(chad)
        .group(lou)
        .group(tit)
        .end();

    // Lead folder is PRESERVED because Lead is a config group that might get siblings (BGVs)
    // Marc items become "Lead", "Lead 2" under the Lead folder
    let lead = TrackGroup::folder("Lead")
        .track("Lead")
        .item("Marc VOX.wav")
        .track("Lead 2")
        .item("Marc VOX 2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // --- SFX ---
    // boomK__BottleOver matches "boom" pattern in SFX group
    let sfx = TrackGroup::single_track("SFX", "boomK__BottleOver_02.wav");

    // --- Reference ---
    // GIRL_MDN matches "MDN" (mixdown) pattern in Reference group
    let reference = TrackGroup::single_track("Reference", "GIRL_MDN.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .group(sfx)
        .group(reference)
        .build();
    expected
}

#[test]
fn beatles_girl() {
    // -- Setup & Fixtures
    // This song features Chad, Lou, Tit doing BGVs and Sam, Perry on acoustic guitars
    let items = vec![
        "AC GTR 1_01.wav",
        "Acoustic Guitar_TAPE .wav",
        "Bass_TAPE .wav",
        "BASS.03_01.wav",
        "boomK__BottleOver_02.wav",
        "boomK__FloorOmni414_02.wav",
        "boomK__KickRe20_02.wav",
        "Chad1_01.wav",
        "Chad2_01.wav",
        "Drums_TAPE_2.wav",
        "Framus end track Perry_01.wav",
        "Girl Sam J160 1_01.wav",
        "Girl Sam J160 2_01.wav",
        "GIRL_MDN.wav",
        "kitA_KickRe20_02.wav",
        "Lou1_01.wav",
        "Lou2_01.wav",
        "Marc VOX 2.wav",
        "Marc VOX.wav",
        "OverHead DRUMS_01.wav",
        "Snare DRUMS _01.R.wav",
        "Tit1_01.wav",
        "Tit2_01.wav",
        "Tit3_01.wav",
    ];
    let config = default_config();

    // -- Exec
    // Step 1: Initial sort using monarchy
    let mut structure = monarchy_sort(items, &config).unwrap();

    // Step 2: Move all BGV performer tracks from Unsorted to BGVs in one operation
    // Moving all at once allows the organizer to see multiple performers and create subfolders
    let _bgv_sorted = move_items_matching_any_to_group(
        &mut structure,
        &config,
        &["Chad", "Lou", "Tit"], // All BGV performers at once
        &["Unsorted"],
        "BGVs",
        &["Vocals"],
    )
    .unwrap();

    // Step 3: Re-apply collapse to clean up hierarchy
    reapply_collapse(&mut structure, &config);

    // Step 4: Expand items to individual child tracks
    expand_items_to_children(&mut structure);

    // Step 5: Collapse single-child folders (e.g., Cymbals/OH → OH)
    collapse_single_child_folders(&mut structure);

    // Step 6: Cleanup display names
    cleanup_display_names(&mut structure);

    // Convert to tracks
    let tracks = structure.clone().to_tracks();

    // -- Check
    println!("\n=== BEATLES GIRL - Final Track Structure ===");
    daw_proto::display_tracklist(&tracks);
    println!();

    // ============================================================================
    // Define expected structure
    // ============================================================================

    // --- Drums ---
    let expected = beatles_girl_expected();

    // Full structure assertion
    assert_tracks_equal(&tracks, &expected).unwrap();
}
