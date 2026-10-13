fn main() {
    let path = std::env::args().nth(1).expect("path");
    let s = dawfile_protools::read_session(&path, 48000).unwrap();
    for t in s.all_tracks() {
        println!(
            "{} mute={} solo={} color=0x{:02x}",
            t.name, t.mute as u8, t.solo as u8, t.color_byte
        );
    }
}
