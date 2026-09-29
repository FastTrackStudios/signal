//! `render_patch` — render an Omnisphere `.prt_omn` (or a `.signalpack`) through Signal's own
//! engine to a WAV, timed like daw's `omni_render` (the real-Omnisphere
//! reference harness): the note starts at 0, is held `--hold` s, then the tail
//! runs `--tail` s. Render both, compare them.
//!
//! ```text
//! cargo run -p signal-synth --release --example render_patch -- <patch.prt_omn> <out.wav> \
//!     [--note 48 --vel 100 --hold 1.5 --tail 0.3 --sr 48000 --cc 1=127]
//! ```

use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;
use signal_synth::omni_import::{SoundsourceIndex, load_patch_file};

fn note_event(on: bool, key: u8, vel: u8) -> PluginMidiEvent {
    use daw::service::{Channel, KeyNumber, MidiEvent, Velocity};
    PluginMidiEvent {
        offset: 0,
        message: if on {
            MidiEvent::NoteOn {
                channel: Channel::new(0),
                key: KeyNumber::new(key),
                velocity: Velocity::new(vel),
            }
        } else {
            MidiEvent::NoteOff {
                channel: Channel::new(0),
                key: KeyNumber::new(key),
                velocity: Velocity::new(0),
            }
        },
    }
}

fn write_wav(path: &str, sr: u32, l: &[f32], r: &[f32]) {
    let n = l.len();
    let mut out = Vec::with_capacity(44 + n * 8);
    let data = (n * 8) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&(sr * 8).to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        out.extend_from_slice(&l[i].to_le_bytes());
        out.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(path, out).expect("write wav");
}

/// Keep only the Soundsource in every Oscillator container (a bisect aid).
fn source_only(c: &signal_sampler::rig_node::Container) -> signal_sampler::rig_node::Container {
    use signal_sampler::rig_node::RigNode;
    let mut out = c.clone();
    if out.name == "Oscillator" {
        out.children.retain(
            |ch| matches!(ch, RigNode::Block { block } if block.display_name() == "Soundsource"),
        );
    }
    for ch in &mut out.children {
        if let RigNode::Container { container } = ch {
            *container = source_only(container);
        }
    }
    out
}

fn main() {
    // `RUST_LOG=warn` surfaces the sampler's streaming / import warnings.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(patch), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: render_patch <patch.prt_omn> <out.wav> [--note N --vel V --hold S --tail S --sr HZ]"
        );
        std::process::exit(2);
    };
    let opt = |name: &str, default: f32| -> f32 {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (note, vel) = (opt("--note", 48.0) as u8, opt("--vel", 100.0) as u8);
    let (hold, tail, sr) = (
        opt("--hold", 1.5),
        opt("--tail", 0.3),
        opt("--sr", 48_000.0) as u32,
    );

    // The real index, so sample layers resolve their soundsources (an empty
    // one leaves them silent). Point `FTS_OMNISPHERE_ROOT` etc. at the local
    // extraction.
    // A `.signalpack` plays as a keys layer would hold it: one Sampler block
    // as the Soundsource of an Oscillator module (NI / Keyscape pianos).
    let tree = if patch.ends_with(".signalpack") {
        use signal_sampler::rig_node::Container;
        let mut src = signal_sampler::RigBlock::sample_lib(patch.as_str());
        src.name = "Soundsource".into();
        // `--source-param name=value`: build-time params on the Sampler
        // block, as a keys layer sets them (e.g. `piano_snapshot=worship`).
        for kv in args.windows(2).filter(|w| w[0] == "--source-param").map(|w| &w[1]) {
            if let Some((k, v)) = kv.split_once('=') {
                src = src.with_param(k, v);
            }
        }
        Container::preset("pack").add(Container::module("Oscillator").add(src))
    } else {
        load_patch_file(std::path::Path::new(patch), &SoundsourceIndex::scan_default())
            .unwrap_or_else(|e| panic!("import {patch}: {e}"))
    };
    if std::env::var_os("RENDER_DEBUG").is_some() {
        if let Ok(xml) = std::fs::read_to_string(patch) {
            if let Ok(p) = signal_synth::omni_import::parse_patch(&xml) {
                for (i, l) in p.layers.iter().enumerate() {
                    eprintln!(
                        "layer {i}: enabled {} level {:.3} ss {:?} waves {:?} shape {:.2}",
                        l.enabled,
                        l.level,
                        l.soundsource,
                        l.waves.as_ref().map(|w| &w.0),
                        l.osc_shape
                    );
                }
            }
        }
        for n in ["Layer A", "Layer B", "Layer C", "Layer D"] {
            if let Some(c) = tree.find(n) {
                eprintln!("{n}: output_db {:.1}", c.output_db);
            }
        }
        if let Some(amp) = tree.find("Layer A").and_then(|l| l.find("Amp Stage")) {
            for b in amp.blocks() {
                eprintln!("Layer A amp block {:?} {:?}", b.display_name(), b.params);
            }
        }
        for n in ["Layer A", "Layer B", "Layer C", "Layer D"] {
            if let Some(osc) = tree.find(n).and_then(|l| l.find("Oscillator")) {
                for b in osc.blocks() {
                    eprintln!("{n} osc block {:?} {:?}", b.display_name(), b.params);
                }
            }
        }
    }
    let tree = if std::env::var_os("RENDER_BARE").is_some() {
        use signal_sampler::rig_node::Container;
        let osc = tree
            .find("Layer A")
            .and_then(|l| l.find("Oscillator"))
            .and_then(|o| {
                o.blocks()
                    .into_iter()
                    .find(|b| b.display_name() == "Soundsource")
                    .cloned()
            })
            .expect("layer A soundsource");
        Container::preset("bare").add(Container::module("Oscillator").add(osc))
    } else {
        tree
    };
    let tree = if std::env::var_os("RENDER_SOURCE_ONLY").is_some() {
        source_only(&tree)
    } else {
        tree
    };
    let mut rn = RenderNode::compile(&tree, sr);
    let block = 256usize;
    rn.prepare(f64::from(sr), block as u32);
    // `--bpm`: the host tempo (synced LFOs, envelopes, delays).
    rn.set_tempo(opt("--bpm", 120.0));
    // Let sample sources finish their first loads before the note (the
    // reference harness pre-rolls too).
    let (mut l, mut r) = (vec![0.0f32; block], vec![0.0f32; block]);
    // `--cc N=V` controller values, sent with the first pre-roll block.
    let ccs: Vec<PluginMidiEvent> = args
        .windows(2)
        .filter(|w| w[0] == "--cc")
        .filter_map(|w| {
            use daw::service::{Channel, ControllerNumber, ControllerValue, MidiEvent};
            let (n, v) = w[1].split_once('=')?;
            Some(PluginMidiEvent {
                offset: 0,
                message: MidiEvent::ControlChange {
                    channel: Channel::new(0),
                    controller: ControllerNumber::new(n.parse().ok()?),
                    value: ControllerValue::new(v.parse().ok()?),
                },
            })
        })
        .collect();
    // `--prev N`: a note held from late in the pre-roll and released just
    // after the main note (legato), for glide.
    let prev = args
        .iter()
        .position(|a| a == "--prev")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u8>().ok());
    let prev_on: Vec<PluginMidiEvent> =
        prev.map(|k| note_event(true, k, vel)).into_iter().collect();
    // Pre-roll long enough for streaming packs to open their zones (a
    // cache miss drops the voice): `RENDER_PREROLL_MS`, default 200.
    let preroll_blocks = std::env::var("RENDER_PREROLL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(40, |ms| (ms / 5).max(40));
    // Wait for the sample blocks to finish opening their zones (bounded),
    // so the note never races the preload.
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 120 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for i in 0..preroll_blocks {
        let midi: &[PluginMidiEvent] = match i {
            0 => &ccs,
            i if i == preroll_blocks - 5 => &prev_on,
            _ => &[],
        };
        let ev = PluginEvents {
            params: &[],
            midi,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let total = ((hold + tail) * sr as f32) as usize;
    let off_at = (hold * sr as f32) as usize;
    let (mut out_l, mut out_r) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let mut t = 0usize;
    while t < total {
        let midi: Vec<PluginMidiEvent> = if t == 0 {
            let mut m = vec![note_event(true, note, vel)];
            if let Some(k) = prev {
                let mut off = note_event(false, k, 0);
                off.offset = 1;
                m.push(off);
            }
            m
        } else if t <= off_at && off_at < t + block {
            vec![note_event(false, note, 0)]
        } else {
            Vec::new()
        };
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        l.fill(0.0);
        r.fill(0.0);
        rn.render(&mut l, &mut r, &ev);
        out_l.extend_from_slice(&l);
        out_r.extend_from_slice(&r);
        t += block;
    }
    out_l.truncate(total);
    out_r.truncate(total);
    write_wav(out, sr, &out_l, &out_r);
    let rms = (out_l.iter().map(|v| v * v).sum::<f32>() / total.max(1) as f32).sqrt();
    println!("{out}: {:.2} s, rms {rms:.4}", total as f32 / sr as f32);
}
