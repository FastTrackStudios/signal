//! Demo the native PTX writer. Writes a few specs to /tmp/ and prints
//! the path + size + a quick re-parse sanity check.

use dawfile_protools::write::{NativeTrackSpec, write_single_track_ptx};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let specs = [
        ("default", NativeTrackSpec::default()),
        (
            "renamed",
            NativeTrackSpec {
                name: "MyTrack".to_string(),
                ..NativeTrackSpec::default()
            },
        ),
        (
            "colored",
            NativeTrackSpec {
                color: 0x18,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "muted",
            NativeTrackSpec {
                mute: true,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "soloed",
            NativeTrackSpec {
                solo: true,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "full",
            NativeTrackSpec {
                name: "FullSpec".to_string(),
                color: 0x18,
                mute: false,
                solo: true,
                volume_centibel: -60,
                pan: 50,
                ..NativeTrackSpec::default()
            },
        ),
    ];

    for (label, spec) in &specs {
        let bytes = write_single_track_ptx(spec)?;
        let path = std::env::temp_dir().join(format!("native_{label}.ptx"));
        std::fs::write(&path, &bytes)?;
        let session = dawfile_protools::read_session(path.to_str().unwrap(), 48000)?;
        let t = session.all_tracks().find(|t| t.name == spec.name);
        println!(
            "[{label:10}] {} ({} bytes) — track name={:?} color=0x{:02x} mute={} solo={} vol={} pan={}",
            path.display(),
            bytes.len(),
            t.map(|t| t.name.as_str()).unwrap_or("?"),
            t.map(|t| t.color_byte).unwrap_or(0),
            t.map(|t| t.mute as u8).unwrap_or(0),
            t.map(|t| t.solo as u8).unwrap_or(0),
            t.map(|t| t.volume_centibel).unwrap_or(0),
            t.map(|t| t.pan).unwrap_or(0),
        );
    }
    Ok(())
}
