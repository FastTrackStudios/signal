fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/cloned_track.ptx".into());
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let needle = b"\x0a\x00\x00\x00ProbeTrack";
    let n = s
        .data
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count();
    println!("ProbeTrack occurrences: {n}");
    // Where are they?
    let mut start = 0;
    while let Some(p) = s.data[start..]
        .windows(needle.len())
        .position(|w| w == needle)
    {
        println!("  at 0x{:06x}", start + p);
        start = start + p + needle.len();
    }
    Ok(())
}
