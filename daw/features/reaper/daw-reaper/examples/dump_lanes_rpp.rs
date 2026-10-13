//! Dump a synthetic 3-playlist (Pro Tools alternate playlists) project to a
//! REAPER .rpp file so a human can open it in REAPER and confirm the fixed
//! item lanes look right.
//!
//! Usage: cargo run -p daw-reaper --example dump_lanes_rpp
//!
//! Writes /tmp/pt_playlists_demo.rpp and prints the path.

use daw_reaper::project_import::build_demo_playlist_project;
use dawfile_reaper::{ReaperProject, RppSerialize};

fn main() {
    // Same conversion seam the tests use: synthetic ProToolsSession ->
    // ReaperProject in fixed-lane (comp) mode.
    let project: ReaperProject = build_demo_playlist_project();
    let rpp = project.to_rpp_string();

    let out = "/tmp/pt_playlists_demo.rpp";
    std::fs::write(out, &rpp).expect("write demo .rpp");
    println!("Wrote {out}");
}
