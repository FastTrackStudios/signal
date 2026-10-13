//! Cross-platform abstractions for native and WASM targets.
//!
//! Provides unified APIs for async primitives that differ between platforms:
//! - **RwLock**: `tokio::sync::RwLock` (its `sync` feature is wasm-clean — no
//!   reactor assumptions, no mio — so one type serves both targets)
//! - **sleep**: `tokio::time::sleep` on native, `gloo_timers` on WASM

use std::time::Duration;

/// Sleep for the given duration, compatible with both native and WASM targets.
pub async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(duration).await;
    }
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::sleep(duration).await;
    }
}

// ─── RwLock abstraction ──────────────────────────────────────────────────────

/// Async `RwLock`, native and WASM. `tokio::sync::RwLock` (the `sync` feature
/// is runtime-agnostic and wasm-clean) — `.read().await` / `.write().await`,
/// guards deref to the inner value. Replaces the retired moire.
pub use tokio::sync::RwLock;
