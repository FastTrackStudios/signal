//! The waveforms' threads: a rayon pool of their own.
//!
//! Reading peaks — every item's overview when a song opens or an edit
//! makes new items, and the fine detail a zoom asks for — runs on several
//! cores at once, off the window's thread and never on the audio's.
//!
//! Its own pool rather than rayon's global one: half the cores at most,
//! so the audio's lane pool and the window keep theirs while a kit of
//! thousands of slices is read.

/// The pool, built on first use; `None` if it would not start.
fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: std::sync::LazyLock<Option<rayon::ThreadPool>> = std::sync::LazyLock::new(|| {
        let cores = std::thread::available_parallelism().map_or(2, usize::from);
        rayon::ThreadPoolBuilder::new()
            .num_threads((cores / 2).clamp(2, 12))
            .thread_name(|n| format!("wave-{n}"))
            .build()
            .ok()
    });
    POOL.as_ref()
}

/// Run `job` on the pool, in the background.
pub fn spawn(job: impl FnOnce() + Send + 'static) {
    match pool() {
        Some(pool) => pool.spawn(job),
        None => {
            let _ = std::thread::Builder::new()
                .name("wave".to_owned())
                .spawn(job);
        }
    }
}

/// `read` over every one of `jobs` on the pool, in parallel, waiting
/// for them all; what each gave, in order.
pub fn map<T: Sync, R: Send>(jobs: &[T], read: impl Fn(&T) -> R + Sync + Send) -> Vec<R> {
    use rayon::prelude::*;
    match pool() {
        Some(pool) => pool.install(|| jobs.par_iter().map(&read).collect()),
        None => jobs.iter().map(read).collect(),
    }
}
