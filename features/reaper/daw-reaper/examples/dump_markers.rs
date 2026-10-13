fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("=== {} markers ===", s.markers.len());
    for m in &s.markers {
        println!(
            "  #{:>3} {:<30} tick={:>10} sample={:>10} color={:?}",
            m.number, m.name, m.tick_pos, m.sample_pos, m.color_rgb
        );
    }
    Ok(())
}
