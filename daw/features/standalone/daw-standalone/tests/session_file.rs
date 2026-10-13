//! A live project saved as a `.session` and opened back: the engine state
//! after the round trip is the engine state before it.
//!
//! The fixture is a small `.RPP` opened into a `Standalone`, then edited
//! the way the session app's prepare pass edits a song — a folder with
//! reparented tracks, a send, a generated MIDI track playing a built-in
//! FX, markers and regions on named ruler lanes, a tempo map with a
//! time-signature change, colours, a mixer width — before it is saved.

#![cfg(feature = "session-file")]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use daw_proto::fx::{Effects, FxChainContext, InstalledFx};
use daw_proto::primitives::{Duration, PositionInSeconds, TimeSignature};
use daw_proto::project::ProjectContext;
use daw_proto::{Marker, Position, Region, TempoPoint};
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::session_file::{
    load_session, load_session_history, save_session, save_session_with_history, session_rpp_text,
};
use daw_standalone::sync::{FxChainKey, ItemEntry, ProjectState, RulerLane, Standalone, TakeList};

// ────────────────────────────────────────────────────────────────────
// A built-in FX factory, as the session app injects one
// ────────────────────────────────────────────────────────────────────

const CLICK: &str = "fts.guide:click";
const COUNT: &str = "fts.guide:count";

#[derive(Default)]
struct GuideFactory {
    created: AtomicUsize,
}

impl FxFactory for GuideFactory {
    fn installed(&self) -> Vec<InstalledFx> {
        [CLICK, COUNT]
            .into_iter()
            .map(|n| InstalledFx {
                name: n.to_string(),
                ident: n.to_string(),
            })
            .collect()
    }

    fn create(&self, name: &str, _sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        self.provides(name).then(|| {
            self.created.fetch_add(1, Ordering::SeqCst);
            Box::new(Silence(name.to_string(), Vec::new())) as Box<dyn PluginInstance>
        })
    }
}

struct Silence(String, Vec<u8>);

impl PluginInstance for Silence {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: self.0.clone(),
            name: self.0.clone(),
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
        out_l.fill(0.0);
        out_r.fill(0.0);
        Ok(())
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.1 = state.to_vec();
        Ok(())
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        Ok(self.1.clone())
    }
    fn deactivate(&mut self) {}
}

fn engine() -> (Standalone, Arc<GuideFactory>) {
    let daw = Standalone::new();
    let factory = Arc::new(GuideFactory::default());
    daw.set_fx_factory(factory.clone());
    (daw, factory)
}

// ────────────────────────────────────────────────────────────────────
// The fixture
// ────────────────────────────────────────────────────────────────────

const DRUMS: &str = "{A0000000-0000-0000-0000-000000000001}";
const BASS: &str = "{A0000000-0000-0000-0000-000000000002}";
const KEYS: &str = "{A0000000-0000-0000-0000-000000000003}";
const DRUMS_ITEM: &str = "{B0000000-0000-0000-0000-000000000001}";
const BUS: &str = "c0ffee00-0000-4000-8000-000000000001";
const CLICK_TRACK: &str = "c0ffee00-0000-4000-8000-000000000002";
const CLICK_ITEM: &str = "c0ffee00-0000-4000-8000-000000000003";
const CLICK_TAKE: &str = "c0ffee00-0000-4000-8000-000000000004";

/// A song as REAPER writes one, with things the engine never models: a
/// VST3 with a state blob, a metronome block, a foreign extension block,
/// `PLAYOFFS`/`VU` lines.
const SONG_RPP: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  RIPPLE 0 0
  GRID 3199 8 1 8 1 0 0 0
  TEMPO 120 4 4 0
  SAMPLERATE 48000 0 0
  <METRONOME 7 2
    VOL 0.25 0.125
    PATTERNSTR ABBB
  >
  MASTERMUTESOLO 0
  MASTER_VOLUME 1 0 -1 -1 1
  <TRACK {A0000000-0000-0000-0000-000000000001}
    NAME Drums
    PEAKCOL 16576
    BEAT -1
    AUTOMODE 0
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    IPHASE 0
    PLAYOFFS 0 1
    ISBUS 0 0
    BUSCOMP 0 0 0 0 0
    SHOWINMIX 1 0.6667 0.5 1 0.5 0 0 0
    SEL 0
    REC 0 0 1 0 0 0 0 0
    VU 2
    NCHAN 2
    FX 1
    TRACKID {A0000000-0000-0000-0000-000000000001}
    PERF 0
    MIDIOUT -1
    MAINSEND 1 0
    <FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST3: Some Compressor (Vendor)" "Some Compressor.vst3" 0 "" 1234{ABCDEF01} ""
        c29tZSBzdGF0ZQ==
        bW9yZSBzdGF0ZQ==
      >
      FLOATPOS 0 0 0 0
      FXID {F0000000-0000-0000-0000-000000000001}
      WAK 0 0
    >
    <ITEM
      POSITION 0
      SNAPOFFS 0
      LENGTH 4
      LOOP 1
      ALLTAKES 0
      FADEIN 1 0.01 0 1 0 0 0
      FADEOUT 1 0.01 0 1 0 0 0
      MUTE 0 0
      SEL 0
      IGUID {B0000000-0000-0000-0000-000000000001}
      IID 1
      NAME drums.wav
      VOLPAN 1 0 1 -1
      SOFFS 0
      PLAYRATE 1 1 0 -1 0 0.0025
      CHANMODE 0
      GUID {C0000000-0000-0000-0000-000000000001}
      <SOURCE WAVE
        FILE "Media/drums.wav"
      >
    >
  >
  <TRACK {A0000000-0000-0000-0000-000000000002}
    NAME Bass
    PEAKCOL 16576
    AUTOMODE 0
    VOLPAN 0.8 0.1 -1 -1 1
    MUTESOLO 0 0 0
    ISBUS 0 0
    SHOWINMIX 1 0.6667 0.5 1 0.5 0 0 0
    REC 0 0 1 0 0 0 0 0
    NCHAN 2
    FX 1
    TRACKID {A0000000-0000-0000-0000-000000000002}
    MAINSEND 1 0
    <ITEM
      POSITION 2
      SNAPOFFS 0
      LENGTH 6
      LOOP 0
      ALLTAKES 0
      FADEIN 1 0 0 1 0 0 0
      FADEOUT 1 0 0 1 0 0 0
      MUTE 0 0
      SEL 0
      IGUID {B0000000-0000-0000-0000-000000000002}
      IID 2
      NAME bass.wav
      VOLPAN 1 0 1 -1
      SOFFS 0.5
      PLAYRATE 1 1 0 -1 0 0.0025
      CHANMODE 0
      GUID {C0000000-0000-0000-0000-000000000002}
      <SOURCE WAVE
        FILE "Media/bass.wav"
      >
    >
  >
  <TRACK {A0000000-0000-0000-0000-000000000003}
    NAME Keys
    PEAKCOL 16576
    AUTOMODE 0
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    ISBUS 0 0
    SHOWINMIX 1 0.6667 0.5 1 0.5 0 0 0
    REC 0 0 1 0 0 0 0 0
    NCHAN 2
    FX 1
    TRACKID {A0000000-0000-0000-0000-000000000003}
    MAINSEND 1 0
  >
  <EXTSTATE
    <OTHERPLUGIN
      KEY value
    >
  >
>
"#;

/// The song opened into a fresh engine, from `dir/Song.RPP`.
fn open_song(dir: &Path) -> (Standalone, Arc<GuideFactory>, String) {
    std::fs::create_dir_all(dir).unwrap();
    let rpp = dir.join("Song.RPP");
    std::fs::write(&rpp, SONG_RPP).unwrap();
    let (daw, factory) = engine();
    let loaded = load_rpp_text(&daw, "Song", &rpp.to_string_lossy(), SONG_RPP).unwrap();
    (daw, factory, loaded.project_guid)
}

/// What the session app's prepare pass does to a song, in miniature.
fn prepare(daw: &Standalone, guid: &str) {
    daw.write_project(guid, |p| {
        // A folder bus at the top with Drums and Bass reparented into it.
        let mut bus = p.tracks[2].clone();
        bus.guid = BUS.into();
        bus.name = "BUS".into();
        bus.is_folder = true;
        bus.color = None;
        p.tracks.insert(0, bus);
        p.track_ext.insert(BUS.into(), Default::default());
        p.items_by_track.insert(BUS.into(), Vec::new());
        for t in &mut p.tracks {
            match t.guid.as_str() {
                DRUMS | BASS => t.parent_guid = Some(BUS.into()),
                KEYS => {
                    t.name = "Keys 2".into();
                    t.volume = 0.5;
                    t.pan = -0.3;
                }
                _ => {}
            }
            if t.guid == BASS {
                t.color = Some(0x33_66_99);
                t.muted = true;
            }
            if t.guid == DRUMS {
                t.width = Some(120);
            }
        }

        // A send Drums → Keys.
        let mut send = daw_proto::TrackRoute::default();
        send.route_type = daw_proto::RouteType::Send;
        send.source_track_guid = DRUMS.into();
        send.dest_track_guid = Some(KEYS.into());
        send.volume = 0.5;
        send.pan = -0.25;
        send.send_mode = daw_proto::routing::SendMode::PreFx;
        p.sends.entry(DRUMS.into()).or_default().push(send);

        // The drums item moved.
        p.items.get_mut(DRUMS_ITEM).unwrap().item.position = PositionInSeconds::from_seconds(1.0);

        // A generated MIDI track at the end.
        let mut click = p.tracks[1].clone();
        click.guid = CLICK_TRACK.into();
        click.name = "Click".into();
        click.parent_guid = None;
        click.color = Some(0xff_80_00);
        click.width = None;
        click.muted = false;
        p.tracks.push(click);
        p.track_ext.insert(CLICK_TRACK.into(), Default::default());
        let mut item = p.items[DRUMS_ITEM].item.clone();
        item.guid = CLICK_ITEM.into();
        item.track_guid = CLICK_TRACK.into();
        item.position = PositionInSeconds::from_seconds(0.0);
        item.length = Duration::from_seconds(8.0);
        item.loop_source = false;
        item.fade_in_length = Duration::from_seconds(0.0);
        item.fade_out_length = Duration::from_seconds(0.0);
        p.items.insert(CLICK_ITEM.into(), ItemEntry { item });
        p.items_by_track
            .insert(CLICK_TRACK.into(), vec![CLICK_ITEM.into()]);
        let mut take = p.takes[DRUMS_ITEM].takes[0].clone();
        take.guid = CLICK_TAKE.into();
        take.item_guid = CLICK_ITEM.into();
        take.name = "Click".into();
        take.is_midi = true;
        take.source_type = daw_proto::item::SourceType::Midi;
        take.source_file_path = None;
        p.takes.insert(
            CLICK_ITEM.into(),
            TakeList {
                active_idx: 0,
                takes: vec![take],
            },
        );
        let notes = (0..8)
            .map(|i| daw_proto::midi::MidiNote {
                index: i,
                channel: 9,
                pitch: if i % 4 == 0 { 60 } else { 61 },
                velocity: if i % 4 == 0 { 110 } else { 80 },
                start_ppq: f64::from(i),
                length_ppq: 0.5,
                selected: false,
                muted: false,
            })
            .collect();
        p.midi_notes.insert(CLICK_TAKE.into(), notes);
        p.midi_ccs.insert(
            CLICK_TAKE.into(),
            vec![daw_proto::midi::MidiCC {
                index: 0,
                channel: 9,
                controller: 7,
                value: 100,
                position_ppq: 0.0,
                selected: false,
            }],
        );

        // Ruler lanes, markers and regions.
        p.ruler_lanes.insert(
            0,
            RulerLane {
                name: "SONG".into(),
                flags: RulerLane::DEFAULT_REGION,
            },
        );
        p.ruler_lanes.insert(
            1,
            RulerLane {
                name: "MARKS".into(),
                flags: RulerLane::DEFAULT_MARKER,
            },
        );
        p.markers.insert(
            0,
            Marker {
                id: Some(0),
                position: Position::from_time(PositionInSeconds::from_seconds(2.0)),
                name: "Hit".into(),
                color: Some(0xff_00_00),
                guid: Some("{D0000000-0000-0000-0000-000000000001}".into()),
                lane: Some(1),
            },
        );
        p.markers.insert(
            1,
            Marker {
                id: Some(1),
                position: Position::from_time(PositionInSeconds::from_seconds(6.5)),
                name: "Stop".into(),
                color: None,
                guid: Some("{D0000000-0000-0000-0000-000000000002}".into()),
                lane: Some(1),
            },
        );
        let mut verse = Region::from_seconds(0.0, 8.0, "Verse".to_string());
        verse.id = Some(0);
        verse.color = Some(0x00_ff_00);
        verse.guid = Some("{D0000000-0000-0000-0000-000000000003}".into());
        verse.lane = Some(0);
        p.regions.insert(0, verse);
        let mut chorus = Region::from_seconds(8.0, 16.0, "Chorus".to_string());
        chorus.id = Some(1);
        chorus.guid = Some("{D0000000-0000-0000-0000-000000000004}".into());
        chorus.lane = Some(0);
        p.regions.insert(1, chorus);

        // A tempo map with a time-signature change.
        let mut first = TempoPoint::new(
            Position::from_time(PositionInSeconds::from_seconds(0.0)),
            120.0,
        );
        first.time_signature = Some(TimeSignature::new(4, 4));
        let mut second = TempoPoint::new(
            Position::from_time(PositionInSeconds::from_seconds(8.0)),
            90.0,
        );
        second.time_signature = Some(TimeSignature::new(3, 4));
        p.tempo_points = vec![first, second];
    })
    .unwrap();

    // Built-in FX, through the engine's own service: one on the new
    // track, one beside the original VST3 on Drums.
    let ctx = ProjectContext::Project(guid.to_string());
    Effects::add(
        daw,
        ctx.clone(),
        FxChainContext::Track(CLICK_TRACK.into()),
        CLICK,
    )
    .unwrap();
    Effects::add(daw, ctx, FxChainContext::Track(DRUMS.into()), COUNT).unwrap();
}

// ────────────────────────────────────────────────────────────────────
// The comparison
// ────────────────────────────────────────────────────────────────────

/// Everything the loader sets, in a comparable form.
#[derive(Debug, PartialEq)]
struct Snapshot {
    transport: (u64, u32, u32),
    master: (u64, u64, bool),
    tracks: Vec<String>,
    sends: Vec<String>,
    hw_outputs: Vec<String>,
    items: Vec<String>,
    midi: Vec<String>,
    fx: Vec<String>,
    envelopes: Vec<String>,
    ruler_lanes: Vec<String>,
    markers: Vec<String>,
    regions: Vec<String>,
    tempo: Vec<String>,
}

fn snapshot(daw: &Standalone, guid: &str) -> Snapshot {
    daw.read_project(guid, snapshot_of).unwrap()
}

fn snapshot_of(p: &ProjectState) -> Snapshot {
    let secs = |pos: &Position| pos.time.map_or(-1.0, |t| t.as_seconds());
    let tracks = p
        .tracks
        .iter()
        .map(|t| {
            let ext = p.track_ext.get(&t.guid).cloned().unwrap_or_default();
            format!(
                "{} {:?} parent={:?} folder={} color={:?} vol={} pan={} mute={} solo={} \
                 phase={} sel={} width={:?} height={:?} tcp={} mcp={} armed={} mon={:?} \
                 input={:?} mainsend={} nch={} auto={:?} lanes={} group={:?}",
                t.guid,
                t.name,
                t.parent_guid,
                t.is_folder,
                t.color,
                t.volume,
                t.pan,
                t.muted,
                t.soloed,
                t.phase_inverted,
                t.selected,
                t.width,
                t.height,
                t.visible_in_tcp,
                t.visible_in_mixer,
                t.armed,
                t.input_monitor,
                ext.record_input,
                ext.parent_send_enabled,
                ext.num_channels,
                t.automation_mode,
                t.lane_count,
                t.grouping,
            )
        })
        .collect();
    let routes = |map: &std::collections::HashMap<String, Vec<daw_proto::TrackRoute>>| {
        let mut out: Vec<String> = map
            .iter()
            .flat_map(|(src, rs)| {
                rs.iter().map(move |r| {
                    format!(
                        "{src} -> {:?}/{:?} mode={:?} vol={} pan={} mute={} phase={}",
                        r.dest_track_guid,
                        r.hw_output_index,
                        r.send_mode,
                        r.volume,
                        r.pan,
                        r.muted,
                        r.phase_inverted
                    )
                })
            })
            .collect();
        out.sort();
        out
    };
    let mut items = Vec::new();
    let mut midi = Vec::new();
    for t in &p.tracks {
        for ig in p.items_by_track.get(&t.guid).into_iter().flatten() {
            let i = &p.items[ig].item;
            items.push(format!(
                "{} {} pos={} len={} snap={} mute={} vol={} fin={}/{:?} fout={}/{:?} color={:?} \
                 loop={} lane={:?}",
                t.guid,
                i.guid,
                i.position.as_seconds(),
                i.length.as_seconds(),
                i.snap_offset.as_seconds(),
                i.muted,
                i.volume,
                i.fade_in_length.as_seconds(),
                i.fade_in_shape,
                i.fade_out_length.as_seconds(),
                i.fade_out_shape,
                i.color,
                i.loop_source,
                i.fixed_lane,
            ));
            let Some(tl) = p.takes.get(ig) else { continue };
            items.push(format!("  active={}", tl.active_idx));
            for tk in &tl.takes {
                items.push(format!(
                    "  take {} {:?} vol={} rate={} chan={} offs={} src={:?} {:?} midi={}",
                    tk.guid,
                    tk.name,
                    tk.volume,
                    tk.play_rate,
                    tk.channel_mode,
                    tk.start_offset.as_seconds(),
                    tk.source_type,
                    tk.source_file_path,
                    tk.is_midi,
                ));
                for n in p.midi_notes.get(&tk.guid).into_iter().flatten() {
                    midi.push(format!(
                        "{} note ch={} p={} v={} at={} len={}",
                        tk.guid, n.channel, n.pitch, n.velocity, n.start_ppq, n.length_ppq
                    ));
                }
                for c in p.midi_ccs.get(&tk.guid).into_iter().flatten() {
                    midi.push(format!(
                        "{} cc ch={} {}={} at={}",
                        tk.guid, c.channel, c.controller, c.value, c.position_ppq
                    ));
                }
            }
        }
    }
    let fx = p
        .tracks
        .iter()
        .map(|t| {
            let names: Vec<String> = p
                .fx_chains
                .get(&FxChainKey::Track(t.guid.clone()))
                .into_iter()
                .flatten()
                .map(|e| format!("{}{}", e.fx.name, if e.fx.enabled { "" } else { " (off)" }))
                .collect();
            format!("{}: {}", t.guid, names.join(", "))
        })
        .collect();
    let mut envelopes: Vec<String> = p
        .envelopes
        .iter()
        .map(|((g, k), d)| {
            let pts: Vec<String> = d
                .points
                .iter()
                .map(|pt| format!("{:.6}={:.6}", pt.time.as_seconds(), pt.value))
                .collect();
            format!("{g} {k:?} {}", pts.join(" "))
        })
        .collect();
    envelopes.sort();
    let ruler_lanes = p
        .ruler_lanes
        .iter()
        .map(|(i, l)| format!("{i} {} {}", l.name, l.flags))
        .collect();
    let mut markers: Vec<String> = p
        .markers
        .values()
        .map(|m: &Marker| {
            format!(
                "{} {:?} color={:?} lane={:?} guid={:?}",
                secs(&m.position),
                m.name,
                m.color,
                m.lane,
                m.guid
            )
        })
        .collect();
    markers.sort();
    let mut regions: Vec<String> = p
        .regions
        .values()
        .map(|r| {
            format!(
                "{}..{} {:?} color={:?} lane={:?} guid={:?}",
                r.time_range.start_seconds(),
                r.time_range.end_seconds(),
                r.name,
                r.color,
                r.lane,
                r.guid
            )
        })
        .collect();
    regions.sort();
    let tempo = p
        .tempo_points
        .iter()
        .map(|tp| {
            format!(
                "{} {} {:?}",
                secs(&tp.position),
                tp.bpm,
                tp.time_signature.map(|s| (s.numerator, s.denominator))
            )
        })
        .collect();
    Snapshot {
        transport: (
            p.transport.tempo.bpm.to_bits(),
            p.transport.time_signature.numerator,
            p.transport.time_signature.denominator,
        ),
        master: (
            p.master_volume.to_bits(),
            p.master_pan.to_bits(),
            p.master_muted,
        ),
        tracks,
        sends: routes(&p.sends),
        hw_outputs: routes(&p.hw_outputs),
        items,
        midi,
        fx,
        envelopes,
        ruler_lanes,
        markers,
        regions,
        tempo,
    }
}

/// Field-by-field, so a failure names what drifted.
fn assert_same(before: &Snapshot, after: &Snapshot) {
    macro_rules! same {
        ($f:ident) => {
            assert_eq!(
                before.$f, after.$f,
                concat!("`", stringify!($f), "` changed")
            );
        };
    }
    same!(transport);
    same!(master);
    same!(tracks);
    same!(sends);
    same!(hw_outputs);
    same!(items);
    same!(midi);
    same!(fx);
    same!(envelopes);
    same!(ruler_lanes);
    same!(markers);
    same!(regions);
    same!(tempo);
}

fn manifest(dir: &Path) -> String {
    let name = dir.file_stem().unwrap().to_string_lossy().into_owned();
    std::fs::read_to_string(dir.join(format!("{name}.session"))).unwrap()
}

// ────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────

/// What a built-in FX was set to — a sampler's kit, a rig's capture —
/// is saved with the session and handed back when it opens; and a session
/// saved again unchanged is the same file.
#[test]
fn a_builtin_fx_keeps_its_state_through_save_and_load() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    let ctx = ProjectContext::Project(guid.clone());
    let fx = Effects::add(&daw, ctx, FxChainContext::Track(DRUMS.to_string()), COUNT)
        .expect("the built-in adds");
    daw.apply_plugin_state(&fx, b"kit=heavy thwacky").unwrap();

    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let first = session_rpp_text(&dir).unwrap();

    let (fresh, _) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let back = fresh
        .read_project(&loaded.project_guid, |p| {
            p.fx_chains
                .get(&FxChainKey::Track(DRUMS.to_string()))
                .and_then(|c| c.iter().find(|e| e.fx.name == COUNT))
                .map(|e| e.fx.guid.clone())
        })
        .flatten()
        .expect("the built-in came back");
    assert_eq!(
        fresh.save_plugin_state(&back).unwrap(),
        b"kit=heavy thwacky"
    );

    let again = song.join("Again.session");
    save_session(&fresh, &loaded.project_guid, &again).unwrap();
    assert_eq!(
        session_rpp_text(&again).unwrap(),
        first,
        "saved again, the same"
    );
}

/// A saved session reopened has the live project's digest — every
/// section, the FX's held state included — and an edit after reopening is
/// named by section.
#[test]
fn a_reopened_session_has_the_live_projects_digest() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let fx = Effects::add(
        &daw,
        ProjectContext::Project(guid.clone()),
        FxChainContext::Track(DRUMS.to_string()),
        COUNT,
    )
    .expect("the built-in adds");
    daw.apply_plugin_state(&fx, b"kit=heavy thwacky").unwrap();
    // A send from the drums' 5/6 (a plugin's own output there) into the
    // keys: its source channels reopen with it.
    let ctx = ProjectContext::Project(guid.clone());
    let index = daw_proto::Routing::add_send(
        &daw,
        ctx.clone(),
        daw_proto::TrackRef::Guid(DRUMS.to_string()),
        daw_proto::TrackRef::Guid(KEYS.to_string()),
    )
    .expect("a send");
    daw_proto::Routing::set_source_channels(
        &daw,
        ctx,
        daw_proto::RouteLocation {
            track: daw_proto::TrackRef::Guid(DRUMS.to_string()),
            route_type: daw_proto::RouteType::Send,
            route: daw_proto::RouteRef::Index(index),
        },
        4,
        2,
    )
    .expect("from 5/6");
    // A parameter of it automated: a ramp, then a step.
    let at = daw_proto::automation::EnvelopeLocation::new(
        daw_proto::TrackRef::Guid(DRUMS.to_string()),
        daw_proto::automation::EnvelopeRef::FxParam {
            fx_guid: fx.clone(),
            param_index: 3,
        },
    );
    for (time, value, shape) in [
        (1.0, 0.25, daw_proto::automation::EnvelopeShape::Linear),
        (2.5, 0.75, daw_proto::automation::EnvelopeShape::Square),
        (4.0, 0.5, daw_proto::automation::EnvelopeShape::Linear),
    ] {
        let _ = daw_proto::Automation::add_point(
            &daw,
            ProjectContext::Project(guid.clone()),
            at.clone(),
            daw_proto::automation::AddPointParams::new(
                daw_proto::primitives::PositionInSeconds::from_seconds(time),
                value,
                shape,
            ),
        );
    }
    let before = daw_standalone::digest::digest(&daw, &guid).expect("a digest");
    assert!(
        before.sections.iter().any(
            |(name, lines)| *name == "envelopes" && lines.iter().any(|l| l.contains("FxParam"))
        ),
        "the automation is in the digest"
    );

    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let (fresh, _) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let after = daw_standalone::digest::digest(&fresh, &loaded.project_guid).expect("a digest");
    let apart = daw_standalone::digest::differences(&before, &after);
    assert!(apart.is_empty(), "the same project: {apart:#?}");

    assert_eq!(
        daw_standalone::digest::fingerprint(&daw, &guid),
        daw_standalone::digest::fingerprint(&fresh, &loaded.project_guid),
        "the same fingerprint"
    );

    // The control: a change after the reopen is seen, by section.
    let back = fresh
        .read_project(&loaded.project_guid, |p| {
            p.fx_chains
                .get(&FxChainKey::Track(DRUMS.to_string()))
                .and_then(|c| c.iter().find(|e| e.fx.name == COUNT))
                .map(|e| e.fx.guid.clone())
        })
        .flatten()
        .expect("the built-in came back");
    fresh.apply_plugin_state(&back, b"kit=another").unwrap();
    let edited = daw_standalone::digest::digest(&fresh, &loaded.project_guid).expect("a digest");
    let apart = daw_standalone::digest::differences(&before, &edited);
    assert!(
        apart.first().is_some_and(|l| l.starts_with("fx:")),
        "{apart:#?}"
    );
    assert_ne!(
        daw_standalone::digest::fingerprint(&daw, &guid),
        daw_standalone::digest::fingerprint(&fresh, &loaded.project_guid),
        "an edit changes the fingerprint"
    );
}

/// Media is written relative to the folder a save is opened from: beside
/// the song, `Media/…`; saved anywhere else, where the audio really is.
#[test]
fn a_session_saved_elsewhere_still_finds_its_media() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    daw_standalone::project_loader::anchor_media(&daw, &guid, &song);
    let files = |dir: &Path| -> Vec<String> {
        session_rpp_text(dir)
            .unwrap()
            .lines()
            .filter_map(|l| l.trim().strip_prefix("FILE ").map(str::to_owned))
            .collect()
    };

    let beside = song.join("Song.session");
    save_session(&daw, &guid, &beside).unwrap();
    assert!(
        files(&beside).iter().all(|f| f.starts_with("\"Media/")),
        "beside the song, relative: {:?}",
        files(&beside)
    );

    let elsewhere = tmp.path().join("Backups").join("Song.session");
    save_session(&daw, &guid, &elsewhere).unwrap();
    let media = song.join("Media").to_string_lossy().into_owned();
    assert!(
        files(&elsewhere).iter().all(|f| f.contains(&media)),
        "elsewhere, where the audio is: {:?}",
        files(&elsewhere)
    );
}

#[test]
fn a_prepared_song_survives_save_and_load() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let before = snapshot(&daw, &guid);

    let dir = song.join("Song.session");
    assert_eq!(save_session(&daw, &guid, &dir).unwrap(), dir);

    let (fresh, factory) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let after = snapshot(&fresh, &loaded.project_guid);
    assert_same(&before, &after);

    // The built-ins came back through the factory, not as placeholders.
    assert_eq!(factory.created.load(Ordering::SeqCst), 2);
    assert!(
        after
            .fx
            .iter()
            .any(|l| l == &format!("{CLICK_TRACK}: {CLICK}"))
    );
    assert!(after.fx.iter().any(|l| l == &format!("{DRUMS}: {COUNT}")));

    // The folder: BUS opens it, Bass closes it, Keys is back outside.
    let parents: Vec<(String, Option<String>, bool)> = fresh
        .read_project(&loaded.project_guid, |p| {
            p.tracks
                .iter()
                .map(|t| (t.name.clone(), t.parent_guid.clone(), t.is_folder))
                .collect()
        })
        .unwrap();
    assert_eq!(
        parents,
        vec![
            ("BUS".into(), None, true),
            ("Drums".into(), Some(BUS.into()), false),
            ("Bass".into(), Some(BUS.into()), false),
            ("Keys 2".into(), None, false),
            ("Click".into(), None, false),
        ]
    );

    // What the engine never modelled is still in the file.
    let text = session_rpp_text(&dir).unwrap();
    for kept in [
        "c29tZSBzdGF0ZQ==",
        "bW9yZSBzdGF0ZQ==",
        "FXID {F0000000-0000-0000-0000-000000000001}",
        "PATTERNSTR ABBB",
        "PLAYOFFS 0 1",
        "VU 2",
        "<OTHERPLUGIN",
        "KEY value",
        "GRID 3199 8 1 8 1 0 0 0",
        "SAMPLERATE 48000 0 0",
        "FILE \"Media/bass.wav\"",
        "<CLAP \"fts.guide:click\" \"fts.guide:click\" \"\"",
    ] {
        assert!(text.contains(kept), "`{kept}` was lost:\n{text}");
    }
    // An untouched item goes out as REAPER wrote it.
    assert!(text.contains("PLAYRATE 1 1 0 -1 0 0.0025"), "{text}");
}

/// An item's label — the name a chord or key item carries, REAPER's
/// `P_NOTES` — is kept, on an item with a take and on one without (the
/// KEY track's empty items). Without it a saved song came back with a
/// blank CHORDS lane.
#[test]
fn item_labels_survive_save_and_load() {
    const KEY_ITEM: &str = "{0000000A-0000-0000-0000-00000000000A}";
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    daw.write_project(&guid, |p| {
        p.items.get_mut(DRUMS_ITEM).unwrap().item.label = Some("4add2".into());
        let mut key = p.items[DRUMS_ITEM].item.clone();
        key.guid = KEY_ITEM.into();
        key.track_guid = KEYS.into();
        key.label = Some("#D".into());
        p.items.insert(KEY_ITEM.into(), ItemEntry { item: key });
        p.items_by_track
            .entry(KEYS.into())
            .or_default()
            .push(KEY_ITEM.into());
        p.takes.remove(KEY_ITEM);
    })
    .unwrap();

    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let (fresh, _) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let labels = fresh
        .read_project(&loaded.project_guid, |p| {
            [DRUMS_ITEM, KEY_ITEM].map(|g| p.items.get(g).and_then(|e| e.item.label.clone()))
        })
        .unwrap();
    assert_eq!(labels, [Some("4add2".to_owned()), Some("#D".to_owned())]);
}

/// Tracks, items, markers and regions made with a caller's GUID (a peer
/// re-creating what another engine made) come back from a `.session` save
/// under exactly that GUID, whatever its spelling — the shared session keys
/// them by it, so a load that re-spelled one would orphan it.
#[test]
fn guids_given_at_creation_survive_save_and_load() {
    use daw_proto::{ItemSpan, Items, Markers, Regions, TimeRange, TrackRef, Tracks};
    const T_BRACED: &str = "{D0000000-0000-4000-8000-000000000001}";
    const T_BARE: &str = "d0000000-0000-4000-8000-000000000002";
    const I_BRACED: &str = "{E0000000-0000-4000-8000-000000000001}";
    const I_BARE: &str = "e0000000-0000-4000-8000-000000000002";
    const M_GUID: &str = "{F0000000-0000-4000-8000-000000000001}";
    const R_GUID: &str = "f0000000-0000-4000-8000-000000000002";

    let check = |daw: &Standalone, guid: &str| {
        let ctx = ProjectContext::Project(guid.to_string());
        let span = |p: f64| {
            ItemSpan::new(
                PositionInSeconds::from_seconds(p),
                Duration::from_seconds(1.0),
            )
        };
        Tracks::add_with_guid(daw, ctx.clone(), T_BRACED, "Peer A", None).unwrap();
        Tracks::add_with_guid(daw, ctx.clone(), T_BARE, "Peer B", Some(0)).unwrap();
        Items::add_item_with_guid(
            daw,
            ctx.clone(),
            TrackRef::Guid(T_BRACED.into()),
            I_BRACED,
            span(1.0),
        )
        .unwrap();
        Items::add_item_with_guid(
            daw,
            ctx.clone(),
            TrackRef::Guid(T_BARE.into()),
            I_BARE,
            span(2.0),
        )
        .unwrap();
        Markers::add_with_guid(daw, ctx.clone(), M_GUID, 3.0, "Peer marker").unwrap();
        Regions::add_with_guid(
            daw,
            ctx.clone(),
            R_GUID,
            TimeRange::from_seconds(4.0, 6.0),
            "Peer region",
        )
        .unwrap();
        let marker_guid_before = Markers::add(daw, ctx.clone(), 7.0, "Local").unwrap();
        let local_marker = Markers::get(daw, ctx.clone(), marker_guid_before)
            .unwrap()
            .guid
            .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Song.session");
        save_session(daw, guid, &dir).unwrap();
        let (fresh, _) = engine();
        let back = load_session(&fresh, "Song", &dir).unwrap();
        let ctx = ProjectContext::Project(back.project_guid.clone());

        let tracks: Vec<_> = Tracks::all(&fresh, ctx.clone())
            .into_iter()
            .map(|t| (t.guid, t.name))
            .collect();
        assert!(
            tracks.contains(&(T_BRACED.into(), "Peer A".into())),
            "{tracks:?}"
        );
        assert!(
            tracks.contains(&(T_BARE.into(), "Peer B".into())),
            "{tracks:?}"
        );
        assert_eq!(tracks[0].0, T_BARE, "at_index survives too");
        for (item, track) in [(I_BRACED, T_BRACED), (I_BARE, T_BARE)] {
            let got = Items::get_item(&fresh, ctx.clone(), daw_proto::ItemRef::Guid(item.into()))
                .unwrap_or_else(|| panic!("item {item} lost its guid"));
            assert_eq!(got.track_guid, track);
        }
        let markers: Vec<_> = Markers::all(&fresh, ctx.clone())
            .into_iter()
            .filter_map(|m| m.guid.map(|g| (g, m.name)))
            .collect();
        assert!(
            markers.contains(&(M_GUID.into(), "Peer marker".into())),
            "{markers:?}"
        );
        assert!(
            markers.contains(&(local_marker, "Local".into())),
            "{markers:?}"
        );
        let regions: Vec<_> = Regions::all(&fresh, ctx)
            .into_iter()
            .filter_map(|r| r.guid.map(|g| (g, r.name)))
            .collect();
        assert!(
            regions.contains(&(R_GUID.into(), "Peer region".into())),
            "{regions:?}"
        );
    };

    // A song opened from its file, and a project built in the engine.
    let tmp = tempfile::tempdir().unwrap();
    let (daw, _, guid) = open_song(&tmp.path().join("Song"));
    check(&daw, &guid);
    let (daw, _) = engine();
    let guid = daw.seed_project(daw_proto::ProjectInfo {
        guid: "built-in-engine".into(),
        name: "Song".into(),
        path: String::new(),
    });
    check(&daw, &guid);
}

/// A project with no file behind it (built in the engine, or opened from
/// text) is written whole from the engine state, and still comes back.
#[test]
fn a_project_with_no_original_is_written_whole() {
    let (daw, _) = engine();
    let loaded = load_rpp_text(&daw, "Song", "", SONG_RPP).unwrap();
    prepare(&daw, &loaded.project_guid);
    let before = snapshot(&daw, &loaded.project_guid);

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Song.session");
    save_session(&daw, &loaded.project_guid, &dir).unwrap();
    let (fresh, factory) = engine();
    let back = load_session(&fresh, "Song", &dir).unwrap();
    assert_same(&before, &snapshot(&fresh, &back.project_guid));
    assert_eq!(factory.created.load(Ordering::SeqCst), 2);
}

#[test]
fn an_unedited_project_saves_as_its_original() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let text = session_rpp_text(&dir).unwrap();
    let norm = |s: &str| {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(norm(&text), norm(SONG_RPP));
}

#[test]
fn saving_a_loaded_session_again_is_byte_identical() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let first_text = session_rpp_text(&dir).unwrap();
    let first_manifest = manifest(&dir);
    let objects = |dir: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir.join("objects"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let first_objects = objects(&dir);

    // Opened and saved over itself, unchanged: the same bytes.
    let (reopened, _) = engine();
    let loaded = load_session(&reopened, "Song", &dir).unwrap();
    save_session(&reopened, &loaded.project_guid, &dir).unwrap();
    assert_eq!(session_rpp_text(&dir).unwrap(), first_text);
    assert_eq!(manifest(&dir), first_manifest);
    assert_eq!(objects(&dir), first_objects);

    // Saved elsewhere, only the document id (minted per new session) and
    // the manifest hash that covers it differ.
    let elsewhere = tmp.path().join("Elsewhere").join("Song.session");
    save_session(&reopened, &loaded.project_guid, &elsewhere).unwrap();
    assert_eq!(session_rpp_text(&elsewhere).unwrap(), first_text);
    let differing: Vec<(String, String)> = first_manifest
        .lines()
        .zip(manifest(&elsewhere).lines())
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .collect();
    assert!(
        differing
            .iter()
            .all(|(a, _)| a.starts_with("id ") || a.starts_with("text_hash ")),
        "{differing:?}"
    );

    // A save over a session with different content leaves none of the old
    // blobs behind.
    reopened
        .write_project(&loaded.project_guid, |p| {
            p.midi_notes.get_mut(CLICK_TAKE).unwrap()[0].pitch = 72;
        })
        .unwrap();
    save_session(&reopened, &loaded.project_guid, &dir).unwrap();
    let (again, _) = engine();
    let back = load_session(&again, "Song", &dir).unwrap();
    assert_same(
        &snapshot(&reopened, &loaded.project_guid),
        &snapshot(&again, &back.project_guid),
    );
    let reachable = std::fs::read_dir(dir.join("objects")).unwrap().count();
    assert_eq!(
        reachable,
        first_objects.len(),
        "stale objects were left behind"
    );
}

/// The prepared "Always On Time", when this machine has it.
#[test]
fn the_organized_session_survives_save_and_load() {
    let rpp = Path::new(
        "/Volumes/build-disk/development/sessions/Always On Time/Always On Time.organized.RPP",
    );
    let Ok(text) = std::fs::read_to_string(rpp) else {
        return;
    };
    let (daw, _) = engine();
    let loaded = load_rpp_text(&daw, "Always On Time", &rpp.to_string_lossy(), &text).unwrap();
    let before = snapshot(&daw, &loaded.project_guid);

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Always On Time.session");
    save_session(&daw, &loaded.project_guid, &dir).unwrap();
    let (fresh, _) = engine();
    let reloaded = load_session(&fresh, "Always On Time", &dir).unwrap();
    assert_same(&before, &snapshot(&fresh, &reloaded.project_guid));

    // Unedited, it is the original's text.
    let norm = |s: &str| {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(norm(&session_rpp_text(&dir).unwrap()), norm(&text));

    // And a second save of the reopened session over the first is the
    // first, byte for byte.
    let first = manifest(&dir);
    save_session(&fresh, &reloaded.project_guid, &dir).unwrap();
    assert_eq!(manifest(&dir), first);

    // Edited the way a prepare pass edits it — every track's fader moved,
    // the order reversed, the first half folded under a new bus — it
    // still comes back as it was saved.
    fresh
        .write_project(&reloaded.project_guid, |p| {
            p.tracks.reverse();
            let mut bus = p.tracks[0].clone();
            bus.guid = BUS.into();
            bus.name = "NEW BUS".into();
            p.track_ext.insert(BUS.into(), Default::default());
            p.items_by_track.insert(BUS.into(), Vec::new());
            let half = p.tracks.len() / 2;
            for (i, t) in p.tracks.iter_mut().enumerate() {
                t.volume *= 0.5;
                t.parent_guid = (i < half).then(|| BUS.to_string());
            }
            bus.parent_guid = None;
            p.tracks.insert(0, bus);
            // A folder is a track something sits in.
            let parents: Vec<Option<String>> =
                p.tracks.iter().map(|t| t.parent_guid.clone()).collect();
            for t in &mut p.tracks {
                t.is_folder = parents.contains(&Some(t.guid.clone()));
            }
        })
        .unwrap();
    let edited = snapshot(&fresh, &reloaded.project_guid);
    let third = tmp.path().join("edited").join("Always On Time.session");
    save_session(&fresh, &reloaded.project_guid, &third).unwrap();
    let (last, _) = engine();
    let back = load_session(&last, "Always On Time", &third).unwrap();
    assert_same(&edited, &snapshot(&last, &back.project_guid));
}

/// A session saved with a live CRDT document hands the same history back
/// on the next open — and a plain save starts it fresh, as before.
#[test]
fn a_session_keeps_the_history_it_was_saved_with() {
    use dawfile_standalone::loro::LoroDoc;
    let (daw, _) = engine();
    let loaded = load_rpp_text(&daw, "Song", "", SONG_RPP).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Song.session");

    let live = LoroDoc::new();
    live.get_text("chart")
        .insert(0, "VS 4\n1 4 6m 5\n")
        .unwrap();
    live.commit();
    live.get_text("chart").insert(0, "IN 2\n1 5\n").unwrap();
    live.commit();
    save_session_with_history(&daw, &loaded.project_guid, &dir, Some(&live)).unwrap();

    let back = load_session_history(&dir).expect("history kept");
    assert_eq!(
        back.get_text("chart").to_string(),
        "IN 2\n1 5\nVS 4\n1 4 6m 5\n"
    );
    assert_eq!(
        back.oplog_vv(),
        live.oplog_vv(),
        "every edit, not just the text"
    );

    save_session(&daw, &loaded.project_guid, &dir).unwrap();
    let fresh = load_session_history(&dir).expect("a plain save still writes a log");
    assert!(fresh.get_text("chart").to_string().is_empty());
}

/// Automation items reopen where they were, with their curves: on a track's
/// volume and on a built-in's parameter, and two instances of one pool
/// still one pool.
#[test]
fn automation_items_reopen_with_their_curves() {
    use daw_proto::automation::{
        AddAutomationItemParams, EnvelopeLocation, EnvelopePoint, EnvelopeRef, EnvelopeShape,
        EnvelopeType, SetAutomationItemPointsParams,
    };
    use daw_proto::{Automation, TrackRef};

    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let ctx = ProjectContext::Project(guid.clone());
    let fx = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(DRUMS.to_string()),
        COUNT,
    )
    .expect("the built-in adds");
    let point = |time: f64, value: f64, shape: EnvelopeShape| EnvelopePoint {
        time: PositionInSeconds::from_seconds(time),
        value,
        shape,
        ..EnvelopePoint::default()
    };
    let volume = EnvelopeLocation::new(
        TrackRef::Guid(DRUMS.to_string()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );
    let param = EnvelopeLocation::new(
        TrackRef::Guid(DRUMS.to_string()),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: 2,
        },
    );
    for at in [volume.clone(), param.clone()] {
        let _ = Automation::add_point(
            &daw,
            ctx.clone(),
            at.clone(),
            daw_proto::automation::AddPointParams::new(
                PositionInSeconds::ZERO,
                0.5,
                EnvelopeShape::Linear,
            ),
        );
        let first = Automation::add_automation_item(
            &daw,
            ctx.clone(),
            at.clone(),
            AddAutomationItemParams::new(
                PositionInSeconds::from_seconds(2.0),
                Duration::from_seconds(2.0),
            ),
        )
        .expect("an item");
        Automation::set_automation_item_points(
            &daw,
            ctx.clone(),
            at.clone(),
            SetAutomationItemPointsParams {
                index: first,
                points: vec![
                    point(0.0, 0.25, EnvelopeShape::SlowStartEnd),
                    point(1.0, 0.75, EnvelopeShape::Square),
                    point(2.0, 0.75, EnvelopeShape::Linear),
                ],
            },
        )
        .expect("its curve");
        // A second instance of the same pool, later on.
        let pool = Automation::automation_items(&daw, ctx.clone(), at.clone())
            .first()
            .map(|ai| ai.pool_id)
            .expect("its pool");
        Automation::add_automation_item(
            &daw,
            ctx.clone(),
            at,
            AddAutomationItemParams::pooled(
                pool,
                PositionInSeconds::from_seconds(6.0),
                Duration::from_seconds(2.0),
            ),
        )
        .expect("another instance");
    }
    let before = daw_standalone::digest::digest(&daw, &guid).expect("a digest");

    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let text = std::fs::read_to_string(dir.join("Song.RPP")).unwrap_or_default();
    let (fresh, _) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let after = daw_standalone::digest::digest(&fresh, &loaded.project_guid).expect("a digest");
    let apart = daw_standalone::digest::differences(&before, &after);
    assert!(apart.is_empty(), "the same automation: {apart:#?}\n{text}");

    let back = ProjectContext::Project(loaded.project_guid.clone());
    let reopened = Automation::automation_items(&fresh, back.clone(), volume.clone());
    assert_eq!(reopened.len(), 2, "{reopened:?}");
    assert!(
        reopened.iter().all(|ai| ai.pool_id == reopened[0].pool_id),
        "still one pool: {reopened:?}"
    );
    // What plays reopens too: the S into the item, and the step in it.
    let at = |t: f64| {
        Automation::value_at(
            &fresh,
            back.clone(),
            volume.clone(),
            PositionInSeconds::from_seconds(t),
        )
    };
    assert!((at(1.0) - 0.5).abs() < 1e-6);
    // A quarter of the way along the S: under the straight line's 0.375.
    assert!(at(2.25) > 0.25 && at(2.25) < 0.36, "eased: {}", at(2.25));
    assert!((at(3.5) - 0.75).abs() < 1e-6);
    assert!((at(5.0) - 0.5).abs() < 1e-6, "the envelope between them");
    assert!(
        (at(7.5) - 0.75).abs() < 1e-6,
        "the second instance plays the pool"
    );
}

/// A pool is the project's: an instance on another track's envelope is the
/// same curve — edited on one, changed on both — and stays one pool through
/// a save and a reopen.
#[test]
fn a_pool_shared_across_tracks_stays_one() {
    use daw_proto::automation::{
        AddAutomationItemParams, EnvelopeLocation, EnvelopePoint, EnvelopeRef, EnvelopeType,
        SetAutomationItemPointsParams,
    };
    use daw_proto::{Automation, TrackRef};

    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let ctx = ProjectContext::Project(guid.clone());
    let volume = |track: &str| {
        EnvelopeLocation::new(
            TrackRef::Guid(track.to_string()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        )
    };
    let point = |time: f64, value: f64| EnvelopePoint {
        time: PositionInSeconds::from_seconds(time),
        value,
        ..EnvelopePoint::default()
    };
    let span = |at: f64| {
        AddAutomationItemParams::new(
            PositionInSeconds::from_seconds(at),
            Duration::from_seconds(2.0),
        )
    };
    let first =
        Automation::add_automation_item(&daw, ctx.clone(), volume(DRUMS), span(2.0)).unwrap();
    let pool = Automation::automation_items(&daw, ctx.clone(), volume(DRUMS))[0].pool_id;
    Automation::add_automation_item(
        &daw,
        ctx.clone(),
        volume(KEYS),
        AddAutomationItemParams::pooled(
            pool,
            PositionInSeconds::from_seconds(6.0),
            Duration::from_seconds(2.0),
        ),
    )
    .unwrap();
    // Edited on the drums: the keys' instance plays it too.
    Automation::set_automation_item_points(
        &daw,
        ctx.clone(),
        volume(DRUMS),
        SetAutomationItemPointsParams {
            index: first,
            points: vec![point(0.0, 0.25), point(2.0, 0.25)],
        },
    )
    .unwrap();
    let keys_curve = Automation::automation_item_points(&daw, ctx.clone(), volume(KEYS), 0);
    assert_eq!(
        keys_curve.len(),
        2,
        "the keys' instance has the curve: {keys_curve:?}"
    );

    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();
    let (fresh, _) = engine();
    let loaded = load_session(&fresh, "Song", &dir).unwrap();
    let back = ProjectContext::Project(loaded.project_guid.clone());
    let (drums, keys) = (
        Automation::automation_items(&fresh, back.clone(), volume(DRUMS)),
        Automation::automation_items(&fresh, back.clone(), volume(KEYS)),
    );
    assert_eq!(
        drums[0].pool_id, keys[0].pool_id,
        "one pool still: {drums:?} {keys:?}"
    );
    // And edited after the reopen, on the keys: the drums follow.
    Automation::set_automation_item_points(
        &fresh,
        back.clone(),
        volume(KEYS),
        SetAutomationItemPointsParams {
            index: 0,
            points: vec![point(0.0, 0.5), point(1.0, 0.75), point(2.0, 0.5)],
        },
    )
    .unwrap();
    assert_eq!(
        Automation::automation_item_points(&fresh, back, volume(DRUMS), 0).len(),
        3
    );
}

/// A song saved by an engine that cannot make one of its built-in FX (a
/// build without that plugin) keeps it: the FX comes back, its state with
/// it, when an engine that can make it opens the song again.
#[test]
fn a_built_in_this_engine_cannot_make_survives_its_save() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let fx = Effects::add(
        &daw,
        ProjectContext::Project(guid.clone()),
        FxChainContext::Track(DRUMS.to_string()),
        COUNT,
    )
    .expect("the built-in adds");
    daw.apply_plugin_state(&fx, b"kit=heavy thwacky").unwrap();
    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();

    // Opened and saved by an engine with no such plugin.
    let without = Standalone::new();
    let blind = load_session(&without, "Song", &dir).unwrap();
    save_session(&without, &blind.project_guid, &dir).unwrap();

    // Opened again by one that has it: there, and as it was.
    let (with, _) = engine();
    let back = load_session(&with, "Song", &dir).unwrap();
    let counts: Vec<String> = with
        .read_project(&back.project_guid, |p| {
            p.fx_chains
                .get(&FxChainKey::Track(DRUMS.to_string()))
                .map(|c| {
                    c.iter()
                        .filter(|e| e.fx.name == COUNT)
                        .map(|e| e.fx.guid.clone())
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    assert!(
        !counts.is_empty(),
        "the built-in survived a save by an engine without it"
    );
    assert!(
        counts.iter().any(|fx| with.save_plugin_state(fx).ok().as_deref() == Some(&b"kit=heavy thwacky"[..])),
        "and what it held"
    );
}

/// A factory that makes only the clicks — an engine built without the
/// counts, as a desktop build without a plugin feature is.
struct ClicksOnly(GuideFactory);

impl FxFactory for ClicksOnly {
    fn installed(&self) -> Vec<InstalledFx> {
        self.0
            .installed()
            .into_iter()
            .filter(|f| f.name == CLICK)
            .collect()
    }
    fn create(&self, name: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        (name == CLICK)
            .then(|| self.0.create(name, sample_rate))
            .flatten()
    }
    fn provides(&self, name: &str) -> bool {
        name == CLICK
    }
}

/// The same when the engine without it CHANGES another built-in on the
/// track: the track's chain is written again, and the one it could not
/// make is still in it, with what it held.
#[test]
fn a_built_in_this_engine_cannot_make_survives_an_edit_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let song = tmp.path().join("Song");
    let (daw, _, guid) = open_song(&song);
    prepare(&daw, &guid);
    let ctx = ProjectContext::Project(guid.clone());
    let chain = FxChainContext::Track(DRUMS.to_string());
    let click = Effects::add(&daw, ctx.clone(), chain.clone(), CLICK).expect("a click");
    daw.apply_plugin_state(&click, b"click=a").unwrap();
    let count = Effects::add(&daw, ctx, chain, COUNT).expect("a count");
    daw.apply_plugin_state(&count, b"kit=heavy thwacky")
        .unwrap();
    let dir = song.join("Song.session");
    save_session(&daw, &guid, &dir).unwrap();

    // Opened by an engine that makes the clicks and not the counts, the
    // click changed, saved.
    let partial = Standalone::new();
    partial.set_fx_factory(Arc::new(ClicksOnly(GuideFactory::default())));
    let opened = load_session(&partial, "Song", &dir).unwrap();
    let clicks: Vec<String> = partial
        .read_project(&opened.project_guid, |p| {
            p.fx_chains
                .get(&FxChainKey::Track(DRUMS.to_string()))
                .map(|c| {
                    c.iter()
                        .filter(|e| e.fx.name == CLICK)
                        .map(|e| e.fx.guid.clone())
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    for fx in &clicks {
        partial.apply_plugin_state(fx, b"click=b").unwrap();
    }
    save_session(&partial, &opened.project_guid, &dir).unwrap();

    let (with, _) = engine();
    let back = load_session(&with, "Song", &dir).unwrap();
    let held: Vec<Vec<u8>> = with
        .read_project(&back.project_guid, |p| {
            p.fx_chains
                .get(&FxChainKey::Track(DRUMS.to_string()))
                .map(|c| c.iter().map(|e| e.fx.guid.clone()).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|fx| with.save_plugin_state(fx).ok())
        .collect();
    assert!(
        held.iter().any(|s| s == b"click=b"),
        "the edit kept: {held:?}"
    );
    assert!(
        held.iter().any(|s| s == b"kit=heavy thwacky"),
        "the count it could not make, kept beside it: {held:?}"
    );
}
