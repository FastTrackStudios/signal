//! Scan all 0x2629 audio regions for f32 fields at suspected clip-gain offsets.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let mut regions: Vec<&RawBlock> = Vec::new();
    collect(&session.blocks, ContentType::AudioRegionNew, &mut regions);

    println!("{} regions", regions.len());
    for (i, r) in regions.iter().enumerate() {
        let payload = r.start + 9;
        // Read region name (length-prefixed at payload+0)
        let nlen = u32::from_le_bytes(data[payload..payload + 4].try_into().unwrap()) as usize;
        let name =
            String::from_utf8_lossy(&data[payload + 4..payload + 4 + nlen.min(64)]).to_string();
        // Suspected gain offsets
        let off_a = payload + 0x6b; // try a couple
        let off_b = payload + 0x6f;
        if off_b + 4 > data.len() {
            continue;
        }
        let f_a = f32::from_le_bytes(data[off_a..off_a + 4].try_into().unwrap());
        let f_b = f32::from_le_bytes(data[off_b..off_b + 4].try_into().unwrap());
        // Only print if not the boring 1.0/1.0
        if (f_a - 1.0).abs() > 1e-4 || (f_b - 1.0).abs() > 1e-4 {
            println!("[{i:03}] gain_a={f_a:.4} gain_b={f_b:.4}  name={name}");
        }
    }
}

fn collect<'a>(blocks: &'a [RawBlock], ct: ContentType, out: &mut Vec<&'a RawBlock>) {
    for b in blocks {
        if b.content_type == Some(ct) {
            out.push(b);
        }
        collect(&b.children, ct, out);
    }
}
