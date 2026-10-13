use dawfile_protools::write::{NativeTrackSpec, write_single_track_ptx};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for (label, name) in [
        ("mute_default", "ProbeTrack"),
        ("mute_renamed_long", "ProbeTrackLongerName"),
    ] {
        let spec = NativeTrackSpec {
            name: name.to_string(),
            mute: true,
            ..NativeTrackSpec::default()
        };
        let bytes = write_single_track_ptx(&spec)?;
        let path = std::env::temp_dir().join(format!("{label}.ptx"));
        std::fs::write(&path, &bytes)?;
        println!("wrote {} ({} bytes)", path.display(), bytes.len());
    }
    Ok(())
}
