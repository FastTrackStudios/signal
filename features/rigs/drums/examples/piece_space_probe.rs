//! Build the drum library's piece similarity space and query it (#77 M4).
//!   cargo run --release -p signal-drums --example piece_space_probe [library-root]

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let root = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2")
    });
    let t0 = std::time::Instant::now();
    let (dir, analyzed, skipped) = signal_drums::piece_space::build(&root)?;
    println!(
        "built piece space: {analyzed} engines ({skipped} skipped) in {:.1}s → {}",
        t0.elapsed().as_secs_f32(),
        dir.display()
    );

    // Query: for each kind, show one piece's nearest neighbours.
    let (space, _) = signal_space::Space::load(&dir)?;
    let mut seen: Vec<&str> = Vec::new();
    for item in &space.items {
        if seen.contains(&item.class.as_str()) {
            continue;
        }
        seen.push(&item.class);
        let full = std::path::Path::new(&space.root).join(&item.path);
        let hits = match signal_drums::piece_space::similar_to(&root, &full, 4) {
            Ok(h) => h,
            Err(e) => {
                println!("   (similar_to error: {e})");
                Vec::new()
            }
        };
        println!("\n[{}] {}", item.class, item.path);
        for (path, score) in hits {
            let short = path.rsplit('/').next().unwrap_or(&path).to_string();
            println!("   {score:.3}  {short}");
        }
    }
    Ok(())
}
