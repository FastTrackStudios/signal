//! Query a built NAM space (#77 M5).
//!   cargo run --release -p signal-nam --example nam_query -- <nam-root> <substr> [k]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(
        args.next().ok_or("usage: nam_query <nam-root> <substr> [k]")?,
    );
    let needle = args.next().unwrap_or_default().to_lowercase();
    let k: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let (space, _) = signal_space::Space::load(&signal_space::Space::space_dir(&root, "nam"))?;
    let it = space
        .items
        .iter()
        .find(|i| i.path.to_lowercase().contains(&needle))
        .ok_or("no model matches that substring")?;
    let full = root.join(&it.path);
    println!(
        "[{}] {}  ({:.0} Hz voicing, out {:.1} dB, knee {:.2})",
        it.class, it.path, it.centroid_hz, it.rms_db, it.percussiveness
    );
    println!("\nsimilar:");
    for (p, sc) in signal_nam::space::similar_to(&root, &full, k)? {
        println!("   {sc:.3}  {p}");
    }
    println!("\npartners (stereo complements):");
    for (p, sc) in signal_nam::space::partner_for(&root, &full, k)? {
        println!("   {sc:.3}  {p}");
    }
    Ok(())
}
