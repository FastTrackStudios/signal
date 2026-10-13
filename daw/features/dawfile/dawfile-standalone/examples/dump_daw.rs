//! Print the `.daw` text for a REAPER project, and what the import did not
//! model.
//!
//! The format's central claim is that it is readable and hand-editable, and
//! the only way to keep that claim honest is to look at the output.
//!
//! ```console
//! cargo run -p dawfile-standalone --example dump_daw -- path/to/Project.RPP
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump_daw <project.RPP>")?;
    let (project, report) = dawfile_standalone::DawProject::import_rpp_file(&path)?;

    println!("{}", project.to_text()?);

    eprintln!("--- not modelled (carried verbatim in objects/) ---");
    for (key, count) in report.ranked() {
        eprintln!("  {key:<32} x{count}");
    }
    eprintln!("chunks carried opaquely: {}", report.opaque_objects);
    Ok(())
}
