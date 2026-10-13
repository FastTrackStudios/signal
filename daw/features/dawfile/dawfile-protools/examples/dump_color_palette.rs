fn main() {
    let path = std::env::args().nth(1).expect("path");
    let s = dawfile_protools::read_session(&path, 48000).unwrap();
    println!("audio tracks:");
    for (i, t) in s.audio_tracks.iter().enumerate() {
        println!(
            "  [{i:02}] color_byte=0x{:02x} ({:>3}) name={}",
            t.color_byte, t.color_byte, t.name
        );
    }
    println!("midi tracks:");
    for (i, t) in s.midi_tracks.iter().enumerate() {
        println!(
            "  [{i:02}] color_byte=0x{:02x} ({:>3}) name={}",
            t.color_byte, t.color_byte, t.name
        );
    }
}
