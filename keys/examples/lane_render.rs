//! Render one lane of the keys profile offline, exactly as the rig builds it
//! (seeded Omnisphere knobs and all), and print its level over a long held
//! note — for "the synth fades out while I hold it" / "the bass is in the
//! wrong octave" without opening an audio device.
//!
//! ```bash
//! FTS_PACK_LIBRARY=… FTS_SAMPLED_ROOT=… \
//!     cargo run --release -p signal-keys --example lane_render -- <lane> [note] [hold_s] [out.wav]
//! ```

use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;

fn ev(on: bool, key: u8, vel: u8) -> PluginMidiEvent {
    use daw::service::{Channel, KeyNumber, MidiEvent, Velocity};
    PluginMidiEvent {
        offset: 0,
        message: if on {
            MidiEvent::NoteOn { channel: Channel::new(0), key: KeyNumber::new(key), velocity: Velocity::new(vel) }
        } else {
            MidiEvent::NoteOff { channel: Channel::new(0), key: KeyNumber::new(key), velocity: Velocity::new(0) }
        },
    }
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lane = args.first().cloned().unwrap_or_else(|| "Bass".into());
    let note: u8 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(48);
    let hold: f32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(8.0);
    let backend = signal_keys::KeysRigBackend::new();
    let program = backend.debug_profile_program().expect("profile program");
    let tree = program.find(&lane).unwrap_or_else(|| panic!("no lane {lane}")).clone();
    if std::env::var_os("LANE_DUMP").is_some() {
        eprintln!("{}", tree.dump());
    }
    let sr = 48_000u32;
    // LANE_BLOCK / LANE_BPM: the app's buffer and tempo (`LANE_BPM=none`
    // leaves the render graph's default tempo alone, as a host without a
    // tempo would).
    let block: usize = std::env::var("LANE_BLOCK").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
    let mut rn = RenderNode::compile(&tree, sr);
    rn.prepare(f64::from(sr), block as u32);
    match std::env::var("LANE_BPM").as_deref() {
        Ok("none") => {}
        Ok(v) => rn.set_tempo(v.parse().unwrap_or(120.0)),
        Err(_) => rn.set_tempo(120.0),
    }
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 120 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (mut l, mut r) = (vec![0.0f32; block], vec![0.0f32; block]);
    let none = PluginEvents { params: &[], midi: &[], note_expressions: &[] };
    for _ in 0..100 {
        rn.render(&mut l, &mut r, &none);
    }
    let total = ((hold + 1.0) * sr as f32) as usize;
    let off = (hold * sr as f32) as usize;
    let (mut ol, mut or) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let mut voice_log: Vec<usize> = Vec::new();
    let mut t = 0;
    while t < total {
        // LANE_CHORD="64,67": more notes struck with the main one.
        let chord: Vec<u8> = std::env::var("LANE_CHORD")
            .ok()
            .map(|v| v.split(',').filter_map(|k| k.trim().parse().ok()).collect())
            .unwrap_or_default();
        let keys: Vec<u8> = std::iter::once(note).chain(chord).collect();
        // LANE_STAGGER_MS=150: each chord note that much after the one
        // before (omni_render's OMNI_STAGGER_MS), all released together.
        let stagger = std::env::var("LANE_STAGGER_MS")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(0, |ms| (ms / 1000.0 * sr as f32) as usize);
        let mut midi: Vec<PluginMidiEvent> = keys
            .iter()
            .enumerate()
            .filter(|(i, _)| (t..t + block).contains(&(i * stagger)))
            .map(|(_, &k)| ev(true, k, 100))
            .collect();
        if t <= off && off < t + block {
            midi.extend(keys.iter().map(|&k| ev(false, k, 0)));
        }
        l.fill(0.0);
        r.fill(0.0);
        rn.render(&mut l, &mut r, &PluginEvents { params: &[], midi: &midi, note_expressions: &[] });
        ol.extend_from_slice(&l);
        or.extend_from_slice(&r);
        // Voices sounding, every half second (sampler sources).
        if t % (sr as usize / 2) < block {
            voice_log.push(rn.active_voices());
        }
        // Streaming voices refill in the background: run at a sane pace.
        std::thread::sleep(std::time::Duration::from_micros(1500));
        t += block;
    }
    // Level every half second.
    let w = (sr / 2) as usize;
    let levels: Vec<String> = ol
        .chunks(w)
        .map(|c| {
            let rms = (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt();
            format!("{:.0}", 20.0 * (rms + 1e-9).log10())
        })
        .collect();
    println!("{lane} note {note}: dB per 0.5 s: {}", levels.join(" "));
    println!(
        "{lane} note {note}: voices per 0.5 s: {}",
        voice_log.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
    );
    if let Some(out) = args.get(3) {
        let n = ol.len();
        let mut b = Vec::with_capacity(44 + n * 8);
        let data = (n * 8) as u32;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&sr.to_le_bytes());
        b.extend_from_slice(&(sr * 8).to_le_bytes());
        b.extend_from_slice(&8u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data.to_le_bytes());
        for i in 0..n {
            b.extend_from_slice(&ol[i].to_le_bytes());
            b.extend_from_slice(&or[i].to_le_bytes());
        }
        std::fs::write(out, b).expect("write wav");
    }
}
