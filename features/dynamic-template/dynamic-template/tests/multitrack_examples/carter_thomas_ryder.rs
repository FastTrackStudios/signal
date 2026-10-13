use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn carter_thomas_ryder_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .track("Drum Kit 1")
        .item("DRUM LOOP 2.wav")
        .track("Drum Kit 2")
        .item("DRUM LOOP.wav")
        .end()
        .track("Bass")
        .item("BASS.wav")
        .folder("Guitars")
        .track("L")
        .item("GTR STAB L.wav")
        .track("R")
        .item("GTR STAB R.wav")
        .end()
        .folder("Synths")
        .track("Pad")
        .item("PADS.wav")
        .folder("Arp")
        .track("Arp 1")
        .item("ARP 1.wav")
        .track("Transition")
        .item("ARP TRANSITION.wav")
        .end()
        .end()
        .folder("Vocals")
        .folder("Lead")
        .folder("Lead")
        .track("Dly 1")
        .item("Vocal EFX _12 DLY.wav")
        .track("Dly 2")
        .item("Vocal EFX _14TH DLY.wav")
        .track("Lead 1")
        .item("Vocal EFX _480L.wav")
        .track("Lead 2")
        .item("Vocal EFX _DIM D.wav")
        .track("Lead 3")
        .item("Vocal EFX _DOUBLER.wav")
        .track("Lead 4")
        .item("Vocal EFX _EMT 140.wav")
        .track("Dly Slap")
        .item("Vocal EFX _SLAP DLY.wav")
        .end()
}

fn carter_thomas_ryder_expected_b() -> TrackStructureBuilder {
    carter_thomas_ryder_expected_a()
        .track("Mono")
        .item("Vocal EFX _MONO DLY.wav")
        .end()
        .folder("BGVs")
        .folder("V1")
        .folder("L")
        .track("BGVs 1")
        .item("V1 BGV1 L1.wav")
        .track("BGVs 2")
        .item("V1 BGV1 L2.wav")
        .track("BGVs 3")
        .item("V1 BGV1 L3.wav")
        .track("BGVs 4")
        .item("V1 BGV1 L4.wav")
        .track("BGVs 5")
        .item("V1 BGV2 L1.wav")
        .track("BGVs 6")
        .item("V1 BGV2 L2.wav")
        .track("BGVs 7")
        .item("V1 BGV2 L3.wav")
        .track("BGVs 8")
        .item("V1 BGV2 L4.wav")
        .track("BGVs 9")
        .item("V1 BGV2 L5.wav")
        .track("BGVs 10")
        .item("V1 BGV2 L6.wav")
        .track("BGVs 11")
        .item("V1 BGV2 L7.wav")
        .end()
        .folder("R")
        .track("BGVs 1")
        .item("V1 BGV1 R1.wav")
        .track("BGVs 2")
        .item("V1 BGV1 R2.wav")
        .track("BGVs 3")
        .item("V1 BGV1 R3.wav")
        .track("BGVs 4")
        .item("V1 BGV1 R4.wav")
        .track("BGVs 5")
        .item("V1 BGV2 R1.wav")
        .track("BGVs 6")
        .item("V1 BGV2 R2.wav")
        .track("BGVs 7")
        .item("V1 BGV2 R3.wav")
        .track("BGVs 8")
        .item("V1 BGV2 R4.wav")
        .track("BGVs 9")
        .item("V1 BGV2 R5.wav")
        .track("BGVs 10")
        .item("V1 BGV2 R6.wav")
        .track("BGVs 11")
        .item("V1 BGV2 R7.wav")
        .end()
        .end()
}

fn carter_thomas_ryder_expected_c() -> TrackStructureBuilder {
    carter_thomas_ryder_expected_b()
        .folder("V2")
        .folder("L")
        .track("BGVs 1")
        .item("V2 BGV1 L1.wav")
        .track("BGVs 2")
        .item("V2 BGV2 L1.wav")
        .track("BGVs 3")
        .item("V2 BGV2 L2.wav")
        .track("BGVs 4")
        .item("V2 BGV2 L3.wav")
        .track("BGVs 5")
        .item("V2 BGV3 L1.wav")
        .track("BGVs 6")
        .item("V2 BGV3 L2.wav")
        .track("BGVs 7")
        .item("V2 BGV3 L3.wav")
        .track("BGVs 8")
        .item("V2 BGV4 L1.wav")
        .track("BGVs 9")
        .item("V2 BGV4 L2.wav")
        .track("BGVs 10")
        .item("V2 BGV4 L3.wav")
        .track("BGVs 11")
        .item("V2 BGV4 L4.wav")
        .track("BGVs 12")
        .item("V2 BGV5 L1.wav")
        .track("BGVs 13")
        .item("V2 BGV5 L2.wav")
        .track("BGVs 14")
        .item("V2 BGV5 L3.wav")
        .end()
        .folder("R")
        .track("BGVs 1")
        .item("V2 BGV1 R2.wav")
        .track("BGVs 2")
        .item("V2 BGV2 R1.wav")
        .track("BGVs 3")
        .item("V2 BGV2 R2.wav")
        .track("BGVs 4")
        .item("V2 BGV2 R3.wav")
        .track("BGVs 5")
        .item("V2 BGV3 R1.wav")
        .track("BGVs 6")
        .item("V2 BGV3 R2.wav")
        .track("BGVs 7")
        .item("V2 BGV3 R3.wav")
        .track("BGVs 8")
        .item("V2 BGV4 R1.wav")
        .track("BGVs 9")
        .item("V2 BGV4 R2.wav")
        .track("BGVs 10")
        .item("V2 BGV4 R3.wav")
        .track("BGVs 11")
        .item("V2 BGV4 R4.wav")
        .track("BGVs 12")
        .item("V2 BGV5 R1.wav")
        .track("BGVs 13")
        .item("V2 BGV5 R2.wav")
        .track("BGVs 14")
        .item("V2 BGV5 R3.wav")
        .end()
        .end()
}

fn carter_thomas_ryder_expected_d() -> TrackStructureBuilder {
    carter_thomas_ryder_expected_c()
        .track("Ad Lib")
        .item("AD LIB 1.wav")
        .folder("BGVs")
        .folder("L")
        .track("L 1")
        .item("HK BGV L.wav")
        .track("L 2")
        .item("HK BGV L2.wav")
        .track("L 3")
        .item("HK BGV2 L1.wav")
        .track("L 4")
        .item("HK BGV2 L2.wav")
        .track("L 5")
        .item("HK BGV2 L3.wav")
        .track("L 6")
        .item("HK BGV2 L4.wav")
        .track("L 7")
        .item("HK BGV2 L5.wav")
        .track("L 8")
        .item("HK BGV2 L6.wav")
        .track("L 9")
        .item("HK BGV3 L1.wav")
        .track("L 10")
        .item("HK BGV3 L2.wav")
        .track("L 11")
        .item("HK BGV3 L3.wav")
        .track("L 12")
        .item("HK BGV3 L4.wav")
        .track("L 13")
        .item("HK BGV3 L5.wav")
        .track("L 14")
        .item("HK BGV3 L6.wav")
        .track("L 15")
        .item("HK BGV4 L1.wav")
        .track("L 16")
        .item("HK BGV5 L1.wav")
        .end()
        .folder("R")
        .track("R 1")
        .item("HK BGV R.wav")
        .track("R 2")
        .item("HK BGV R2.wav")
        .track("R 3")
        .item("HK BGV2 R1.wav")
        .track("R 4")
        .item("HK BGV2 R2.wav")
        .track("R 5")
        .item("HK BGV2 R3.wav")
        .track("R 6")
        .item("HK BGV2 R4.wav")
        .track("R 7")
        .item("HK BGV2 R5.wav")
        .track("R 8")
        .item("HK BGV2 R6.wav")
        .track("R 9")
        .item("HK BGV3 R1.wav")
        .track("R 10")
        .item("HK BGV3 R2.wav")
        .track("R 11")
        .item("HK BGV3 R3.wav")
        .track("R 12")
        .item("HK BGV3 R4.wav")
        .track("R 13")
        .item("HK BGV3 R5.wav")
        .track("R 14")
        .item("HK BGV3 R6.wav")
        .track("R 15")
        .item("HK BGV4 R1.wav")
        .track("R 16")
        .item("HK BGV5 R1.wav")
        .end()
        .end()
        .end()
        .end()
}

fn carter_thomas_ryder_expected() -> daw_proto::TrackHierarchy {
    carter_thomas_ryder_expected_d()
        .track("SFX")
        .item("WAVES SFX.wav")
        .folder("Unsorted")
        .track("THROW")
        .item("12 THROW.wav")
        .track("14TH THROW")
        .item("14TH THROW.wav")
        .track("D-50")
        .item("D-50.wav")
        .track("DBL 1")
        .item("DBL 1.wav")
        .track("DBL 2")
        .item("DBL 2.wav")
        .track("DBL 3")
        .item("DBL 3.wav")
        .track("DBL 4")
        .item("DBL 4.wav")
        .track("DBL 5")
        .item("DBL 5.wav")
        .track("DBL 6")
        .item("DBL 6.wav")
        .track("DBL 7")
        .item("DBL 7.wav")
        .track("DBL 8")
        .item("DBL 8.wav")
        .track("HK VLD 1")
        .item("HK VLD 1.wav")
        .track("HK VLD 2")
        .item("HK VLD 2.wav")
        .track("VERB THROW")
        .item("VERB THROW.wav")
        .track("VRS LD 1")
        .item("VRS LD 1.wav")
        .track("VRS LD 2")
        .item("VRS LD 2.wav")
        .track("VRS LD 3")
        .item("VRS LD 3.wav")
        .end()
        .build()
}

fn carter_thomas_ryder_items_a() -> Vec<&'static str> {
    vec![
        "12 THROW.wav",
        "14TH THROW.wav",
        "AD LIB 1.wav",
        "ARP 1.wav",
        "ARP TRANSITION.wav",
        "BASS.wav",
        "D-50.wav",
        "DBL 1.wav",
        "DBL 2.wav",
        "DBL 3.wav",
        "DBL 4.wav",
        "DBL 5.wav",
        "DBL 6.wav",
        "DBL 7.wav",
        "DBL 8.wav",
        "DRUM LOOP 2.wav",
        "DRUM LOOP.wav",
        "GTR STAB L.wav",
        "GTR STAB R.wav",
        "HK BGV L.wav",
        "HK BGV L2.wav",
        "HK BGV R.wav",
        "HK BGV R2.wav",
        "HK BGV2 L1.wav",
        "HK BGV2 L2.wav",
        "HK BGV2 L3.wav",
        "HK BGV2 L4.wav",
        "HK BGV2 L5.wav",
        "HK BGV2 L6.wav",
        "HK BGV2 R1.wav",
        "HK BGV2 R2.wav",
        "HK BGV2 R3.wav",
        "HK BGV2 R4.wav",
        "HK BGV2 R5.wav",
        "HK BGV2 R6.wav",
        "HK BGV3 L1.wav",
        "HK BGV3 L2.wav",
        "HK BGV3 L3.wav",
        "HK BGV3 L4.wav",
        "HK BGV3 L5.wav",
        "HK BGV3 L6.wav",
        "HK BGV3 R1.wav",
        "HK BGV3 R2.wav",
        "HK BGV3 R3.wav",
        "HK BGV3 R4.wav",
        "HK BGV3 R5.wav",
        "HK BGV3 R6.wav",
        "HK BGV4 L1.wav",
        "HK BGV4 R1.wav",
        "HK BGV5 L1.wav",
        "HK BGV5 R1.wav",
        "HK VLD 1.wav",
        "HK VLD 2.wav",
        "PADS.wav",
        "V1 BGV1 L1.wav",
        "V1 BGV1 L2.wav",
        "V1 BGV1 L3.wav",
        "V1 BGV1 L4.wav",
    ]
}

/// Carter Thomas - Ryder: Electronic production with drum loops, arps, and extensive BGV layers
fn carter_thomas_ryder_items() -> Vec<&'static str> {
    let mut items = carter_thomas_ryder_items_a();
    items.extend([
        "V1 BGV1 R1.wav",
        "V1 BGV1 R2.wav",
        "V1 BGV1 R3.wav",
        "V1 BGV1 R4.wav",
        "V1 BGV2 L1.wav",
        "V1 BGV2 L2.wav",
        "V1 BGV2 L3.wav",
        "V1 BGV2 L4.wav",
        "V1 BGV2 L5.wav",
        "V1 BGV2 L6.wav",
        "V1 BGV2 L7.wav",
        "V1 BGV2 R1.wav",
        "V1 BGV2 R2.wav",
        "V1 BGV2 R3.wav",
        "V1 BGV2 R4.wav",
        "V1 BGV2 R5.wav",
        "V1 BGV2 R6.wav",
        "V1 BGV2 R7.wav",
        "V2 BGV1 L1.wav",
        "V2 BGV1 R2.wav",
        "V2 BGV2 L1.wav",
        "V2 BGV2 L2.wav",
        "V2 BGV2 L3.wav",
        "V2 BGV2 R1.wav",
        "V2 BGV2 R2.wav",
        "V2 BGV2 R3.wav",
        "V2 BGV3 L1.wav",
        "V2 BGV3 L2.wav",
        "V2 BGV3 L3.wav",
        "V2 BGV3 R1.wav",
        "V2 BGV3 R2.wav",
        "V2 BGV3 R3.wav",
        "V2 BGV4 L1.wav",
        "V2 BGV4 L2.wav",
        "V2 BGV4 L3.wav",
        "V2 BGV4 L4.wav",
        "V2 BGV4 R1.wav",
        "V2 BGV4 R2.wav",
        "V2 BGV4 R3.wav",
        "V2 BGV4 R4.wav",
        "V2 BGV5 L1.wav",
        "V2 BGV5 L2.wav",
        "V2 BGV5 L3.wav",
        "V2 BGV5 R1.wav",
        "V2 BGV5 R2.wav",
        "V2 BGV5 R3.wav",
        "VERB THROW.wav",
        "Vocal EFX _12 DLY.wav",
        "Vocal EFX _14TH DLY.wav",
        "Vocal EFX _480L.wav",
        "Vocal EFX _DIM D.wav",
        "Vocal EFX _DOUBLER.wav",
        "Vocal EFX _EMT 140.wav",
        "Vocal EFX _MONO DLY.wav",
        "Vocal EFX _SLAP DLY.wav",
        "VRS LD 1.wav",
        "VRS LD 2.wav",
        "VRS LD 3.wav",
        "WAVES SFX.wav",
    ]);
    items
}

#[test]
fn carter_thomas_ryder() {
    // -- Setup & Fixtures
    let items = carter_thomas_ryder_items();
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = carter_thomas_ryder_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
