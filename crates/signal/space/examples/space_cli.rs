//! Space CLI — build/audit/similar until `fts signal space` absorbs it.
//!
//! ```bash
//! cargo run --release -p signal-space --example space_cli -- build <root> [name] [--pieces]
//! cargo run --release -p signal-space --example space_cli -- audit <root>/Space/<name>.space
//! cargo run --release -p signal-space --example space_cli -- similar <space-dir> <substr> [k]
//! ```

use std::path::{Path, PathBuf};

use signal_space::{Space, build, knn};

fn main() {
    tracing_subscriber_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("build") => {
            let root = PathBuf::from(args.get(1).expect("usage: build <root> [name]"));
            let name = args
                .get(2)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .or_else(|| root.file_name().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "space".into());
            let dir = Space::space_dir(&root, &name);
            let previous = Space::load(&dir).ok();
            let t0 = std::time::Instant::now();
            let granularity = if args.iter().any(|a| a == "--pieces") {
                build::Granularity::Piece
            } else {
                build::Granularity::Sample
            };
            let report = build::build(
                &name,
                &root,
                granularity,
                previous.as_ref().map(|(s, f)| (s, f.as_slice())),
                &|n, total| eprintln!("  analyzed {n}/{total}"),
            );
            report
                .space
                .save(&dir, &report.features)
                .expect("save space");
            println!(
                "built {:?}: {} items ({} analyzed, {} reused, {} failed) in {:.1}s → {}",
                name,
                report.space.items.len(),
                report.analyzed,
                report.reused,
                report.failed.len(),
                t0.elapsed().as_secs_f32(),
                dir.display()
            );
            for (p, e) in report.failed.iter().take(10) {
                eprintln!("  FAILED {}: {e}", p.display());
            }
            audit(&report.space);
        }
        Some("audit") => {
            let (space, _) = Space::load(Path::new(args.get(1).expect("usage: audit <space-dir>")))
                .expect("load space");
            audit(&space);
        }
        Some("similar") => {
            let dir = PathBuf::from(
                args.get(1)
                    .expect("usage: similar <space-dir> <substr> [k]"),
            );
            let pat = args
                .get(2)
                .expect("usage: similar <space-dir> <substr> [k]")
                .to_lowercase();
            let k: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
            let (space, feats) = Space::load(&dir).expect("load space");
            let idx = space
                .items
                .iter()
                .position(|i| i.path.to_lowercase().contains(&pat))
                .expect("no item matches substring");
            let q = &space.items[idx];
            println!(
                "query: [{}] {} ({:.0} Hz, {:.2}s)",
                q.class, q.path, q.centroid_hz, q.duration_s
            );
            for (i, score) in knn::similar(&feats, space.dim, idx, k, |_| true) {
                let it = &space.items[i];
                println!("  {score:.3}  [{}] {}", it.class, it.path);
            }
        }
        _ => eprintln!("usage: space_cli build|audit|similar …"),
    }
}

fn audit(space: &Space) {
    let mut by_class: std::collections::BTreeMap<&str, usize> = Default::default();
    for it in &space.items {
        *by_class.entry(it.class.as_str()).or_default() += 1;
    }
    println!("classes:");
    for (c, n) in by_class {
        println!("  {c:<12} {n}");
    }
}

fn tracing_subscriber_init() {
    // Keep the example dependency-light: env_logger-style output via tracing
    // is unnecessary here; stderr prints suffice.
}
