//! How long a patch switch takes, and where the time goes.
//!
//!   cargo run --release -p signal-guitar --features signal-sampler/pipewire \
//!     --example switch_bench -- [switches]
//!
//! A switch has two halves and only one of them is audible. Activating the
//! patch swaps which preinstalled chain the graph runs — that is the part a
//! player hears, and it should be immediate. Everything after it makes the
//! rest of the rig agree: the chain mirror the UI draws, the delays' tempo,
//! the boost, the drive trims. That half is not audible *as a switch*, but it
//! is audible as a level shift if it lands after the first note, which is what
//! "not gapless" actually sounds like.
//!
//! So this reports both, per switch, and the per-stage split comes from the
//! `patch switch` event the session emits. Run with `RUST_LOG=info` to see the
//! stages; the summary here is the wall clock a player is subject to.
//!
//! Runs silent and ephemeral by default: a measurement must not move the
//! player's position, edit their profile, or be heard. Set
//! `SIGNAL_RIG_SILENT=0` / `SIGNAL_RIG_EPHEMERAL=0` to override.

use architect::rig::RigBackend as _;
use architect::{LocalServer, Scope};
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::RigClient;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // A benchmark leaves no trace and makes no sound unless told otherwise.
    // Defaults rather than forced values, so a run can still be listened to.
    // SAFETY: single-threaded, before the rig or any thread is created.
    unsafe {
        for flag in ["SIGNAL_RIG_SILENT", "SIGNAL_RIG_EPHEMERAL"] {
            if std::env::var_os(flag).is_none() {
                std::env::set_var(flag, "1");
            }
        }
    }


    let switches: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(12);

    let backend = GuitarRigBackend::new();
    eprintln!("opening rig…");
    backend.open_blocking();

    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");

    let model = rig.perf().await.expect("perf");
    let stacks = model.stacks.len();
    assert!(stacks > 0, "profile has no footswitch stacks to switch between");
    eprintln!("{stacks} stacks; {switches} switches…\n");

    // Let the rig settle: the first switch after an open pays for anything the
    // open left cold, and that is not the number a player lives with.
    let _ = rig.press_stack(0).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut times = Vec::with_capacity(switches);
    for i in 0..switches {
        let target = (i % stacks) as u32;
        let t = std::time::Instant::now();
        rig.press_stack(target).await.expect("press_stack");
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        // Long enough apart to be separate switches rather than a queue.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    // What the GUI does when a switch lands. The state hook refetches the node
    // tree on every `Chain` event and redraws from `perf`, so a switch a player
    // *sees* costs the backend switch plus these — and if they dominate, the
    // rig switched long before the screen agreed.
    let mut ui = Vec::new();
    for (label, ms) in [
        ("chain", time_call(|| rig.chain()).await),
        ("nodes", time_call(|| rig.nodes()).await),
        ("perf", time_call(|| rig.perf()).await),
        ("status", time_call(|| rig.status()).await),
        ("presets", time_call(|| rig.presets()).await),
        ("patches", time_call(|| rig.patches()).await),
    ] {
        ui.push((label, ms));
    }

    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let median = times[times.len() / 2];
    let worst = times[times.len() - 1];
    let best = times[0];

    println!();
    println!("  patch switch — {} switches", times.len());
    println!("  ──────────────────────────────");
    println!("  best     {best:7.1} ms");
    println!("  median   {median:7.1} ms");
    println!("  mean     {mean:7.1} ms");
    println!("  worst    {worst:7.1} ms");
    println!();
    // A player pressing a switch mid-bar has about this long before the next
    // beat at a fast tempo; past it, the switch is late rather than immediate.
    if median > 50.0 {
        println!("  median is past 50 ms — a switch this slow is felt.");
    }
    println!();
    println!("  what the GUI fetches after a switch");
    println!("  ──────────────────────────────");
    let mut total = 0.0;
    for (label, ms) in &ui {
        println!("  {label:9} {ms:7.1} ms");
        total += ms;
    }
    println!("  {:9} {total:7.1} ms", "sum");
    println!();
}

/// Time one RPC, discarding its value — the cost is the measurement.
async fn time_call<F, Fut, T, E>(call: F) -> f64
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let t = std::time::Instant::now();
    let _ = call().await;
    t.elapsed().as_secs_f64() * 1000.0
}
