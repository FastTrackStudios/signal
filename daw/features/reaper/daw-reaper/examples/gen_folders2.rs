//! Richer folder ground-truth. Shape:
//!
//!   F1                (folder, depth1)
//!     F1a             (folder, depth2)
//!       leaf1         (leaf d3)
//!     leaf2           (leaf d2, closes F1a)
//!   F2                (folder d1, closes F1 then opens F2) -> sibling folder
//!     leaf3           (leaf d2, closes F2)
//!   leafTop           (leaf d1, top level)

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/folders2.rpp".into());
    let project = ReaperProjectBuilder::new()
        .sample_rate(48000)
        .tempo_with_time_sig(120.0, 4, 4)
        // Folder F1 with two children, fully closed; then sibling folder F2.
        .track("F1", |t| t.folder_start())
        .track("leafA", |t| t)
        .track("leafB", |t| t.folder_end(1)) // closes F1
        .track("F2", |t| t.folder_start())
        .track("leafC", |t| t)
        .track("leafD", |t| t.folder_end(1)) // closes F2
        .track("leafTop", |t| t)
        .build();
    let rpp = project.to_rpp_string();
    std::fs::write(&out, &rpp)?;
    eprintln!("wrote {out}");
    Ok(())
}
