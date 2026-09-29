//! The chord-stress test on the app's REAL render path: the Worship profile
//! as per-lane daw tracks (`KeysRig::open_headless`), rendered by daw's own
//! `ProjectRenderer` at the app's buffer, with the rig limiter on — then
//! five notes at velocity 127, struck again and again.
//!
//! Per block: render time against the realtime deadline, output peak, and
//! non-finite samples. Exit 1 when a block misses the deadline, a sample is
//! non-finite, or the output passes full scale.
//!
//! ```bash
//! FTS_PACK_LIBRARY=… FTS_SAMPLED_ROOT=… \
//!     cargo run --release -p signal-keys --example lane_stress -- [hits] [block]
//! ```

use daw::standalone::audio_engine::render::ProjectRenderer;
use signal_sampler::KeysRig;
use signal_sampler::keys_rig::Scope;

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hits: usize = args.first().and_then(|v| v.parse().ok()).unwrap_or(8);
    let block: u32 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(128);
    let sr = 48_000u32;
    let backend = signal_keys::KeysRigBackend::new();
    let program = backend.debug_profile_lane_program().expect("lane program");
    let rig = KeysRig::open_headless(sr, &program).expect("open_headless");
    rig.set_scope(&Scope::Rig, 0.0, 0.0, 0.0, true);
    let renderer = ProjectRenderer::new(rig.daw(), rig.project_guid(), sr);
    // As the backend holds it: behind a mutex, shared with the status poller.
    let rig = std::sync::Mutex::new(rig);
    renderer.connect_live_midi(256);
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 180 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for _ in 0..400 {
        let _ = renderer.render_block(0, block as usize);
    }
    let deadline = std::time::Duration::from_secs_f64(f64::from(block) / f64::from(sr));
    let busy = std::env::var_os("LANE_STRESS_BUSY").is_some();
    // LANE_STRESS_SMASH=N: N keys across the keyboard struck inside ~50 ms,
    // sustain pedal down the whole time — a forearm on the keys.
    let smash: usize = std::env::var("LANE_STRESS_SMASH").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let chord: Vec<u8> = if smash > 0 {
        (0..smash).map(|i| (36 + (i * 53) % 60) as u8).collect()
    } else {
        vec![48u8, 52, 55, 60, 64]
    };
    if smash > 0 {
        rig.lock().unwrap().cc(64, 127);
    }
    let mut peak_voices = 0usize;
    let per_hit = (sr as usize * 3 / 2) / block as usize;
    let held = per_hit * 2 / 3;
    let (mut times, mut peak, mut nonfinite, mut over) = (Vec::new(), 0.0f32, 0usize, 0usize);
    // A UI polls the rig's status (voice count included) while it plays —
    // that must never cost the audio a block.
    let skips0 = daw::standalone::audio_engine::render::plugin_stage_skips();
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
    scope.spawn(|| {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            if std::env::var_os("LANE_STRESS_OLD_POLL").is_some() {
                // The old status poll: every lane visited UNDER the daw's
                // plugin lock (warm_note walks them the same way).
                let _ = rig.lock().unwrap().warm_note(0, 1);
            } else {
                let _ = rig.lock().unwrap().active_voices();
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
    });
    for b in 0..hits * per_hit {
        let phase = b % per_hit;
        if phase == 0 {
            let r = rig.lock().unwrap();
            for &k in &chord {
                r.note_on(k, 127);
            }
        } else if smash > 0 && phase < 20 {
            // The rest of the forearm lands over the next few blocks.
            let r = rig.lock().unwrap();
            for &k in chord.iter().skip(phase).step_by(20) {
                r.note_on(k, 110);
            }
        } else if phase == held {
            let r = rig.lock().unwrap();
            for &k in &chord {
                r.note_off(k);
            }
        }
        let t = std::time::Instant::now();
        let out = renderer.render_block(0, block as usize);
        let dt = t.elapsed();
        times.push(dt);
        // Headless: publish the load the CPU guard would read from the
        // audio engine, so the guard runs here exactly as in the app.
        // LANE_STRESS_NO_GUARD=1 leaves the guard blind (load 0), for A/B.
        let load = if std::env::var_os("LANE_STRESS_NO_GUARD").is_some() {
            0.0
        } else {
            (dt.as_secs_f64() / deadline.as_secs_f64()) as f32
        };
        signal_sampler::keys_rig::publish_render_load(load);
        if b % 32 == 0 {
            peak_voices = peak_voices.max(rig.lock().unwrap().active_voices());
        }
        if dt > deadline {
            over += 1;
            println!(
                "  late block {:.2} ms at hit {} +{} ms",
                dt.as_secs_f64() * 1e3,
                b / per_hit,
                phase * block as usize * 1000 / sr as usize
            );
        }
        for v in &out.samples {
            if !v.is_finite() {
                nonfinite += 1;
            } else {
                peak = peak.max(v.abs());
            }
        }
        // LANE_STRESS_BUSY=1: no sleeping — pure throughput, stable enough
        // to compare optimisations (a sleeping thread gets moved to
        // efficiency cores and down-clocked, which swamps small changes).
        if !busy {
            if let Some(rest) = deadline.checked_sub(dt) {
                std::thread::sleep(rest);
            }
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let silent = daw::standalone::audio_engine::render::plugin_stage_skips() - skips0;
    println!("  {silent} blocks rendered without instruments (plugin map held by a control thread)");
    let mut sorted = times.clone();
    sorted.sort();
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize].as_secs_f64() * 1e3;
    let mean = times.iter().map(|d| d.as_secs_f64()).sum::<f64>() / times.len() as f64 * 1e3;
    println!("  mean {mean:.3} ms per block ({:.0}% of the deadline)", mean / (deadline.as_secs_f64() * 1e3) * 100.0);
    println!(
        "lanes: {} blocks of {block} (deadline {:.2} ms) — median {:.3} ms, p99 {:.3} ms, max {:.3} ms; {over} over deadline",
        times.len(),
        deadline.as_secs_f64() * 1e3,
        pct(0.5),
        pct(0.99),
        pct(1.0)
    );
    println!("  peak voices {peak_voices}");
    println!("  output peak {:.3} ({:+.1} dBFS); {nonfinite} non-finite samples", peak, 20.0 * peak.max(1e-9).log10());
    if over > 0 || nonfinite > 0 || peak > 1.0 || silent > 0 {
        std::process::exit(1);
    }
}
