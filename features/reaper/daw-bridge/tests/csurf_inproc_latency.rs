//! In-process event-bus publish→receive latency probe.
//!
//! Companion to `csurf_subscribe_latency.rs`. Calls a `Diagnostics`
//! RPC whose body runs **entirely on REAPER's main thread**:
//! subscribe to the hub's broadcast receiver, publish to the hub,
//! spin on `try_recv`. Returns microseconds per sample.
//!
//! This is the floor a real in-process subscriber pays (bridge-side
//! OSC/MIDI translator, audio-graph node, inspector tab) once a
//! csurf event has already fired. It excludes:
//!   - REAPER's csurf callback dispatch (REAPER defers that to its
//!     next main loop tick — outside our control, same ceiling
//!     every consumer of csurf pays)
//!   - vox encoding + IPC (the cross-process test measures that)
//!   - tokio worker scheduling (try_recv is synchronous)
//!
//! Result demonstrates the architecture supports in-process
//! subscribers at broadcast-channel speed using the same backend
//! impl that serves cross-process subscribers over vox.
//!
//! Run with:
//!   cargo xtask reaper-test -- csurf_inproc_latency

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

/// In-process hub publish→receive ceiling. broadcast::send +
/// try_recv on the same thread should land in single-digit µs;
/// 500µs covers worst-case scheduler / context jitter. Trips on
/// real regressions (accidental async hop, hub serialization
/// added, etc.).
const INPROC_BUDGET_US: u64 = 500;

#[test]
#[ignore]
fn csurf_inproc_latency() -> Result<()> {
    run_multi_reaper_test(
        "csurf_inproc_latency",
        vec![
            DawInstanceConfig::new("primary")
                .with_env("DISPLAY", "")
                .with_env("FTS_CSURF_MODE", "full")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-csurf-inproc.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let inst = ctx.by_label("primary");

                // Diagnostics service runs the whole loop inside a
                // single main-thread dispatch.
                let latencies_us = inst.daw.diagnostics().hub_publish_latency(10).await?;

                eyre::ensure!(
                    latencies_us.len() == 10,
                    "probe returned {} samples, expected 10",
                    latencies_us.len()
                );

                for (i, us) in latencies_us.iter().enumerate() {
                    println!("  sample {i}: {us} µs ({:.3} ms)", *us as f64 / 1000.0);
                }

                let mut sorted = latencies_us.clone();
                sorted.sort();
                let p50 = sorted[sorted.len() / 2];
                let p99 = sorted[sorted.len() - 1];
                let mean = sorted.iter().sum::<u64>() / sorted.len() as u64;
                println!(
                    "\n  in-process csurf latency — mean: {mean} µs, p50: {p50} µs, p99: {p99} µs",
                );

                eyre::ensure!(
                    p99 < INPROC_BUDGET_US,
                    "p99 {p99} µs exceeded in-process budget {INPROC_BUDGET_US} µs"
                );

                Ok(())
            })
        },
    )
}
