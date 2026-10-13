//! The Worship profile, live: a GigPerformer-style patch switcher over the
//! standalone guitar rig. Every patch's NAM amp is pre-loaded into the bank, so
//! switching patches is instant and click-free.
//!
//! ```text
//! # From a profile file (edit the .nam paths in it first):
//! cargo run -p signal-sampler --example worship_rig -- crates/signal-sampler/examples/worship.styx
//!
//! # Or build a profile inline from name=path pairs:
//! cargo run -p signal-sampler --example worship_rig -- Clean=clean.nam Lead=lead.nam Ambient=pad.nam
//! ```
//!
//! Live controls (stdin):
//! ```text
//!   1, 2, 3 ...   switch to patch N
//!   <name>        switch to a patch by name (e.g. `lead`)
//!   n / p         next / previous patch  (footswitch-style)
//!   b             toggle bypass (clean DI)
//!   m             meters + status
//!   q             quit
//! ```
//!
//! Optional: `RIG_IN` / `RIG_OUT` env vars substring-match input/output devices.

use std::io::BufRead;
use std::path::Path;

use signal_sampler::{ProfileRig, RigPatch, RigProfile};

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "signal_sampler=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: worship_rig <profile.styx> | <Name=model.nam> ...");
        std::process::exit(2);
    }

    let in_dev = std::env::var("RIG_IN").ok();
    let out_dev = std::env::var("RIG_OUT").ok();
    let rig = signal_sampler::GuitarRig::with_devices(
        in_dev.as_deref(),
        out_dev.as_deref(),
        Some(48_000),
        Some(256),
    )?;
    let mut prig = ProfileRig::new(rig);

    // Either a single .styx profile file, or inline `Name=path.nam` pairs.
    if args.len() == 1 && args[0].ends_with(".styx") {
        let path = Path::new(&args[0]);
        if let Err(e) = prig.load_profile_file(path) {
            eprintln!("failed to load profile {}: {e}", args[0]);
            eprintln!("(edit the model_path entries to point at real .nam files)");
        }
    } else {
        let mut profile = RigProfile::new("Worship");
        for arg in &args {
            match arg.split_once('=') {
                Some((name, path)) => profile = profile.with_patch(RigPatch::amp(name, path)),
                None => eprintln!("ignoring {arg:?} — expected Name=path.nam"),
            }
        }
        if let Err(e) = prig.load_profile(profile, None) {
            eprintln!("failed to load profile: {e}");
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
                switched(&prig, ok);
            }
            "p" => {
                let ok = prig.prev_patch();
                switched(&prig, ok);
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
                switched(&prig, ok);
            }
            name => {
                let ok = prig.activate_named(name);
                if !ok {
                    eprintln!("no patch {name:?} (or its model didn't load)");
                }
                switched(&prig, ok);
            }
        }
    }
    println!("stopping rig.");
    Ok(())
}

fn switched(prig: &ProfileRig, ok: bool) {
    if ok {
        if let Some(p) = prig.active_patch() {
            println!("→ {} ", p.name);
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
