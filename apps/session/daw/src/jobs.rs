//! What is being done off the window's thread, and how far along: reading
//! waveforms, opening and saving the project file — anything the window
//! should not wait on but the person should be able to see.
//!
//! A piece of work holds a [`Job`] while it runs and moves it on as it
//! goes; dropping it ends it. The bottom bar shows what is running
//! ([`running`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

struct Inner {
    label: String,
    done: AtomicUsize,
    total: AtomicUsize,
}

/// One piece of work, running while held.
#[derive(Clone)]
pub struct Job(Arc<Inner>);

fn all() -> &'static Mutex<Vec<Weak<Inner>>> {
    static JOBS: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());
    &JOBS
}

/// Start a piece of work named `label`, `total` steps long (0 for one
/// that does not know).
#[must_use]
pub fn start(label: &str, total: usize) -> Job {
    let inner = Arc::new(Inner {
        label: label.to_owned(),
        done: AtomicUsize::new(0),
        total: AtomicUsize::new(total),
    });
    if let Ok(mut jobs) = all().lock() {
        jobs.retain(|j| j.strong_count() > 0);
        jobs.push(Arc::downgrade(&inner));
    }
    Job(inner)
}

impl Job {
    /// `n` more steps done.
    pub fn advance(&self, n: usize) {
        self.0.done.fetch_add(n, Ordering::Relaxed);
    }
}

/// What is running now: each job's label, steps done and steps in all.
#[must_use]
pub fn running() -> Vec<(String, usize, usize)> {
    all().lock().map_or_else(
        |_| Vec::new(),
        |jobs| {
            jobs.iter()
                .filter_map(Weak::upgrade)
                .map(|j| {
                    (
                        j.label.clone(),
                        j.done.load(Ordering::Relaxed),
                        j.total.load(Ordering::Relaxed),
                    )
                })
                .collect()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A job is listed while held, with how far it is, and gone once
    /// dropped.
    #[test]
    fn a_job_runs_while_held() {
        let job = start("Waveforms (test)", 10);
        job.advance(4);
        let mine = |jobs: Vec<(String, usize, usize)>| {
            jobs.into_iter()
                .find(|(label, ..)| label == "Waveforms (test)")
        };
        assert_eq!(
            mine(running()),
            Some(("Waveforms (test)".to_owned(), 4, 10))
        );
        drop(job);
        assert_eq!(mine(running()), None);
    }
}
