//! Finished edit steps reach the one listener (`daw_standalone::steps`):
//! a block's end under its name, an undo and a redo of it — and nothing
//! for a block begun and never ended. Its own binary, since the listener
//! is the whole engine's.

use std::sync::{Arc, Mutex};

use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, Projects as _, Tracks};
use daw_standalone::sync::Standalone;

#[test]
fn each_finished_step_is_told_with_its_name() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let heard: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let into = Arc::clone(&heard);
    daw_standalone::steps::listen(Arc::new(move |project, label| {
        into.lock()
            .expect("the log")
            .push((project.to_owned(), label.to_owned()));
    }));

    daw.begin_undo_block(ctx.clone(), "Add track");
    Tracks::add(&daw, ctx.clone(), "Kick In", None).expect("a track");
    daw.end_undo_block(ctx.clone(), "Add track", None);
    assert!(daw.undo(ctx.clone()));
    assert!(daw.redo(ctx.clone()));
    // Begun, never ended: no step.
    daw.begin_undo_block(ctx.clone(), "Abandoned");

    let heard = heard.lock().expect("the log").clone();
    let names: Vec<&str> = heard.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(names, ["Add track", "Undo Add track", "Redo Add track"]);
    assert!(
        heard.iter().all(|(p, _)| *p == guid),
        "each told its project"
    );
}
