//! The built-ins through the engine itself: added to a track by name as
//! any FX is, and made by this factory — the FTS processor, every one of
//! its parameters — not the engine's 8-parameter stand-in for a name it
//! cannot make.

use std::sync::Arc;

use daw_proto::fx::{Effects, FxChainContext};
use daw_proto::project::ProjectContext;
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;

const TRACK: &str = "{A0000000-0000-0000-0000-000000000001}";

const SONG: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  TEMPO 120 4 4 0
  SAMPLERATE 48000 0 0
  <TRACK {A0000000-0000-0000-0000-000000000001}
    NAME Drums
  >
>
"#;

#[test]
fn an_fts_processor_is_added_to_a_track_by_name() {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(builtin_fx::BuiltinFx));
    let loaded = load_rpp_text(&daw, "song", "/tmp/song.RPP", SONG).expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let chain = FxChainContext::Track(TRACK.into());
    for name in ["FTS Comp", "CLAP: FTS EQ (FastTrackStudios)"] {
        Effects::add(&daw, project.clone(), chain.clone(), name)
            .unwrap_or_else(|| panic!("{name} added"));
    }
    let fx = Effects::list(&daw, project, chain);
    let names: Vec<(&str, u32)> = fx
        .iter()
        .map(|f| (f.plugin_name.as_str(), f.parameter_count))
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0].0, "FTS Comp");
    assert_eq!(names[1].0, "FTS EQ");
    assert!(
        names.iter().all(|(_, n)| *n > 8),
        "the processor's own parameters, not a stand-in's: {names:?}"
    );
}
