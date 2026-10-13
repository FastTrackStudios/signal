fn main() {
    let path = std::env::args().nth(1).expect("path");
    let s = dawfile_protools::read_session(&path, 48000).unwrap();
    for t in s.all_tracks() {
        println!(
            "{:<20} mute={} inact={} solo={} sdef={} color=0x{:02x} fld={} vol={:>5} pan={:>4} out={:?}",
            t.name,
            t.mute as u8,
            t.inactive as u8,
            t.solo as u8,
            t.solo_defeat as u8,
            t.color_byte,
            t.is_folder as u8,
            t.volume_centibel,
            t.pan,
            t.output,
        );
    }
}
