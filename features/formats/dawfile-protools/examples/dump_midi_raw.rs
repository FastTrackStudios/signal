//! Dump raw 35-byte MIDI event records.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

const MIDI_MAGIC: &[u8] = b"MdNLB";

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();
    let blocks = collect(&session.blocks, ContentType::MidiEventsBlock);
    println!("{} MidiEventsBlock", blocks.len());

    for (idx, b) in blocks.iter().take(20).enumerate() {
        let block_end = (b.start + 9 + b.block_size as usize).min(data.len());
        let mut search = b.start;
        let mut chunk = 0;
        while let Some(magic) = find_magic(data, search, block_end) {
            search = magic + MIDI_MAGIC.len();
            let n = u32::from_le_bytes(data[magic + 11..magic + 15].try_into().unwrap()) as usize;
            let events_start = magic + 23;
            println!("\nblock[{idx}] chunk[{chunk}] magic@0x{magic:06x} n_events={n}");
            for i in 0..n.min(4) {
                let off = events_start + i * 35;
                if off + 35 > data.len() {
                    break;
                }
                let bytes = &data[off..off + 35];
                let hex: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                println!("  [{i:02}] {}", hex.join(" "));
            }
            chunk += 1;
            if chunk >= 10 {
                break;
            }
        }
    }
}

fn find_magic(data: &[u8], start: usize, end: usize) -> Option<usize> {
    if end <= start || end > data.len() {
        return None;
    }
    let slice = &data[start..end];
    slice
        .windows(MIDI_MAGIC.len())
        .position(|w| w == MIDI_MAGIC)
        .map(|p| start + p)
}

fn collect(blocks: &[RawBlock], ct: ContentType) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(blocks: &'a [RawBlock], ct: ContentType, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type == Some(ct) {
                out.push(b);
            }
            rec(&b.children, ct, out);
        }
    }
    rec(blocks, ct, &mut out);
    out
}
