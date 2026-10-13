//! Generate an RPP with a KNOWN nested-folder structure for PT round-trip
//! ground-truth. Shape:
//!
//!   FolderA            (folder start, depth 1)
//!     ChildA1          (leaf)
//!     ChildA2          (folder start, depth 2)
//!       GrandA2a       (leaf)
//!       GrandA2b       (folder_end -2 → closes A2 and A)
//!   SiblingB           (leaf, top level)
//!
//! Usage: cargo run -p daw-reaper --example gen_folders -- /tmp/folders.rpp

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/folders.rpp".into());

    let project = ReaperProjectBuilder::new()
        .sample_rate(48000)
        .tempo_with_time_sig(120.0, 4, 4)
        .track("FolderA", |t| t.folder_start())
        .track("ChildA1", |t| t)
        .track("ChildA2", |t| t.folder_start())
        .track("GrandA2a", |t| t)
        .track("GrandA2b", |t| t.folder_end(2))
        .track("SiblingB", |t| t)
        .build();

    let rpp = project.to_rpp_string();
    std::fs::write(&out, &rpp)?;
    eprintln!("wrote {out} ({} bytes)", rpp.len());
    Ok(())
}
