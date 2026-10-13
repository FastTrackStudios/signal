// Trace solo+rename interaction: write 4 variants and inspect 0x102d +162
// in each.

use dawfile_protools::write::{NativeTrackSpec, write_single_track_ptx};

fn dump_102d_solo_byte(path: &str) -> Option<u8> {
    let raw = std::fs::read(path).ok()?;
    let session = dawfile_protools::parse_raw(raw).ok()?;
    fn find_102d(
        bs: &[dawfile_protools::raw_block::RawBlock],
    ) -> Option<&dawfile_protools::raw_block::RawBlock> {
        for b in bs {
            if b.content_type_raw == 0x102d {
                return Some(b);
            }
            if let Some(x) = find_102d(&b.children) {
                return Some(x);
            }
        }
        None
    }
    let b = find_102d(&session.blocks)?;
    let p = b.start + 9 + 162;
    let data = session.cursor().data();
    if p >= data.len() {
        return None;
    }
    Some(data[p])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let specs = [
        (
            "soloed_default_name",
            NativeTrackSpec {
                solo: true,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "soloed_renamed_short",
            NativeTrackSpec {
                name: "Short".to_string(), // 5 chars vs 10
                solo: true,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "soloed_renamed_long",
            NativeTrackSpec {
                name: "ProbeTrackLongerName".to_string(), // 20 chars
                solo: true,
                ..NativeTrackSpec::default()
            },
        ),
        (
            "soloed_same_length_name",
            NativeTrackSpec {
                name: "TenCharsXX".to_string(), // 10 chars same length
                solo: true,
                ..NativeTrackSpec::default()
            },
        ),
    ];

    for (label, spec) in &specs {
        let bytes = write_single_track_ptx(spec)?;
        let path = std::env::temp_dir().join(format!("solocombo_{label}.ptx"));
        std::fs::write(&path, &bytes)?;
        let b = dump_102d_solo_byte(path.to_str().unwrap()).unwrap_or(0xFF);
        println!(
            "[{label:30}] name={:<25} 0x102d +162 = 0x{b:02x}",
            spec.name
        );
    }
    Ok(())
}
