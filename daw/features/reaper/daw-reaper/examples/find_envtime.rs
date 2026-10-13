fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let needle: [u8; 4] = 48000u32.to_le_bytes();
    let mut start = 0;
    while let Some(p) = s.data[start..].windows(4).position(|w| w == needle) {
        let abs = start + p;
        println!("found 48000 at 0x{:06x}", abs);
        start = abs + 4;
    }
    Ok(())
}
