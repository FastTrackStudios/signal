//! Print MIDI region events with positions and durations.

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let session = dawfile_protools::read_session(&path, 0).unwrap();
    for region in &session.midi_regions {
        println!(
            "region {:?} events={} length={}",
            region.name,
            region.events.len(),
            region.length
        );
        for (i, e) in region.events.iter().take(20).enumerate() {
            println!(
                "  [{i:02}] pos={} dur={} note={} vel={}",
                e.position, e.duration, e.note, e.velocity
            );
        }
    }
}
