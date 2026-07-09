//! Guitar rig command — open a saved rig ([`RigManager`]) and play/switch patches.
//!
//! ```text
//! # List audio devices (find your interface's exact name + channel count):
//! cargo run -p signal-sampler --example guitar_rig -- --list
//!
//! # Open the saved "Guitar Rig" (remembers interface/channel/profile):
//! cargo run -p signal-sampler --example guitar_rig
//!
//! # Set it up the first time (and persist with --save):
//! cargo run -p signal-sampler --example guitar_rig -- \
//!     --input "Yamaha TF" --channel 3 \
//!     --profile /run/media/AudioHaven/Signal/NAM/testing/testing.styx \
//!     --save
//! ```
//!
//! Live: type `1`/`2`/… or a patch name to switch, `n`/`p` to step, `b` bypass,
//! `m` meters, `q` quit. All patches are pre-loaded, so switches are instant.

use std::io::BufRead;

use signal_sampler::{GuitarRig, ProfileRig, RigManager};

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "signal_sampler=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // --list: enumerate devices and exit (no audio opened).
    if args.iter().any(|a| a == "--list") {
        println!("INPUT devices (channel index is 0-based):");
        for d in GuitarRig::input_devices() {
            println!(
                "  [{:>2} ch @ {} Hz]  {}",
                d.channels, d.default_sample_rate, d.name
            );
        }
        println!("\nOUTPUT devices:");
        for d in GuitarRig::output_devices() {
            println!(
                "  [{:>2} ch @ {} Hz]  {}",
                d.channels, d.default_sample_rate, d.name
            );
        }
        return Ok(());
    }

    // Load the named rig's saved settings, then apply any CLI overrides.
    let name = arg(&args, "--rig").unwrap_or_else(|| "Guitar Rig".to_string());
    let mut mgr = RigManager::load(&name);
    if let Some(v) = arg(&args, "--input") {
        mgr.audio.input_device = v;
    }
    if let Some(v) = arg(&args, "--output") {
        mgr.audio.output_device = v;
    }
    if let Some(v) = arg(&args, "--channel") {
        if let Ok(n) = v.parse() {
            mgr.audio.input_channel = n;
        }
    }
    if let Some(v) = arg(&args, "--buffer") {
        if let Ok(n) = v.parse() {
            mgr.audio.buffer_size = n; // requested PipeWire quantum (frames)
        }
    }
    if let Some(v) = arg(&args, "--profile") {
        mgr.profile_path = v;
    }
    if args.iter().any(|a| a == "--level-match") {
        mgr.level_match = true;
    }

    // --write-config: persist these settings and exit (no audio opened). Lets
    // you set up a rig without the interface present.
    if args.iter().any(|a| a == "--write-config") {
        match mgr.save() {
            Ok(()) => println!("wrote rig config → {}", mgr.config_path().display()),
            Err(e) => eprintln!("save failed: {e}"),
        }
        return Ok(());
    }

    println!(
        "Opening rig '{}'  (input='{}' ch{}, output='{}', profile='{}')",
        mgr.name,
        if mgr.audio.input_device.is_empty() {
            "default"
        } else {
            &mgr.audio.input_device
        },
        mgr.audio.input_channel,
        if mgr.audio.output_device.is_empty() {
            "default"
        } else {
            &mgr.audio.output_device
        },
        if mgr.profile_path.is_empty() {
            "(none)"
        } else {
            &mgr.profile_path
        },
    );

    let mut prig = mgr.open()?;
    // Remember the actually-opened devices; persist if asked.
    mgr.remember_opened(prig.rig());
    if args.iter().any(|a| a == "--save") {
        match mgr.save() {
            Ok(()) => println!("saved → {}", mgr.config_path().display()),
            Err(e) => eprintln!("save failed: {e}"),
        }
    }

    print_status(&prig);
    println!("\ntype a command (h for help, q to quit)\n");

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        match line {
            "" => {}
            "q" | "quit" => break,
            "h" | "help" => print_help(),
            "n" => {
                let ok = prig.next_patch();
                announce(&prig, ok);
            }
            "p" => {
                let ok = prig.prev_patch();
                announce(&prig, ok);
            }
            "b" => {
                let now = !prig.rig().is_bypassed();
                prig.rig().set_bypass(now);
                println!("bypass {}", if now { "ON (clean DI)" } else { "off (amp)" });
            }
            "m" => print_status(&prig),
            _ if line.chars().all(|c| c.is_ascii_digit()) => {
                let n: usize = line.parse().unwrap_or(0);
                let ok = n >= 1 && prig.activate(n - 1);
                announce(&prig, ok);
            }
            name => {
                let ok = prig.activate_named(name);
                if !ok {
                    eprintln!("no patch {name:?} (or its model didn't load)");
                }
                announce(&prig, ok);
            }
        }
    }
    println!("stopping rig.");
    Ok(())
}

fn announce(prig: &ProfileRig, ok: bool) {
    if ok {
        if let Some(p) = prig.active_patch() {
            println!("→ {}", p.name);
        }
    }
}

fn print_status(prig: &ProfileRig) {
    println!(
        "profile: {}",
        prig.profile_name().unwrap_or("(none loaded)")
    );
    for (i, p) in prig.patches().iter().enumerate() {
        let mark = if Some(i) == prig.active_index() {
            "▶"
        } else {
            " "
        };
        let avail = if prig.is_patch_available(i) {
            ""
        } else {
            "  (model missing)"
        };
        println!("  {mark} [{}] {}{avail}", i + 1, p.name);
    }
    let rig = prig.rig();
    println!(
        "  in {:>6.1} dB | out {:>6.1} dB | underruns {} | overruns {}",
        lin_to_db(rig.input_peak()),
        lin_to_db(rig.output_peak()),
        rig.underruns(),
        rig.overruns(),
    );
    let bf = rig.block_frames().max(1) as f64;
    let sr = rig.sample_rate.max(1) as f64;
    let ring = rig.ring_frames() as f64;
    println!(
        "  buffer {} fr | ring {:.0} fr | ~{:.1} ms software round-trip (+ interface)",
        rig.block_frames(),
        ring,
        (bf + ring + bf) / sr * 1000.0,
    );
}

fn print_help() {
    println!(
        "commands:\n  1..N   switch to patch N\n  <name> switch by name\n  n / p  next / prev\n  \
         b      toggle bypass\n  m      status/meters\n  q      quit"
    );
}

fn lin_to_db(lin: f32) -> f32 {
    if lin <= 1e-9 {
        -120.0
    } else {
        20.0 * lin.log10()
    }
}
