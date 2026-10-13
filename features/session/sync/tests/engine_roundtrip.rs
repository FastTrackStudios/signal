//! A project through the doc and back: built in daw-standalone with
//! everything schema 2 keeps — an FX with state, a send from its 3/4,
//! automation of a fader and of the FX, fixed lanes, a group, a slice
//! with stretch markers and drum-hit take markers on its recording, a
//! MIDI take — hosted into the doc, and rebuilt
//! from it in an empty second project, which must read back the same.

#![cfg(feature = "engine")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use daw_proto::fx::InstalledFx;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{ProjectInfo, automation::EnvelopeShape};
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::sync::Standalone;
use session_sync::Bridge;
use session_sync::SessionDoc;
use session_sync::engine::{MediaRoot, MemoryBlobs};

/// A plugin that keeps the state it is given — a sampler's kit, say.
struct Keeper(Arc<Mutex<Vec<u8>>>);

impl PluginInstance for Keeper {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "keeper".into(),
            name: "Keeper".into(),
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
    fn prepare(&mut self, _rate: f64, _block: u32) -> Result<(), PluginError> {
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
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        *self.0.lock().unwrap() = state.to_vec();
        Ok(())
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn deactivate(&mut self) {}
}

struct Factory;

impl FxFactory for Factory {
    fn installed(&self) -> Vec<InstalledFx> {
        Vec::new()
    }
    fn provides(&self, name: &str) -> bool {
        name == "Keeper"
    }
    fn create(&self, name: &str, _rate: f64) -> Option<Box<dyn PluginInstance>> {
        self.provides(name)
            .then(|| Box::new(Keeper(Arc::new(Mutex::new(Vec::new())))) as Box<dyn PluginInstance>)
    }
}

/// Through the facade (a remote engine, REAPER) and through the engine in
/// this process: the same project either way.
#[test]
fn a_project_rebuilt_from_the_doc_reads_back_the_same() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(inner())
}

async fn inner() -> eyre::Result<()> {
    let standalone = Standalone::new();
    standalone.set_fx_factory(Arc::new(Factory));
    for guid in ["a", "b", "c", "d"] {
        standalone.seed_project(ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
    }
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    round_trip(&bundle.daw, &standalone, ("a", "b"), false).await?;
    round_trip(&bundle.daw, &standalone, ("c", "d"), true).await
}

async fn round_trip(
    daw: &daw_control::Daw,
    standalone: &Standalone,
    (from, to): (&str, &str),
    direct: bool,
) -> eyre::Result<()> {
    let a = daw.project(from).await?;
    let kick = a.tracks().add_with_guid("{KICK}", "Kick", None).await?;
    let oh = a.tracks().add_with_guid("{OH}", "OHs", None).await?;
    kick.set_ext_state("FTS:kind", "piece").await?;
    kick.set_ext_state("FTS:group", "Drums/Drum Kit/Kick")
        .await?;
    let fx = kick.fx_chain().add("Keeper").await?;
    fx.set_state_chunk(b"kit=heavy thwacky".to_vec()).await?;
    let fx_guid = fx.info().await?.guid;
    let send = kick.sends().add_to("{OH}").await?;
    send.set_source_channels(2, 2).await?;
    send.set_volume(0.5).await?;
    kick.envelopes()
        .volume()
        .add_point(
            PositionInSeconds::from_seconds(1.0),
            0.5,
            EnvelopeShape::Linear,
        )
        .await?;
    let gate = kick.envelopes().fx_param(&fx_guid, 0);
    gate.add_point(
        PositionInSeconds::from_seconds(2.0),
        0.25,
        EnvelopeShape::Square,
    )
    .await?;
    gate.add_point(
        PositionInSeconds::from_seconds(3.0),
        0.75,
        EnvelopeShape::Linear,
    )
    .await?;
    kick.set_lane_count(2).await?;
    kick.set_lane_name(0, "COMP").await?;
    kick.set_lane_name(1, "EDIT").await?;
    kick.set_lane_play_mask(0b10).await?;
    for t in [&kick, &oh] {
        t.set_group_membership(1, true).await?;
    }
    let slice = a
        .items()
        .add_with_guid(
            "{KICK}",
            "{SLICE}",
            PositionInSeconds::from_seconds(2.0),
            Duration::from_seconds(1.0),
        )
        .await?;
    slice.takes().add().await?;
    slice.set_fixed_lane(1).await?;
    slice
        .active_take()
        .set_stretch_markers(vec![daw_proto::StretchMarker {
            position: 0.5,
            source_position: 0.48,
            slope: 0.0,
        }])
        .await?;
    // The slice's recording carries two hits: a detector's, and one put
    // there by hand after it.
    let recording = std::env::temp_dir().join("Kick In-take1.wav");
    slice
        .active_take()
        .set_source_file(&recording.to_string_lossy())
        .await?;
    slice
        .active_take()
        .set_markers(vec![
            daw_proto::TakeMarker {
                id: "h1".into(),
                source_position_seconds: 2.5,
                hit: Some(daw_proto::HitMark {
                    piece: "Kick".into(),
                    run: "run-1".into(),
                    peak_db: -9.5,
                    confidence: 0.95,
                    from_trigger: true,
                    from_mic: true,
                    ..daw_proto::HitMark::default()
                }),
                ..daw_proto::TakeMarker::default()
            },
            daw_proto::TakeMarker {
                id: "h2".into(),
                source_position_seconds: 2.75,
                hit: Some(daw_proto::HitMark {
                    piece: "Kick".into(),
                    origin: daw_proto::MarkOrigin::Manual,
                    ghost: true,
                    note: "a ghost the trigger missed".into(),
                    ..daw_proto::HitMark::default()
                }),
                ..daw_proto::TakeMarker::default()
            },
        ])
        .await?;
    let groove = oh
        .items()
        .create_midi_item_with_notes(4.0, 6.0, Vec::new())
        .await?
        .expect("a MIDI item");
    groove
        .active_take()
        .midi()
        .add_notes_ppq(vec![
            daw_proto::MidiNoteCreate {
                channel: 9,
                pitch: 36,
                velocity: 110,
                start_ppq: 0.0,
                length_ppq: 240.0,
            },
            daw_proto::MidiNoteCreate {
                channel: 9,
                pitch: 38,
                velocity: 90,
                start_ppq: 960.0,
                length_ppq: 240.0,
            },
        ])
        .await?;
    let groove = groove.info().await?.guid;

    let blobs = Arc::new(MemoryBlobs::default());
    let media = MediaRoot(std::env::temp_dir());
    let doc = SessionDoc::new();
    let local: Option<Arc<dyn session_sync::engine::Direct>> =
        direct.then(|| Arc::new(standalone.clone()) as Arc<dyn session_sync::engine::Direct>);
    let host = Bridge::host(
        a,
        media.clone(),
        doc.clone(),
        String::new(),
        blobs.clone(),
        local.clone(),
    )
    .await?;

    let replica = SessionDoc::new();
    replica.import(&doc.snapshot()?, "remote")?;
    let b = daw.project(to).await?;
    let joined = Bridge::join(b, media, replica, blobs, local).await?;

    let (built, rebuilt) = (host.shown(), joined.shown());
    let kick = &built.tracks[0];
    assert_eq!(kick.fx.len(), 1, "the FX was read: {kick:?}");
    assert!(kick.fx[0].state.is_some(), "with its state");
    assert_eq!(kick.sends.len(), 1);
    assert_eq!(
        kick.ext.get("FTS:kind").map(String::as_str),
        Some("piece"),
        "its kind read"
    );
    assert_eq!(kick.envelopes.len(), 2, "{:?}", kick.envelopes.keys());
    assert_eq!(built.items["{SLICE}"].take.stretch.len(), 1);
    assert_eq!(
        built.items[&groove].take.notes.len(),
        2,
        "the MIDI was read"
    );
    assert_eq!(
        built.tracks, rebuilt.tracks,
        "tracks rebuilt the same (direct: {direct})"
    );
    assert_eq!(
        built.items, rebuilt.items,
        "items rebuilt the same (direct: {direct})"
    );
    assert_eq!(built.take_markers.len(), 2, "the hits were read");
    let manual = built
        .take_markers
        .values()
        .find(|m| m.id == "h2")
        .and_then(|m| m.hit.as_ref())
        .expect("the hand-placed hit");
    assert_eq!(manual.origin, "manual");
    assert!(manual.ghost);
    assert_eq!(
        built.take_markers, rebuilt.take_markers,
        "hits rebuilt the same (direct: {direct})"
    );
    Ok(())
}
