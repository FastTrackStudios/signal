//! Live **Keys rig** — hosts a composition-tree preset (a [`RenderNode`]) as one
//! MIDI-driven instrument on daw's audio engine. The MIDI analog of
//! [`GuitarRig`](signal_sampler::rig::GuitarRig), but it plays the Nord-style composition
//! tree: the central MIDI input feeds the tree, whose [`Zone`](signal_sampler::rig_node::Zone)s
//! route notes to layers (splits + velocity crossfades), and the live oscillators
//! (and, later, the rest of the native DSP) render the audio.
//!
//! Same engine pattern as [`SamplerRig`](signal_sampler::SamplerRig): an output-only daw
//! project with one track; the preset is wrapped as a [`KeysInstrument`]
//! (`PluginInstance`) inserted into that track's fx slot, and hardware/UI MIDI is
//! pushed into daw's live-MIDI ring keyed by the track — so the renderer hands it
//! to the instrument each block. Swapping the preset is a glitch-free
//! `insert_plugin_instance` under the renderer lock.

use std::sync::Arc;

use daw::service::{FxChainContext, FxChains, ProjectContext, ProjectInfo, Tracks};
use daw::standalone::Standalone;
use daw::standalone::audio_engine::AudioEngine;
use daw::standalone::metering::Meters;
use daw::standalone::transport_engine::{PlayStateRepr, TransportShared};
use daw_audio_io::AudioIoPrefs;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use signal_sampler::MidiMonitor;
use signal_sampler::node_render::RenderNode;
use signal_sampler::rig_node::Container;

/// Build a channel-0 `MidiEvent::NoteOn` from raw note/velocity.
fn ev_note_on(note: u8, vel: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOn {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(vel),
    }
}

/// Build a channel-0 `MidiEvent::NoteOff` from a raw note.
fn ev_note_off(note: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOff {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(0),
    }
}

/// Build a channel-0 `MidiEvent::ControlChange` from raw controller/value.
fn ev_cc(controller: u8, value: u8) -> midicore::MidiEvent {
    use midicore::{Channel, ControllerNumber, ControllerValue, MidiEvent};
    MidiEvent::ControlChange {
        channel: Channel::new(0),
        controller: ControllerNumber::new(controller),
        value: ControllerValue::new(value),
    }
}

/// Block size the keys instrument is prepared for.
const PREPARE_BLOCK: u32 = 1024;
const KEYS_PROJECT_NAME: &str = "FTS Keys";
const KEYS_TRACK_NAME: &str = "Keys";

/// A composition-tree preset wrapped as a daw instrument. The renderer runs it
/// each block; it ignores audio input and renders the tree from the block's MIDI.
pub struct KeysInstrument {
    render: RenderNode,
    prepared: bool,
}

impl KeysInstrument {
    pub fn new(tree: &Container, sample_rate: u32) -> Self {
        Self {
            render: RenderNode::compile(tree, sample_rate),
            prepared: false,
        }
    }
}

impl PluginInstance for KeysInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.keys.instrument".into(),
            name: "Keys".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _v: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _t: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.render.prepare(sample_rate, block_size);
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        self.render.process(in_l, in_r, out_l, out_r, events);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// A live, device-backed keys rig playing a composition-tree preset.
pub struct KeysRig {
    daw: Standalone,
    _engine: AudioEngine,
    _shared: Arc<TransportShared>,
    #[allow(dead_code)]
    project_guid: String,
    track_guid: String,
    fx_guid: String,
    meters: Arc<Meters>,
    sample_rate: u32,
    preset_name: String,
    midi_monitor: MidiMonitor,
}

impl KeysRig {
    /// Open a device, build the project, and host `tree` as the playable preset.
    // r[impl keys.rig.composition-tree]
    // r[impl keys.rig.output-only]
    pub fn open(prefs: &AudioIoPrefs, tree: &Container) -> eyre::Result<Self> {
        let daw = Standalone::new();
        let project_guid = signal_sampler::rig::uuid_string();
        daw.seed_project(ProjectInfo {
            guid: project_guid.clone(),
            name: KEYS_PROJECT_NAME.to_string(),
            path: String::new(),
        });
        daw.set_current_project(&project_guid);

        // One instrument track carrying the KeysInstrument.
        let track_guid =
            <Standalone as Tracks>::add(&daw, ProjectContext::Current, KEYS_TRACK_NAME, None)
                .map_err(|e| eyre::eyre!("keys rig: add track failed: {e}"))?;
        let fx_ctx = FxChainContext::track(track_guid.clone());
        let idx = <Standalone as FxChains>::add(&daw, fx_ctx.clone(), "keys")
            .map_err(|e| eyre::eyre!("keys rig: reserve fx slot failed: {e}"))?;
        let fx_guid = <Standalone as FxChains>::get(&daw, fx_ctx, idx)
            .map(|fx| fx.guid)
            .ok_or_else(|| eyre::eyre!("keys rig: fx slot vanished after add"))?;

        // Output-only engine (a synth generates, never records).
        let mut io = prefs.clone();
        io.want_input = false;
        let req_rate = if prefs.sample_rate != 0 {
            prefs.sample_rate
        } else {
            48_000
        };
        let shared = Arc::new(TransportShared::new(req_rate, 120.0));
        let engine =
            AudioEngine::with_project_prefs(daw.clone(), project_guid.clone(), shared.clone(), &io)
                .map_err(|e| eyre::eyre!("keys rig: audio engine failed: {e}"))?;
        let sample_rate = engine.sample_rate();

        let meters = Meters::new(1);
        daw.set_meters(meters.clone());

        // Compile + install the preset instrument.
        let mut inst = KeysInstrument::new(tree, sample_rate);
        let _ = inst.prepare(sample_rate as f64, PREPARE_BLOCK);
        daw.insert_plugin_instance(fx_guid.clone(), Box::new(inst));

        shared.set_play_state(PlayStateRepr::Playing);
        tracing::info!(sample_rate, preset = %tree.name, "keys rig started on daw engine");

        Ok(Self {
            daw,
            _engine: engine,
            _shared: shared,
            project_guid,
            track_guid,
            fx_guid,
            meters,
            sample_rate,
            preset_name: tree.name.clone(),
            midi_monitor: MidiMonitor::default(),
        })
    }

    /// Swap the playable preset (glitch-free re-insert under the renderer lock).
    pub fn load_preset(&mut self, tree: &Container) {
        let mut inst = KeysInstrument::new(tree, self.sample_rate);
        let _ = inst.prepare(self.sample_rate as f64, PREPARE_BLOCK);
        self.daw
            .insert_plugin_instance(self.fx_guid.clone(), Box::new(inst));
        self.preset_name = tree.name.clone();
    }

    // ── MIDI driving ─────────────────────────────────────────────────────

    fn dispatch(&self, msg: midicore::MidiEvent) {
        self.daw.push_live_midi(&self.track_guid, msg);
    }

    pub fn note_on(&self, note: u8, velocity: u8) {
        self.dispatch(ev_note_on(note, velocity));
    }
    pub fn note_off(&self, note: u8) {
        self.dispatch(ev_note_off(note));
    }
    pub fn cc(&self, controller: u8, value: u8) {
        self.dispatch(ev_cc(controller, value));
    }
    /// All Notes Off (CC 123).
    pub fn all_notes_off(&self) {
        self.dispatch(ev_cc(123, 0));
    }
    /// Panic — All Sound Off (CC 120).
    pub fn panic(&self) {
        self.dispatch(ev_cc(120, 0));
    }

    /// Enumerate hardware MIDI input ports.
    pub fn midi_input_ports() -> Vec<String> {
        midicore::midir::input_ports()
    }

    /// Open a hardware MIDI keyboard and forward its events into the rig.
    pub fn attach_midi(
        &self,
        selection: midicore::PortSelector,
    ) -> eyre::Result<midicore::midir::MidiInput> {
        let daw = self.daw.clone();
        let track = self.track_guid.clone();
        let monitor = self.midi_monitor.clone();
        midicore::midir::MidiInput::open(selection, move |t: midicore::TimedEvent| {
            monitor.record(&t.event);
            daw.push_live_midi(&track, t.event);
        })
    }

    pub fn midi_monitor(&self) -> MidiMonitor {
        self.midi_monitor.clone()
    }

    // ── Read-side ────────────────────────────────────────────────────────

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn preset_name(&self) -> &str {
        &self.preset_name
    }

    /// Post-fader output peak (linear) of the keys track, for a level meter.
    pub fn output_peak(&self) -> f32 {
        self.meters
            .cell(0)
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

    /// The instrument renders a preset's tree from MIDI — device-free, so it runs
    /// in CI. Plays a note through the layering demo and checks it's audible.
    #[test]
    fn keys_instrument_renders_a_preset_from_midi() {
        let preset = signal_sampler::nord::layering_demo();
        let mut inst = KeysInstrument::new(&preset, 48_000);
        inst.prepare(48_000.0, 256).unwrap();

        let (inl, inr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let (mut outl, mut outr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        // A right-hand note, hard hit → the Bright Lead layer sounds.
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(72, 110),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        inst.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
            .unwrap();
        let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 256.0).sqrt();
        assert!(
            rms > 1e-3,
            "keys instrument should render audible output, rms={rms}"
        );
    }

    /// Machine-local: the full `just keys` render path plays the Keyscape-
    /// realized program (piano through the whole Nord tree). Run explicitly:
    /// `cargo test -p signal-sampler --lib piano_program -- --ignored`
    #[test]
    #[ignore = "requires the local Keyscape extraction on AudioHaven"]
    fn keys_instrument_renders_the_piano_program() {
        let Some(preset) = signal_sampler::nord::nord_stage_piano_preset() else {
            eprintln!("skipping: Keyscape extraction not present");
            return;
        };
        let mut inst = KeysInstrument::new(&preset, 48_000);
        inst.prepare(48_000.0, 512).unwrap();

        let (inl, inr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let (mut outl, mut outr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(60, 100),
        }];
        // Samples decode on a background thread — retrigger until audible.
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            inst.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
                .unwrap();
            let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
            heard = heard.max(rms);
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(heard > 1e-3, "piano program should be audible, rms={heard}");
    }
}
