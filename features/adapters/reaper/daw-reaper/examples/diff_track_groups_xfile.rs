//! Cross-file byte-range diff (decrypted). Args: a.ptx a_start a_end b.ptx b_start b_end.

fn parse_hex(s: &str) -> Result<usize, std::num::ParseIntError> {
    usize::from_str_radix(s.trim_start_matches("0x"), 16)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let a = dawfile_protools::parse_raw(std::fs::read(&args[1])?)?;
    let a_start = parse_hex(&args[2])?;
    let a_end = parse_hex(&args[3])?;
    let b = dawfile_protools::parse_raw(std::fs::read(&args[4])?)?;
    let b_start = parse_hex(&args[5])?;
    let b_end = parse_hex(&args[6])?;
    let ad = &a.data[a_start..a_end];
    let bd = &b.data[b_start..b_end];
    println!("a={} bytes, b={} bytes", ad.len(), bd.len());
    let n = ad.len().min(bd.len());
    let mut diffs = Vec::new();
    for i in 0..n {
        if ad[i] != bd[i] {
            diffs.push(i);
        }
    }
    println!("differing offsets: {}", diffs.len());
    let mut i = 0;
    while i < diffs.len() {
        let s = diffs[i];
        let mut e = s;
        while i + 1 < diffs.len() && diffs[i + 1] == e + 1 {
            i += 1;
            e = diffs[i];
        }
        let av: Vec<String> = (s..=e).map(|o| format!("{:02x}", ad[o])).collect();
        let bv: Vec<String> = (s..=e).map(|o| format!("{:02x}", bd[o])).collect();
        println!(
            "  +{s:>5} (len {:>3}): a=[{}] b=[{}]",
            e - s + 1,
            av.join(" "),
            bv.join(" ")
        );
        i += 1;
    }
    Ok(())
}
