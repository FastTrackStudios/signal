fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let out_path = std::env::args().nth(2).unwrap();
    std::fs::write(&out_path, &s.data)?;
    eprintln!("wrote {} bytes", s.data.len());
    Ok(())
}
