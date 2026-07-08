//! Headless smoke test for the detached rig core, end to end over a real vox
//! link: serve the backend on an in-process `LocalServer`, control it through
//! `RigClient`, and watch `RigEvent`s arrive on the `#[subscribe]` stream —
//! exactly what a desktop/browser GUI does, minus the pixels.
//!
//!   cargo run -p signal-guitar --features signal-sampler/pipewire --example headless_rig

use architect::{LocalServer, Scope};
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::{RigClient, RigEvent, RigStreamClient};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let backend = GuitarRigBackend::new();
    eprintln!("opening rig (blocking)…");
    backend.open_blocking();

    // Serve + connect the same way the desktop app does.
    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");
    let stream: RigStreamClient = server.establish().await.expect("stream client");

    let status = rig.status().await.expect("status");
    eprintln!("status: {status:?}");
    let perf = rig.perf().await.expect("perf");
    eprintln!(
        "profile: {:?}, stacks: {:?}",
        perf.profile_name,
        perf.stacks.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
    );
    eprintln!("chain: {} blocks", rig.chain().await.expect("chain").len());

    // Subscribe, then mutate — the stream must carry meters + the change.
    // vox scopes channels to their request: the subscribe call stays in
    // flight for the life of the subscription, so spawn it (aborting the
    // task is the unsubscribe — same race `architect::use_stream` runs).
    let (tx, mut rx) = vox::channel::<RigEvent>();
    let subscription = tokio::spawn(async move { stream.events(tx).await });

    // The attach confirms itself by the first meter event (the pump runs at
    // ~30 Hz) — only mutate after it, or the perf/chain publishes land
    // before the sink is attached (the hub has no replay).
    let first = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("attach timed out")
        .expect("recv")
        .expect("stream open");
    let mut n_status = 0u32;
    let _ = first.map(|ev| {
        assert!(matches!(ev, RigEvent::Status(_)), "expected meter event first");
        n_status += 1;
    });

    rig.press_stack(1).await.expect("press stack");

    let (mut n_perf, mut n_chain) = (0u32, 0u32);
    while n_perf == 0 || n_chain == 0 {
        let recv = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        let Ok(Ok(Some(event))) = recv else {
            eprintln!("stream ended / timed out");
            break;
        };
        let _ = event.map(|ev| match ev {
            RigEvent::Status(_) => n_status += 1,
            RigEvent::Perf(p) => {
                n_perf += 1;
                let active: Vec<_> = p.stacks.iter().filter(|s| s.is_active).map(|s| s.name.clone()).collect();
                eprintln!("perf event: active stack(s) {active:?}");
            }
            RigEvent::Chain(c) => {
                n_chain += 1;
                eprintln!("chain event: {} blocks", c.len());
            }
        });
    }
    eprintln!("events received: {n_status} status, {n_perf} perf, {n_chain} chain");
    assert!(n_status > 0, "no meter events");
    assert!(n_perf > 0, "no perf events");
    assert!(n_chain > 0, "no chain events");

    subscription.abort(); // unsubscribe
    rig.stop().await.expect("stop");
    eprintln!("OK — detached control + event stream verified");
}
