//! The Session DAW view in a browser: the web demo's bundle ([`page`]).
//!
//! A browser build only (`just web-daw`, the nix `session-app-web`). The
//! page needs session-daw's `web` feature, and a native build of this
//! crate — a workspace-wide `cargo check` — must not switch that feature
//! on for the desktop's session-daw, so on native there is nothing here.

#[cfg(target_arch = "wasm32")]
mod page;

#[cfg(target_arch = "wasm32")]
fn main() {
    page::run();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
