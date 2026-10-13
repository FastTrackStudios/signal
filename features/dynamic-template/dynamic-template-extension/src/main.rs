//! Dynamic Template domain SHM guest process — currently inactive.
//!
//! Historically connected to REAPER via daw-bridge SHM and registered the
//! dynamic-template actions out-of-process. The upstream `daw` crate has since
//! retired the public SHM-guest connect helper (`daw_extension_runtime::connect`
//! / `GuestOptions`) and the `ActionRegistry::subscribe_actions` /
//! `Tracks::apply_hierarchy` APIs this binary depended on. The new model is an
//! in-process REAPER cdylib extension built on `daw_extension_runtime::Extension
//! Runtime` (see `daw-reaper-dioxus-ext` for the reference shape).
//!
//! Until this crate is ported to that model, the binary just logs a notice and
//! exits so the workspace still builds.

use tracing::warn;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let pid = std::process::id();
    warn!(
        "[dynamic-template:{pid}] SHM-guest extension is disabled — upstream daw \
         API removed the connect/subscribe surface this binary relied on. Use \
         the in-process DawModule (crate `dynamic-template`, module \
         `DynamicTemplateModule`) loaded by a REAPER cdylib instead."
    );
}
