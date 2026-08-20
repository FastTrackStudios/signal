//! **Does mute/solo actually reach the audio?** Boots the rig for real
//! (device + packs), plays a note into two lanes, and reads the per-lane
//! meters back out of `status()` — the same numbers the mixer draws.
//!
//! ```bash
//! cargo run -p signal-keys --example mute_probe
//! ```
//!
//! Each step prints every live lane's peak, so a lane that is supposed to be
//! silenced and isn't shows up as a number instead of a dash.

use std::time::Duration;

use signal_keys_proto::keys::KeysRig as KeysRigSvc;

/// Play `note` and report the loudest each lane got while it rang. Waits for
/// the previous note's tail (and the meters' own fall-back) to clear first —
/// a pad ringing from the last strike would otherwise be read as this one's.
fn strike(backend: &signal_keys::KeysRigBackend, note: u32) -> Vec<(String, f32)> {
    for _ in 0..80 {
        std::thread::sleep(Duration::from_millis(50));
        let loudest = KeysRigSvc::status(backend)
            .meters
            .iter()
            .map(|m| m.peak)
            .fold(0.0f32, f32::max);
        if loudest < 1e-4 {
            break;
        }
    }
    let mut peaks: std::collections::BTreeMap<String, f32> = Default::default();
    KeysRigSvc::trigger(backend, note, 100);
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(25));
        for m in KeysRigSvc::status(backend).meters {
            let e = peaks.entry(m.name).or_insert(0.0);
            *e = e.max(m.peak);
        }
    }
    KeysRigSvc::trigger(backend, note, 0);
    peaks.into_iter().collect()
}

/// Lanes only (an engine's meter is the sum of its lanes; a module's is
/// inside one), loudest first.
fn lanes(peaks: &[(String, f32)], names: &[String]) -> String {
    let mut rows: Vec<String> = names
        .iter()
        .map(|n| {
            let peak = peaks
                .iter()
                .find(|(m, _)| m == n)
                .map(|(_, p)| *p)
                .unwrap_or(0.0);
            if peak <= 1e-5 {
                format!("{n}: —")
            } else {
                format!("{n}: {:.1} dBFS", 20.0 * peak.log10())
            }
        })
        .collect();
    rows.sort();
    rows.join("   ")
}

fn main() {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let backend = signal_keys::KeysRigBackend::new();
    KeysRigSvc::start(&backend);

    // The device + the program open off-thread; wait for a running rig with
    // at least one lane that resolved a pack.
    let mut ready = false;
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(100));
        let st = KeysRigSvc::status(&backend);
        if let Some(err) = st.last_error.as_ref() {
            println!("rig error: {err}");
            return;
        }
        if st.running && !st.meters.is_empty() {
            ready = true;
            break;
        }
    }
    if !ready {
        println!("rig never came up (no device / no packs?) — nothing to probe");
        return;
    }

    let mixer = KeysRigSvc::mixer(&backend);
    let live: Vec<String> = mixer
        .engines
        .iter()
        .flat_map(|e| e.layers.iter())
        .filter(|l| l.live)
        .map(|l| l.name.clone())
        .collect();
    println!("live lanes: {}", live.join(", "));
    let Some(first) = live.first().cloned() else {
        println!("no lane has a resolved patch — nothing to probe");
        return;
    };

    let open = strike(&backend, 60);
    println!("\nopen           {}", lanes(&open, &live));

    // Mute the lane that is actually SOUNDING — muting a silent one proves
    // nothing. ("Pad" is the interesting case: its engine has the same name,
    // and the two used to share one cell.)
    let first = open
        .iter()
        .filter(|(n, _)| live.contains(n))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n.clone())
        .unwrap_or(first);

    KeysRigSvc::set_layer_mute(&backend, first.clone(), true);
    println!("mute {first:<9} {}", lanes(&strike(&backend, 60), &live));
    KeysRigSvc::set_layer_mute(&backend, first.clone(), false);

    KeysRigSvc::set_layer_solo(&backend, first.clone(), true);
    println!("solo {first:<9} {}", lanes(&strike(&backend, 60), &live));
    KeysRigSvc::set_layer_solo(&backend, first.clone(), false);

    println!("clear          {}", lanes(&strike(&backend, 60), &live));
    KeysRigSvc::stop(&backend);
}
