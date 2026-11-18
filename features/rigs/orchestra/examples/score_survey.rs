//! Survey orchestral scores: load each `.mxl` / `.musicxml` with keyflow's
//! orchestra parser and list its parts, note counts and tempo map — the first
//! check that a piece can go through the score → audio path at all.
//!
//! ```text
//! cargo run -p signal-orchestra --example score_survey -- ~/Downloads/"Orchestral Scores"
//! ```

use std::path::{Path, PathBuf};

fn scores_in(dir: &Path) -> eyre::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "mxl" | "musicxml" | "xml"))
        })
        .collect();
    out.sort();
    Ok(out)
}

fn main() -> eyre::Result<()> {
    let arg = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("usage: score_survey <dir|file>"))?;
    let path = PathBuf::from(arg);
    let files = if path.is_dir() {
        scores_in(&path)?
    } else {
        vec![path]
    };
    let mut failed = 0usize;
    for f in &files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        match keyflow_lang::orchestra::score::load(f) {
            Ok(sc) => {
                let notes: usize = sc.parts.iter().map(|p| p.notes.len()).sum();
                println!(
                    "OK   {name}: {} parts, {notes} notes, {} tempo points",
                    sc.parts.len(),
                    sc.meta.tempos.len()
                );
                for p in &sc.parts {
                    println!("       - {:<32} {:>5} notes", p.name, p.notes.len());
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {name}: {e}");
            }
        }
    }
    println!("{} scores, {failed} failed", files.len());
    Ok(())
}
