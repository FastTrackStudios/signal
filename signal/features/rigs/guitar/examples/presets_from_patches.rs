//! Make a preset from each patch of the playing profile that has none — the
//! same as Presets mode's "Make presets from this profile's patches", for a
//! config with no window open on it (packaging a release, a fresh machine).
//! The patches are left as they are.
//!
//! ```sh
//! cargo run --release -p signal-guitar --example presets_from_patches
//! ```

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
    backend.open_blocking();
    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");
    rig.presets_from_patches().await.expect("presets_from_patches");
    let comp = rig.compositions().await.expect("compositions");
    let made = comp.modules.iter().filter(|m| m.module == "Preset").count();
    tracing::info!(presets = made, "presets in the library");
}
