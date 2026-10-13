use daw_proto::{assert_tracks_equal, TrackGroup, TrackNode, TrackStructureBuilder};
use dynamic_template::*;
use monarchy::{
    cleanup_display_names, expand_items_to_children, monarchy_sort, move_unsorted_to_group,
    reapply_collapse,
};

/// Count all items in a flat track list
#[allow(dead_code)]
fn count_items(tracks: &[TrackNode]) -> usize {
    tracks.iter().map(|t| t.items.len()).sum()
}

#[test]
fn radiohead_paranoid_android() {
    // -- Setup & Fixtures
    let items = vec![
        "1 Kick In.05_04.wav",
        "10 Rack 10-St.01.R.05_03.wav",
        "11  Rack 12.01.R.05_03.wav",
        "12 Floor.01.R.05_03.wav",
        "13 Knee Mic.01.R.05_03.wav",
        "14 Mono Ovh.01.R.05_03.wav",
        "15 Room HH.05_03.wav",
        "19 Cabasa_03.wav",
        "20 Clave_03.wav",
        "21 Guiro Shaker_03.wav",
        "22 Guiro_03.wav",
        "23 Vibraslap_03.wav",
        "24 SHAKER_03.wav",
        "25 _Bass.01_03.wav",
        "26 Acc Guitar_03.wav",
        "28 EdCrunch_03.wav",
        "29 JohnyCrunch1_03.wav",
        "3 Kick Out.05_04.wav",
        "30 EdCrunch2_03.wav",
        "31 JohnyCrunch2_03.wav",
        "33 EdPitch_03.wav",
        "34 JohnyLead_03.wav",
        "35 JohnyPhaser1_03.wav",
        "36 JohnyPhaser1.dup1_03.wav",
        "39 JohnyPhaser2_03.wav",
        "40 CK MAP]_03.wav",
        "42 intro count.1_03.wav",
        "43 Robot Voice_03.wav",
        "44 Bells.2_03.wav",
        "45 DX7 .2_03.wav",
        "46 FX1.2_03.wav",
        "47 FX2.2_03.wav",
        "48 Mellotron.2_03.wav",
        "49 Organ Chords.2_03.wav",
        "5 Snare.05_03.wav",
        "50 Organ Notes.2_03.wav",
        "51 Organ Slide.2_03.wav",
        "52 Piano_03.wav",
        "53 rhodes_david bennett_03.wav",
        "54 prophet synth_david bennett_03.wav",
        "56 Lead Voc_03.wav",
        "57 Lead Voc Dbl_03.wav",
        "58 Lead Voc Dbl.dup1_03.wav",
        "59 Vocal 3_03.wav",
        "6 Snare.dup1.05_03.wav",
        "60 lead vox quad_03.wav",
        "61 Bridge vocal extra_03.wav",
        "62 Outro vocal 1_03.wav",
        "63 Outro vocal 2_03.wav",
        "64 Outro vocal 3_03.wav",
        "65 extra vocal2_03.wav",
        "66 extra vocal3_03.wav",
        "67 Voca Middle Bridge1_03.wav",
        "68 Voca Middle Bridge2_03.wav",
        "69 Voca Middle Bridge3_03.wav",
        "70 Voca Middle Bridge4_03.wav",
        "71 Voca Middle Bridge5_03.wav",
        "8 Snare Btm.05_03.wav",
        "9 SNR VERB.05_03.wav",
        "Paranoid_Android_Cover_PLP_JH_MIX_1_Master.wav",
    ];

    let config = default_config();

    // -- Exec
    let mut structure = monarchy_sort(items, &config).unwrap();

    println!("\n=== STEP 1: Initial monarchy_sort ===");
    println!("{structure}");

    // Move Ed/Johny guitar tracks from Unsorted to Electric Guitar
    println!("\n=== STEP 2: Moving guitar tracks from Unsorted to Electric Guitar ===");
    let guitar_sorted = move_unsorted_to_group(
        &mut structure,
        &config,
        &["Unsorted"],
        "Electric",
        &["Guitars"],
    )
    .unwrap();
    println!("Sorted {guitar_sorted} guitar items");

    // Note: FX1/FX2 now automatically go to SFX group (no manual step needed)

    // Step 3: Re-apply collapse to clean up hierarchy
    reapply_collapse(&mut structure, &config);

    // Step 4: Expand items to individual child tracks
    // This ensures each item becomes its own track (one item per track rule)
    expand_items_to_children(&mut structure);

    // Step 5: Cleanup display names by stripping redundant context
    // This removes parent folder names from child track names and numbers duplicates
    cleanup_display_names(&mut structure);

    println!("\n=== STEP 5: Final structure after cleanup_display_names ===");
    println!("{structure}");

    // Convert to tracks for display
    let tracks = structure.clone().to_tracks();

    println!("\n=== Final Track list ===");
    daw_proto::display_tracklist(&tracks);

    let expected = radiohead_paranoid_android_expected();

    // -- Check
    assert_tracks_equal(&tracks, &expected).unwrap();
}

fn radiohead_paranoid_android_expected_a() -> (TrackGroup, TrackGroup) {
    // --- Drums ---
    let kick = TrackGroup::folder("Kick")
        .track("In")
        .item("1 Kick In.05_04.wav")
        .track("Out")
        .item("3 Kick Out.05_04.wav")
        .end();

    // Snare 1, Snare 2 stay flat within SUM (no redundant Snare subfolder
    // since "Snare" is already an ancestor in the context stack)
    let snare_sum = TrackGroup::folder("SUM")
        .track("Bottom")
        .item("8 Snare Btm.05_03.wav")
        .track("Snare 1")
        .item("5 Snare.05_03.wav")
        .track("Snare 2")
        .item("6 Snare.dup1.05_03.wav")
        .end();

    let snare = TrackGroup::folder("Snare")
        .group(snare_sum)
        .track("Verb")
        .item("9 SNR VERB.05_03.wav")
        .end();

    let toms = TrackGroup::folder("Toms")
        .track("Floor")
        .item("12 Floor.01.R.05_03.wav")
        .track(r#"Rack 10""#)
        .item("10 Rack 10-St.01.R.05_03.wav")
        .track(r#"Rack 12""#)
        .item("11  Rack 12.01.R.05_03.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("14 Mono Ovh.01.R.05_03.wav")
        .track("Hi Hat")
        .item("15 Room HH.05_03.wav")
        .end();

    let rooms = TrackGroup::single_track("Rooms", "13 Knee Mic.01.R.05_03.wav");

    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms)
        .group(cymbals)
        .group(rooms)
        .end();

    // --- Percussion ---
    // "Guiro Shaker" extracts arrangement="Shaker"
    // "Guiro" defaults to arrangement="Main", but since there are no layer siblings,
    // "Main" is replaced with parent name "Guiro" for clarity
    let guiro = TrackGroup::folder("Guiro")
        .track("Guiro")
        .item("22 Guiro_03.wav")
        .track("Shaker")
        .item("21 Guiro Shaker_03.wav")
        .end();

    let percussion = TrackGroup::folder("Percussion")
        .track("Shaker")
        .item("24 SHAKER_03.wav")
        .track("Cabasa")
        .item("19 Cabasa_03.wav")
        .group(guiro)
        .track("Clave")
        .item("20 Clave_03.wav")
        .track("Vibraslap")
        .item("23 Vibraslap_03.wav")
        .end();
    (drums, percussion)
}

fn radiohead_paranoid_android_expected_b() -> (TrackGroup, TrackGroup) {
    // --- Guitars ---
    let ed_crunch = TrackGroup::folder("Crunch")
        .track("Crunch 1")
        .item("28 EdCrunch_03.wav")
        .track("Crunch 2")
        .item("30 EdCrunch2_03.wav")
        .end();

    let ed = TrackGroup::folder("Ed")
        .group(ed_crunch)
        .track("Pitch")
        .item("33 EdPitch_03.wav")
        .end();

    let johny_crunch = TrackGroup::folder("Crunch")
        .track("Crunch 1")
        .item("29 JohnyCrunch1_03.wav")
        .track("Crunch 2")
        .item("31 JohnyCrunch2_03.wav")
        .end();

    let johny_phaser = TrackGroup::folder("Phaser")
        .track("Phaser 1")
        .item("35 JohnyPhaser1_03.wav")
        .track("Phaser 2")
        .item("36 JohnyPhaser1.dup1_03.wav")
        .track("Phaser 3")
        .item("39 JohnyPhaser2_03.wav")
        .end();

    let johny = TrackGroup::folder("Johny")
        .group(johny_crunch)
        .track("Lead")
        .item("34 JohnyLead_03.wav")
        .group(johny_phaser)
        .end();

    let electric = TrackGroup::folder("Electric").group(ed).group(johny).end();

    let acc_guitar = TrackGroup::single_track("Acoustic", "26 Acc Guitar_03.wav");

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .group(acc_guitar)
        .end();

    // --- Keys ---
    let electric_keys = TrackGroup::folder("Electric")
        .track("Rhodes")
        .item("53 rhodes_david bennett_03.wav")
        .track("DX7")
        .item("45 DX7 .2_03.wav")
        .track("Mellotron")
        .item("48 Mellotron.2_03.wav")
        .end();

    // "Organ X" extracts arrangement values: Chords, Notes, Slide (title case)
    let organ = TrackGroup::folder("Organ")
        .track("Chords")
        .item("49 Organ Chords.2_03.wav")
        .track("Notes")
        .item("50 Organ Notes.2_03.wav")
        .track("Slide")
        .item("51 Organ Slide.2_03.wav")
        .end();

    let keys = TrackGroup::folder("Keys")
        .track("Piano")
        .item("52 Piano_03.wav")
        .group(electric_keys)
        .group(organ)
        .end();
    (guitars, keys)
}

fn radiohead_paranoid_android_expected_c() -> (TrackGroup, TrackGroup) {
    // --- Synths ---
    let synths = TrackGroup::folder("Synths")
        .track("Prophet")
        .item("54 prophet synth_david bennett_03.wav")
        .track("Bells")
        .item("44 Bells.2_03.wav")
        .end();

    // --- Vocals ---
    // Middle Bridge is a section - tracks match Lead via "voca" pattern
    // Creates a folder since there are multiple numbered tracks
    // Child tracks use "Lead 1-5" since parent is "Middle Bridge"
    let middle_bridge = TrackGroup::folder("Middle Bridge")
        .track("Lead 1")
        .item("67 Voca Middle Bridge1_03.wav")
        .track("Lead 2")
        .item("68 Voca Middle Bridge2_03.wav")
        .track("Lead 3")
        .item("69 Voca Middle Bridge3_03.wav")
        .track("Lead 4")
        .item("70 Voca Middle Bridge4_03.wav")
        .track("Lead 5")
        .item("71 Voca Middle Bridge5_03.wav")
        .end();

    let vocal_outro = TrackGroup::folder("Outro")
        .track("Outro 1")
        .item("62 Outro vocal 1_03.wav")
        .track("Outro 2")
        .item("63 Outro vocal 2_03.wav")
        .track("Outro 3")
        .item("64 Outro vocal 3_03.wav")
        .end();

    // Main layer tracks (no explicit layer = Main default)
    // Items get "Lead 1", "Lead 2", "Lead 3" as they match Lead group
    let vocal_main = TrackGroup::folder("Main")
        .track("Lead 1")
        .item("56 Lead Voc_03.wav")
        .track("Lead 2")
        .item("65 extra vocal2_03.wav")
        .track("Lead 3")
        .item("66 extra vocal3_03.wav")
        .end();

    // DBL (double) layer tracks
    // Items get "Lead 1", "Lead 2" since parent is DBL
    let vocal_dbl = TrackGroup::folder("DBL")
        .track("Lead 1")
        .item("57 Lead Voc Dbl_03.wav")
        .track("Lead 2")
        .item("58 Lead Voc Dbl.dup1_03.wav")
        .end();

    // "Quad" is a layer (like Main, DBL), extracted from "lead vox quad"
    let vocal_quad = TrackGroup::single_track("Quad", "60 lead vox quad_03.wav");

    // Lead vocals - Lead collapses into Vocals when it's the only vocal type
    // Bridge is a single track (section), Middle Bridge is a sibling folder
    let lead = TrackGroup::folder("Vocals")
        .track("Bridge")
        .item("61 Bridge vocal extra_03.wav")
        .group(middle_bridge)
        .group(vocal_outro)
        .group(vocal_main)
        .group(vocal_quad)
        .track("Vocals 3")
        .item("59 Vocal 3_03.wav")
        .group(vocal_dbl)
        .end();
    (synths, lead)
}

fn radiohead_paranoid_android_expected_d() -> (TrackGroup, TrackGroup, TrackGroup, TrackGroup) {
    // --- SFX ---
    let sfx = TrackGroup::folder("SFX")
        .track("Robot Voice")
        .item("43 Robot Voice_03.wav")
        .track("FX1")
        .item("46 FX1.2_03.wav")
        .track("FX2")
        .item("47 FX2.2_03.wav")
        .end();

    // --- Guide ---
    // "intro count" matches Count subgroup
    let guide = TrackGroup::single_track("Guide", "42 intro count.1_03.wav");

    // --- Reference ---
    // "Master" in filename matches Reference group
    let reference = TrackGroup::single_track(
        "Reference",
        "Paranoid_Android_Cover_PLP_JH_MIX_1_Master.wav",
    );

    // --- Unsorted ---
    let unsorted = TrackGroup::single_track("Unsorted", "40 CK MAP]_03.wav");
    (sfx, guide, reference, unsorted)
}

fn radiohead_paranoid_android_expected() -> daw_proto::TrackHierarchy {
    let (drums, percussion) = radiohead_paranoid_android_expected_a();
    let (guitars, keys) = radiohead_paranoid_android_expected_b();
    let (synths, lead) = radiohead_paranoid_android_expected_c();
    let (sfx, guide, reference, unsorted) = radiohead_paranoid_android_expected_d();
    TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .track("Bass")
        .item("25 _Bass.01_03.wav")
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(lead) // Vocals is transparent, so Lead appears directly
        .group(sfx)
        .group(guide)
        .group(reference)
        .group(unsorted)
        .build()
}

// ============================================================================
// EXPECTED TRACK HIERARCHY (documented version)
// ============================================================================
//
// Drums/
//   ├─ Kick/
//   │   ├─ In             ← 1 Kick In.05_04.wav
//   │   └─ Out            ← 3 Kick Out.05_04.wav
//   ├─ Snare/
//   │   ├─ Sum/                           (tagged collection for mic positions)
//   │   │   ├─ Top        ← 5 Snare.05_03.wav (default = Top when Bottom exists)
//   │   │   ├─ Top 2      ← 6 Snare.dup1.05_03.wav
//   │   │   └─ Bottom     ← 8 Snare Btm.05_03.wav
//   │   └─ Verb           ← 9 SNR VERB.05_03.wav (FX print, sibling to Sum)
//   ├─ Toms/
//   │   ├─ Rack 10        ← 10 Rack 10-St.01.R.05_03.wav (10" rack tom)
//   │   ├─ Rack 12        ← 11 Rack 12.01.R.05_03.wav (12" rack tom)
//   │   └─ Floor          ← 12 Floor.01.R.05_03.wav
//   ├─ Cymbals/
//   │   ├─ OH Mono        ← 14 Mono Ovh.01.R.05_03.wav (OVH → Cymbals)
//   │   └─ HH             ← 15 Room HH.05_03.wav (HH → Cymbals, despite "Room" prefix)
//   └─ Rooms/
//       └─ Knee Mic       ← 13 Knee Mic.01.R.05_03.wav
//
// Percussion/
//   ├─ Cabasa             ← 19 Cabasa_03.wav
//   ├─ Clave              ← 20 Clave_03.wav
//   ├─ Guiro Shaker       ← 21 Guiro Shaker_03.wav
//   ├─ Guiro              ← 22 Guiro_03.wav
//   ├─ Vibraslap          ← 23 Vibraslap_03.wav
//   └─ Shaker             ← 24 SHAKER_03.wav
//
// Bass                     ← 25 _Bass.01_03.wav (single item, no subfolder needed)
//
// Guitars/
//   └─ Electric/
//       ├─ Ed/
//       │   ├─ Crunch     ← 28 EdCrunch_03.wav
//       │   ├─ Crunch 2   ← 30 EdCrunch2_03.wav
//       │   └─ Pitch      ← 33 EdPitch_03.wav
//       └─ Johny/
//           ├─ Crunch 1   ← 29 JohnyCrunch1_03.wav
//           ├─ Crunch 2   ← 31 JohnyCrunch2_03.wav
//           ├─ Lead       ← 34 JohnyLead_03.wav
//           ├─ Phaser 1   ← 35 JohnyPhaser1_03.wav
//           ├─ Phaser 1 2 ← 36 JohnyPhaser1.dup1_03.wav (dup = layer 2)
//           └─ Phaser 2   ← 39 JohnyPhaser2_03.wav
//
// Keys/
//   ├─ Piano              ← 52 Piano_03.wav
//   ├─ Electric/
//   │   ├─ Rhodes         ← 53 rhodes_david bennett_03.wav
//   │   ├─ DX7            ← 45 DX7 .2_03.wav
//   │   └─ Mellotron      ← 48 Mellotron.2_03.wav
//   └─ Organ/
//       ├─ Chords         ← 49 Organ Chords.2_03.wav
//       ├─ Notes          ← 50 Organ Notes.2_03.wav
//       └─ Slide          ← 51 Organ Slide.2_03.wav
//
// Synths/
//   └─ Prophet            ← 54 prophet synth_david bennett_03.wav
//
// Vocals/
//   ├─ Lead/
//   │   ├─ Bridge         ← 61 Bridge vocal extra_03.wav
//   │   ├─ Outro/
//   │   │   ├─ 1          ← 62 Outro vocal 1_03.wav
//   │   │   ├─ 2          ← 63 Outro vocal 2_03.wav
//   │   │   └─ 3          ← 64 Outro vocal 3_03.wav
//   │   ├─ Main/
//   │   │   ├─ Voc        ← 56 Lead Voc_03.wav
//   │   │   ├─ Dbl        ← 57 Lead Voc Dbl_03.wav
//   │   │   └─ Dbl 2      ← 58 Lead Voc Dbl.dup1_03.wav
//   │   ├─ 3              ← 59 Vocal 3_03.wav
//   │   └─ DBL/
//   │       ├─ Voc 1      ← 57 Lead Voc Dbl_03.wav
//   │       └─ Voc 2      ← 58 Lead Voc Dbl.dup1_03.wav
//   ├─ Bridge 1           ← 67 Voca Middle Bridge1_03.wav
//   ├─ Bridge 2           ← 68 Voca Middle Bridge2_03.wav
//   ├─ Bridge 3           ← 69 Voca Middle Bridge3_03.wav
//   ├─ Bridge 4           ← 70 Voca Middle Bridge4_03.wav
//   └─ Bridge 5           ← 71 Voca Middle Bridge5_03.wav
//
// SFX/
//   ├─ FX1                ← 46 FX1.2_03.wav
//   ├─ FX2                ← 47 FX2.2_03.wav
//   ├─ Robot Voice        ← 43 Robot Voice_03.wav
//   └─ Intro Count        ← 42 intro count.1_03.wav
//
// Unsorted/
//   ├─ CK MAP             ← 40 CK MAP]_03.wav (likely click/map track)
//   └─ Mix Master         ← Paranoid_Android_Cover_PLP_JH_MIX_1_Master.wav
//
// ============================================================================
// NOTES:
// - Track numbers (1-71) are original session order, not sorting priority
// - "Ed" and "Johny" are guitarists (Ed O'Brien, Jonny Greenwood)
// - ".dup1" suffix indicates duplicate/layer 2 track
// - ".2_03" and ".05_03" suffixes are likely Pro Tools region identifiers
// - Snare default is "Top" (not "Main") when Bottom mic exists
// - Snare mic positions go in Sum folder, FX prints (Verb) are siblings to Sum
// - OVH/Overheads and HH/Hi-Hat → Cymbals bus (not Rooms)
// - Rack tom numbers indicate size (10", 12")
// - DX7 is a Yamaha FM synthesizer → Electric Keys
// - Mellotron is a tape-replay keyboard → Electric Keys
// - Prophet is a Sequential Circuits synthesizer
// - Vocals organized by Section first (Main, Bridge, Outro)
// - Within Main section: "Lead" is an arrangement with layers (Main, Dbl, Dbl 2)
// - Other vocals in Main section (Vocal 3, Quad, Extra) are separate parts
// - PLP = Play Along Print, JH = initials (mixer?)
// ============================================================================
