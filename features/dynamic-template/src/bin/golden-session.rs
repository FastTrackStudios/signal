//! Regenerate the golden session fixtures — what `just daw-template` runs.
//!
//! ```text
//! golden-session [FIXTURES_DIR]
//! ```
//!
//! Writes every fixture project file and the media it references under
//! the fixtures directory (`features/dynamic-template/fixtures/golden` by
//! default), and rewrites the checklist section of
//! `docs/spec/session/maximal-template.md` from the golden-rule checks.

use std::path::PathBuf;

use dynamic_template::golden_session::checklist::{self, Golden};
use dynamic_template::golden_session::write_fixtures;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).map_or_else(
        dynamic_template::golden_session::fixtures_dir,
        PathBuf::from,
    );
    let written = write_fixtures(&dir)?;
    for project in &written.projects {
        let text = std::fs::read_to_string(dir.join(project))?;
        let tracks = text
            .lines()
            .filter(|l| l.trim_start().starts_with("<TRACK"))
            .count();
        let items = text
            .lines()
            .filter(|l| l.trim_start().starts_with("<ITEM"))
            .count();
        println!(
            "wrote {} — {tracks} tracks, {items} items",
            dir.join(project).display()
        );
    }
    println!(
        "wrote {} media files under {}",
        written.media.len(),
        dir.join("media").display()
    );

    let doc = checklist::doc_path();
    let before = std::fs::read_to_string(&doc)?;
    let after = checklist::regenerate(&before, &Golden::load(&dir))?;
    if after == before {
        println!("{} is current", doc.display());
    } else {
        std::fs::write(&doc, after)?;
        println!("rewrote the checklist in {}", doc.display());
    }
    Ok(())
}
