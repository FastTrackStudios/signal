fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let start = usize::from_str_radix(
        std::env::args().nth(2).unwrap().trim_start_matches("0x"),
        16,
    )?;
    let len: usize = std::env::args().nth(3).unwrap().parse()?;
    let bytes = &s.data[start..start + len];
    for (i, b) in bytes.iter().enumerate() {
        if i % 16 == 0 {
            print!("  +{i:>3}: ");
        }
        print!("{:02x} ", b);
        if i % 16 == 15 {
            println!();
        }
    }
    println!();
    Ok(())
}
