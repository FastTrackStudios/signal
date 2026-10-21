//! A rig's own threads (loading a kit, building a space, opening audio) run
//! inside one shared Tokio runtime, so whatever they reach that spawns a
//! task — the daw engine, a PubSub pump — has a reactor to spawn on. Without
//! it a thread that is not a runtime's panics there ("no reactor running"):
//! harmless where the host happens to be inside one, fatal in an app that
//! embeds the rigs (the iPad).

use std::sync::OnceLock;

/// The runtime rig threads enter: small, shared by every rig.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("rig-rt")
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("rig runtime: {e}"))
    })
}

/// Spawn a named rig thread that runs `f` inside [`runtime`].
pub fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name(name.into()).spawn(move || {
        let _rt = runtime().enter();
        f();
    })
}
