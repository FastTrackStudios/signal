use dawfile_protools::raw_block::{RawBlock, RawSession};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: dump_send_byte <ptx>");
    let raw = std::fs::read(&path)?;
    let session: RawSession = dawfile_protools::parse_raw(raw)?;
    let parsed = dawfile_protools::read_session(&path, 48000)?;

    fn collect<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            collect(&b.children, ct, out);
        }
    }
    let mut wrappers: Vec<&RawBlock> = Vec::new();
    collect(&session.blocks, 0x260d, &mut wrappers);
    println!("found {} 0x260d wrappers in file", wrappers.len());

    // Print every 0x260d wrapper's first 0x260a child's +8 byte
    println!("0x260d index → 0x260a[0] +8 byte (interpret send-routing flag):");
    for (i, w) in wrappers.iter().enumerate() {
        let send = w
            .children
            .iter()
            .find(|c| c.content_type_raw == 0x260a)
            .and_then(|a| session.data.get(a.start + 9 + 8).copied())
            .unwrap_or(0xff);
        println!("  [{i:>2}] send@260a[0]+8=0x{send:02x}");
    }

    println!("\nparsed Track records (audio + midi, in parse order):");
    for (i, t) in parsed
        .audio_tracks
        .iter()
        .chain(parsed.midi_tracks.iter())
        .enumerate()
    {
        println!(
            "  [{i:>2}] vol={:>+5}cB mute={:>5} inactive={:>5}  {}",
            t.volume_centibel, t.mute, t.inactive, t.name
        );
    }
    Ok(())
}
