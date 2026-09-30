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
    // LANE_STRESS_RT=1: the renderer as the live engine runs it — never
    // waiting on the project lock, snapshots built off the audio thread.
    if std::env::var_os("LANE_STRESS_RT").is_some() {
        renderer.set_realtime();
    }
    // LANE_STRESS_CONTROL=1: a control thread moving every lane's fader
    // every 10 ms — each a project mutation, as a knob drag or a stack press
    // sends them — while the rig plays.
    let control = std::env::var_os("LANE_STRESS_CONTROL").is_some();
    // LANE_STRESS_SLOW=1: voices through their per-frame path only — to
    // check the fast path against it (the output hash must match).
    if std::env::var_os("LANE_STRESS_SLOW").is_some() {
        signal_sampler::engine::voice::set_fast_path(false);
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let stats = std::env::var_os("LANE_STRESS_FAST_STATS").is_some();
    signal_sampler::engine::voice::FAST_STATS.store(stats, std::sync::atomic::Ordering::Relaxed);
    let lanes: Vec<String> = rig
        .cell_peaks()
        .into_iter()
        .filter(|(role, _, _)| *role == signal_sampler::rig_node::Role::Layer)
        .map(|(_, name, _)| name)
        .collect();
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
    // (voices, block ms) per block — what a voice costs.
    let mut cost: Vec<(f64, f64)> = Vec::new();
    let per_hit = (sr as usize * 3 / 2) / block as usize;
    let held = per_hit * 2 / 3;
    let (mut times, mut peak, mut nonfinite, mut over) = (Vec::new(), 0.0f32, 0usize, 0usize);
    // A UI polls the rig's status (voice count included) while it plays —
    // that must never cost the audio a block.
    let skips0 = daw::standalone::audio_engine::render::plugin_stage_skips();
    let cuts0 = signal_sampler::keys_rig::guard_cuts();
    let sheds0 = signal_sampler::keys_rig::guard_sheds();
    let steals0 = signal_sampler::engine::voice::note_steals();
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
    scope.spawn(|| {
        let mut tick = 0u32;
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            if control {
                tick += 1;
                let r = rig.lock().unwrap();
                let v = if tick % 2 == 0 { 1.0 } else { 0.99 };
                for lane in &lanes {
                    r.set_lane_volume(signal_sampler::rig_node::Role::Layer, lane, v);
                }
                drop(r);
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
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
        // LANE_STRESS_LOAD_FLOOR=0.9: the app runs heavier than this bare
        // harness (UI, meters, other rigs); publish at least this load so
        // the guard is tested where the app actually sits.
        let floor: f32 = std::env::var("LANE_STRESS_LOAD_FLOOR").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let load = if std::env::var_os("LANE_STRESS_NO_GUARD").is_some() {
            0.0
        } else {
            ((dt.as_secs_f64() / deadline.as_secs_f64()) as f32).max(floor)
        };
        signal_sampler::keys_rig::publish_render_load(load);
        let voices_now = rig.lock().unwrap().active_voices();
        peak_voices = peak_voices.max(voices_now);
        cost.push((voices_now as f64, dt.as_secs_f64() * 1e3));
        // Realtime mode: where the block went (reset every block).
        let profile = rig.lock().unwrap().daw().take_block_profile();
        if dt > deadline {
            over += 1;
            println!(
                "  late block {:.2} ms at hit {} +{} ms{}",
                dt.as_secs_f64() * 1e3,
                b / per_hit,
                phase * block as usize * 1000 / sr as usize,
                profile.map_or(String::new(), |p| format!(
                    " — snapshot {} µs, plugin lock {} µs, fx {} µs, slowest track #{} {} µs",
                    p.snapshot_us, p.plugin_lock_us, p.fx_us, p.slowest_track, p.slowest_track_us
                ))
            );
        }
        for v in &out.samples {
            hash = (hash ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01b3);
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
    // Least squares: block ms = base + per_voice × voices.
    {
        let n = cost.len() as f64;
        let (sx, sy) = cost.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        let (mx, my) = (sx / n, sy / n);
        let (sxy, sxx) = cost.iter().fold((0.0, 0.0), |(a, b), (x, y)| {
            (a + (x - mx) * (y - my), b + (x - mx) * (x - mx))
        });
        let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
        let base = my - slope * mx;
        let fit = |v: f64| base + slope * v;
        println!(
            "  cost: {:.1} µs per voice per block + {:.2} ms base; the {:.2} ms deadline holds ~{:.0} voices",
            slope * 1e3,
            base,
            deadline.as_secs_f64() * 1e3,
            (deadline.as_secs_f64() * 1e3 - base) / slope.max(1e-9)
        );
        for v in [128.0, 256.0, 384.0, 512.0] {
            print!("  [{v:.0} voices → {:.2} ms]", fit(v));
        }
        println!();
    }
    let cuts = signal_sampler::keys_rig::guard_cuts() - cuts0;
    println!("  {cuts} held notes cut by the CPU guard");
    println!(
        "  {} lane-blocks shed release tails; {} notes stolen at a polyphony limit",
        signal_sampler::keys_rig::guard_sheds() - sheds0,
        signal_sampler::engine::voice::note_steals() - steals0
    );
    // Ordinary chords must come through whole, whatever the load.
    let chord_cut = smash == 0 && cuts > 0;
    println!("  output peak {:.3} ({:+.1} dBFS); {nonfinite} non-finite samples", peak, 20.0 * peak.max(1e-9).log10());
    println!("  output hash {hash:016x}");
    if stats {
        let names = ["not sounding", "start hold", "release hold", "attack delay", "gain ramp", "bloom", "decay",
                     "pitch shift", "filter", "flex env", "breakpoint env", "vibrato", "glide", "reverse", "ping-pong", "marker"];
        for (n, c) in names.iter().zip(signal_sampler::engine::voice::FAST_MISS.iter()) {
            let c = c.load(std::sync::atomic::Ordering::Relaxed);
            if c > 0 {
                println!("  fast path missed ({n}): {c} voice-blocks");
            }
        }
    }
    if over > 0 || nonfinite > 0 || peak > 1.0 || silent > 0 || chord_cut {
        std::process::exit(1);
    }
}
