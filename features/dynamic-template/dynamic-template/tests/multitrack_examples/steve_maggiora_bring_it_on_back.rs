use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn steve_maggiora_bring_it_on_back_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .folder("SUM")
        .track("In")
        .item("Kick In.wav")
        .track("Out")
        .item("Kick Out.wav")
        .end()
        .track("Kick")
        .item("Kick_SUM.wav")
        .end()
        .folder("Snare")
        .track("Top")
        .item("Snare Top.wav")
        .track("Bottom")
        .item("Snare Bottom.wav")
        .track("Snare")
        .item("Snare_SUM.wav")
        .end()
        .folder("Toms")
        .track("Floor")
        .item("Floor Tom_SUM.wav")
        .folder("Bottom")
        .track("Tom Bottom 1")
        .item("Floor Tom Bottom.wav")
        .track("Tom Bottom 2")
        .item("Rack Tom Bottom.wav")
        .end()
        .folder("Top")
        .track("Tom Top 1")
        .item("Floor Tom Top.wav")
        .track("Tom Top 2")
        .item("Rack Tom Top.wav")
        .end()
        .track("Tom")
        .item("Rack Tom_SUM.wav")
        .end()
        .folder("Cymbals")
        .track("Hi Hat")
        .item("HH.wav")
        .folder("OH")
        .track("OH 1")
        .item("OH_SUM.wav")
        .track("OH 2")
        .item("OHL.wav")
        .track("OH 3")
        .item("OHR.wav")
        .end()
        .track("Cymbals")
        .item("Cymbal Swell.wav")
        .end()
        .folder("Rooms")
        .track("L")
        .item("Room L.wav")
        .track("R")
        .item("Room R.wav")
        .track("Mono")
        .item("Room Mono.wav")
        .track("Rooms 1")
        .item("Room C.wav")
        .track("Rooms 2")
        .item("Rooms_SUM.wav")
        .end()
        .end()
        .folder("Bass")
        .track("Amp")
        .item("Bass Amp.wav")
        .track("Bass 1")
        .item("Bass DI.wav")
        .track("Bass 2")
        .item("Bass_SUM.wav")
        .end()
        .folder("Guitars")
        .folder("Lead")
        .track("Amp")
        .item("Lead Guitar Amp.wav")
        .track("DI")
        .item("Lead Guitar DI.wav")
        .track("Electric")
        .item("Lead Guitar_SUM.wav")
        .end()
        .folder("Guitars")
        .track("DI")
        .item("Guitar DI.wav")
        .track("Electric")
        .item("Guitar_SUM.wav")
        .end()
        .folder("Guitars 1")
        .track("Electric Amp 1 1")
        .item("Guitar Amp 1A.wav")
        .track("Electric Amp 1 2")
        .item("Guitar Amp 1B.wav")
        .end()
        .end()
}

fn steve_maggiora_bring_it_on_back_expected() -> daw_proto::TrackHierarchy {
    steve_maggiora_bring_it_on_back_expected_a()
        .folder("Keys")
        .folder("Piano")
        .track("Piano 1")
        .item("Piano L.wav")
        .track("Piano 2")
        .item("Piano R.wav")
        .track("Piano 3")
        .item("Piano_SUM.wav")
        .end()
        .folder("Wurlitzer")
        .track("Wurlitzer 1")
        .item("Wurlitzer L.wav")
        .track("Wurlitzer 2")
        .item("Wurlitzer R.wav")
        .track("Wurlitzer 3")
        .item("Wurlitzer_SUM.wav")
        .end()
        .end()
        .folder("Synths")
        .track("Boho Synth Pluck")
        .item("Boho Synth Pluck.wav")
        .track("Synth Rise Run 1")
        .item("Synth Rise Run 1.wav")
        .track("Synth Rise Run 2")
        .item("Synth Rise Run 2.wav")
        .end()
        .folder("Horns")
        .track("Saxophone 1")
        .item("03 Tenor sax - Bring It On Back.wav")
        .track("Saxophone 2")
        .item("04 Bari sax - Bring It On Back.wav")
        .track("Saxophone 3")
        .item("06 Sax FX only - Bring It On  Back Multis.wav")
        .end()
        .folder("Vocals")
        .folder("Lead")
        .folder("Kim")
        .track("Kim")
        .item("Kim VOX_SUM.wav")
        .folder("Kim 1")
        .track("Kim 1")
        .item("Kim VOX 1A.wav")
        .track("Kim 2")
        .item("Kim VOX 1B.wav")
        .end()
        .end()
        .folder("Main")
        .track("Lead 1")
        .item("Lead VOX.wav")
        .track("Lead 2")
        .item("Xavier VOX.wav")
        .end()
        .folder("Double")
        .track("Double 1")
        .item("Lead VOX Double 1.wav")
        .track("Double 2")
        .item("Lead VOX Double 2.wav")
        .track("Double 3")
        .item("Lead VOX Double_SUM.wav")
        .end()
        .end()
        .folder("BGVs")
        .folder("Bridge")
        .track("Bridge 1")
        .item("07 MG_s Bridge BG_s - Bring It On  Back Multis.wav")
        .track("Bridge 2")
        .item("08 MG_s Bridge BG_s FX only - Bring It On  Back Multis.wav")
        .end()
        .folder("BGVs")
        .track("BGVs 1")
        .item("BGV1_SUM.wav")
        .track("BGVs 2")
        .item("BGV1A.wav")
        .track("BGVs 3")
        .item("BGV1B.wav")
        .track("BGVs 4")
        .item("BGV1C.wav")
        .track("BGVs 5")
        .item("BGV1D.wav")
        .end()
        .end()
        .end()
        .folder("Orchestra")
        .track("Trumpets")
        .item("01 Trumpet - Bring It On Back.wav")
        .track("Trombone")
        .item("02 Trombone - Bring It On Back.wav")
        .end()
        .track("Unsorted")
        .item("05Sax fills - Bring It On  Back Multis.wav")
        .build()
}

#[test]
fn steve_maggiora_bring_it_on_back() {
    // -- Setup & Fixtures
    // Steve Maggiora - Bring It On Back: 71-track soul/R&B session with horn section and layered BGVs
    let items = vec![
        "01 Trumpet - Bring It On Back.wav",
        "02 Trombone - Bring It On Back.wav",
        "03 Tenor sax - Bring It On Back.wav",
        "04 Bari sax - Bring It On Back.wav",
        "05Sax fills - Bring It On  Back Multis.wav",
        "06 Sax FX only - Bring It On  Back Multis.wav",
        "07 MG_s Bridge BG_s - Bring It On  Back Multis.wav",
        "08 MG_s Bridge BG_s FX only - Bring It On  Back Multis.wav",
        "Bass Amp.wav",
        "Bass DI.wav",
        "Bass_SUM.wav",
        "BGV1_SUM.wav",
        "BGV1A.wav",
        "BGV1B.wav",
        "BGV1C.wav",
        "BGV1D.wav",
        "Boho Synth Pluck.wav",
        "Cymbal Swell.wav",
        "Floor Tom Bottom.wav",
        "Floor Tom Top.wav",
        "Floor Tom_SUM.wav",
        "Guitar Amp 1A.wav",
        "Guitar Amp 1B.wav",
        "Guitar DI.wav",
        "Guitar_SUM.wav",
        "HH.wav",
        "Kick In.wav",
        "Kick Out.wav",
        "Kick_SUM.wav",
        "Kim VOX 1A.wav",
        "Kim VOX 1B.wav",
        "Kim VOX_SUM.wav",
        "Lead Guitar Amp.wav",
        "Lead Guitar DI.wav",
        "Lead Guitar_SUM.wav",
        "Lead VOX Double 1.wav",
        "Lead VOX Double 2.wav",
        "Lead VOX Double_SUM.wav",
        "Lead VOX.wav",
        "OH_SUM.wav",
        "OHL.wav",
        "OHR.wav",
        "Piano L.wav",
        "Piano R.wav",
        "Piano_SUM.wav",
        "Rack Tom Bottom.wav",
        "Rack Tom Top.wav",
        "Rack Tom_SUM.wav",
        "Room C.wav",
        "Room L.wav",
        "Room Mono.wav",
        "Room R.wav",
        "Rooms_SUM.wav",
        "Snare Bottom.wav",
        "Snare Top.wav",
        "Snare_SUM.wav",
        "Synth Rise Run 1.wav",
        "Synth Rise Run 2.wav",
        "Wurlitzer L.wav",
        "Wurlitzer R.wav",
        "Wurlitzer_SUM.wav",
        "Xavier VOX.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick/                    ← SUM (In/Out) and Kick
    //   ├─ Snare/                   ← Top, Bottom, Snare
    //   ├─ Toms/                    ← Floor, Bottom, Top, Tom
    //   ├─ Cymbals/
    //   │   ├─ Hi Hat               ← HH.wav
    //   │   ├─ OH/                  ← OH_SUM, OHL, OHR
    //   │   └─ Cymbals              ← Cymbal Swell
    //   └─ Rooms/                   ← L, R, Mono, C, Rooms_SUM
    // Bass/                         ← Amp, Bass 1 (DI), Bass 2 (SUM)
    // Guitars/
    //   ├─ Lead/                    ← Lead Guitar Amp/DI/SUM
    //   ├─ Guitars/                 ← Guitar DI/SUM
    //   └─ Guitars 1/               ← Guitar Amp 1A/1B
    // Keys/
    //   ├─ Piano/                   ← Piano L/R/SUM
    //   └─ Wurlitzer/               ← Wurlitzer L/R/SUM
    // Synths/                       ← Boho Synth Pluck, Synth Rise Run 1/2
    // Horns/
    //   └─ Saxophone 1-3            ← Tenor sax, Bari sax, Sax FX
    // Vocals/
    //   ├─ Lead/                    ← Kim VOX, Lead VOX, Xavier VOX, Doubles
    //   └─ BGVs/
    //       ├─ Bridge/              ← MG's Bridge BG's (now recognized as BG)
    //       └─ BGVs/                ← BGV1_SUM, BGV1A-D
    // Orchestra/
    //   ├─ Trumpets                 ← 01 Trumpet
    //   └─ Trombone                 ← 02 Trombone
    // Unsorted                      ← 05Sax fills (single track, not a folder)
    let expected = steve_maggiora_bring_it_on_back_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
