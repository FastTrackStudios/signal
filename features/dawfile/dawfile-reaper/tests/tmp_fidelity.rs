const SRC: &str =
    "/run/media/AudioHaven/Project/Crescendum-Rockstars/it knows my name/it knows my name.RPP";
#[test]
fn tree_fidelity() {
    let Ok(orig) = std::fs::read_to_string(SRC) else {
        eprintln!("skip");
        return;
    };
    let c = dawfile_reaper::read_rpp_chunk(&orig).expect("parse");
    let out = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(c));
    let a: Vec<&str> = orig
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let b: Vec<&str> = out
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    println!("orig {} out {}", a.len(), b.len());
    let mut diffs = 0;
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            if diffs < 10 {
                println!(
                    "line {}:\n  - {}\n  + {}",
                    i + 1,
                    &x[..x.len().min(110)],
                    &y[..y.len().min(110)]
                );
            }
            diffs += 1;
        }
    }
    println!("differing lines: {diffs}");
    assert_eq!(diffs, 0, "tree round-trip is not byte-exact");
}
