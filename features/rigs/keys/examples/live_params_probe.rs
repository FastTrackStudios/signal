//! Hardware probe: open the REAL keys rig on this machine's audio stack and
//! exercise the realtime parameter surface — held chord, cutoff / envelope
//! sweeps via the same RPCs the remotes call, then both wheels.
//!
//! Prints status lines; listen for: no dropouts during sweeps, filter
//! audibly closing/opening, release edits changing the tail, the pitch
//! wheel bending the held chord and returning to center.
//!
//! ```bash
//! cargo run -p signal-keys --example live_params_probe
//! ```

use std::thread::sleep;
use std::time::Duration;

use signal_keys::KeysRigBackend;
use signal_keys::proto::keys::KeysRig as Svc;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,signal_sampler=warn".into()),
        )
        .init();

    let b = KeysRigBackend::new();
    println!("probe: starting audio…");
    Svc::start(&b);
    for _ in 0..60 {
        if Svc::status(&b).running {
            break;
        }
        sleep(Duration::from_millis(250));
    }
    let st = Svc::status(&b);
    println!(
        "probe: running={} preset={:?} err={:?}",
        st.running, st.loaded_preset, st.last_error
    );
    if !st.running {
        eprintln!("probe: audio did not open — aborting");
        std::process::exit(1);
    }
    // Let the preload get the middle of the keyboard resident.
    sleep(Duration::from_secs(2));

    let mixer = Svc::mixer(&b);
    let lane = mixer
        .engines
        .iter()
        .flat_map(|e| e.layers.iter())
        .find(|l| l.live)
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "Keys A".into());
    println!("probe: driving lane {lane:?}");

    let chord = [60u32, 64, 67];
    let on = |b: &KeysRigBackend| {
        for n in chord {
            Svc::trigger(b, n, 78);
        }
    };
    let off = |b: &KeysRigBackend| {
        for n in chord {
            Svc::trigger(b, n, 0);
        }
    };

    // ── 1. Cutoff sweep on a held chord (live overlay / chain filter) ──
    println!("probe: chord + cutoff sweep down/up (4 s)…");
    on(&b);
    sleep(Duration::from_millis(400));
    {
        let st = Svc::status(&b);
        println!(
            "probe: METERS master={:.4} voices={} lanes={:?}",
            st.master_peak,
            st.voices,
            st.meters
                .iter()
                .filter(|m| m.peak > 1e-5)
                .map(|m| format!("{}:{:.3}", m.name, m.peak))
                .collect::<Vec<_>>()
        );
    }
    for i in 0..40 {
        // 20 kHz → 200 Hz → back, exponential-ish.
        let t = (i as f32 / 39.0) * 2.0; // 0..2
        let phase = if t < 1.0 { 1.0 - t } else { t - 1.0 }; // 1→0→1
        let hz = 200.0 * (100.0f32).powf(phase);
        Svc::set_layer_macro(&b, lane.clone(), 0, "filter.cutoff".into(), hz);
        sleep(Duration::from_millis(100));
    }
    off(&b);
    sleep(Duration::from_millis(600));

    // ── 2. Release-time edit heard on the tail ──
    println!("probe: short vs long release tails…");
    Svc::set_layer_macro(&b, lane.clone(), 0, "env1.release".into(), 60.0);
    on(&b);
    sleep(Duration::from_millis(700));
    off(&b);
    sleep(Duration::from_millis(800)); // short tail
    Svc::set_layer_macro(&b, lane.clone(), 0, "env1.release".into(), 2500.0);
    on(&b);
    sleep(Duration::from_millis(700));
    off(&b);
    sleep(Duration::from_millis(2200)); // long tail rings

    // ── 3. Pitch wheel on a held chord ──
    println!("probe: pitch wheel up / center / down / center…");
    on(&b);
    sleep(Duration::from_millis(500));
    for raw in [16_383u32, 8_192, 0, 8_192] {
        Svc::pitch_bend(&b, raw);
        sleep(Duration::from_millis(700));
    }

    // ── 4. Mod wheel in and out ──
    println!("probe: mod wheel up then down…");
    Svc::mod_wheel(&b, 127);
    sleep(Duration::from_millis(1500));
    Svc::mod_wheel(&b, 0);
    sleep(Duration::from_millis(500));
    off(&b);
    sleep(Duration::from_millis(900));

    let st = Svc::status(&b);
    println!(
        "probe: done — running={} voices={} err={:?}",
        st.running, st.voices, st.last_error
    );
    Svc::stop(&b);
    sleep(Duration::from_millis(300));
    println!("probe: stopped");
}
