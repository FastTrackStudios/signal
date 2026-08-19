//! `module_dsp` — prove the module's Filter block and Amp Envelope are
//! actually processing: render the same source through three settings and
//! compare what comes out.
//!
//! ```bash
//! cargo run -p signal-synth --example module_dsp
//! ```

use daw::service::{Channel, KeyNumber, MidiEvent, Velocity};
use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;
use signal_sampler::rig_node::Container;
use signal_synth::engine::{signal_module_with, ModuleSettings};
use signal_synth::Source;

const SR: u32 = 48_000;

/// Render `frames` of one module, holding a note the whole time.
fn render(set: &ModuleSettings, frames: usize) -> Vec<f32> {
    let tree = Container::layer("L").add(signal_module_with("M", set));
    let mut node = RenderNode::compile(&tree, SR);
    node.prepare(SR as f64, 512);
    let (mut out_l, mut out_r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let (silence_l, silence_r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let mut all = Vec::with_capacity(frames);
    let mut sent = false;
    while all.len() < frames {
        out_l.fill(0.0);
        out_r.fill(0.0);
        let midi: Vec<PluginMidiEvent> = if sent {
            Vec::new()
        } else {
            sent = true;
            vec![PluginMidiEvent {
                offset: 0,
                message: MidiEvent::NoteOn {
                    channel: Channel::new(0),
                    key: KeyNumber::new(60),
                    velocity: Velocity::new(100),
                },
            }]
        };
        let events = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        node.process(&silence_l, &silence_r, &mut out_l, &mut out_r, &events);
        all.extend_from_slice(&out_l);
    }
    all.truncate(frames);
    all
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn main() {
    let base = ModuleSettings {
        source: Source::Synth,
        ..ModuleSettings::default()
    };

    // 1. Wide open, instant attack — the reference.
    let open = render(&base, SR as usize / 2);
    // 2. Same, filter closed to 200 Hz: a saw through a low cutoff has to be
    //    quieter.
    let closed = render(
        &ModuleSettings {
            cutoff_hz: 200.0,
            ..base.clone()
        },
        SR as usize / 2,
    );
    // 3. Same, with a half-second attack: the first 50 ms must be near silent.
    let slow = render(
        &ModuleSettings {
            amp_env: (500.0, 0.0, 1.0, 200.0),
            ..base.clone()
        },
        SR as usize / 2,
    );

    let (open_rms, closed_rms) = (rms(&open), rms(&closed));
    let head = SR as usize / 20; // 50 ms
    let (slow_head, open_head) = (rms(&slow[..head]), rms(&open[..head]));

    println!("open      rms {open_rms:.5}");
    println!(
        "cutoff200 rms {closed_rms:.5}   ({:.0}% of open)",
        closed_rms / open_rms.max(1e-9) * 100.0
    );
    println!("attack500 first 50ms rms {slow_head:.5}  vs open {open_head:.5}");

    let filter_works = open_rms > 1e-4 && closed_rms < open_rms * 0.7;
    let env_works = open_head > 1e-4 && slow_head < open_head * 0.5;
    println!(
        "\nfilter {} · amp envelope {}",
        if filter_works { "RUNS" } else { "NOT RUNNING" },
        if env_works { "RUNS" } else { "NOT RUNNING" },
    );
    if !(filter_works && env_works) {
        std::process::exit(1);
    }
}
