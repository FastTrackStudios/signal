//! A plugin's outputs past its main pair: what it puts on 3/4 is what a
//! send from the track's 3/4 carries (a kit's overhead mics, sent into the
//! overheads), and none of it reaches the track's own output.

#![cfg(feature = "decode")]

use std::sync::Arc;
use std::sync::Mutex;

use daw_proto::fx::{Effects, FxChainContext, InstalledFx};
use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, RouteLocation, RouteRef, RouteType, Routing, TrackRef, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::sync::Standalone;

/// A plugin silent on its main pair, and putting 0.5 / 0.25 on 3/4.
struct Counting(Arc<Mutex<Vec<f64>>>, Vec<f32>, Vec<f32>);

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
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let _ = (in_l, in_r, events, &self.0);
        out_l.fill(0.0);
        out_r.fill(0.0);
        self.1 = vec![0.5; out_l.len()];
        self.2 = vec![0.25; out_r.len()];
        Ok(())
    }
    fn aux_out(&self, pair: usize) -> Option<(&[f32], &[f32])> {
        (pair == 0).then_some((self.1.as_slice(), self.2.as_slice()))
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
        self.provides(name).then(|| {
            Box::new(Counting(self.0.clone(), Vec::new(), Vec::new())) as Box<dyn PluginInstance>
        })
    }
}

/// The song's master after one block with `from` the send's source
/// channel (0 the main pair, 2 the plugin's 3/4).
fn master_with_send_from(from: u32) -> f32 {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(Factory(Arc::new(Mutex::new(Vec::new())))));
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let sample = Tracks::add(&daw, ctx.clone(), "Sample", None).expect("a track");
    let oh = Tracks::add(&daw, ctx.clone(), "OH", None).expect("a track");
    Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(sample.clone()),
        "Counting",
    )
    .expect("the fx");
    let index = Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(sample.clone()),
        TrackRef::Guid(oh),
    )
    .expect("a send");
    Routing::set_source_channels(
        &daw,
        ctx,
        RouteLocation {
            track: TrackRef::Guid(sample),
            route_type: RouteType::Send,
            route: RouteRef::Index(index),
        },
        from,
        2,
    )
    .expect("the source");
    let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
    let block = renderer.render_block(0, 256);
    block.samples.iter().map(|s| s.abs()).fold(0.0, f32::max)
}

/// The plugin's 3/4 reaches the overheads through a send from 3/4; a
/// send from the main pair carries its silence.
#[test]
fn a_send_from_3_4_carries_the_plugins_aux_output() {
    let heard = master_with_send_from(2);
    assert!(heard > 0.2, "the aux output, sent: {heard}");
    let main = master_with_send_from(0);
    assert!(main < 1e-6, "the main pair is silent: {main}");
}
