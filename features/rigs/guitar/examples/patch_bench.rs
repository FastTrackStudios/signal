//! Will this patch — or every patch a setlist reaches — play without drops?
//!
//!   cargo run --release -p signal-guitar --example patch_bench -- \
//!     [--profile Worship] [--patch NAME]... [--setlist "CYA 9-24-26"] \
//!     [--block 64] [--rate 48000] [--secs 3]
//!
//! Offline, on the rig's own engine, exactly as the live rig plays: every
//! selected patch loaded into one rig (as on stage), each switched in by the
//! footswitch path (`activate` + the chain's bypasses), the DI reference
//! playing. Each block of `--block` frames is timed against its realtime
//! budget (`block / rate`):
//!
//! - **switch**: the first `--secs` after switching in — the outgoing
//!   patch's delay and reverb tails still ringing, the switch's own work;
//! - **steady**: the `--secs` after that.
//!
//! Per patch: mean, 99th percentile and worst block as % of the budget, and
//! how many blocks went over. A block over 100 % is a dropout on a real
//! device; the live device also has the OS and the other apps to share with,
//! so treat anything above ~60 % as a risk on stage. Exits non-zero when any
//! block went over, so it can gate a set.
//!
//! Uses the live config (`XDG_CONFIG_HOME`), writes nothing.

use std::sync::Arc;
use std::time::Instant;

use signal_guitar::library::RigLibrary;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigProfile};

struct Args {
    profile: Option<String>,
    patches: Vec<String>,
    setlist: Option<String>,
    block: usize,
    rate: u32,
    secs: f64,
    /// Blocks to force bypassed after each switch (by chain name), to see
    /// what a block costs.
    bypass: Vec<String>,
    /// Also print the patches' chains (the backed blocks, on/off).
    chains: bool,
    /// Per-block DSP and latency for each patch, instead of the switch run.
    blocks: bool,
}

fn args() -> Args {
    let mut a = Args { profile: None, patches: Vec::new(), setlist: None, block: 64, rate: 48_000, secs: 3.0, bypass: Vec::new(), chains: false, blocks: false };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if k == "--chains" {
            a.chains = true;
            continue;
        }
        if k == "--blocks" {
            a.blocks = true;
            continue;
        }
        let v = it.next().unwrap_or_default();
        match k.as_str() {
            "--profile" => a.profile = Some(v),
            "--patch" => a.patches.push(v),
            "--setlist" => a.setlist = Some(v),
            "--block" => a.block = v.parse().unwrap_or(64),
            "--rate" => a.rate = v.parse().unwrap_or(48_000),
            "--secs" => a.secs = v.parse().unwrap_or(3.0),
            "--bypass" => a.bypass.push(v),
            _ => eprintln!("unknown flag {k}"),
        }
    }
    a
}

#[derive(Default)]
struct Stats {
    us: Vec<f64>,
}

impl Stats {
    fn line(&mut self, budget_us: f64) -> (String, usize) {
        if self.us.is_empty() {
            return ("—".into(), 0);
        }
        self.us.sort_by(f64::total_cmp);
        let n = self.us.len();
        let mean = self.us.iter().sum::<f64>() / n as f64;
        let p99 = self.us[(n * 99 / 100).min(n - 1)];
        let max = self.us[n - 1];
        let over = self.us.iter().filter(|u| **u > budget_us).count();
        let pct = |u: f64| u / budget_us * 100.0;
        (
            format!("mean {:5.1}%  p99 {:5.1}%  max {:6.1}%  over {over:4}", pct(mean), pct(p99), pct(max)),
            over,
        )
    }
}

fn main() {
    let a = args();
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let def = match &a.profile {
        None => lib.profile.clone(),
        Some(n) => lib
            .profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(n))
            .cloned()
            .unwrap_or_else(|| panic!("no profile {n:?}")),
    };

    // Which patches: the named ones; else the setlist's reach (the
    // profile's own patches and the songs' patches); else all.
    let set_songs: Option<Vec<String>> = a.setlist.as_ref().map(|s| {
        lib.setlists
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(s))
            .unwrap_or_else(|| panic!("no setlist {s:?}"))
            .entries
            .iter()
            .map(|e| e.song.clone())
            .collect()
    });
    let wanted = |p: &signal_guitar::profiles::PatchDef| -> bool {
        if !a.patches.is_empty() {
            return a.patches.iter().any(|n| n.eq_ignore_ascii_case(&p.name));
        }
        match &set_songs {
            Some(songs) => p.song.is_empty() || songs.iter().any(|s| s.eq_ignore_ascii_case(&p.song)),
            None => true,
        }
    };

    let mut built = signal_guitar::nodes::profile_from_library(&def, &lib.drive_presets);
    let comp = RigLibrary::load_compositions();
    for patch in &mut built.patches {
        if let Some(d) = def.patches.iter().find(|d| d.name.eq_ignore_ascii_case(&patch.name)) {
            signal_guitar::macros::apply_positions(d, &comp, patch);
        }
    }
    let chosen: Vec<_> = built
        .patches
        .iter()
        .filter(|p| def.patches.iter().any(|d| d.name.eq_ignore_ascii_case(&p.name) && wanted(d)))
        .cloned()
        .collect();
    if chosen.is_empty() {
        eprintln!("no patches selected");
        std::process::exit(2);
    }

    if a.blocks {
        let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(a.rate));
        let input: Vec<f32> = di.samples.iter().map(|&s| s as f32).collect();
        let budget = a.block as f64 / f64::from(a.rate) * 1e6;
        for p in &chosen {
            let playing: Vec<_> = p
                .chain
                .iter()
                .filter(|b| b.has_backend() && !b.bypassed && !a.bypass.iter().any(|n| n.eq_ignore_ascii_case(&b.name)))
                .cloned()
                .collect();
            let costs = signal_sampler::block_profile::profile_chain(&playing, a.rate, a.block, &input, a.secs + 1.0);
            let sum: f64 = costs.iter().map(|c| c.mean_us).sum();
            println!("\n{}  — {:.1}% of the {}-frame budget ({:.0} µs), all blocks", p.name, sum / budget * 100.0, a.block, budget);
            println!("  {:<22} {:<16} {:>8} {:>8} {:>8} {:>8}", "block", "kind", "mean %", "p99 %", "max %", "latency");
            for c in &costs {
                if let Some(e) = &c.error {
                    println!("  {:<22} {:<16} failed: {e}", c.name, c.kind);
                    continue;
                }
                println!(
                    "  {:<22} {:<16} {:>7.1}% {:>7.1}% {:>7.1}% {:>8}",
                    c.name,
                    c.kind,
                    c.mean_us / budget * 100.0,
                    c.p99_us / budget * 100.0,
                    c.max_us / budget * 100.0,
                    if c.latency == 0 { "0".to_string() } else { format!("{} smp", c.latency) }
                );
            }
        }
        return;
    }

    let rig = GuitarRig::open_offline(a.rate).expect("offline rig");
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("bench");
    profile.patches = chosen.clone();
    let t = Instant::now();
    prig.load_profile(profile, None).expect("profile builds");
    println!(
        "{} patches built in {:.1} s · block {} @ {} Hz · budget {:.0} µs",
        chosen.len(),
        t.elapsed().as_secs_f64(),
        a.block,
        a.rate,
        a.block as f64 / f64::from(a.rate) * 1e6
    );

    let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(a.rate));
    let samples: Arc<Vec<f32>> = Arc::new(di.samples.iter().map(|&s| s as f32).collect());
    let budget_us = a.block as f64 / f64::from(a.rate) * 1e6;
    let blocks = (a.secs * f64::from(a.rate) / a.block as f64) as usize;

    // Warm the first patch so its switch is measured against a playing rig.
    prig.activate(0);
    signal_guitar::measure::apply_chain_bypass(&prig);
    prig.rig().start_test_signal(samples.clone());
    prig.rig().render_offline(a.rate as usize);

    let mut worst = 0usize;
    println!("{:<28} {:<58} {}", "patch", "switch (first secs, tails ringing)", "steady");
    for i in 0..chosen.len() {
        let target = (i + 1) % chosen.len();
        prig.activate(target);
        signal_guitar::measure::apply_chain_bypass(&prig);
        let backed: Vec<_> = chosen[target].chain.iter().filter(|b| b.has_backend()).collect();
        for (b, id) in backed.iter().zip(prig.active_block_ids()) {
            if a.bypass.iter().any(|n| n.eq_ignore_ascii_case(&b.name)) {
                prig.rig().set_block_slot_bypass(&id, true);
            }
        }
        if a.chains {
            let on: Vec<String> = backed
                .iter()
                .filter(|b| !b.bypassed && !a.bypass.iter().any(|n| n.eq_ignore_ascii_case(&b.name)))
                .map(|b| b.name.clone())
                .collect();
            println!("  {} plays: {}", chosen[target].name, on.join(" · "));
        }
        prig.rig().start_test_signal(samples.clone());
        let mut sw = Stats::default();
        let mut st = Stats::default();
        for k in 0..blocks * 2 {
            let t = Instant::now();
            prig.rig().render_offline(a.block);
            let us = t.elapsed().as_secs_f64() * 1e6;
            if k < blocks { sw.us.push(us) } else { st.us.push(us) }
        }
        let (s1, o1) = sw.line(budget_us);
        let (s2, o2) = st.line(budget_us);
        worst += o1 + o2;
        println!("{:<28} {s1}   {s2}", chosen[target].name);
    }
    if worst > 0 {
        println!("{worst} blocks over budget");
        std::process::exit(1);
    }
    println!("no block over budget");
}
