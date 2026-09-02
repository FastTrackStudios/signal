//! When does a plugin actually accept a parameter change?
//!
//! Soundtoys' Decapitator takes `Style` on its edit controller — a query
//! returns the new style — while its processor falls silent and stays
//! silent. Every other VST3 in the fleet accepts the same call. Rather than
//! record that as a limitation, this tries the orderings a host can use and
//! reports which one the plugin actually honours.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example param_apply_probe -- \
//!     --plugin "…/Decapitator.vst3" --param Style --value 0.5 [--also Drive=0.6]
//! ```

use signal_analyzer::harmonics::{self, ToneSpec};
use signal_plugin_host::HostedPlugin;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn render(p: &mut HostedPlugin, mono: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(mono.len());
    let mut pos = 0;
    while pos < mono.len() {
        let n = BLOCK.min(mono.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = mono[pos + i];
            buf[2 * i + 1] = mono[pos + i];
        }
        if p.process_interleaved(&mut buf, &[], &[]).is_err() {
            break;
        }
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

fn measure(p: &mut HostedPlugin, freq: f64) -> (f64, f64) {
    let spec = ToneSpec { freq_hz: freq, level_db: -12.0, duration_s: 2.0 };
    let x = harmonics::tone(&spec, SR);
    let y = render(p, &x);
    let h = harmonics::analyze(&y, freq, SR, 8, SR as usize / 2);
    (h.thd_percent, h.fundamental_db + 12.0)
}

fn main() {
    let path = arg("--plugin").expect("--plugin");
    let pname = arg("--param").expect("--param");
    let value: f64 = arg("--value").expect("--value").parse().expect("number");
    let also: Vec<(String, f64)> = arg("--also")
        .map(|s| {
            s.split(';')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.trim().to_string(), v.trim().parse().unwrap_or(0.0)))
                .collect()
        })
        .unwrap_or_default();

    // Resolve ids once, from a prepared instance. Several plugins report no
    // parameters at all until they are activated, so the "set before
    // prepare" ordering below cannot look them up for itself.
    let ids: std::collections::HashMap<String, u32> = {
        let mut probe = match HostedPlugin::load(&path) {
            Ok(Some(mut p)) => {
                p.prepare(SR, BLOCK as u32).unwrap();
                p
            }
            other => {
                eprintln!("could not load: {other:?}");
                std::process::exit(1);
            }
        };
        probe.params().iter().map(|q| (q.name.to_lowercase(), q.id)).collect()
    };
    let id_of = |want: &str| -> u32 {
        *ids.get(&want.to_lowercase())
            .unwrap_or_else(|| panic!("no parameter called {want}"))
    };

    // Every ordering a host might plausibly use, each on a fresh instance.
    let orderings: Vec<(&str, fn(&mut HostedPlugin, u32, f64, &[(u32, f64)]))> = vec![
        ("prepare, then set", |p, id, v, extra| {
            p.prepare(SR, BLOCK as u32).unwrap();
            for (e, ev) in extra {
                p.set_param(*e, *ev);
            }
            p.set_param(id, v);
        }),
        ("set, then prepare", |p, id, v, extra| {
            for (e, ev) in extra {
                p.set_param(*e, *ev);
            }
            p.set_param(id, v);
            p.prepare(SR, BLOCK as u32).unwrap();
        }),
        ("prepare, set, re-prepare", |p, id, v, extra| {
            p.prepare(SR, BLOCK as u32).unwrap();
            for (e, ev) in extra {
                p.set_param(*e, *ev);
            }
            p.set_param(id, v);
            p.prepare(SR, BLOCK as u32).unwrap();
        }),
        ("prepare, set, flush a block, set again", |p, id, v, extra| {
            p.prepare(SR, BLOCK as u32).unwrap();
            for (e, ev) in extra {
                p.set_param(*e, *ev);
            }
            p.set_param(id, v);
            let _ = render(p, &vec![0.0f32; BLOCK]);
            p.set_param(id, v);
        }),
    ];

    println!("{path}\n  setting {pname} = {value}");

    // A fifth route, which is not an ordering but a different mechanism:
    // set the value, save the plugin's state, and load that state into a
    // fresh instance. VST3 parameters that are not automatable cannot be
    // delivered through the process call's parameter queue at all — they
    // travel as component state. If a plugin honours this and ignores the
    // queue, that is what it is telling you.
    {
        let mut src = HostedPlugin::load(&path).ok().flatten().expect("load");
        src.prepare(SR, BLOCK as u32).unwrap();
        for (n, v) in &also {
            src.set_param(id_of(n), *v);
        }
        src.set_param(id_of(&pname), value);
        let _ = render(&mut src, &vec![0.0f32; BLOCK * 4]);
        match src.save_state() {
            Err(e) => println!("  {:<38} save_state failed: {e:?}", "state round-trip"),
            Ok(blob) => {
                let mut dst = HostedPlugin::load(&path).ok().flatten().expect("load");
                dst.prepare(SR, BLOCK as u32).unwrap();
                match dst.load_state(&blob) {
                    Err(e) => println!("  {:<38} load_state failed: {e:?}", "state round-trip"),
                    Ok(()) => {
                        let (thd, gain) = measure(&mut dst, 1000.0);
                        let cur = dst.param_value(id_of(&pname)).unwrap_or(f64::NAN);
                        let txt = dst.value_to_text(id_of(&pname), cur);
                        println!(
                            "  {:<38} THD {thd:>8.4}%  gain {gain:>7.2} dB  latency {:>4}  reads back {:?}",
                            "state round-trip",
                            dst.latency(),
                            txt.unwrap_or_default()
                        );
                    }
                }
            }
        }
    }
    for (label, apply) in orderings {
        let mut p = match HostedPlugin::load(&path) {
            Ok(Some(p)) => p,
            other => {
                eprintln!("could not load: {other:?}");
                std::process::exit(1);
            }
        };
        let id = id_of(&pname);
        let extra: Vec<(u32, f64)> = also.iter().map(|(n, v)| (id_of(n), *v)).collect();
        apply(&mut p, id, value, &extra);
        let (thd, gain) = measure(&mut p, 1000.0);
        let current = p.param_value(id).unwrap_or(f64::NAN);
        let readback = p.value_to_text(id, current);
        // Latency is the tell for a re-init: a plugin that changes its
        // latency has restarted something, and a host that ignores the
        // notification can be left holding a processor that is not running.
        let latency = p.latency();
        println!(
            "  {label:<38} THD {thd:>8.4}%  gain {gain:>7.2} dB  latency {latency:>4}  reads back {:?}",
            readback.unwrap_or_default()
        );
    }
}
