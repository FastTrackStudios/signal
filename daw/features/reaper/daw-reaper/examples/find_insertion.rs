//! For two PTX files of slightly different size, find the region where
//! bytes were INSERTED. Aligns the common prefix and suffix and reports
//! the inserted bytes.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a_path = std::env::args().nth(1).expect("a.ptx (smaller)");
    let b_path = std::env::args().nth(2).expect("b.ptx (larger)");

    let a_raw = std::fs::read(&a_path)?;
    let b_raw = std::fs::read(&b_path)?;
    let a = dawfile_protools::parse_raw(a_raw)?;
    let b = dawfile_protools::parse_raw(b_raw)?;
    let ad = a.cursor().data();
    let bd = b.cursor().data();

    if bd.len() < ad.len() {
        eprintln!("note: b is smaller than a; swap args");
        return Ok(());
    }
    let delta = bd.len() - ad.len();
    println!("a size {}  b size {}  delta {}", ad.len(), bd.len(), delta);

    // Find common prefix length
    let mut prefix = 0usize;
    while prefix < ad.len() && prefix < bd.len() && ad[prefix] == bd[prefix] {
        prefix += 1;
    }
    // Find common suffix length
    let mut suffix = 0usize;
    while suffix < ad.len() - prefix
        && suffix < bd.len() - prefix
        && ad[ad.len() - 1 - suffix] == bd[bd.len() - 1 - suffix]
    {
        suffix += 1;
    }

    println!("common prefix: {} bytes (0..0x{prefix:x})", prefix);
    println!("common suffix: {} bytes (last {suffix})", suffix);

    // The "insertion zone" is between [prefix..a.len()-suffix] in A
    // mapped to [prefix..b.len()-suffix] in B.
    let a_middle_end = ad.len() - suffix;
    let b_middle_end = bd.len() - suffix;
    let a_middle_len = a_middle_end - prefix;
    let b_middle_len = b_middle_end - prefix;

    println!(
        "diff window: a[{prefix}..{a_middle_end}] ({a_middle_len} bytes) vs b[{prefix}..{b_middle_end}] ({b_middle_len} bytes)"
    );

    // Print both windows (truncated to ~80 bytes each)
    let n_a = a_middle_len.min(80);
    let n_b = b_middle_len.min(80);
    let a_bytes: Vec<String> = (prefix..prefix + n_a)
        .map(|i| format!("{:02x}", ad[i]))
        .collect();
    let b_bytes: Vec<String> = (prefix..prefix + n_b)
        .map(|i| format!("{:02x}", bd[i]))
        .collect();
    println!("a window: {}", a_bytes.join(" "));
    println!("b window: {}", b_bytes.join(" "));
    Ok(())
}
