//! A content host on its own — the libraries and nothing else, no audio:
//!
//! ```sh
//! cargo run -p signal-content-library --example content_host -- \
//!     /Volumes/build-disk/development/libraries/Signal 0.0.0.0:4141
//! ```
//!
//! serves that library root (its id prefix ``) at `ws://<addr>/vox`, and the
//! machine's rig folders under `Signal/<rig>`, as the engine does. A device's
//! Libraries page connects to it like to a studio.

use signal_content_library::ContentLibraryBackend;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .unwrap_or_else(|| "/run/media/AudioHaven/Signal/Libraries".into());
    let addr = args.next().unwrap_or_else(|| "0.0.0.0:4141".into());
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_default()
        .join("signal");
    let mut roots = vec![(String::new(), std::path::PathBuf::from(root))];
    for (rig, dir) in [
        ("guitar", "rig"),
        ("bass", "bass-rig"),
        ("nam", "nam"),
        ("drums", "drums"),
        ("ekit", "ekit"),
    ] {
        roots.push((format!("Signal/{rig}"), config.join(dir)));
    }
    let host = ContentLibraryBackend::with_roots(roots);
    tracing::info!(%addr, "content host: ws://{addr}/vox");
    architect::host::EngineHost::new(host.router(), addr)
        .serve()
        .await;
}
