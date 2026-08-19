//! `pack_hold_rms` — play a bare soundsource (`.signalpack` or `library.styx`)
//! holding one note, and print per-second RMS. Isolates whether a soundsource
//! *sustains* (its loop works) from any patch filter/envelope processing.
//!
//! ```text
//! cargo run -p signal-synth --release --example pack_hold_rms -- <spec_path> [note]
//! ```

use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;
use signal_sampler::rig_node::Container;

fn main() {
    // Surface the sampler's preload / cache-miss / zone tracing. `RUST_LOG`
    // overrides; default to signal_sampler debug so the sample path is visible.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("signal_sampler=debug")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut args = std::env::args().skip(1);
    let spec = args
        .next()
        .expect("usage: pack_hold_rms <spec_path> [note]");
    let note: u8 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);

    // Minimal tree: one layer, one sample block realized by the spec.
    let tree = Container::preset("probe").add(
        Container::engine("e")
            .add(Container::layer("A").add(Container::module("src").sample_block("Source", spec))),
    );

    let mut rn = RenderNode::compile(&tree, 48_000);
    rn.prepare(48_000.0, 512);
    let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
    let midi = [PluginMidiEvent {
        offset: 0,
        message: daw::service::MidiEvent::NoteOn {
            channel: daw::service::Channel::new(0),
            key: daw::service::KeyNumber::new(note),
            velocity: daw::service::Velocity::new(100),
        },
    }];

    let per_sec = 48_000 / 512;
    let mut sec = 0usize;
    let mut acc = 0.0f32;
    let mut n = 0;
    println!("holding note {note} for 6 s — per-second peak RMS:");
    // Retrigger the note EVERY block for the first ~2 s (the sampler drops a
    // note-on whose sample isn't cached yet — it must be re-sent until the
    // background preload has decoded the zone), then hold silently and measure
    // whether it SUSTAINS via the loop.
    let warm = per_sec * 2;
    for b in 0..(per_sec * 8) {
        let ev = PluginEvents {
            params: &[],
            midi: if b < warm { &midi } else { &[] },
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        let rms = (l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
        acc = acc.max(rms);
        n += 1;
        if b < 90 {
            std::thread::sleep(std::time::Duration::from_millis(4)); // let streaming fill
        }
        if n >= per_sec {
            println!("  t={sec}s  peak_rms={acc:.5}");
            sec += 1;
            acc = 0.0;
            n = 0;
        }
    }
}
