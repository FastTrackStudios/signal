//! `patch_hold_rms` — import a full `.prt_omn`, hold one note, print per-second
//! RMS. Shows the decay shape so we can tell what chokes a held note (filter
//! env vs amp env vs mod engine). Optionally strip the Filters modules to
//! bisect the cause.
//!
//! ```text
//! cargo run -p signal-synth --release --example patch_hold_rms -- "<patch.prt_omn>" [note] [--no-filters]
//! ```

use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;
use signal_sampler::rig_node::{Container, RigNode};
use signal_synth::omni_import::{load_patch_file, SoundsourceIndex};

/// Clone the tree, dropping any container named "Filters".
fn strip_filters(c: &Container) -> Container {
    let mut out = c.clone();
    out.children.retain(
        |ch| !matches!(ch, RigNode::Container { container } if container.name == "Filters"),
    );
    for ch in out.children.iter_mut() {
        if let RigNode::Container { container } = ch {
            *container = strip_filters(container);
        }
    }
    out
}

/// In every "Oscillator" container, keep only the Sampler block (drop the
/// native Wavetable/Harmonia/etc. that would generate their own tone in series).
fn sample_only_osc(c: &Container) -> Container {
    use signal_proto::block::BlockType;
    let mut out = c.clone();
    if out.name == "Oscillator" {
        out.children.retain(
            |ch| matches!(ch, RigNode::Block { block } if block.block_type == BlockType::Sampler),
        );
    }
    for ch in out.children.iter_mut() {
        if let RigNode::Container { container } = ch {
            *container = sample_only_osc(container);
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .expect("usage: patch_hold_rms <patch.prt_omn> [note] [--no-filters]");
    let note: u8 = args
        .iter()
        .skip(1)
        .find_map(|a| a.parse().ok())
        .unwrap_or(60);
    let no_filters = args.iter().any(|a| a == "--no-filters");
    let sample_only = args.iter().any(|a| a == "--sample-only");

    let index = SoundsourceIndex::scan_default();
    let mut tree = load_patch_file(std::path::Path::new(path), &index).expect("import");
    if no_filters {
        tree = strip_filters(&tree);
        eprintln!("(filters stripped)");
    }
    if sample_only {
        tree = sample_only_osc(&tree);
        eprintln!("(oscillator stack reduced to the Sample block only)");
    }

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
    let warm = per_sec * 2;
    let mut sec = 0usize;
    let mut acc = 0.0f32;
    let mut n = 0;
    println!("hold note {note} 10 s (retrigger first 2 s) — per-second peak RMS:");
    for b in 0..(per_sec * 10) {
        let ev = PluginEvents {
            params: &[],
            midi: if b < warm { &midi } else { &[] },
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        acc = acc.max((l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt());
        n += 1;
        if b < warm {
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        if n >= per_sec {
            println!("  t={sec}s  peak_rms={acc:.5}");
            sec += 1;
            acc = 0.0;
            n = 0;
        }
    }
}
