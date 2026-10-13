//! Compare two byte ranges and report differing offsets (relative to range start).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let a_start = usize::from_str_radix(
        std::env::args().nth(2).unwrap().trim_start_matches("0x"),
        16,
    )?;
    let a_end = usize::from_str_radix(
        std::env::args().nth(3).unwrap().trim_start_matches("0x"),
        16,
    )?;
    let b_start = usize::from_str_radix(
        std::env::args().nth(4).unwrap().trim_start_matches("0x"),
        16,
    )?;
    let b_end = usize::from_str_radix(
        std::env::args().nth(5).unwrap().trim_start_matches("0x"),
        16,
    )?;

    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let data = &s.data;
    let alen = a_end - a_start;
    let blen = b_end - b_start;
    println!(
        "a: 0x{a_start:06x}..0x{a_end:06x} ({alen} bytes)\nb: 0x{b_start:06x}..0x{b_end:06x} ({blen} bytes)"
    );
    let n = alen.min(blen);
    let mut diffs = Vec::new();
    for i in 0..n {
        let av = data[a_start + i];
        let bv = data[b_start + i];
        if av != bv {
            diffs.push((i, av, bv));
        }
    }
    println!("differing offsets: {}", diffs.len());
    // Group consecutive diffs
    let mut i = 0;
    while i < diffs.len() {
        let start = diffs[i].0;
        let mut end = start;
        while i + 1 < diffs.len() && diffs[i + 1].0 == end + 1 {
            i += 1;
            end = diffs[i].0;
        }
        let a_bytes: Vec<String> = (start..=end)
            .map(|o| format!("{:02x}", data[a_start + o]))
            .collect();
        let b_bytes: Vec<String> = (start..=end)
            .map(|o| format!("{:02x}", data[b_start + o]))
            .collect();
        println!(
            "  +{start:>5} (len {:>3}): a=[{}] b=[{}]",
            end - start + 1,
            a_bytes.join(" "),
            b_bytes.join(" ")
        );
        i += 1;
    }
    Ok(())
}
