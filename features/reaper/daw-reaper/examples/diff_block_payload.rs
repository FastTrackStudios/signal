//! Diff one specific block (by content_type, first occurrence) between
//! two PTX files. Useful for isolating envelope encoding when blocks
//! differ in size.

use dawfile_protools::raw_block::RawBlock;

fn first_block_of_ct(blocks: &[RawBlock], ct: u16) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type_raw == ct {
            return Some(b);
        }
        if let Some(x) = first_block_of_ct(&b.children, ct) {
            return Some(x);
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a_path = std::env::args().nth(1).expect("a.ptx");
    let b_path = std::env::args().nth(2).expect("b.ptx");
    let ct: u16 = u16::from_str_radix(
        std::env::args()
            .nth(3)
            .expect("ct (hex, e.g. 0002)")
            .trim_start_matches("0x"),
        16,
    )?;

    let a = dawfile_protools::parse_raw(std::fs::read(&a_path)?)?;
    let b = dawfile_protools::parse_raw(std::fs::read(&b_path)?)?;
    let ad = a.cursor().data();
    let bd = b.cursor().data();

    let ab = first_block_of_ct(&a.blocks, ct).expect("a no block");
    let bb = first_block_of_ct(&b.blocks, ct).expect("b no block");

    let a_payload = ab.start + 9;
    let b_payload = bb.start + 9;
    let a_size = (ab.block_size as usize).saturating_sub(2);
    let b_size = (bb.block_size as usize).saturating_sub(2);
    println!(
        "block 0x{ct:04x}  a size={a_size}  b size={b_size}  delta={}",
        b_size as i64 - a_size as i64
    );

    // Common prefix in the block payload
    let mut prefix = 0usize;
    while prefix < a_size && prefix < b_size && ad[a_payload + prefix] == bd[b_payload + prefix] {
        prefix += 1;
    }
    // Common suffix
    let mut suffix = 0usize;
    while suffix < a_size - prefix
        && suffix < b_size - prefix
        && ad[a_payload + a_size - 1 - suffix] == bd[b_payload + b_size - 1 - suffix]
    {
        suffix += 1;
    }

    println!("common prefix in payload: {prefix}");
    println!("common suffix in payload: {suffix}");

    let a_mid_start = a_payload + prefix;
    let a_mid_end = a_payload + a_size - suffix;
    let b_mid_start = b_payload + prefix;
    let b_mid_end = b_payload + b_size - suffix;
    let a_mid_len = a_mid_end - a_mid_start;
    let b_mid_len = b_mid_end - b_mid_start;
    println!("a middle: {a_mid_len} bytes, b middle: {b_mid_len} bytes");

    // Print up to 80 bytes of each
    let print_n_a = a_mid_len.min(80);
    let print_n_b = b_mid_len.min(80);
    let a_hex: Vec<String> = (a_mid_start..a_mid_start + print_n_a)
        .map(|i| format!("{:02x}", ad[i]))
        .collect();
    let b_hex: Vec<String> = (b_mid_start..b_mid_start + print_n_b)
        .map(|i| format!("{:02x}", bd[i]))
        .collect();
    println!("a middle: {}", a_hex.join(" "));
    println!("b middle: {}", b_hex.join(" "));
    Ok(())
}
