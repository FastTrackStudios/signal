//! Changing the model while audio is running.
//!
//! Loading a `.nam` reads a file and allocates; dropping one deallocates.
//! Neither may happen on the audio thread, and both have to happen for a user
//! to change amp without stopping playback — which is the entire point of a
//! browser inside the plugin.
//!
//! So the swap is a two-way handoff around a loader thread:
//!
//! ```text
//!   editor ──request(path)──▶ loader ──loaded──▶ audio
//!                              ▲                  │
//!                              └────retired───────┘
//! ```
//!
//! The audio thread only ever does a non-blocking `try_recv` (takes the new
//! model) and a non-blocking `try_send` (hands the old one back). It never
//! loads, never frees, and never blocks: a full or empty queue means "not
//! this block", and the plugin keeps playing what it has.
//!
//! Bounded channels, allocated once at construction — an unbounded channel
//! allocates per send, which is the thing being avoided.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use neural_amp_modeler::NamModel;

/// How many swaps may be in flight before one is dropped.
///
/// Three is generous for a human clicking models in a browser, and small
/// enough that a stall cannot pile up memory.
const DEPTH: usize = 3;

/// The audio thread's end: take newly loaded models, return retired ones.
pub struct AudioEnd {
    loaded: Receiver<NamModel>,
    retired: SyncSender<NamModel>,
    /// A model this thread has finished with but could not hand back yet.
    ///
    /// Without it there is a case with no safe answer: having taken a fresh
    /// model out of the queue, if the retire queue is full then *something*
    /// has to be dropped here. Holding the old one instead costs one
    /// `Option` move and means no `NamModel` is ever freed under the audio
    /// callback, which is the property this whole module exists for.
    stuck: Option<NamModel>,
}

/// The editor's end: ask for a model by path.
#[derive(Clone)]
pub struct EditorEnd {
    requests: SyncSender<PathBuf>,
}

/// Build the swap and start its loader thread.
///
/// `sample_rate` and `max_block` are what the loader primes each model with,
/// so a model arriving at the audio thread is already reset and can be played
/// on the very next block.
pub fn start(sample_rate: f64, max_block: usize) -> (AudioEnd, EditorEnd) {
    let (request_tx, request_rx) = sync_channel::<PathBuf>(DEPTH);
    let (loaded_tx, loaded_rx) = sync_channel::<NamModel>(DEPTH);
    let (retired_tx, retired_rx) = sync_channel::<NamModel>(DEPTH);

    std::thread::Builder::new()
        .name("nam-model-loader".into())
        .spawn(move || loader(&request_rx, &loaded_tx, &retired_rx, sample_rate, max_block))
        .ok();

    (
        AudioEnd {
            loaded: loaded_rx,
            retired: retired_tx,
            stuck: None,
        },
        EditorEnd {
            requests: request_tx,
        },
    )
}

/// Load requested models; drop retired ones. Ends when the editor end and the
/// audio end are both gone.
fn loader(
    requests: &Receiver<PathBuf>,
    loaded: &SyncSender<NamModel>,
    retired: &Receiver<NamModel>,
    sample_rate: f64,
    max_block: usize,
) {
    while let Ok(path) = requests.recv() {
        // Free anything the audio thread has handed back before allocating
        // more — the retired model is usually the one being replaced.
        while retired.try_recv().is_ok() {}

        match NamModel::load(&path) {
            Ok(mut model) => {
                // Primed here so the audio thread can play it immediately;
                // `reset` allocates and prewarms the receptive field.
                model.reset(sample_rate, max_block.max(1));
                if loaded.try_send(model).is_err() {
                    tracing::warn!(path = %path.display(), "nam: swap queue full — model dropped");
                }
            }
            Err(e) => tracing::warn!(path = %path.display(), %e, "nam: model failed to load"),
        }
    }
    // The editor is gone; free whatever audio returns before we exit.
    while retired.try_recv().is_ok() {}
}

impl AudioEnd {
    /// Take a newly loaded model, if one is waiting. Realtime-safe.
    ///
    /// The caller passes the model it is playing; if a fresh one is ready the
    /// old one goes back to the loader to be freed there. Three ordered steps,
    /// so that no `NamModel` is ever dropped on this thread:
    ///
    /// 1. Hand back anything left stuck from a previous block. While one is
    ///    stuck, take nothing new — a fresh model left in the queue is not a
    ///    problem, a freed one here is.
    /// 2. Take a fresh model, if there is one.
    /// 3. Hand the old one back, and if that queue is full, hold it in
    ///    [`Self::stuck`] for the next block.
    ///
    /// Every step is a non-blocking queue operation on a channel allocated at
    /// construction. Nothing here allocates, frees, or waits.
    pub fn take(&mut self, current: Option<NamModel>) -> Option<NamModel> {
        use std::sync::mpsc::TrySendError;

        if let Some(old) = self.stuck.take() {
            match self.retired.try_send(old) {
                Ok(()) => {}
                Err(TrySendError::Full(old) | TrySendError::Disconnected(old)) => {
                    self.stuck = Some(old);
                    return current;
                }
            }
        }

        let Ok(fresh) = self.loaded.try_recv() else {
            // `Empty` and `Disconnected` mean the same thing to this thread:
            // keep playing what you have.
            return current;
        };

        if let Some(old) = current {
            match self.retired.try_send(old) {
                Ok(()) => {}
                Err(TrySendError::Full(old) | TrySendError::Disconnected(old)) => {
                    self.stuck = Some(old);
                }
            }
        }
        Some(fresh)
    }
}

impl EditorEnd {
    /// Ask for a model. Returns false when the queue is full — the caller is
    /// clicking faster than a file can be read, and the click is dropped.
    pub fn request(&self, path: PathBuf) -> bool {
        self.requests.try_send(path).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the audio thread depends on: with nothing loaded, taking
    /// is a no-op that hands back exactly what it was given.
    #[test]
    fn taking_with_nothing_waiting_keeps_the_current_model() {
        let (mut audio, _editor) = start(48_000.0, 512);
        assert!(audio.take(None).is_none());
    }

    #[test]
    fn a_request_for_a_missing_file_does_not_kill_the_loader() {
        let (mut audio, editor) = start(48_000.0, 512);
        assert!(editor.request(PathBuf::from("/definitely/not/here.nam")));
        // The loader logs and carries on; nothing reaches the audio thread,
        // and a later request would still be served.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(audio.take(None).is_none());
        assert!(editor.request(PathBuf::from("/also/not/here.nam")));
    }

    /// A disconnected loader must not make the audio thread lose its model —
    /// this is what a plugin instance being torn down looks like from here.
    #[test]
    fn a_dead_loader_leaves_the_current_model_alone() {
        let (mut audio, editor) = start(48_000.0, 512);
        drop(editor);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(audio.take(None).is_none(), "no model, and none invented");
    }

    /// Clicking faster than the disk is a dropped click, never a block.
    #[test]
    fn a_full_request_queue_refuses_rather_than_waits() {
        let (_audio, editor) = start(48_000.0, 512);
        // The loader is single-threaded and each miss is fast, so this is not
        // a guaranteed overflow — what is guaranteed is that no call blocks.
        for _ in 0..64 {
            let _ = editor.request(PathBuf::from("/nope.nam"));
        }
    }
}
