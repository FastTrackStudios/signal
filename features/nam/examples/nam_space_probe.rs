//! Build the NAM model space and query it (#77 M5).
//!   cargo run --release -p signal-nam --example nam_space_probe [nam-root]

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/signal/rig/models")
    });
    let t0 = std::time::Instant::now();
    let (dir, probed, skipped) = signal_nam::space::build(&root)?;
    println!(
        "probed {probed} models ({skipped} skipped) in {:.1}s → {}\n",
        t0.elapsed().as_secs_f32(),
        dir.display()
    );

    let (space, _) = signal_space::Space::load(&dir)?;
    for item in &space.items {
        println!(
            "  [{:<12}] {:>6.0} Hz  out {:>6.1} dB  knee {:.2}  {}",
            item.class, item.centroid_hz, item.rms_db, item.percussiveness, item.path
        );
    }

    if let Some(first) = space.items.first() {
        let full = std::path::Path::new(&space.root).join(&first.path);
        println!("\nsimilar to {}:", first.path);
        for (path, score) in signal_nam::space::similar_to(&root, &full, 4)? {
            println!("   {score:.3}  {path}");
        }
        println!("\npartners for {} (stereo complements):", first.path);
        for (path, score) in signal_nam::space::partner_for(&root, &full, 4)? {
            println!("   {score:.3}  {path}");
        }
    }
    Ok(())
}
