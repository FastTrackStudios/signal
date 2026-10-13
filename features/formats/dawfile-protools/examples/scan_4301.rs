//! Scan a directory of .ptx fixtures for any non-zero 0x4301 payload.

fn main() {
    let dir = std::env::args().nth(1).expect("dir");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(ext, "ptx" | "ptf" | "pts") {
            continue;
        }
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };
        let Ok(session) = dawfile_protools::parse_raw(raw) else {
            continue;
        };
        let data = session.cursor().data();
        let blocks = collect_4301(&session.blocks);
        let total = blocks.len();
        let mut nonzero = 0usize;
        let mut samples: Vec<String> = Vec::new();
        for b in &blocks {
            let p = b.start + 9;
            let end = (b.end).min(data.len());
            let payload = &data[p..end];
            if payload.iter().any(|&x| x != 0) {
                nonzero += 1;
                if samples.len() < 3 {
                    let hex: String = payload
                        .iter()
                        .take(16)
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join("");
                    samples.push(format!("{hex}..."));
                }
            }
        }
        if nonzero > 0 {
            println!(
                "{}: {nonzero}/{total} 0x4301 non-zero — examples: {}",
                path.file_name().unwrap().to_string_lossy(),
                samples.join(" | ")
            );
        } else {
            println!(
                "{}: 0/{total} 0x4301 non-zero",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }
}

fn collect_4301(
    blocks: &[dawfile_protools::raw_block::RawBlock],
) -> Vec<&dawfile_protools::raw_block::RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(
        blocks: &'a [dawfile_protools::raw_block::RawBlock],
        out: &mut Vec<&'a dawfile_protools::raw_block::RawBlock>,
    ) {
        for b in blocks {
            if b.content_type_raw == 0x4301 {
                out.push(b);
            }
            rec(&b.children, out);
        }
    }
    rec(blocks, &mut out);
    out
}
