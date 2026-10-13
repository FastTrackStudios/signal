//! A plugin parameter's automation plays: at each block the engine hands the
//! plugin the value its envelope has there (an FX-param envelope, Read).

#![cfg(feature = "decode")]

use std::sync::Arc;
use std::sync::Mutex;

use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeShape};
use daw_proto::fx::{Effects, FxChainContext, InstalledFx};
use daw_proto::primitives::PositionInSeconds;
use daw_proto::project::ProjectContext;
use daw_proto::{Automation, ProjectInfo, TrackRef, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::sync::Standalone;

/// A plugin that keeps, each block, the value it is given for parameter 0.
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
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        if let Some((_, value)) = events.params.iter().find(|(id, _)| *id == 0) {
            self.0.lock().expect("the log").push(*value);
        }
        out_l.copy_from_slice(in_l);
        out_r.copy_from_slice(in_r);
        Ok(())
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
fn a_parameters_automation_plays() {
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
    let track = Tracks::add(&daw, ctx.clone(), "Verb", None).expect("a track");
    let fx = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "Counting",
    )
    .expect("the fx");
    let at = EnvelopeLocation::new(
        TrackRef::Guid(track),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: 0,
        },
    );
    for (time, value) in [(0.0, 0.0), (2.0, 1.0)] {
        let _ = Automation::add_point(
            &daw,
            ctx.clone(),
            at.clone(),
            AddPointParams::new(
                PositionInSeconds::from_seconds(time),
                value,
                EnvelopeShape::Linear,
            ),
        );
    }
    let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
    let _ = renderer.render_block(0, 512);
    let _ = renderer.render_block(48_000, 512);
    let told = told.lock().expect("the log");
    assert_eq!(told.len(), 2, "told each block: {told:?}");
    assert!(told[0].abs() < 1e-6, "0 at the start: {told:?}");
    assert!((told[1] - 0.5).abs() < 1e-6, "halfway at 1 s: {told:?}");
}

/// An automation item plays over its window instead of the envelope's
/// own points, and its curve bends the way its points say.
#[test]
fn an_automation_item_plays_over_its_window() {
    use daw_proto::automation::{AddAutomationItemParams, SetAutomationItemPointsParams};
    use daw_proto::primitives::Duration;

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
    let track = Tracks::add(&daw, ctx.clone(), "Kick", None).expect("a track");
    let fx = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "Counting",
    )
    .expect("the fx");
    let at = EnvelopeLocation::new(
        TrackRef::Guid(track),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: 0,
        },
    );
    // The verse at 0.2 all through.
    let _ = Automation::add_point(
        &daw,
        ctx.clone(),
        at.clone(),
        AddPointParams::new(PositionInSeconds::ZERO, 0.2, EnvelopeShape::Linear),
    );
    // A chorus from 2 s to 4 s: up to 0.8 along an S over its first second.
    let index = Automation::add_automation_item(
        &daw,
        ctx.clone(),
        at.clone(),
        AddAutomationItemParams::new(
            PositionInSeconds::from_seconds(2.0),
            Duration::from_seconds(2.0),
        ),
    )
    .expect("the item");
    let point =
        |time: f64, value: f64, shape: EnvelopeShape| daw_proto::automation::EnvelopePoint {
            time: PositionInSeconds::from_seconds(time),
            value,
            shape,
            ..Default::default()
        };
    Automation::set_automation_item_points(
        &daw,
        ctx.clone(),
        at.clone(),
        SetAutomationItemPointsParams {
            index,
            points: vec![
                point(0.0, 0.2, EnvelopeShape::SlowStartEnd),
                point(1.0, 0.8, EnvelopeShape::Linear),
                point(2.0, 0.8, EnvelopeShape::Linear),
            ],
        },
    )
    .expect("its curve");

    let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
    for seconds in [1.0, 2.25, 2.5, 3.5, 5.0] {
        let frame = (seconds * 48_000.0) as u64;
        let _ = renderer.render_block(frame, 512);
    }
    let told = told.lock().expect("the log");
    let [verse, quarter, half, chorus, after] = told.as_slice() else {
        panic!("told each block: {told:?}");
    };
    assert!((verse - 0.2).abs() < 1e-6, "{told:?}");
    // A quarter of the way along the S: under the straight line's 0.35.
    assert!(*quarter > 0.2 && *quarter < 0.35, "eased in: {told:?}");
    assert!((half - 0.5).abs() < 1e-6, "{told:?}");
    assert!((chorus - 0.8).abs() < 1e-6, "{told:?}");
    assert!(
        (after - 0.2).abs() < 1e-6,
        "the envelope again after: {told:?}"
    );
}

/// A plugin whose one parameter has an id of its own (7) and a plain range
/// (−24..24 dB): what it is told, block by block, is kept.
struct Ranged(Arc<Mutex<Vec<(u32, f64)>>>);

impl PluginInstance for Ranged {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "ranged".into(),
            name: "Ranged".into(),
            vendor: "test".into(),
            version: "0".into(),
            format: PluginFormat::Synthetic,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![PluginParamInfo {
            id: 7,
            name: "Gain".into(),
            min: -24.0,
            max: 24.0,
            default: 0.0,
        }]
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
        self.0
            .lock()
            .expect("the log")
            .extend(events.params.iter().copied());
        out_l.copy_from_slice(in_l);
        out_r.copy_from_slice(in_r);
        Ok(())
    }
    fn deactivate(&mut self) {}
}

struct RangedFactory(Arc<Mutex<Vec<(u32, f64)>>>);

impl FxFactory for RangedFactory {
    fn installed(&self) -> Vec<InstalledFx> {
        Vec::new()
    }
    fn provides(&self, name: &str) -> bool {
        name == "Ranged"
    }
    fn create(&self, name: &str, _sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        self.provides(name)
            .then(|| Box::new(Ranged(self.0.clone())) as Box<dyn PluginInstance>)
    }
}

/// An envelope is 0..1 along a parameter's range: the plugin is told the
/// plain value (−24 dB at the bottom, 0 dB halfway), under its own id —
/// not the 0..1, and not the parameter's place in its list.
#[test]
fn an_automated_parameter_reaches_its_plugin_in_its_own_units() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let told = Arc::new(Mutex::new(Vec::new()));
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(RangedFactory(told.clone())));
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let track = Tracks::add(&daw, ctx.clone(), "Bass", None).expect("a track");
    let fx = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "Ranged",
    )
    .expect("the fx");
    let at = EnvelopeLocation::new(
        TrackRef::Guid(track),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: 0,
        },
    );
    for (time, value) in [(0.0, 0.0), (2.0, 1.0)] {
        let _ = Automation::add_point(
            &daw,
            ctx.clone(),
            at.clone(),
            AddPointParams::new(
                PositionInSeconds::from_seconds(time),
                value,
                EnvelopeShape::Linear,
            ),
        );
    }
    let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
    let _ = renderer.render_block(0, 512);
    let _ = renderer.render_block(48_000, 512);
    let told = told.lock().expect("the log");
    let gains: Vec<f64> = told
        .iter()
        .filter(|(id, _)| *id == 7)
        .map(|(_, v)| *v)
        .collect();
    assert_eq!(gains.len(), 2, "told under its id each block: {told:?}");
    assert!(
        (gains[0] + 24.0).abs() < 1e-6,
        "the bottom of its range: {told:?}"
    );
    assert!(gains[1].abs() < 1e-6, "halfway, 0 dB: {told:?}");
}
