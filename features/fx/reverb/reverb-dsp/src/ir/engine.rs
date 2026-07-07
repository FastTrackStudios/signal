//! Background IR loader thread.
//!
//! [`IrEngine`] owns a worker that decodes + transforms IRs off the GUI
//! and audio threads. Submit jobs via [`IrEngine::submit`]; poll
//! [`IrEngine::try_recv`] for ready results.
//!
//! Architectural shape matches REEV-R's "load on a separate thread,
//! swap when ready" design — but exposed as a generic channel so any
//! caller (plugin, CLI, test) can drive it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};

use realfft::RealFftPlanner;

use super::asset::{IrAsset, IrLoadError};
use super::prepared::{PreparedIr, PreparedIrPair};
use super::transforms::IrTransforms;

/// Job submitted to the loader.
#[derive(Debug, Clone)]
pub struct IrJob {
    /// Caller-defined identifier — returned in the result so requesters
    /// can correlate stale loads (only honor the newest ID).
    pub id: u64,
    pub path: PathBuf,
    pub target_sample_rate: f64,
    pub transforms: IrTransforms,
}

/// Result the loader produces.
#[derive(Debug)]
pub struct IrResult {
    pub id: u64,
    pub source_path: PathBuf,
    pub outcome: Result<ProcessedIr, IrLoadError>,
}

/// Final stereo IR — ready for [`crate::algorithms::convolution::Convolution::load_ir_stereo`].
#[derive(Debug, Clone)]
pub struct ProcessedIr {
    pub left: Vec<f64>,
    pub right: Vec<f64>,
    pub sample_rate: f64,
    /// Original asset, kept so the GUI can show duration / channel
    /// count even after the IR has been transformed.
    pub source_frames: usize,
    pub source_channels: usize,
}

pub struct IrEngine {
    tx_jobs: Sender<IrJob>,
    rx_results: Receiver<IrResult>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl IrEngine {
    /// Spawn a worker thread.
    pub fn new() -> Self {
        let (tx_jobs, rx_jobs) = unbounded::<IrJob>();
        let (tx_results, rx_results) = unbounded::<IrResult>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_w = stop.clone();

        let worker = thread::Builder::new()
            .name("reverb-ir-loader".into())
            .spawn(move || {
                while !stop_w.load(Ordering::Relaxed) {
                    match rx_jobs.recv() {
                        Ok(job) => {
                            let result = process_job(&job);
                            let _ = tx_results.send(IrResult {
                                id: job.id,
                                source_path: job.path,
                                outcome: result,
                            });
                        }
                        Err(_) => break,
                    }
                }
            })
            .expect("failed to spawn ir loader thread");

        Self {
            tx_jobs,
            rx_results,
            stop,
            worker: Some(worker),
        }
    }

    pub fn submit(&self, job: IrJob) -> Result<(), crossbeam_channel::SendError<IrJob>> {
        self.tx_jobs.send(job)
    }

    pub fn try_recv(&self) -> Result<IrResult, TryRecvError> {
        self.rx_results.try_recv()
    }

    /// Convenience for the most common case: just submit a path.
    pub fn submit_path<P: AsRef<Path>>(
        &self,
        id: u64,
        path: P,
        target_sample_rate: f64,
        transforms: IrTransforms,
    ) -> Result<(), crossbeam_channel::SendError<IrJob>> {
        self.submit(IrJob {
            id,
            path: path.as_ref().to_path_buf(),
            target_sample_rate,
            transforms,
        })
    }

    /// Spawn a relay thread that forwards successful loads onto a
    /// dedicated `Receiver<ProcessedIr>` channel — convenient for
    /// feeding [`crate::chain::ReverbChain::set_ir_swap_receiver`].
    /// Errors are dropped (typically the plugin GUI handles errors via
    /// `try_recv` on the engine's primary channel directly).
    pub fn spawn_processed_relay(&self) -> Receiver<ProcessedIr> {
        let (tx, rx) = unbounded::<ProcessedIr>();
        let src = self.rx_results.clone();
        thread::Builder::new()
            .name("reverb-ir-relay".into())
            .spawn(move || {
                while let Ok(result) = src.recv() {
                    if let Ok(ir) = result.outcome {
                        if tx.send(ir).is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("failed to spawn ir relay thread");
        rx
    }

    /// Spawn a relay thread that consumes successful loads, performs
    /// the partitioned-FFT precompute on this background thread, and
    /// forwards a [`PreparedIrPair`] downstream. Feed the returned
    /// receiver into
    /// [`crate::chain::ReverbChain::set_prepared_ir_receiver`] for
    /// zero-FFT-cost IR swaps on the audio thread.
    ///
    /// The relay shares a single [`RealFftPlanner`] across loads so
    /// FFT plan caching pays off across many IR changes.
    pub fn spawn_prepared_relay(&self) -> Receiver<PreparedIrPair> {
        let (tx, rx) = unbounded::<PreparedIrPair>();
        let src = self.rx_results.clone();
        thread::Builder::new()
            .name("reverb-ir-prepare".into())
            .spawn(move || {
                let mut planner = RealFftPlanner::<f64>::new();
                while let Ok(result) = src.recv() {
                    let Ok(ir) = result.outcome else { continue };
                    let pair = PreparedIrPair {
                        left: PreparedIr::build_with_planner(&ir.left, &mut planner),
                        right: PreparedIr::build_with_planner(&ir.right, &mut planner),
                    };
                    if tx.send(pair).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn ir prepare thread");
        rx
    }
}

impl Default for IrEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IrEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Drop the sender to wake the worker out of recv().
        // We swap in a dummy channel so the original sender is dropped.
        let (tx_dummy, _) = unbounded::<IrJob>();
        let _ = std::mem::replace(&mut self.tx_jobs, tx_dummy);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

fn process_job(job: &IrJob) -> Result<ProcessedIr, IrLoadError> {
    let asset = IrAsset::load(&job.path, job.target_sample_rate)?;
    let source_frames = asset.frames();
    let source_channels = asset.num_channels();
    let (left, right) = job.transforms.apply(&asset);
    Ok(ProcessedIr {
        left,
        right,
        sample_rate: job.target_sample_rate,
        source_frames,
        source_channels,
    })
}
