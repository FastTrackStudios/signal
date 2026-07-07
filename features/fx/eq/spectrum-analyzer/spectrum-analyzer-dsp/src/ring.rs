//! Lock-free sample handoff from the audio thread to the analyzer.
//!
//! The audio thread is the sole producer (`push`); the UI-side `tick` is the
//! sole consumer (`drain`). This is the *only* surface the realtime thread
//! touches — no FFT, no allocation, no locks. The ring is sized once to the
//! maximum FFT size so resolution changes never reallocate it.

use rtrb::{Consumer, Producer, RingBuffer};

/// Producer half — lives on the audio thread.
pub struct RingProducer {
    tx: Producer<f32>,
}

impl RingProducer {
    /// Push mono samples. Drops samples that don't fit rather than blocking —
    /// a full ring just means the UI fell behind, which is harmless for a meter.
    #[inline]
    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            // Ignore the "full" error: a stale meter is fine, a stall is not.
            let _ = self.tx.push(s);
        }
    }
}

/// Consumer half — lives on the UI/tick thread.
pub struct RingConsumer {
    rx: Consumer<f32>,
}

impl RingConsumer {
    /// Append all currently-available samples into `dst`.
    pub fn drain(&mut self, dst: &mut Vec<f32>) {
        while let Ok(s) = self.rx.pop() {
            dst.push(s);
        }
    }

    /// Number of samples ready to read.
    pub fn available(&self) -> usize {
        self.rx.slots()
    }
}

/// Create a producer/consumer pair holding up to `capacity` samples.
pub fn ring(capacity: usize) -> (RingProducer, RingConsumer) {
    let (tx, rx) = RingBuffer::<f32>::new(capacity);
    (RingProducer { tx }, RingConsumer { rx })
}
