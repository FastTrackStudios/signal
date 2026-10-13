//! Dump parsed items for a named track — loader fidelity check.
use daw_proto::ProjectContext;
use daw_proto::item::{ItemRef, Items};
use daw_proto::take::Takes;
use daw_proto::track::Tracks;
use daw_standalone::sync::Standalone;

fn main() {
    let rpp = std::env::args().nth(1).expect("rpp path");
    let track_name = std::env::args().nth(2).expect("track name");
    let text = std::fs::read_to_string(&rpp).expect("read");
    let daw = Standalone::new();
    let proj =
        daw_standalone::project_loader::load_rpp_text(&daw, "dump", &rpp, &text).expect("load");
    let ctx = ProjectContext::Project(proj.project_guid.clone());
    for (i, t) in Tracks::all(&daw, ctx.clone()).iter().enumerate() {
        if !t.name.contains(&track_name) {
            continue;
        }
        println!("track {i} {:?}", t.name);
        for item in Items::get_items(&daw, ctx.clone(), daw_proto::TrackRef::Index(i as u32)) {
            let take = Takes::get_active_take(&daw, ctx.clone(), ItemRef::Guid(item.guid.clone()));
            println!(
                "  pos={:.4} len={:.4} soffs={:?} rate={:?} src={:?} name={:?}",
                item.position.as_seconds(),
                item.length.as_seconds(),
                take.as_ref().map(|t| t.start_offset.as_seconds()),
                take.as_ref().map(|t| t.play_rate),
                take.as_ref().and_then(|t| t.source_file_path.clone()),
                take.as_ref().map(|t| t.name.clone()),
            );
        }
    }
}
