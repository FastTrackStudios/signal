use dawfile_reaper::io::parse_project_text;
fn main() {
    let f = std::env::args().nth(1).unwrap();
    let want = std::env::args().nth(2).unwrap();
    let p = parse_project_text(&std::fs::read_to_string(&f).unwrap()).unwrap();
    for t in &p.tracks {
        if t.name.contains(&want) {
            println!("TRACK '{}' ({} items):", t.name, t.items.len());
            for it in &t.items {
                let src = it
                    .takes
                    .first()
                    .and_then(|tk| tk.source.as_ref())
                    .map(|s| format!("{:?}", s.source_type))
                    .unwrap_or("-".into());
                let file = it
                    .takes
                    .first()
                    .and_then(|tk| tk.source.as_ref())
                    .map(|s| s.file_path.clone())
                    .unwrap_or_default();
                println!(
                    "  pos={:.3} len={:.3} src={} '{}' file={}",
                    it.position,
                    it.length,
                    src,
                    it.name,
                    file.split('/').next_back().unwrap_or("")
                );
            }
        }
    }
}
