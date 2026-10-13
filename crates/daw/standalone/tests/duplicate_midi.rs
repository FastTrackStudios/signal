//! A duplicated MIDI item plays what the original does: its take's notes
//! come with it. A split is a duplicate trimmed to the right half, so
//! without them every note after a cut was lost.

use daw_proto::midi::{Midi, MidiNoteCreate};
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, Items, MidiTakeLocation, ProjectInfo, TakeRef, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

#[test]
fn a_duplicated_midi_item_keeps_its_notes() {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "DRUM MIDI", None).expect("a track");
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 8.0)
        .expect("a MIDI item");
    for beat in 0..8_u32 {
        Midi::add_note(
            &daw,
            loc.clone(),
            MidiNoteCreate::new(36, 100, f64::from(beat), 0.25),
        );
    }
    let ItemRef::Guid(item) = &loc.item else {
        panic!("a guid");
    };
    let copy = daw
        .duplicate_item(ctx.clone(), ItemRef::Guid(item.clone()))
        .expect("a copy");
    let notes = |guid: &str| {
        Midi::notes(
            &daw,
            MidiTakeLocation::new(ctx.clone(), ItemRef::Guid(guid.to_owned()), TakeRef::Active),
        )
        .len()
    };
    assert_eq!(notes(item), 8);
    assert_eq!(notes(&copy), 8, "the copy's take has the notes");
}
