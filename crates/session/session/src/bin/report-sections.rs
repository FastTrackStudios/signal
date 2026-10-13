//! Offline batch report: for each `.RPP`, list the Sections-lane regions in
//! order and confirm each name parses as a valid keyflow `SectionType` — the
//! same parse (`parse_region_section_type`) the live setlist-building path
//! runs, so a name that fails here is a section that would silently not
//! show up in the performance view.
//!
//! ```text
//! report-sections <in.rpp>...
//! ```

use std::env;

use daw::service::{ProjectContext, Regions};
use session::keyflow::actions::parse_region_section_type;
use session_proto::ruler_lanes::CoreLane;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: report-sections <in.rpp>...");
        std::process::exit(2);
    }

    let mut any_invalid = false;
    for input in &args {
        match report_one(input) {
            Ok(had_invalid) => any_invalid |= had_invalid,
            Err(err) => {
                eprintln!("error: {input}: {err}");
                any_invalid = true;
            }
        }
    }
    if any_invalid {
        std::process::exit(1);
    }
}

fn report_one(input: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let project = dawfile_reaper::io::read_project(input)?;
    let name = std::path::Path::new(input)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("Song");

    let daw = session::keyflow::offline::OfflineDaw::new(project, name);
    let sections_lane = CoreLane::Sections.lane_index();
    let mut regions: Vec<_> = Regions::all(&daw, ProjectContext::Current)
        .into_iter()
        .filter(|r| r.lane == Some(sections_lane))
        .collect();
    regions.sort_by(|a, b| a.start_seconds().total_cmp(&b.start_seconds()));

    println!("\n== {name} ({} sections) ==", regions.len());
    let mut had_invalid = false;
    for region in &regions {
        let parsed = parse_region_section_type(&region.name);
        let status = match &parsed {
            Some(section_type) => format!("{section_type:?}"),
            None => {
                had_invalid = true;
                "INVALID — will not show up".to_string()
            }
        };
        println!(
            "  {:>7.1}s - {:>7.1}s  {:<16} {}",
            region.start_seconds(),
            region.end_seconds(),
            region.name,
            status
        );
    }
    if regions.is_empty() {
        println!("  (no regions on the Sections lane)");
        had_invalid = true;
    }
    Ok(had_invalid)
}
