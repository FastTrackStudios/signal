use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn katie_ferrara_how_deep_is_your_love_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("Kick 1")
        .item("Kick SILK BLUE 100%_01.wav")
        .track("Kick 2")
        .item("Kick SILK BLUE 50%_01.wav")
        .track("Kick 3")
        .item("Kick SILK OFF_05.wav")
        .track("Kick 4")
        .item("Kick SILK RED 100%_01.wav")
        .track("Kick 5")
        .item("Kick SILK RED 50%_01.wav")
        .end()
        .folder("Snare")
        .track("Snare 1")
        .item("Snare SILK BLUE 100%_01.wav")
        .track("Snare 2")
        .item("Snare SILK BLUE 50%_01.wav")
        .track("Snare 3")
        .item("Snare SILK OFF_01.wav")
        .track("Snare 4")
        .item("Snare SILK RED 100%_01.wav")
        .track("Snare 5")
        .item("Snare SILK RED 50%_01.wav")
        .end()
        .folder("Cymbals")
        .folder("Hi Hat")
        .track("Hi Hat 1")
        .item("HH SILK BLUE 100%_01.wav")
        .track("Hi Hat 2")
        .item("HH SILK BLUE 50%_01.wav")
        .track("Hi Hat 3")
        .item("HH SILK OFF_02.wav")
        .track("Hi Hat 4")
        .item("HH SILK RED 100%_01.wav")
        .track("Hi Hat 5")
        .item("HH SILK RED 50%_01.wav")
        .end()
        .folder("Ride")
        .track("Ride 1")
        .item("Ride SILK BLUE 100%_01.wav")
        .track("Ride 2")
        .item("Ride SILK BLUE 50%_01.wav")
        .track("Ride 3")
        .item("Ride SILK OFF_02.wav")
        .track("Ride 4")
        .item("Ride SILK RED 100%_01.wav")
        .track("Ride 5")
        .item("Ride SILK RED 50%_01.wav")
        .end()
        .end()
        .end()
        .folder("Bass")
        .track("Bass 1")
        .item("Bass SILK BLUE 100%_02.wav")
        .track("Bass 2")
        .item("Bass SILK BLUE 50%_02.wav")
        .track("Bass 3")
        .item("Bass SILK OFF_02.wav")
        .track("Bass 4")
        .item("Bass SILK RED 100%_02.wav")
        .track("Bass 5")
        .item("Bass SILK RED 50%_02.wav")
        .end()
        .folder("Guitars")
        .track("Acoustic 1")
        .item("Acoustic SILK BLUE 100%_03.wav")
        .track("Acoustic 2")
        .item("Acoustic SILK BLUE 50%_03.wav")
        .track("Acoustic 3")
        .item("Acoustic SILK OFF.02_03-02.wav")
        .track("Acoustic 4")
        .item("Acoustic SILK RED 100%_03.wav")
        .track("Acoustic 5")
        .item("Acoustic SILK RED 50%_03.wav")
        .end()
        .folder("Keys")
        .track("Keys 1")
        .item("Keys Silk Blue 100%_01.wav")
        .track("Keys 2")
        .item("Keys Silk Blue 50%_02.wav")
        .track("Keys 3")
        .item("Keys Silk OFF_01.wav")
        .track("Keys 4")
        .item("Keys Silk RED 100%_01-03.wav")
        .track("Keys 5")
        .item("Keys Silk RED 50%_01.wav")
        .end()
}

fn katie_ferrara_how_deep_is_your_love_expected() -> daw_proto::TrackHierarchy {
    katie_ferrara_how_deep_is_your_love_expected_a()
        .folder("Vocals")
        .folder("Lead")
        .folder("Katie")
        .track("Katie 1")
        .item("Katie Vocal SILK BLUE 100%_01.wav")
        .track("Katie 2")
        .item("Katie Vocal SILK BLUE 50%_01.wav")
        .track("Katie 3")
        .item("Katie Vocal SILK OFF.02_01.wav")
        .track("Katie 4")
        .item("Katie Vocal SILK RED 100%_01.wav")
        .track("Katie 5")
        .item("Katie Vocal SILK RED 50%_01.wav")
        .end()
        .folder("Steve")
        .track("Steve 1")
        .item("Steve Vocal SILK Blue 100%.wav")
        .track("Steve 2")
        .item("Steve Vocal SILK Blue 50%_01.wav")
        .track("Steve 3")
        .item("Steve Vocal SILK OFF.03_01.wav")
        .track("Steve 4")
        .item("Steve Vocal SILK RED 100%_01.wav")
        .track("Steve 5")
        .item("Steve Vocal SILK RED 50%_01.wav")
        .end()
        .end()
        .folder("BGVs")
        .folder("Low")
        .track("Steve 1")
        .item("Steve BGV Vocal SILK OFF.LOW_01.wav")
        .track("Steve 2")
        .item("Steve BGV Vocal SILK OFF.LOW.DBLE_01.wav")
        .end()
        .folder("High")
        .folder("High")
        .track("L")
        .item("Steve BGV Vocal SILK OFF.High.L.Dble_01.wav")
        .track("R")
        .item("Steve BGV Vocal SILK OFF.High.R.Dble_01.wav")
        .track("Steve")
        .item("Steve BGV Vocal SILK OFF.High_01.wav")
        .end()
        .folder("High 2")
        .track("L")
        .item("Steve BGV Vocal SILK OFF.High.2.L.Dble_01.wav")
        .track("R")
        .item("Steve BGV Vocal SILK OFF.High.2.R.Dble_01.wav")
        .track("Steve 2")
        .item("Steve BGV Vocal SILK OFF.High.2_01.wav")
        .end()
        .end()
        .folder("BGVs")
        .folder("L")
        .track("Steve L 1")
        .item("Steve BGV Vocal SILK OFF.BV.L.dble_01.wav")
        .track("Steve L 2")
        .item("Steve BGV Vocal SILK OFF.BV3.L.Dble_01.wav")
        .end()
        .folder("R")
        .track("Steve R 1")
        .item("Steve BGV Vocal SILK OFF.9th.R.Dble_01.wav")
        .track("Steve R 2")
        .item("Steve BGV Vocal SILK OFF.BV.R.dble_01.wav")
        .track("Steve R 3")
        .item("Steve BGV Vocal SILK OFF.BV3.R.Dble_01.wav")
        .end()
        .track("Steve 1")
        .item("Steve BGV Vocal SILK OFF.(th.LDble_01.wav")
        .track("Steve 2")
        .item("Steve BGV Vocal SILK OFF.9th_01.wav")
        .track("Steve 3")
        .item("Steve BGV Vocal SILK OFF.BV_01.wav")
        .track("Steve 4")
        .item("Steve BGV Vocal SILK OFF.BV3_01.wav")
        .end()
        .folder("BGVs 2")
        .track("L")
        .item("Steve BGV Vocal SILK OFF.BV.2.L.Dble_01.wav")
        .track("R")
        .item("Steve BGV Vocal SILK OFF.BV.2.R.Dble_01.wav")
        .track("Steve 2")
        .item("Steve BGV Vocal SILK OFF.BV.2_01.wav")
        .end()
        .end()
        .end()
        .build()
}

#[test]
fn katie_ferrara_how_deep_is_your_love() {
    // -- Setup & Fixtures
    // Katie Ferrara - How Deep Is Your Love: Complex session with SILK preamp variations and extensive BGVs
    let items = vec![
        "Acoustic SILK BLUE 100%_03.wav",
        "Acoustic SILK BLUE 50%_03.wav",
        "Acoustic SILK OFF.02_03-02.wav",
        "Acoustic SILK RED 100%_03.wav",
        "Acoustic SILK RED 50%_03.wav",
        "Bass SILK BLUE 100%_02.wav",
        "Bass SILK BLUE 50%_02.wav",
        "Bass SILK OFF_02.wav",
        "Bass SILK RED 100%_02.wav",
        "Bass SILK RED 50%_02.wav",
        "HH SILK BLUE 100%_01.wav",
        "HH SILK BLUE 50%_01.wav",
        "HH SILK OFF_02.wav",
        "HH SILK RED 100%_01.wav",
        "HH SILK RED 50%_01.wav",
        "Katie Vocal SILK BLUE 100%_01.wav",
        "Katie Vocal SILK BLUE 50%_01.wav",
        "Katie Vocal SILK OFF.02_01.wav",
        "Katie Vocal SILK RED 100%_01.wav",
        "Katie Vocal SILK RED 50%_01.wav",
        "Keys Silk Blue 100%_01.wav",
        "Keys Silk Blue 50%_02.wav",
        "Keys Silk OFF_01.wav",
        "Keys Silk RED 100%_01-03.wav",
        "Keys Silk RED 50%_01.wav",
        "Kick SILK BLUE 100%_01.wav",
        "Kick SILK BLUE 50%_01.wav",
        "Kick SILK OFF_05.wav",
        "Kick SILK RED 100%_01.wav",
        "Kick SILK RED 50%_01.wav",
        "Ride SILK BLUE 100%_01.wav",
        "Ride SILK BLUE 50%_01.wav",
        "Ride SILK OFF_02.wav",
        "Ride SILK RED 100%_01.wav",
        "Ride SILK RED 50%_01.wav",
        "Snare SILK BLUE 100%_01.wav",
        "Snare SILK BLUE 50%_01.wav",
        "Snare SILK OFF_01.wav",
        "Snare SILK RED 100%_01.wav",
        "Snare SILK RED 50%_01.wav",
        "Steve BGV Vocal SILK OFF.(th.LDble_01.wav",
        "Steve BGV Vocal SILK OFF.9th_01.wav",
        "Steve BGV Vocal SILK OFF.9th.R.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV_01.wav",
        "Steve BGV Vocal SILK OFF.BV.2_01.wav",
        "Steve BGV Vocal SILK OFF.BV.2.L.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV.2.R.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV.L.dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV.R.dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV3_01.wav",
        "Steve BGV Vocal SILK OFF.BV3.L.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.BV3.R.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.High_01.wav",
        "Steve BGV Vocal SILK OFF.High.2_01.wav",
        "Steve BGV Vocal SILK OFF.High.2.L.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.High.2.R.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.High.L.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.High.R.Dble_01.wav",
        "Steve BGV Vocal SILK OFF.LOW_01.wav",
        "Steve BGV Vocal SILK OFF.LOW.DBLE_01.wav",
        "Steve Vocal SILK Blue 100%.wav",
        "Steve Vocal SILK Blue 50%_01.wav",
        "Steve Vocal SILK OFF.03_01.wav",
        "Steve Vocal SILK RED 100%_01.wav",
        "Steve Vocal SILK RED 50%_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = katie_ferrara_how_deep_is_your_love_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
