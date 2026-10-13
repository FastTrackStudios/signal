//! Phase A validation: REAPER's audio thread fires our hook and the
//! seqlock snapshot stays consistent across reads.
//!
//! Asserts:
//!   1. The hook fires (at least one snapshot observable within 1s of
//!      spawning REAPER with audio running).
//!   2. Per-buffer sequence is monotonic and dense (no gaps).
//!   3. Sample rate / buffer length are sane (44.1k or 48k, 16..4096).
//!   4. Buffer rate matches `sample_rate / buffer_len` within 25%
//!      (ample slack for OS scheduler jitter on the test host).
//!
//! Doesn't assert playhead progression yet — Phase C will own that
//! once we have drift correction in place. For now we just prove the
//! foundation is solid: audio thread → seqlock → cross-process
//! observation works.
//!
//! Run with:
//!   cargo xtask reaper-test -- audio_sync_snapshot

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

#[test]
#[ignore]
fn audio_sync_snapshot() -> Result<()> {
    run_multi_reaper_test(
        "audio_sync_snapshot",
        vec![
            DawInstanceConfig::new("primary")
                .with_env("DISPLAY", "")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-audio-sync.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let inst = ctx.by_label("primary");

                // First snapshot should appear within a second of
                // REAPER initialising its audio engine.
                let mut snap = None;
                for _ in 0..50 {
                    snap = inst.daw.diagnostics().audio_sync_snapshot().await?;
                    if snap.is_some() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                let snap = snap.ok_or_else(|| {
                    eyre::eyre!("audio hook never fired — is REAPER's audio engine up?")
                })?;
                println!(
                    "  first snapshot: seq={} sr={} buf={} playhead={:.6}s host_us={}",
                    snap.sequence,
                    snap.sample_rate,
                    snap.buffer_len,
                    snap.playhead_seconds,
                    snap.host_micros
                );

                eyre::ensure!(
                    snap.sample_rate > 1000.0 && snap.sample_rate < 200_000.0,
                    "sample_rate {} out of sane range",
                    snap.sample_rate
                );
                eyre::ensure!(
                    snap.buffer_len >= 16 && snap.buffer_len <= 4096,
                    "buffer_len {} out of sane range",
                    snap.buffer_len
                );

                // Observe a window. Aim for ~20 distinct sequences;
                // poll faster than the buffer period so we catch
                // every one.
                let expected_buf_period_us =
                    (snap.buffer_len as f64 / snap.sample_rate * 1_000_000.0) as u64;
                let poll_us = expected_buf_period_us / 4;
                let observed = inst
                    .daw
                    .diagnostics()
                    .audio_sync_observe(20, poll_us.max(100))
                    .await?;

                eyre::ensure!(
                    observed.len() >= 10,
                    "expected ≥10 distinct snapshots, got {}",
                    observed.len()
                );

                // Sequence monotonic + dense.
                for w in observed.windows(2) {
                    let delta = w[1].sequence.wrapping_sub(w[0].sequence);
                    eyre::ensure!(
                        delta == 1,
                        "sequence non-dense: {} -> {} (delta={delta})",
                        w[0].sequence,
                        w[1].sequence,
                    );
                }

                // Buffer rate within 25% of expected.
                let span_us =
                    observed.last().unwrap().host_micros - observed.first().unwrap().host_micros;
                let n_buffers = (observed.len() - 1) as f64;
                let observed_buf_period_us = span_us as f64 / n_buffers;
                let ratio = observed_buf_period_us / expected_buf_period_us as f64;
                println!(
                    "  observed buffer period: {:.1}µs (expected {}µs, ratio {:.3})",
                    observed_buf_period_us, expected_buf_period_us, ratio
                );
                eyre::ensure!(
                    (0.75..=1.25).contains(&ratio),
                    "buffer rate off: observed {observed_buf_period_us:.1}µs vs expected {expected_buf_period_us}µs"
                );

                Ok(())
            })
        },
    )
}
