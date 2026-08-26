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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};

use realfft::RealFftPlanner;

use super::asset::{IrAsset, IrLoadError};
use super::prepared::{PreparedIr, PreparedIrPair};
use super::transforms::IrTransforms;
use crate::algorithm::IrSlot;

/// Job submitted to the loader.
#[derive(Debug, Clone)]
pub struct IrJob {
    /// Caller-defined identifier — returned in the result so requesters
    /// can correlate stale loads (only honor the newest ID).
    pub id: u64,
    pub path: PathBuf,
    pub target_sample_rate: f64,
    pub transforms: IrTransforms,
    /// Destination IR slot (A = main, B = the morph target).
    pub slot: IrSlot,
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
    /// Destination IR slot, carried through from [`IrJob::slot`].
    pub slot: IrSlot,
    /// True-stereo cross legs (L→R, R→L) when the source was a
    /// 4-channel file or an auto-paired `_L`/`_R` stereo file set.
    pub cross: Option<(Vec<f64>, Vec<f64>)>,
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

    #[allow(clippy::result_large_err)]
    pub fn submit(&self, job: IrJob) -> Result<(), crossbeam_channel::SendError<IrJob>> {
        self.tx_jobs.send(job)
    }

    pub fn try_recv(&self) -> Result<IrResult, TryRecvError> {
        self.rx_results.try_recv()
    }

    /// Convenience for the most common case: just submit a path (slot A).
    #[allow(clippy::result_large_err)]
    pub fn submit_path<P: AsRef<Path>>(
        &self,
        id: u64,
        path: P,
        target_sample_rate: f64,
        transforms: IrTransforms,
    ) -> Result<(), crossbeam_channel::SendError<IrJob>> {
        self.submit_path_slot(id, path, target_sample_rate, transforms, IrSlot::A)
    }

    /// Submit a path load destined for a specific IR slot (B feeds the
    /// convolution morph's second convolver).
    #[allow(clippy::result_large_err)]
    pub fn submit_path_slot<P: AsRef<Path>>(
        &self,
        id: u64,
        path: P,
        target_sample_rate: f64,
        transforms: IrTransforms,
        slot: IrSlot,
    ) -> Result<(), crossbeam_channel::SendError<IrJob>> {
        self.submit(IrJob {
            id,
            path: path.as_ref().to_path_buf(),
            target_sample_rate,
            transforms,
            slot,
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
                    let cross = ir.cross.as_ref().map(|(lr, rl)| {
                        Box::new((
                            PreparedIr::build_with_planner(lr, &mut planner),
                            PreparedIr::build_with_planner(rl, &mut planner),
                        ))
                    });
                    let cross_raw = ir.cross.map(|(lr, rl)| (Arc::new(lr), Arc::new(rl)));
                    let pair = PreparedIrPair {
                        left: PreparedIr::build_with_planner(&ir.left, &mut planner),
                        right: PreparedIr::build_with_planner(&ir.right, &mut planner),
                        slot: ir.slot,
                        reshape: false,
                        // Retain the transformed-but-unshaped IR so the
                        // Impulse engine can re-shape without a disk
                        // round-trip.
                        raw: Some((Arc::new(ir.left), Arc::new(ir.right))),
                        cross,
                        cross_raw,
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

/// Job for the Impulse-param re-preparation worker. `Arc` clones of
/// the original IR keep submission allocation-free on the audio thread.
#[derive(Clone)]
pub struct ReshapeJob {
    pub slot: IrSlot,
    pub left: Arc<Vec<f64>>,
    pub right: Arc<Vec<f64>>,
    /// Full transform set to apply (the caller maps ImpulseParams onto
    /// decay_frac / tail_gate / attack_frac / stretch / reverse).
    pub transforms: IrTransforms,
    pub sample_rate: f64,
    /// True-stereo cross originals (LR, RL) to shape alongside.
    #[allow(clippy::type_complexity)]
    pub cross: Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)>,
}

/// Background worker that re-shapes an Impulse engine's IR when its
/// live shaping params change: applies [`IrTransforms`] to the stored
/// original, FFT-prepares the result, and delivers a
/// [`PreparedIrPair`] (tagged `reshape: true`) for the audio thread to
/// hot-swap via the chain's prepared-IR receiver.
///
/// Knob sweeps are debounced by draining the job queue and keeping
/// only the newest job per slot before doing any work.
pub struct ImpulseReshaper {
    tx_jobs: Sender<ReshapeJob>,
    tx_trash: Sender<crate::ir::IrTrash>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ImpulseReshaper {
    /// Spawn the worker. Feed the returned receiver into
    /// [`crate::chain::ReverbChain::set_prepared_ir_receiver`] (or
    /// merge it with the loader relay's channel upstream).
    pub fn new() -> (Self, Receiver<PreparedIrPair>) {
        let (tx_jobs, rx_jobs) = unbounded::<ReshapeJob>();
        let (tx_out, rx_out) = unbounded::<PreparedIrPair>();
        let (tx_trash, rx_trash) = unbounded::<crate::ir::IrTrash>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_w = stop.clone();

        let worker = thread::Builder::new()
            .name("reverb-ir-reshape".into())
            .spawn(move || {
                let mut planner = RealFftPlanner::<f64>::new();
                while !stop_w.load(Ordering::Relaxed) {
                    // Dispose buffers the audio thread displaced during
                    // IR swaps — dropping here keeps frees off the RT
                    // thread. Runs every wake, including timeouts.
                    while rx_trash.try_recv().is_ok() {}
                    // recv with a timeout so a stop request still wakes
                    // us when a cloned sender (handed to the audio
                    // thread via `sender()`) keeps the channel alive.
                    let first = match rx_jobs.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(job) => job,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    };
                    // Debounce: drain to the newest job per slot.
                    let mut latest_a = None;
                    let mut latest_b = None;
                    match first.slot {
                        IrSlot::A => latest_a = Some(first),
                        IrSlot::B => latest_b = Some(first),
                    }
                    while let Ok(job) = rx_jobs.try_recv() {
                        match job.slot {
                            IrSlot::A => latest_a = Some(job),
                            IrSlot::B => latest_b = Some(job),
                        }
                    }
                    for job in [latest_a, latest_b].into_iter().flatten() {
                        let asset = IrAsset::from_stereo(
                            (*job.left).clone(),
                            (*job.right).clone(),
                            job.sample_rate,
                        );
                        let (l, r) = job.transforms.apply(&asset);
                        let cross = job.cross.as_ref().map(|(lr, rl)| {
                            let (slr, srl) = job.transforms.apply_pair(
                                (**lr).clone(),
                                (**rl).clone(),
                                job.sample_rate,
                            );
                            Box::new((
                                PreparedIr::build_with_planner(&slr, &mut planner),
                                PreparedIr::build_with_planner(&srl, &mut planner),
                            ))
                        });
                        let pair = PreparedIrPair {
                            left: PreparedIr::build_with_planner(&l, &mut planner),
                            right: PreparedIr::build_with_planner(&r, &mut planner),
                            slot: job.slot,
                            reshape: true,
                            raw: None,
                            cross,
                            cross_raw: None,
                        };
                        if tx_out.send(pair).is_err() {
                            return;
                        }
                    }
                }
            })
            .expect("failed to spawn ir reshape thread");

        (
            Self {
                tx_jobs,
                tx_trash,
                stop,
                worker: Some(worker),
            },
            rx_out,
        )
    }

    /// Submit a re-shape. RT-safe: the job is `Arc` clones + a `Copy`
    /// transform set; the channel is lock-free.
    #[allow(clippy::result_large_err)]
    pub fn submit(&self, job: ReshapeJob) -> Result<(), crossbeam_channel::SendError<ReshapeJob>> {
        self.tx_jobs.send(job)
    }

    /// A cloneable submission handle for the audio-thread side.
    pub fn sender(&self) -> Sender<ReshapeJob> {
        self.tx_jobs.clone()
    }

    /// Disposal handle for buffers displaced by audio-thread IR swaps —
    /// feed into [`crate::chain::ReverbChain::set_ir_trash_sender`].
    /// The worker drops whatever arrives.
    pub fn trash_sender(&self) -> Sender<crate::ir::IrTrash> {
        self.tx_trash.clone()
    }
}

impl Drop for ImpulseReshaper {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let (tx_dummy, _) = unbounded::<ReshapeJob>();
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

    // True-stereo detection: a 4-channel file is [LL, LR, RL, RR]; a
    // stereo file named `*_L` with a `*_R` sibling (or vice versa) is
    // the two-file convention (L file = source-L → LL/LR).
    let quad: Option<[Vec<f64>; 4]> = if asset.num_channels() >= 4 {
        Some([
            asset.channels[0].clone(),
            asset.channels[1].clone(),
            asset.channels[2].clone(),
            asset.channels[3].clone(),
        ])
    } else if asset.num_channels() == 2 {
        true_stereo_sibling(&job.path).and_then(|(l_path, r_path)| {
            let l = if l_path == job.path {
                asset.clone()
            } else {
                IrAsset::load(&l_path, job.target_sample_rate).ok()?
            };
            let r = if r_path == job.path {
                asset.clone()
            } else {
                IrAsset::load(&r_path, job.target_sample_rate).ok()?
            };
            (l.num_channels() >= 2 && r.num_channels() >= 2).then(|| {
                [
                    l.channels[0].clone(),
                    l.channels[1].clone(),
                    r.channels[0].clone(),
                    r.channels[1].clone(),
                ]
            })
        })
    } else {
        None
    };

    if let Some([ll, lr, rl, rr]) = quad {
        let (left, right) = job.transforms.apply_pair(ll, rr, job.target_sample_rate);
        let cross = job.transforms.apply_pair(lr, rl, job.target_sample_rate);
        return Ok(ProcessedIr {
            left,
            right,
            sample_rate: job.target_sample_rate,
            source_frames,
            source_channels: 4,
            slot: job.slot,
            cross: Some(cross),
        });
    }

    let (left, right) = job.transforms.apply(&asset);
    Ok(ProcessedIr {
        left,
        right,
        sample_rate: job.target_sample_rate,
        source_frames,
        source_channels,
        slot: job.slot,
        cross: None,
    })
}

/// Map a path with an `_L`/`-L`/` L` (or `R`) stem suffix to its
/// true-stereo (L-file, R-file) pair, if the sibling exists on disk.
fn true_stereo_sibling(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let stem = path.file_stem()?.to_str()?;
    let ext = path.extension()?.to_str()?;
    for (suffix_l, suffix_r) in [("_L", "_R"), ("-L", "-R"), (" L", " R")] {
        let (this, other, this_is_l) = if stem.ends_with(suffix_l) {
            (suffix_l, suffix_r, true)
        } else if stem.ends_with(suffix_r) {
            (suffix_r, suffix_l, false)
        } else {
            continue;
        };
        let base = &stem[..stem.len() - this.len()];
        let sibling = path.with_file_name(format!("{base}{other}.{ext}"));
        if sibling.is_file() {
            return Some(if this_is_l {
                (path.to_path_buf(), sibling)
            } else {
                (sibling, path.to_path_buf())
            });
        }
    }
    None
}
