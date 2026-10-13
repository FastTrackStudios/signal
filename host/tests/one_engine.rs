//! Two rigs in one engine (docs/design/one-engine.md): each a folder of
//! tracks in the engine's one project, both heard in the one render, each
//! track metered by guid however the other rig has moved it.

use daw::service::{ProjectContext, Tracks as _};
use daw::standalone::audio_engine::render::ProjectRenderer;
use daw::standalone::metering::Meters;
use daw::standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_rig_host::engine::Engine;

/// An instrument that plays a constant: easy to find in the mix.
struct Constant(f32);

impl PluginInstance for Constant {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "constant".into(),
            name: "Constant".into(),
            vendor: "test".into(),
            version: "0".into(),
            format: PluginFormat::Synthetic,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, _sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        true
    }
    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        out_l.fill(self.0);
        out_r.fill(self.0);
        Ok(())
    }
    fn deactivate(&mut self) {}
}

fn instrument(rig: &signal_rig_host::engine::Rig, track: &str, level: f32) {
    let slot = rig.add_fx_slot(track, "instrument").expect("a slot");
    rig.daw()
        .insert_plugin_instance(slot, Box::new(Constant(level)));
}

#[test]
fn two_rigs_play_in_one_engine() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let engine = Engine::new("Signal");

    let guitar = engine.add_rig("Guitar").expect("guitar rig");
    let amp = guitar.add_track("Amp").expect("amp track");
    let keys = engine.add_rig("Keys").expect("keys rig");
    let piano = keys.add_track("Piano").expect("piano track");
    // Added after the keys rig: it goes in the guitar's folder, not last.
    let cab = guitar.add_track("Cab").expect("cab track");

    instrument(&guitar, &amp, 0.25);
    instrument(&keys, &piano, 0.125);
    // What starting the device does to the meters, without a device.
    engine.daw().set_meters(Meters::new(0));
    engine.daw().set_meters(Meters::new(1));
    // Growing past the bank happens on the next project change:
    keys.add_track("Pad").expect("pad track");

    let tracks = engine
        .daw()
        .all(ProjectContext::Project(engine.project_guid().to_string()));
    let order: Vec<(&str, i32)> = tracks
        .iter()
        .map(|t| (t.name.as_str(), t.folder_depth))
        .collect();
    assert_eq!(
        order,
        [
            ("Guitar", 1),
            ("Amp", 0),
            ("Cab", -1),
            ("Keys", 1),
            ("Piano", 0),
            ("Pad", -1),
        ]
    );

    let renderer = ProjectRenderer::new(engine.daw(), engine.project_guid(), 48_000);
    let block = renderer.render_block(0, 256);
    let peak = block.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.3, "both rigs in the master: peak {peak}");

    let (amp_l, _) = guitar.peak(&amp).expect("the amp is metered");
    let (piano_l, _) = keys.peak(&piano).expect("the piano is metered");
    assert!(amp_l > 0.0 && piano_l > 0.0, "amp {amp_l}, piano {piano_l}");
    assert!(
        guitar.peak(&cab).is_some(),
        "a track added later is metered too"
    );
}

#[test]
fn a_rig_nests_folders_and_keeps_them_closed() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let engine = Engine::new("Signal");
    let keys = engine.add_rig("Keys").expect("keys rig");
    let e1 = keys.add_track("Engine 1").expect("engine 1");
    keys.add_track_in(&e1, "Layer 1").expect("layer 1");
    keys.add_track_in(&e1, "Layer 2").expect("layer 2");
    let e2 = keys.add_track("Engine 2").expect("engine 2");
    keys.add_track_in(&e2, "Layer 3").expect("layer 3");
    let drums = engine.add_rig("Drums").expect("drums rig");
    drums.add_track("Kick").expect("kick");
    // Into the first engine folder, after both later folders exist.
    keys.add_track_in(&e1, "Layer 4").expect("layer 4");

    let tracks = engine
        .daw()
        .all(ProjectContext::Project(engine.project_guid().to_string()));
    let order: Vec<(&str, i32)> = tracks
        .iter()
        .map(|t| (t.name.as_str(), t.folder_depth))
        .collect();
    assert_eq!(
        order,
        [
            ("Keys", 1),
            ("Engine 1", 1),
            ("Layer 1", 0),
            ("Layer 2", 0),
            ("Layer 4", -1),
            ("Engine 2", 1),
            ("Layer 3", -2),
            ("Drums", 1),
            ("Kick", -1),
        ]
    );
    assert!(
        keys.add_track_in(drums.folder_guid(), "Stray").is_err(),
        "another rig's folder"
    );
}

#[test]
fn removing_a_track_takes_what_is_below_it_and_keeps_the_folders_closed() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let engine = Engine::new("Signal");
    let drums = engine.add_rig("Drums").expect("drums rig");
    let kick = drums.add_track("Kick").expect("kick");
    drums.add_track_in(&kick, "Kick In").expect("kick in");
    drums.add_track("Snare").expect("snare");
    drums.remove_track(&kick).expect("remove the kick");
    let tracks = engine
        .daw()
        .all(ProjectContext::Project(engine.project_guid().to_string()));
    let order: Vec<(&str, i32)> = tracks
        .iter()
        .map(|t| (t.name.as_str(), t.folder_depth))
        .collect();
    assert_eq!(order, [("Drums", 1), ("Snare", -1)]);
    assert_eq!(drums.tracks().len(), 1);
}
