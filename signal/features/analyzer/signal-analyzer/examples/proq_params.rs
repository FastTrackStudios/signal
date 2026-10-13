//! Read Pro-Q 4's own parameter mappings out of the plugin.
//!
//! The `.ffp` text presets list every band field by name, but several are
//! stored in Pro-Q's internal units rather than the ones the interface shows —
//! `Threshold=0.6666` is not -0.67 dB of anything. Guessing a range from a few
//! preset files gets a curve that is plausible and wrong, which is exactly the
//! failure mode that put the Pro-Q 4 band map off by one earlier in this work.
//!
//! So the mapping is read from the plugin: set the automation parameter, run a
//! block so the write lands, and ask the plugin what it now says the value is.
//! That is the same technique the Valhalla mode tables were recovered with.
//!
//! ```sh
//! cargo run -p signal-analyzer --example proq_params -- \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" --list
//! cargo run -p signal-analyzer --example proq_params -- \
//!     --plugin ... --param "Band 1 Threshold" --steps 20
//! ```

use signal_plugin_host::HostedPlugin;

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 256;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!(
            "usage: proq_params --plugin <path> [--list | --param <name> [--steps N]] \\\n       \
             [--filter <substring>]"
        );
        std::process::exit(2);
    };

    let mut plugin = match HostedPlugin::load(&path) {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("{path}: resolved to the synthetic backend");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{path}: load failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = plugin.prepare(SAMPLE_RATE, BLOCK as u32) {
        eprintln!("prepare failed: {e}");
        std::process::exit(1);
    }
    println!("loaded: {}", plugin.descriptor().name);

    // `--preset` loads a real `.ffp` as the plugin's state. Pro-Q refuses
    // host writes to `Used`, `Dynamic Range`, `Threshold` and the dynamics
    // timing, so setting a band up one parameter at a time is impossible;
    // state is the only door in, and behind it the plugin's own behavior is
    // measurable.
    // `--dump-state <file>` writes the plugin's own state out, which is how
    // the container it expects gets learned: a `.ffp` is the bare FFBS blob,
    // and whatever the plugin wraps around it is the difference between the
    // two.
    if let Some(out) = arg("--dump-state") {
        match plugin.save_state() {
            Ok(bytes) => {
                println!(
                    "state: {} bytes, first 32: {:02x?}",
                    bytes.len(),
                    &bytes[..32.min(bytes.len())]
                );
                std::fs::write(&out, &bytes).expect("write state");
                println!("wrote {out}");
            }
            Err(e) => eprintln!("save_state failed: {e}"),
        }
        return;
    }

    if let Some(preset) = arg("--preset") {
        let bytes = std::fs::read(&preset).expect("read preset");
        let floats: Vec<f32> = if signal_import::fabfilter::parser::is_text_format(&bytes) {
            let text = String::from_utf8_lossy(&bytes);
            signal_import::fabfilter::parser::parse_ffp_text(&text)
                .expect("parse")
                .parameters
                .iter()
                .map(|(_, v)| *v as f32)
                .collect()
        } else {
            signal_import::fabfilter::ffbs::parse(&bytes)
                .expect("parse")
                .params
        };
        // The plugin's state is not a bare FFBS blob: it is `DAW3` + a
        // length, then FFBS, then a metadata trailer naming the preset. Rather
        // than reconstruct that container, take the plugin's own state and
        // splice the preset's floats into it — the vector is a fixed 600
        // entries, so nothing else moves and no length has to be rewritten.
        let mut blob = plugin.save_state().expect("save_state");
        // Presets saved by older Pro-Q 4 builds carry a shorter vector (576
        // against the current 600) — the 24 bands at the front are identical,
        // the tail is globals this build added since. Splice what the preset
        // has and leave the plugin's own defaults for the rest.
        let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
        let n = count.min(floats.len());
        if n != count {
            println!(
                "preset carries {} floats against the plugin's {count}",
                floats.len()
            );
        }
        for (i, v) in floats.iter().take(n).enumerate() {
            let at = 20 + i * 4;
            blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        // `--set-float <index>=<value>` overwrites one slot of the vector
        // before it is loaded. Pro-Q refuses host writes to several band
        // parameters, so this is the only way to put a chosen value in front
        // of the plugin and read back what it calls it — which is how a stored
        // number's real unit gets established rather than guessed.
        for a in std::env::args() {
            let Some((idx, value)) = a.split_once('=') else {
                continue;
            };
            let (Ok(idx), Ok(value)) = (idx.parse::<usize>(), value.parse::<f32>()) else {
                continue;
            };
            if idx < count {
                let at = 20 + idx * 4;
                blob[at..at + 4].copy_from_slice(&value.to_le_bytes());
                println!("set float[{idx}] = {value}");
            }
        }
        match plugin.load_state(&blob) {
            Ok(()) => println!("loaded preset state: {} floats", floats.len()),
            Err(e) => {
                eprintln!("load_state failed: {e}");
                std::process::exit(1);
            }
        }
        let mut warm = vec![0.0f32; BLOCK * 2];
        let _ = plugin.process_interleaved(&mut warm, &[], &[]);
    }

    let params = plugin.params();
    println!("{} parameters", params.len());

    if std::env::args().any(|a| a == "--list") {
        let filter = arg("--filter").unwrap_or_default().to_lowercase();
        for p in &params {
            if filter.is_empty() || p.name.to_lowercase().contains(&filter) {
                println!(
                    "  id {:>5}  {:<44} [{:.4} .. {:.4}]",
                    p.id, p.name, p.min, p.max
                );
            }
        }
        return;
    }

    // `--show <substring>` prints what the plugin currently says each matching
    // parameter is — the readback that turns a stored number into its real
    // unit. With `--preset` in front of it, that is a preset's own values in
    // the plugin's own words.
    if let Some(filter) = arg("--show") {
        let filter = filter.to_lowercase();
        for p in &params {
            if !p.name.to_lowercase().contains(&filter) {
                continue;
            }
            let v = plugin.param_value(p.id).unwrap_or(f64::NAN);
            let text = plugin.value_to_text(p.id, v).unwrap_or_default();
            println!("  {:<40} raw {v:>12.6}  \"{text}\"", p.name);
        }
        return;
    }

    let Some(want) = arg("--param") else {
        eprintln!("need --list or --param <name>");
        std::process::exit(2);
    };
    let Some(info) = params.iter().find(|p| p.name.eq_ignore_ascii_case(&want)) else {
        eprintln!("no parameter named {want:?}");
        std::process::exit(1);
    };

    // Pro-Q ignores writes to a band that is not in use — the readback comes
    // straight back as the default and a sweep looks like a flat line. So
    // `--set` runs any number of `Name=value` writes first, which is how the
    // band gets switched on before the parameter under test is swept.
    for a in std::env::args() {
        let Some((name, value)) = a.split_once('=') else {
            continue;
        };
        let Ok(value) = value.parse::<f64>() else {
            continue;
        };
        let Some(target) = params.iter().find(|p| p.name.eq_ignore_ascii_case(name)) else {
            continue;
        };
        plugin.set_param(target.id, value);
        let mut warm = vec![0.0f32; BLOCK * 2];
        let _ = plugin.process_interleaved(&mut warm, &[], &[]);
        println!(
            "set {}: {value} -> readback {:?}",
            target.name,
            plugin.param_value(target.id)
        );
    }

    let steps: usize = arg("--steps").and_then(|s| s.parse().ok()).unwrap_or(20);
    let mut scratch = vec![0.0f32; BLOCK * 2];
    println!(
        "\n{} — {steps} steps over [{:.4} .. {:.4}]",
        info.name, info.min, info.max
    );
    for k in 0..=steps {
        let v = (info.max - info.min).mul_add(k as f64 / steps as f64, info.min);
        plugin.set_param(info.id, v);
        scratch.fill(0.0);
        if plugin.process_interleaved(&mut scratch, &[], &[]).is_err() {
            eprintln!("  process failed at {v:.4}");
            break;
        }
        let readback = plugin.param_value(info.id).unwrap_or(v);
        match plugin.value_to_text(info.id, readback) {
            Some(text) => println!("  {v:.6}  ->  readback {readback:.6}  \"{text}\""),
            None => println!("  {v:.6}  ->  readback {readback:.6}  (no text)"),
        }
    }
}
