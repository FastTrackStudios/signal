//! Backend-agnostic main-thread affinity.
//!
//! Some DAW backends require certain operations to run with a specific thread
//! affinity. REAPER is the canonical example: its FFI (`EnumProjects`, track
//! enumeration, etc.) may only be called from REAPER's single main thread, and
//! calling it from a vox worker thread deadlocks on REAPER's internal lock.
//! Other backends — `daw-standalone`, tests, future engines — have no such
//! requirement and can run the work inline on any thread.
//!
//! This module defines the abstraction. Each backend installs its own
//! [`MainThreadExecutor`] (REAPER installs a `TaskSupport`-backed one;
//! standalone installs none). Service code calls [`query`] / [`run`] and stays
//! backend-agnostic:
//!
//! ```rust,ignore
//! let projects = daw_proto::main_thread::query(move || daw.list()).await;
//! ```
//!
//! With an executor installed, the closure runs under the required affinity and
//! the result is awaited back. With none installed, the closure runs inline —
//! a no-op bounce — so the exact same service code runs against standalone.

use std::sync::OnceLock;

/// A backend-provided executor that runs closures under whatever thread
/// affinity the backend requires. Installed once per process via
/// [`set_executor`].
pub trait MainThreadExecutor: Send + Sync + 'static {
    /// Schedule a closure to run under the backend's required affinity,
    /// fire-and-forget. The closure runs as soon as the backend can service it.
    fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>);
}

static EXECUTOR: OnceLock<Box<dyn MainThreadExecutor>> = OnceLock::new();

/// Install the backend's main-thread executor. The first install wins;
/// subsequent calls are ignored. Backends without a main-thread requirement
/// need not call this — [`query`] / [`run`] then run inline.
pub fn set_executor<E: MainThreadExecutor>(executor: E) {
    let _ = EXECUTOR.set(Box::new(executor));
}

/// Whether a backend executor is installed (i.e. the active backend has a
/// main-thread requirement).
pub fn is_installed() -> bool {
    EXECUTOR.get().is_some()
}

#[cfg(not(target_arch = "wasm32"))]
fn executor() -> Option<&'static dyn MainThreadExecutor> {
    EXECUTOR.get().map(|b| b.as_ref())
}

/// Run `f` under the backend's required affinity and return its value.
///
/// With an executor installed (e.g. REAPER), `f` runs on the main thread and
/// the result is awaited back over a oneshot. With none installed
/// (standalone/tests), `f` runs inline on the calling thread. Returns `None`
/// only if an installed executor dropped the task without running it.
#[cfg(not(target_arch = "wasm32"))]
pub async fn query<F, R>(f: F) -> Option<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    match executor() {
        Some(exec) => {
            let (tx, rx) = futures::channel::oneshot::channel();
            exec.spawn(Box::new(move || {
                let _ = tx.send(f());
            }));
            rx.await.ok()
        }
        None => Some(f()),
    }
}

/// Wasm build of [`query`]: the browser runtime is single-threaded and no
/// backend installs a [`MainThreadExecutor`] there (only REAPER does, and
/// that's native-only), so `f` always runs inline. The `Send` bounds the
/// native cross-thread bounce needs are dropped — the closure captures the
/// browser's `!Send` in-process backend.
#[cfg(target_arch = "wasm32")]
pub async fn query<F, R>(f: F) -> Option<R>
where
    F: FnOnce() -> R + 'static,
    R: 'static,
{
    Some(f())
}

/// Run `f` under the backend's required affinity, fire-and-forget.
///
/// With an executor installed, `f` is scheduled under the required affinity.
/// With none installed, `f` runs inline immediately.
#[cfg(not(target_arch = "wasm32"))]
pub fn run<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    match executor() {
        Some(exec) => exec.spawn(Box::new(f)),
        None => f(),
    }
}

/// Wasm build of [`run`]: always inline (see [`query`]); no `Send` bound.
#[cfg(target_arch = "wasm32")]
pub fn run<F>(f: F)
where
    F: FnOnce() + 'static,
{
    f()
}
