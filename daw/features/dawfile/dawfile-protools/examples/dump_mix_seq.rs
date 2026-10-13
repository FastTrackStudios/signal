//! Print the raw sequence of 0x1029 blocks in document order with
//! their vol/mute/pan, so we can see if Master/Click prefix the
//! audio-track entries.

use dawfile_protools::content_type::ContentType;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let mut blocks: Vec<usize> = Vec::new();
    collect(&session.blocks, ContentType::TrackMixSettings, &mut blocks);

    println!("found {} 0x1029 blocks in document order:", blocks.len());
    for (i, &start) in blocks.iter().enumerate() {
        // block header is at `start`; content_type=2B at +7; payload begins at start+9
        let payload = start + 9;
        if payload + 17 > data.len() {
            break;
        }
        let vol = i32::from_le_bytes(data[payload + 1..payload + 5].try_into().unwrap());
        let mute = data[payload + 5];
        let pan = i32::from_le_bytes(data[payload + 13..payload + 17].try_into().unwrap());
        println!("  [{i:02}] @0x{start:06x}  vol={vol:>5}  mute={mute}  pan={pan:>4}");
    }
}

fn collect(
    blocks: &[dawfile_protools::raw_block::RawBlock],
    ct: ContentType,
    out: &mut Vec<usize>,
) {
    for b in blocks {
        if b.content_type == Some(ct) {
            out.push(b.start);
        }
        collect(&b.children, ct, out);
    }
}
