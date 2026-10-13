//! Convert a Pro Tools session to a REAPER project file.
//!
//! Usage: `cargo run -p daw-reaper --example pt_to_rpp -- <input.ptx> [output.rpp] [audio-base-dir]`
//!
//! `audio-base-dir` (optional): emit audio FILE paths under this directory's
//! `Audio Files/` instead of the .ptx's own folder — use when the .rpp will be
//! opened on a different machine (pass the session folder as it exists there).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: pt_to_rpp <input.ptx> [output.rpp] [audio-base-dir]")?;
    let output = args.next().unwrap_or_else(|| {
        let stem = std::path::Path::new(&input)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session");
        format!("{stem}.rpp")
    });

    let rpp = match args.next() {
        Some(audio_base) => {
            daw_reaper::project_import::protools_to_rpp_with_audio_base(&input, &audio_base)?
        }
        None => daw_reaper::project_import::protools_to_rpp(&input)?,
    };
    std::fs::write(&output, &rpp)?;
    eprintln!("wrote {} ({} bytes)", output, rpp.len());
    Ok(())
}
