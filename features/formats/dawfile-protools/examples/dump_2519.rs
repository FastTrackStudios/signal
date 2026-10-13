//! Walk 0x2519 and print each child 0x251a's name and embedded 0x1029 vol.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let cursor = session.cursor();
    let data = cursor.data();

    let track_list = find(&session.blocks, ContentType::MidiTrackList).expect("no 0x2519");

    println!("0x2519 has {} children", track_list.children.len());
    let mut idx = 0;
    for c in &track_list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        // payload begins at c.start + 9; name length-prefixed string at payload + 4
        // Block.offset = start+7 (content_type); name @ Block.offset + 4 = start + 11
        let name_off = c.start + 11;
        let (name, _) = cursor.length_prefixed_string(name_off);
        let mix = find_mix(c, data);
        // +2 byte holds the track-kind discriminator per roadmap §5
        let kind_byte = if c.start + 7 + 2 < data.len() {
            Some(data[c.start + 7 + 2])
        } else {
            None
        };
        println!(
            "  [{idx:02}] kind=0x{:02x} name={:?}  0x1029(vol,mute,pan)={:?}",
            kind_byte.unwrap_or(0xff),
            name,
            mix
        );
        idx += 1;
    }
}

fn find(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(x) = find(&b.children, ct) {
            return Some(x);
        }
    }
    None
}

fn find_mix(b: &RawBlock, data: &[u8]) -> Option<(i32, u8, i32)> {
    if b.content_type == Some(ContentType::TrackMixSettings) {
        let payload = b.start + 9;
        if payload + 17 > data.len() {
            return None;
        }
        let vol = i32::from_le_bytes(data[payload + 1..payload + 5].try_into().unwrap());
        let mute = data[payload + 5];
        let pan = i32::from_le_bytes(data[payload + 13..payload + 17].try_into().unwrap());
        return Some((vol, mute, pan));
    }
    for c in &b.children {
        if let Some(v) = find_mix(c, data) {
            return Some(v);
        }
    }
    None
}
