//! Watch the RUNNING keys rig's realtime health, live.
//!
//! `rt_probe` opens its own rig, which means it cannot be used while the
//! desktop app holds the audio device — and the session you actually care
//! about is the one you are playing. This attaches to the engine over vox
//! instead and prints what the audio thread is doing, once a second.
//!
//! ```bash
//! cargo run --release -p signal-keys --example rt_watch
//! ```
//!
//! The two halves answer different questions, and the rig has been failing
//! the second while passing the first:
//!
//! - **late**  — `xruns` / `over_budget` / render time. Was the callback on
//!   time?
//! - **wrong** — `holes` / `clicks`. Did on-time audio come out correct? A
//!   hole is silence with signal on both sides, which is what a starved
//!   stream or a stranded voice produces and what a listener hears as
//!   crackle. Neither deadline counter moves while it happens.
use std::time::Duration;

use signal_keys_proto::keys::KeysRigClient;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".into());
    let link = vox_websocket::WsLink::connect(&url)
        .await
        .map_err(|e| eyre::eyre!("connect {url}: {e:?} — is the engine running?"))?;
    let rig: KeysRigClient = vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| eyre::eyre!("KeysRig handshake: {e:?}"))?;

    // `--play` drives chords through the rig's own `trigger` RPC — the
    // on-screen piano's path — so the watcher can produce the load it is
    // measuring. Without it, it waits for a human at the keyboard.
    let play = std::env::args().any(|a| a == "--play");
    if play {
        let rig2: KeysRigClient = vox_core::initiator_on(
            vox_websocket::WsLink::connect(&url)
                .await
                .map_err(|e| eyre::eyre!("connect (player): {e:?}"))?,
        )
        .establish()
        .await
        .map_err(|e| eyre::eyre!("handshake (player): {e:?}"))?;
        tokio::spawn(async move {
            // Twenty notes at a time: ten fingers with an octave doubler is
            // an ordinary way to play this rig, and it is where the reported
            // glitching lives.
            const CHORDS: [[u32; 4]; 4] = [
                [60, 64, 67, 72],
                [57, 60, 64, 69],
                [53, 57, 60, 65],
                [55, 59, 62, 67],
            ];
            let mut i = 0usize;
            loop {
                let chord = CHORDS[i % CHORDS.len()];
                for oct in 0..5u32 {
                    for n in chord {
                        let note = n + oct * 12;
                        if note <= 108 {
                            let _ = rig2.trigger(note, 100).await;
                        }
                    }
                }
                i += 1;
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
        });
    }

    println!("watching {url} — play the rig; ctrl-c to stop\n");
    println!(
        "{:>6} {:>7} {:>9} {:>9} {:>7} {:>6} {:>7} {:>8} {:>7}",
        "voices", "blocks", "mean_ms", "peak_ms", "budget", "xruns", "over", "holes", "clicks"
    );

    let (mut last_blocks, mut last_holes, mut last_clicks, mut last_over) =
        (0u64, 0u64, 0u64, 0u64);
    loop {
        let s = rig
            .status()
            .await
            .map_err(|e| eyre::eyre!("status: {e:?}"))?;
        let rt = &s.rt;
        let budget_ms = if rt.block_frames > 0 {
            f64::from(rt.block_frames) / 48_000.0 * 1000.0
        } else {
            0.0
        };
        // Deltas, not totals: a counter that has been climbing since the rig
        // opened says nothing about whether it is happening NOW.
        println!(
            "{:>6} {:>7} {:>9.3} {:>9.3} {:>7.2} {:>6} {:>7} {:>8} {:>7}",
            s.voices,
            rt.blocks.saturating_sub(last_blocks),
            rt.mean_render_ms,
            rt.peak_render_ms,
            budget_ms,
            rt.xruns,
            rt.over_budget.saturating_sub(last_over),
            rt.holes.saturating_sub(last_holes),
            rt.clicks.saturating_sub(last_clicks),
        );
        (last_blocks, last_holes, last_clicks, last_over) =
            (rt.blocks, rt.holes, rt.clicks, rt.over_budget);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
