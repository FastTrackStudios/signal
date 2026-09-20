//! What the guitar rig costs to run, measured on the real device.
//!
//!   cargo run --release -p signal-guitar --features signal-sampler/pipewire \
//!     --example rig_bench -- [seconds] [warmup-seconds]
//!
//! # Why a live benchmark and not an offline one
//!
//! The rig's cost is a property of the chain a player is actually running —
//! which captures are resident, which blocks are bypassed, what quantum the
//! graph negotiated — and none of that survives being lifted into a synthetic
//! harness. So this opens the rig for real and reads the same counters the
//! realtime callback writes for the app's own DSP strip. The number it prints
//! is the number the player is subject to.
//!
//! # Warm-up is discarded, and that is the point
//!
//! `EngineStats` accumulates from the moment the device opens, and the first
//! blocks after an open are the most expensive ones the rig will ever render:
//! caches cold, pages unfaulted, captures still being touched for the first
//! time. Averaged in, they flatter a slow build and punish a fast one. So the
//! mean here is a *windowed* mean, reconstructed from the difference between
//! two cumulative samples — total render is `mean × blocks`, so the mean over
//! a window is the difference of the totals over the difference of the counts.
//!
//! Peak is reported for the window too, but note it can only be read
//! cumulatively over the wire: it is the worst block since the open, warm-up
//! included. Read it as a ceiling, and read `drops` for whether any block
//! actually missed its deadline.

use architect::rig::RigBackend as _;
use architect::{LocalServer, Scope};
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::RigPerf;
use signal_guitar::proto::rig::RigClient;

/// A cumulative sample: enough to difference two of them into a window.
struct Sample {
    blocks: u64,
    total_us: f64,
    perf: RigPerf,
}

impl Sample {
    fn of(perf: RigPerf) -> Self {
        Self {
            blocks: perf.blocks,
            // The wire carries the mean, not the total — and the total is what
            // differences cleanly.
            total_us: f64::from(perf.mean_render_us) * perf.blocks as f64,
            perf,
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(10);
    let warmup: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(3);

    let backend = GuitarRigBackend::new();
    eprintln!("opening rig…");
    backend.open_blocking();

    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");

    let sample = |rig: RigClient| async move {
        let status = rig.status().await.expect("status");
        assert!(status.running, "rig is not running — no device opened");
        Sample::of(status.perf)
    };

    eprintln!("warming up for {warmup}s…");
    tokio::time::sleep(std::time::Duration::from_secs(warmup)).await;
    let start = sample(rig.clone()).await;
    assert!(
        start.blocks > 0,
        "no blocks rendered during warm-up — the device is open but silent"
    );

    eprintln!("measuring for {seconds}s…");
    tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
    let end = sample(rig).await;

    let blocks = end.blocks.saturating_sub(start.blocks);
    assert!(blocks > 0, "no blocks rendered during the window");
    let mean_us = (end.total_us - start.total_us) / blocks as f64;
    let p = &end.perf;
    let budget_us = f64::from(p.budget_us().max(1));
    let drops = p.over_budget.saturating_sub(start.perf.over_budget);
    let xruns = p.xruns.saturating_sub(start.perf.xruns);

    println!();
    println!("  guitar rig — {blocks} blocks over {seconds}s");
    println!("  ────────────────────────────────────────────");
    println!(
        "  block          {} frames @ {} Hz  ({:.2} ms budget, {:.1} ms round trip)",
        p.block_frames,
        p.sample_rate,
        budget_us / 1000.0,
        p.buffer_latency_ms(),
    );
    println!(
        "  mean render    {:.3} ms   ({:.1}% of budget)",
        mean_us / 1000.0,
        mean_us / budget_us * 100.0,
    );
    println!(
        "  worst render   {:.3} ms   ({:.1}% of budget, since open)",
        f64::from(p.peak_render_us) / 1000.0,
        f64::from(p.peak_render_us) / budget_us * 100.0,
    );
    println!("  headroom       {:.1}x", budget_us / mean_us.max(1.0));
    println!("  over budget    {drops}   (blocks that missed their deadline)");
    println!("  graph xruns    {xruns}");
    println!();

    if drops > 0 {
        eprintln!("FAIL: {drops} blocks overran their deadline — the player hears this.");
        std::process::exit(1);
    }
}
