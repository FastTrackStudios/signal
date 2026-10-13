//! A muted track's FX chain is not run when nothing would hear it: the
//! fader zeroes a muted track, and only a pre-fader (post-FX) send taps
//! it before that. A mix's muted alternatives cost nothing while off.

#![cfg(feature = "decode")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use daw_proto::fx::{Effects, FxChainContext, InstalledFx};
use daw_proto::project::ProjectContext;
use daw_proto::routing::SendMode;
use daw_proto::{ProjectInfo, RouteRef, Routing, TrackRef, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::sync::Standalone;

/// A plugin that counts the blocks it is asked to process.
struct Counting(Arc<AtomicUsize>);

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
        self.0.fetch_add(1, Ordering::SeqCst);
        out_l.copy_from_slice(in_l);
        out_r.copy_from_slice(in_r);
        Ok(())
    }
    fn deactivate(&mut self) {}
}

struct Factory(Arc<AtomicUsize>);

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
fn a_muted_tracks_chain_runs_only_when_something_hears_it() {
    let blocks = Arc::new(AtomicUsize::new(0));
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(Factory(blocks.clone())));
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let verb = Tracks::add(&daw, ctx.clone(), "Verb", None).expect("a track");
    let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).expect("a bus");
    Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(verb.clone()),
        "Counting",
    )
    .expect("the fx");
    let render = || {
        blocks.store(0, Ordering::SeqCst);
        let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
        for block in 0..4 {
            let _ = renderer.render_block(block * 512, 512);
        }
        blocks.load(Ordering::SeqCst)
    };
    assert!(render() > 0, "playing: the chain runs");
    Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(verb.clone()), true).expect("muted");
    assert_eq!(render(), 0, "muted, heard by nothing: not run");
    // A post-FX (pre-fader) send hears it through the mute.
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(verb.clone()),
        TrackRef::Guid(bus),
    )
    .expect("a send");
    Routing::set_send_mode(
        &daw,
        ctx,
        TrackRef::Guid(verb),
        RouteRef::Index(0),
        SendMode::PostFx,
    )
    .expect("pre-fader");
    assert!(render() > 0, "muted but sent pre-fader: the chain runs");
}
