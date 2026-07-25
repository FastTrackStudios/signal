//! `pedal_tree` — the sustain pedal through the program the rig actually
//! plays: a compiled layer → module → Source/Filters/Amp tree.
//!
//! A CC that never reaches the sampler is a pedal that does nothing, and the
//! tree is where it could get lost (zone filtering, module routing). This
//! plays a note with the pedal down, lifts the key, and checks the note is
//! still sounding — then lifts the pedal and checks it falls away.
//!
//! ```bash
//! cargo run -p signal-synth --release --example pedal_tree -- [pack]
//! ```

use daw::service::{Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, Velocity};
use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;
use signal_sampler::rig_node::Container;
use signal_synth::Source;
use signal_synth::engine::{ModuleSettings, signal_layer_with};

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/LA Custom C7 Grand.signalpack";

fn main() {
    let pack = std::env::args().nth(1).unwrap_or_else(|| PACK.to_string());
    let set = ModuleSettings {
        source: Source::Sample(pack.clone()),
        ..ModuleSettings::default()
    };
    let tree = Container::parallel("Rig").add(signal_layer_with("Keys A", &[set]));
    let mut node = RenderNode::compile(&tree, 48_000);
    node.prepare(48_000.0, 512);

    let ev = |m: MidiEvent| PluginMidiEvent { offset: 0, message: m };
    let note_on = ev(MidiEvent::NoteOn {
        channel: Channel::new(0),
        key: KeyNumber::new(60),
        velocity: Velocity::new(100),
    });
    let note_off = ev(MidiEvent::NoteOff {
        channel: Channel::new(0),
        key: KeyNumber::new(60),
        velocity: Velocity::new(0),
    });
    let pedal = |v: u8| {
        ev(MidiEvent::ControlChange {
            channel: Channel::new(0),
            controller: ControllerNumber::new(64),
            value: ControllerValue::new(v),
        })
    };

    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let (sl, sr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let mut run = |node: &mut RenderNode, midi: Vec<PluginMidiEvent>, blocks: usize| -> f32 {
        let mut peak = 0.0f32;
        for b in 0..blocks {
            l.fill(0.0);
            r.fill(0.0);
            let m = if b == 0 { midi.clone() } else { Vec::new() };
            let events = PluginEvents { params: &[], midi: &m, note_expressions: &[] };
            node.process(&sl, &sr, &mut l, &mut r, &events);
            peak = peak.max(l.iter().fold(0.0f32, |a, s| a.max(s.abs())));
            std::thread::sleep(std::time::Duration::from_micros(10_667));
        }
        peak
    };

    run(&mut node, Vec::new(), 40); // let the streamer warm
    let sounding = run(&mut node, vec![pedal(127), note_on], 40);
    let key_up = run(&mut node, vec![note_off], 90);
    // After the pedal lifts, watch it fall: a pad's release is long, so the
    // test is that it keeps decaying, not that it hits zero on a stopwatch.
    let mut tail = Vec::new();
    let mut midi = vec![pedal(0)];
    for _ in 0..4 {
        tail.push(run(&mut node, std::mem::take(&mut midi), 90));
    }

    println!("pedal down + key down   peak {sounding:.4}");
    println!("key UP, pedal held      peak {key_up:.4}   (must keep sounding)");
    println!(
        "pedal up, decaying      {}   (must fall away)",
        tail.iter().map(|v| format!("{v:.4}")).collect::<Vec<_>>().join(" → "),
    );

    let held = sounding > 1e-3 && key_up > sounding * 0.15;
    let released = tail.last().copied().unwrap_or(1.0) < tail[0] * 0.5;
    println!(
        "\npedal holds notes: {} · releases them: {}",
        if held { "YES" } else { "NO" },
        if released { "YES" } else { "NO" },
    );
    if !(held && released) {
        std::process::exit(1);
    }
}
