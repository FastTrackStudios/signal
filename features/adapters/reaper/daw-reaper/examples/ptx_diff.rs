//! Byte-diff two .ptx files of identical size. Reports each differing byte
//! position with both values. Useful for identifying which PTX bytes encode
//! a single feature (run probes with `rpp_to_ptx_probe`, then diff against
//! `baseline.ptx`).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a_path = std::env::args().nth(1).expect("a.ptx");
    let b_path = std::env::args().nth(2).expect("b.ptx");

    let a = std::fs::read(&a_path)?;
    let b = std::fs::read(&b_path)?;

    if a.len() != b.len() {
        eprintln!("files differ in size: {} vs {}", a.len(), b.len());
    }

    let n = a.len().min(b.len());
    let mut diffs: Vec<(usize, u8, u8)> = Vec::new();
    for i in 0..n {
        if a[i] != b[i] {
            diffs.push((i, a[i], b[i]));
        }
    }

    println!(
        "{} differing bytes between {} and {}",
        diffs.len(),
        a_path,
        b_path
    );

    if diffs.is_empty() {
        return Ok(());
    }

    // Group adjacent diffs into runs
    let mut runs: Vec<(usize, usize)> = Vec::new(); // (start, len)
    let mut cur_start = diffs[0].0;
    let mut cur_end = cur_start;
    for &(off, _, _) in &diffs[1..] {
        if off == cur_end + 1 {
            cur_end = off;
        } else {
            runs.push((cur_start, cur_end - cur_start + 1));
            cur_start = off;
            cur_end = off;
        }
    }
    runs.push((cur_start, cur_end - cur_start + 1));

    println!("{} runs:", runs.len());
    for (start, len) in &runs {
        let end = start + len;
        let a_slice: Vec<String> = (*start..end).map(|i| format!("{:02x}", a[i])).collect();
        let b_slice: Vec<String> = (*start..end).map(|i| format!("{:02x}", b[i])).collect();
        println!(
            "  off=0x{:06x} len={:>3}  a={}  b={}",
            start,
            len,
            a_slice.join(" "),
            b_slice.join(" ")
        );
    }

    Ok(())
}
