//! Compare two RPPs windowed to first 120s. Usage: qa_compare <ref.rpp> <ours.rpp>
use dawfile_reaper::io::parse_project_text;
const W: f64 = 120.0;
fn load(p: &str) -> dawfile_reaper::types::project::ReaperProject {
    parse_project_text(&std::fs::read_to_string(p).unwrap()).unwrap()
}
fn items_in_window(t: &dawfile_reaper::types::track::Track) -> Vec<i64> {
    t.items
        .iter()
        .filter(|i| i.position < W - 0.01)
        .map(|i| (i.position * 1000.0).round() as i64)
        .collect()
}
fn main() {
    let (a, b) = (
        std::env::args().nth(1).unwrap(),
        std::env::args().nth(2).unwrap(),
    );
    let (pa, pb) = (load(&a), load(&b));
    println!("tracks: ref={} ours={}", pa.tracks.len(), pb.tracks.len());
    // markers within window
    let mw = |p: &dawfile_reaper::types::project::ReaperProject| {
        p.markers_regions
            .markers
            .iter()
            .filter(|m| m.position < W - 0.01)
            .count()
    };
    println!(
        "markers(<120s): ref={} ours={}  (all: ref={} ours={})",
        mw(&pa),
        mw(&pb),
        pa.markers_regions.markers.len(),
        pb.markers_regions.markers.len()
    );
    // per-track item-position-set comparison within window, aligned positionally
    let mut tracks_item_mismatch = 0;
    let mut total_ref = 0;
    let mut total_ours = 0;
    let max = pa.tracks.len().max(pb.tracks.len());
    let mut details = Vec::new();
    for i in 0..max {
        let ia = pa.tracks.get(i).map(items_in_window).unwrap_or_default();
        let ib = pb.tracks.get(i).map(items_in_window).unwrap_or_default();
        total_ref += ia.len();
        total_ours += ib.len();
        // set compare (ours should contain all ref positions within window)
        let mut sa = ia.clone();
        sa.sort();
        let mut sb = ib.clone();
        sb.sort();
        if sa != sb {
            tracks_item_mismatch += 1;
            let na = pa.tracks.get(i).map(|t| t.name.as_str()).unwrap_or("-");
            details.push(format!(
                "  [{:2}] '{}' refpos={:?} ourspos={:?}",
                i, na, sa, sb
            ));
        }
    }
    println!(
        "items(<120s): ref={} ours={}  track-position-mismatches={}",
        total_ref, total_ours, tracks_item_mismatch
    );
    for d in details.iter().take(12) {
        println!("{}", d);
    }
}
