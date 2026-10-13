//! Render the whole keys rig as the app plays it — the Worship profile at
//! its starting levels and switches, hosted by the backend itself, so the
//! live paths (the mod wheel, knobs) reach it exactly as in the app — and
//! write the mix to a WAV, printing its level and brightness per half second.
//!
//! A chord is held for `RIG_HOLD` seconds (default 6). `RIG_WHEEL=127` moves
//! the mod wheel there at 2 s (and back to 0 at 4 s), through the same entry
//! point the app's MIDI tap uses. `RIG_MUTE=Pad,Shimmer` mutes lanes first.
//!
//! ```bash
//! FTS_PACK_LIBRARY=… FTS_SAMPLED_ROOT=… \
//!     cargo run --release -p signal-keys --example rig_render -- out.wav
//! ```

use daw::standalone::audio_engine::render::ProjectRenderer;
use signal_keys_proto::keys::KeysRig as _;

const SR: u32 = 48_000;
const BLOCK: usize = 128;

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let out = std::env::args().nth(1).unwrap_or_else(|| "rig_render.wav".into());
    let env = |k: &str| std::env::var(k).ok();
    let hold: f32 = env("RIG_HOLD").and_then(|v| v.parse().ok()).unwrap_or(6.0);
    let wheel: Option<u32> = env("RIG_WHEEL").and_then(|v| v.parse().ok());
    // RIG_RESTRIKE=5: strike the chord again at 5 s (after the wheel).
    let restrike: Option<usize> = env("RIG_RESTRIKE")
        .and_then(|v| v.parse::<f32>().ok())
        .map(|t| (t * SR as f32) as usize / BLOCK);
    let chord: Vec<u8> = env("RIG_CHORD")
        .map(|v| v.split(',').filter_map(|k| k.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![48, 55, 60, 64, 67]);

    let backend = signal_keys::KeysRigBackend::new();
    assert!(backend.debug_open_headless(SR), "open headless");
    for lane in env("RIG_MUTE").unwrap_or_default().split(',').filter(|l| !l.is_empty()) {
        backend.set_layer_mute(lane.to_string(), true);
    }
    let renderer = backend
        .debug_with_rig(|r| ProjectRenderer::new(r.daw(), r.project_guid(), SR))
        .expect("rig");
    renderer.connect_live_midi(256);
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 180 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for _ in 0..400 {
        let _ = renderer.render_block(0, BLOCK);
    }

    let total = ((hold + 2.0) * SR as f32) as usize / BLOCK;
    let off_at = (hold * SR as f32) as usize / BLOCK;
    let (up_at, down_at) = (2 * SR as usize / BLOCK, 4 * SR as usize / BLOCK);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for b in 0..total {
        let note = |f: &dyn Fn(&signal_sampler::KeysRig)| backend.debug_with_rig(f);
        if b == 0 {
            note(&|rig| chord.iter().for_each(|&k| rig.note_on(k, 90)));
        }
        if Some(b) == restrike {
            note(&|rig| chord.iter().for_each(|&k| rig.note_off(k)));
            note(&|rig| chord.iter().for_each(|&k| rig.note_on(k, 90)));
        }
        if b == off_at {
            note(&|rig| chord.iter().for_each(|&k| rig.note_off(k)));
        }
        // RIG_REBUILD=1: rebuild the program at 2 s (then let it load).
        if b == up_at && std::env::var_os("RIG_REBUILD").is_some() {
            backend.debug_rebuild();
            while signal_sampler::rig::preloads_pending() > 0 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        if let Some(w) = wheel {
            if b == up_at {
                backend.mod_wheel(w);
            }
            if b == down_at {
                backend.mod_wheel(0);
            }
        }
        let out = renderer.render_block(0, BLOCK);
        if b == SR as usize / BLOCK {
            backend.debug_with_rig(|rig| {
                for (role, name, peak) in rig.cell_peaks() {
                    println!("  meter {:>8} {name:<10} {:6.1} dB", role.tag(), 20.0 * peak.max(1e-9).log10());
                }
            });
        }
        for f in out.samples.chunks(2) {
            l.push(f[0]);
            r.push(f.get(1).copied().unwrap_or(f[0]));
        }
        // Real time, so the wheel's worker gets its turn as in the app.
        std::thread::sleep(std::time::Duration::from_secs_f64(BLOCK as f64 / f64::from(SR)));
    }

    let half = SR as usize / 2;
    for (i, (cl, cr)) in l.chunks(half).zip(r.chunks(half)).enumerate() {
        let mono: Vec<f32> = cl.iter().zip(cr).map(|(a, b)| 0.5 * (a + b)).collect();
        let rms = (mono.iter().map(|v| v * v).sum::<f32>() / mono.len() as f32).sqrt();
        // Brightness: high-passed (first difference) energy over total.
        let diff = mono.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>();
        let tot = mono.iter().map(|v| v * v).sum::<f32>().max(1e-20);
        let bright_hz = (diff / tot).sqrt() * SR as f32 / std::f32::consts::TAU;
        println!(
            "{:4.1}s  rms {:6.1} dB  brightness {:6.0} Hz",
            i as f32 * 0.5,
            20.0 * rms.max(1e-9).log10(),
            bright_hz
        );
    }
    write_wav(&out, &l, &r);
    println!("wrote {out}");
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let n = l.len() as u32;
    let mut b = Vec::with_capacity(44 + n as usize * 8);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 8).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&SR.to_le_bytes());
    b.extend_from_slice(&(SR * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 8).to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}
