//! Level every patch and print what it measured.
//!
//!   cargo run --release -p signal-guitar --features signal-sampler/pipewire \
//!     --example level_patches
//!
//! The same pass the sidebar's "Level patches" button runs, in a terminal —
//! useful because the interesting output is a table of numbers, and because a
//! first pass renders every chain offline and takes a while.
//!
//! Runs silent by default (nothing to hear: the work is offline), but NOT
//! ephemeral — the point of the pass is to write the trims it measures. Set
//! `SIGNAL_RIG_EPHEMERAL=1` to measure without keeping them.

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

    // SAFETY: single-threaded, before the rig or any thread is created.
    unsafe {
        if std::env::var_os("SIGNAL_RIG_SILENT").is_none() {
            std::env::set_var("SIGNAL_RIG_SILENT", "1");
        }
    }

    let backend = GuitarRigBackend::new();
    eprintln!("opening rig…");
    backend.open_blocking();

    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");

    eprintln!("levelling (first pass renders every chain — this takes a while)…\n");
    rig.level_patches().await.expect("level_patches");

    // Poll rather than subscribe: this is a script, and the progress event and
    // the final state carry the same thing.
    let mut last = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let p = rig.level_progress().await.expect("level_progress");
        if p.done != last {
            last = p.done;
            eprintln!("  {}/{} {}", p.done, p.total, p.patch);
        }
        if p.complete {
            report(&p);
            break;
        }
        if p.total == 0 {
            eprintln!("no patches to level");
            break;
        }
    }
}

fn report(p: &signal_guitar::proto::LevelProgress) {
    println!();
    println!("  patch levels — {} patches", p.results.len());
    println!("  ────────────────────────────────────────────");
    println!("  {:<22} {:>8}  {:>8}", "patch", "LUFS", "trim dB");
    for r in &p.results {
        if r.lufs.is_finite() {
            println!("  {:<22} {:>8.1}  {:>+8.1}", r.patch, r.lufs, r.trim_db);
        } else {
            println!("  {:<22} {:>8}  {:>8}", r.patch, "—", "not measured");
        }
    }

    // Only measured patches have a spread; reporting one across patches that
    // did not render would read as a finding when it is a failure.
    let mut lufs: Vec<f32> = p.results.iter().map(|r| r.lufs).filter(|l| l.is_finite()).collect();
    lufs.sort_by(f32::total_cmp);
    let unmeasured = p.results.len() - lufs.len();
    println!();
    match (lufs.first(), lufs.last()) {
        (Some(lo), Some(hi)) if lufs.len() > 1 => {
            println!("  quietest {lo:.1} LUFS, loudest {hi:.1} LUFS");
            println!("  spread   {:.1} dB before trimming", hi - lo);
        }
        _ => println!("  too few patches measured to report a spread"),
    }
    if unmeasured > 0 {
        println!("  {unmeasured} of {} patches did not render", p.results.len());
    }
    println!();
}
