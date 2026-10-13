fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    for t in s.all_tracks() {
        println!(
            "name={:<10} vol={:>5} mute={} solo={} color=0x{:02x} pan={:>4} idx={}",
            t.name, t.volume_centibel, t.mute, t.solo, t.color_byte, t.pan, t.index
        );
    }
    Ok(())
}
