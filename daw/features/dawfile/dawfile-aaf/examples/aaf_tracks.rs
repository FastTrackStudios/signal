//! Print tracks + clip names from an AAF.

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let s = dawfile_aaf::read_session(&path).unwrap();
    for comp in &s.compositions {
        println!("Composition: {:?}", comp.name);
        for (i, t) in comp.tracks.iter().enumerate() {
            println!(
                "  [{i:02}] slot={} kind={:?} clips={} name={:?}",
                t.slot_id,
                t.kind,
                t.clips.len(),
                t.name
            );
        }
    }
}
