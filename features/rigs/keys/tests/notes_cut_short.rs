//! Normal playing must never have a note cut short by the CPU guard — the
//! Dolceola above all, whose notes ring for a second after key-up and
//! whose tails were being faded mid-song.
//!
//! The whole Worship rig as the app hosts it (the backend's own rig,
//! headless, realtime renderer), played the way a worship set is: sustain
//! pedal down and re-caught on each change, left-hand chords with the bass
//! note, a right-hand eighth-note melody. Every lane's cut report comes from
//! `lane_health`, the counters the app's log reads.
//!
//! Needs the sample libraries (`FTS_PACK_LIBRARY`, `FTS_SAMPLED_ROOT`,
//! `FTS_OMNISPHERE_ROOT`); skipped without them. Its own process: it points
//! the config dir at a temp dir so the player's saved profile, stacks and
//! MIDI learn are not used or touched.

use daw::standalone::audio_engine::render::ProjectRenderer;

const SR: u32 = 48_000;
const BLOCK: usize = 128;

#[test]
fn normal_playing_cuts_no_note_short() {
    if std::env::var_os("FTS_PACK_LIBRARY").is_none() {
        eprintln!("skipped: no sample libraries (FTS_PACK_LIBRARY)");
        return;
    }
    let dir = std::env::temp_dir().join(format!("keys-cuts-{}", std::process::id()));
    // SAFETY: the only test in this binary, before any thread reads the env.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::remove_var("FTS_KEYS_PROFILE");
    }

    let backend = signal_keys::KeysRigBackend::new();
    assert!(backend.debug_open_headless(SR), "open the rig headless");
    let renderer = backend
        .debug_with_rig(|r| ProjectRenderer::new(r.daw(), r.project_guid(), SR))
        .expect("rig");
    renderer.set_realtime();
    renderer.connect_live_midi(1024);
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 240 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for _ in 0..400 {
        let _ = renderer.render_block(0, BLOCK);
    }
    let _ = signal_sampler::lane_health::reports(); // a clean window

    // 72 BPM: a bar is 3.33 s, an eighth 0.417 s. Four bars, played twice.
    let eighth = 60.0 / 72.0 / 2.0;
    let chords: [&[u8]; 4] = [
        &[36, 48, 55, 60, 64],   // C
        &[35, 47, 55, 59, 62],   // G/B
        &[33, 45, 52, 57, 60],   // Am
        &[29, 41, 53, 57, 60],   // F
    ];
    let melody: [u8; 8] = [72, 74, 76, 79, 76, 74, 72, 67];
    // (time s, event) — 0 note-off, 1 note-on, 2 pedal.
    let mut events: Vec<(f64, u8, u8)> = Vec::new();
    for rep in 0..2 {
        for (b, chord) in chords.iter().enumerate() {
            let bar = (rep * 4 + b) as f64 * eighth * 8.0;
            // Pedal re-caught just after the change.
            events.push((bar, 2, 0));
            for &k in *chord {
                events.push((bar, 1, k));
                events.push((bar + eighth * 7.5, 0, k));
            }
            events.push((bar + 0.05, 2, 127));
            for (i, &k) in melody.iter().enumerate() {
                let t = bar + i as f64 * eighth;
                events.push((t, 1, k));
                events.push((t + eighth * 0.9, 0, k));
            }
        }
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    let end = events.last().map_or(0.0, |e| e.0) + 3.0;

    let deadline = BLOCK as f64 / f64::from(SR);
    let mut loads: Vec<(f32, i64)> = Vec::new();
    let mut next = 0;
    let blocks = (end / deadline) as usize;
    for b in 0..blocks {
        let now = b as f64 * deadline;
        while next < events.len() && events[next].0 <= now {
            let (_, kind, v) = events[next];
            backend.debug_with_rig(|rig| match kind {
                0 => rig.note_off(v),
                1 => rig.note_on(v, 92),
                _ => rig.cc(64, v),
            });
            next += 1;
        }
        let t = std::time::Instant::now();
        let _ = renderer.render_block(0, BLOCK);
        // The load the live engine would report for this block.
        let load = (t.elapsed().as_secs_f64() / deadline) as f32;
        signal_sampler::keys_rig::publish_render_load(load);
        loads.push((load, signal_sampler::keys_rig::total_voices()));
    }

    {
        let mut l: Vec<f32> = loads.iter().map(|x| x.0).collect();
        l.sort_by(f32::total_cmp);
        let pct = |p: f64| l[((l.len() - 1) as f64 * p) as usize];
        let over = l.iter().filter(|&&x| x > 1.0).count();
        let busiest = loads.iter().max_by_key(|x| x.1).copied().unwrap_or_default();
        eprintln!(
            "render load: median {:.2}, p99 {:.2}, max {:.2}; {over} blocks over the deadline; \
             most voices {} (load {:.2} that block)",
            pct(0.5), pct(0.99), pct(1.0), busiest.1, busiest.0
        );
    }
    let lanes = signal_sampler::lane_health::reports();
    let rig_peak: u32 = lanes.iter().map(|l| l.peak_voices).sum();
    for l in &lanes {
        eprintln!(
            "{:<10} peak {:>3} voices, faded {:>3} (shed: load {:>3} / count {:>3} blocks; cut {:>3} blocks), stolen {}",
            l.name, l.peak_voices, l.faded, l.shed_by_load, l.shed_by_count, l.cut_blocks, l.stolen
        );
    }
    eprintln!("rig peak (sum of lane peaks): {rig_peak} voices");
    let faded: Vec<&signal_sampler::lane_health::LaneReport> =
        lanes.iter().filter(|l| l.faded > 0).collect();
    assert!(faded.is_empty(), "normal playing had voices faded by the CPU guard: {faded:?}");
    let dolceola = lanes.iter().find(|l| l.name == "Synth 1").expect("the Dolceola lane");
    assert!(dolceola.peak_voices > 0, "the Dolceola played");

    let _ = std::fs::remove_dir_all(dir);
}
