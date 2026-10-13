use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn tears_for_fears_shout_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("SUM")
        .item("01.Kick.In.One_01-02.wav")
        .track("Kick")
        .item("11.Kick (Mono)_01-02.wav")
        .end()
        .folder("Snare")
        .track("Top")
        .item("03.Snare Top_01-02.wav")
        .track("Snare 1")
        .item("04.Snare Room_01-02.wav")
        .track("Snare 2")
        .item("12.Snare_01-02.wav")
        .end()
        .folder("Cymbals")
        .track("OH")
        .item("09.Toms etc Overhead_01-02.wav")
        .track("Hi Hat 1")
        .item("05.Hi Hat Close_01-02.wav")
        .track("Hi Hat 2")
        .item("06.Hi Hat Room_01-02.wav")
        .end()
        .folder("Rooms")
        .track("Rooms 1")
        .item("02.Kick.Room_01-02.wav")
        .track("Rooms 2")
        .item("10.Toms etc Room_01-02.wav")
        .end()
        .end()
        .track("Percussion")
        .item("26.Weird Synth Patch - CASIO Timpani_1.1_01-02.wav")
        .track("Bass")
        .item("13.Bass_01-02.wav")
        .folder("Guitars")
        .folder("Amp")
        .track("Electric 1")
        .item("15.Guitar.Amp.Heavy.Rakes_01-02.wav")
        .track("Electric 2")
        .item("16.Guitar.Amp.Room.Heavy.Rakes_01-02.wav")
        .track("Electric 3")
        .item("18.Guitar.Amp.Two_01-02.wav")
        .track("Electric 4")
        .item("19.Guitar.Amp.Room.Two_01-02.wav")
        .track("Electric 5")
        .item("21.Guitar.Amp_01-02.wav")
        .end()
        .folder("DI")
        .track("Electric 1")
        .item("14.Guitar.DI.Heavy.Rakes_01-02.wav")
        .track("Electric 2")
        .item("17.Guitar.DI.Two_01-02.wav")
        .track("Electric 3")
        .item("20.Guitar.DI_01-02.wav")
        .end()
        .end()
        .folder("Keys")
        .track("Organ")
        .item("24.NORD B3_1.1_01-02.wav")
        .track("Keys")
        .item("33.Keys_01-02.wav")
        .end()
}

fn tears_for_fears_shout_expected() -> daw_proto::TrackHierarchy {
    tears_for_fears_shout_expected_a()
        .folder("Synths")
        .track("Breathy Synth 1")
        .item("22.Breathy Synth 1 - Casio CTK-601 Briteness_01-02.wav")
        .track("Breathy Synth 2 - _1.1")
        .item("23.Breathy Synth 2 - Casio CTK-601 Briteness_1.1_01-02.wav")
        .track("1.1")
        .item("25.Weird Synth Patch - CASIO Charang_1.1_01-02.wav")
        .track("1_1.1")
        .item("27.Weird Synth Patch - FA06 1_1.1_01-02.wav")
        .track("2_1.1")
        .item("28.Weird Synth Patch - FA06 2_1.1_01-02.wav")
        .track("3_1.1")
        .item("29.Weird Synth Patch - FA06 3_1.1_01-02.wav")
        .track("4_1.1")
        .item("30.Weird Synth Patch - FA06 4_1.1_01-02.wav")
        .track("1.1")
        .item("31.Weird Synth Patch - FA06 Dog Bark_1.1_01-02.wav")
        .track("Synth_SUM_1.1")
        .item("32.Weird Synth Patch - FA06 Synth_SUM_1.1_01-02.wav")
        .end()
        .folder("Vocals")
        .folder("Lead")
        .folder("Chorus")
        .folder("Main")
        .track("L")
        .item("45.SHOUT VOX_CHORUS SHOUT L_01-02.R.wav")
        .folder("R")
        .track("Chorus 1")
        .item("42.SHOUT VOX_CHORUS LEAD_01-02.R.wav")
        .track("Chorus 2")
        .item("43.SHOUT VOX_CHORUS POWERFUL END_01-02.R.wav")
        .track("Chorus 3")
        .item("46.SHOUT VOX_CHORUS SHOUT R_01-02.R.wav")
        .end()
        .end()
        .folder("DBL")
        .track("L")
        .item("40.SHOUT VOX_CHORUS DBL L_01-02.R.wav")
        .folder("R")
        .track("Chorus 1")
        .item("39.SHOUT VOX_CHORUS DBL CENTRE_01-02.R.wav")
        .track("Chorus 2")
        .item("41.SHOUT VOX_CHORUS DBL R_01-02.R.wav")
        .end()
        .end()
        .end()
        .track("Verse")
        .item("44.SHOUT VOX_VERSE_01-02.R.wav")
        .end()
        .folder("BGVs")
        .folder("BGVs 1")
        .track("L")
        .item("34.SHOUT VOX_CHORUS AD LIB 1 L_01-02.R.wav")
        .track("R")
        .item("35.SHOUT VOX_CHORUS AD LIB 1 R_01-02.R.wav")
        .end()
        .track("BGVs 2")
        .item("36.SHOUT VOX_CHORUS AD LIB 2_01-02.R.wav")
        .track("BGVs 3")
        .item("37.SHOUT VOX_CHORUS AD LIB 3_01-02.R.wav")
        .track("BGVs 4")
        .item("38.SHOUT VOX_CHORUS AD LIB 4_01-02.R.wav")
        .end()
        .end()
        .track("Orchestra")
        .item("26.Weird Synth Patch - CASIO Timpani_1.1_01-02.wav")
        .folder("Unsorted")
        .track("Tea Time Over")
        .item("07.Tea Time Over_01-02.wav")
        .track("Tea Time Logo")
        .item("08.Tea Time Logo_01-02.wav")
        .track("Shout_ BIMM_4824")
        .item("47.Shout_Neumann BIMM_4824_mst_01-02.wav")
        .end()
        .build()
}

#[test]
fn tears_for_fears_shout() {
    // -- Setup & Fixtures
    // Tears for Fears - Shout: 47-track 80s synth-pop session with complex vocal arrangement
    let items = vec![
        "01.Kick.In.One_01-02.wav",
        "02.Kick.Room_01-02.wav",
        "03.Snare Top_01-02.wav",
        "04.Snare Room_01-02.wav",
        "05.Hi Hat Close_01-02.wav",
        "06.Hi Hat Room_01-02.wav",
        "07.Tea Time Over_01-02.wav",
        "08.Tea Time Logo_01-02.wav",
        "09.Toms etc Overhead_01-02.wav",
        "10.Toms etc Room_01-02.wav",
        "11.Kick (Mono)_01-02.wav",
        "12.Snare_01-02.wav",
        "13.Bass_01-02.wav",
        "14.Guitar.DI.Heavy.Rakes_01-02.wav",
        "15.Guitar.Amp.Heavy.Rakes_01-02.wav",
        "16.Guitar.Amp.Room.Heavy.Rakes_01-02.wav",
        "17.Guitar.DI.Two_01-02.wav",
        "18.Guitar.Amp.Two_01-02.wav",
        "19.Guitar.Amp.Room.Two_01-02.wav",
        "20.Guitar.DI_01-02.wav",
        "21.Guitar.Amp_01-02.wav",
        "22.Breathy Synth 1 - Casio CTK-601 Briteness_01-02.wav",
        "23.Breathy Synth 2 - Casio CTK-601 Briteness_1.1_01-02.wav",
        "24.NORD B3_1.1_01-02.wav",
        "25.Weird Synth Patch - CASIO Charang_1.1_01-02.wav",
        "26.Weird Synth Patch - CASIO Timpani_1.1_01-02.wav",
        "27.Weird Synth Patch - FA06 1_1.1_01-02.wav",
        "28.Weird Synth Patch - FA06 2_1.1_01-02.wav",
        "29.Weird Synth Patch - FA06 3_1.1_01-02.wav",
        "30.Weird Synth Patch - FA06 4_1.1_01-02.wav",
        "31.Weird Synth Patch - FA06 Dog Bark_1.1_01-02.wav",
        "32.Weird Synth Patch - FA06 Synth_SUM_1.1_01-02.wav",
        "33.Keys_01-02.wav",
        "34.SHOUT VOX_CHORUS AD LIB 1 L_01-02.R.wav",
        "35.SHOUT VOX_CHORUS AD LIB 1 R_01-02.R.wav",
        "36.SHOUT VOX_CHORUS AD LIB 2_01-02.R.wav",
        "37.SHOUT VOX_CHORUS AD LIB 3_01-02.R.wav",
        "38.SHOUT VOX_CHORUS AD LIB 4_01-02.R.wav",
        "39.SHOUT VOX_CHORUS DBL CENTRE_01-02.R.wav",
        "40.SHOUT VOX_CHORUS DBL L_01-02.R.wav",
        "41.SHOUT VOX_CHORUS DBL R_01-02.R.wav",
        "42.SHOUT VOX_CHORUS LEAD_01-02.R.wav",
        "43.SHOUT VOX_CHORUS POWERFUL END_01-02.R.wav",
        "44.SHOUT VOX_VERSE_01-02.R.wav",
        "45.SHOUT VOX_CHORUS SHOUT L_01-02.R.wav",
        "46.SHOUT VOX_CHORUS SHOUT R_01-02.R.wav",
        "47.Shout_Neumann BIMM_4824_mst_01-02.wav",
    ];

    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick/
    //   │   ├─ SUM                  ← 01.Kick.In.One (recognized as kick sum)
    //   │   └─ Kick                 ← 11.Kick (Mono)
    //   ├─ Snare/
    //   │   ├─ Top                  ← 03.Snare Top
    //   │   ├─ Snare 1              ← 04.Snare Room
    //   │   └─ Snare 2              ← 12.Snare
    //   ├─ Cymbals/
    //   │   ├─ OH                   ← 09.Toms etc Overhead
    //   │   ├─ Hi Hat 1             ← 05.Hi Hat Close
    //   │   └─ Hi Hat 2             ← 06.Hi Hat Room
    //   └─ Rooms/
    //       ├─ Rooms 1              ← 02.Kick.Room
    //       └─ Rooms 2              ← 10.Toms etc Room
    // Note: Guitar.Amp.Room tracks excluded from Drums/Rooms (guitar exclusion), now in Guitars/Amp
    // Percussion                    ← 26.Weird Synth Patch - CASIO Timpani (timpani)
    // Bass                          ← 13.Bass
    // Guitars/
    //   ├─ Amp/                     ← Guitar amps grouped
    //   └─ DI/                      ← Guitar DIs grouped
    // Keys/
    //   ├─ Organ                    ← 24.NORD B3 (B3 = organ)
    //   └─ Keys                     ← 33.Keys
    // Synths/                       ← 22-32 synth patches
    // Vocals/                       ← Complex chorus/verse structure
    // Orchestra                     ← Same track as Percussion (timpani triggers both)
    // Unsorted/                     ← Tea Time Over/Logo, Shout_Neumann mix
    let expected = tears_for_fears_shout_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
