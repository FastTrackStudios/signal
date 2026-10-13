//! The engine tells every plugin the song's tempo before each block
//! (`PluginInstance::set_tempo`), from the tempo map at the block's start:
//! what a synced delay or a reverb's tail in notes reads its note against.

#![cfg(feature = "decode")]

use std::sync::Arc;
use std::sync::Mutex;

use daw_proto::fx::{Effects, FxChainContext, InstalledFx};
use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, TempoMap, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::sync::Standalone;

/// A plugin that keeps every tempo it is told.
struct Counting(Arc<Mutex<Vec<f64>>>);

impl PluginInstance for Counting {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "counting".into(),
            name: "Counting".into(),
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
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        out_l.copy_from_slice(in_l);
        out_r.copy_from_slice(in_r);
        Ok(())
    }
    fn set_tempo(&mut self, bpm: f64) {
        self.0.lock().expect("the log").push(bpm);
    }
    fn deactivate(&mut self) {}
}

struct Factory(Arc<Mutex<Vec<f64>>>);

impl FxFactory for Factory {
    fn installed(&self) -> Vec<InstalledFx> {
        Vec::new()
    }
    fn provides(&self, name: &str) -> bool {
        name == "Counting"
    }
    fn create(&self, name: &str, _sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        self.provides(name)
            .then(|| Box::new(Counting(self.0.clone())) as Box<dyn PluginInstance>)
    }
}

#[test]
fn every_plugin_is_told_the_songs_tempo() {
    // Setting the tempo tells its subscribers on a task.
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let told = Arc::new(Mutex::new(Vec::new()));
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(Factory(told.clone())));
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    TempoMap::set_default_tempo(&daw, ctx.clone(), 90.0).expect("a tempo");
    let track = Tracks::add(&daw, ctx.clone(), "Verb", None).expect("a track");
    Effects::add(&daw, ctx, FxChainContext::Track(track), "Counting").expect("the fx");
    let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
    for block in 0..4 {
        let _ = renderer.render_block(block * 512, 512);
    }
    let told = told.lock().expect("the log");
    assert!(told.len() >= 4, "told before every block: {told:?}");
    assert!(
        told.iter().all(|t| (t - 90.0).abs() < 1e-9),
        "the song's 90: {told:?}"
    );
}
