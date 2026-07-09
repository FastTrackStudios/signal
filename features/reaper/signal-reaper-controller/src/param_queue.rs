//! Lock-free parameter write queue for SHM → audio thread communication.
//!
//! The signal-extension sends [`ParamWriteRequest`]s over SHM. A background
//! task receives them and pushes into this ring buffer. The plugin's
//! `process()` callback drains the queue and applies writes via
//! `TrackFX_SetParamNormalized`.

use crossbeam_channel::{Receiver, Sender, TrySendError};
use signal_proto::ParamWriteRequest;

/// Capacity of the parameter write ring buffer.
///
/// At 48kHz / 64 samples per block = 750 blocks/sec.
/// 4096 entries handles bursts of ~5 blocks' worth of writes.
const QUEUE_CAPACITY: usize = 4096;

/// Producer side — used by the SHM receiver task.
#[derive(Clone)]
pub struct ParamQueueProducer {
    tx: Sender<ParamWriteRequest>,
}

/// Consumer side — used by the audio thread in `process()`.
pub struct ParamQueueConsumer {
    rx: Receiver<ParamWriteRequest>,
}

/// Create a linked producer/consumer pair.
pub fn param_queue() -> (ParamQueueProducer, ParamQueueConsumer) {
    let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAPACITY);
    (ParamQueueProducer { tx }, ParamQueueConsumer { rx })
}

impl ParamQueueProducer {
    /// Enqueue a parameter write. Returns `false` if the queue is full
    /// (audio thread not draining fast enough — drop the write).
    pub fn try_send(&self, write: ParamWriteRequest) -> bool {
        match self.tx.try_send(write) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::warn!("param queue full — dropping write");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

impl ParamQueueConsumer {
    /// Drain all pending writes (non-blocking). Call from `process()`.
    pub fn drain(&self, out: &mut Vec<ParamWriteRequest>) {
        while let Ok(write) = self.rx.try_recv() {
            out.push(write);
        }
    }
}
