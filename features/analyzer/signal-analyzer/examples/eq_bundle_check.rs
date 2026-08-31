//! Does the installed FTS EQ bundle render what its parameters say?
//!
//! One bell, written into the plugin as state, measured back out and compared
//! with the engine's own curve for the same band. If a single bell does not
//! come back right, nothing built on top of the plugin can be trusted, and
//! the fault is in the plugin or its bundle rather than in whatever wrote the
//! state.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_bundle_check -- \
//!     --plugin ~/.clap/"FTS EQ.clap"
//! ```

use signal_analyzer::eq_transfer::{self, Stimulus};
use signal_plugin_host::HostedPlugin;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// A state holding exactly one band: a +12 dB bell at 1 kHz, engine Q 1.
fn one_bell() -> Vec<u8> {
    let mut params = serde_json::Map::new();
    params.insert("on_1".into(), serde_json::json!({"f32": 1.0}));
    params.insert("freq_1".into(), serde_json::json!({"f32": 1000.0}));
    params.insert("gain_1".into(), serde_json::json!({"f32": 12.0}));
    // The plugin publishes √2 times the engine's Q.
    params.insert(
        "q_1".into(),
        serde_json::json!({"f32": std::f64::consts::SQRT_2 as f32}),
    );
    params.insert("type_1".into(), serde_json::json!({"i32": 0}));
    params.insert("slope_1".into(), serde_json::json!({"f32": 2.0}));
    params.insert("place_1".into(), serde_json::json!({"i32": 0}));
    for n in 2..=24 {
        params.insert(format!("on_{n}"), serde_json::json!({"f32": 0.0}));
    }
    let doc = serde_json::json!({
        "version": "0.1.0", "params": params, "fields": serde_json::Map::new()
    });
    let json = serde_json::to_vec(&doc).unwrap();
    let mut out = (json.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(&json);
    out
}

fn main() {
    let path = arg("--plugin").unwrap_or_else(|| {
        format!("{}/.clap/FTS EQ.clap", std::env::var("HOME").unwrap_or_default())
    });
    let mut plugin = match HostedPlugin::load(&path) {
        Ok(Some(mut p)) => {
            p.prepare(SR, BLOCK as u32).expect("prepare");
            p
        }
        _ => {
            eprintln!("{path}: could not load");
            std::process::exit(1);
        }
    };
    plugin.load_state(&one_bell()).expect("load_state");

    let (l, r) = eq_transfer::stimulus(
        eq_transfer::frames_needed(),
        10.0f64.powf(-18.8 / 20.0),
        Stimulus::Flat,
        SR,
    );
    let (mut ol, mut or) = (Vec::with_capacity(l.len()), Vec::with_capacity(l.len()));
    let mut pos = 0;
    while pos < l.len() {
        let n = BLOCK.min(l.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = l[pos + i];
            buf[2 * i + 1] = r[pos + i];
        }
        plugin.process_interleaved(&mut buf, &[], &[]).expect("process");
        ol.extend((0..n).map(|i| buf[2 * i]));
        or.extend((0..n).map(|i| buf[2 * i + 1]));
        pos += n;
    }

    let mut engine = eq_dsp::engine::FtsEq::new(SR);
    engine.set_band(
        0,
        eq_dsp::engine::BandConfig {
            used: true,
            enabled: true,
            freq_hz: 1000.0,
            gain_db: 12.0,
            q: 1.0,
            shape: 0,
            slope: 2.0,
            placement: eq_dsp::band::Placement::Stereo,
            stream: 0,
        },
    );

    let (dm, _) = eq_transfer::to_ms(&l, &r);
    let (om, _) = eq_transfer::to_ms(&ol, &or);
    let centres = eq_transfer::band_centres();
    let measured = eq_transfer::response_db(
        &eq_transfer::spectrum(&dm),
        &eq_transfer::spectrum(&om),
        &centres,
        SR,
    );

    println!("  a +12 dB bell at 1 kHz, Q 1\n");
    println!("  {:>8} {:>10} {:>10} {:>8}", "Hz", "bundle", "engine", "diff");
    let mut worst = 0.0f64;
    for (i, hz) in centres.iter().enumerate() {
        let want = engine.static_magnitude_db(*hz);
        worst = worst.max((measured[i] - want).abs());
        println!("  {hz:>8.0} {:>10.2} {want:>10.2} {:>8.2}", measured[i], measured[i] - want);
    }
    println!("\n  worst {worst:.2} dB");
}
