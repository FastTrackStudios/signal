//! Dump routing facts: which tracks reach master (parent_send), parents,
//! hw outs — find where the signal path dead-ends.
use daw_proto::ProjectContext;
use daw_proto::routing::Routing;
use daw_proto::track::Tracks;
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;
use std::path::PathBuf;

fn main() {
    let rpp = std::env::args().nth(1).expect("rpp");
    let text = std::fs::read_to_string(&rpp).expect("read");
    let dir = PathBuf::from(&rpp).parent().map(PathBuf::from).unwrap();
    let daw = Standalone::new();
    daw.media_bay()
        .set_file_resolver(Box::new(ProjectRelativeResolver::new(dir)));
    let proj = load_rpp_text(&daw, "probe", &rpp, &text).expect("load");
    let ctx = ProjectContext::Project(proj.project_guid.clone());
    let tracks = Tracks::all(&daw, ctx.clone());
    let mut to_master = 0;
    for (i, t) in tracks.iter().enumerate() {
        let tref = daw_proto::TrackRef::Index(i as u32);
        let ps = Routing::parent_send_enabled(&daw, ctx.clone(), tref.clone());
        let hw = Routing::hardware_outputs(&daw, ctx.clone(), tref.clone()).len();
        let sends = Routing::send_count(&daw, ctx.clone(), tref.clone());
        if t.parent_guid.is_none() && ps {
            to_master += 1;
        }
        if std::env::var("FTS_ALL").is_ok() || i < 12 || (t.parent_guid.is_none() && (ps || hw > 0))
        {
            let parent_name = t
                .parent_guid
                .as_deref()
                .map(|pg| {
                    tracks
                        .iter()
                        .find(|o| o.guid == pg)
                        .map(|o| o.name.clone())
                        .unwrap_or_else(|| format!("MISSING({pg})"))
                })
                .unwrap_or_else(|| "-".into());
            let lanes = if t.lane_count > 0 {
                format!(
                    " lanes={} play={:#x} names={:?}",
                    t.lane_count, t.lane_play_mask, t.lane_names
                )
            } else {
                String::new()
            };
            println!(
                "{i:3} {:28} parent={parent_name:<20} parent_send={ps} hw={hw} sends={sends} folder={} muted={} vol={:.3}{lanes}",
                t.name, t.is_folder, t.muted, t.volume,
            );
        }
    }
    println!("top-level tracks with parent_send: {to_master}");
}
