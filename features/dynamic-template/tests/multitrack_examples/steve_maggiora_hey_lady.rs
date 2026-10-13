use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn steve_maggiora_hey_lady_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("Kick 1")
        .item("AD.Bootstraps.Kick.83.5BPM.L.wav")
        .track("Kick 2")
        .item("AD.Bootstraps.Kick.83.5BPM.R.wav")
        .track("Kick 3")
        .item("AD.Metal.Kick.Print.83.5BPM.L.wav")
        .track("Kick 4")
        .item("AD.Metal.Kick.Print.83.5BPM.R.wav")
        .track("Kick 5")
        .item("Kick.83.5BPM.wav")
        .end()
        .folder("Snare")
        .track("Top")
        .item("SN Top.83.5BPM.wav")
        .track("Bottom")
        .item("SN Bot.83.5BPM.wav")
        .track("Snare 1")
        .item("Snare D 28_Oct.Pitch.83.5BPM.wav")
        .track("Snare 2")
        .item("Snare D 28.83.5BPM.wav")
        .end()
        .track("Toms")
        .item("Rack.27.Trick.83.5BPM.wav")
        .folder("Cymbals")
        .folder("OH")
        .track("L")
        .item("OH.83.5BPM.L.wav")
        .track("R")
        .item("OH.83.5BPM.R.wav")
        .end()
        .track("Hi Hat 5")
        .item("Hi Hat.83.5BPM.wav")
        .end()
        .track("Drum Kit Big 5 L")
        .item("AD.Big Kit Gretch.83.5BPM.L.wav")
        .track("Drum Kit Big 5 R")
        .item("AD.Big Kit Gretch.83.5BPM.R.wav")
        .track("Drum Kit Grace Big 5 L")
        .item("AD.Big Kit Gretch.Grace.83.5BPM.L.wav")
        .track("Drum Kit Grace Big 5 R")
        .item("AD.Big Kit Gretch.Grace.83.5BPM.R.wav")
        .end()
        .folder("Bass")
        .folder("Upright")
        .track("Upright Bass 1")
        .item("Upright.Forty.Seven.83.5BPM.wav")
        .track("Upright Bass 2")
        .item("Upright.Ribbon.83.5BPM.wav")
        .end()
        .track("Bass 1")
        .item("Bass.Di.83.5BPM.wav")
        .track("Bass 2")
        .item("Bass.Mic.83.5BPM.wav")
        .track("Bass 3")
        .item("Bass.Mic.dup1.83.5BPM.wav")
        .end()
        .folder("Guitars")
        .track("DI")
        .item("Gtr_Di.83.5BPM.wav")
        .track("Electric 1")
        .item("Gtr_57.83.5BPM.wav")
        .track("Electric 2")
        .item("Gtr_57.dup1.83.5BPM.wav")
        .end()
        .folder("Keys")
        .track("Organ 1")
        .item("B3.83.5BPM.C.wav")
        .track("Organ 2")
        .item("B3.83.5BPM.L.wav")
        .track("Organ 3")
        .item("B3.83.5BPM.R.wav")
        .track("Organ 4")
        .item("Hammond Hi.dup1.83.5BPM.L.wav")
        .track("Organ 5")
        .item("Hammond Hi.dup1.83.5BPM.R.wav")
        .end()
}

fn steve_maggiora_hey_lady_expected() -> daw_proto::TrackHierarchy {
    steve_maggiora_hey_lady_expected_a()
        .folder("Horns")
        .track("Saxophone Verb 5 L 1")
        .item("sax verb.83.5BPM.L.wav")
        .track("Saxophone Verb 5 R 1")
        .item("sax verb.83.5BPM.R.wav")
        .track("Saxophone Verb 5 L 2")
        .item("sax verb2.83.5BPM.L.wav")
        .track("Saxophone Verb 5 R 2")
        .item("sax verb2.83.5BPM.R.wav")
        .track("Saxophone Verb 5 L 3")
        .item("sax verb3.83.5BPM.L.wav")
        .track("Saxophone Verb 5 R 3")
        .item("sax verb3.83.5BPM.R.wav")
        .track("Saxophone 1")
        .item("sax1.83.5BPM.wav")
        .track("Saxophone 2")
        .item("sax2.83.5BPM.wav")
        .track("Saxophone 3")
        .item("sax3.83.5BPM.wav")
        .end()
        .folder("Vocals")
        .folder("L")
        .track("Lead Plate 5 L")
        .item("Vocal.Eko.Plate.83.5BPM.L.wav")
        .track("Lead")
        .item("Vocal.Magic.83.5BPM.L.wav")
        .end()
        .folder("R")
        .track("Lead Plate 5 R")
        .item("Vocal.Eko.Plate.83.5BPM.R.wav")
        .track("Lead")
        .item("Vocal.Magic.83.5BPM.R.wav")
        .end()
        .track("Lead 1")
        .item("Vocal.Comp.83.5BPM.wav")
        .track("Lead 2")
        .item("Vocal.Dub.83.5BPM.wav")
        .end()
        .folder("SFX")
        .track("H3000.One.New 1")
        .item("H3000.One.New.83.5BPM.L.wav")
        .track("H3000.One.New 2")
        .item("H3000.One.New.83.5BPM.R.wav")
        .track("H3000.Three.New 1")
        .item("H3000.Three.New.83.5BPM.L.wav")
        .track("H3000.Three.New 2")
        .item("H3000.Three.New.83.5BPM.R.wav")
        .track("H3000.Two.New 1")
        .item("H3000.Two.New.83.5BPM.L.wav")
        .track("H3000.Two.New 2")
        .item("H3000.Two.New.83.5BPM.R.wav")
        .end()
        .folder("Reference")
        .track("Hey.Lady .10.08.15.Mix.Recall 1")
        .item("Hey.Lady.83.5.BPM.10.08.15.Mix.Recall.L.wav")
        .track("Hey.Lady .10.08.15.Mix.Recall 2")
        .item("Hey.Lady.83.5.BPM.10.08.15.Mix.Recall.R.wav")
        .end()
        .folder("Unsorted")
        .track("AD.Evil Metal.Tama 1")
        .item("AD.Evil Metal.Tama.83.5BPM.L.wav")
        .track("AD.Evil Metal.Tama 2")
        .item("AD.Evil Metal.Tama.83.5BPM.R.wav")
        .track("AD.Great.Rock.Sanre.Pearl 1")
        .item("AD.Great.Rock.Sanre.Pearl.83.5BPM.L.wav")
        .track("AD.Great.Rock.Sanre.Pearl 2")
        .item("AD.Great.Rock.Sanre.Pearl.83.5BPM.R.wav")
        .track("AD.Great.Rock.Sanre.Pearl.Grace 1")
        .item("AD.Great.Rock.Sanre.Pearl.Grace.83.5BPM.L.wav")
        .track("AD.Great.Rock.Sanre.Pearl.Grace 2")
        .item("AD.Great.Rock.Sanre.Pearl.Grace.83.5BPM.R.wav")
        .track("AD.Hard.Rock.Tama 1")
        .item("AD.Hard.Rock.Tama.83.5BPM.L.wav")
        .track("AD.Hard.Rock.Tama 2")
        .item("AD.Hard.Rock.Tama.83.5BPM.R.wav")
        .track("AD.Hard.Rock.Tama.Grace 1")
        .item("AD.Hard.Rock.Tama.Grace.83.5BPM.L.wav")
        .track("AD.Hard.Rock.Tama.Grace 2")
        .item("AD.Hard.Rock.Tama.Grace.83.5BPM.R.wav")
        .track("Echo.Plex.Move")
        .item("Echo.Plex.Move.83.5BPM.wav")
        .track("Flr.27.Trick")
        .item("Flr.27.Trick.83.5BPM.wav")
        .end()
        .build()
}

#[test]
fn steve_maggiora_hey_lady() {
    // -- Setup & Fixtures
    // Steve Maggiora - Hey Lady: Complex session with AD samples, saxes, and H3000 effects at 83.5 BPM
    let items = vec![
        "AD.Big Kit Gretch.83.5BPM.L.wav",
        "AD.Big Kit Gretch.83.5BPM.R.wav",
        "AD.Big Kit Gretch.Grace.83.5BPM.L.wav",
        "AD.Big Kit Gretch.Grace.83.5BPM.R.wav",
        "AD.Bootstraps.Kick.83.5BPM.L.wav",
        "AD.Bootstraps.Kick.83.5BPM.R.wav",
        "AD.Evil Metal.Tama.83.5BPM.L.wav",
        "AD.Evil Metal.Tama.83.5BPM.R.wav",
        "AD.Great.Rock.Sanre.Pearl.83.5BPM.L.wav",
        "AD.Great.Rock.Sanre.Pearl.83.5BPM.R.wav",
        "AD.Great.Rock.Sanre.Pearl.Grace.83.5BPM.L.wav",
        "AD.Great.Rock.Sanre.Pearl.Grace.83.5BPM.R.wav",
        "AD.Hard.Rock.Tama.83.5BPM.L.wav",
        "AD.Hard.Rock.Tama.83.5BPM.R.wav",
        "AD.Hard.Rock.Tama.Grace.83.5BPM.L.wav",
        "AD.Hard.Rock.Tama.Grace.83.5BPM.R.wav",
        "AD.Metal.Kick.Print.83.5BPM.L.wav",
        "AD.Metal.Kick.Print.83.5BPM.R.wav",
        "B3.83.5BPM.C.wav",
        "B3.83.5BPM.L.wav",
        "B3.83.5BPM.R.wav",
        "Bass.Di.83.5BPM.wav",
        "Bass.Mic.83.5BPM.wav",
        "Bass.Mic.dup1.83.5BPM.wav",
        "Echo.Plex.Move.83.5BPM.wav",
        "Flr.27.Trick.83.5BPM.wav",
        "Gtr_57.83.5BPM.wav",
        "Gtr_57.dup1.83.5BPM.wav",
        "Gtr_Di.83.5BPM.wav",
        "H3000.One.New.83.5BPM.L.wav",
        "H3000.One.New.83.5BPM.R.wav",
        "H3000.Three.New.83.5BPM.L.wav",
        "H3000.Three.New.83.5BPM.R.wav",
        "H3000.Two.New.83.5BPM.L.wav",
        "H3000.Two.New.83.5BPM.R.wav",
        "Hammond Hi.dup1.83.5BPM.L.wav",
        "Hammond Hi.dup1.83.5BPM.R.wav",
        "Hey.Lady.83.5.BPM.10.08.15.Mix.Recall.L.wav",
        "Hey.Lady.83.5.BPM.10.08.15.Mix.Recall.R.wav",
        "Hi Hat.83.5BPM.wav",
        "Kick.83.5BPM.wav",
        "OH.83.5BPM.L.wav",
        "OH.83.5BPM.R.wav",
        "Rack.27.Trick.83.5BPM.wav",
        "sax verb.83.5BPM.L.wav",
        "sax verb.83.5BPM.R.wav",
        "sax verb2.83.5BPM.L.wav",
        "sax verb2.83.5BPM.R.wav",
        "sax verb3.83.5BPM.L.wav",
        "sax verb3.83.5BPM.R.wav",
        "sax1.83.5BPM.wav",
        "sax2.83.5BPM.wav",
        "sax3.83.5BPM.wav",
        "SN Bot.83.5BPM.wav",
        "SN Top.83.5BPM.wav",
        "Snare D 28_Oct.Pitch.83.5BPM.wav",
        "Snare D 28.83.5BPM.wav",
        "Upright.Forty.Seven.83.5BPM.wav",
        "Upright.Ribbon.83.5BPM.wav",
        "Vocal.Comp.83.5BPM.wav",
        "Vocal.Dub.83.5BPM.wav",
        "Vocal.Eko.Plate.83.5BPM.L.wav",
        "Vocal.Eko.Plate.83.5BPM.R.wav",
        "Vocal.Magic.83.5BPM.L.wav",
        "Vocal.Magic.83.5BPM.R.wav",
    ];

    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = steve_maggiora_hey_lady_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
