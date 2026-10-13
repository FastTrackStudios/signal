fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("=== {} edit groups ===", s.edit_groups.len());
    for (i, g) in s.edit_groups.iter().enumerate().take(20) {
        println!("  [{i:>3}] color={:>5?} name={:?}", g.color, g.name);
    }
    if s.edit_groups.len() > 20 {
        println!("  ... ({} more)", s.edit_groups.len() - 20);
    }
    println!("=== {} stem mappings ===", s.stem_mappings.len());
    for (i, name) in s.stem_mappings.iter().enumerate() {
        println!("  [{i:>3}] {:?}", name);
    }
    println!("=== {} internal tracks ===", s.internal_tracks.len());
    for (i, t) in s.internal_tracks.iter().enumerate() {
        let uid_hex: String = t.routing_uid.iter().map(|b| format!("{b:02x}")).collect();
        println!("  [{i:>3}] uid={uid_hex} name={:?}", t.name);
    }
    Ok(())
}
